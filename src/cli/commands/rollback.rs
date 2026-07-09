use anyhow::{anyhow, bail, Context};
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use std::process::Command;

/// Revert the most recent kaptaind-produced commit (or a specific one).
///
/// The daemon prefixes every automated commit subject with `kaptaind:`. This
/// command finds the latest such commit and runs `git revert` to undo it
/// safely (a new revert commit, never a destructive rewrite).
pub fn handle_rollback(
    config: &Config,
    commit: Option<&str>,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let repo = &config.repo_path;

    let inside = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(repo)
        .output()
        .context("failed to run git; is git installed and on PATH?")?;
    if !inside.status.success() {
        bail!("{} is not a git repository", repo.display());
    }

    let target = match commit {
        Some(c) => resolve_commit(repo, c)?,
        None => find_latest_kaptaind_commit(repo)?,
    };

    let short = target.short.clone();
    println!(
        "{} rollback target: {} {}",
        "↩️".cyan(),
        short.as_str().yellow().bold(),
        target.subject
    );

    if dry_run || !yes {
        println!(
            "\nDry run. To execute, run:\n  {} {} --yes",
            "kaptaind-cli rollback".green(),
            commit.unwrap_or("")
        );
        println!(
            "Equivalent git command:\n  {}",
            format!("git -C {} revert --no-edit {}", repo.display(), short).dimmed()
        );
        return Ok(());
    }

    let status = Command::new("git")
        .args(["revert", "--no-edit", &target.hash])
        .current_dir(repo)
        .status()
        .context("failed to spawn git revert")?;

    if !status.success() {
        bail!(
            "git revert failed (exit {}). Resolve conflicts or run `git revert --abort`, then retry.",
            status.code().unwrap_or(-1)
        );
    }

    println!(
        "{} reverted {} — inspect with `git show HEAD`",
        "✅".green(),
        target.short
    );
    Ok(())
}

struct CommitRef {
    hash: String,
    short: String,
    subject: String,
}

fn resolve_commit(repo: &std::path::Path, spec: &str) -> anyhow::Result<CommitRef> {
    let out = Command::new("git")
        .args(["log", "-1", "--format=%H%n%s", spec])
        .current_dir(repo)
        .output()
        .context("failed to run git log")?;
    if !out.status.success() {
        bail!("could not resolve commit '{}'", spec);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let hash = lines.next().unwrap_or("").trim().to_string();
    let subject = lines.next().unwrap_or("").trim().to_string();
    if hash.is_empty() {
        return Err(anyhow!("empty resolution for '{}'", spec));
    }
    Ok(CommitRef {
        short: short_hash(&hash),
        hash,
        subject,
    })
}

fn find_latest_kaptaind_commit(repo: &std::path::Path) -> anyhow::Result<CommitRef> {
    let out = Command::new("git")
        .args(["log", "--format=%H%n%s", "-n", "100"])
        .current_dir(repo)
        .output()
        .context("failed to run git log")?;
    if !out.status.success() {
        bail!("git log failed");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    while let Some(hash) = lines.next() {
        let subject = lines.next().unwrap_or("").trim().to_string();
        let hash = hash.trim();
        if subject.starts_with("kaptaind:") && !hash.is_empty() {
            return Ok(CommitRef {
                short: short_hash(hash),
                hash: hash.to_string(),
                subject,
            });
        }
    }
    bail!("no kaptaind-produced commits found in the last 100 commits")
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(8).collect()
}
