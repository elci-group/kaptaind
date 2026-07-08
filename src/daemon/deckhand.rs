use crate::config::loader::DeckhandConfig;
use crate::config::Config;
use crate::daemon::health::Metrics;
use crate::daemon::shutdown::ShutdownToken;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Result of one automated storage-management pass.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageReport {
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub bytes_freed: u64,
    pub files_removed: u64,
    pub errors: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
}

/// Start the background deckhand storage-management task.
pub async fn start_storage_task(
    config: Config,
    mut shutdown: ShutdownToken,
    metrics: Arc<Metrics>,
) {
    let interval = Duration::from_secs(config.deckhand.interval_minutes * 60);
    let repo_path = config.repo_path.clone();
    let deckhand_cfg = config.deckhand.clone();

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the immediate first tick so we don't clean right on startup.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let repo_path = repo_path.clone();
                let cfg = deckhand_cfg.clone();
                let metrics = metrics.clone();
                tokio::spawn(async move {
                    match run_storage_pass(&repo_path, &cfg).await {
                        Ok(report) => {
                            metrics.storage_cleaned_bytes.fetch_add(report.bytes_freed, Ordering::Relaxed);
                            metrics.storage_cleaned_files.fetch_add(report.files_removed, Ordering::Relaxed);
                            if report.bytes_freed > 0 || report.errors > 0 {
                                tracing::info!(
                                    bytes_freed = report.bytes_freed,
                                    files_removed = report.files_removed,
                                    errors = report.errors,
                                    duration_ms = report.duration_ms,
                                    "storage management pass complete"
                                );
                            } else {
                                tracing::debug!("storage management pass complete; nothing freed");
                            }
                            if let Err(err) = persist_report(&repo_path, &report) {
                                tracing::warn!(error = %err, "failed to persist storage report");
                            }
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "storage management pass failed");
                        }
                    }
                });
            }
            _ = shutdown.wait() => {
                tracing::info!("storage management task shutting down");
                break;
            }
        }
    }
}

/// Run a single storage-management pass synchronously, wrapped for async use.
pub async fn run_storage_pass(repo_path: &Path, cfg: &DeckhandConfig) -> Result<StorageReport> {
    let repo_path = repo_path.to_path_buf();
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || run_storage_pass_sync(&repo_path, &cfg))
        .await
        .context("storage pass panicked")?
}

fn run_storage_pass_sync(repo_path: &Path, cfg: &DeckhandConfig) -> Result<StorageReport> {
    let started_at = Utc::now();
    let start = Instant::now();
    let mut report = StorageReport {
        started_at,
        ..StorageReport::default()
    };

    if should_skip_due_to_free_space(repo_path, cfg.min_free_percent) {
        report.skipped_reason = Some(format!(
            "disk free space above {}% threshold",
            cfg.min_free_percent
        ));
        report.duration_ms = start.elapsed().as_millis() as u64;
        return Ok(report);
    }

    let watched_dirs = collect_watched_dirs(repo_path)?;
    let before = measure_dirs(&watched_dirs)?;

    let dh_cfg = build_deckhand_config(repo_path, cfg);

    // Clean each requested profile.
    for profile in &cfg.clean_profiles {
        if let Err(err) = deckhand::clean::run(
            &dh_cfg,
            profile,
            cfg.dry_run,
            cfg.clean_older_than_days,
            None,
        ) {
            tracing::warn!(profile = %profile, error = %err, "deckhand clean failed");
            report.errors += 1;
        }
    }

    // Sweep stale artifacts and caches.
    if let Err(err) = deckhand::sweep::run(&dh_cfg, repo_path, cfg.dry_run, cfg.sweep_keep_days) {
        tracing::warn!(error = %err, "deckhand sweep failed");
        report.errors += 1;
    }

    let after = measure_dirs(&watched_dirs)?;
    report.bytes_freed = before.saturating_sub(after);
    report.duration_ms = start.elapsed().as_millis() as u64;

    Ok(report)
}

fn should_skip_due_to_free_space(repo_path: &Path, min_free_percent: u64) -> bool {
    if min_free_percent == 0 {
        return false;
    }
    match (fs2::available_space(repo_path), fs2::total_space(repo_path)) {
        (Ok(available), Ok(total)) if total > 0 => {
            let free_percent = (available as f64 / total as f64 * 100.0) as u64;
            free_percent > min_free_percent
        }
        _ => {
            tracing::debug!("unable to determine disk free space; proceeding with storage pass");
            false
        }
    }
}

