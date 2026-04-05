use crate::config::loader::DistributionConfig;
use crate::release::packager::PackageResult;
use std::path::Path;

/// Dispatch the packaged artifacts to configured distribution targets.
///
/// Currently implements **local** distribution only. S3 and registry targets
/// are declared in config but require external crates not included by default;
/// they are logged as warnings and skipped.
pub fn distribute(
    pkg: &PackageResult,
    config: &DistributionConfig,
    repo_path: &Path,
) -> anyhow::Result<()> {
    if let Some(local) = &config.local {
        let dest_dir = if Path::new(&local.path).is_absolute() {
            Path::new(&local.path).to_path_buf()
        } else {
            repo_path.join(&local.path)
        };

        std::fs::create_dir_all(&dest_dir)?;

        // Copy tarball
        let dest_tarball = dest_dir.join(
            pkg.tarball
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("tarball has no filename"))?,
        );
        std::fs::copy(&pkg.tarball, &dest_tarball)?;

        // Copy manifest
        let manifest_name = format!("{}-manifest.json", pkg.manifest.version);
        let src_manifest = pkg
            .tarball
            .parent()
            .unwrap_or(Path::new("."))
            .join(&manifest_name);
        if src_manifest.exists() {
            std::fs::copy(&src_manifest, dest_dir.join(&manifest_name))?;
        }

        tracing::info!(
            version = pkg.manifest.version,
            dest = ?dest_dir,
            "artifact distributed to local path"
        );
    }

    if config.s3.is_some() {
        tracing::warn!("S3 distribution configured but not yet implemented; skipping");
    }

    if config.registry.is_some() {
        tracing::warn!("registry distribution configured but not yet implemented; skipping");
    }

    Ok(())
}
