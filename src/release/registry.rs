//! Docker Registry distribution implementation.
//!
//! Supports Docker Hub and private registries.
//! Uses Docker's HTTP API V2 for image pushes.

use crate::config::loader::RegistryDistConfig;
use crate::release::packager::PackageResult;
use anyhow::{anyhow, Context};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Registry distributor that builds and pushes Docker images.
pub struct RegistryDistributor {
    config: RegistryDistConfig,
    registry: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

impl RegistryDistributor {
    /// Create a new registry distributor from config.
    /// Credentials are read from environment:
    /// - `DOCKER_REGISTRY` (optional, for private registries)
    /// - `DOCKER_USERNAME` (optional)
    /// - `DOCKER_PASSWORD` (optional)
    pub fn new(config: RegistryDistConfig) -> Self {
        let registry = std::env::var("DOCKER_REGISTRY").ok();
        let username = std::env::var("DOCKER_USERNAME").ok();
        let password = std::env::var("DOCKER_PASSWORD").ok();

        Self {
            config,
            registry,
            username,
            password,
        }
    }

    /// Build and push a Docker image containing the package.
    pub async fn distribute(&self, pkg: &PackageResult) -> anyhow::Result<()> {
        // Verify Docker is available
        self.verify_docker().await?;

        // Build full image name
        let image_name = self.build_image_name();
        let tag = &pkg.manifest.version;
        let full_tag = format!("{}:{}", image_name, tag);

        // Create a temporary build context
        let build_dir = pkg.tarball.parent().unwrap_or(Path::new("."));

        // Create a minimal Dockerfile
        let dockerfile_content = self.generate_dockerfile(pkg)?;
        let dockerfile_path = build_dir.join("Dockerfile.kaptaind");
        tokio::fs::write(&dockerfile_path, dockerfile_content)
            .await
            .context("failed to write temporary Dockerfile")?;

        // Login if credentials provided
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            self.docker_login(user, pass).await?;
        }

        // Build the image
        tracing::info!(image = %full_tag, "building Docker image");
        let build_output = Command::new("docker")
            .arg("build")
            .arg("-f")
            .arg(&dockerfile_path)
            .arg("-t")
            .arg(&full_tag)
            .arg(build_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("failed to execute docker build")?;

        if !build_output.status.success() {
            let stderr = String::from_utf8_lossy(&build_output.stderr);
            return Err(anyhow!("docker build failed: {}", stderr));
        }

        // Push the image
        tracing::info!(image = %full_tag, registry = ?self.registry, "pushing Docker image");
        let push_output = Command::new("docker")
            .arg("push")
            .arg(&full_tag)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("failed to execute docker push")?;

        if !push_output.status.success() {
            let stderr = String::from_utf8_lossy(&push_output.stderr);
            return Err(anyhow!("docker push failed: {}", stderr));
        }

        // Cleanup temporary Dockerfile
        let _ = tokio::fs::remove_file(&dockerfile_path).await;

        tracing::info!(
            version = pkg.manifest.version,
            image = %full_tag,
            registry = ?self.registry,
            "image distributed to registry"
        );

        Ok(())
    }

    fn build_image_name(&self) -> String {
        if let Some(registry) = &self.registry {
            format!("{}/{}", registry.trim_end_matches('/'), self.config.image)
        } else {
            self.config.image.clone()
        }
    }

    async fn verify_docker(&self) -> anyhow::Result<()> {
        let output = Command::new("docker")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await
            .context("docker not found. Is Docker installed and in PATH?")?;

        if !output.status.success() {
            return Err(anyhow!("docker daemon not running or not accessible"));
        }

        Ok(())
    }

    async fn docker_login(&self, username: &str, password: &str) -> anyhow::Result<()> {
        let registry = self.registry.as_deref().unwrap_or("docker.io");

        let output = Command::new("docker")
            .arg("login")
            .arg("-u")
            .arg(username)
            .arg("-p")
            .arg(password)
            .arg(registry)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("failed to execute docker login")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("docker login failed: {}", stderr));
        }

        Ok(())
    }

    fn generate_dockerfile(&self, pkg: &PackageResult) -> anyhow::Result<String> {
        let tarball_name = pkg
            .tarball
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("invalid tarball filename"))?;

        let dockerfile = format!(
            r#"# Auto-generated by kaptaind
FROM scratch as artifacts
COPY {} /artifacts/
COPY {}-manifest.json /artifacts/

# Metadata
LABEL org.opencontainers.image.version="{}"
LABEL org.opencontainers.image.revision="{}"
LABEL org.opencontainers.image.title="{}"
LABEL org.opencontainers.image.created="{}"

# Default to showing the manifest
CMD ["cat", "/artifacts/{}-manifest.json"]
"#,
            tarball_name,
            pkg.manifest.version,
            pkg.manifest.version,
            pkg.manifest.commit,
            pkg.manifest.version,
            chrono::Utc::now().to_rfc3339(),
            pkg.manifest.version
        );

        Ok(dockerfile)
    }
}

/// Distribute to a container registry using external tool (crane, skopeo, etc.)
pub struct ExternalRegistryDistributor {
    tool: String,
    config: RegistryDistConfig,
}

impl ExternalRegistryDistributor {
    pub fn new(tool: impl Into<String>, config: RegistryDistConfig) -> Self {
        Self {
            tool: tool.into(),
            config,
        }
    }

    pub async fn distribute(&self, _pkg: &PackageResult) -> anyhow::Result<()> {
        match self.tool.as_str() {
            "crane" => {
                // Future: Implement crane (Google's container tool) support
                Err(anyhow!("crane distribution not yet implemented"))
            }
            "skopeo" => {
                // Future: Implement skopeo support for non-Docker environments
                Err(anyhow!("skopeo distribution not yet implemented"))
            }
            _ => Err(anyhow!("unknown external registry tool: {}", self.tool)),
        }
    }
}
