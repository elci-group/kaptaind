use git2::Repository;
use std::process::Command;

pub fn push(repo: &Repository, branch: &str) -> anyhow::Result<()> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("Repository has no working directory"))?;

    // Attempting to push using the system git binary.
    // This dramatically reduces friction with third-party tools (SSH agents, 2FA,
    // credential helpers, corporate proxies) because it inherits the user's existing environment.
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    
    let output = Command::new("git")
        .current_dir(workdir)
        .arg("push")
        .arg("origin")
        .arg(&refspec)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute git command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git push failed: {}", stderr.trim());
    }

    Ok(())
}
