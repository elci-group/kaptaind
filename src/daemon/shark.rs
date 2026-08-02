use crate::config::loader::{Config, SharkMode};
use crate::daemon::health::{DaemonEvent, Metrics};
use crate::util::file_lock::FileLockExt;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::Instrument;

/// A held leadership lease.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lease {
    pub instance_id: String,
    pub acquired_at: DateTime<Utc>,
    pub renewed_at: DateTime<Utc>,
    pub ttl_ms: u64,
}

impl Lease {
    pub fn is_expired(&self) -> bool {
        Utc::now()
            .signed_duration_since(self.renewed_at)
            .num_milliseconds()
            > self.ttl_ms as i64
    }
}

/// Runtime role of this kaptaind instance in the Shark Stating topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceRole {
    /// Not leader; monitoring the current leader.
    Standby,
    /// Actively holding the lease and running the scheduler.
    Leader,
    /// Attempting to acquire leadership.
    Candidate,
    /// Releasing leadership and shutting down (upgrade scenario).
    Retiring,
    /// Monitoring only; never participates in leadership.
    Observer,
}

impl fmt::Display for InstanceRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceRole::Standby => write!(f, "standby"),
            InstanceRole::Leader => write!(f, "leader"),
            InstanceRole::Candidate => write!(f, "candidate"),
            InstanceRole::Retiring => write!(f, "retiring"),
            InstanceRole::Observer => write!(f, "observer"),
        }
    }
}

pub struct AtomicRole {
    inner: AtomicU8,
}

impl Default for AtomicRole {
    fn default() -> Self {
        Self {
            inner: AtomicU8::new(0),
        }
    }
}

const ROLE_STANDBY: u8 = 0;
const ROLE_LEADER: u8 = 1;
const ROLE_CANDIDATE: u8 = 2;
const ROLE_RETIRING: u8 = 3;
const ROLE_OBSERVER: u8 = 4;

impl AtomicRole {
    pub fn load(&self) -> InstanceRole {
        // SeqCst ensures role changes are immediately visible to all threads,
        // including the health endpoint and leadership-loss watcher.
        match self.inner.load(Ordering::SeqCst) {
            ROLE_LEADER => InstanceRole::Leader,
            ROLE_CANDIDATE => InstanceRole::Candidate,
            ROLE_RETIRING => InstanceRole::Retiring,
            ROLE_OBSERVER => InstanceRole::Observer,
            _ => InstanceRole::Standby,
        }
    }

    pub fn store(&self, role: InstanceRole) {
        let value = match role {
            InstanceRole::Standby => ROLE_STANDBY,
            InstanceRole::Leader => ROLE_LEADER,
            InstanceRole::Candidate => ROLE_CANDIDATE,
            InstanceRole::Retiring => ROLE_RETIRING,
            InstanceRole::Observer => ROLE_OBSERVER,
        };
        self.inner.store(value, Ordering::SeqCst);
    }
}

/// Authority layer: decides which instance may lead.
pub trait Arbiter: Send + Sync {
    /// Directory used by this arbiter for shared state.
    fn dir(&self) -> &Path;
    /// Try to acquire leadership. Returns true if this instance now holds the lease.
    fn try_acquire(&self, instance_id: &str, ttl_ms: u64) -> Result<bool>;
    /// Renew leadership. Returns true if renewal succeeded.
    fn renew(&self, instance_id: &str, ttl_ms: u64) -> Result<bool>;
    /// Voluntarily release leadership.
    fn release(&self, instance_id: &str) -> Result<()>;
    /// Read the current lease, if any.
    fn current_lease(&self) -> Result<Option<Lease>>;
}

/// File-based arbiter using atomic writes + advisory locking in a shared directory.
///
/// Advisory locking (`flock`) prevents split-brain when multiple instances run on
/// the same host and share the arbiter path. The lock is held only for the
/// duration of the read-modify-write operation.
pub struct FileArbiter {
    dir: PathBuf,
    lease_path: PathBuf,
    lock_path: PathBuf,
}

