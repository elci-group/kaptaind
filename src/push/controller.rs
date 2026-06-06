use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct PushOptions {
    pub remote: String,
    pub branch: String,
    pub dry_run: bool,
    pub protect_branches: Vec<String>,
}

pub async fn push(
    repo_path: &Path,
    options: &PushOptions,
    retry: &crate::config::loader::RetryConfig,
) -> anyhow::Result<()> {
    if options
        .protect_branches
        .iter()
        .any(|branch| branch == &options.branch)
    {
        anyhow::bail!(
            "push to protected branch '{}' is disabled by configuration",
            options.branch
        );
    }

    let refspec = format!("refs/heads/{0}:refs/heads/{0}", options.branch);
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=retry.max_attempts {
        let mut command = Command::new("git");
        command.current_dir(repo_path).arg("push");

        if options.dry_run {
            command.arg("--dry-run");
        }

        match command
            .arg(&options.remote)
            .arg(&refspec)
            .kill_on_drop(true)
            .spawn()
        {
            Ok(mut child) => {
                match child.wait().await {
                    Ok(status) => {
                        if status.success() {
                            return Ok(());
                        }
                        last_error = Some(anyhow::anyhow!(
                            "Git push failed with status: {}",
                            status
                        ));
                    }
                    Err(e) => {
                        last_error = Some(anyhow::anyhow!(
                            "Failed to wait for git push: {}",
                            e
                        ));
                    }
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!(
                    "Failed to execute git command: {}",
                    e
                ));
            }
        }

        if attempt < retry.max_attempts {
            let delay_ms = (retry.initial_delay_ms as f64
                * retry.backoff_multiplier.powi((attempt - 1) as i32))
                .min(retry.max_delay_ms as f64) as u64;
            tracing::debug!(
                "push attempt {} failed, retrying in {}ms",
                attempt,
                delay_ms
            );
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    Err(last_error.unwrap_or_else(|| {
        anyhow::anyhow!("Git push failed after {} attempts", retry.max_attempts)
    }))
}
