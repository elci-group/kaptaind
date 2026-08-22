//! Explicit Git branch lifecycle and release governance.
//!
//! Git refs are treated as projections of a typed lifecycle.  In particular,
//! production refs are only writable by [`issue_release`] and [`rollback`].

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

pub const STATE_FILE: &str = ".kaptaind/lifecycle.json";
pub const STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Desktop,
    Mobile,
}

impl Platform {
    pub fn development_branch(&self) -> &'static str {
        match self {
            Self::Desktop => "desktop/development",
            Self::Mobile => "mobile/development",
        }
    }

    pub fn production_branch(&self) -> &'static str {
        match self {
            Self::Desktop => "desktop/production",
            Self::Mobile => "mobile/production",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "version", rename_all = "kebab-case")]
pub enum BranchRole {
    DesktopDevelopment,
    DesktopProduction,
    MobileDevelopment,
    MobileProduction,
    Integration,
    LocalStaging,
    ServerStaging,
    Release(Version),
    Unmanaged,
}

impl BranchRole {
    pub fn classify(name: &str) -> Self {
        match name {
            "desktop/development" => Self::DesktopDevelopment,
            "desktop/production" => Self::DesktopProduction,
            "mobile/development" => Self::MobileDevelopment,
            "mobile/production" => Self::MobileProduction,
            "integration" => Self::Integration,
            "local/staging" => Self::LocalStaging,
            "server/staging" => Self::ServerStaging,
            _ => name
                .strip_prefix("release/")
                .and_then(|version| Version::parse(version).ok())
                .map(Self::Release)
                .unwrap_or(Self::Unmanaged),
        }
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::DesktopProduction | Self::MobileProduction)
    }
}

