//! First-run version enforcement (`[versioning].bootstrap`).
//!
//! A repository with no `VERSION` file and no root `Cargo.toml
//! [package].version` has no baseline, so every cluster would fail with
//! `baseline_unresolvable`. This module runs once at daemon startup and
//! either establishes a baseline (`initialize`) or refuses to start
//! (`refuse`), so the gap surfaces on the operator's terminal instead of in
//! per-cluster decisions.

use std::path::Path;

use crate::config::loader::{OperationMode, VersionBootstrap, VersioningConfig, WorkspacePolicy};
use crate::version::workspace::WorkspaceLayout;

/// Where a bootstrap seed came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedSource {
    /// Highest semver git tag (`vX.Y.Z` / `X.Y.Z`), carrying the tag name.
    GitTag(String),
    /// `[versioning].initial_version`.
    InitialVersion,
}

impl std::fmt::Display for SeedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedSource::GitTag(tag) => write!(f, "git tag {tag}"),
            SeedSource::InitialVersion => write!(f, "[versioning].initial_version"),
        }
    }
}

/// Result of [`ensure_baseline`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapOutcome {
    /// A baseline already resolves; nothing to do.
    Existing(semver::Version),
    /// `VERSION` was written with the seed.
    Initialized {
        version: semver::Version,
        source: SeedSource,
    },
    /// Observe mode: the seed that actuation would write; nothing written.
    WouldInitialize {
        version: semver::Version,
        source: SeedSource,
    },
    /// Virtual workspace under a member-scoped policy: there is no root
    /// version to own; member baselines resolve during writeback.
    NotApplicable,
}