impl FileArbiter {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let dir = path.into();
        std::fs::create_dir_all(&dir)?;
        let lease_path = dir.join("lease.json");
        let lock_path = dir.join("lease.lock");
        // Ensure the lock file exists; holding the lock does not require it to be non-empty.
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock_path)?;
        Ok(Self {
            dir,
            lease_path,
            lock_path,
        })
    }

    fn read_lease(&self) -> Result<Option<Lease>> {
        if !self.lease_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&self.lease_path)?;
        let lease: Lease = serde_json::from_str(&content)?;
        Ok(Some(lease))
    }

    fn write_lease(&self, lease: &Lease) -> Result<()> {
        let tmp = self.lease_path.with_extension("tmp");
        let content = serde_json::to_string_pretty(lease)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &self.lease_path)?;
        Ok(())
    }

    /// Acquire an exclusive advisory lock and run the closure.
    /// The lock is released when the returned guard is dropped.
    fn with_lock<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.lock_path)
            .with_context(|| format!("failed to open lock file {}", self.lock_path.display()))?;
        file.lock_exclusive().with_context(|| {
            format!(
                "failed to acquire exclusive lock on {}",
                self.lock_path.display()
            )
        })?;
        let result = f();
        // Best-effort unlock; the OS will release the lock when the file is closed anyway.
        if let Err(error) = file.unlock() {
            tracing::warn!(
                ?error,
                operation = "with_lock",
                source_line = line!(),
                "best-effort operation failed"
            );
        }
        result
    }
}

impl Arbiter for FileArbiter {
    fn dir(&self) -> &Path {
        &self.dir
    }

    fn try_acquire(&self, instance_id: &str, ttl_ms: u64) -> Result<bool> {
        self.with_lock(|| {
            if let Some(lease) = self.read_lease()? {
                if !lease.is_expired() {
                    return Ok(false);
                }
            }
            let now = Utc::now();
            let lease = Lease {
                instance_id: instance_id.to_string(),
                acquired_at: now,
                renewed_at: now,
                ttl_ms,
            };
            self.write_lease(&lease)?;
            Ok(true)
        })
    }

    fn renew(&self, instance_id: &str, ttl_ms: u64) -> Result<bool> {
        self.with_lock(|| {
            let Some(lease) = self.read_lease()? else {
                return Ok(false);
            };
            if lease.instance_id != instance_id {
                return Ok(false);
            }
            let now = Utc::now();
            let new_lease = Lease {
                instance_id: instance_id.to_string(),
                acquired_at: lease.acquired_at,
                renewed_at: now,
                ttl_ms,
            };
            self.write_lease(&new_lease)?;
            Ok(true)
        })
    }

    fn release(&self, instance_id: &str) -> Result<()> {
        self.with_lock(|| {
            if let Some(lease) = self.read_lease()? {
                if lease.instance_id == instance_id {
                    if let Err(error) = std::fs::remove_file(&self.lease_path) {
                        tracing::warn!(
                            ?error,
                            operation = "release",
                            source_line = line!(),
                            "best-effort operation failed"
                        );
                    }
                }
            }
            Ok(())
        })
    }

    fn current_lease(&self) -> Result<Option<Lease>> {
        // Lease reads are also protected by the advisory lock so that a concurrent
        // write does not leave readers with a torn or missing file.
        self.with_lock(|| self.read_lease())
    }
}

/// Shared runtime state exposed to the rest of the daemon.
pub struct SharkRuntime {
    pub role: Arc<AtomicRole>,
    pub instance_id: String,
    pub arbiter: Arc<dyn Arbiter>,
    /// True while this instance is performing a voluntary upgrade handoff.
    pub upgrade_in_progress: Arc<AtomicBool>,
    /// When the current upgrade handoff started, if any.
    pub upgrade_started_at: Arc<Mutex<Option<DateTime<Utc>>>>,
}

impl SharkRuntime {
    pub fn new(config: &Config) -> Result<Self> {
        let arbiter = Arc::new(FileArbiter::new(config.shark_arbiter_path())?);
        Ok(Self {
            role: Arc::new(AtomicRole::default()),
            instance_id: config.shark_instance_id(),
            arbiter,
            upgrade_in_progress: Arc::new(AtomicBool::new(false)),
            upgrade_started_at: Arc::new(Mutex::new(None)),
        })
    }

    pub fn current_role(&self) -> InstanceRole {
        self.role.load()
    }

    pub fn current_lease(&self) -> Result<Option<Lease>> {
        self.arbiter.current_lease()
    }
}

fn emit_event(
    event_tx: &Option<broadcast::Sender<DaemonEvent>>,
    event_type: &str,
    payload: serde_json::Value,
) {
    if let Some(tx) = event_tx {
        let _ = tx.send(DaemonEvent {
            event_type: event_type.to_string(),
            payload,
        });
    }
}

fn inc_metric(
    metrics: &Option<Arc<Metrics>>,
    counter: fn(&Metrics) -> &std::sync::atomic::AtomicUsize,
) {
    if let Some(m) = metrics {
        counter(m).fetch_add(1, Ordering::Relaxed);
    }
}