pub const MANDATORY_BRANCHES: &[&str] = &[
    "desktop/development",
    "desktop/production",
    "mobile/development",
    "mobile/production",
    "integration",
    "local/staging",
    "server/staging",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationGate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRecord {
    pub commit: String,
    pub validated_at: DateTime<Utc>,
    pub gates: Vec<ValidationGate>,
}

impl ValidationRecord {
    pub fn passed(&self) -> bool {
        self.gates.iter().all(|gate| gate.passed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCandidate {
    pub version: Version,
    pub branch: String,
    pub source_branch: String,
    pub source_commit: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<ValidationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseEvent {
    pub version: Version,
    pub source_commit: String,
    pub deployed_commit: String,
    pub candidate_branch: String,
    pub production_branch: String,
    pub issued_at: DateTime<Utc>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_of: Option<Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagingRecord {
    pub source_commit: String,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleState {
    pub schema: u32,
    #[serde(default)]
    pub candidates: BTreeMap<String, ReleaseCandidate>,
    #[serde(default)]
    pub releases: Vec<ReleaseEvent>,
    #[serde(default)]
    pub staging: BTreeMap<String, StagingRecord>,
}

impl Default for LifecycleState {
    fn default() -> Self {
        Self {
            schema: STATE_SCHEMA,
            candidates: BTreeMap::new(),
            releases: Vec::new(),
            staging: BTreeMap::new(),
        }
    }
}

impl LifecycleState {
    pub fn load(repo: &Path) -> Result<Self> {
        let path = repo.join(STATE_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()))
            }
        };
        let state: Self = serde_json::from_str(&text)
            .with_context(|| format!("malformed lifecycle metadata in {}", path.display()))?;
        if state.schema != STATE_SCHEMA {
            bail!(
                "unsupported lifecycle schema {} (expected {})",
                state.schema,
                STATE_SCHEMA
            );
        }
        Ok(state)
    }

    pub fn save(&self, repo: &Path) -> Result<()> {
        let path = repo.join(STATE_FILE);
        let parent = path
            .parent()
            .context("lifecycle state path has no parent")?;
        std::fs::create_dir_all(parent)?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    }

    pub fn latest_release(&self, platform: Platform) -> Option<&ReleaseEvent> {
        let branch = platform.production_branch();
        self.releases
            .iter()
            .filter(|release| release.production_branch == branch)
            .max_by(|a, b| a.version.cmp(&b.version))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchSnapshot {
    pub name: String,
    pub role: BranchRole,
    pub commit: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LifecycleStatus {
    pub current_branch: String,
    pub semantic_branch_type: BranchRole,
    pub channel: Option<String>,
    pub version: Option<String>,
    pub current_commit: String,
    pub upstream_branch: Option<String>,
    pub production_version: Option<String>,
    pub production_commit: Option<String>,
    pub development_version: Option<String>,
    pub development_commit: Option<String>,
    pub release_candidate: Option<String>,
    pub changes_pending: bool,
    pub promotion_available: bool,
    pub branches: Vec<BranchSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitReport {
    pub created: Vec<String>,
    pub existing: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Divergence {
    pub source: String,
    pub target: String,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub missing: Vec<String>,
    pub divergences: Vec<Divergence>,
}

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub build_command: Option<String>,
    pub test_command: Option<String>,
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_success(repo: &Path, args: &[&str]) -> Result<bool> {
    Ok(Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?
        .success())
}

fn ref_commit(repo: &Path, reference: &str) -> Result<Option<String>> {
    let full = format!("refs/heads/{reference}");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet", &full])
        .output()?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ))
    } else {
        Ok(None)
    }
}

fn tag_commit(repo: &Path, version: &Version) -> Result<Option<String>> {
    let tag = format!("refs/tags/v{version}");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet", &tag])
        .output()?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ))
    } else {
        Ok(None)
    }
}

fn is_clean(repo: &Path) -> Result<bool> {
    Ok(
        git_output(repo, &["status", "--porcelain", "--untracked-files=all"])?
            .lines()
            .all(|line| {
                line.get(3..)
                    .is_some_and(|path| path == STATE_FILE || path.starts_with(".kaptaind/audit"))
            }),
    )
}

fn version_at(repo: &Path, reference: &str) -> Option<String> {
    git_output(repo, &["show", &format!("{reference}:VERSION")])
        .ok()
        .map(|value| value.trim().to_owned())
}

fn ahead_behind(repo: &Path, source: &str, target: &str) -> Result<(usize, usize)> {
    let counts = git_output(
        repo,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{target}...{source}"),
        ],
    )?;
    let mut parts = counts.split_whitespace();
    let behind = parts.next().unwrap_or("0").parse()?;
    let ahead = parts.next().unwrap_or("0").parse()?;
    Ok((ahead, behind))
}

fn is_ancestor(repo: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    git_success(repo, &["merge-base", "--is-ancestor", ancestor, descendant])
}

pub fn status(repo: &Path, platform: Platform) -> Result<LifecycleStatus> {
    let current_branch = git_output(repo, &["branch", "--show-current"])?;
    if current_branch.is_empty() {
        bail!("detached HEAD has no semantic branch role");
    }
    let current_commit = git_output(repo, &["rev-parse", "HEAD"])?;
    let role = BranchRole::classify(&current_branch);
    let state = LifecycleState::load(repo)?;
    let production = platform.production_branch();
    let development = platform.development_branch();
    let production_commit = ref_commit(repo, production)?;
    let development_commit = ref_commit(repo, development)?;
    let upstream_branch = git_output(repo, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok();
    let dirty = !is_clean(repo)?;
    let unreleased_commits = match (&production_commit, &development_commit) {
        (Some(production), Some(development)) => production != development,
        (None, Some(_)) => true,
        _ => false,
    };
    let changes_pending = dirty || unreleased_commits;
    let promotion_available = !dirty
        && matches!(
            role,
            BranchRole::DesktopDevelopment
                | BranchRole::MobileDevelopment
                | BranchRole::Integration
        )
        && ref_commit(repo, "integration")?.is_some();
    let branches = MANDATORY_BRANCHES
        .iter()
        .map(|name| {
            Ok(BranchSnapshot {
                name: (*name).to_owned(),
                role: BranchRole::classify(name),
                commit: ref_commit(repo, name)?,
                version: version_at(repo, name),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let release_candidate = state
        .candidates
        .values()
        .max_by(|a, b| a.version.cmp(&b.version))
        .map(|candidate| candidate.version.to_string());
    Ok(LifecycleStatus {
        current_branch: current_branch.clone(),
        semantic_branch_type: role,
        channel: if current_branch == development {
            Some("bleeding".into())
        } else if current_branch == production {
            Some("stable".into())
        } else {
            None
        },
        version: version_at(repo, "HEAD"),
        current_commit,
        upstream_branch,
        production_version: state
            .latest_release(platform.clone())
            .map(|release| release.version.to_string()),
        production_commit,
        development_version: version_at(repo, development),
        development_commit,
        release_candidate,
        changes_pending,
        promotion_available,
        branches,
    })
}

pub fn init(repo: &Path, dry_run: bool) -> Result<InitReport> {
    let head = git_output(repo, &["rev-parse", "HEAD"])
        .context("branch init requires at least one commit")?;
    let mut state = LifecycleState::load(repo)?;
    if state.releases.is_empty() {
        let legacy = crate::release::index::load_index(repo);
        if let Some(entry) = legacy
            .releases
            .iter()
            .filter_map(|entry| {
                Version::parse(&entry.version)
                    .ok()
                    .map(|version| (version, entry))
            })
            .max_by(|(left, _), (right, _)| left.cmp(right))
        {
            if tag_commit(repo, &entry.0)?.as_deref() == Some(entry.1.commit.as_str()) {
                for platform in [Platform::Desktop, Platform::Mobile] {
                    state.releases.push(ReleaseEvent {
                        version: entry.0.clone(),
                        source_commit: entry.1.commit.clone(),
                        deployed_commit: entry.1.commit.clone(),
                        candidate_branch: format!("legacy/v{}", entry.0),
                        production_branch: platform.production_branch().into(),
                        issued_at: DateTime::from_timestamp(entry.1.released_at, 0)
                            .unwrap_or_else(Utc::now),
                        actor: "schema-migration".into(),
                        rollback_of: None,
                    });
                }
            }
        }
    }
    for platform in [Platform::Desktop, Platform::Mobile] {
        let branch = platform.production_branch();
        if let Some(actual) = ref_commit(repo, branch)? {
            let expected = state.latest_release(platform).ok_or_else(|| {
                anyhow!(
                    "incompatible production branch `{branch}` exists without an issued release record"
                )
            })?;
            if actual != expected.deployed_commit {
                bail!(
                    "incompatible production branch `{branch}` points to {actual}, expected issued release {} at {}",
                    expected.version,
                    expected.deployed_commit
                );
            }
        }
    }
    let mut report = InitReport {
        created: Vec::new(),
        existing: Vec::new(),
        dry_run,
    };
    for branch in MANDATORY_BRANCHES {
        if ref_commit(repo, branch)?.is_some() {
            report.existing.push((*branch).to_owned());
        } else if matches!(
            BranchRole::classify(branch),
            BranchRole::DesktopProduction | BranchRole::MobileProduction
        ) && state
            .latest_release(if *branch == "desktop/production" {
                Platform::Desktop
            } else {
                Platform::Mobile
            })
            .is_none()
        {
            continue;
        } else {
            report.created.push((*branch).to_owned());
            if !dry_run {
                let commit = match *branch {
                    "desktop/production" => state
                        .latest_release(Platform::Desktop)
                        .map(|release| release.deployed_commit.as_str()),
                    "mobile/production" => state
                        .latest_release(Platform::Mobile)
                        .map(|release| release.deployed_commit.as_str()),
                    _ => None,
                }
                .unwrap_or(&head);
                git_output(repo, &["branch", branch, commit])?;
            }
        }
    }
    if !dry_run {
        state.save(repo)?;
    }
    Ok(report)
}

pub fn sync(repo: &Path) -> Result<SyncReport> {
    let missing = MANDATORY_BRANCHES
        .iter()
        .filter_map(|name| match ref_commit(repo, name) {
            Ok(None) => Some(Ok((*name).to_owned())),
            Ok(Some(_)) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    let mut divergences = Vec::new();
    for (source, target) in [
        ("desktop/development", "integration"),
        ("mobile/development", "integration"),
        ("integration", "local/staging"),
        ("integration", "server/staging"),
        ("desktop/production", "desktop/development"),
        ("mobile/production", "mobile/development"),
    ] {
        if ref_commit(repo, source)?.is_some() && ref_commit(repo, target)?.is_some() {
            let (ahead, behind) = ahead_behind(repo, source, target)?;
            if ahead > 0 && behind > 0 {
                divergences.push(Divergence {
                    source: source.into(),
                    target: target.into(),
                    ahead,
                    behind,
                });
            }
        }
    }
    Ok(SyncReport {
        missing,
        divergences,
    })
}

fn transition_allowed(source: &BranchRole, target: &BranchRole) -> bool {
    matches!(
        (source, target),
        (BranchRole::DesktopDevelopment, BranchRole::Integration)
            | (BranchRole::MobileDevelopment, BranchRole::Integration)
            | (BranchRole::Integration, BranchRole::LocalStaging)
            | (BranchRole::Integration, BranchRole::ServerStaging)
            | (BranchRole::Release(_), BranchRole::LocalStaging)
            | (BranchRole::Release(_), BranchRole::ServerStaging)
    )
}

pub fn promote(repo: &Path, source: &str, target: &str, dry_run: bool) -> Result<()> {
    let source_role = BranchRole::classify(source);
    let target_role = BranchRole::classify(target);
    if source_role == BranchRole::Unmanaged || target_role == BranchRole::Unmanaged {
        bail!("promotion source and target must be Kaptaind-managed branches");
    }
    if target_role.is_production() {
        bail!("production can only advance through `release issue`");
    }
    if !transition_allowed(&source_role, &target_role) {
        bail!("transition from {source} to {target} is not permitted");
    }
    if !is_clean(repo)? {
        bail!("promotion requires a clean working tree");
    }
    let source_commit = ref_commit(repo, source)?
        .ok_or_else(|| anyhow!("source branch `{source}` does not exist"))?;
    let target_commit = ref_commit(repo, target)?
        .ok_or_else(|| anyhow!("target branch `{target}` does not exist"))?;
    if !is_ancestor(repo, &target_commit, &source_commit)? {
        bail!("source `{source}` and target `{target}` have diverged; resolve explicitly before promotion");
    }
    if dry_run || source_commit == target_commit {
        return Ok(());
    }
    let current = git_output(repo, &["branch", "--show-current"])?;
    if current == target {
        bail!("refusing to move checked-out target branch `{target}`; switch away first");
    }
    git_output(
        repo,
        &[
            "update-ref",
            &format!("refs/heads/{target}"),
            &source_commit,
            &target_commit,
        ],
    )?;
    if matches!(
        target_role,
        BranchRole::LocalStaging | BranchRole::ServerStaging
    ) {
        let mut state = LifecycleState::load(repo)?;
        state.staging.insert(
            target.to_owned(),
            StagingRecord {
                source_commit,
                recorded_at: Utc::now(),
            },
        );
        state.save(repo)?;
    }
    Ok(())
}

/// Execute configured validation gates before a branch transition. The CLI
/// always calls this before [`promote`]; it is separate so callers can render
/// the individual machine-readable gate results.
pub fn validate_promotion(repo: &Path, config: &ValidationConfig) -> Result<Vec<ValidationGate>> {
    let mut gates = vec![ValidationGate {
        name: "working-tree-clean".into(),
        passed: is_clean(repo)?,
        detail: "git status --porcelain".into(),
    }];
    if let Some(command) = &config.test_command {
        gates.push(command_gate(repo, "tests", command));
    }
    if let Some(command) = &config.build_command {
        gates.push(command_gate(repo, "build", command));
    }
    if gates.iter().any(|gate| !gate.passed) {
        bail!("branch promotion failed required validation");
    }
    Ok(gates)
}

pub fn prepare_release(
    repo: &Path,
    version: &str,
    source: &str,
    dry_run: bool,
) -> Result<ReleaseCandidate> {
    if !is_clean(repo)? {
        bail!("release preparation requires a clean working tree");
    }
    let version =
        Version::parse(version).context("release version must be valid semantic version")?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        bail!("production release versions must not contain prerelease or build metadata");
    }
    if BranchRole::classify(source) != BranchRole::Integration {
        bail!("release candidates must be prepared from `integration`");
    }
    let mut state = LifecycleState::load(repo)?;
    if state.candidates.contains_key(&version.to_string())
        || state
            .releases
            .iter()
            .any(|release| release.version == version)
    {
        bail!("release {version} already exists");
    }
    if let Some(latest) = state.releases.iter().map(|release| &release.version).max() {
        if version <= *latest {
            bail!("release {version} must be greater than current production version {latest}");
        }
    }
    if tag_commit(repo, &version)?.is_some() {
        bail!("immutable tag v{version} already exists");
    }
    let source_commit = ref_commit(repo, source)?
        .ok_or_else(|| anyhow!("source branch `{source}` does not exist"))?;
    let branch = format!("release/{version}");
    if ref_commit(repo, &branch)?.is_some() {
        bail!("release branch `{branch}` already exists without matching candidate metadata");
    }
    let candidate = ReleaseCandidate {
        version: version.clone(),
        branch: branch.clone(),
        source_branch: source.into(),
        source_commit: source_commit.clone(),
        created_at: Utc::now(),
        validation: None,
    };
    if !dry_run {
        git_output(repo, &["branch", &branch, &source_commit])?;
        state
            .candidates
            .insert(version.to_string(), candidate.clone());
        state.save(repo)?;
    }
    Ok(candidate)
}

fn command_gate(repo: &Path, name: &str, command: &str) -> ValidationGate {
    match Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(repo)
        .output()
    {
        Ok(output) => ValidationGate {
            name: name.into(),
            passed: output.status.success(),
            detail: if output.status.success() {
                "passed".into()
            } else {
                String::from_utf8_lossy(&output.stderr).trim().to_owned()
            },
        },
        Err(error) => ValidationGate {
            name: name.into(),
            passed: false,
            detail: error.to_string(),
        },
    }
}

fn rollback_tree(repo: &Path, source_commit: &str, version: &Version) -> Result<String> {
    let state_dir = repo.join(".kaptaind");
    std::fs::create_dir_all(&state_dir)?;
    let index = state_dir.join(format!("rollback-index-{}", std::process::id()));
    let run = |args: &[&str]| -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .env("GIT_INDEX_FILE", &index)
            .args(args)
            .output()?;
        if !output.status.success() {
            bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let result = (|| {
        run(&["read-tree", &format!("{source_commit}^{{tree}}")])?;
        let mut child = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["hash-object", "-w", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        use std::io::Write as _;
        child
            .stdin
            .as_mut()
            .context("git hash-object stdin unavailable")?
            .write_all(format!("{version}\n").as_bytes())?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!("failed to create rollback VERSION blob");
        }
        let blob = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        run(&[
            "update-index",
            "--add",
            "--cacheinfo",
            "100644",
            &blob,
            "VERSION",
        ])?;
        run(&["write-tree"])
    })();
    if let Err(error) = std::fs::remove_file(&index) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %index.display(), error = %error, "failed to remove rollback temporary index");
        }
    }
    result
}

pub fn validate_release(
    repo: &Path,
    version: &str,
    config: &ValidationConfig,
) -> Result<ValidationRecord> {
    let version = Version::parse(version)?;
    let mut state = LifecycleState::load(repo)?;
    let candidate = state
        .candidates
        .get_mut(&version.to_string())
        .ok_or_else(|| anyhow!("release candidate {version} does not exist"))?;
    let branch_commit = ref_commit(repo, &candidate.branch)?
        .ok_or_else(|| anyhow!("candidate branch `{}` is missing", candidate.branch))?;
    let expected_version = version.to_string();
    let mut gates = vec![
        ValidationGate {
            name: "candidate-identity".into(),
            passed: branch_commit == candidate.source_commit,
            detail: branch_commit.clone(),
        },
        ValidationGate {
            name: "working-tree-clean".into(),
            passed: is_clean(repo)?,
            detail: "git status --porcelain".into(),
        },
        ValidationGate {
            name: "version-consistency".into(),
            passed: version_at(repo, &candidate.branch).as_deref()
                == Some(expected_version.as_str()),
            detail: format!("{}:VERSION", candidate.branch),
        },
        ValidationGate {
            name: "branch-consistency".into(),
            passed: BranchRole::classify(&candidate.branch) == BranchRole::Release(version.clone()),
            detail: candidate.branch.clone(),
        },
    ];
    if let Some(command) = &config.test_command {
        gates.push(command_gate(repo, "tests", command));
    }
    if let Some(command) = &config.build_command {
        gates.push(command_gate(repo, "build", command));
    }
    let record = ValidationRecord {
        commit: branch_commit,
        validated_at: Utc::now(),
        gates,
    };
    candidate.validation = Some(record.clone());
    state.save(repo)?;
    if !record.passed() {
        bail!("release {version} failed validation");
    }
    Ok(record)
}

pub fn issue_release(
    repo: &Path,
    version: &str,
    platform: Platform,
    actor: &str,
    dry_run: bool,
) -> Result<ReleaseEvent> {
    if !is_clean(repo)? {
        bail!("release issuance requires a clean working tree");
    }
    let version = Version::parse(version)?;
    let mut state = LifecycleState::load(repo)?;
    if state.releases.iter().any(|release| {
        release.version == version && release.production_branch == platform.production_branch()
    }) {
        bail!(
            "release {version} has already been issued for {:?}",
            platform
        );
    }
    let candidate = state
        .candidates
        .get(&version.to_string())
        .ok_or_else(|| anyhow!("release candidate {version} does not exist"))?
        .clone();
    let validation = candidate
        .validation
        .as_ref()
        .filter(|validation| validation.passed())
        .ok_or_else(|| anyhow!("release candidate {version} has not passed validation"))?;
    let candidate_commit = ref_commit(repo, &candidate.branch)?
        .ok_or_else(|| anyhow!("candidate branch `{}` is missing", candidate.branch))?;
    if candidate_commit != candidate.source_commit || validation.commit != candidate_commit {
        bail!("release candidate {version} changed since preparation or validation");
    }
    let existing_tag = tag_commit(repo, &version)?;
    if existing_tag
        .as_deref()
        .is_some_and(|commit| commit != candidate_commit)
    {
        bail!("immutable tag v{version} already exists at a different commit");
    }
    let production_branch = platform.production_branch();
    if git_output(repo, &["branch", "--show-current"])? == production_branch {
        bail!("refusing to advance checked-out production branch `{production_branch}`; switch away first");
    }
    let old_production = ref_commit(repo, production_branch)?;
    if let Some(old) = &old_production {
        if !is_ancestor(repo, old, &candidate_commit)? {
            bail!("candidate and production have diverged; production promotion blocked");
        }
    }
    let event = ReleaseEvent {
        version: version.clone(),
        source_commit: candidate.source_commit.clone(),
        deployed_commit: candidate_commit.clone(),
        candidate_branch: candidate.branch.clone(),
        production_branch: production_branch.into(),
        issued_at: Utc::now(),
        actor: actor.into(),
        rollback_of: None,
    };
    if dry_run {
        return Ok(event);
    }
    let tag_ref = format!("refs/tags/v{version}");
    let production_ref = format!("refs/heads/{production_branch}");
    let mut transaction = String::from("start\n");
    if existing_tag.is_none() {
        transaction.push_str(&format!("create {tag_ref} {candidate_commit}\n"));
    }
    match old_production {
        Some(old) => transaction.push_str(&format!(
            "update {production_ref} {candidate_commit} {old}\n"
        )),
        None => transaction.push_str(&format!("create {production_ref} {candidate_commit}\n")),
    }
    transaction.push_str("prepare\ncommit\n");
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["update-ref", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .context("git update-ref stdin unavailable")?
        .write_all(transaction.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("atomic production/tag reference transaction failed");
    }
    state.releases.push(event.clone());
    state.save(repo)?;
    crate::audit::log_event(
        repo,
        actor,
        "lifecycle.release-issued",
        true,
        serde_json::to_value(&event)?,
    );
    Ok(event)
}

pub fn rollback(
    repo: &Path,
    target: &str,
    new_version: &str,
    platform: Platform,
    actor: &str,
    dry_run: bool,
) -> Result<ReleaseEvent> {
    if !is_clean(repo)? {
        bail!("release rollback requires a clean working tree");
    }
    let target = Version::parse(target)?;
    let new_version = Version::parse(new_version)?;
    let mut state = LifecycleState::load(repo)?;
    let target_release = state
        .releases
        .iter()
        .find(|release| {
            release.version == target && release.production_branch == platform.production_branch()
        })
        .cloned()
        .ok_or_else(|| anyhow!("released target {target} does not exist"))?;
    let latest = state
        .latest_release(platform.clone())
        .ok_or_else(|| anyhow!("no production release exists"))?;
    if new_version <= latest.version {
        bail!(
            "rollback release {new_version} must be newer than current production {}",
            latest.version
        );
    }
    if tag_commit(repo, &new_version)?.is_some() {
        bail!("immutable tag v{new_version} already exists");
    }
    let production_branch = platform.production_branch();
    if git_output(repo, &["branch", "--show-current"])? == production_branch {
        bail!("refusing to advance checked-out production branch `{production_branch}`; switch away first");
    }
    let production_commit = ref_commit(repo, production_branch)?
        .ok_or_else(|| anyhow!("production branch `{production_branch}` is missing"))?;
    let message = format!("kaptaind: release {new_version} rollback to {target}");
    let deployed_commit = if dry_run {
        production_commit.clone()
    } else {
        let tree = rollback_tree(repo, &target_release.deployed_commit, &new_version)?;
        git_output(
            repo,
            &[
                "commit-tree",
                &tree,
                "-p",
                &production_commit,
                "-m",
                &message,
            ],
        )?
    };
    let event = ReleaseEvent {
        version: new_version.clone(),
        source_commit: target_release.source_commit,
        deployed_commit: deployed_commit.clone(),
        candidate_branch: format!("rollback/{target}"),
        production_branch: production_branch.into(),
        issued_at: Utc::now(),
        actor: actor.into(),
        rollback_of: Some(target),
    };
    if dry_run {
        return Ok(event);
    }
    let tag_ref = format!("refs/tags/v{new_version}");
    let production_ref = format!("refs/heads/{production_branch}");
    let transaction = format!(
        "start\ncreate {tag_ref} {deployed_commit}\nupdate {production_ref} {deployed_commit} {production_commit}\nprepare\ncommit\n"
    );
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["update-ref", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    use std::io::Write as _;
    child
        .stdin
        .as_mut()
        .context("git update-ref stdin unavailable")?
        .write_all(transaction.as_bytes())?;
    if !child.wait()?.success() {
        bail!("atomic rollback production/tag reference transaction failed");
    }
    state.releases.push(event.clone());
    state.save(repo)?;
    crate::audit::log_event(
        repo,
        actor,
        "lifecycle.release-rollback",
        true,
        serde_json::to_value(&event)?,
    );
    Ok(event)
}

pub fn checkout_channel(
    repo: &Path,
    channel: &str,
    platform: Platform,
    dry_run: bool,
) -> Result<String> {
    if !is_clean(repo)? {
        bail!("checkout requires a clean working tree");
    }
    let branch = match channel {
        "stable" => platform.production_branch(),
        "bleeding" => platform.development_branch(),
        _ => bail!("unknown channel `{channel}` (expected stable or bleeding)"),
    };
    if ref_commit(repo, branch)?.is_none() {
        bail!("channel `{channel}` cannot resolve because `{branch}` is missing");
    }
    if channel == "stable" {
        let state = LifecycleState::load(repo)?;
        let release = state
            .latest_release(platform)
            .ok_or_else(|| anyhow!("stable has no issued production release"))?;
        let commit = ref_commit(repo, branch)?.unwrap_or_default();
        if commit != release.deployed_commit {
            bail!("stable production ref does not match its latest issued release");
        }
    }
    if !dry_run {
        git_output(repo, &["switch", branch])?;
    }
    Ok(branch.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        for args in [
            vec!["init"],
            vec!["config", "user.name", "Kaptaind Test"],
            vec!["config", "user.email", "test@example.com"],
        ] {
            git_output(dir.path(), &args).unwrap();
        }
        std::fs::write(dir.path().join("VERSION"), "1.0.0").unwrap();
        git_output(dir.path(), &["add", "VERSION"]).unwrap();
        git_output(dir.path(), &["commit", "-m", "initial"]).unwrap();
        dir
    }

    #[test]
    fn classifies_every_mandatory_role() {
        assert_eq!(
            BranchRole::classify("desktop/development"),
            BranchRole::DesktopDevelopment
        );
        assert!(matches!(
            BranchRole::classify("release/1.2.3"),
            BranchRole::Release(_)
        ));
        assert_eq!(BranchRole::classify("release/nope"), BranchRole::Unmanaged);
    }

    #[test]
    fn init_is_idempotent() {
        let dir = repo();
        let first = init(dir.path(), false).unwrap();
        assert_eq!(first.created.len(), MANDATORY_BRANCHES.len() - 2);
        let second = init(dir.path(), false).unwrap();
        assert!(second.created.is_empty());
        assert_eq!(second.existing.len(), MANDATORY_BRANCHES.len() - 2);
    }

    #[test]
    fn init_rejects_unreleased_production_ref() {
        let dir = repo();
        git_output(dir.path(), &["branch", "desktop/production", "HEAD"]).unwrap();
        let error = init(dir.path(), false).unwrap_err();
        assert!(error
            .to_string()
            .contains("without an issued release record"));
        assert!(ref_commit(dir.path(), "integration").unwrap().is_none());
    }

    #[test]
    fn production_cannot_be_generically_promoted() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        let error = promote(dir.path(), "integration", "desktop/production", false).unwrap_err();
        assert!(error.to_string().contains("release issue"));
    }

    #[test]
    fn stable_requires_an_issued_release() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        let error = checkout_channel(dir.path(), "stable", Platform::Desktop, true).unwrap_err();
        assert!(
            error.to_string().contains("desktop/production")
                || error.to_string().contains("no issued production release")
        );
    }

    #[test]
    fn prepare_validate_and_issue_are_explicit() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        prepare_release(dir.path(), "1.0.0", "integration", false).unwrap();
        validate_release(
            dir.path(),
            "1.0.0",
            &ValidationConfig {
                build_command: None,
                test_command: None,
            },
        )
        .unwrap();
        let event = issue_release(dir.path(), "1.0.0", Platform::Desktop, "test", false).unwrap();
        assert_eq!(
            ref_commit(dir.path(), "desktop/production")
                .unwrap()
                .as_deref(),
            Some(event.deployed_commit.as_str())
        );
        assert_eq!(
            tag_commit(dir.path(), &Version::parse("1.0.0").unwrap())
                .unwrap()
                .as_deref(),
            Some(event.deployed_commit.as_str())
        );
        assert_eq!(
            checkout_channel(dir.path(), "stable", Platform::Desktop, true).unwrap(),
            "desktop/production"
        );
    }

    #[test]
    fn changed_candidate_cannot_be_issued() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        prepare_release(dir.path(), "1.0.0", "integration", false).unwrap();
        validate_release(
            dir.path(),
            "1.0.0",
            &ValidationConfig {
                build_command: None,
                test_command: None,
            },
        )
        .unwrap();
        let head = git_output(dir.path(), &["rev-parse", "HEAD"]).unwrap();
        git_output(
            dir.path(),
            &[
                "commit-tree",
                &git_output(dir.path(), &["rev-parse", "HEAD^{tree}"]).unwrap(),
                "-p",
                &head,
                "-m",
                "mutation",
            ],
        )
        .map(|new| {
            git_output(
                dir.path(),
                &["update-ref", "refs/heads/release/1.0.0", &new],
            )
            .unwrap()
        })
        .unwrap();
        assert!(issue_release(dir.path(), "1.0.0", Platform::Desktop, "test", false).is_err());
    }

    #[test]
    fn failed_gate_and_dirty_tree_block_progress() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        prepare_release(dir.path(), "1.0.0", "integration", false).unwrap();
        assert!(validate_release(
            dir.path(),
            "1.0.0",
            &ValidationConfig {
                build_command: None,
                test_command: Some("exit 7".into()),
            }
        )
        .is_err());
        assert!(issue_release(dir.path(), "1.0.0", Platform::Desktop, "test", false).is_err());

        std::fs::write(dir.path().join("dirty.txt"), "dirty").unwrap();
        assert!(promote(dir.path(), "integration", "local/staging", false)
            .unwrap_err()
            .to_string()
            .contains("clean working tree"));
    }

    #[test]
    fn divergent_promotion_is_never_resolved_automatically() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        let head = git_output(dir.path(), &["rev-parse", "HEAD"]).unwrap();
        let tree = git_output(dir.path(), &["rev-parse", "HEAD^{tree}"]).unwrap();
        let left = git_output(
            dir.path(),
            &["commit-tree", &tree, "-p", &head, "-m", "left"],
        )
        .unwrap();
        let right = git_output(
            dir.path(),
            &["commit-tree", &tree, "-p", &head, "-m", "right"],
        )
        .unwrap();
        git_output(
            dir.path(),
            &["update-ref", "refs/heads/integration", &left, &head],
        )
        .unwrap();
        git_output(
            dir.path(),
            &["update-ref", "refs/heads/local/staging", &right, &head],
        )
        .unwrap();
        let error = promote(dir.path(), "integration", "local/staging", false).unwrap_err();
        assert!(error.to_string().contains("diverged"));
        assert_eq!(
            ref_commit(dir.path(), "local/staging").unwrap().as_deref(),
            Some(right.as_str())
        );
    }

    #[test]
    fn one_candidate_can_issue_same_tag_to_both_platforms() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        prepare_release(dir.path(), "1.0.0", "integration", false).unwrap();
        validate_release(
            dir.path(),
            "1.0.0",
            &ValidationConfig {
                build_command: None,
                test_command: None,
            },
        )
        .unwrap();
        issue_release(dir.path(), "1.0.0", Platform::Desktop, "test", false).unwrap();
        issue_release(dir.path(), "1.0.0", Platform::Mobile, "test", false).unwrap();
        assert_eq!(
            ref_commit(dir.path(), "desktop/production").unwrap(),
            ref_commit(dir.path(), "mobile/production").unwrap()
        );
    }

    #[test]
    fn rollback_adds_history_instead_of_resetting_production() {
        let dir = repo();
        init(dir.path(), false).unwrap();
        prepare_release(dir.path(), "1.0.0", "integration", false).unwrap();
        validate_release(
            dir.path(),
            "1.0.0",
            &ValidationConfig {
                build_command: None,
                test_command: None,
            },
        )
        .unwrap();
        let first = issue_release(dir.path(), "1.0.0", Platform::Desktop, "test", false).unwrap();
        let rollback = rollback(
            dir.path(),
            "1.0.0",
            "1.0.1",
            Platform::Desktop,
            "test",
            false,
        )
        .unwrap();
        assert_ne!(rollback.deployed_commit, first.deployed_commit);
        assert!(is_ancestor(
            dir.path(),
            &first.deployed_commit,
            &rollback.deployed_commit
        )
        .unwrap());
        assert_eq!(version_at(dir.path(), "v1.0.1").as_deref(), Some("1.0.1"));
    }
}
