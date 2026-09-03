//! External merge/consolidation/review-load analysis through Hybreed,
//! Scrawny, and Emulsify.
//!
//! The integration is deliberately advisory: it records deterministic tool
//! output and never merges, resets, or rewrites a repository branch. Hybreed
//! judges branch relationships, Scrawny judges how hard the *content* of
//! `source` since it diverged from `target` will be to review, and Emulsify
//! judges multi-tree consolidation risk. A Scrawny policy failure is a
//! verdict, not a tool crash — see `run_scrawny_check`.

use crate::config::loader::IntegrationsConfig;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationReport {
    pub generated_at: DateTime<Utc>,
    pub repository: PathBuf,
    pub target: String,
    pub source: String,
    pub hybreed: Value,
    pub scrawny: Value,
    pub emulsify: Value,
    pub recommendation: String,
    pub persisted: Option<PathBuf>,
}

/// Analyse two refs without changing the worktree. `target` is the host and
/// `source` is the proposed fork/contributor branch.
pub fn analyse(
    repo: &Path,
    target: &str,
    source: &str,
    config: &IntegrationsConfig,
    persist: bool,
) -> Result<IntegrationReport> {
    let hybreed_program = if config.hybreed_command.trim().is_empty() {
        "hybreed"
    } else {
        config.hybreed_command.as_str()
    };
    let emulsify_program = if config.emulsify_command.trim().is_empty() {
        "emulsify"
    } else {
        config.emulsify_command.as_str()
    };
    let scrawny_program = if config.scrawny_command.trim().is_empty() {
        "scrawny"
    } else {
        config.scrawny_command.as_str()
    };
    let hybreed = run_json(
        hybreed_program,
        &[
            "decide",
            "--repo",
            path_arg(repo),
            "--format",
            "json",
            target,
            source,
        ],
        config.timeout_secs,
    )
    .with_context(|| format!("running Hybreed for {target} and {source}"))?;

    // Only `source`'s own changes since it diverged from `target` — the unit
    // a reviewer would actually be asked to approve, not the union of both
    // branches' history.
    let range_diff = diff_range(repo, target, source)?;
    let scrawny = run_scrawny_check(
        scrawny_program,
        &["--path", path_arg(repo), "--format", "json", "check", "--stdin"],
        range_diff.as_bytes(),
    )
    .with_context(|| format!("running Scrawny for {target} and {source}"))?;

    let work = TempTrees::new(repo, target, source)?;
    let emulsify = run_json(
        emulsify_program,
        &[
            "analyze",
            path_arg(&work.target),
            path_arg(&work.source),
            "--json",
        ],
        config.timeout_secs,
    )
    .with_context(|| format!("running Emulsify for {target} and {source}"))?;
    drop(work);

    let recommendation = recommendation(&hybreed, &scrawny, &emulsify);
    let mut report = IntegrationReport {
        generated_at: Utc::now(),
        repository: repo.to_path_buf(),
        target: target.to_owned(),
        source: source.to_owned(),
        hybreed,
        scrawny,
        emulsify,
        recommendation,
        persisted: None,
    };

    if persist {
        let dir = repo.join(".kaptaind").join("integration");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!(
            "{}-{}.json",
            Utc::now().format("%Y%m%dT%H%M%SZ"),
            Uuid::new_v4()
        ));
        fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
        report.persisted = Some(path.clone());
        crate::audit::log_event(
            repo,
            "kaptaind-cli",
            "integration.analysis",
            true,
            serde_json::json!({"target": target, "source": source, "report": path}),
        );
    }
    Ok(report)
}

