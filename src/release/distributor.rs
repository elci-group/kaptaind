use crate::config::loader::DistributionConfig;
use crate::release::packager::PackageResult;
use std::path::Path;

/// Dispatch the packaged artifacts to configured distribution targets.
///
/// Supports local, S3, and Docker registry distribution.
pub async fn distribute(
    pkg: &PackageResult,
    config: &DistributionConfig,
    repo_path: &Path,
) -> anyhow::Result<()> {
    let mut had_success = false;
    let mut errors = Vec::new();

    // Local distribution
    if let Some(local) = &config.local {
        match distribute_local(pkg, local, repo_path).await {
            Ok(_) => had_success = true,
            Err(e) => {
                tracing::warn!(error = %e, "local distribution failed");
                errors.push(format!("local: {}", e));
            }
        }
    }

    // S3 distribution
    if let Some(s3_config) = &config.s3 {
        match crate::release::s3::S3Distributor::new(s3_config.clone()) {
            Ok(distributor) => {
                match distributor.distribute(pkg).await {
                    Ok(_) => had_success = true,
                    Err(e) => {
                        tracing::warn!(error = %e, "S3 distribution failed");
                        errors.push(format!("s3: {}", e));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "S3 distributor initialization failed");
                errors.push(format!("s3 init: {}", e));
            }
        }
    }

    // Registry distribution
    if let Some(registry_config) = &config.registry {
        let distributor = crate::release::registry::RegistryDistributor::new(registry_config.clone());
        match distributor.distribute(pkg).await {
            Ok(_) => had_success = true,
            Err(e) => {
                tracing::warn!(error = %e, "registry distribution failed");
                errors.push(format!("registry: {}", e));
            }
        }
    }

    // Return success if at least one distribution method succeeded
    if had_success {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "all distribution methods failed: {}",
            errors.join("; ")
        ))
    }
}

async fn distribute_local(
    pkg: &PackageResult,
    local: &crate::config::loader::LocalDistConfig,
    repo_path: &Path,
) -> anyhow::Result<()> {
    let dest_dir = if Path::new(&local.path).is_absolute() {
        Path::new(&local.path).to_path_buf()
    } else {
        repo_path.join(&local.path)
    };

    tokio::fs::create_dir_all(&dest_dir).await?;

    // Copy tarball
    let dest_tarball = dest_dir.join(
        pkg.tarball
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("tarball has no filename"))?,
    );
    tokio::fs::copy(&pkg.tarball, &dest_tarball).await?;

    // Copy manifest
    let manifest_name = format!("{}-manifest.json", pkg.manifest.version);
    let src_manifest = pkg
        .tarball
        .parent()
        .unwrap_or(Path::new("."))
        .join(&manifest_name);
    if src_manifest.exists() {
        tokio::fs::copy(&src_manifest, dest_dir.join(&manifest_name)).await?;
    }

    tracing::info!(
        version = pkg.manifest.version,
        dest = ?dest_dir,
        "artifact distributed to local path"
    );

    Ok(())
}
