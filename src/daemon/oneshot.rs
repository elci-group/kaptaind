//! One-shot transaction for `kaptaind commit` (AGX-1).
//!
//! Runs exactly one cluster, built from the current pending changes, through
//! the daemon's own pipeline ([`scheduler::process_cluster_with_options`]:
//! suspend gate, test hook, analysis, version writeback, commit), optionally
//! pushes through the push orchestrator, and returns a structured report.
//! No stage is reimplemented here — this module only assembles the state the
//! scheduler normally owns, then reads back the decision it recorded.

use crate::config::Config;
use crate::daemon::decisions::outcome;
use crate::daemon::scheduler::{self, IgnoreMatcher, SelfWriteGuard, State, StatusReport};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::task::JoinSet;

/// Outcome code for a refusal because a live daemon owns the repository.
pub const DAEMON_RUNNING: &str = "daemon_running";
/// Outcome code for a refusal because `[trust].execution` is not trusted.
pub const UNTRUSTED: &str = "untrusted";
/// Outcome code for a refusal because RBAC denies `daemon.start`.
pub const UNAUTHORIZED: &str = "unauthorized";
/// Outcome code for a refusal because Shark HA is enabled: only the elected
/// leader daemon may commit.
pub const HA_ENABLED: &str = "ha_enabled";
/// Outcome code for a refusal because another `kaptaind commit` holds
/// `.kaptaind/commit.lock`.
pub const COMMIT_IN_PROGRESS: &str = "commit_in_progress";

/// Exit code: a commit was created (and any requested push succeeded).
pub const EXIT_COMMITTED: i32 = 0;
/// Exit code: an error occurred.
pub const EXIT_ERROR: i32 = 1;
/// Exit code: nothing to commit (clean tree, below threshold, no bump).
pub const EXIT_NOTHING: i32 = 3;
/// Exit code: refused by a gate (suspended, observe mode, untrusted, tests
/// failed, baseline unresolvable, live daemon, push disabled/refused, ...).
pub const EXIT_REFUSED: i32 = 4;

/// Per-cluster overrides threaded into the scheduler pipeline by the one-shot
/// path. The daemon passes [`ClusterOptions::default`], which changes nothing.
#[derive(Debug, Clone, Default)]
pub struct ClusterOptions {
    /// Bypass the test hook for this cluster only (AGX-5.3). Recorded in the
    /// decision reason and as a `Kaptaind-Skipped-Tests:` commit trailer.
    pub skip_tests_reason: Option<String>,
}

impl ClusterOptions {
    /// Append this transaction's trailers to a commit message, joining an
    /// existing trailer block (e.g. `Signed-off-by:`) when there is one.
    pub(crate) fn apply_trailers(&self, msg: String) -> String {
        let Some(reason) = self.skip_tests_reason.as_deref() else {
            return msg;
        };
        let trailer = format!("Kaptaind-Skipped-Tests: {reason}");
        let trimmed = msg.trim_end();
        let joins_block = trimmed
            .rsplit_once("\n\n")
            .map(|(_, last)| !last.is_empty() && last.lines().all(is_trailer_line))
            .unwrap_or(false);
        if joins_block {
            format!("{trimmed}\n{trailer}")
        } else {
            format!("{trimmed}\n\n{trailer}")
        }
    }

    /// Decorate a decision reason with the skipped-tests justification.
    pub(crate) fn decision_reason(&self, base: impl Into<String>) -> String {
        let base = base.into();
        match self.skip_tests_reason.as_deref() {
            Some(reason) => format!("{base}; tests skipped: {reason}"),
            None => base,
        }
    }
}

/// `Token: value` with a capitalized, space-free token (git trailer shape).
fn is_trailer_line(line: &str) -> bool {
    let Some((token, _)) = line.split_once(": ") else {
        return false;
    };
    token.starts_with(|c: char| c.is_ascii_uppercase())
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Normalize a `--skip-tests` reason to one non-empty line.
pub fn normalize_skip_reason(reason: &str) -> anyhow::Result<String> {
    let line = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        anyhow::bail!("--skip-tests requires a non-empty REASON");
    }
    Ok(line)
}

