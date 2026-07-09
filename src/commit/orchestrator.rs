use crate::config::loader::{CommitConfig, StagingConfig, StagingMode};
use crate::git::repo;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

/// Filename patterns that must never be committed, regardless of `kaptaind.toml`
/// or `.gitignore`. Evaluated against every changed path (recursively).
const SECRET_DENYLIST: &[&str] = &[
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "*.p12",
    "*.pfx",
    "id_rsa",
    "id_dsa",
    "*.keystore",
    "*.secret",
];

/// Expand patterns containing no `/` so they also match at any depth: a bare
/// pattern like `.env*` then also covers `apps/api/.env`.
fn expand_recursive(patterns: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for p in patterns {
        out.push(p.clone());
        if !p.contains('/') {
            out.push(format!("**/{p}"));
        }
    }
    out
}

/// Stage and commit with the default "all" strategy.
pub fn commit(repo_path: &Path, msg: &str, commit_config: &CommitConfig) -> anyhow::Result<()> {
    commit_with_staging(
        repo_path,
        msg,
        &StagingConfig::default(),
        &[],
        commit_config,
    )
}

/// Stage and commit with configurable staging behavior.
pub fn commit_with_staging(
    repo_path: &Path,
    msg: &str,
    staging: &StagingConfig,
    cluster_paths: &[PathBuf],
    commit_config: &CommitConfig,
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
                let include_globs = build_globset(&staging.include)?;
                let paths: Vec<PathBuf> = repo::changed_paths(repo_path)?
                    .into_iter()
                    .filter(|path| include_globs.is_match(path))
                    .collect();
                add_paths(repo_path, &paths)?;
            }
        }
    }

    unstage_excluded(repo_path, &staging.exclude)?;

    if commit_config.sign {
        repo::commit_signed(repo_path, msg, commit_config.gpg_key_id.as_deref())?;
    } else {
        repo::run_git(repo_path, &["commit", "-m", msg])?;
    }

    Ok(())
}

fn add_paths(repo_path: &Path, paths: &[PathBuf]) -> anyhow::Result<()> {
    for path in paths {
        let full_path = repo_path.join(path);
        // Skip transient paths that no longer exist (e.g. git tmp objects).
        if !full_path.exists() {
            continue;
        }
        // Skip paths ignored by git (e.g. build outputs, caches, .git internals).
        if is_ignored(repo_path, path) {
            continue;
        }
        repo::run_git(repo_path, &["add", "--", &path.to_string_lossy()])?;
    }
    Ok(())
}

/// Returns true if git considers the path ignored.
/// `git check-ignore` exits 0 for ignored, 1 for not ignored, 128 on error.
fn is_ignored(repo_path: &Path, path: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["check-ignore", "-q", "--"])
        .arg(path)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn unstage_excluded(repo_path: &Path, excludes: &[String]) -> anyhow::Result<()> {
    // User-supplied excludes (recursive-expanded) plus a hardcoded secret
    // denylist that cannot be turned off from config. Both are matched against
    // the repo-relative changed paths.
    let mut patterns = expand_recursive(excludes);
    let deny: Vec<String> = SECRET_DENYLIST.iter().map(|s| s.to_string()).collect();
    patterns.extend(expand_recursive(&deny));
    let globset = build_globset(&patterns)?;

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

fn build_globset(patterns: &[String]) -> anyhow::Result<GlobSet> {
    // Fail closed: an invalid pattern aborts the commit rather than being
    // silently dropped (which could stage a secret).
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .map_err(|e| anyhow::anyhow!("invalid staging glob {pattern:?}: {e}"))?;
        builder.add(glob);
    }
    Ok(builder.build()?)
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

        commit_with_staging(
            repo.path(),
            "all mode",
            &staging,
            &[],
            &CommitConfig::default(),
        )
        .unwrap();

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
            &CommitConfig::default(),
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

        commit_with_staging(
            repo.path(),
            "pattern mode",
            &staging,
            &[],
            &CommitConfig::default(),
        )
        .unwrap();

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

        let err = commit_with_staging(
            repo.path(),
            "nothing",
            &staging,
            &[],
            &CommitConfig::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("git commit"));
    }

    #[test]
    fn cluster_mode_skips_ignored_paths() {
        let repo = TestRepo::new();
        repo.write(".gitignore", "ignored/\n");
        repo.run(&["add", ".gitignore"]);
        repo.run(&["commit", "-m", "add gitignore"]);

        repo.write("src/a.rs", "changed");
        repo.write("ignored/b.txt", "changed");

        let staging = StagingConfig {
            mode: StagingMode::Cluster,
            include: vec![],
            exclude: vec![],
        };

        commit_with_staging(
            repo.path(),
            "cluster mode skips ignored",
            &staging,
            &[PathBuf::from("src/a.rs"), PathBuf::from("ignored/b.txt")],
            &CommitConfig::default(),
        )
        .unwrap();

        assert_eq!(repo.last_commit_files(), vec!["src/a.rs"]);
    }

    #[test]
    fn cluster_mode_skips_missing_paths() {
        let repo = TestRepo::new();
        repo.write("src/a.rs", "changed");

        let staging = StagingConfig {
            mode: StagingMode::Cluster,
            include: vec![],
            exclude: vec![],
        };

        commit_with_staging(
            repo.path(),
            "cluster mode skips missing",
            &staging,
            &[
                PathBuf::from("src/a.rs"),
                PathBuf::from("does/not/exist.tmp"),
            ],
            &CommitConfig::default(),
        )
        .unwrap();

        assert_eq!(repo.last_commit_files(), vec!["src/a.rs"]);
    }

    #[test]
    fn signing_enabled_attempts_gpg_signature() {
        let repo = TestRepo::new();
        repo.write("src/a.rs", "changed");

        let commit_config = CommitConfig {
            sign: true,
            gpg_key_id: None,
        };

        assert_signing_attempted_or_succeeded(repo.path(), &commit_config);
    }

    #[test]
    fn signing_with_key_id_attempts_gpg_signature() {
        let repo = TestRepo::new();
        repo.write("src/a.rs", "changed");

        let commit_config = CommitConfig {
            sign: true,
            gpg_key_id: Some("test@example.com".to_string()),
        };

        assert_signing_attempted_or_succeeded(repo.path(), &commit_config);
    }

    fn assert_signing_attempted_or_succeeded(repo_path: &Path, commit_config: &CommitConfig) {
        let result = commit_with_staging(
            repo_path,
            "signed commit",
            &StagingConfig::default(),
            &[],
            commit_config,
        );

        match result {
            Ok(()) => {
                // If signing succeeded, the commit object must contain a GPG signature.
                let output = std::process::Command::new("git")
                    .arg("-C")
                    .arg(repo_path)
                    .args(["cat-file", "commit", "HEAD"])
                    .output()
                    .unwrap();
                let commit_obj = String::from_utf8_lossy(&output.stdout).to_lowercase();
                assert!(
                    commit_obj.contains("gpgsig") || commit_obj.contains("begin pgp signature"),
                    "signed commit did not produce a gpgsig header"
                );
            }
            Err(err) => {
                // In environments without a usable GPG key, the commit fails with a
                // GPG-related error, which proves the -S flag was passed.
                let text = err.to_string().to_lowercase();
                assert!(
                    text.contains("gpg") || text.contains("pgp") || text.contains("sign"),
                    "expected a GPG-related failure, got: {}",
                    err
                );
            }
        }
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
