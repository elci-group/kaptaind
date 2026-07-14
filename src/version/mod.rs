use std::path::Path;

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

/// Resolve the project's baseline version without ever guessing.
///
/// Precedence: `VERSION` file, then `Cargo.toml` `[package].version`.
/// A present-but-unparseable source — or the absence of both — is an
/// error: silently falling back to `0.1.0` can fabricate a downgrade
/// against the real manifest version and desync the VERSION/manifest
/// pair.
pub fn resolve_baseline(repo_path: &Path) -> anyhow::Result<semver::Version> {
    let version_path = repo_path.join("VERSION");
    if version_path.exists() {
        let content = std::fs::read_to_string(&version_path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", version_path.display()))?;
        return semver::Version::parse(content.trim()).map_err(|e| {
            anyhow::anyhow!(
                "{} does not contain a valid semver version: {e}",
                version_path.display()
            )
        });
    }

    let cargo_path = repo_path.join("Cargo.toml");
    if cargo_path.exists() {
        let content = std::fs::read_to_string(&cargo_path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", cargo_path.display()))?;
        let doc = content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", cargo_path.display()))?;
        if let Some(raw) = doc
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
        {
            return semver::Version::parse(raw).map_err(|e| {
                anyhow::anyhow!(
                    "{} [package].version is not valid semver: {e}",
                    cargo_path.display()
                )
            });
        }
    }

    anyhow::bail!(
        "no VERSION file and no Cargo.toml [package].version in {} — refusing to guess a baseline",
        repo_path.display()
    );
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
}
