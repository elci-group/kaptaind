use crate::config::Config;
use anyhow::Context;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub async fn start(config: Config) -> anyhow::Result<()> {
    // Initialize trace database
    crate::aoc::db::init_db(&config.repo_path)?;

    let (tx, rx) = tokio::sync::mpsc::channel(1000);
    let shutdown = Arc::new(AtomicBool::new(false));

    let watcher_handle = crate::watcher::fs::start(tx.clone(), config.watch.clone(), shutdown.clone())?;
    let scheduler = tokio::spawn(crate::daemon::scheduler::run(rx, config));
    tokio::pin!(scheduler);

    tokio::select! {
        result = &mut scheduler => {
            result.context("scheduler task join failed")?;
        }
        signal = tokio::signal::ctrl_c() => {
            signal?;
        }
    }

    shutdown.store(true, Ordering::Relaxed);
    drop(tx);

    if !scheduler.is_finished() {
        scheduler.await.context("scheduler shutdown failed")?;
    }

    tokio::task::spawn_blocking(move || crate::watcher::fs::join(watcher_handle))
        .await
        .context("watcher join task failed")??;

    Ok(())
}
