//! Read-only git access. Census and extraction never check out a commit.

use anyhow::{anyhow, Context};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommitMeta {
    pub sha: String,
    pub parents: Vec<String>,
    pub committed_at: String,
    pub subject: String,
    pub body: String,
}

impl GitCommitMeta {
    pub fn message(&self) -> String {
        if self.body.is_empty() {
            self.subject.clone()
        } else {
            format!("{}\n\n{}", self.subject, self.body)
        }
    }

    pub fn parent(&self) -> Option<&str> {
        self.parents.first().map(String::as_str)
    }
}

pub fn git_text(repo: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = git_command(repo, args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn git_ok(repo: &Path, args: &[&str]) -> bool {
    git_command(repo, args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git_command(repo: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .arg("--no-pager")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists() || git_ok(path, &["rev-parse", "--git-dir"])
}

pub fn rev_count(repo: &Path, extra: &[&str]) -> anyhow::Result<u64> {
    let mut args = vec!["rev-list", "--count"];
    args.extend_from_slice(extra);
    let text = git_text(repo, &args)?;
    text.parse()
        .with_context(|| format!("rev-list count was not a number: {text}"))
}

pub fn head_sha(repo: &Path) -> anyhow::Result<String> {
    git_text(repo, &["rev-parse", "HEAD"])
}

pub fn origin_url(repo: &Path) -> Option<String> {
    git_text(repo, &["remote", "get-url", "origin"]).ok()
}

pub fn github_slug(url: &str) -> Option<(String, String)> {
    let url = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let mut parts = rest.split('/');
    let org = parts.next()?.trim();
    let name = parts.next()?.trim();
    if org.is_empty() || name.is_empty() || parts.next().is_some() {
        return None;
    }
    Some((org.to_string(), name.to_string()))
}

pub fn log_matching(repo: &Path, grep: &str) -> anyhow::Result<Vec<GitCommitMeta>> {
    let raw = git_text(
        repo,
        &[
            "log",
            "--extended-regexp",
            "--grep",
            grep,
            "--format=%x1e%H%x1f%P%x1f%cI%x1f%s%x1f%b",
        ],
    )?;
    Ok(parse_log(&raw))
}

pub fn log_all_subjects(repo: &Path, limit: Option<usize>) -> anyhow::Result<Vec<GitCommitMeta>> {
    let mut args = vec![
        "log".to_string(),
        "--format=%x1e%H%x1f%P%x1f%cI%x1f%s%x1f%b".to_string(),
    ];
    if let Some(limit) = limit {
        args.push(format!("--max-count={limit}"));
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let raw = git_text(repo, &arg_refs)?;
    Ok(parse_log(&raw))
}

pub fn parse_log(raw: &str) -> Vec<GitCommitMeta> {
    raw.split('\u{1e}')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let mut parts = record.splitn(5, '\u{1f}');
            let sha = parts.next()?.trim().to_string();
            if sha.is_empty() {
                return None;
            }
            let parents = parts
                .next()
                .unwrap_or("")
                .split_whitespace()
                .map(str::to_string)
                .collect();
            let committed_at = parts.next().unwrap_or("").trim().to_string();
            let subject = parts.next().unwrap_or("").trim().to_string();
            let body = parts.next().unwrap_or("").trim().to_string();
            Some(GitCommitMeta {
                sha,
                parents,
                committed_at,
                subject,
                body,
            })
        })
        .collect()
}

pub fn show_path(repo: &Path, spec: &str) -> Option<String> {
    let output = git_command(repo, &["show", spec]).output().ok()?;
    if !output.status.success() || output.stdout.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn diff_path(repo: &Path, commit: &str, path: &str) -> anyhow::Result<String> {
    git_text(
        repo,
        &[
            "diff-tree",
            "-m",
            "--first-parent",
            "--root",
            "--no-commit-id",
            "-p",
            "--unified=3",
            commit,
            "--",
            path,
        ],
    )
}

pub fn changed_paths(repo: &Path, commit: &str) -> anyhow::Result<Vec<String>> {
    let raw = git_text(
        repo,
        &[
            "diff-tree",
            "-m",
            "--first-parent",
            "--root",
            "--no-commit-id",
            "--name-only",
            "-r",
            commit,
        ],
    )?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn tree_sha(repo: &Path, commit: &str) -> Option<String> {
    git_text(repo, &["rev-parse", &format!("{commit}^{{tree}}")]).ok()
}

pub fn tags(repo: &Path) -> anyhow::Result<Vec<String>> {
    let raw = git_text(repo, &["tag", "--list"])?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn tag_commits(repo: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let raw = git_text(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)%09%(objectname)",
            "refs/tags",
        ],
    )?;
    Ok(raw
        .lines()
        .filter_map(|line| {
            let (name, sha) = line.split_once('\t')?;
            Some((name.trim().to_string(), sha.trim().to_string()))
        })
        .collect())
}

/// The next `horizon` commits after `commit`, oldest first. Each item is the
/// subject and body joined the way a full commit message is read.
pub fn descendant_messages(repo: &Path, commit: &str, horizon: u32) -> Vec<String> {
    let spec = format!("{commit}..HEAD");
    let Ok(raw) = git_text(
        repo,
        &[
            "log",
            "--reverse",
            "--format=%x1e%s%x1f%b",
            &format!("--max-count={horizon}"),
            &spec,
        ],
    ) else {
        return Vec::new();
    };
    raw.split('\u{1e}')
        .filter(|record| !record.trim().is_empty())
        .map(|record| {
            let mut parts = record.splitn(2, '\u{1f}');
            let subject = parts.next().unwrap_or("").trim();
            let body = parts.next().unwrap_or("").trim();
            if body.is_empty() {
                subject.to_string()
            } else {
                format!("{subject}\n\n{body}")
            }
        })
        .filter(|message| !message.is_empty())
        .collect()
}

pub fn discover_repos(roots: &[PathBuf], max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in roots {
        walk_repos(root, 0, max_depth, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

fn walk_repos(dir: &Path, depth: usize, max_depth: usize, found: &mut Vec<PathBuf>) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    if dir.join(".git").is_dir() || dir.join(".git").is_file() {
        found.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            "target" | "node_modules" | "dist" | "vendor" | ".kaptaind" | ".cargo" | ".git"
        ) {
            continue;
        }
        walk_repos(&path, depth + 1, max_depth, found);
    }
}

pub fn list_github_org(org: &str) -> anyhow::Result<Vec<RemoteRepo>> {
    let mut command = Command::new("gh");
    command
        .args([
            "repo",
            "list",
            org,
            "--limit",
            "1000",
            "--json",
            "name,url,sshUrl,isArchived",
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    let output = command
        .output()
        .context("failed to run gh repo list; install and authenticate the GitHub CLI")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("gh repo list {org} failed: {}", stderr.trim()));
    }
    let parsed: Vec<GhRepo> = serde_json::from_slice(&output.stdout)
        .context("gh repo list did not return the expected JSON")?;
    Ok(parsed
        .into_iter()
        .map(|repo| RemoteRepo {
            name: repo.name,
            url: repo.url,
            ssh_url: repo.ssh_url,
            archived: repo.is_archived,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRepo {
    pub name: String,
    pub url: String,
    pub ssh_url: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Deserialize)]
struct GhRepo {
    name: String,
    url: String,
    #[serde(rename = "sshUrl")]
    ssh_url: Option<String>,
    #[serde(rename = "isArchived", default)]
    is_archived: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_slug_accepts_ssh_and_https() {
        assert_eq!(
            github_slug("git@github.com:elci-group/kaptaind.git"),
            Some(("elci-group".to_string(), "kaptaind".to_string()))
        );
        assert_eq!(
            github_slug("https://github.com/elci-group/kaptaind"),
            Some(("elci-group".to_string(), "kaptaind".to_string()))
        );
        assert!(github_slug("https://gitlab.com/acme/app.git").is_none());
    }

    #[test]
    fn log_records_keep_multiline_bodies() {
        let raw =
            "\u{1e}abc\u{1f}def\u{1f}2020-01-01T00:00:00Z\u{1f}subject\u{1f}line one\nline two";
        let commits = parse_log(raw);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "abc");
        assert_eq!(commits[0].parents, vec!["def".to_string()]);
        assert!(commits[0].body.contains("line two"));
    }
}