/// Run a fallible operation with exponential backoff and jitter.
///
/// Returns the result of `op` on success, or the last error after `max_retries`.
async fn with_backoff<T, F, Fut>(mut op: F, max_retries: u32, base_delay: Duration) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_err = None;
    for attempt in 0..max_retries {
        match op().await {
            Ok(v) => return Ok(v),
            Err(err) => {
                tracing::warn!(error = %err, attempt, "shark operation failed, retrying");
                last_err = Some(err);
                let jitter = rand::random::<u64>() % 100;
                let delay = base_delay
                    .mul_f32(2f32.powi(attempt as i32))
                    .saturating_add(Duration::from_millis(jitter));
                tokio::time::sleep(delay).await;
            }
        }
    }
    let error = last_err.unwrap_or_else(|| anyhow::anyhow!("shark operation exhausted retries"));
    tracing::error!(
        ?error,
        max_retries,
        component = module_path!(),
        "shark operation exhausted its retry budget"
    );
    Err(error)
}

/// Start the Shark Stating task.
///
/// Returns a watch receiver that is `true` when this instance holds leadership.
/// If leadership is lost, the receiver flips to `false` and the caller should
/// initiate graceful shutdown.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn start_shark_task(
    config: Config,
    mut shutdown: crate::daemon::shutdown::ShutdownToken,
    event_tx: Option<broadcast::Sender<DaemonEvent>>,
    metrics: Option<Arc<Metrics>>,
) -> Result<(SharkRuntime, tokio::sync::watch::Receiver<bool>)> {
    let runtime = SharkRuntime::new(&config)?;
    let instance_id = runtime.instance_id.clone();
    let role = runtime.role.clone();
    let role_for_init = runtime.role.clone();
    let arbiter = runtime.arbiter.clone();
    let upgrade_in_progress = runtime.upgrade_in_progress.clone();
    let upgrade_started_at = runtime.upgrade_started_at.clone();

    let heartbeat = Duration::from_millis(config.shark.heartbeat_interval_ms.max(100));
    let ttl_ms = config
        .shark
        .lease_ttl_ms
        .max(heartbeat.as_millis() as u64 * 3);
    let timeout = Duration::from_millis(config.shark.heartbeat_timeout_ms.max(500));

    let initial_mode = config.shark.mode.clone();
    let observer = matches!(initial_mode, SharkMode::Observer);
    let force_leader = matches!(initial_mode, SharkMode::Leader);

    let (tx, rx) = tokio::sync::watch::channel(false);
    let tx_clone = tx.clone();

    let event_tx_clone = event_tx.clone();
    let metrics_clone = metrics.clone();

    let shark_task = async move {
        let mut interval = tokio::time::interval(heartbeat);
        let mut retire_marker: Option<RetireMarker> = None;
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        if observer {
            role.store(InstanceRole::Observer);
            if let Err(error) = tx_clone.send(false) {
                tracing::warn!(
                    ?error,
                    operation = "start_shark_task",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
            emit_event(
                &event_tx_clone,
                "shark.observer",
                serde_json::json!({"instance_id": instance_id}),
            );
        } else {
            role.store(InstanceRole::Standby);
            if let Err(error) = tx_clone.send(false) {
                tracing::warn!(
                    ?error,
                    operation = "start_shark_task",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
        }

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if observer {
                        continue;
                    }

                    // Check for voluntary retire request (upgrade flow).
                    if retire_marker.is_none() {
                        match check_retire_marker(&arbiter, &instance_id).await {
                            Ok(Some(marker)) => {
                                tracing::info!(component = module_path!(), "retire marker found; entering retiring state");
                                role.store(InstanceRole::Retiring);
                                upgrade_in_progress.store(true, Ordering::SeqCst);
                                *upgrade_started_at
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner()) = Some(Utc::now());
                                emit_event(
                                    &event_tx_clone,
                                    "shark.retire_marked",
                                    serde_json::json!({"instance_id": instance_id}),
                                );
                                retire_marker = Some(marker);
                            }
                            Ok(None) => {}
                            Err(err) => {
                                tracing::warn!(error = %err, "failed to check retire marker");
                            }
                        }
                    }
                    if role.load() == InstanceRole::Retiring {
                        const ROLLBACK_TIMEOUT: Duration = Duration::from_secs(15);

                        // Verify the standby is healthy before releasing leadership.
                        let standby_healthy =
                            match retire_marker.as_ref().and_then(|m| m.standby_health_port) {
                                Some(port) => {
                                    match wait_for_standby_ready(port, ROLLBACK_TIMEOUT).await {
                                        Ok(()) => true,
                                        Err(err) => {
                                            tracing::error!(error = %err, "standby failed health checks; rolling back upgrade");
                                            false
                                        }
                                    }
                                }
                                None => true,
                            };

                        if !standby_healthy {
                            rollback_upgrade(
                                arbiter.clone(),
                                instance_id.clone(),
                                ttl_ms,
                                role.clone(),
                                tx_clone.clone(),
                                event_tx_clone.clone(),
                                upgrade_in_progress.clone(),
                                upgrade_started_at.clone(),
                                "standby health check failed".to_string(),
                            )
                            .await;
                            continue;
                        }

                        tracing::info!(component = module_path!(), "standby is healthy; releasing leadership for upgrade");
                        if let Err(err) = with_backoff(
                            || async { arbiter.release(&instance_id) },
                            3,
                            Duration::from_millis(50),
                        )
                        .await
                        {
                            tracing::error!(error = %err, "failed to release leadership during upgrade; rolling back");
                            rollback_upgrade(
                                arbiter.clone(),
                                instance_id.clone(),
                                ttl_ms,
                                role.clone(),
                                tx_clone.clone(),
                                event_tx_clone.clone(),
                                upgrade_in_progress.clone(),
                                upgrade_started_at.clone(),
                                "failed to release leadership".to_string(),
                            )
                            .await;
                            continue;
                        }
                        if let Err(error) = tx_clone.send(false) {
                            tracing::warn!(?error, operation = "start_shark_task", source_line = line!(), "best-effort operation failed");
                        }
                        emit_event(
                            &event_tx_clone,
                            "shark.retired",
                            serde_json::json!({"instance_id": instance_id}),
                        );

                        // Wait for the standby to acquire leadership.
                        let handoff_timeout =
                            Duration::from_millis(config.shark.upgrade_handoff_timeout_ms);
                        match wait_for_lease_change(&arbiter, &instance_id, handoff_timeout).await {
                            Ok(Some(lease)) => {
                                upgrade_in_progress.store(false, Ordering::SeqCst);
                                *upgrade_started_at
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner()) = None;
                                emit_event(
                                    &event_tx_clone,
                                    "shark.upgrade_complete",
                                    serde_json::json!({
                                        "instance_id": instance_id,
                                        "new_leader": lease.instance_id,
                                        "acquired_at": lease.acquired_at,
                                    }),
                                );
                                crate::audit::log_event(
                                    &config.repo_path,
                                    &instance_id,
                                    "shark.upgrade_complete",
                                    true,
                                    serde_json::json!({
                                        "new_leader": lease.instance_id,
                                        "standby_health_port": retire_marker
                                            .as_ref()
                                            .and_then(|m| m.standby_health_port),
                                    }),
                                );
                                break;
                            }
                            result @ (Ok(None) | Err(_)) => {
                                tracing::error!(
                                    ?result,
                                    operation = "leadership_handoff",
                                    "standby failed to acquire leadership"
                                );
                                rollback_upgrade(
                                    arbiter.clone(),
                                    instance_id.clone(),
                                    ttl_ms,
                                    role.clone(),
                                    tx_clone.clone(),
                                    event_tx_clone.clone(),
                                    upgrade_in_progress.clone(),
                                    upgrade_started_at.clone(),
                                    "standby did not acquire leadership".to_string(),
                                )
                                .await;
                                continue;
                            }
                        }
                    }

                    match role.load() {
                        InstanceRole::Standby | InstanceRole::Candidate => {
                            if force_leader {
                                role.store(InstanceRole::Candidate);
                            }

                            match with_backoff(
                                || async { arbiter.try_acquire(&instance_id, ttl_ms) },
                                3,
                                Duration::from_millis(50),
                            ).await {
                                Ok(true) => {
                                    tracing::info!(component = module_path!(), "acquired shark leadership");
                                    role.store(InstanceRole::Leader);
                                    if let Err(error) = tx_clone.send(true) {
                                        tracing::warn!(?error, operation = "start_shark_task", source_line = line!(), "best-effort operation failed");
                                    }
                                    inc_metric(&metrics_clone, |m| &m.shark_leadership_acquired);
                                    emit_event(
                                        &event_tx_clone,
                                        "shark.leader_acquired",
                                        serde_json::json!({"instance_id": instance_id, "ttl_ms": ttl_ms}),
                                    );
                                }
                                Ok(false) => {
                                    // Another instance holds the lease. Verify it is alive.
                                    match arbiter.current_lease() {
                                        Ok(Some(lease)) if !lease.is_expired() => {
                                            tracing::debug!(leader = %lease.instance_id, "leader alive");
                                        }
                                        _ => {
                                            tracing::warn!(component = module_path!(), "leader lease missing or expired; will retry");
                                            role.store(InstanceRole::Candidate);
                                        }
                                    }
                                }
                                Err(err) => {
                                    tracing::error!(error = %err, "failed to acquire leadership");
                                }
                            }
                        }
                        InstanceRole::Leader => {
                            match with_backoff(
                                || async { arbiter.renew(&instance_id, ttl_ms) },
                                3,
                                Duration::from_millis(50),
                            ).await {
                                Ok(true) => {
                                    tracing::trace!(component = module_path!(), "renewed shark leadership");
                                }
                                Ok(false) => {
                                    tracing::error!(component = module_path!(), "lost shark leadership");
                                    role.store(InstanceRole::Standby);
                                    if let Err(error) = tx_clone.send(false) {
                                        tracing::warn!(?error, operation = "start_shark_task", source_line = line!(), "best-effort operation failed");
                                    }
                                    inc_metric(&metrics_clone, |m| &m.shark_leadership_lost);
                                    emit_event(
                                        &event_tx_clone,
                                        "shark.leader_lost",
                                        serde_json::json!({"instance_id": instance_id}),
                                    );
                                }
                                Err(err) => {
                                    tracing::error!(error = %err, "failed to renew leadership");
                                }
                            }
                        }
                        InstanceRole::Retiring => {
                            // Should have been handled above; safety break.
                            break;
                        }
                        InstanceRole::Observer => {
                            // Never changes.
                        }
                    }
                }
                _ = shutdown.wait() => {
                    tracing::info!(component = module_path!(), "shark task received shutdown signal");
                    let _ = with_backoff(
                        || async { arbiter.release(&instance_id) },
                        3,
                        Duration::from_millis(50),
                    ).await;
                    role.store(InstanceRole::Standby);
                    if let Err(error) = tx_clone.send(false) {
                        tracing::warn!(?error, operation = "start_shark_task", source_line = line!(), "best-effort operation failed");
                    }
                    emit_event(
                        &event_tx_clone,
                        "shark.shutdown",
                        serde_json::json!({"instance_id": instance_id}),
                    );
                    break;
                }
            }
        }
    };
    let task = tokio::spawn(shark_task.in_current_span());

    // Wait briefly for the task to settle into an initial role before returning.
    let deadline = tokio::time::Instant::now() + timeout;
    let rx_for_init = rx.clone();
    while tokio::time::Instant::now() < deadline {
        if *rx_for_init.borrow() || matches!(role_for_init.load(), InstanceRole::Observer) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Keep the task alive by not awaiting it here; the runtime owns the handle.
    let ownership_task = async move {
        if let Err(error) = task.await {
            tracing::error!(%error, operation = "shark_runtime", "shark runtime task failed");
        }
    };
    tokio::spawn(ownership_task.in_current_span());

    Ok((runtime, rx))
}

