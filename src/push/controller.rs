use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

use crate::config::loader::{PushProtectionConfig, RemoteConfig, RetryConfig};
use anyhow::{bail, Context};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct PushOptions {
    pub remote: String,
    pub branch: String,
    pub dry_run: bool,
    pub protect_branches: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MultiRemotePushOptions {
    pub remotes: Vec<RemoteConfig>,
    pub branch: String,
    pub dry_run: bool,
    pub protect_branches: Vec<String>,
}

// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn push(
    repo_path: &Path,
    options: &PushOptions,
    retry: &RetryConfig,
    protection: &PushProtectionConfig,
) -> anyhow::Result<()> {
    push_with_audit(repo_path, options, retry, protection, "push").await
}

// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn push_multi_remote(
    repo_path: &Path,
    options: &MultiRemotePushOptions,
    retry: &RetryConfig,
    protection: &PushProtectionConfig,
) -> anyhow::Result<Vec<String>> {
    let mut enabled_remotes: Vec<_> = options.remotes.iter().filter(|r| r.enabled).collect();

    // Sort by priority (lower numbers first)
    enabled_remotes.sort_by_key(|r| r.priority);

    let mut successful_pushes = Vec::new();
    let mut last_error: Option<anyhow::Error> = None;

    for remote_config in enabled_remotes {
        let push_options = PushOptions {
            remote: remote_config.name.clone(),
            branch: options.branch.clone(),
            dry_run: options.dry_run,
            protect_branches: options.protect_branches.clone(),
        };

        match push_with_audit(repo_path, &push_options, retry, protection, "push").await {
            Ok(_) => {
                successful_pushes.push(remote_config.name.clone());
                tracing::info!(
                    remote = %remote_config.name,
                    role = %remote_config.role,
                    "push succeeded"
                );
            }
            Err(e) => {
                tracing::warn!(
                    remote = %remote_config.name,
                    role = %remote_config.role,
                    error = %e,
                    "push failed, continuing with other remotes"
                );
                last_error = Some(e);
            }
        }
    }

    if successful_pushes.is_empty() {
        let error =
            last_error.unwrap_or_else(|| anyhow::anyhow!("no remotes were successfully pushed"));
        tracing::error!(error = %error, "all remotes failed; no push succeeded");
        Err(error)
    } else {
        Ok(successful_pushes)
    }
}

