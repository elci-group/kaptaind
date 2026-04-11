use crate::config::Config;
use anyhow::Context;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};

fn cleanup_stale_git_lock(repo_path: &Path) {
    let lock = repo_path.join(".git/index.lock");
    if lock.exists() {
        if let Err(e) = std::fs::remove_file(&lock) {
            tracing::warn!(?e, "failed to remove stale git lock");
        }
    }
}

pub async fn start(config: Config) -> anyhow::Result<()> {
    // Initialize trace database
    crate::aoc::db::init_db(&config.repo_path)?;

    // Clean up stale .git/index.lock if present
    cleanup_stale_git_lock(&config.repo_path);

    let (tx, rx) = tokio::sync::mpsc::channel(1000);
    let atomic_shutdown = Arc::new(AtomicBool::new(false));

    let watcher_handle = crate::watcher::fs::start(tx.clone(), config.watch.clone(), atomic_shutdown.clone())?;

    // Create shutdown channel for graceful shutdown signal
    let (shutdown_handle, shutdown_token) = crate::daemon::shutdown::channel();

    let scheduler = tokio::spawn(crate::daemon::scheduler::run(rx, config.clone(), shutdown_token));
    tokio::pin!(scheduler);

    // Setup signal handlers: SIGINT and SIGTERM
    let mut sigterm = signal(SignalKind::terminate()).context("failed to setup SIGTERM handler")?;

    tokio::select! {
        result = &mut scheduler => {
            result.context("scheduler task join failed")?;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("SIGINT received, initiating graceful shutdown");
            shutdown_handle.signal();
        }
        _ = sigterm.recv() => {
            tracing::info!("SIGTERM received, initiating graceful shutdown");
            shutdown_handle.signal();
        }
    }

    atomic_shutdown.store(true, Ordering::Relaxed);
    drop(tx);

    // Wait for scheduler with a hard timeout
    let shutdown_timeout = Duration::from_secs(30);
    if !scheduler.is_finished() {
        if tokio::time::timeout(shutdown_timeout, &mut scheduler).await.is_err() {
            tracing::error!("scheduler shutdown timeout ({:?}), forcing exit", shutdown_timeout);
        }
    }

    tokio::task::spawn_blocking(move || crate::watcher::fs::join(watcher_handle))
        .await
        .context("watcher join task failed")??;

    Ok(())
}