/// The highest root-shaped semver tag (`vX.Y.Z` or `X.Y.Z`). Member tags
/// such as `kaptaind-diff-v1.0.0` never parse and are ignored. `None` when
/// git is unavailable, the path is not a repository, or no tag parses.
pub fn highest_semver_tag(repo_path: &Path) -> Option<(semver::Version, String)> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["tag", "--list"])
        .output()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|tag| {
            let tag = tag.trim();
            let raw = tag.strip_prefix('v').unwrap_or(tag);
            // traci: allow -- non-semver tags are skipped by design.
            semver::Version::parse(raw)
                .ok()
                .map(|v| (v, tag.to_string()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
}

/// The baseline `initialize` would establish: highest semver tag, else
/// `[versioning].initial_version`. Seeding from tags avoids fabricating a
/// downgrade against already-released history.
pub fn seed(
    repo_path: &Path,
    versioning: &VersioningConfig,
) -> anyhow::Result<(semver::Version, SeedSource)> {
    if let Some((version, tag)) = highest_semver_tag(repo_path) {
        return Ok((version, SeedSource::GitTag(tag)));
    }
    let version = semver::Version::parse(&versioning.initial_version).map_err(|e| {
        anyhow::anyhow!(
            "[versioning].initial_version = {:?} is not valid semver: {e}",
            versioning.initial_version
        )
    })?;
    Ok((version, SeedSource::InitialVersion))
}

/// `true` when neither `VERSION` nor root `[package].version` is present.
/// Present-but-invalid sources are *not* missing — they are errors that
/// `resolve_baseline` reports and bootstrap must never paper over.
pub fn baseline_missing(repo_path: &Path) -> bool {
    super::read_version_file(repo_path).is_none()
        && super::read_manifest_version(repo_path).is_none()
}

/// Enforce a version baseline before the daemon starts watching.
///
/// - A resolvable baseline → `Existing`.
/// - An unreadable/unparseable `VERSION` or manifest version → error.
/// - No baseline and `bootstrap = "refuse"` → error with remediation.
/// - No baseline and `bootstrap = "initialize"` → write `VERSION` from
///   [`seed`] (actuate) or report the seed only (observe).
pub fn ensure_baseline(
    repo_path: &Path,
    versioning: &VersioningConfig,
    mode: OperationMode,
) -> anyhow::Result<BootstrapOutcome> {
    if !baseline_missing(repo_path) {
        return super::resolve_baseline(repo_path).map(BootstrapOutcome::Existing);
    }

    if !matches!(versioning.workspace, WorkspacePolicy::RootOnly)
        && matches!(
            WorkspaceLayout::discover(repo_path),
            Ok(WorkspaceLayout::Virtual { .. })
        )
    {
        return Ok(BootstrapOutcome::NotApplicable);
    }

    if matches!(versioning.bootstrap, VersionBootstrap::Refuse) {
        anyhow::bail!(
            "no version baseline in {}: no VERSION file and no Cargo.toml [package].version. \
             [versioning].bootstrap = \"refuse\" — create VERSION (e.g. `echo 0.1.0 > VERSION`) \
             and commit it, or set bootstrap = \"initialize\"",
            repo_path.display()
        );
    }

    let (version, source) = seed(repo_path, versioning)?;
    if matches!(mode, OperationMode::Observe) {
        return Ok(BootstrapOutcome::WouldInitialize { version, source });
    }

    let path = repo_path.join("VERSION");
    std::fs::write(&path, format!("{version}\n"))
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", path.display()))?;
    Ok(BootstrapOutcome::Initialized { version, source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    }

    fn repo_with_tags(tags: &[&str]) -> tempfile::TempDir {
        let dir = tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "init"]);
        for tag in tags {
            git(dir.path(), &["tag", tag]);
        }
        dir
    }

    fn versioning(bootstrap: VersionBootstrap) -> VersioningConfig {
        VersioningConfig {
            bootstrap,
            ..VersioningConfig::default()
        }
    }

    #[test]
    fn existing_version_file_is_untouched() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("VERSION"), "2.3.4\n").expect("VERSION");
        let outcome = ensure_baseline(
            dir.path(),
            &versioning(VersionBootstrap::Refuse),
            OperationMode::Actuate,
        )
        .expect("existing baseline");
        assert_eq!(
            outcome,
            BootstrapOutcome::Existing(semver::Version::new(2, 3, 4))
        );
    }

    #[test]
    fn invalid_version_file_is_an_error_not_a_bootstrap() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("VERSION"), "garbage\n").expect("VERSION");
        assert!(ensure_baseline(
            dir.path(),
            &versioning(VersionBootstrap::Initialize),
            OperationMode::Actuate,
        )
        .is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("VERSION")).unwrap(),
            "garbage\n"
        );
    }

    #[test]
    fn refuse_policy_errors_when_missing() {
        let dir = tempdir().expect("tempdir");
        let err = ensure_baseline(
            dir.path(),
            &versioning(VersionBootstrap::Refuse),
            OperationMode::Actuate,
        )
        .expect_err("must refuse");
        assert!(err.to_string().contains("bootstrap = \"refuse\""), "{err}");
        assert!(!dir.path().join("VERSION").exists());
    }

    #[test]
    fn initialize_writes_initial_version_without_tags() {
        let dir = tempdir().expect("tempdir");
        let cfg = VersioningConfig {
            initial_version: "1.0.0".to_string(),
            ..VersioningConfig::default()
        };
        let outcome =
            ensure_baseline(dir.path(), &cfg, OperationMode::Actuate).expect("initialize");
        assert_eq!(
            outcome,
            BootstrapOutcome::Initialized {
                version: semver::Version::new(1, 0, 0),
                source: SeedSource::InitialVersion,
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("VERSION")).unwrap(),
            "1.0.0\n"
        );
        assert_eq!(
            crate::version::resolve_baseline(dir.path()).unwrap(),
            semver::Version::new(1, 0, 0)
        );
    }

    #[test]
    fn initialize_seeds_from_highest_root_tag() {
        let dir = repo_with_tags(&[
            "v1.2.0",
            "v1.10.0",
            "1.3.0",
            "kaptaind-diff-v9.0.0",
            "nightly",
        ]);
        let outcome = ensure_baseline(
            dir.path(),
            &versioning(VersionBootstrap::Initialize),
            OperationMode::Actuate,
        )
        .expect("initialize");
        assert_eq!(
            outcome,
            BootstrapOutcome::Initialized {
                version: semver::Version::new(1, 10, 0),
                source: SeedSource::GitTag("v1.10.0".to_string()),
            }
        );
    }

    #[test]
    fn observe_mode_reports_without_writing() {
        let dir = tempdir().expect("tempdir");
        let outcome = ensure_baseline(
            dir.path(),
            &versioning(VersionBootstrap::Initialize),
            OperationMode::Observe,
        )
        .expect("observe");
        assert_eq!(
            outcome,
            BootstrapOutcome::WouldInitialize {
                version: semver::Version::new(0, 1, 0),
                source: SeedSource::InitialVersion,
            }
        );
        assert!(!dir.path().join("VERSION").exists());
    }

    #[test]
    fn manifest_version_counts_as_baseline() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.4.0\"\n",
        )
        .expect("manifest");
        let outcome = ensure_baseline(
            dir.path(),
            &versioning(VersionBootstrap::Refuse),
            OperationMode::Actuate,
        )
        .expect("manifest baseline");
        assert_eq!(
            outcome,
            BootstrapOutcome::Existing(semver::Version::new(0, 4, 0))
        );
        assert!(!dir.path().join("VERSION").exists());
    }
}
