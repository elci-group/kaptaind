use std::path::Path;

pub mod workspace;
pub mod writeback;

pub use kaptaind_diff::version::{apply, Bump};
/// Decide the version bump using configurable score thresholds.
pub fn decide(
    weight: &crate::weight::WeightResult,
    thresholds: &crate::config::loader::VersionThresholdConfig,
) -> Bump {
    kaptaind_diff::version::decide(
        weight.score,
        weight.api_breaking,
        weight.api_added,
        thresholds.minor,
        thresholds.patch,
    )
}

/// Convenience wrapper using legacy hardcoded thresholds (0.6 / 0.1).
pub fn decide_default(weight: &crate::weight::WeightResult) -> Bump {
    decide(
        weight,
        &crate::config::loader::VersionThresholdConfig::default(),
    )
}

/// Read and parse the `VERSION` file under `repo_path`.
/// `None` when absent; `Some(Err(_))` when present but unreadable/unparseable.
fn read_version_file(repo_path: &Path) -> Option<anyhow::Result<semver::Version>> {
    let version_path = repo_path.join("VERSION");
    if !version_path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(&version_path) {
        Ok(content) => content,
        Err(e) => {
            return Some(Err(anyhow::anyhow!(
                "failed to read {}: {e}",
                version_path.display()
            )))
        }
    };
    Some(semver::Version::parse(content.trim()).map_err(|e| {
        anyhow::anyhow!(
            "{} does not contain a valid semver version: {e}",
            version_path.display()
        )
    }))
}

/// Read and parse `[package].version` from the root `Cargo.toml` under
/// `repo_path`. `None` when absent or when the manifest has no
/// `[package].version` (e.g. a virtual workspace root); `Some(Err(_))` when
/// present but unreadable/unparseable.
fn read_manifest_version(repo_path: &Path) -> Option<anyhow::Result<semver::Version>> {
    let cargo_path = repo_path.join("Cargo.toml");
    if !cargo_path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(&cargo_path) {
        Ok(content) => content,
        Err(e) => {
            return Some(Err(anyhow::anyhow!(
                "failed to read {}: {e}",
                cargo_path.display()
            )))
        }
    };
    let doc = match content.parse::<toml_edit::DocumentMut>() {
        Ok(doc) => doc,
        Err(e) => {
            return Some(Err(anyhow::anyhow!(
                "failed to parse {}: {e}",
                cargo_path.display()
            )))
        }
    };
    let raw = doc
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())?;
    Some(semver::Version::parse(raw).map_err(|e| {
        anyhow::anyhow!(
            "{} [package].version is not valid semver: {e}",
            cargo_path.display()
        )
    }))
}

/// Resolve the project's baseline version without ever guessing.
///
/// Precedence: `VERSION` file, then `Cargo.toml` `[package].version`.
/// A present-but-unparseable source — or the absence of both — is an
/// error: silently falling back to `0.1.0` can fabricate a downgrade
/// against the real manifest version and desync the VERSION/manifest
/// pair.
pub fn resolve_baseline(repo_path: &Path) -> anyhow::Result<semver::Version> {
    if let Some(version) = read_version_file(repo_path) {
        return version;
    }
    if let Some(version) = read_manifest_version(repo_path) {
        return version;
    }
    anyhow::bail!(
        "no VERSION file and no Cargo.toml [package].version in {} — refusing to guess a baseline",
        repo_path.display()
    );
}

