use crate::config::loader::{StagingConfig, StagingMode};
use git2::Repository;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

/// Stage and commit with the default "all" strategy.
pub fn commit(repo: &Repository, msg: &str) -> Result<(), git2::Error> {
    commit_with_staging(repo, msg, &StagingConfig::default(), &[])
}

/// Stage and commit with configurable staging behavior.
pub fn commit_with_staging(
    repo: &Repository,
    msg: &str,
    staging: &StagingConfig,
    cluster_paths: &[PathBuf],
) -> Result<(), git2::Error> {
    let mut index = repo.index()?;

    match staging.mode {
        StagingMode::All => {
            // Existing behavior: stage everything, then remove excludes
            index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
            remove_excluded(&mut index, &staging.exclude, repo);
        }
        StagingMode::Cluster => {
            // Only stage files that were part of the detected cluster
            let repo_root = repo.workdir().unwrap_or(Path::new("."));
            for path in cluster_paths {
                let relative = path.strip_prefix(repo_root).unwrap_or(path);
                let _ = index.add_path(relative);
            }
            // Also stage VERSION file (always needed for version bumps)
            let _ = index.add_path(Path::new("VERSION"));
            let _ = index.add_path(Path::new("Cargo.toml"));
            remove_excluded(&mut index, &staging.exclude, repo);
        }
        StagingMode::Pattern => {
            // Stage only files matching include patterns
            if staging.include.is_empty() {
                // No includes specified, fall back to all
                index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
            } else {
                let include_globs = build_globset(&staging.include);
                index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, Some(&mut |path: &Path, _| {
                    if let Some(ref gs) = include_globs {
                        if gs.is_match(path) { 0 } else { 1 } // 0 = include, 1 = skip
                    } else {
                        0
                    }
                }))?;
            }
            remove_excluded(&mut index, &staging.exclude, repo);
        }
    }

    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let sig = repo.signature()?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    match parent {
        Some(head) => {
            repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&head])?;
        }
        None => {
            repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[])?;
        }
    }

    Ok(())
}

fn remove_excluded(index: &mut git2::Index, excludes: &[String], repo: &Repository) {
    if excludes.is_empty() {
        return;
    }
    let Some(gs) = build_globset(excludes) else { return };
    let repo_root = repo.workdir().unwrap_or(Path::new("."));

    // Collect paths to remove (can't modify index while iterating)
    let to_remove: Vec<PathBuf> = index
        .iter()
        .filter_map(|entry| {
            let path_str = String::from_utf8_lossy(&entry.path);
            let path = PathBuf::from(path_str.as_ref());
            if gs.is_match(&path) {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    for path in &to_remove {
        let _ = index.remove(path, 0);
        // Restore file from HEAD so it's not left unstaged
        let full_path = repo_root.join(path);
        if full_path.exists() {
            let _ = index.add_path(path);
            let _ = index.remove(path, 0);
        }
    }
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
