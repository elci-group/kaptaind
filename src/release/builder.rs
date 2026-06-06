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
pub async fn run(config: &BuildConfig, repo_path: &Path) -> BuildStatus {
    let Some(command) = config.command.as_deref() else {
        return BuildStatus::Skipped;
    };

    if let Err(err) = crate::util::shell_validation::validate_shell_command(command) {
        tracing::warn!(error = %err, command = command, "shell command validation failed");
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
        Ok(Ok(output)) => BuildStatus::Failed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Ok(Err(err)) => BuildStatus::Failed {
            code: None,
            stderr: err.to_string(),
        },
        Err(_elapsed) => BuildStatus::Failed {
            code: None,
            stderr: format!("build timed out after {}s", config.timeout_secs),
        },
    }
}
