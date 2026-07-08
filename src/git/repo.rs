use anyhow::{anyhow, Context};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Repo {
    root: PathBuf,
}

impl Repo {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let output = git(path)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("failed to run git rev-parse")?;

        if !output.status.success() {
            return Err(anyhow!(
                "not a git repository: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Self {
            root: PathBuf::from(root),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn changed_paths(&self) -> anyhow::Result<Vec<PathBuf>> {
        #[cfg(feature = "git2")]
        {
            git2_backend::changed_paths_git2(&self.root)
        }
        #[cfg(not(feature = "git2"))]
        {
            changed_paths(&self.root)
        }
    }

    pub fn is_clean(&self) -> anyhow::Result<bool> {
        #[cfg(feature = "git2")]
        {
            git2_backend::is_clean_git2(&self.root)
        }
        #[cfg(not(feature = "git2"))]
        {
            Ok(self.changed_paths()?.is_empty())
        }
    }

    pub fn head_commit_hash(&self) -> anyhow::Result<String> {
        #[cfg(feature = "git2")]
        {
            git2_backend::head_commit_hash_git2(&self.root)
        }
        #[cfg(not(feature = "git2"))]
        {
            let output = git(&self.root)
                .args(["rev-parse", "HEAD"])
                .output()
                .context("failed to run git rev-parse HEAD")?;

            if !output.status.success() {
                return Err(git_error("rev-parse HEAD", &output));
            }

            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
    }
}

pub fn ensure_git_available() -> anyhow::Result<()> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .context("failed to run git --version")?;

    if !output.status.success() {
        return Err(git_error("--version", &output));
    }

    Ok(())
}

pub fn changed_paths(repo_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let output = git(repo_path)
        .args(["status", "--porcelain", "-z"])
        .output()
        .context("failed to run git status")?;

    if !output.status.success() {
        return Err(git_error("status --porcelain", &output));
    }

    Ok(parse_porcelain_paths(&output.stdout))
}

pub fn run_git(repo_path: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = git(repo_path)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        return Err(git_error(&args.join(" "), &output));
    }

    Ok(())
}

/// Create a GPG-signed commit.
///
/// Runs `git commit -S[=<key_id>] -m <msg>`. The caller is responsible for
/// ensuring the GPG key is available and that git is configured to use it.
pub fn commit_signed(repo_path: &Path, msg: &str, gpg_key_id: Option<&str>) -> anyhow::Result<()> {
    let output = match gpg_key_id {
        Some(key_id) => git(repo_path)
            .args(["commit", &format!("-S={}", key_id), "-m", msg])
            .output()
            .context("failed to run git commit -S=<key_id>")?,
        None => git(repo_path)
            .args(["commit", "-S", "-m", msg])
            .output()
            .context("failed to run git commit -S")?,
    };

    if !output.status.success() {
        return Err(git_error("commit -S", &output));
    }

    Ok(())
}

fn git(repo_path: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo_path);
    command
}

fn parse_porcelain_paths(output: &[u8]) -> Vec<PathBuf> {
    output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            if entry.len() < 4 {
                return None;
            }
            let path = String::from_utf8_lossy(&entry[3..]).to_string();
            Some(PathBuf::from(path))
        })
        .collect()
}

fn git_error(command: &str, output: &std::process::Output) -> anyhow::Error {
    anyhow!(
        "git {} failed: {}",
        command,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

#[cfg(feature = "git2")]
mod git2_backend {
    use super::*;
    use anyhow::Context;

    pub fn is_clean_git2(repo_path: &Path) -> anyhow::Result<bool> {
        let repo = git2::Repository::open(repo_path).context("failed to open repo with git2")?;
        let statuses = repo.statuses(None).context("failed to get statuses")?;
        Ok(statuses.is_empty())
    }

    pub fn changed_paths_git2(repo_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let repo = git2::Repository::open(repo_path).context("failed to open repo with git2")?;
        let statuses = repo.statuses(None).context("failed to get statuses")?;
        let mut paths = Vec::new();
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                paths.push(PathBuf::from(path));
            }
        }
        Ok(paths)
    }

    pub fn head_commit_hash_git2(repo_path: &Path) -> anyhow::Result<String> {
        let repo = git2::Repository::open(repo_path).context("failed to open repo with git2")?;
        let head = repo.head().context("failed to get HEAD")?;
        let commit = head.peel_to_commit().context("failed to peel to commit")?;
        Ok(commit.id().to_string())
    }
}