async fn push_with_audit(
    repo_path: &Path,
    options: &PushOptions,
    retry: &RetryConfig,
    protection: &PushProtectionConfig,
    actor: &str,
) -> anyhow::Result<()> {
    crate::integration::automatic_check(repo_path)
        .context("automatic Hybreed/Scrawny/Emulsify integration check failed")?;
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

    check_branch_protection(repo_path, options, protection).await?;

    let refspec = format!("refs/heads/{0}:refs/heads/{0}", options.branch);
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=retry.max_attempts {
        let mut command = Command::new("git");
        command
            .current_dir(repo_path)
            // Never let git block waiting on a TTY for credentials: a stuck
            // credential helper or askpass prompt has no terminal to read
            // from in a daemon context and would otherwise hang the push
            // silently until an external caller times it out.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "false")
            .env("SSH_ASKPASS_REQUIRE", "never")
            .arg("push");
        if options.dry_run {
            command.arg("--dry-run");
        }
        command
            .arg(&options.remote)
            .arg(&refspec)
            .kill_on_drop(true);

        let attempt_timeout = Duration::from_secs(retry.attempt_timeout_secs);
        let output = match tokio::time::timeout(attempt_timeout, command.output()).await {
            Ok(result) => result,
            Err(_) => {
                tracing::error!(
                    attempt,
                    timeout_secs = retry.attempt_timeout_secs,
                    operation = "push_with_audit",
                    "git push attempt timed out; killing child process"
                );
                last_error = Some(anyhow::anyhow!(
                    "git push attempt {} timed out after {}s",
                    attempt,
                    retry.attempt_timeout_secs
                ));
                if attempt < retry.max_attempts {
                    let delay_ms = (retry.initial_delay_ms as f64
                        * retry.backoff_multiplier.powi((attempt - 1) as i32))
                    .min(retry.max_delay_ms as f64) as u64;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                continue;
            }
        };

        match output {
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if output.status.success() {
                    crate::audit::log_push(
                        repo_path,
                        actor,
                        "",
                        &options.branch,
                        &options.remote,
                        true,
                        None,
                    );
                    return Ok(());
                }
                last_error = Some(anyhow::anyhow!(
                    "Git push failed with status: {}{}",
                    output.status,
                    if stderr.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", stderr)
                    }
                ));
            }
            Err(e) => {
                tracing::error!(
                    ?e,
                    operation = "push_with_audit",
                    source_line = line!(),
                    "push with audit returned an error"
                );
                last_error = Some(anyhow::anyhow!("Failed to execute git command: {}", e));
            }
        }

        if attempt < retry.max_attempts {
            let delay_ms = (retry.initial_delay_ms as f64
                * retry.backoff_multiplier.powi((attempt - 1) as i32))
            .min(retry.max_delay_ms as f64) as u64;
            tracing::debug!(
                component = module_path!(),
                "push attempt {} failed, retrying in {}ms",
                attempt,
                delay_ms
            );
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    let err = last_error.unwrap_or_else(|| {
        anyhow::anyhow!("Git push failed after {} attempts", retry.max_attempts)
    });
    crate::audit::log_push(
        repo_path,
        actor,
        "",
        &options.branch,
        &options.remote,
        false,
        Some(&err.to_string()),
    );
    tracing::error!(error = ?err, attempts = retry.max_attempts, "git push exhausted its retry budget");
    Err(err)
}

/// Verifies that all configured required CI checks have passed for the target
/// branch before allowing a push.
///
/// If GitHub is the remote and a token is configured, the GitHub REST API is
/// queried for both legacy commit statuses and GitHub Actions check runs. In
/// all other cases the function falls back to a local `.kaptaind/ci-status.json`
/// file, or returns `Ok` if no local override is present.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn check_branch_protection(
    repo_path: &Path,
    options: &PushOptions,
    protection: &PushProtectionConfig,
) -> anyhow::Result<()> {
    if !protection.require_ci_pass || protection.required_status_checks.is_empty() {
        return Ok(());
    }

    let remote_url = match git_remote_url(repo_path, &options.remote).await {
        Ok(url) => url,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to resolve remote URL; falling back to local CI status check"
            );
            return local_ci_status_check(repo_path, protection).await;
        }
    };

    let Some((owner, repo)) = parse_github_owner_repo(&remote_url) else {
        tracing::warn!(
            remote_url = %remote_url,
            "remote is not GitHub; falling back to local CI status check"
        );
        return local_ci_status_check(repo_path, protection).await;
    };

    let Some(token_env) = protection.github_token_env.as_deref() else {
        tracing::warn!(
            component = module_path!(),
            "github_token_env not configured; falling back to local CI status check"
        );
        return local_ci_status_check(repo_path, protection).await;
    };

    let Ok(token) = std::env::var(token_env) else {
        tracing::warn!(
            env = %token_env,
            "GitHub token environment variable not set; falling back to local CI status check"
        );
        return local_ci_status_check(repo_path, protection).await;
    };

    let client = crate::util::http::hardened_client(std::time::Duration::from_secs(20));
    let mut passing = HashSet::new();

    match query_combined_status(&client, &owner, &repo, &options.branch, &token).await {
        Ok(statuses) => {
            for status in statuses {
                if status.state.eq_ignore_ascii_case("success") {
                    passing.insert(status.context);
                }
            }
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to query GitHub combined status"
            );
        }
    }

    match query_check_runs(&client, &owner, &repo, &options.branch, &token).await {
        Ok(runs) => {
            for run in runs {
                if run.status.eq_ignore_ascii_case("completed")
                    && run
                        .conclusion
                        .as_deref()
                        .unwrap_or("")
                        .eq_ignore_ascii_case("success")
                {
                    passing.insert(run.name);
                }
            }
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to query GitHub check runs"
            );
        }
    }

    let mut failing = Vec::new();
    for required in &protection.required_status_checks {
        if !passing.contains(required) {
            failing.push(required.clone());
        }
    }

    if failing.is_empty() {
        Ok(())
    } else {
        bail!(
            "required CI checks did not pass for '{}': {}",
            options.branch,
            failing.join(", ")
        )
    }
}

