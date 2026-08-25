//! Transactional remote acquisition and integration.
//!
//! Pull deliberately composes Git primitives (`fetch`, graph inspection,
//! `merge`, and `rebase`) instead of invoking `git pull`. Fetching and
//! integration are separate stages and every mutating integration receives a
//! recovery ref and a persistent operation journal.

use crate::config::loader::{IntegrationsConfig, PullConfig};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;
use uuid::Uuid;

pub const JSON_SCHEMA: &str = "kaptaind.pull.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStrategy {
    Auto,
    FastForward,
    Merge,
    Rebase,
    Hybreed,
    Emulsify,
    Manual,
}

impl std::str::FromStr for IntegrationStrategy {
    type Err = PullError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "fast-forward" | "fast_forward" | "ff" => Ok(Self::FastForward),
            "merge" => Ok(Self::Merge),
            "rebase" => Ok(Self::Rebase),
            "hybreed" => Ok(Self::Hybreed),
            "emulsify" => Ok(Self::Emulsify),
            "manual" => Ok(Self::Manual),
            _ => Err(PullError::invalid(format!(
                "unknown pull strategy `{value}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    UpToDate,
    RemoteAhead,
    LocalAhead,
    Diverged,
    UnrelatedHistories,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionState {
    Created,
    Preflight,
    Fetched,
    Analysed,
    Planned,
    Executing,
    Verifying,
    Committed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone)]
pub struct PullOptions {
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub strategy: IntegrationStrategy,
    pub check: bool,
    pub dry_run: bool,
    pub force: bool,
    pub autostash: bool,
    pub verbose: bool,
    /// Emit the human assessment before any integration mutation.
    pub emit_assessment: bool,
}

impl Default for PullOptions {
    fn default() -> Self {
        Self {
            remote: None,
            branch: None,
            strategy: IntegrationStrategy::Auto,
            check: false,
            dry_run: false,
            force: false,
            autostash: false,
            verbose: false,
            emit_assessment: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefUpdate {
    pub reference: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FetchResult {
    pub updated_refs: Vec<RefUpdate>,
    pub new_refs: Vec<RefUpdate>,
    pub deleted_refs: Vec<RefUpdate>,
    pub rejected_refs: Vec<RefUpdate>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteResolution {
    pub remote: String,
    pub remote_url: String,
    pub remote_head: Option<String>,
    pub local_branch: String,
    pub local_ref: String,
    pub remote_branch: String,
    pub remote_ref: String,
    pub upstream: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullAnalysis {
    pub local_head: String,
    pub remote_head: String,
    pub merge_base: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub topology: Topology,
    pub changed_local: usize,
    pub changed_remote: usize,
    pub overlapping_paths: Vec<PathBuf>,
    pub predicted_conflicts: usize,
    pub risk_score: f32,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDecision {
    pub requested: IntegrationStrategy,
    pub selected: IntegrationStrategy,
    pub confidence: f32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Verification {
    pub repository: bool,
    pub index: bool,
    pub conflicts: bool,
    pub worktree: bool,
    pub head: bool,
    pub build: Option<bool>,
    pub tests: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictSummary {
    pub total: usize,
    pub resolved: usize,
    pub manual: usize,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    Content,
    RenameRename,
    RenameDelete,
    DeleteModify,
    DirectoryFile,
    Submodule,
    Binary,
    Symlink,
    Mode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Unresolved,
    Proposed,
    ReviewRequired,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub path: PathBuf,
    pub kind: ConflictKind,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub base: Option<String>,
    pub resolution_status: ResolutionStatus,
    /// A proposal confidence, never a certainty. `None` means no proposal was made.
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullReport {
    pub schema: String,
    pub operation: String,
    pub status: String,
    pub transaction_id: String,
    pub repository: PathBuf,
    pub remote: String,
    pub remote_url: String,
    pub local_ref: String,
    pub remote_ref: String,
    pub before: String,
    pub remote_head: String,
    pub after: Option<String>,
    pub topology: Topology,
    pub strategy: IntegrationStrategy,
    pub strategy_confidence: f32,
    pub strategy_reasons: Vec<String>,
    pub ahead: usize,
    pub behind: usize,
    pub risk: RiskLevel,
    pub risk_score: f32,
    pub conflicts: ConflictSummary,
    pub verification: Verification,
    pub fetch: FetchResult,
    pub dry_run: bool,
    pub journal: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PullTransaction {
    schema: String,
    transaction_id: String,
    repository: PathBuf,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    state: TransactionState,
    remote: Option<RemoteResolution>,
    initial_head: String,
    initial_index_tree: Option<String>,
    initial_worktree_dirty: bool,
    initial_stash: Option<String>,
    fetch_result: Option<FetchResult>,
    analysis: Option<PullAnalysis>,
    strategy: Option<StrategyDecision>,
    final_head: Option<String>,
    recovery_ref: Option<String>,
    autostash_oid: Option<String>,
    result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Generic = 1,
    InvalidInvocation = 2,
    UnsafeRepository = 3,
    RemoteUnavailable = 4,
    Authentication = 5,
    Conflicts = 6,
    Verification = 7,
    Rollback = 8,
    InProgress = 9,
    PolicyDenied = 10,
}

#[derive(Debug)]
pub struct PullError {
    pub code: ExitCode,
    pub message: String,
}

impl PullError {
    fn new(code: ExitCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(ExitCode::InvalidInvocation, message)
    }

    pub fn exit_code(&self) -> i32 {
        self.code as i32
    }
}

impl fmt::Display for PullError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PullError {}

pub fn run(
    repo: &Path,
    options: &PullOptions,
    config: &PullConfig,
    integrations: &IntegrationsConfig,
) -> Result<PullReport, PullError> {
    let repo = discover_root(repo)?;
    let transaction_id = Uuid::new_v4().to_string();
    let initial_head = git_text(&repo, &["rev-parse", "HEAD"])?;
    let dirty = !git_bytes(
        &repo,
        &[
            "status",
            "--porcelain",
            "-z",
            "--",
            ".",
            ":(exclude).kaptaind",
        ],
    )?
    .is_empty();
    let initial_index_tree = git_text(&repo, &["write-tree"]).ok();
    let journal = repo
        .join(".kaptaind")
        .join("transactions")
        .join(&transaction_id);
    fs::create_dir_all(&journal).map_err(io_error)?;
    let _lock = OperationLock::acquire(&repo, &transaction_id)?;
    let mut transaction = PullTransaction {
        schema: JSON_SCHEMA.to_owned(),
        transaction_id: transaction_id.clone(),
        repository: repo.clone(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        state: TransactionState::Created,
        remote: None,
        initial_head: initial_head.clone(),
        initial_index_tree,
        initial_worktree_dirty: dirty,
        initial_stash: git_text(&repo, &["rev-parse", "--verify", "refs/stash"]).ok(),
        fetch_result: None,
        analysis: None,
        strategy: None,
        final_head: None,
        recovery_ref: None,
        autostash_oid: None,
        result: None,
    };
    persist(&journal, &transaction)?;
    event(&journal, &transaction, "pull.started", "ok", None)?;

    transition(&journal, &mut transaction, TransactionState::Preflight)?;
    preflight(&repo)?;
    let resolution = resolve_remote(&repo, options, config)?;
    transaction.remote = Some(resolution.clone());
    persist(&journal, &transaction)?;
    event(&journal, &transaction, "pull.preflight", "ok", None)?;

    event(
        &journal,
        &transaction,
        "pull.fetch.started",
        "started",
        None,
    )?;
    let fetch = fetch(&repo, &resolution, config)?;
    transaction.fetch_result = Some(fetch.clone());
    transition(&journal, &mut transaction, TransactionState::Fetched)?;
    event(
        &journal,
        &transaction,
        "pull.fetch.completed",
        "ok",
        Some(fetch.duration_ms),
    )?;

    let analysis = analyse(&repo, &resolution, config)?;
    transaction.analysis = Some(analysis.clone());
    transition(&journal, &mut transaction, TransactionState::Analysed)?;
    event(
        &journal,
        &transaction,
        "pull.analysis.completed",
        "ok",
        None,
    )?;
    let decision = select_strategy(&analysis, &resolution, options, config)?;
    transaction.strategy = Some(decision.clone());
    transition(&journal, &mut transaction, TransactionState::Planned)?;
    fs::write(
        journal.join("plan.json"),
        serde_json::to_vec_pretty(&decision).map_err(json_error)?,
    )
    .map_err(io_error)?;
    event(&journal, &transaction, "pull.strategy.selected", "ok", None)?;

    let mut report = base_report(ReportInput {
        repo: &repo,
        journal: &journal,
        transaction: &transaction,
        resolution: &resolution,
        analysis: &analysis,
        decision: &decision,
        fetch: &fetch,
        dry_run: options.dry_run,
    });
    if options.check
        || options.dry_run
        || matches!(analysis.topology, Topology::UpToDate | Topology::LocalAhead)
    {
        report.status = match analysis.topology {
            Topology::UpToDate => "up_to_date",
            Topology::LocalAhead => "local_ahead",
            _ if options.dry_run => "dry_run",
            _ => "checked",
        }
        .to_owned();
        transaction.result = Some(report.status.clone());
        transition(&journal, &mut transaction, TransactionState::Committed)?;
        write_result(&journal, &report)?;
        return Ok(report);
    }

    if options.emit_assessment {
        print!("{}", render_assessment(&report));
        let _ = std::io::stdout().flush();
    }

    if dirty {
        if options.autostash || config.autostash {
            transaction.autostash_oid = create_autostash(&repo, &transaction_id)?;
            persist(&journal, &transaction)?;
        } else {
            fail_transaction(&journal, &mut transaction, "working tree is dirty")?;
            return Err(PullError::new(
                ExitCode::UnsafeRepository,
                "working tree contains uncommitted changes; commit them or use --autostash",
            ));
        }
    }

    if analysis.risk == RiskLevel::Critical && !options.force {
        fail_transaction(&journal, &mut transaction, "critical risk requires --force")?;
        return Err(PullError::new(
            ExitCode::PolicyDenied,
            "pull risk is CRITICAL; explicit --force approval is required",
        ));
    }

    if matches!(decision.selected, IntegrationStrategy::Manual) {
        fail_transaction(&journal, &mut transaction, "manual integration required")?;
        return Err(PullError::new(
            ExitCode::PolicyDenied,
            "repository topology requires an explicit integration strategy",
        ));
    }

    let recovery_ref = format!("refs/kaptaind/recovery/{transaction_id}");
    git_ok(&repo, &["update-ref", &recovery_ref, &initial_head])?;
    transaction.recovery_ref = Some(recovery_ref);
    transition(&journal, &mut transaction, TransactionState::Executing)?;
    event(
        &journal,
        &transaction,
        "pull.integration.started",
        "started",
        None,
    )?;

    if matches!(
        decision.selected,
        IntegrationStrategy::Hybreed | IntegrationStrategy::Emulsify
    ) {
        let advisory = match crate::integration::analyse(
            &repo,
            &resolution.local_ref,
            &resolution.remote_ref,
            integrations,
            false,
        ) {
            Ok(advisory) => advisory,
            Err(error) => {
                fail_transaction(&journal, &mut transaction, &error.to_string())?;
                return Err(PullError::new(ExitCode::PolicyDenied, error.to_string()));
            }
        };
        fs::write(
            journal.join("reconciliation.json"),
            serde_json::to_vec_pretty(&advisory).map_err(json_error)?,
        )
        .map_err(io_error)?;
    }

    let integration_result = integrate(&repo, &resolution, decision.selected);
    if let Err(error) = integration_result {
        let conflict_records = conflicts(&repo).unwrap_or_default();
        let conflicts: Vec<_> = conflict_records
            .iter()
            .map(|conflict| conflict.path.clone())
            .collect();
        fs::write(
            journal.join("conflicts.json"),
            serde_json::to_vec_pretty(&conflict_records).map_err(json_error)?,
        )
        .map_err(io_error)?;
        fail_transaction(&journal, &mut transaction, &error.message)?;
        if !conflicts.is_empty() {
            event(
                &journal,
                &transaction,
                "pull.conflict.detected",
                "manual",
                None,
            )?;
            return Err(PullError::new(
                ExitCode::Conflicts,
                format!(
                    "integration stopped with {} conflict(s); resolve them, then use `kaptaind-cli pull --continue`, or use `--abort`",
                    conflicts.len()
                ),
            ));
        }
        return Err(error);
    }

    transition(&journal, &mut transaction, TransactionState::Verifying)?;
    event(
        &journal,
        &transaction,
        "pull.verification.started",
        "started",
        None,
    )?;
    let verification = verify(&repo, &resolution, config)?;
    report.verification = verification.clone();
    if !verification.repository
        || !verification.index
        || !verification.conflicts
        || !verification.head
        || verification.build == Some(false)
        || verification.tests == Some(false)
    {
        fail_transaction(&journal, &mut transaction, "verification failed")?;
        if rollback_to_recovery(&repo, &transaction).is_err() {
            return Err(PullError::new(
                ExitCode::Rollback,
                format!(
                    "verification failed and automatic rollback failed; recovery ref: {}",
                    transaction.recovery_ref.as_deref().unwrap_or("unavailable")
                ),
            ));
        }
        restore_autostash(&repo, &mut transaction)?;
        return Err(PullError::new(
            ExitCode::Verification,
            "post-pull verification failed; the original HEAD was restored",
        ));
    }

    restore_autostash(&repo, &mut transaction)?;
    let final_head = git_text(&repo, &["rev-parse", "HEAD"])?;
    transaction.final_head = Some(final_head.clone());
    transaction.result = Some("success".to_owned());
    transition(&journal, &mut transaction, TransactionState::Committed)?;
    report.status = "success".to_owned();
    report.after = Some(final_head);
    report.verification = verification;
    write_result(&journal, &report)?;
    event(&journal, &transaction, "pull.completed", "success", None)?;
    Ok(report)
}

pub fn status(repo: &Path) -> Result<Option<serde_json::Value>, PullError> {
    let repo = discover_root(repo)?;
    let Some(path) = latest_transaction(&repo)? else {
        return Ok(None);
    };
    let bytes = fs::read(path.join("metadata.json")).map_err(io_error)?;
    serde_json::from_slice(&bytes).map(Some).map_err(json_error)
}

pub fn continue_operation(repo: &Path, config: &PullConfig) -> Result<PullReport, PullError> {
    let repo = discover_root(repo)?;
    let journal = latest_transaction(&repo)?.ok_or_else(|| {
        PullError::new(
            ExitCode::UnsafeRepository,
            "no pull transaction is available",
        )
    })?;
    let mut transaction = read_transaction(&journal)?;
    let resolution = transaction.remote.clone().ok_or_else(|| {
        PullError::new(
            ExitCode::UnsafeRepository,
            "transaction has no resolved remote",
        )
    })?;
    let analysis = transaction
        .analysis
        .clone()
        .ok_or_else(|| PullError::new(ExitCode::UnsafeRepository, "transaction has no analysis"))?;
    let decision = transaction
        .strategy
        .clone()
        .ok_or_else(|| PullError::new(ExitCode::UnsafeRepository, "transaction has no strategy"))?;
    let fetch = transaction.fetch_result.clone().unwrap_or_default();
    let _lock = OperationLock::acquire(&repo, &transaction.transaction_id)?;
    if !conflict_paths(&repo)?.is_empty() {
        return Err(PullError::new(
            ExitCode::Conflicts,
            "unmerged index entries remain; resolve and stage every conflict before --continue",
        ));
    }
    if git_path_exists(&repo, "MERGE_HEAD") {
        git_ok(&repo, &["commit", "--no-edit"])?;
    } else if git_path_exists(&repo, "rebase-merge") || git_path_exists(&repo, "rebase-apply") {
        let output = git_command(&repo)
            .env("GIT_EDITOR", "true")
            .args(["rebase", "--continue"])
            .output()
            .map_err(io_error)?;
        classify_output("rebase --continue", output)?;
    } else {
        return Err(PullError::new(
            ExitCode::UnsafeRepository,
            "no merge or rebase is in progress",
        ));
    }
    transition(&journal, &mut transaction, TransactionState::Verifying)?;
    let verification = verify(&repo, &resolution, config)?;
    if !verification.repository || !verification.index || !verification.conflicts {
        return Err(PullError::new(
            ExitCode::Verification,
            "verification failed",
        ));
    }
    restore_autostash(&repo, &mut transaction)?;
    let final_head = git_text(&repo, &["rev-parse", "HEAD"])?;
    transaction.final_head = Some(final_head.clone());
    transaction.result = Some("success".to_owned());
    transition(&journal, &mut transaction, TransactionState::Committed)?;
    let mut report = base_report(ReportInput {
        repo: &repo,
        journal: &journal,
        transaction: &transaction,
        resolution: &resolution,
        analysis: &analysis,
        decision: &decision,
        fetch: &fetch,
        dry_run: false,
    });
    report.status = "success".to_owned();
    report.after = Some(final_head);
    report.verification = verification;
    write_result(&journal, &report)?;
    Ok(report)
}

pub fn abort(repo: &Path) -> Result<(), PullError> {
    let repo = discover_root(repo)?;
    let journal = latest_transaction(&repo)?.ok_or_else(|| {
        PullError::new(
            ExitCode::UnsafeRepository,
            "no pull transaction is available",
        )
    })?;
    let mut transaction = read_transaction(&journal)?;
    let _lock = OperationLock::acquire(&repo, &transaction.transaction_id)?;
    if git_path_exists(&repo, "MERGE_HEAD") {
        git_ok(&repo, &["merge", "--abort"])?;
    } else if git_path_exists(&repo, "rebase-merge") || git_path_exists(&repo, "rebase-apply") {
        git_ok(&repo, &["rebase", "--abort"])?;
    }
    rollback_to_recovery(&repo, &transaction)?;
    restore_autostash(&repo, &mut transaction)?;
    transaction.result = Some("aborted".to_owned());
    transition(&journal, &mut transaction, TransactionState::Aborted)?;
    event(
        &journal,
        &transaction,
        "pull.rollback.completed",
        "aborted",
        None,
    )?;
    Ok(())
}

pub fn recover(repo: &Path) -> Result<(), PullError> {
    clear_stale_lock(repo)?;
    abort(repo)
}

fn preflight(repo: &Path) -> Result<(), PullError> {
    for (path, operation) in [
        ("MERGE_HEAD", "merge"),
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("BISECT_LOG", "bisect"),
    ] {
        if git_path_exists(repo, path) {
            return Err(PullError::new(
                ExitCode::UnsafeRepository,
                format!("an existing {operation} operation is in progress; use pull --continue or --abort where applicable"),
            ));
        }
    }
    if git_path(repo, "index.lock").is_some_and(|path| path.exists()) {
        return Err(PullError::new(
            ExitCode::UnsafeRepository,
            "repository index lock exists; verify no Git process is active before recovery",
        ));
    }
    Ok(())
}

fn resolve_remote(
    repo: &Path,
    options: &PullOptions,
    config: &PullConfig,
) -> Result<RemoteResolution, PullError> {
    let local_branch =
        git_text(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).map_err(|_| {
            PullError::new(
                ExitCode::UnsafeRepository,
                "detached HEAD cannot be pulled without first checking out a branch",
            )
        })?;
    validate_ref_component(&local_branch)?;
    let upstream_remote = git_text(
        repo,
        &["config", "--get", &format!("branch.{local_branch}.remote")],
    )
    .ok();
    let upstream_merge = git_text(
        repo,
        &["config", "--get", &format!("branch.{local_branch}.merge")],
    )
    .ok();
    let remote = options
        .remote
        .clone()
        .or_else(|| config.remote.clone())
        .or(upstream_remote)
        .ok_or_else(|| {
            PullError::invalid(
                "pull target cannot be determined: no remote is configured for the current branch",
            )
        })?;
    validate_ref_component(&remote)?;
    let remote_branch = options
        .branch
        .clone()
        .or_else(|| config.branch.clone())
        .or_else(|| {
            upstream_merge
                .as_deref()
                .and_then(|value| value.strip_prefix("refs/heads/"))
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            PullError::invalid("pull target cannot be determined: no upstream branch is configured")
        })?;
    validate_branch(&remote_branch)?;
    let url = git_text(repo, &["remote", "get-url", &remote]).map_err(|_| {
        PullError::new(
            ExitCode::RemoteUnavailable,
            format!("remote `{remote}` does not exist"),
        )
    })?;
    let remote_head = git_text(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            &format!("refs/remotes/{remote}/HEAD"),
        ],
    )
    .ok();
    Ok(RemoteResolution {
        remote: remote.clone(),
        remote_url: redact_url(&url),
        remote_head,
        local_branch: local_branch.clone(),
        local_ref: format!("refs/heads/{local_branch}"),
        remote_branch: remote_branch.clone(),
        remote_ref: format!("refs/remotes/{remote}/{remote_branch}"),
        upstream: git_text(repo, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok(),
    })
}

fn fetch(
    repo: &Path,
    resolution: &RemoteResolution,
    config: &PullConfig,
) -> Result<FetchResult, PullError> {
    let before = git_text(repo, &["rev-parse", "--verify", &resolution.remote_ref]).ok();
    let started = Instant::now();
    let refspec = format!(
        "+refs/heads/{}:{}",
        resolution.remote_branch, resolution.remote_ref
    );
    let mut command = git_command(repo);
    command.arg("fetch");
    if config.prune {
        command.arg("--prune");
    }
    match config.tags.as_str() {
        "all" => {
            command.arg("--tags");
        }
        "none" => {
            command.arg("--no-tags");
        }
        _ => {}
    }
    let output = command
        .arg(&resolution.remote)
        .arg(&refspec)
        .output()
        .map_err(io_error)?;
    classify_output("fetch", output)?;
    let after = git_text(repo, &["rev-parse", "--verify", &resolution.remote_ref]).ok();
    let update = RefUpdate {
        reference: resolution.remote_ref.clone(),
        before: before.clone(),
        after: after.clone(),
    };
    let mut result = FetchResult {
        duration_ms: started.elapsed().as_millis(),
        ..FetchResult::default()
    };
    match (before, after) {
        (None, Some(_)) => result.new_refs.push(update),
        (Some(_), None) => result.deleted_refs.push(update),
        (Some(a), Some(b)) if a != b => result.updated_refs.push(update),
        _ => {}
    }
    if result.deleted_refs.len() == 1 {
        return Err(PullError::new(
            ExitCode::RemoteUnavailable,
            format!("remote branch `{}` was deleted", resolution.remote_branch),
        ));
    }
    Ok(result)
}

fn analyse(
    repo: &Path,
    resolution: &RemoteResolution,
    config: &PullConfig,
) -> Result<PullAnalysis, PullError> {
    let local_head = git_text(repo, &["rev-parse", &resolution.local_ref])?;
    let remote_head = git_text(repo, &["rev-parse", &resolution.remote_ref]).map_err(|_| {
        PullError::new(
            ExitCode::RemoteUnavailable,
            format!("remote ref `{}` was not found", resolution.remote_ref),
        )
    })?;
    if local_head == remote_head {
        return Ok(PullAnalysis {
            local_head,
            remote_head,
            merge_base: None,
            ahead: 0,
            behind: 0,
            topology: Topology::UpToDate,
            changed_local: 0,
            changed_remote: 0,
            overlapping_paths: Vec::new(),
            predicted_conflicts: 0,
            risk_score: 0.0,
            risk: RiskLevel::Low,
        });
    }
    let merge_base = git_text(
        repo,
        &["merge-base", &resolution.local_ref, &resolution.remote_ref],
    )
    .ok();
    let Some(base) = merge_base.clone() else {
        return Ok(PullAnalysis {
            local_head,
            remote_head,
            merge_base: None,
            ahead: 0,
            behind: 0,
            topology: Topology::UnrelatedHistories,
            changed_local: 0,
            changed_remote: 0,
            overlapping_paths: Vec::new(),
            predicted_conflicts: 0,
            risk_score: 1.0,
            risk: RiskLevel::Critical,
        });
    };
    let counts = git_text(
        repo,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{}...{}", resolution.local_ref, resolution.remote_ref),
        ],
    )?;
    let mut fields = counts.split_whitespace();
    let ahead = fields.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let behind = fields.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let topology = match (ahead, behind) {
        (0, 0) => Topology::UpToDate,
        (0, _) => Topology::RemoteAhead,
        (_, 0) => Topology::LocalAhead,
        _ => Topology::Diverged,
    };
    let local_paths = changed_set(repo, &base, &resolution.local_ref)?;
    let remote_paths = changed_set(repo, &base, &resolution.remote_ref)?;
    let overlapping_paths: Vec<_> = local_paths.intersection(&remote_paths).cloned().collect();
    let total = local_paths.union(&remote_paths).count().max(1);
    let overlap_ratio = overlapping_paths.len() as f32 / total as f32;
    let rewrite = matches!(config.default_strategy.as_str(), "rebase") as u8 as f32;
    let protected = is_protected(&resolution.local_branch, &config.protected_branches) as u8 as f32;
    let risk_score = (0.25 * ((ahead + behind) as f32 / 50.0).min(1.0)
        + 0.40 * overlap_ratio
        + 0.15 * rewrite
        + 0.20 * protected)
        .min(1.0);
    let risk = if risk_score >= config.critical_risk_threshold {
        RiskLevel::Critical
    } else if risk_score >= 0.65 {
        RiskLevel::High
    } else if risk_score >= 0.30 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
    Ok(PullAnalysis {
        local_head,
        remote_head,
        merge_base: Some(base),
        ahead,
        behind,
        topology,
        changed_local: local_paths.len(),
        changed_remote: remote_paths.len(),
        predicted_conflicts: overlapping_paths.len(),
        overlapping_paths,
        risk_score,
        risk,
    })
}

fn select_strategy(
    analysis: &PullAnalysis,
    resolution: &RemoteResolution,
    options: &PullOptions,
    config: &PullConfig,
) -> Result<StrategyDecision, PullError> {
    let protected = is_protected(&resolution.local_branch, &config.protected_branches);
    let requested = options.strategy;
    let (selected, confidence, mut reasons) = match analysis.topology {
        Topology::UpToDate | Topology::LocalAhead => (
            IntegrationStrategy::Manual,
            1.0,
            vec![format!(
                "topology is {:?}; no integration is needed",
                analysis.topology
            )],
        ),
        Topology::RemoteAhead => (
            IntegrationStrategy::FastForward,
            1.0,
            vec!["local HEAD is an ancestor of the remote HEAD".to_owned()],
        ),
        Topology::UnrelatedHistories => (
            IntegrationStrategy::Manual,
            1.0,
            vec!["no merge base exists".to_owned()],
        ),
        Topology::Diverged if requested != IntegrationStrategy::Auto => (
            requested,
            1.0,
            vec!["strategy was explicitly selected".to_owned()],
        ),
        Topology::Diverged if protected => (
            IntegrationStrategy::Manual,
            0.95,
            vec!["protected branches forbid implicit reconciliation".to_owned()],
        ),
        Topology::Diverged if analysis.predicted_conflicts > 0 => (
            IntegrationStrategy::Hybreed,
            0.80,
            vec!["overlapping local and remote paths require reconciliation analysis".to_owned()],
        ),
        Topology::Diverged => {
            let configured = config
                .default_strategy
                .parse()
                .unwrap_or(IntegrationStrategy::Merge);
            let selected = if configured == IntegrationStrategy::Auto {
                IntegrationStrategy::Merge
            } else {
                configured
            };
            (
                selected,
                0.90,
                vec!["divergent histories have no overlapping changed paths".to_owned()],
            )
        }
    };
    if protected && selected == IntegrationStrategy::Rebase && !options.force {
        return Err(PullError::new(
            ExitCode::PolicyDenied,
            "rebase is forbidden on a protected branch without --force",
        ));
    }
    if requested == IntegrationStrategy::FastForward && analysis.topology != Topology::RemoteAhead {
        return Err(PullError::new(
            ExitCode::PolicyDenied,
            "fast-forward was requested but local HEAD is not an ancestor of the remote HEAD",
        ));
    }
    reasons.push(format!(
        "risk score {:.2} ({:?})",
        analysis.risk_score, analysis.risk
    ));
    Ok(StrategyDecision {
        requested,
        selected,
        confidence,
        reasons,
    })
}

fn integrate(
    repo: &Path,
    resolution: &RemoteResolution,
    strategy: IntegrationStrategy,
) -> Result<(), PullError> {
    let output = match strategy {
        IntegrationStrategy::FastForward => git_command(repo)
            .args(["merge", "--ff-only", &resolution.remote_ref])
            .output(),
        IntegrationStrategy::Merge
        | IntegrationStrategy::Hybreed
        | IntegrationStrategy::Emulsify => git_command(repo)
            .args(["merge", "--no-ff", "--no-edit", &resolution.remote_ref])
            .output(),
        IntegrationStrategy::Rebase => git_command(repo)
            .args(["rebase", &resolution.remote_ref])
            .output(),
        IntegrationStrategy::Auto | IntegrationStrategy::Manual => {
            return Err(PullError::new(
                ExitCode::PolicyDenied,
                "integration strategy is not executable",
            ))
        }
    }
    .map_err(io_error)?;
    classify_output("integration", output).map(|_| ())
}

fn verify(
    repo: &Path,
    resolution: &RemoteResolution,
    config: &PullConfig,
) -> Result<Verification, PullError> {
    let repository = git_command(repo)
        .args(["fsck", "--connectivity-only"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let conflicts = conflict_paths(repo)?.is_empty();
    let index = git_command(repo)
        .args(["diff", "--cached", "--check"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let worktree = git_command(repo)
        .args(["diff", "--check"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let head = git_text(repo, &["rev-parse", "HEAD"])
        .map(|value| !value.is_empty())
        .unwrap_or(false)
        && git_command(repo)
            .args([
                "merge-base",
                "--is-ancestor",
                &resolution.remote_ref,
                "HEAD",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    let build = config
        .verify_build
        .as_deref()
        .map(|command| shell_check(repo, command));
    let tests = config
        .verify_tests
        .as_deref()
        .map(|command| shell_check(repo, command));
    Ok(Verification {
        repository,
        index,
        conflicts,
        worktree,
        head,
        build,
        tests,
    })
}

fn create_autostash(repo: &Path, id: &str) -> Result<Option<String>, PullError> {
    let before = git_text(repo, &["rev-parse", "--verify", "refs/stash"]).ok();
    git_ok(
        repo,
        &[
            "stash",
            "push",
            "--include-untracked",
            "-m",
            &format!("kaptaind-pull:{id}"),
            "--",
            ".",
            ":(exclude).kaptaind",
        ],
    )?;
    let after = git_text(repo, &["rev-parse", "--verify", "refs/stash"]).ok();
    Ok((after != before).then_some(after).flatten())
}

fn restore_autostash(repo: &Path, transaction: &mut PullTransaction) -> Result<(), PullError> {
    let Some(oid) = transaction.autostash_oid.clone() else {
        return Ok(());
    };
    let output = git_command(repo)
        .args(["stash", "apply", "--index", &oid])
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(PullError::new(ExitCode::Conflicts, "pull succeeded, but restoring the Kaptaind autostash produced conflicts; the stash was preserved"));
    }
    if git_text(repo, &["rev-parse", "--verify", "refs/stash"])
        .ok()
        .as_deref()
        == Some(oid.as_str())
    {
        git_ok(repo, &["stash", "drop", "stash@{0}"])?;
    }
    transaction.autostash_oid = None;
    Ok(())
}

fn rollback_to_recovery(repo: &Path, transaction: &PullTransaction) -> Result<(), PullError> {
    let Some(reference) = transaction.recovery_ref.as_deref() else {
        return Ok(());
    };
    git_ok(repo, &["reset", "--hard", reference])
        .map_err(|error| PullError::new(ExitCode::Rollback, error.message))
}

struct ReportInput<'a> {
    repo: &'a Path,
    journal: &'a Path,
    transaction: &'a PullTransaction,
    resolution: &'a RemoteResolution,
    analysis: &'a PullAnalysis,
    decision: &'a StrategyDecision,
    fetch: &'a FetchResult,
    dry_run: bool,
}

fn base_report(input: ReportInput<'_>) -> PullReport {
    let ReportInput {
        repo,
        journal,
        transaction,
        resolution,
        analysis,
        decision,
        fetch,
        dry_run,
    } = input;
    PullReport {
        schema: JSON_SCHEMA.to_owned(),
        operation: "pull".to_owned(),
        status: "planned".to_owned(),
        transaction_id: transaction.transaction_id.clone(),
        repository: repo.to_path_buf(),
        remote: resolution.remote.clone(),
        remote_url: resolution.remote_url.clone(),
        local_ref: resolution.local_ref.clone(),
        remote_ref: resolution.remote_ref.clone(),
        before: analysis.local_head.clone(),
        remote_head: analysis.remote_head.clone(),
        after: None,
        topology: analysis.topology,
        strategy: decision.selected,
        strategy_confidence: decision.confidence,
        strategy_reasons: decision.reasons.clone(),
        ahead: analysis.ahead,
        behind: analysis.behind,
        risk: analysis.risk,
        risk_score: analysis.risk_score,
        conflicts: ConflictSummary {
            total: analysis.predicted_conflicts,
            resolved: 0,
            manual: analysis.predicted_conflicts,
            paths: analysis.overlapping_paths.clone(),
        },
        verification: Verification::default(),
        fetch: fetch.clone(),
        dry_run,
        journal: journal.to_path_buf(),
    }
}

pub fn render_text(report: &PullReport, verbose: bool) -> String {
    let mut output = format!(
        "Kaptaind Pull\n\nRepository:  {}\nRemote:      {}\nRemote ref:  {}\nLocal ref:   {}\n\nRemote state:\n  Local:      {}\n  Remote:     {}\n\nAnalysis:\n  Topology:             {:?}\n  Local commits ahead:  {}\n  Remote commits ahead: {}\n  Predicted conflicts:  {}\n  Risk:                 {:?} ({:.2})\n\nIntegration:\n  Strategy:             {:?}\n  Confidence:           {:.2}\n",
        report.repository.display(), report.remote, report.remote_ref, report.local_ref,
        short(&report.before), short(&report.remote_head), report.topology, report.ahead,
        report.behind, report.conflicts.total, report.risk, report.risk_score,
        report.strategy, report.strategy_confidence,
    );
    if verbose {
        output.push_str("  Reasons:\n");
        for reason in &report.strategy_reasons {
            output.push_str(&format!("    - {reason}\n"));
        }
        output.push_str(&format!("  Journal: {}\n", report.journal.display()));
    }
    let status = match report.status.as_str() {
        "up_to_date" => "Already up to date.",
        "local_ahead" => "Local branch is ahead of upstream; no integration was necessary.",
        "dry_run" => "DRY RUN: fetched and analysed remote state; no local branch, index, worktree, or history was modified.",
        "checked" => "CHECK COMPLETE: remote state was fetched and analysed; no integration was performed.",
        "success" => "KAPTAIND PULL COMPLETE: repository state verified.",
        other => other,
    };
    output.push_str(&format!("\nStatus:\n  {status}\n"));
    output
}

fn render_assessment(report: &PullReport) -> String {
    let warning = if report.strategy == IntegrationStrategy::Rebase {
        "\nWARNING: local commits will be rewritten. A recovery ref has been planned.\n"
    } else {
        ""
    };
    format!(
        "Kaptaind Pull Assessment\n\nRemote:      {}\nRemote ref:  {}\nLocal ref:   {}\nLocal HEAD:  {}\nRemote HEAD: {}\nTopology:    {:?}\nAhead:       {}\nBehind:      {}\nStrategy:    {:?}\nConflicts predicted: {}\nRisk:        {:?} ({:.2})\nConfidence:  {:.2}\n{}\nPlanning reconciliation...\n\n",
        report.remote,
        report.remote_ref,
        report.local_ref,
        short(&report.before),
        short(&report.remote_head),
        report.topology,
        report.ahead,
        report.behind,
        report.strategy,
        report.conflicts.total,
        report.risk,
        report.risk_score,
        report.strategy_confidence,
        warning,
    )
}

fn changed_set(repo: &Path, from: &str, to: &str) -> Result<BTreeSet<PathBuf>, PullError> {
    let bytes = git_bytes(repo, &["diff", "--name-only", "-z", from, to])?;
    Ok(bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| PathBuf::from(String::from_utf8_lossy(part).into_owned()))
        .collect())
}

fn conflict_paths(repo: &Path) -> Result<Vec<PathBuf>, PullError> {
    let bytes = git_bytes(repo, &["diff", "--name-only", "--diff-filter=U", "-z"])?;
    Ok(bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| PathBuf::from(String::from_utf8_lossy(part).into_owned()))
        .collect())
}

fn conflicts(repo: &Path) -> Result<Vec<Conflict>, PullError> {
    #[derive(Default)]
    struct Stages {
        modes: [Option<String>; 3],
        blobs: [Option<String>; 3],
    }

    let bytes = git_bytes(repo, &["ls-files", "-u", "-z"])?;
    let mut by_path = std::collections::BTreeMap::<PathBuf, Stages>::new();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        let Some(tab) = entry.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let header = String::from_utf8_lossy(&entry[..tab]);
        let mut fields = header.split_whitespace();
        let (Some(mode), Some(blob), Some(stage)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(stage) = stage.parse::<usize>() else {
            continue;
        };
        if !(1..=3).contains(&stage) {
            continue;
        }
        let path = PathBuf::from(String::from_utf8_lossy(&entry[tab + 1..]).into_owned());
        let stages = by_path.entry(path).or_default();
        stages.modes[stage - 1] = Some(mode.to_owned());
        stages.blobs[stage - 1] = Some(blob.to_owned());
    }
    let mut result = Vec::with_capacity(by_path.len());
    for (path, stages) in by_path {
        let kind = classify_conflict(repo, &stages.modes, &stages.blobs);
        result.push(Conflict {
            path,
            kind,
            base: stages.blobs[0].clone(),
            ours: stages.blobs[1].clone(),
            theirs: stages.blobs[2].clone(),
            resolution_status: ResolutionStatus::Unresolved,
            confidence: None,
        });
    }
    Ok(result)
}

fn classify_conflict(
    repo: &Path,
    modes: &[Option<String>; 3],
    blobs: &[Option<String>; 3],
) -> ConflictKind {
    if modes.iter().flatten().any(|mode| mode == "160000") {
        return ConflictKind::Submodule;
    }
    if modes.iter().flatten().any(|mode| mode == "120000") {
        return ConflictKind::Symlink;
    }
    if blobs[1].is_none() || blobs[2].is_none() {
        return ConflictKind::DeleteModify;
    }
    let distinct_modes: BTreeSet<_> = modes.iter().flatten().collect();
    if distinct_modes.len() > 1 {
        return ConflictKind::Mode;
    }
    if [blobs[1].as_deref(), blobs[2].as_deref()]
        .into_iter()
        .flatten()
        .any(|blob| blob_is_binary(repo, blob))
    {
        return ConflictKind::Binary;
    }
    ConflictKind::Content
}

fn blob_is_binary(repo: &Path, blob: &str) -> bool {
    git_bytes(repo, &["cat-file", "blob", blob])
        .map(|bytes| bytes.iter().take(8192).any(|byte| *byte == 0))
        .unwrap_or(false)
}

fn discover_root(repo: &Path) -> Result<PathBuf, PullError> {
    git_text(repo, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .map_err(|_| {
            PullError::new(
                ExitCode::UnsafeRepository,
                format!("{} is not a Git repository", repo.display()),
            )
        })
}

fn git_command(repo: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).env("GIT_TERMINAL_PROMPT", "0");
    command
}

fn git_text(repo: &Path, args: &[&str]) -> Result<String, PullError> {
    let output = git_command(repo).args(args).output().map_err(io_error)?;
    let output = classify_output(&args.join(" "), output)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_bytes(repo: &Path, args: &[&str]) -> Result<Vec<u8>, PullError> {
    let output = git_command(repo).args(args).output().map_err(io_error)?;
    Ok(classify_output(&args.join(" "), output)?.stdout)
}

fn git_ok(repo: &Path, args: &[&str]) -> Result<(), PullError> {
    git_text(repo, args).map(|_| ())
}

fn classify_output(operation: &str, output: Output) -> Result<Output, PullError> {
    if output.status.success() {
        return Ok(output);
    }
    let raw = String::from_utf8_lossy(&output.stderr);
    let message = redact_message(&raw);
    let lower = message.to_ascii_lowercase();
    let code = if lower.contains("authentication")
        || lower.contains("permission denied")
        || lower.contains("access denied")
        || lower.contains("could not read username")
    {
        ExitCode::Authentication
    } else if lower.contains("could not resolve host")
        || lower.contains("unable to access")
        || lower.contains("connection timed out")
        || lower.contains("connection reset")
        || lower.contains("repository not found")
        || lower.contains("couldn't find remote ref")
    {
        ExitCode::RemoteUnavailable
    } else if lower.contains("conflict") || lower.contains("could not apply") {
        ExitCode::Conflicts
    } else {
        ExitCode::Generic
    };
    Err(PullError::new(
        code,
        format!("git {operation} failed: {}", message.trim()),
    ))
}

fn validate_ref_component(value: &str) -> Result<(), PullError> {
    if value.is_empty()
        || value.starts_with('-')
        || value.contains(char::is_whitespace)
        || value.contains(['~', '^', ':', '?', '*', '[', '\\'])
    {
        return Err(PullError::invalid(format!(
            "unsafe Git ref component `{value}`"
        )));
    }
    Ok(())
}

fn validate_branch(value: &str) -> Result<(), PullError> {
    for component in value.split('/') {
        validate_ref_component(component)?;
    }
    if value.contains("..")
        || value.ends_with('.')
        || value.ends_with(".lock")
        || value.contains("@{")
    {
        return Err(PullError::invalid(format!("invalid branch name `{value}`")));
    }
    Ok(())
}

fn redact_url(value: &str) -> String {
    if let Ok(mut url) = reqwest::Url::parse(value) {
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        return url.to_string();
    }
    if let Some((_, host_path)) = value.rsplit_once('@') {
        return host_path.to_owned();
    }
    value.to_owned()
}

fn redact_message(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            if word.contains("://") {
                redact_url(word)
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_protected(branch: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        pattern == branch
            || pattern
                .strip_suffix("/*")
                .is_some_and(|prefix| branch.starts_with(&format!("{prefix}/")))
    })
}

fn shell_check(repo: &Path, command: &str) -> bool {
    Command::new("sh")
        .args(["-lc", command])
        .current_dir(repo)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn git_path_exists(repo: &Path, name: &str) -> bool {
    git_path(repo, name).is_some_and(|path| path.exists())
}

fn git_path(repo: &Path, name: &str) -> Option<PathBuf> {
    git_text(repo, &["rev-parse", "--git-path", name])
        .ok()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                repo.join(path)
            }
        })
}

fn transition(
    journal: &Path,
    transaction: &mut PullTransaction,
    state: TransactionState,
) -> Result<(), PullError> {
    transaction.state = state;
    transaction.updated_at = Utc::now();
    persist(journal, transaction)
}

fn fail_transaction(
    journal: &Path,
    transaction: &mut PullTransaction,
    message: &str,
) -> Result<(), PullError> {
    transaction.result = Some(message.to_owned());
    transition(journal, transaction, TransactionState::Failed)?;
    event(journal, transaction, "pull.failed", "failed", None)
}

fn persist(journal: &Path, transaction: &PullTransaction) -> Result<(), PullError> {
    fs::write(
        journal.join("metadata.json"),
        serde_json::to_vec_pretty(transaction).map_err(json_error)?,
    )
    .map_err(io_error)
}

fn read_transaction(journal: &Path) -> Result<PullTransaction, PullError> {
    let bytes = fs::read(journal.join("metadata.json")).map_err(io_error)?;
    serde_json::from_slice(&bytes).map_err(json_error)
}

fn write_result(journal: &Path, report: &PullReport) -> Result<(), PullError> {
    fs::write(
        journal.join("result.json"),
        serde_json::to_vec_pretty(report).map_err(json_error)?,
    )
    .map_err(io_error)
}

fn event(
    journal: &Path,
    transaction: &PullTransaction,
    name: &str,
    status: &str,
    duration_ms: Option<u128>,
) -> Result<(), PullError> {
    let value = serde_json::json!({
        "event": name, "transaction_id": transaction.transaction_id, "timestamp": Utc::now(),
        "repository": transaction.repository, "branch": transaction.remote.as_ref().map(|r| &r.local_branch),
        "stage": transaction.state, "duration_ms": duration_ms, "status": status,
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal.join("events.jsonl"))
        .map_err(io_error)?;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&value).map_err(json_error)?
    )
    .map_err(io_error)
}

fn latest_transaction(repo: &Path) -> Result<Option<PathBuf>, PullError> {
    let root = repo.join(".kaptaind").join("transactions");
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        if entry.path().join("metadata.json").exists() {
            candidates.push(entry.path());
        }
    }
    candidates.sort_by_key(|path| {
        fs::metadata(path.join("metadata.json"))
            .and_then(|m| m.modified())
            .ok()
    });
    Ok(candidates.pop())
}

struct OperationLock {
    path: PathBuf,
}

impl OperationLock {
    fn acquire(repo: &Path, transaction_id: &str) -> Result<Self, PullError> {
        let path = git_path(repo, "kaptaind/pull.lock").ok_or_else(|| {
            PullError::new(
                ExitCode::UnsafeRepository,
                "cannot resolve Git operation lock path",
            )
        })?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                PullError::new(ExitCode::InProgress, "repository is currently being modified by another Kaptaind pull transaction")
            } else { io_error(error) }
        })?;
        let contents = serde_json::json!({"transaction_id": transaction_id, "pid": std::process::id(), "created_at": Utc::now()});
        writeln!(file, "{}", contents).map_err(io_error)?;
        Ok(Self { path })
    }
}

impl Drop for OperationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn clear_stale_lock(repo: &Path) -> Result<(), PullError> {
    let repo = discover_root(repo)?;
    let Some(path) = git_path(&repo, "kaptaind/pull.lock") else {
        return Ok(());
    };
    let Ok(bytes) = fs::read(&path) else {
        return Ok(());
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        PullError::new(
            ExitCode::InProgress,
            "pull lock metadata is corrupt; inspect the lock before manual recovery",
        )
    })?;
    let pid = value.get("pid").and_then(serde_json::Value::as_u64);
    if pid.is_some_and(|pid| PathBuf::from(format!("/proc/{pid}")).exists()) {
        return Err(PullError::new(
            ExitCode::InProgress,
            format!(
                "Kaptaind pull process {} still owns the repository lock",
                pid.unwrap_or_default()
            ),
        ));
    }
    fs::remove_file(path).map_err(io_error)
}

fn short(value: &str) -> &str {
    value.get(..7).unwrap_or(value)
}
fn io_error(error: impl fmt::Display) -> PullError {
    PullError::new(ExitCode::Generic, error.to_string())
}
fn json_error(error: impl fmt::Display) -> PullError {
    PullError::new(ExitCode::Generic, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn strategy_parser_is_stable() {
        assert_eq!(
            "ff".parse::<IntegrationStrategy>().unwrap(),
            IntegrationStrategy::FastForward
        );
        assert!("reckless".parse::<IntegrationStrategy>().is_err());
    }

    #[test]
    fn credentials_are_redacted() {
        assert_eq!(
            redact_url("https://user:secret@example.com/repo.git?token=x"),
            "https://example.com/repo.git"
        );
        assert_eq!(
            redact_url("token@example.com:repo.git"),
            "example.com:repo.git"
        );
    }

    #[test]
    fn protected_branch_patterns_are_configurable() {
        let patterns = vec!["main".to_owned(), "release/*".to_owned()];
        assert!(is_protected("main", &patterns));
        assert!(is_protected("release/1.2", &patterns));
        assert!(!is_protected("development", &patterns));
    }

    #[test]
    fn check_classifies_up_to_date_without_moving_head() {
        let fixture = PullFixture::new();
        let before = git_text(&fixture.local, &["rev-parse", "HEAD"]).unwrap();
        let options = PullOptions {
            check: true,
            ..PullOptions::default()
        };
        let report = run(
            &fixture.local,
            &options,
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap();
        assert_eq!(report.topology, Topology::UpToDate);
        assert_eq!(report.status, "up_to_date");
        assert_eq!(
            git_text(&fixture.local, &["rev-parse", "HEAD"]).unwrap(),
            before
        );
    }

    #[test]
    fn dry_run_fetches_and_classifies_remote_ahead_without_integration() {
        let fixture = PullFixture::new();
        fixture.remote_commit("remote.txt", "remote\n");
        let before = git_text(&fixture.local, &["rev-parse", "HEAD"]).unwrap();
        let options = PullOptions {
            dry_run: true,
            ..PullOptions::default()
        };
        let report = run(
            &fixture.local,
            &options,
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap();
        assert_eq!(report.topology, Topology::RemoteAhead);
        assert_eq!(report.strategy, IntegrationStrategy::FastForward);
        assert_eq!(report.status, "dry_run");
        assert_eq!(
            git_text(&fixture.local, &["rev-parse", "HEAD"]).unwrap(),
            before
        );
        assert_ne!(report.remote_head, before);
    }

    #[test]
    fn remote_ahead_is_integrated_with_verified_fast_forward() {
        let fixture = PullFixture::new();
        fixture.remote_commit("remote.txt", "remote\n");
        let report = run(
            &fixture.local,
            &PullOptions::default(),
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap();
        assert_eq!(report.status, "success");
        assert_eq!(report.after.as_deref(), Some(report.remote_head.as_str()));
        assert!(report.verification.repository);
        assert!(report.verification.index);
        assert!(report.verification.conflicts);
        assert!(report.verification.head);
        assert!(fixture.local.join("remote.txt").exists());
    }

    #[test]
    fn local_ahead_requires_no_pull_integration() {
        let fixture = PullFixture::new();
        fixture.local_commit("local.txt", "local\n");
        let options = PullOptions {
            check: true,
            ..PullOptions::default()
        };
        let report = run(
            &fixture.local,
            &options,
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap();
        assert_eq!(report.topology, Topology::LocalAhead);
        assert_eq!(report.ahead, 1);
        assert_eq!(report.behind, 0);
        assert_eq!(report.status, "local_ahead");
    }

    #[test]
    fn diverged_non_overlapping_histories_select_explainable_merge() {
        let fixture = PullFixture::new();
        fixture.local_commit("local.txt", "local\n");
        fixture.remote_commit("remote.txt", "remote\n");
        let options = PullOptions {
            dry_run: true,
            ..PullOptions::default()
        };
        let report = run(
            &fixture.local,
            &options,
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap();
        assert_eq!(report.topology, Topology::Diverged);
        assert_eq!(report.strategy, IntegrationStrategy::Merge);
        assert_eq!(report.conflicts.total, 0);
        assert!(!report.strategy_reasons.is_empty());
    }

    #[test]
    fn dirty_worktree_is_refused_before_integration() {
        let fixture = PullFixture::new();
        fixture.remote_commit("remote.txt", "remote\n");
        fs::write(fixture.local.join("untracked.txt"), "do not lose me\n").unwrap();
        let error = run(
            &fixture.local,
            &PullOptions::default(),
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, ExitCode::UnsafeRepository);
        assert_eq!(
            fs::read_to_string(fixture.local.join("untracked.txt")).unwrap(),
            "do not lose me\n"
        );
    }

    #[test]
    fn merge_conflict_is_structured_and_abort_restores_original_head() {
        let fixture = PullFixture::new();
        fixture.local_commit("base.txt", "ours\n");
        fixture.remote_commit("base.txt", "theirs\n");
        let initial = git_text(&fixture.local, &["rev-parse", "HEAD"]).unwrap();
        let options = PullOptions {
            strategy: IntegrationStrategy::Merge,
            ..PullOptions::default()
        };
        let error = run(
            &fixture.local,
            &options,
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, ExitCode::Conflicts);
        let journal = latest_transaction(&fixture.local).unwrap().unwrap();
        let conflicts: Vec<Conflict> =
            serde_json::from_slice(&fs::read(journal.join("conflicts.json")).unwrap()).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, PathBuf::from("base.txt"));
        assert_eq!(conflicts[0].kind, ConflictKind::Content);
        assert!(conflicts[0].base.is_some());
        assert!(conflicts[0].ours.is_some());
        assert!(conflicts[0].theirs.is_some());
        abort(&fixture.local).unwrap();
        assert_eq!(
            git_text(&fixture.local, &["rev-parse", "HEAD"]).unwrap(),
            initial
        );
        assert!(!git_path_exists(&fixture.local, "MERGE_HEAD"));
    }

    #[test]
    fn autostash_restores_untracked_work_after_fast_forward() {
        let fixture = PullFixture::new();
        fixture.remote_commit("remote.txt", "remote\n");
        fs::write(fixture.local.join("draft.txt"), "draft\n").unwrap();
        let options = PullOptions {
            autostash: true,
            ..PullOptions::default()
        };
        let report = run(
            &fixture.local,
            &options,
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap();
        assert_eq!(report.status, "success");
        assert_eq!(
            fs::read_to_string(fixture.local.join("draft.txt")).unwrap(),
            "draft\n"
        );
        assert!(fixture.local.join("remote.txt").exists());
    }

    #[test]
    fn clean_rebase_records_recovery_and_rewrites_local_commit() {
        let fixture = PullFixture::new();
        fixture.local_commit("local.txt", "local\n");
        fixture.remote_commit("remote.txt", "remote\n");
        let original = git_text(&fixture.local, &["rev-parse", "HEAD"]).unwrap();
        let options = PullOptions {
            strategy: IntegrationStrategy::Rebase,
            ..PullOptions::default()
        };
        let report = run(
            &fixture.local,
            &options,
            &PullConfig::default(),
            &IntegrationsConfig::default(),
        )
        .unwrap();
        assert_eq!(report.status, "success");
        assert_ne!(report.after.as_deref(), Some(original.as_str()));
        assert!(git_text(
            &fixture.local,
            &[
                "show-ref",
                "--verify",
                &format!("refs/kaptaind/recovery/{}", report.transaction_id)
            ]
        )
        .is_ok());
    }

    struct PullFixture {
        _root: TempDir,
        writer: PathBuf,
        local: PathBuf,
    }

    impl PullFixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let remote = root.path().join("remote.git");
            let writer = root.path().join("writer");
            let local = root.path().join("local");
            test_git(root.path(), &["init", "--bare", remote.to_str().unwrap()]);
            test_git(root.path(), &["init", writer.to_str().unwrap()]);
            configure_identity(&writer);
            fs::write(writer.join("base.txt"), "base\n").unwrap();
            test_git(&writer, &["add", "base.txt"]);
            test_git(&writer, &["commit", "-m", "base"]);
            test_git(&writer, &["branch", "-M", "dev"]);
            test_git(
                &writer,
                &["remote", "add", "origin", remote.to_str().unwrap()],
            );
            test_git(&writer, &["push", "-u", "origin", "dev"]);
            test_git(&remote, &["symbolic-ref", "HEAD", "refs/heads/dev"]);
            test_git(
                root.path(),
                &["clone", remote.to_str().unwrap(), local.to_str().unwrap()],
            );
            configure_identity(&local);
            Self {
                _root: root,
                writer,
                local,
            }
        }

        fn remote_commit(&self, path: &str, contents: &str) {
            fs::write(self.writer.join(path), contents).unwrap();
            test_git(&self.writer, &["add", path]);
            test_git(&self.writer, &["commit", "-m", "remote change"]);
            test_git(&self.writer, &["push", "origin", "dev"]);
        }

        fn local_commit(&self, path: &str, contents: &str) {
            fs::write(self.local.join(path), contents).unwrap();
            test_git(&self.local, &["add", path]);
            test_git(&self.local, &["commit", "-m", "local change"]);
        }
    }

    fn configure_identity(repo: &Path) {
        test_git(repo, &["config", "user.name", "Kaptaind Pull Test"]);
        test_git(repo, &["config", "user.email", "pull-test@example.invalid"]);
    }

    fn test_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
