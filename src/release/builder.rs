use crate::config::loader::BuildConfig;
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum BuildStatus {
    Passed,
    Failed { code: Option<i32>, stderr: String },
    Skipped,
}

impl BuildStatus {
    pub fn passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "pass",
            Self::Failed { .. } => "fail",
            Self::Skipped => "skip",
        }
    }
}

/// Run the configured build command and return its result.
///
/// Returns `BuildStatus::Skipped` when no `[build]` command is configured.
#[tracing::instrument(
    skip_all,
    fields(
        correlation_id = %uuid::Uuid::new_v4(),
        repo_path = %repo_path.display()
    )
)]
pub async fn run(config: &BuildConfig, repo_path: &Path) -> BuildStatus {
    let Some(command) = config.command.as_deref() else {
        return BuildStatus::Skipped;
    };

    if let Err(err) = crate::util::shell_validation::validate_shell_command(command) {
        tracing::warn!(error = %err, command = command, "shell command validation rejected build");
        return BuildStatus::Failed {
            code: None,
            stderr: format!("rejected unsafe build command: {err}"),
        };
    }

    tracing::info!(command = command, "running build step");

    let timeout = tokio::time::Duration::from_secs(config.timeout_secs);

    let result = tokio::time::timeout(
        timeout,
        Command::new("sh")
            .arg("-lc")
            .arg(command)
            .current_dir(repo_path)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => BuildStatus::Passed,
        Ok(Ok(output)) => {
            tracing::error!(
                exit_code = ?output.status.code(),
                "release build command failed"
            );
            BuildStatus::Failed {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            }
        }
        Ok(Err(err)) => {
            tracing::error!(error = %err, "failed to spawn release build command");
            BuildStatus::Failed {
                code: None,
                stderr: err.to_string(),
            }
        }
        Err(_elapsed) => {
            tracing::error!(
                timeout_secs = config.timeout_secs,
                "release build command timed out"
            );
            BuildStatus::Failed {
                code: None,
                stderr: format!("build timed out after {}s", config.timeout_secs),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejected_command_is_not_executed() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("executed");
        let config = BuildConfig {
            command: Some(format!("echo $(touch {})", marker.display())),
            ..BuildConfig::default()
        };

        let status = run(&config, dir.path()).await;

        assert!(matches!(status, BuildStatus::Failed { code: None, .. }));
        assert!(!marker.exists());
    }
}