/// Enforce the `[versioning].consistency` policy: when both `VERSION` and
/// root `Cargo.toml [package].version` exist, they must agree.
///
/// Precedence alone (VERSION wins) would silently ignore manual manifest
/// edits; surfacing the disagreement immediately is safer. Only the strict
/// policy errors — `warn` logs and proceeds, `off` is a no-op. When one or
/// both sources are absent, unreadable, or unparseable there is nothing to
/// compare (parse errors are reported by `resolve_baseline` instead).
pub fn check_consistency(
    repo_path: &Path,
    policy: crate::config::loader::VersionConsistency,
) -> anyhow::Result<()> {
    use crate::config::loader::VersionConsistency;

    if matches!(policy, VersionConsistency::Off) {
        return Ok(());
    }
    let (Some(Ok(version_file)), Some(Ok(manifest))) = (
        read_version_file(repo_path),
        read_manifest_version(repo_path),
    ) else {
        return Ok(());
    };
    if version_file == manifest {
        return Ok(());
    }
    let detail = format!(
        "VERSION ({version_file}) and Cargo.toml [package].version ({manifest}) disagree — \
         reconcile them manually, or set [versioning].consistency = \"warn\" or \"off\""
    );
    match policy {
        VersionConsistency::Strict => anyhow::bail!(detail),
        VersionConsistency::Warn => {
            tracing::warn!(%version_file, %manifest, "version sources disagree; using VERSION");
            Ok(())
        }
        VersionConsistency::Off => Ok(()), // unreachable: returned early above
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn baseline_prefers_version_file_over_manifest() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("VERSION"), "1.2.3\n").expect("VERSION");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"9.9.9\"\n",
        )
        .expect("Cargo.toml");
        assert_eq!(
            resolve_baseline(dir.path()).expect("baseline"),
            semver::Version::new(1, 2, 3)
        );
    }

    #[test]
    fn baseline_falls_back_to_cargo_toml() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"2.0.1\"\n",
        )
        .expect("Cargo.toml");
        assert_eq!(
            resolve_baseline(dir.path()).expect("baseline"),
            semver::Version::new(2, 0, 1)
        );
    }

    #[test]
    fn baseline_errors_when_neither_source_exists() {
        let dir = tempdir().expect("tempdir");
        let err = resolve_baseline(dir.path()).expect_err("must not guess");
        assert!(err.to_string().contains("refusing to guess"));
    }

    #[test]
    fn baseline_errors_on_unparseable_version_file() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("VERSION"), "not-a-version\n").expect("VERSION");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"2.0.1\"\n",
        )
        .expect("Cargo.toml");
        let err = resolve_baseline(dir.path()).expect_err("must not fall through");
        assert!(err.to_string().contains("valid semver"));
    }

    #[test]
    fn baseline_errors_on_unparseable_manifest_version() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"garbage\"\n",
        )
        .expect("Cargo.toml");
        let err = resolve_baseline(dir.path()).expect_err("must not guess");
        assert!(err.to_string().contains("not valid semver"));
    }

    fn write_pair(dir: &Path, version: &str, manifest: &str) {
        std::fs::write(dir.join("VERSION"), version).expect("VERSION");
        std::fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"x\"\nversion = \"{manifest}\"\n"),
        )
        .expect("Cargo.toml");
    }

    #[test]
    fn consistency_passes_when_sources_agree() {
        let dir = tempdir().expect("tempdir");
        write_pair(dir.path(), "1.2.3\n", "1.2.3");
        for policy in [
            crate::config::loader::VersionConsistency::Strict,
            crate::config::loader::VersionConsistency::Warn,
            crate::config::loader::VersionConsistency::Off,
        ] {
            check_consistency(dir.path(), policy).expect("agreement must pass");
        }
    }

    #[test]
    fn consistency_strict_errors_on_mismatch() {
        let dir = tempdir().expect("tempdir");
        write_pair(dir.path(), "2.4.0", "2.3.0");
        let err = check_consistency(
            dir.path(),
            crate::config::loader::VersionConsistency::Strict,
        )
        .expect_err("strict must refuse a mismatch");
        let msg = err.to_string();
        assert!(msg.contains("2.4.0"));
        assert!(msg.contains("2.3.0"));
        assert!(msg.contains("consistency"));
    }

    #[test]
    fn consistency_warn_and_off_allow_mismatch() {
        let dir = tempdir().expect("tempdir");
        write_pair(dir.path(), "2.4.0", "2.3.0");
        check_consistency(dir.path(), crate::config::loader::VersionConsistency::Warn)
            .expect("warn must pass");
        check_consistency(dir.path(), crate::config::loader::VersionConsistency::Off)
            .expect("off must pass");
    }

    #[test]
    fn consistency_passes_with_single_or_no_source() {
        use crate::config::loader::VersionConsistency;
        let version_only = tempdir().expect("tempdir");
        std::fs::write(version_only.path().join("VERSION"), "1.0.0").expect("VERSION");
        check_consistency(version_only.path(), VersionConsistency::Strict)
            .expect("VERSION-only must pass");

        let manifest_only = tempdir().expect("tempdir");
        std::fs::write(
            manifest_only.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"1.0.0\"\n",
        )
        .expect("Cargo.toml");
        check_consistency(manifest_only.path(), VersionConsistency::Strict)
            .expect("manifest-only must pass");

        // Virtual workspace root: manifest parses but has no [package].version.
        let virtual_ws = tempdir().expect("tempdir");
        std::fs::write(virtual_ws.path().join("VERSION"), "1.0.0").expect("VERSION");
        std::fs::write(
            virtual_ws.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .expect("Cargo.toml");
        check_consistency(virtual_ws.path(), VersionConsistency::Strict)
            .expect("virtual workspace must pass");

        let neither = tempdir().expect("tempdir");
        check_consistency(neither.path(), VersionConsistency::Strict)
            .expect("no sources must pass");
    }
}
