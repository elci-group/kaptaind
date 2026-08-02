use crate::config::loader::WatchConfig;
use crate::watcher::FsEvent;
use anyhow::{anyhow, Context};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc::Sender;

type WatchStartResult = Result<(), String>;

pub fn start(
    tx: Sender<FsEvent>,
    config: WatchConfig,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<thread::JoinHandle<notify::Result<()>>> {
    let (ready_tx, ready_rx) = mpsc::channel::<WatchStartResult>();

    let handle = thread::spawn(move || watch_loop(tx, config, shutdown, ready_tx));

    match ready_rx
        .recv()
        .context("watcher startup signal not received")?
    {
        Ok(()) => Ok(handle),
        Err(message) => {
            if let Err(error) = handle.join() {
                tracing::warn!(
                    ?error,
                    operation = "start",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
            tracing::error!(%message, operation = "watcher_start", "filesystem watcher failed to start");
            Err(anyhow!(message))
        }
    }
}

fn watch_loop(
    tx: Sender<FsEvent>,
    config: WatchConfig,
    shutdown: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<WatchStartResult>,
) -> notify::Result<()> {
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => {
            if let Err(error) = tx.blocking_send(FsEvent::from(event)) {
                tracing::warn!(
                    ?error,
                    operation = "watch_loop",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
        }
        Err(err) => {
            tracing::error!(error = %err, "watch event error");
        }
    })?;

    let recursive_mode = if config.recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };

    match watcher.watch(Path::new(&config.path), recursive_mode) {
        Ok(()) => {
            if let Err(error) = ready_tx.send(Ok(())) {
                tracing::warn!(
                    ?error,
                    operation = "watch_loop",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
        }
        Err(err) => {
            if let Err(error) = ready_tx.send(Err(err.to_string())) {
                tracing::warn!(
                    ?error,
                    operation = "watch_loop",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
            return Ok(());
        }
    }

    while !shutdown.load(Ordering::Relaxed) {
        thread::park_timeout(Duration::from_millis(100));
    }

    Ok(())
}

pub fn join(handle: thread::JoinHandle<notify::Result<()>>) -> anyhow::Result<()> {
    let result = handle
        .join()
        .map_err(|error| anyhow!("watcher thread panicked: {error:?}"))?;
    result.map_err(Into::into)
}