/// Inputs for one `kaptaind commit` transaction.
#[derive(Debug, Clone, Default)]
pub struct OneshotOptions {
    /// Restrict the cluster and staging to these absolute paths (files or
    /// directories). Empty means all pending changes.
    pub paths: Vec<PathBuf>,
    /// Push after a successful commit.
    pub push: bool,
    /// Run even while a live daemon holds `.kaptaind/daemon.pid`.
    pub force: bool,
    /// Bypass the test hook for this transaction (mandatory justification).
    pub skip_tests_reason: Option<String>,
}

/// Push result reported by `kaptaind commit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PushStatus {
    Ok,
    Disabled,
    Refused,
    Failed,
    /// No commit was created, so there was nothing to push.
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushReport {
    pub status: PushStatus,
    pub detail: String,
}

/// Machine-readable result of one transaction (`kaptaind commit --json`).
#[derive(Debug, Clone, Serialize)]
pub struct OneshotReport {
    /// Decision outcome code (see `daemon::decisions::outcome`), or one of
    /// [`DAEMON_RUNNING`] / [`UNTRUSTED`] for pre-flight refusals.
    pub outcome: String,
    pub version_before: Option<String>,
    pub version_after: Option<String>,
    /// Commit SHA, or null when no commit was created.
    pub commit: Option<String>,
    pub push: PushReport,
    pub reason: String,
    pub cluster_id: Option<String>,
    /// Project-relative paths in the cluster.
    pub paths: Vec<String>,
    pub skipped_tests: Option<String>,
    /// Whether the push was explicitly requested with `--push`.
    #[serde(skip)]
    pub push_requested: bool,
}

impl OneshotReport {
    fn new(outcome: &str, reason: impl Into<String>) -> Self {
        Self {
            outcome: outcome.to_string(),
            version_before: None,
            version_after: None,
            commit: None,
            push: PushReport {
                status: PushStatus::Skipped,
                detail: "no commit created".to_string(),
            },
            reason: reason.into(),
            cluster_id: None,
            paths: Vec::new(),
            skipped_tests: None,
            push_requested: false,
        }
    }

    pub fn committed(&self) -> bool {
        self.commit.is_some()
    }

    /// Process exit code for this report (documented in `kaptaind commit --help`).
    pub fn exit_code(&self) -> i32 {
        let base = outcome_exit_code(&self.outcome);
        if base != EXIT_COMMITTED || !self.committed() {
            return if base == EXIT_COMMITTED {
                EXIT_ERROR
            } else {
                base
            };
        }
        match self.push.status {
            PushStatus::Ok | PushStatus::Skipped => EXIT_COMMITTED,
            PushStatus::Disabled if self.push_requested => EXIT_REFUSED,
            PushStatus::Disabled => EXIT_COMMITTED,
            PushStatus::Refused => EXIT_REFUSED,
            PushStatus::Failed => EXIT_ERROR,
        }
    }
}

/// Map a decision outcome code to the `kaptaind commit` exit code.
pub fn outcome_exit_code(code: &str) -> i32 {
    match code {
        outcome::COMMIT | outcome::CHORE_COMMIT => EXIT_COMMITTED,
        outcome::CLEAN_TREE | outcome::NO_BUMP | outcome::NOTHING_STAGED => EXIT_NOTHING,
        outcome::SUSPENDED
        | outcome::OBSERVED
        | outcome::BLOCKED
        | outcome::TEST_FAILED
        | outcome::TEST_HOOK_NOT_FOUND
        | outcome::BASELINE_UNRESOLVABLE
        | outcome::VERSION_MISMATCH
        | outcome::PRE_COMMIT_HOOK_FAILED
        | outcome::RATE_LIMITED
        | DAEMON_RUNNING
        | UNTRUSTED
        | UNAUTHORIZED
        | HA_ENABLED
        | COMMIT_IN_PROGRESS => EXIT_REFUSED,
        _ => EXIT_ERROR,
    }
}