async fn git_remote_url(repo_path: &Path, remote: &str) -> anyhow::Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["remote", "get-url", remote])
        .output()
        .await
        .context("failed to run git remote get-url")?;

    if !output.status.success() {
        bail!(
            "git remote get-url failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_github_owner_repo(url: &str) -> Option<(String, String)> {
    let https_prefix = "https://github.com/";
    let ssh_prefix = "git@github.com:";
    let ssh_url_prefix = "ssh://git@github.com/";

    let maybe_rest = if let Some(rest) = url.strip_prefix(https_prefix) {
        Some(rest)
    } else if let Some(rest) = url.strip_prefix(ssh_prefix) {
        Some(rest)
    } else {
        url.strip_prefix(ssh_url_prefix)
    };

    let rest = maybe_rest?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");

    let (owner, repo) = rest.split_once('/')?;

    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    Some((owner.to_string(), repo.to_string()))
}

#[derive(Debug, Deserialize)]
struct CombinedStatus {
    statuses: Vec<CommitStatus>,
}

#[derive(Debug, Deserialize)]
struct CommitStatus {
    context: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct CheckRunsResponse {
    check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
struct CheckRun {
    name: String,
    status: String,
    conclusion: Option<String>,
}

async fn query_combined_status(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
    token: &str,
) -> anyhow::Result<Vec<CommitStatus>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{branch}/status");
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "kaptaind")
        .send()
        .await
        .context("failed to send GitHub status request")?;

    let response = response
        .error_for_status()
        .context("GitHub status request")?;
    let payload: CombinedStatus = response
        .json()
        .await
        .context("failed to parse GitHub status response")?;
    Ok(payload.statuses)
}

async fn query_check_runs(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
    token: &str,
) -> anyhow::Result<Vec<CheckRun>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{branch}/check-runs");
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "kaptaind")
        .send()
        .await
        .context("failed to send GitHub check-runs request")?;

    let response = response
        .error_for_status()
        .context("GitHub check-runs request")?;
    let payload: CheckRunsResponse = response
        .json()
        .await
        .context("failed to parse GitHub check-runs response")?;
    Ok(payload.check_runs)
}

#[derive(Debug, Deserialize)]
struct LocalCiStatus {
    #[serde(default)]
    checks: std::collections::HashMap<String, String>,
}

async fn local_ci_status_check(
    repo_path: &Path,
    protection: &PushProtectionConfig,
) -> anyhow::Result<()> {
    let path = repo_path.join(".kaptaind").join("ci-status.json");
    if !path.exists() {
        return Ok(());
    }

    let contents = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))?;
    let status: LocalCiStatus = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    let mut failing = Vec::new();
    for required in &protection.required_status_checks {
        let state = status.checks.get(required).map(String::as_str);
        if !state
            .map(|s| s.eq_ignore_ascii_case("success"))
            .unwrap_or(false)
        {
            failing.push(required.clone());
        }
    }

    if failing.is_empty() {
        Ok(())
    } else {
        bail!(
            "required CI checks did not pass for local status file '{}': {}",
            path.display(),
            failing.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_urls() {
        let cases = vec![
            ("https://github.com/owner/repo.git", ("owner", "repo")),
            ("https://github.com/owner/repo", ("owner", "repo")),
            ("git@github.com:owner/repo.git", ("owner", "repo")),
            ("git@github.com:owner/repo", ("owner", "repo")),
            ("ssh://git@github.com/owner/repo.git", ("owner", "repo")),
            ("https://github.com/org/repo-name.git", ("org", "repo-name")),
        ];

        for (url, expected) in cases {
            let parsed = parse_github_owner_repo(url).expect(url);
            assert_eq!(parsed.0, expected.0, "owner mismatch for {url}");
            assert_eq!(parsed.1, expected.1, "repo mismatch for {url}");
        }

        assert!(parse_github_owner_repo("https://gitlab.com/owner/repo.git").is_none());
        assert!(parse_github_owner_repo("not-a-url").is_none());
    }

    #[tokio::test]
    async fn test_check_branch_protection_skips_when_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let options = PushOptions {
            remote: "origin".to_string(),
            branch: "main".to_string(),
            dry_run: true,
            protect_branches: Vec::new(),
        };
        let protection = PushProtectionConfig {
            require_ci_pass: false,
            required_status_checks: vec!["ci/test".to_string()],
            github_token_env: None,
        };

        let result = check_branch_protection(temp.path(), &options, &protection).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_check_branch_protection_bails_on_missing_local_checks() {
        let temp = tempfile::tempdir().unwrap();
        let kaptaind_dir = temp.path().join(".kaptaind");
        tokio::fs::create_dir_all(&kaptaind_dir).await.unwrap();
        tokio::fs::write(
            kaptaind_dir.join("ci-status.json"),
            r#"{"checks": {"ci/lint": "success"}}"#,
        )
        .await
        .unwrap();

        let options = PushOptions {
            remote: "origin".to_string(),
            branch: "main".to_string(),
            dry_run: true,
            protect_branches: Vec::new(),
        };
        let protection = PushProtectionConfig {
            require_ci_pass: true,
            required_status_checks: vec!["ci/test".to_string(), "ci/lint".to_string()],
            github_token_env: None,
        };

        let result = check_branch_protection(temp.path(), &options, &protection).await;
        let err = result.expect_err("should fail when required checks are missing");
        let msg = err.to_string();
        assert!(
            msg.contains("ci/test"),
            "missing check should be listed: {msg}"
        );
        assert!(
            !msg.contains("ci/lint"),
            "passing check should not be listed: {msg}"
        );
    }
}
