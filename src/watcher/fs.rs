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
            let _ = handle.join();
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
            let _ = tx.blocking_send(FsEvent::from(event));
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
            let _ = ready_tx.send(Ok(()));
        }
        Err(err) => {
            let _ = ready_tx.send(Err(err.to_string()));
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
        .map_err(|_| anyhow!("watcher thread panicked"))?;
    result.map_err(Into::into)
}
