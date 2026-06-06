use crate::config::loader::BuildConfig;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Package manifest written alongside every release artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub version: String,
    pub commit: String,
    pub timestamp: i64,
    pub stability: f64,
    pub artifacts: Vec<String>,
    pub checksum: String,
}

pub struct PackageResult {
    pub tarball: PathBuf,
    pub manifest: PackageManifest,
}

/// Create a `.tar.gz` of the build artifact and write `manifest.json` into
/// the release output directory.
///
/// Returns the path to the tarball and the manifest on success.
pub fn create(
    version: &str,
    commit: &str,
    stability: f64,
    build_config: &BuildConfig,
    releases_dir: &Path,
) -> anyhow::Result<PackageResult> {
    std::fs::create_dir_all(releases_dir)?;

    let artifact_path = Path::new(&build_config.artifact_path);
    if !artifact_path.exists() {
        anyhow::bail!(
            "build artifact not found at '{}'; ensure [build] command produces it",
            artifact_path.display()
        );
    }

    // Determine artifact filename
    let artifact_name = artifact_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("artifact");

    let tarball_name = format!("{}-{}.tar.gz", artifact_name, version);
    let tarball_path = releases_dir.join(&tarball_name);

    // Build the tarball
    let tarball_file = std::fs::File::create(&tarball_path)?;
    let enc = GzEncoder::new(tarball_file, Compression::best());
    let mut tar = tar::Builder::new(enc);
    tar.append_path_with_name(artifact_path, artifact_name)?;
    tar.finish()?;

    // Compute SHA-256 checksum of the tarball
    let tarball_bytes = std::fs::read(&tarball_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&tarball_bytes);
    let checksum = format!("sha256:{}", crate::util::hex::encode(hasher.finalize()));

    let manifest = PackageManifest {
        version: version.to_string(),
        commit: commit.to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        stability,
        artifacts: vec![tarball_name.clone()],
        checksum,
    };

    // Write manifest.json
    let manifest_path = releases_dir.join(format!("{}-manifest.json", version));
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    tracing::info!(
        version = version,
        tarball = ?tarball_path,
        "release package created"
    );

    Ok(PackageResult {
        tarball: tarball_path,
        manifest,
    })
}
