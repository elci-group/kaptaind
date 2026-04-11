use std::path::Path;
use tokio::process::Command;

pub async fn push(repo_path: &Path, branch: &str) -> anyhow::Result<()> {
    // Attempting to push using the system git binary.
    // This dramatically reduces friction with third-party tools (SSH agents, 2FA,
    // credential helpers, corporate proxies) because it inherits the user's existing environment.
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    let mut child = Command::new("git")
        .current_dir(repo_path)
        .arg("push")
        .arg("origin")
        .arg(&refspec)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to execute git command: {}", e))?;

    let status = child
        .wait()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wait for git push: {}", e))?;

    if !status.success() {
        anyhow::bail!("Git push failed with status: {}", status);
    }

    Ok(())
}
