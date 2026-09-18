//! Git plumbing for the promotion engine, checkout-free wherever possible so
//! planning and validation never disturb the caller's working tree.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

pub fn output(repo: &Path, args: &[&str]) -> Result<String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !result.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}

fn rev_parse(repo: &Path, reference: &str) -> Result<Option<String>> {
    let result = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet", reference])
        .output()?;
    Ok(if result.status.success() {
        Some(String::from_utf8_lossy(&result.stdout).trim().to_owned())
    } else {
        None
    })
}

pub fn branch_commit(repo: &Path, branch: &str) -> Result<Option<String>> {
    rev_parse(repo, &format!("refs/heads/{branch}"))
}

pub fn current_branch(repo: &Path) -> Result<String> {
    output(repo, &["branch", "--show-current"])
}

pub fn is_ancestor(repo: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    Ok(Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .status()?
        .success())
}

/// A repository is clean if nothing outside Kaptaind's own `.kaptaind/`
/// bookkeeping (promotion records, audit log) is staged, modified, or
/// untracked. Otherwise every `lifecycle plan`/`validate` call would dirty
/// the tree it just inspected via its own promotion store.
pub fn is_clean(repo: &Path) -> Result<bool> {
    Ok(
        output(repo, &["status", "--porcelain", "--untracked-files=all"])?
            .lines()
            .all(|line| {
                line.get(3..)
                    .is_some_and(|path| path.starts_with(".kaptaind/"))
            }),
    )
}

pub fn commit_count(repo: &Path, source: &str, target: &str) -> Result<usize> {
    output(
        repo,
        &["rev-list", "--count", &format!("{target}..{source}")],
    )?
    .parse()
    .context("unexpected `git rev-list --count` output")
}

pub struct DiffStat {
    pub files: usize,
    pub insertions: usize,
    pub deletions: usize,
}

pub fn diff_stat(repo: &Path, source: &str, target: &str) -> Result<DiffStat> {
    let text = output(
        repo,
        &["diff", "--shortstat", &format!("{target}...{source}")],
    )?;
    let mut stat = DiffStat {
        files: 0,
        insertions: 0,
        deletions: 0,
    };
    for part in text.split(',') {
        let part = part.trim();
        if let Some(count) = part
            .strip_suffix(" files changed")
            .or_else(|| part.strip_suffix(" file changed"))
        {
            stat.files = count.trim().parse().unwrap_or(0);
        } else if let Some(count) = part
            .strip_suffix(" insertions(+)")
            .or_else(|| part.strip_suffix(" insertion(+)"))
        {
            stat.insertions = count.trim().parse().unwrap_or(0);
        } else if let Some(count) = part
            .strip_suffix(" deletions(-)")
            .or_else(|| part.strip_suffix(" deletion(-)"))
        {
            stat.deletions = count.trim().parse().unwrap_or(0);
        }
    }
    Ok(stat)
}

/// Count the paths a merge of `source` into `target` would conflict on,
/// without touching the index or working tree.
pub fn conflict_count(repo: &Path, source_commit: &str, target_commit: &str) -> Result<usize> {
    let result = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "merge-tree",
            "--write-tree",
            "--name-only",
            target_commit,
            source_commit,
        ])
        .output()?;
    if result.status.success() {
        return Ok(0);
    }
    // On conflict, `merge-tree` prints the tree OID, a blank line, then the
    // conflicted path list (one per line, thanks to --name-only), then a
    // second blank line before any informational messages.
    let text = String::from_utf8_lossy(&result.stdout);
    match text.split("\n\n").nth(1) {
        Some(section) => Ok(section
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()),
        None => Ok(1),
    }
}

/// Merge `source` into `target` as a merge commit, without checking out
/// either ref. Returns the new commit hash.
pub fn merge_commit(
    repo: &Path,
    source_branch: &str,
    target_branch: &str,
    source_commit: &str,
    target_commit: &str,
    message: &str,
) -> Result<String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["merge-tree", "--write-tree", target_commit, source_commit])
        .output()?;
    if !result.status.success() {
        bail!("merge of `{source_branch}` into `{target_branch}` produced conflicts");
    }
    let tree = String::from_utf8_lossy(&result.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_owned();
    if tree.is_empty() {
        bail!("git merge-tree returned no tree for `{source_branch}` into `{target_branch}`");
    }
    output(
        repo,
        &[
            "commit-tree",
            &tree,
            "-p",
            target_commit,
            "-p",
            source_commit,
            "-m",
            message,
        ],
    )
}

/// Atomically move `branch` from `expected_old` to `new_commit`, refusing if
/// the ref moved since it was inspected. This is the concurrency guard that
/// invalidates a stale plan instead of mutating past it.
pub fn compare_and_swap_branch(
    repo: &Path,
    branch: &str,
    expected_old: &str,
    new_commit: &str,
) -> Result<()> {
    output(
        repo,
        &[
            "update-ref",
            &format!("refs/heads/{branch}"),
            new_commit,
            expected_old,
        ],
    )?;
    Ok(())
}
