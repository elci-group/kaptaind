//! External merge/consolidation analysis through Hybreed and Emulsify.
//!
//! The integration is deliberately advisory: it records deterministic tool
//! output and never merges, resets, or rewrites a repository branch.

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

    let recommendation = recommendation(&hybreed, &emulsify);
    let mut report = IntegrationReport {
        generated_at: Utc::now(),
        repository: repo.to_path_buf(),
        target: target.to_owned(),
        source: source.to_owned(),
        hybreed,
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

fn recommendation(hybreed: &Value, emulsify: &Value) -> String {
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
    format!("Hybreed: {decision}; Emulsify: {action}; explicit validation required")
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
    fn recommendation_combines_both_tools() {
        let result = recommendation(
            &json!({"decision":"Merge"}),
            &json!({"plan":{"steps":[{"action":"emulsify"}]}}),
        );
        assert!(result.contains("Merge"));
        assert!(result.contains("emulsify"));
    }
}
