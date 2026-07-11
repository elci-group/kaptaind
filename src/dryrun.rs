//! `kaptaind --dry-run`: the full decision pipeline minus staging/commit (C4).
//!
//! Runs the same one-shot path as `kaptaind-cli analyze` over the current
//! pending changes, then prints the full decision the daemon *would* make:
//! bump, next version, and the exact deterministic commit message.

use crate::config::Config;
use anyhow::Context;
use chrono::Utc;

pub fn run(config: &Config) -> anyhow::Result<()> {
    let repo = crate::git::repo::Repo::open(&config.repo_path).with_context(|| {
        format!(
            "could not open git repository at {}",
            config.repo_path.display()
        )
    })?;
    let repo_ctx = crate::git::repo::RepoContext::new(repo.root(), &config.repo_path);
    // Status paths are git-root-relative; scope to the project and re-anchor
    // them the same way the scheduler's startup rescan does.
    let paths: Vec<std::path::PathBuf> = repo
        .changed_paths()?
        .into_iter()
        .filter_map(|p| repo_ctx.to_project_relative(&p))
        .map(|rel| config.repo_path.join(rel))
        .collect();

    if paths.is_empty() {
        println!("Working tree is clean. Nothing would be committed.");
        return Ok(());
    }

    let timestamp = Utc::now();
    let cluster = crate::cluster::engine::Cluster {
        id: uuid::Uuid::new_v4(),
        started_at: timestamp,
        ended_at: timestamp,
        events: vec![crate::watcher::FsEvent {
            paths,
            kind: crate::watcher::FsEventKind::Modify,
            timestamp,
        }],
    };

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

    let previous = crate::version::resolve_baseline(&config.repo_path)?;
    let next = crate::version::apply(previous, bump);
    let message =
        crate::commit::message::format_commit(&cluster, &diff, &weight, bump, &next, &None);

    println!("decision: commit");
    println!("bump: {bump:?}");
    println!("next version: v{next}");
    println!("commit message:");
    println!("{message}");
    Ok(())
}