fn build_deckhand_config(repo_path: &Path, cfg: &DeckhandConfig) -> deckhand::config::Config {
    use deckhand::config::{
        AutoCleanConfig, CleanConfig, StatusConfig, SweepConfig, WorkspaceConfig,
    };

    deckhand::config::Config {
        workspace: WorkspaceConfig {
            path: repo_path.to_path_buf(),
            members: deckhand::config::MemberSpec::Auto,
        },
        clean: CleanConfig {
            profiles: cfg.clean_profiles.clone(),
            keep_incremental: false,
            keep_days: cfg.clean_older_than_days.unwrap_or(0),
            languages: vec!["cargo".to_string()],
            allow_native_commands: false,
            remove_node_modules: false,
            remove_venvs: false,
        },
        sweep: SweepConfig {
            registry_cache: true,
            git_checkouts: true,
            keep_registry_days: cfg.sweep_keep_days,
            node_modules: false,
            python_bytecode: false,
            go_build_cache: false,
            swift_derived_data: false,
        },
        status: StatusConfig {
            warn_free_percent: cfg.min_free_percent,
        },
        auto_clean: AutoCleanConfig::default(),
    }
}

fn collect_watched_dirs(repo_path: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();

    // Workspace root target directory (covers workspace-level Cargo builds).
    let root_target = repo_path.join("target");
    if root_target.exists() {
        dirs.push(root_target);
    }

    // Per-member target directories.
    if let Ok(ws) = deckhand::workspace::discover(repo_path, &["cargo".to_string()]) {
        for project in &ws.projects {
            let target = project.path.join("target");
            if target.exists() {
                dirs.push(target);
            }
        }
    }

    // Cargo caches, if resolvable.
    if let Ok(cargo_home) = cargo_home() {
        let registry = cargo_home.join("registry/cache");
        if registry.exists() {
            dirs.push(registry);
        }
        let git = cargo_home.join("git/checkouts");
        if git.exists() {
            dirs.push(git);
        }
    }

    Ok(dirs)
}

fn cargo_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("CARGO_HOME") {
        Ok(PathBuf::from(home))
    } else {
        let home = std::env::var("HOME").context("HOME not set")?;
        Ok(PathBuf::from(home).join(".cargo"))
    }
}

fn measure_dirs(dirs: &[PathBuf]) -> Result<u64> {
    let mut total = 0u64;
    for dir in dirs {
        total += deckhand::fmt::dir_size(dir)?;
    }
    Ok(total)
}

fn persist_report(repo_path: &Path, report: &StorageReport) -> Result<()> {
    let dir = repo_path.join(".kaptaind");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("storage.json");
    let tmp = path.with_extension("tmp");
    let content = serde_json::to_string_pretty(report)?;
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Load the most recent storage report, if one exists.
pub fn load_report(repo_path: &Path) -> Option<StorageReport> {
    let path = repo_path.join(".kaptaind").join("storage.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn storage_report_roundtrips() {
        let report = StorageReport {
            started_at: Utc::now(),
            duration_ms: 1234,
            bytes_freed: 1024 * 1024,
            files_removed: 42,
            errors: 0,
            skipped_reason: None,
        };
        let dir = tempdir().unwrap();
        persist_report(dir.path(), &report).unwrap();
        let loaded = load_report(dir.path()).unwrap();
        assert_eq!(loaded.bytes_freed, report.bytes_freed);
        assert_eq!(loaded.files_removed, report.files_removed);
        assert_eq!(loaded.errors, report.errors);
    }

    #[test]
    fn build_deckhand_config_honors_kaptaind_settings() {
        let dir = tempdir().unwrap();
        let cfg = DeckhandConfig {
            enabled: true,
            interval_minutes: 60,
            sweep_keep_days: 7,
            clean_profiles: vec!["release".to_string()],
            clean_older_than_days: Some(14),
            dry_run: true,
            min_free_percent: 20,
        };
        let dh = build_deckhand_config(dir.path(), &cfg);
        assert_eq!(dh.workspace.path, dir.path());
        assert_eq!(dh.sweep.keep_registry_days, 7);
        assert_eq!(dh.clean.profiles, vec!["release"]);
        assert_eq!(dh.clean.keep_days, 14);
        assert_eq!(dh.status.warn_free_percent, 20);
    }
}
