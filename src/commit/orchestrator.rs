use crate::config::loader::{StagingConfig, StagingMode};
use crate::git::repo;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

/// Stage and commit with the default "all" strategy.
pub fn commit(repo_path: &Path, msg: &str) -> anyhow::Result<()> {
    commit_with_staging(repo_path, msg, &StagingConfig::default(), &[])
}

/// Stage and commit with configurable staging behavior.
pub fn commit_with_staging(
    repo_path: &Path,
    msg: &str,
    staging: &StagingConfig,
    cluster_paths: &[PathBuf],
) -> anyhow::Result<()> {
    match staging.mode {
        StagingMode::All => {
            repo::run_git(repo_path, &["add", "-A"])?;
        }
        StagingMode::Cluster => {
            add_paths(repo_path, cluster_paths)?;
            let mut meta_paths = vec![PathBuf::from("VERSION")];
            for cargo_rel in ["Cargo.toml", "src-tauri/Cargo.toml"] {
                if repo_path.join(cargo_rel).exists() {
                    meta_paths.push(PathBuf::from(cargo_rel));
                }
            }
            add_paths(repo_path, &meta_paths)?;
        }
        StagingMode::Pattern => {
            if staging.include.is_empty() {
                repo::run_git(repo_path, &["add", "-A"])?;
            } else {
                let include_globs = build_globset(&staging.include);
                let paths: Vec<PathBuf> = repo::changed_paths(repo_path)?
                    .into_iter()
                    .filter(|path| {
                        include_globs
                            .as_ref()
                            .map(|gs| gs.is_match(path))
                            .unwrap_or(true)
                    })
                    .collect();
                add_paths(repo_path, &paths)?;
            }
        }
    }

    unstage_excluded(repo_path, &staging.exclude)?;
    repo::run_git(repo_path, &["commit", "-m", msg])?;
    Ok(())
}

fn add_paths(repo_path: &Path, paths: &[PathBuf]) -> anyhow::Result<()> {
    for path in paths {
        repo::run_git(repo_path, &["add", "--", &path.to_string_lossy()])?;
    }
    Ok(())
}

fn unstage_excluded(repo_path: &Path, excludes: &[String]) -> anyhow::Result<()> {
    if excludes.is_empty() {
        return Ok(());
    }

    let Some(globset) = build_globset(excludes) else {
        return Ok(());
    };

    for path in repo::changed_paths(repo_path)? {
        if globset.is_match(&path) {
            repo::run_git(
                repo_path,
                &["reset", "-q", "HEAD", "--", &path.to_string_lossy()],
            )?;
        }
    }

    Ok(())
}

fn build_globset(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if let Ok(glob) = Glob::new(pattern) {
            builder.add(glob);
        }
    }
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn all_mode_commits_every_change_except_excludes() {
        let repo = TestRepo::new();
        repo.write("visible.txt", "changed");
        repo.write("secret.txt", "changed");

        let staging = StagingConfig {
            mode: StagingMode::All,
            include: vec![],
            exclude: vec!["secret.txt".to_string()],
        };

        commit_with_staging(repo.path(), "all mode", &staging, &[]).unwrap();

        assert_eq!(repo.last_commit_files(), vec!["visible.txt"]);
        assert!(repo.changed_files().contains(&PathBuf::from("secret.txt")));
    }

    #[test]
    fn cluster_mode_commits_only_cluster_paths() {
        let repo = TestRepo::new();
        repo.write("src/a.rs", "changed");
        repo.write("src/b.rs", "changed");

        let staging = StagingConfig {
            mode: StagingMode::Cluster,
            include: vec![],
            exclude: vec![],
        };

        commit_with_staging(
            repo.path(),
            "cluster mode",
            &staging,
            &[PathBuf::from("src/a.rs")],
        )
        .unwrap();

        assert_eq!(repo.last_commit_files(), vec!["src/a.rs"]);
        assert!(repo.changed_files().contains(&PathBuf::from("src/b.rs")));
    }

    #[test]
    fn pattern_mode_commits_only_included_paths() {
        let repo = TestRepo::new();
        repo.write("src/a.rs", "changed");
        repo.write("README.md", "changed");

        let staging = StagingConfig {
            mode: StagingMode::Pattern,
            include: vec!["**/*.rs".to_string()],
            exclude: vec![],
        };

        commit_with_staging(repo.path(), "pattern mode", &staging, &[]).unwrap();

        assert_eq!(repo.last_commit_files(), vec!["src/a.rs"]);
        assert!(repo.changed_files().contains(&PathBuf::from("README.md")));
    }

    #[test]
    fn no_staged_changes_returns_error() {
        let repo = TestRepo::new();
        let staging = StagingConfig {
            mode: StagingMode::Pattern,
            include: vec!["missing/**".to_string()],
            exclude: vec![],
        };

        let err = commit_with_staging(repo.path(), "nothing", &staging, &[]).unwrap_err();
        assert!(err.to_string().contains("git commit"));
    }

    struct TestRepo {
        dir: tempfile::TempDir,
    }

    impl TestRepo {
        fn new() -> Self {
            let dir = tempdir().unwrap();
            let repo = Self { dir };
            repo.run(&["init"]);
            repo.run(&["config", "user.name", "Kaptaind Test"]);
            repo.run(&["config", "user.email", "kaptaind@example.com"]);
            repo.write("src/a.rs", "initial");
            repo.write("src/b.rs", "initial");
            repo.write("README.md", "initial");
            repo.write("visible.txt", "initial");
            repo.write("secret.txt", "initial");
            repo.write("VERSION", "0.1.0");
            repo.write(
                "Cargo.toml",
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
            );
            repo.run(&["add", "-A"]);
            repo.run(&["commit", "-m", "initial"]);
            repo
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }

        fn write(&self, path: &str, contents: &str) {
            let path = self.dir.path().join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }

        fn changed_files(&self) -> Vec<PathBuf> {
            repo::changed_paths(self.path()).unwrap()
        }

        fn last_commit_files(&self) -> Vec<String> {
            let output = self.output(&["show", "--name-only", "--format=", "HEAD"]);
            let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .collect();
            files.sort();
            files
        }

        fn run(&self, args: &[&str]) {
            let output = self.output(args);
            assert!(
                output.status.success(),
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn output(&self, args: &[&str]) -> std::process::Output {
            std::process::Command::new("git")
                .arg("-C")
                .arg(self.path())
                .args(args)
                .output()
                .unwrap()
        }
    }
}