/// Canonicalize `path`, or its parent for a path that no longer exists, so
/// `--paths` filters and pending paths compare through the same symlinks
/// (e.g. macOS `/tmp` -> `/private/tmp`) and without `..` components.
fn canonical_lossy(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// Narrow pending paths to `filters`. A pending path equal to or beneath a
/// filter is kept. git reports a new untracked directory as the directory
/// itself, so a filter beneath a pending directory selects the filter path
/// (only that file or subtree is clustered and staged, not the whole
/// directory).
fn select_paths(pending: Vec<PathBuf>, filters: &[PathBuf]) -> Vec<PathBuf> {
    let filters: Vec<PathBuf> = filters.iter().map(|f| canonical_lossy(f)).collect();
    let mut selected: Vec<PathBuf> = Vec::new();
    for path in pending {
        let canonical = canonical_lossy(&path);
        if filters.iter().any(|f| canonical.starts_with(f)) {
            selected.push(path);
            continue;
        }
        if canonical.is_dir() {
            for filter in filters.iter().filter(|f| f.starts_with(&canonical)) {
                let rel = filter.strip_prefix(&canonical).unwrap_or(filter);
                selected.push(path.join(rel));
            }
        }
    }
    selected.sort();
    selected.dedup();
    selected
}

/// Exclusive `.kaptaind/commit.lock` held for one transaction; removed on
/// drop. A lock left by a dead process is reclaimed.
struct CommitLock {
    path: PathBuf,
}

impl CommitLock {
    /// `Ok(None)` when a live process already holds the lock.
    fn acquire(repo_path: &Path) -> anyhow::Result<Option<Self>> {
        let dir = repo_path.join(".kaptaind");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("commit.lock");
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    writeln!(file, "{}", std::process::id())?;
                    return Ok(Some(Self { path }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Removes the file when its pid is dead or unparseable.
                    if let crate::daemon::pidfile::PidFileState::Live(_) =
                        crate::daemon::pidfile::validate_and_clean(&path)
                    {
                        return Ok(None);
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(None)
    }

    fn holder(repo_path: &Path) -> Option<String> {
        std::fs::read_to_string(repo_path.join(".kaptaind").join("commit.lock"))
            .ok()
            .map(|pid| pid.trim().to_string())
    }
}

impl Drop for CommitLock {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            tracing::warn!(%error, path = %self.path.display(), "failed to remove commit lock");
        }
    }
}

fn head_sha(repo: &crate::git::repo::Repo) -> Option<String> {
    repo.head_commit_hash().ok().filter(|sha| !sha.is_empty())
}

/// Run one `kaptaind commit` transaction. `config` must already carry any
/// CLI overrides (`--repo`, `--config`). Pre-flight refusals are returned as
/// reports, not errors; `Err` means the transaction could not be attempted.
pub async fn run(config: &Config, opts: &OneshotOptions) -> anyhow::Result<OneshotReport> {
    // AGX-1.5: never race a live daemon over the same worktree.
    let pid_path = config.repo_path.join(".kaptaind").join("daemon.pid");
    if let crate::daemon::pidfile::PidFileState::Live(pid) =
        crate::daemon::pidfile::validate_and_clean(&pid_path)
    {
        if pid != std::process::id() && !opts.force {
            return Ok(OneshotReport::new(
                DAEMON_RUNNING,
                format!(
                    "a kaptaind daemon (pid {pid}) is running for {}; it will commit these \
                     changes itself — let it, or pass --force",
                    config.repo_path.display()
                ),
            ));
        }
    }

    // Same validation as the daemon start path; an untrusted configuration
    // is a gate refusal, any other validation failure an error.
    if let Err(error) = config.validate_execution_trust() {
        return Ok(OneshotReport::new(UNTRUSTED, error.to_string()));
    }
    config.validate()?;
    // The same authorization the daemon start path enforces before running
    // this pipeline.
    if let Err(error) = crate::rbac::check_permission(&config.rbac, "daemon.start") {
        return Ok(OneshotReport::new(UNAUTHORIZED, error.to_string()));
    }
    if config.shark.enabled {
        return Ok(OneshotReport::new(
            HA_ENABLED,
            "[shark] HA is enabled: only the elected leader daemon commits for this \
             repository; `kaptaind commit` cannot take part in leader election",
        ));
    }
    crate::git::repo::ensure_git_available()
        .map_err(|err| anyhow::anyhow!("kaptaind requires git in PATH: {err}"))?;
    crate::util::permissions::harden_runtime_tree(&config.repo_path)
        .map_err(|err| anyhow::anyhow!("failed to secure .kaptaind runtime state: {err}"))?;
    let Some(_lock) = CommitLock::acquire(&config.repo_path)? else {
        let holder = CommitLock::holder(&config.repo_path).unwrap_or_else(|| "?".to_string());
        return Ok(OneshotReport::new(
            COMMIT_IN_PROGRESS,
            format!("another `kaptaind commit` (pid {holder}) is running for this repository"),
        ));
    };

    let mut repo = crate::git::repo::Repo::open(&config.repo_path)?;

    // Build the cluster before any kaptaind write (bootstrap, status.json)
    // so only genuine pending work is clustered.
    let ignore = IgnoreMatcher::load(&config.repo_path, &config.watch.ignore_file);
    let mut paths = crate::dryrun::pending_paths(config, &repo)?;
    paths.retain(|p| !ignore.is_ignored(std::slice::from_ref(p)));
    if !opts.paths.is_empty() {
        paths = select_paths(paths, &opts.paths);
    }
    if paths.is_empty() {
        let reason = if opts.paths.is_empty() {
            "working tree clean; nothing to commit".to_string()
        } else {
            "no pending changes under the given --paths".to_string()
        };
        let mut report = OneshotReport::new(outcome::CLEAN_TREE, reason);
        report.version_before = crate::version::resolve_baseline(&config.repo_path)
            .ok()
            .map(|v| v.to_string());
        report.version_after = report.version_before.clone();
        report.push_requested = opts.push;
        return Ok(report);
    }

    // A suspended repository refuses before any write, so the bootstrap
    // below never leaves a seeded VERSION behind a refusal.
    if let Ok(Some(state)) = crate::daemon::suspend::load(&config.repo_path) {
        let mut report = OneshotReport::new(
            outcome::SUSPENDED,
            format!(
                "daemon is suspended{}",
                state.reason.map(|r| format!(": {r}")).unwrap_or_default()
            ),
        );
        report.push_requested = opts.push;
        return Ok(report);
    }

    // First-run version enforcement, exactly as the daemon start path does.
    let baseline = match crate::version::bootstrap::ensure_baseline(
        &config.repo_path,
        &config.versioning,
        config.operation.mode,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return Ok(OneshotReport::new(
                outcome::BASELINE_UNRESOLVABLE,
                error.to_string(),
            ))
        }
    };
    if let crate::version::bootstrap::BootstrapOutcome::Initialized { version, source } = &baseline
    {
        tracing::info!(%version, %source, "initialized VERSION baseline for first commit");
    }
    let version_before = crate::version::resolve_baseline(&config.repo_path)
        .ok()
        .map(|v| v.to_string());

    // The pipeline runs with push disabled; this module pushes afterwards so
    // the result is observable and the branch is never pushed twice.
    let mut pipeline_config = config.clone();
    pipeline_config.push.enabled = false;
    if !opts.paths.is_empty() {
        // AGX-1.2: stage only the cluster (+ version files), never the rest
        // of the dirty tree.
        pipeline_config.staging.mode = crate::config::loader::StagingMode::Cluster;
    }
    let cluster_opts = ClusterOptions {
        skip_tests_reason: opts.skip_tests_reason.clone(),
    };

    let cluster = crate::dryrun::synthetic_cluster(paths);
    let cluster_id = cluster.id.to_string();
    let rel_paths: Vec<String> = cluster
        .events
        .iter()
        .flat_map(|e| e.paths.iter())
        .map(|p| {
            p.strip_prefix(&config.repo_path)
                .unwrap_or(p)
                .display()
                .to_string()
        })
        .collect();

    let angler = match crate::angler::AnglerSystem::new(&config.angler, &config.repo_path) {
        Ok(system) => Some(system),
        Err(error) => {
            tracing::error!(error = %error, "failed to initialize angler system");
            None
        }
    };
    let (vacs_engine, vacs_rx) = crate::vacs::VacsEngine::new(&config.repo_path, &config.vacs);
    let mut tasks: JoinSet<()> = JoinSet::new();
    let (shutdown_handle, shutdown) = crate::daemon::shutdown::channel();
    let metrics = Arc::new(crate::daemon::health::Metrics::default());
    let (event_tx, _event_rx) = tokio::sync::broadcast::channel(64);
    let mut self_writes = SelfWriteGuard::new(config.repo_path.clone());
    let mut test_streak = scheduler::TestFailureStreak::default();
    let mut last_commit_at = None;
    let mut status = StatusReport {
        status: State::Idle,
        last_version: version_before.clone(),
        last_action_time: chrono::Utc::now(),
        last_error: None,
        current_task: None,
        progress_percent: None,
    };

    let head_before = head_sha(&repo);
    scheduler::process_cluster_with_options(
        &mut repo,
        &pipeline_config,
        &mut last_commit_at,
        cluster,
        &mut status,
        &mut self_writes,
        &mut test_streak,
        &vacs_engine,
        &mut tasks,
        shutdown.clone_token(),
        angler.as_ref(),
        metrics,
        event_tx,
        &cluster_opts,
    )
    .await;
    let head_after = head_sha(&repo);

    // A VERSION seeded by this transaction's bootstrap is not left behind
    // when the transaction does not commit.
    if head_after == head_before {
        if let crate::version::bootstrap::BootstrapOutcome::Initialized { .. } = &baseline {
            let seeded = config.repo_path.join("VERSION");
            if let Err(error) = std::fs::remove_file(&seeded) {
                tracing::warn!(%error, path = %seeded.display(), "failed to remove seeded VERSION");
            }
        }
    }

    // Read back the decision this cluster recorded.
    let record = crate::daemon::decisions::tail_decisions(&config.repo_path, 200)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .find(|r| r.cluster_id == cluster_id);
    let mut report = match &record {
        Some(record) => OneshotReport::new(&record.outcome, record.reason.clone()),
        None => OneshotReport::new(
            outcome::ERROR,
            status
                .last_error
                .clone()
                .unwrap_or_else(|| "pipeline exited without recording a decision".to_string()),
        ),
    };
    report.cluster_id = Some(cluster_id);
    report.paths = rel_paths;
    report.skipped_tests = opts.skip_tests_reason.clone();
    report.push_requested = opts.push;
    report.version_before = version_before.clone();
    report.version_after = record
        .as_ref()
        .filter(|r| r.outcome == outcome::COMMIT)
        .and_then(|r| r.version.clone())
        .or(version_before);
    let committed = matches!(
        report.outcome.as_str(),
        outcome::COMMIT | outcome::CHORE_COMMIT
    );
    if committed && head_after.is_some() && head_after != head_before {
        report.commit = head_after;
    } else if committed {
        report.outcome = outcome::ERROR.to_string();
        report.reason = "decision recorded a commit but HEAD did not move".to_string();
    }

    if report.committed() {
        report.push = push_after_commit(config, opts.push, angler.as_ref(), &report).await;
    }

    // Let spawned post-commit work (VACS, qualification, pruning) finish
    // before the process exits.
    let mut vacs_rx = vacs_rx;
    vacs_rx.close();
    Arc::clone(&vacs_engine).process_queue(vacs_rx).await;
    while tasks.join_next().await.is_some() {}
    drop(shutdown_handle);

    Ok(report)
}

/// AGX-1.3: push through the configured orchestrator (protected branches,
/// pre-push hooks, CI gating, retry). A closed gate is reported, never
/// silently skipped.
async fn push_after_commit(
    config: &Config,
    requested: bool,
    angler: Option<&crate::angler::AnglerSystem>,
    report: &OneshotReport,
) -> PushReport {
    let gate_open = config.push.enabled && config.capabilities.network_push;
    if !gate_open {
        let detail = if requested {
            "push is disabled: set [push] enabled = true and [capabilities] network_push = true \
             in kaptaind.toml to allow kaptaind to push commits"
        } else {
            "push not requested and disabled by configuration ([push] enabled / \
             [capabilities] network_push)"
        };
        return PushReport {
            status: PushStatus::Disabled,
            detail: detail.to_string(),
        };
    }
    match crate::push::run_configured(
        &config.repo_path,
        config,
        angler,
        &crate::push::PushOverrides::default(),
    )
    .await
    {
        Ok(summary) => {
            let remotes = summary.remotes_pushed.join(", ");
            if !summary.dry_run {
                crate::daemon::notification::notify_push_success(
                    &config.notify,
                    report.version_after.as_deref().unwrap_or("unknown"),
                    &summary.branch,
                    &remotes,
                    config.capabilities.network_webhooks,
                );
            }
            PushReport {
                status: PushStatus::Ok,
                detail: if summary.dry_run {
                    format!(
                        "dry run: would push branch '{}' to {remotes}",
                        summary.branch
                    )
                } else {
                    format!("pushed branch '{}' to {remotes}", summary.branch)
                },
            }
        }
        Err(error) => {
            let detail = error.to_string();
            let refused = error
                .downcast_ref::<crate::push::PrePushHookFailed>()
                .is_some()
                || detail.contains("protected branch")
                || detail.contains("required CI checks");
            crate::daemon::notification::notify_warning(
                &config.notify,
                &detail,
                if refused {
                    "Push refused"
                } else {
                    "Push failed"
                },
                config.capabilities.network_webhooks,
            );
            PushReport {
                status: if refused {
                    PushStatus::Refused
                } else {
                    PushStatus::Failed
                },
                detail,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailer_starts_new_paragraph_after_body() {
        let opts = ClusterOptions {
            skip_tests_reason: Some("flaky CI".into()),
        };
        let msg = opts.apply_trailers("feat: x\n\nkaptaind: Minor -> v1.1.0 [..]".into());
        assert_eq!(
            msg,
            "feat: x\n\nkaptaind: Minor -> v1.1.0 [..]\n\nKaptaind-Skipped-Tests: flaky CI"
        );
    }

    #[test]
    fn trailer_joins_signoff_block() {
        let opts = ClusterOptions {
            skip_tests_reason: Some("hotfix".into()),
        };
        let msg = opts.apply_trailers("feat: x\n\nbody\n\nSigned-off-by: k <k@l>".into());
        assert!(msg.ends_with("Signed-off-by: k <k@l>\nKaptaind-Skipped-Tests: hotfix"));
    }

    #[test]
    fn default_options_are_inert() {
        let opts = ClusterOptions::default();
        assert_eq!(opts.apply_trailers("a\n\nb".into()), "a\n\nb");
        assert_eq!(opts.decision_reason("committed"), "committed");
    }

    #[test]
    fn skip_reason_is_normalized_and_required() {
        assert_eq!(normalize_skip_reason("  a\nb  ").unwrap(), "a b");
        assert!(normalize_skip_reason("  \n ").is_err());
    }

    #[test]
    fn exit_codes_follow_outcomes() {
        assert_eq!(outcome_exit_code(outcome::NO_BUMP), EXIT_NOTHING);
        assert_eq!(outcome_exit_code(outcome::CLEAN_TREE), EXIT_NOTHING);
        assert_eq!(outcome_exit_code(outcome::SUSPENDED), EXIT_REFUSED);
        assert_eq!(outcome_exit_code(outcome::TEST_FAILED), EXIT_REFUSED);
        assert_eq!(outcome_exit_code(DAEMON_RUNNING), EXIT_REFUSED);
        assert_eq!(outcome_exit_code(outcome::COMMIT_FAILED), EXIT_ERROR);
        assert_eq!(outcome_exit_code(outcome::COMMIT), EXIT_COMMITTED);
    }

    #[test]
    fn requested_push_that_is_disabled_is_a_refusal() {
        let mut report = OneshotReport::new(outcome::COMMIT, "committed");
        report.commit = Some("abc".into());
        report.push.status = PushStatus::Disabled;
        assert_eq!(report.exit_code(), EXIT_COMMITTED);
        report.push_requested = true;
        assert_eq!(report.exit_code(), EXIT_REFUSED);
        report.push.status = PushStatus::Failed;
        assert_eq!(report.exit_code(), EXIT_ERROR);
        report.push.status = PushStatus::Ok;
        assert_eq!(report.exit_code(), EXIT_COMMITTED);
    }

    #[test]
    fn path_filter_matches_files_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("srcfoo")).unwrap();
        for file in ["src/lib.rs", "a.txt", "b.txt"] {
            std::fs::write(root.join(file), "x").unwrap();
        }
        let pending = vec![
            root.join("src/lib.rs"),
            root.join("a.txt"),
            root.join("srcfoo"),
            root.join("b.txt"),
        ];
        let filters = vec![root.join("src"), root.join("a.txt")];
        assert_eq!(
            select_paths(pending, &filters),
            vec![root.join("a.txt"), root.join("src/lib.rs")]
        );
    }

    #[test]
    fn path_filter_selects_file_inside_new_untracked_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("src/newmod")).unwrap();
        std::fs::write(root.join("src/newmod/lib.rs"), "x").unwrap();
        std::fs::write(root.join("src/newmod/other.rs"), "x").unwrap();
        // git reports the untracked directory itself.
        let pending = vec![root.join("src/newmod")];
        let filters = vec![root.join("src/newmod/lib.rs")];
        assert_eq!(
            select_paths(pending, &filters),
            vec![root.join("src/newmod/lib.rs")]
        );
    }

    #[test]
    fn path_filter_compares_through_dotdot_components() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.rs"), "x").unwrap();
        // Pending path built from a non-canonical repo_path (`--repo sub/..`).
        let pending = vec![root.join("sub/../a.rs")];
        let filters = vec![root.join("a.rs")];
        assert_eq!(select_paths(pending, &filters).len(), 1);
    }

    #[test]
    fn commit_lock_is_exclusive_and_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let lock = CommitLock::acquire(dir.path()).unwrap();
        assert!(lock.is_some());
        assert!(CommitLock::acquire(dir.path()).unwrap().is_none());
        drop(lock);
        assert!(CommitLock::acquire(dir.path()).unwrap().is_some());
    }

    #[test]
    fn test_hook_not_found_is_a_refusal() {
        assert_eq!(
            outcome_exit_code(outcome::TEST_HOOK_NOT_FOUND),
            EXIT_REFUSED
        );
        assert_eq!(outcome_exit_code(HA_ENABLED), EXIT_REFUSED);
        assert_eq!(outcome_exit_code(COMMIT_IN_PROGRESS), EXIT_REFUSED);
    }
}
