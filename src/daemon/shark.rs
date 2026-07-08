use crate::config::loader::{Config, SharkMode};
use crate::daemon::health::{DaemonEvent, Metrics};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

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
        let _ = file.unlock();
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
                    let _ = std::fs::remove_file(&self.lease_path);
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
}

impl SharkRuntime {
    pub fn new(config: &Config) -> Result<Self> {
        let arbiter = Arc::new(FileArbiter::new(config.shark_arbiter_path())?);
        Ok(Self {
            role: Arc::new(AtomicRole::default()),
            instance_id: config.shark_instance_id(),
            arbiter,
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
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("shark operation exhausted retries")))
}

/// Start the Shark Stating task.
///
/// Returns a watch receiver that is `true` when this instance holds leadership.
/// If leadership is lost, the receiver flips to `false` and the caller should
/// initiate graceful shutdown.
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

    let task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(heartbeat);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        if observer {
            role.store(InstanceRole::Observer);
            let _ = tx_clone.send(false);
            emit_event(
                &event_tx_clone,
                "shark.observer",
                serde_json::json!({"instance_id": instance_id}),
            );
        } else {
            role.store(InstanceRole::Standby);
            let _ = tx_clone.send(false);
        }

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if observer {
                        continue;
                    }

                    // Check for voluntary retire request (upgrade flow).
                    match check_retire_marker(&arbiter, &instance_id).await {
                        Ok(true) => {
                            tracing::info!("retire marker found; entering retiring state");
                            role.store(InstanceRole::Retiring);
                            emit_event(
                                &event_tx_clone,
                                "shark.retire_marked",
                                serde_json::json!({"instance_id": instance_id}),
                            );
                        }
                        Ok(false) => {}
                        Err(err) => {
                            tracing::warn!(error = %err, "failed to check retire marker");
                        }
                    }
                    if role.load() == InstanceRole::Retiring {
                        tracing::info!("retire marker found; releasing leadership");
                        let _ = with_backoff(
                            || async { arbiter.release(&instance_id) },
                            3,
                            Duration::from_millis(50),
                        ).await;
                        let _ = tx_clone.send(false);
                        emit_event(
                            &event_tx_clone,
                            "shark.retired",
                            serde_json::json!({"instance_id": instance_id}),
                        );
                        break;
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
                                    tracing::info!("acquired shark leadership");
                                    role.store(InstanceRole::Leader);
                                    let _ = tx_clone.send(true);
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
                                            tracing::warn!("leader lease missing or expired; will retry");
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
                                    tracing::trace!("renewed shark leadership");
                                }
                                Ok(false) => {
                                    tracing::error!("lost shark leadership");
                                    role.store(InstanceRole::Standby);
                                    let _ = tx_clone.send(false);
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
                    tracing::info!("shark task received shutdown signal");
                    let _ = with_backoff(
                        || async { arbiter.release(&instance_id) },
                        3,
                        Duration::from_millis(50),
                    ).await;
                    role.store(InstanceRole::Standby);
                    let _ = tx_clone.send(false);
                    emit_event(
                        &event_tx_clone,
                        "shark.shutdown",
                        serde_json::json!({"instance_id": instance_id}),
                    );
                    break;
                }
            }
        }
    });

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
    tokio::spawn(async move {
        let _ = task.await;
    });

    Ok((runtime, rx))
}

async fn check_retire_marker(arbiter: &Arc<dyn Arbiter>, instance_id: &str) -> Result<bool> {
    let marker_path = arbiter.dir().join("retire.json");
    if !marker_path.exists() {
        return Ok(false);
    }
    let content = tokio::fs::read_to_string(&marker_path).await?;
    let marker: RetireMarker = serde_json::from_str(&content)?;
    if marker.instance_id == instance_id {
        // Remove marker so we don't re-read it after restart.
        let _ = tokio::fs::remove_file(&marker_path).await;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RetireMarker {
    instance_id: String,
    retired_at: DateTime<Utc>,
}

/// Request the named instance to retire. Used by the upgrade CLI flow.
pub fn request_retire(arbiter_path: impl Into<PathBuf>, instance_id: &str) -> Result<()> {
    let dir = arbiter_path.into();
    std::fs::create_dir_all(&dir)?;
    let marker = RetireMarker {
        instance_id: instance_id.to_string(),
        retired_at: Utc::now(),
    };
    let path = dir.join("retire.json");
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&marker)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Cancel a previously requested retirement.
pub fn cancel_retire(arbiter_path: impl Into<PathBuf>, instance_id: &str) -> Result<()> {
    let dir = arbiter_path.into();
    let path = dir.join("retire.json");
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

/// Wait until `predicate` returns true or timeout elapses.
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

/// Poll a kaptaind health endpoint until it returns `"status": "ok"`.
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
        request_retire(dir.path(), "old-instance").unwrap();
        let path = dir.path().join("retire.json");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let marker: RetireMarker = serde_json::from_str(&content).unwrap();
        assert_eq!(marker.instance_id, "old-instance");
    }

    #[test]
    fn cancel_retire_removes_matching_marker() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "instance-a").unwrap();
        cancel_retire(dir.path(), "instance-a").unwrap();
        assert!(!dir.path().join("retire.json").exists());
    }

    #[test]
    fn cancel_retire_ignores_non_matching_marker() {
        let dir = tempdir().unwrap();
        request_retire(dir.path(), "instance-a").unwrap();
        cancel_retire(dir.path(), "instance-b").unwrap();
        assert!(dir.path().join("retire.json").exists());
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
}