/// Select a deterministic pair of refs for an automatic commit/push check.
/// The current branch is compared with its upstream, then `main`, then
/// `integration`; missing or identical refs are skipped for compatibility with
/// repositories that do not use Kaptaind's lifecycle topology.
pub fn automatic_refs(repo: &Path) -> Result<Option<(String, String)>> {
    let current = current_branch(repo)?;
    if current.is_empty() {
        return Ok(None);
    }
    let candidates = [
        git_output(repo, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok(),
        Some("main".to_owned()),
        Some("integration".to_owned()),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate != current && ref_exists(repo, &candidate) {
            return Ok(Some((candidate, current)));
        }
    }
    Ok(None)
}

pub fn current_branch(repo: &Path) -> Result<String> {
    git_output(repo, &["branch", "--show-current"])
}

/// Run the default integration check for callers (such as the push
/// controller) that do not already hold a fully loaded `Config`.
pub fn automatic_check(repo: &Path) -> Result<()> {
    let config =
        crate::config::loader::load_from_path(&repo.join("kaptaind.toml")).unwrap_or_else(|_| {
            crate::config::Config {
                repo_path: repo.to_path_buf(),
                ..crate::config::Config::default()
            }
        });
    if !config.integrations.enabled {
        return Ok(());
    }
    let Some((target, source)) = automatic_refs(repo)? else {
        return Ok(());
    };
    match analyse(repo, &target, &source, &config.integrations, true) {
        Ok(_) => Ok(()),
        Err(error) if config.integrations.required => Err(error),
        Err(error) => {
            tracing::warn!(error = %error, "integration analysis failed during push; continuing because it is advisory");
            Ok(())
        }
    }
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn ref_exists(repo: &Path, reference: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", reference])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn recommendation(hybreed: &Value, scrawny: &Value, emulsify: &Value) -> String {
    let decision = hybreed
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or("Unknown");
    let action = emulsify
        .get("plan")
        .and_then(|v| v.get("steps"))
        .and_then(Value::as_array)
        .and_then(|steps| steps.first())
        .and_then(|step| step.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("review");
    let review = match scrawny.get("passed").and_then(Value::as_bool) {
        Some(true) => "review-ready".to_owned(),
        Some(false) => {
            let load = scrawny.get("review_load").and_then(Value::as_u64).unwrap_or(0);
            let max = scrawny.get("max_review_load").and_then(Value::as_u64).unwrap_or(0);
            format!("review load {load}/100 exceeds policy ({max})")
        }
        None => "unavailable".to_owned(),
    };
    format!("Hybreed: {decision}; Scrawny: {review}; Emulsify: {action}; explicit validation required")
}

fn run_json(program: &str, args: &[&str], _timeout_secs: u64) -> Result<Value> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("could not execute {program}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "{program} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8(output.stdout).context("tool output was not UTF-8")?;
    serde_json::from_str(&text).with_context(|| format!("{program} did not return JSON"))
}

/// `source`'s own changes since it diverged from `target` (three-dot diff
/// against the merge base) — the unit Scrawny should score, not the union of
/// both branches' unrelated history.
fn diff_range(repo: &Path, target: &str, source: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("diff")
        .arg(format!("{target}...{source}"))
        .output()
        .with_context(|| format!("running git diff {target}...{source}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "git diff {target}...{source} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run `scrawny check --stdin` with `stdin_data` piped in.
///
/// Unlike `run_json`, a non-zero exit here is not necessarily a tool
/// failure: `scrawny check` exits non-zero specifically to signal a policy
/// FAIL, with a valid JSON verdict still on stdout (`{"passed": false, ...}`).
/// That is data for the report, not an execution error — only bail when
/// stdout doesn't carry a recognisable verdict at all.
fn run_scrawny_check(program: &str, args: &[&str], stdin_data: &[u8]) -> Result<Value> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not execute {program}"))?;
    child
        .stdin
        .take()
        .expect("stdin was requested via Stdio::piped()")
        .write_all(stdin_data)
        .with_context(|| format!("writing diff to {program} stdin"))?;
    let output = child
        .wait_with_output()
        .with_context(|| format!("waiting for {program}"))?;

    let text = String::from_utf8(output.stdout).context("tool output was not UTF-8")?;
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("{program} did not return JSON ({}): {}", output.status, String::from_utf8_lossy(&output.stderr).trim()))?;

    if !output.status.success() && value.get("passed").is_none() {
        anyhow::bail!(
            "{program} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(value)
}

fn path_arg(path: &Path) -> &str {
    path.to_str().unwrap_or("")
}

struct TempTrees {
    root: PathBuf,
    target: PathBuf,
    source: PathBuf,
}

impl TempTrees {
    fn new(repo: &Path, target: &str, source: &str) -> Result<Self> {
        let root = std::env::temp_dir().join(format!("kaptaind-integration-{}", Uuid::new_v4()));
        let target_dir = root.join("target");
        let source_dir = root.join("source");
        fs::create_dir_all(&target_dir)?;
        fs::create_dir_all(&source_dir)?;
        archive(repo, target, &target_dir)?;
        archive(repo, source, &source_dir)?;
        Ok(Self {
            root,
            target: target_dir,
            source: source_dir,
        })
    }
}

impl Drop for TempTrees {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn archive(repo: &Path, reference: &str, destination: &Path) -> Result<()> {
    let archive = destination.join("snapshot.tar");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("archive")
        .arg(reference)
        .arg("-o")
        .arg(&archive)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "cannot archive {reference}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(destination)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "cannot extract {reference}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = fs::remove_file(archive);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::recommendation;
    use serde_json::json;

    #[test]
    fn recommendation_combines_all_three_tools() {
        let result = recommendation(
            &json!({"decision":"Merge"}),
            &json!({"passed": true}),
            &json!({"plan":{"steps":[{"action":"emulsify"}]}}),
        );
        assert!(result.contains("Merge"));
        assert!(result.contains("review-ready"));
        assert!(result.contains("emulsify"));
    }

    #[test]
    fn recommendation_surfaces_a_scrawny_policy_failure() {
        let result = recommendation(
            &json!({"decision":"Merge"}),
            &json!({"passed": false, "review_load": 81, "max_review_load": 70}),
            &json!({"plan":{"steps":[{"action":"emulsify"}]}}),
        );
        assert!(result.contains("81/100"));
        assert!(result.contains("exceeds policy (70)"));
    }

    #[test]
    fn recommendation_reports_scrawny_unavailable_without_failing() {
        let result = recommendation(&json!({}), &json!(null), &json!({}));
        assert!(result.contains("Scrawny: unavailable"));
    }
}