async fn check_retire_marker(
    arbiter: &Arc<dyn Arbiter>,
    instance_id: &str,
) -> Result<Option<RetireMarker>> {
    let marker_path = arbiter.dir().join("retire.json");
    if !marker_path.exists() {
        return Ok(None);
    }
    let content = tokio::fs::read_to_string(&marker_path).await?;
    let marker: RetireMarker = serde_json::from_str(&content)?;
    if marker.instance_id == instance_id {
        Ok(Some(marker))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RetireMarker {
    instance_id: String,
    retired_at: DateTime<Utc>,
    #[serde(default)]
    standby_health_port: Option<u16>,
}

/// Request the named instance to retire. Used by the upgrade CLI flow.
pub fn request_retire(
    arbiter_path: impl Into<PathBuf>,
    instance_id: &str,
    standby_health_port: Option<u16>,
) -> Result<()> {
    let dir = arbiter_path.into();
    std::fs::create_dir_all(&dir)?;
    let marker = RetireMarker {
        instance_id: instance_id.to_string(),
        retired_at: Utc::now(),
        standby_health_port,
    };
    let path = dir.join("retire.json");
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&marker)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Cancel a previously written retire marker.
pub fn clear_retire_marker(arbiter_path: &Path, instance_id: &str) -> Result<()> {
    let path = arbiter_path.join("retire.json");
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        if let Ok(marker) = serde_json::from_str::<RetireMarker>(&content) {
            if marker.instance_id == instance_id {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// Cancel a previously requested retirement.
pub fn cancel_retire(arbiter_path: impl Into<PathBuf>, instance_id: &str) -> Result<()> {
    clear_retire_marker(&arbiter_path.into(), instance_id)
}

/// Cancel an in-progress upgrade by clearing the retire marker.
pub fn cancel_upgrade(arbiter_path: &Path, instance_id: &str) {
    if let Err(err) = clear_retire_marker(arbiter_path, instance_id) {
        tracing::warn!(error = %err, "failed to cancel upgrade");
    }
}

/// Wait until `predicate` returns true or timeout elapses.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn wait_for<F>(mut predicate: F, timeout: Duration) -> Result<()>
where
    F: FnMut() -> Result<bool>,
{
    let start = tokio::time::Instant::now();
    let interval = Duration::from_millis(250);
    while start.elapsed() < timeout {
        if predicate()? {
            return Ok(());
        }
        tokio::time::sleep(interval).await;
    }
    anyhow::bail!("timeout waiting for condition")
}

/// Spawn a standby instance for zero-downtime upgrade.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn spawn_standby(
    repo_path: &Path,
    binary_path: &Path,
    arbiter_path: &Path,
    health_port: Option<u16>,
) -> Result<std::process::Child> {
    if !binary_path.exists() {
        anyhow::bail!("upgrade binary does not exist: {}", binary_path.display());
    }

    let mut cmd = std::process::Command::new(binary_path);
    cmd.arg("--shark-mode")
        .arg("auto")
        .arg("--shark-arbiter")
        .arg(arbiter_path)
        .current_dir(repo_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    if let Some(port) = health_port {
        cmd.arg("--health-port").arg(port.to_string());
    }

    let child = cmd.spawn().with_context(|| {
        format!(
            "failed to spawn standby kaptaind from {}",
            binary_path.display()
        )
    })?;

    Ok(child)
}

/// Roll back an upgrade: clear the retire marker, attempt to reclaim leadership,
/// and emit a `shark.upgrade_rollback` event.
#[allow(clippy::too_many_arguments)]
async fn rollback_upgrade(
    arbiter: Arc<dyn Arbiter>,
    instance_id: String,
    ttl_ms: u64,
    role: Arc<AtomicRole>,
    tx: tokio::sync::watch::Sender<bool>,
    event_tx: Option<broadcast::Sender<DaemonEvent>>,
    upgrade_in_progress: Arc<AtomicBool>,
    upgrade_started_at: Arc<Mutex<Option<DateTime<Utc>>>>,
    reason: String,
) {
    if let Err(err) = clear_retire_marker(arbiter.dir(), &instance_id) {
        tracing::warn!(error = %err, "failed to clear retire marker during rollback");
    }

    // Attempt to reclaim leadership immediately so this instance can resume
    // service rather than waiting for the next heartbeat tick. If this instance
    // already holds the lease (the common rollback case), renew it; otherwise
    // try to acquire a fresh lease.
    let instance_id_for_retry = instance_id.clone();
    let reclaimed = with_backoff(
        move || {
            let arbiter = arbiter.clone();
            let instance_id = instance_id_for_retry.clone();
            async move {
                match arbiter.current_lease() {
                    Ok(Some(lease)) if lease.instance_id == instance_id && !lease.is_expired() => {
                        arbiter.renew(&instance_id, ttl_ms)
                    }
                    _ => arbiter.try_acquire(&instance_id, ttl_ms),
                }
            }
        },
        3,
        Duration::from_millis(50),
    )
    .await
    .unwrap_or(false);

    role.store(if reclaimed {
        InstanceRole::Leader
    } else {
        InstanceRole::Candidate
    });
    if let Err(error) = tx.send(reclaimed) {
        tracing::warn!(
            ?error,
            operation = "rollback_upgrade",
            source_line = line!(),
            "best-effort operation failed"
        );
    }
    upgrade_in_progress.store(false, Ordering::SeqCst);
    *upgrade_started_at
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    emit_event(
        &event_tx,
        "shark.upgrade_rollback",
        serde_json::json!({
            "instance_id": instance_id,
            "reason": reason,
            "reclaimed": reclaimed,
        }),
    );
}

/// Wait until a lease is held by an instance other than `instance_id`.
async fn wait_for_lease_change(
    arbiter: &Arc<dyn Arbiter>,
    instance_id: &str,
    timeout: Duration,
) -> Result<Option<Lease>> {
    let start = tokio::time::Instant::now();
    let interval = Duration::from_millis(250);
    while start.elapsed() < timeout {
        match arbiter.current_lease() {
            Ok(Some(lease)) if lease.instance_id != instance_id => return Ok(Some(lease)),
            _ => {}
        }
        tokio::time::sleep(interval).await;
    }
    anyhow::bail!("timeout waiting for leadership handoff")
}

/// Poll a kaptaind health endpoint until it returns `"status": "ok"`.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn wait_for_standby_ready(health_port: u16, timeout: Duration) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/health", health_port);
    let start = tokio::time::Instant::now();
    let interval = Duration::from_millis(250);
    while start.elapsed() < timeout {
        match reqwest::get(&url).await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if body.get("status").and_then(|v| v.as_str()) == Some("ok") {
                        return Ok(());
                    }
                }
            }
            _ => {}
        }
        tokio::time::sleep(interval).await;
    }
    anyhow::bail!("standby health endpoint did not become ready")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lease_expires_after_ttl() {
        let lease = Lease {
            instance_id: "a".to_string(),
            acquired_at: Utc::now(),
            renewed_at: Utc::now() - chrono::Duration::milliseconds(100),
            ttl_ms: 50,
        };
        assert!(lease.is_expired());
    }

    #[test]
    fn lease_is_alive_before_ttl() {
        let lease = Lease {
            instance_id: "a".to_string(),
            acquired_at: Utc::now(),
            renewed_at: Utc::now(),
            ttl_ms: 10_000,
        };
        assert!(!lease.is_expired());
    }

    #[test]
    fn file_arbiter_acquires_empty_lease() {
        let dir = tempdir().unwrap();
        let arbiter = FileArbiter::new(dir.path()).unwrap();
        assert!(arbiter.try_acquire("a", 5000).unwrap());
        let lease = arbiter.current_lease().unwrap().unwrap();
        assert_eq!(lease.instance_id, "a");
    }

    #[test]
    fn file_arbiter_rejects_second_acquirer() {
        let dir = tempdir().unwrap();
        let arbiter = FileArbiter::new(dir.path()).unwrap();
        assert!(arbiter.try_acquire("a", 5000).unwrap());
        assert!(!arbiter.try_acquire("b", 5000).unwrap());
    }

    #[test]
    fn file_arbiter_allows_takeover_after_expiry() {
        let dir = tempdir().unwrap();
        let arbiter = FileArbiter::new(dir.path()).unwrap();

        // Acquire with a lease that is already expired.
        let now = Utc::now();
        let expired = Lease {
            instance_id: "a".to_string(),
            acquired_at: now - chrono::Duration::seconds(10),
            renewed_at: now - chrono::Duration::seconds(10),
            ttl_ms: 100,
        };
        arbiter.write_lease(&expired).unwrap();

        assert!(arbiter.try_acquire("b", 5000).unwrap());
        let lease = arbiter.current_lease().unwrap().unwrap();
        assert_eq!(lease.instance_id, "b");
    }

    #[test]
    fn file_arbiter_renew_only_by_holder() {
        let dir = tempdir().unwrap();
        let arbiter = FileArbiter::new(dir.path()).unwrap();
        assert!(arbiter.try_acquire("a", 5000).unwrap());
        assert!(arbiter.renew("a", 5000).unwrap());
        assert!(!arbiter.renew("b", 5000).unwrap());
    }

    #[test]
    fn file_arbiter_release_only_by_holder() {
        let dir = tempdir().unwrap();
        let arbiter = FileArbiter::new(dir.path()).unwrap();
        assert!(arbiter.try_acquire("a", 5000).unwrap());
        arbiter.release("b").unwrap(); // no-op, not holder
        assert!(arbiter.current_lease().unwrap().is_some());
        arbiter.release("a").unwrap();
        assert!(arbiter.current_lease().unwrap().is_none());
    }

    #[test]
    fn atomic_role_roundtrips() {
        let role = AtomicRole::default();
        assert!(matches!(role.load(), InstanceRole::Standby));
        role.store(InstanceRole::Leader);
        assert!(matches!(role.load(), InstanceRole::Leader));
        role.store(InstanceRole::Retiring);
        assert!(matches!(role.load(), InstanceRole::Retiring));
    }

    #[test]
    fn retire_marker_roundtrips() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "old-instance", None).unwrap();
        let path = dir.path().join("retire.json");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let marker: RetireMarker = serde_json::from_str(&content).unwrap();
        assert_eq!(marker.instance_id, "old-instance");
        assert!(marker.standby_health_port.is_none());
    }

    #[test]
    fn retire_marker_preserves_health_port() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "old-instance", Some(9090)).unwrap();
        let content = std::fs::read_to_string(dir.path().join("retire.json")).unwrap();
        let marker: RetireMarker = serde_json::from_str(&content).unwrap();
        assert_eq!(marker.standby_health_port, Some(9090));
    }

    #[test]
    fn cancel_retire_removes_matching_marker() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "instance-a", None).unwrap();
        cancel_retire(dir.path(), "instance-a").unwrap();
        assert!(!dir.path().join("retire.json").exists());
    }

    #[test]
    fn cancel_retire_ignores_non_matching_marker() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "instance-a", None).unwrap();
        cancel_retire(dir.path(), "instance-b").unwrap();
        assert!(dir.path().join("retire.json").exists());
    }

    #[test]
    fn clear_retire_marker_removes_matching_marker() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "instance-a", None).unwrap();
        clear_retire_marker(dir.path(), "instance-a").unwrap();
        assert!(!dir.path().join("retire.json").exists());
    }

    #[test]
    fn clear_retire_marker_ignores_non_matching_marker() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "instance-a", None).unwrap();
        clear_retire_marker(dir.path(), "instance-b").unwrap();
        assert!(dir.path().join("retire.json").exists());
    }

    #[test]
    fn cancel_upgrade_clears_matching_marker() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "instance-a", None).unwrap();
        cancel_upgrade(dir.path(), "instance-a");
        assert!(!dir.path().join("retire.json").exists());
    }

    #[tokio::test]
    async fn concurrent_acquire_only_one_wins() {
        let dir = tempdir().unwrap();
        let arbiter = Arc::new(FileArbiter::new(dir.path()).unwrap());

        let mut handles = Vec::new();
        for i in 0..10 {
            let a = arbiter.clone();
            handles.push(tokio::task::spawn_blocking(move || {
                a.try_acquire(&format!("instance-{}", i), 5000)
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap().unwrap());
        }

        assert_eq!(results.iter().filter(|&&r| r).count(), 1);
        assert!(arbiter.current_lease().unwrap().is_some());
    }

    #[tokio::test]
    async fn rollback_upgrade_restores_leader_role() {
        let dir = tempdir().unwrap();
        let arbiter = Arc::new(FileArbiter::new(dir.path()).unwrap());
        let instance_id = "leader-a".to_string();
        assert!(arbiter.try_acquire(&instance_id, 5000).unwrap());
        arbiter.release(&instance_id).unwrap();

        request_retire(dir.path(), &instance_id, None).unwrap();

        let role = Arc::new(AtomicRole::default());
        role.store(InstanceRole::Retiring);
        let (tx, mut rx) = tokio::sync::watch::channel(false);
        let upgrade_in_progress = Arc::new(AtomicBool::new(true));
        let upgrade_started_at = Arc::new(Mutex::new(Some(Utc::now())));

        rollback_upgrade(
            arbiter.clone(),
            instance_id.clone(),
            5000,
            role.clone(),
            tx,
            None,
            upgrade_in_progress.clone(),
            upgrade_started_at.clone(),
            "test rollback".to_string(),
        )
        .await;

        assert!(matches!(role.load(), InstanceRole::Leader));
        assert!(*rx.borrow_and_update());
        assert!(!upgrade_in_progress.load(Ordering::SeqCst));
        assert!(upgrade_started_at.lock().unwrap().is_none());
        assert!(!dir.path().join("retire.json").exists());
        let lease = arbiter.current_lease().unwrap();
        assert_eq!(
            lease.as_ref().map(|l| l.instance_id.as_str()),
            Some("leader-a")
        );
    }

    #[tokio::test]
    async fn wait_for_lease_change_detects_new_leader() {
        let dir = tempdir().unwrap();
        let arbiter: Arc<dyn Arbiter> = Arc::new(FileArbiter::new(dir.path()).unwrap());
        assert!(arbiter.try_acquire("old", 5000).unwrap());

        // Start a background task that acquires the lease after a short delay.
        let arbiter_clone = arbiter.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            arbiter_clone.release("old").unwrap();
            assert!(arbiter_clone.try_acquire("new", 5000).unwrap());
        });

        let lease = wait_for_lease_change(&arbiter, "old", Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(lease.as_ref().map(|l| l.instance_id.as_str()), Some("new"));
    }

    #[tokio::test]
    async fn wait_for_standby_ready_fails_when_standby_unhealthy() {
        // Port 0 is invalid, so the health endpoint cannot succeed.
        let result = wait_for_standby_ready(0, Duration::from_millis(250)).await;
        assert!(result.is_err());
    }
}
