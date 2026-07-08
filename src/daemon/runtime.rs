use crate::config::Config;
use crate::daemon::health::{start_health_server, DaemonEvent, HealthState, Metrics};
use crate::daemon::notification::{notify_start, notify_stop};
use anyhow::Context;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};

fn warn_if_git_lock_exists(repo_path: &Path) {
    let lock = repo_path.join(".git/index.lock");
    if lock.exists() {
        tracing::warn!(
            path = %lock.display(),
            "git index lock exists; leaving it untouched to avoid interrupting an active git operation"
        );
    }
}

pub async fn start(config: Config) -> anyhow::Result<()> {
    // Initialize trace database
    crate::aoc::db::init_db(&config.repo_path)?;

    warn_if_git_lock_exists(&config.repo_path);

    if config.air_gapped {
        tracing::warn!("air-gapped mode enabled: push, webhooks, and bundle scoring are disabled");
    }

    if !config.capabilities.network_push {
        tracing::info!("capability network_push is disabled");
    }
    if !config.capabilities.network_webhooks {
        tracing::info!("capability network_webhooks is disabled");
    }
    if !config.capabilities.network_inference {
        tracing::info!("capability network_inference is disabled");
    }
    if !config.capabilities.bundle_scoring {
        tracing::info!("capability bundle_scoring is disabled");
    }
    if !config.capabilities.external_plugins {
        tracing::info!("capability external_plugins is disabled");
    }

    let metrics = Arc::new(Metrics::default());

    let (event_tx, _event_rx) = tokio::sync::broadcast::channel::<DaemonEvent>(256);

    // Create shutdown channel early so all subsystems can share it.
    let (shutdown_handle, mut shutdown_token) = crate::daemon::shutdown::channel();

    // Start optional Shark Stating task and wait for leadership before running scheduler.
    let shark_runtime: Option<Arc<crate::daemon::shark::SharkRuntime>> = if config.shark.enabled {
        tracing::info!("shark stating enabled; waiting for leadership");
        let (runtime, mut leader_rx) = crate::daemon::shark::start_shark_task(
            config.clone(),
            shutdown_token.clone_token(),
            Some(event_tx.clone()),
            Some(metrics.clone()),
        )
        .await
        .context("failed to start shark stating task")?;
        let runtime = Arc::new(runtime);

        // Block until this instance becomes leader or shutdown is requested.
        let leader_or_shutdown = async {
            loop {
                if *leader_rx.borrow() {
                    return true;
                }
                tokio::select! {
                    changed = leader_rx.changed() => {
                        if changed.is_err() {
                            return false;
                        }
                    }
                    _ = shutdown_token.wait() => {
                        return false;
                    }
                }
            }
        };

        if leader_or_shutdown.await {
            tracing::info!("acquired shark leadership; starting scheduler");
        } else {
            tracing::info!("shutdown requested while waiting for leadership; exiting");
            return Ok(());
        }

        // Watch for leadership loss and trigger shutdown if we lose the lease.
        let shutdown_handle_clone = shutdown_handle.clone();
        tokio::spawn(watch_leadership_loss(leader_rx, shutdown_handle_clone));

        Some(runtime)
    } else {
        None
    };

    notify_start(
        &config.notify,
        &config.repo_path,
        config.capabilities.network_webhooks,
    );

    // Spawn health endpoint
    let health_state = HealthState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        repo_path: config.repo_path.clone(),
        metrics: metrics.clone(),
        event_tx: event_tx.clone(),
        shark: shark_runtime.clone(),
    };
    tokio::spawn(start_health_server(config.health_port, health_state));

    // Spawn scheduled pruning task
    let repo_path = config.repo_path.clone();
    let prune_interval = Duration::from_secs(config.prune_interval_minutes * 60);
    let retention_days = config.retention_days;
    let prune_metrics = metrics.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(prune_interval);
        interval.tick().await; // first tick fires immediately
        loop {
            interval.tick().await;
            let result =
                crate::daemon::prune::prune_analysis_artifacts(&repo_path, retention_days).await;
            tracing::info!(
                deleted = result.deleted,
                errors = result.errors,
                "pruned analysis artifacts"
            );
            prune_metrics
                .artifacts_pruned
                .fetch_add(result.deleted, Ordering::Relaxed);
        }
    });

    let (tx, rx) = tokio::sync::mpsc::channel(1000);
    let atomic_shutdown = Arc::new(AtomicBool::new(false));

    let watcher_handle =
        crate::watcher::fs::start(tx.clone(), config.watch.clone(), atomic_shutdown.clone())?;

    // Spawn optional automated storage management task
    if config.deckhand.enabled {
        tracing::info!(
            interval_minutes = config.deckhand.interval_minutes,
            "deckhand storage management enabled"
        );
        tokio::spawn(crate::daemon::deckhand::start_storage_task(
            config.clone(),
            shutdown_token.clone_token(),
            metrics.clone(),
        ));
    }

    let scheduler = tokio::spawn(crate::daemon::scheduler::run(
        rx,
        config.clone(),
        shutdown_token,
        metrics.clone(),
        event_tx,
    ));
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
    if !scheduler.is_finished()
        && tokio::time::timeout(shutdown_timeout, &mut scheduler)
            .await
            .is_err()
    {
        tracing::error!(
            "scheduler shutdown timeout ({:?}), forcing exit",
            shutdown_timeout
        );
    }

    tokio::task::spawn_blocking(move || crate::watcher::fs::join(watcher_handle))
        .await
        .context("watcher join task failed")??;

    notify_stop(
        &config.notify,
        &config.repo_path,
        config.capabilities.network_webhooks,
    );

    Ok(())
}

async fn watch_leadership_loss(
    mut leader_rx: tokio::sync::watch::Receiver<bool>,
    shutdown_handle: crate::daemon::shutdown::ShutdownHandle,
) {
    loop {
        if leader_rx.changed().await.is_err() {
            break;
        }
        if !*leader_rx.borrow() {
            tracing::warn!("leadership lost; initiating graceful shutdown");
            shutdown_handle.signal();
            break;
        }
    }
}
