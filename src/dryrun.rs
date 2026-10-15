//! `kaptaind --dry-run`: the full decision pipeline minus staging/commit (C4).
//!
//! Runs the same one-shot path as `kaptaind analyze` over the current
//! pending changes, then prints the full decision the daemon *would* make:
//! bump, next version, and the exact deterministic commit message.

use crate::config::Config;
use anyhow::Context;
use chrono::Utc;
use std::path::PathBuf;

/// Absolute paths of the project's current pending (uncommitted) changes.
///
/// Status paths are git-root-relative; they are scoped to the project and
/// re-anchored under `config.repo_path` the same way the scheduler's startup
/// rescan does. Shared by `--dry-run` and the one-shot `kaptaind commit`.
pub fn pending_paths(
    config: &Config,
    repo: &crate::git::repo::Repo,
) -> anyhow::Result<Vec<PathBuf>> {
    let repo_ctx = crate::git::repo::RepoContext::new(repo.root(), &config.repo_path);
    Ok(repo
        .changed_paths()?
        .into_iter()
        .filter_map(|p| repo_ctx.to_project_relative(&p))
        .map(|rel| config.repo_path.join(rel))
        .collect())
}

/// A single-event cluster over `paths`, stamped now — the same shape the
/// scheduler forms for its startup catch-up cluster.
pub fn synthetic_cluster(paths: Vec<PathBuf>) -> crate::cluster::engine::Cluster {
    let timestamp = Utc::now();
    crate::cluster::engine::Cluster {
        id: uuid::Uuid::new_v4(),
        started_at: timestamp,
        ended_at: timestamp,
        events: vec![crate::watcher::FsEvent {
            paths,
            kind: crate::watcher::FsEventKind::Modify,
            timestamp,
        }],
    }
}

pub fn run(config: &Config) -> anyhow::Result<()> {
    let repo = crate::git::repo::Repo::open(&config.repo_path).with_context(|| {
        format!(
            "could not open git repository at {}",
            config.repo_path.display()
        )
    })?;
    let paths = pending_paths(config, &repo)?;

    if paths.is_empty() {
        println!("Working tree is clean. Nothing would be committed.");
        return Ok(());
    }

    let cluster = synthetic_cluster(paths);

    let mut diff = crate::diff::analyze_with_plugins(&cluster, &config.repo_path, &config.plugins);
    if config.bundle.command.is_some() && config.capabilities.bundle_scoring {
        diff.bundle = crate::diff::bundle::bundle_score(&config.bundle, &config.repo_path).score;
    }
    let weight = crate::weight::compute(&diff, &config.weights);
    let bump = crate::version::decide(&weight, &config.version_thresholds);

    println!("kaptaind dry run — no files will be written, nothing committed");
    println!("paths analyzed: {}", diff.touched_paths);
    println!(
        "score: {:.3} (minor threshold {:.3}, patch threshold {:.3})",
        weight.score, config.version_thresholds.minor, config.version_thresholds.patch
    );

    if bump == crate::version::Bump::None {
        if config.commit.require_bump {
            println!("decision: skip (no_bump — score below patch threshold)");
        } else {
            let message =
                crate::commit::message::format_chore_commit(&cluster, &diff, &weight, &None);
            println!("decision: chore commit (require_bump = false — no version bump)");
            println!("commit message:");
            println!("{message}");
        }
        return Ok(());
    }

    let previous = if crate::version::bootstrap::baseline_missing(&config.repo_path) {
        let (seed, source) =
            crate::version::bootstrap::seed(&config.repo_path, &config.versioning)?;
        println!("baseline: none — first run would initialize VERSION to {seed} (from {source})");
        seed
    } else {
        crate::version::resolve_baseline(&config.repo_path)?
    };
    let next = crate::version::apply(previous, bump);
    // Mirror the scheduler's W2 member scope so the previewed message
    // matches what the daemon would commit.
    let cluster_paths: Vec<std::path::PathBuf> = cluster
        .events
        .iter()
        .flat_map(|e| e.paths.iter().cloned())
        .collect();
    let layout = crate::version::workspace::WorkspaceLayout::discover(&config.repo_path)
        .unwrap_or(crate::version::workspace::WorkspaceLayout::Single);
    let member_scope = if !matches!(layout, crate::version::workspace::WorkspaceLayout::Single)
        && !matches!(
            config.versioning.workspace,
            crate::config::loader::WorkspacePolicy::RootOnly
        ) {
        crate::version::workspace::WorkspaceLayout::dominant_member(
            &layout,
            &cluster_paths,
            &config.repo_path,
        )
        .map(|m| m.name.clone())
    } else {
        None
    };
    let message = crate::commit::message::format_commit(
        &cluster,
        &diff,
        &weight,
        bump,
        &next,
        &None,
        member_scope.as_deref(),
    );

    println!("decision: commit");
    println!("bump: {bump:?}");
    println!("next version: v{next}");
    println!("commit message:");
    println!("{message}");
    Ok(())
}
