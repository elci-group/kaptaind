use crate::angler::{AnglerSystem, BaitContext, BaitEvent, WebhookEvent};
use crate::aoc::tracer;
use crate::cluster::engine::{Cluster, ClusterEngine};
use crate::config::loader::TestConfig;
use crate::config::Config;
use crate::daemon::health::{DaemonEvent, Metrics};
use crate::daemon::policy::{self, Policy};
use crate::daemon::prune::prune_analysis_artifacts;
use crate::daemon::status::write_status;
use crate::daemon::trace::write_trace_if_active;
use crate::diff::DiffAnalysis;
use crate::git::repo::Repo;
use crate::release::ship::{run_nightly, run_stable, OutputFormat, ShipKind, ShipOptions};
use crate::version::Bump;
use crate::watcher::FsEvent;
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use semver::Version;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinSet;

pub use crate::daemon::status::{State, StatusReport};

fn broadcast_event(
    tx: &tokio::sync::broadcast::Sender<DaemonEvent>,
    event_type: &str,
    payload: serde_json::Value,
) {
    let _ = tx.send(DaemonEvent {
        event_type: event_type.to_string(),
        payload,
    });
}

/// How long a daemon-caused write suppresses the watch event it generates.
const SELF_WRITE_TTL: Duration = Duration::from_secs(60);
/// Bound on remembered self-writes so the guard cannot grow without limit.
const SELF_WRITE_CAP: usize = 64;

/// TTL-bounded record of paths the daemon itself just wrote, used to suppress
/// the watch events those writes generate.
///
/// Without this, `save_version`'s writeback (VERSION/Cargo.toml, plus a
/// Cargo.lock touch from the next build) re-clusters and every auto-commit
/// spawns the next one — observed live as three self-commits in under three
/// minutes (v0.1.6 -> v0.1.9), stopped only by killing the daemon. Suppression
/// is deliberately narrow: exact absolute paths, 60s TTL, and mixed events are
/// split so a genuine edit that shares an event with a self-write still
/// clusters.
struct SelfWriteGuard {
    writes: VecDeque<(PathBuf, Instant)>,
}

impl SelfWriteGuard {
    fn new() -> Self {
        Self {
            writes: VecDeque::new(),
        }
    }

    /// Record paths the daemon is about to write / has just written.
    fn record(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let now = Instant::now();
        for path in paths {
            self.writes.push_back((path, now));
        }
        while self.writes.len() > SELF_WRITE_CAP {
            self.writes.pop_front();
        }
    }

    /// Drop expired entries. Pushes are chronological, so the front is oldest.
    fn prune(&mut self) {
        while let Some((_, when)) = self.writes.front() {
            if when.elapsed() > SELF_WRITE_TTL {
                self.writes.pop_front();
            } else {
                break;
            }
        }
    }

    /// True if `path` is a daemon-caused write within the TTL window.
    fn is_self_write(&mut self, path: &Path) -> bool {
        self.prune();
        self.writes.iter().any(|(recorded, _)| recorded == path)
    }
}

pub async fn run(
    mut rx: Receiver<FsEvent>,
    config: Config,
    mut shutdown: crate::daemon::shutdown::ShutdownToken,
    metrics: Arc<Metrics>,
    event_tx: tokio::sync::broadcast::Sender<DaemonEvent>,
) {
    // Initialize Angler system
    let angler = match AnglerSystem::new(&config.angler, &config.repo_path) {
        Ok(system) => {
            if system.is_active() {
                tracing::info!("Angler system initialized");
            }
            Some(system)
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to initialize angler system");
            None
        }
    };

    let (vacs_engine, vacs_rx) = crate::vacs::VacsEngine::new(&config.repo_path, &config.vacs);
    let vacs_engine_clone = vacs_engine.clone();

    let mut tasks: JoinSet<()> = JoinSet::new();
    tasks.spawn(async move {
        vacs_engine_clone.process_queue(vacs_rx).await;
    });

    let mut prune_interval =
        tokio::time::interval(Duration::from_secs(config.prune_interval_minutes * 60));
    // Skip the immediate first tick so we don't prune right on startup
    prune_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut ship_check_interval = tokio::time::interval(Duration::from_secs(60));
    ship_check_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut pulse_interval = tokio::time::interval(Duration::from_secs(900));
    pulse_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let daemon_start_time = Utc::now();
    let mut ship_nightly_next_fire: Option<DateTime<Utc>> = None;
    let mut ship_stable_next_fire: Option<DateTime<Utc>> = None;
    let ship_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut cluster_engine = ClusterEngine::new_from_config(&config.cluster);
    let mut repo = match Repo::open(&config.repo_path) {
        Ok(repo) => repo,
        Err(err) => {
            tracing::error!(error = %err, path = ?config.repo_path, "failed to open repository");
            return;
        }
    };

    let ignore_matcher = IgnoreMatcher::load(&config.repo_path, &config.watch.ignore_file);
    let mut last_commit_at: Option<DateTime<Utc>> = None;
    let mut self_writes = SelfWriteGuard::new();

    let mut status = StatusReport {
        status: State::Idle,
        last_version: load_version(&config.repo_path.join("VERSION")).map(|v| v.to_string()),
        last_action_time: Utc::now(),
        last_error: None,
        current_task: None,
        progress_percent: None,
    };
    write_status(&config.repo_path, &status);

    // Startup reconciliation: changes made while the daemon was down would
    // otherwise sit uncommitted until the next edit. Form a single catch-up
    // cluster through the normal pipeline so it is scored, tested, and gated
    // like any other change (finding #9).
    if config.watch.rescan_on_start {
        let ctx = crate::git::repo::RepoContext::new(repo.root(), &config.repo_path);
        let mut pending: Vec<PathBuf> = repo
            .changed_paths()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| ctx.to_project_relative(&p))
            .map(|rel| config.repo_path.join(rel))
            .collect();
        // Drop ignored paths individually (e.g. an untracked .kaptaind/
        // directory) rather than discarding the whole catch-up.
        pending.retain(|p| !ignore_matcher.is_ignored(std::slice::from_ref(p)));
        if !pending.is_empty() {
            tracing::info!(
                paths = pending.len(),
                "startup rescan: pending changes found, forming catch-up cluster"
            );
            let now = Utc::now();
            let cluster = Cluster {
                id: uuid::Uuid::new_v4(),
                started_at: now,
                ended_at: now,
                events: vec![FsEvent {
                    paths: pending,
                    kind: crate::watcher::FsEventKind::Modify,
                    timestamp: now,
                }],
            };
            process_cluster(
                &mut repo,
                &config,
                &mut last_commit_at,
                cluster,
                &mut status,
                &mut self_writes,
                &vacs_engine,
                &mut tasks,
                shutdown.clone_token(),
                angler.as_ref(),
                metrics.clone(),
                event_tx.clone(),
            )
            .await;
        }
    }

    loop {
        tokio::select! {
            maybe_event = rx.recv() => {
                match maybe_event {
                    Some(mut event) => {
                        if ignore_matcher.is_ignored(&event.paths) {
                            tracing::trace!(?event.paths, "event ignored");
                            continue;
                        }

                        // Suppress events caused by the daemon's own writes
                        // (version writeback etc.); without this every
                        // auto-commit re-triggers the next cluster in an
                        // unbounded cascade. Mixed events are split so a
                        // genuine edit sharing the event still clusters.
                        let original_len = event.paths.len();
                        event.paths.retain(|p| !self_writes.is_self_write(p));
                        if event.paths.is_empty() {
                            tracing::trace!("self-write event suppressed");
                            continue;
                        }
                        if event.paths.len() != original_len {
                            tracing::trace!(
                                remaining = ?event.paths,
                                "self-write paths stripped from mixed event"
                            );
                        }

                        if matches!(status.status, State::Idle) {
                            tracing::trace!("transitioning state to Clustering");
                            status.set_task(State::Clustering, "Collecting changes", None);
                            write_status(&config.repo_path, &status);
                        }

                        tracing::trace!(?event.paths, "ingesting event");
                        if let Some(cluster) = cluster_engine.ingest(event) {
                            tracing::info!(cluster_id = %cluster.id, "cluster window expired by new event");
                            process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status, &mut self_writes, &vacs_engine, &mut tasks, shutdown.clone_token(), angler.as_ref(), metrics.clone(), event_tx.clone()).await;
                        }
                    }
                    None => {
                        tracing::trace!("event channel closed");
                        if let Some(cluster) = cluster_engine.flush() {
                            process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status, &mut self_writes, &vacs_engine, &mut tasks, shutdown.clone_token(), angler.as_ref(), metrics.clone(), event_tx.clone()).await;
                        }
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(config.cluster.flush_after.unwrap_or(config.cluster.window)) => {
                if let Some(cluster) = cluster_engine.flush() {
                    tracing::info!(cluster_id = %cluster.id, "cluster window expired by timeout");
                    process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status, &mut self_writes, &vacs_engine, &mut tasks, shutdown.clone_token(), angler.as_ref(), metrics.clone(), event_tx.clone()).await;
                }
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {
                // Task completed; reap it without blocking
            }
            _ = prune_interval.tick() => {
                let repo_path = config.repo_path.clone();
                let retention = config.retention_days;
                let metrics = metrics.clone();
                tasks.spawn(async move {
                    let result = prune_analysis_artifacts(&repo_path, retention).await;
                    metrics.artifacts_pruned.fetch_add(result.deleted, Ordering::Relaxed);
                    if result.deleted > 0 || result.errors > 0 {
                        tracing::info!(deleted = result.deleted, errors = result.errors, "pruned old analysis artifacts");
                    }
                });
            }
            _ = ship_check_interval.tick() => {
                let now = Utc::now();
                maybe_run_auto_ship(
                    &config,
                    now,
                    &mut ship_nightly_next_fire,
                    &mut ship_stable_next_fire,
                    &ship_running,
                    &mut tasks,
                );
            }
            _ = pulse_interval.tick() => {
                let uptime = Utc::now().signed_duration_since(daemon_start_time).num_seconds().max(0) as u64;
                let clusters = metrics.clusters_processed.load(Ordering::Relaxed) as u64;
                let version = status.last_version.clone().unwrap_or_else(|| "unknown".to_string());
                crate::daemon::notification::notify_pulse(
                    &config.notify,
                    uptime,
                    clusters,
                    &version,
                    config.capabilities.network_webhooks,
                );
            }
            _ = shutdown.wait() => {
                tracing::info!("shutdown signal received, draining tasks");
                status.set_task(State::Stopping, "Shutting down", None);
                write_status(&config.repo_path, &status);
                break;
            }
        }
    }

    // Drain all remaining tasks with a timeout
    let drain = async { while tasks.join_next().await.is_some() {} };
    let drain_timeout = Duration::from_secs(25);
    if tokio::time::timeout(drain_timeout, drain).await.is_err() {
        tracing::warn!("task drain timeout (25s), aborting remaining tasks");
        tasks.abort_all();
    }

    status.set_task(State::Stopped, "Stopped", None);
    write_status(&config.repo_path, &status);
    tracing::info!("scheduler shutdown complete");
}

#[allow(clippy::too_many_arguments)]
async fn process_cluster(
    repo: &mut Repo,
    config: &Config,
    last_commit_at: &mut Option<DateTime<Utc>>,
    cluster: Cluster,
    status: &mut StatusReport,
    self_writes: &mut SelfWriteGuard,
    vacs_engine: &crate::vacs::VacsEngine,
    tasks: &mut JoinSet<()>,
    shutdown: crate::daemon::shutdown::ShutdownToken,
    angler: Option<&AnglerSystem>,
    metrics: Arc<Metrics>,
    event_tx: tokio::sync::broadcast::Sender<DaemonEvent>,
) {
    let now = Utc::now();
    let mut test_outcome = TestOutcome::Skipped; // Default; will be overwritten if we get to testing

    metrics.clusters_processed.fetch_add(1, Ordering::Relaxed);

    // Load policy if configured
    let policy: Option<Policy> = config
        .policy_id
        .as_ref()
        .and_then(|id| Policy::load_or_default(&config.repo_path, id).ok());

    // Collect cluster paths early for policy checks
    let cluster_paths: Vec<PathBuf> = cluster
        .events
        .iter()
        .flat_map(|e| e.paths.iter().cloned())
        .collect();

    // Enforce file_pattern_allowlist
    if let Some(ref p) = policy {
        if !p.file_pattern_allowlist.is_empty()
            && !policy::cluster_matches_allowlist(&cluster_paths, &p.file_pattern_allowlist)
        {
            tracing::debug!("cluster blocked by file_pattern_allowlist");
            write_trace_if_active(
                &config.repo_path,
                &cluster,
                tracer::TraceResult::Skipped {
                    reason: "allowlist".to_string(),
                },
                test_outcome.trace_test(),
                None,
            );
            status.set_idle();
            write_status(&config.repo_path, status);
            return;
        }
    }

    // Attempt to link interceptor agent events matching this cluster
    let agent_events = crate::aoc::interceptor::consume_events_in_window(
        &config.repo_path,
        cluster.started_at - chrono::Duration::seconds(5),
        cluster.ended_at + chrono::Duration::seconds(5),
    )
    .unwrap_or_default();

    // Just take the latest relevant event if any
    let agent_event = agent_events.into_iter().last();

    if !rate_limit_allows(now, *last_commit_at, config.ratelimit.min_commit_interval) {
        tracing::debug!("commit rate-limited");
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "rate_limited".to_string(),
            },
            test_outcome.trace_test(),
            agent_event.clone(),
        );
        status.set_idle();
        write_status(&config.repo_path, status);
        return;
    }

    let is_clean = match repo.is_clean() {
        Ok(clean) => clean,
        Err(err) => {
            tracing::error!(error = %err, "failed to inspect working tree");
            status.set_failed(err.to_string());
            write_status(&config.repo_path, status);
            return;
        }
    };

    if is_clean {
        tracing::debug!("working tree clean; nothing to commit");
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "clean_tree".to_string(),
            },
            test_outcome.trace_test(),
            agent_event.clone(),
        );
        status.set_idle();
        write_status(&config.repo_path, status);
        return;
    }

    status.set_task(State::Testing, "Running tests", Some(25));
    write_status(&config.repo_path, status);
    tracing::trace!("running test hook");

    // Branch protection: force test required on protected branches
    let branch_protection_forces_tests = policy
        .as_ref()
        .map(|p| policy::is_branch_protected(&config.repo_path, &p.branch_protection))
        .unwrap_or(false);

    test_outcome = run_test_hook(config).await;

    if matches!(test_outcome, TestOutcome::Failed { .. }) {
        metrics.test_hook_failures.fetch_add(1, Ordering::Relaxed);
    }

    let tests_required = config.test.required
        || branch_protection_forces_tests
        || policy
            .as_ref()
            .map(|p| p.min_test_coverage)
            .unwrap_or(false);

    if tests_required && matches!(test_outcome, TestOutcome::Failed { .. }) {
        log_test_failure(&test_outcome);

        if policy
            .as_ref()
            .map(|p| p.min_test_coverage)
            .unwrap_or(false)
        {
            let _ = policy::append_audit_log(
                &config.repo_path,
                &policy::AuditEntry {
                    timestamp: Utc::now(),
                    action: "commit_blocked".to_string(),
                    resource: "test_failure".to_string(),
                    details: serde_json::json!({
                        "reason": "min_test_coverage",
                        "cluster_id": cluster.id.to_string(),
                    }),
                },
            );
        }

        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "test_failed".to_string(),
            },
            test_outcome.trace_test(),
            agent_event.clone(),
        );
        if let TestOutcome::Failed { stderr, .. } = &test_outcome {
            status.set_failed(stderr.clone());
        } else {
            status.set_failed("Tests failed".to_string());
        }
        write_status(&config.repo_path, status);
        crate::daemon::notification::notify_warning(
            &config.notify,
            "Tests failed",
            "Test hook",
            config.capabilities.network_webhooks,
        );
        broadcast_event(
            &event_tx,
            "warning",
            serde_json::json!({
                "title": "Tests failed",
                "source": "Test hook",
            }),
        );
        return;
    }

    status.set_task(State::Committing, "Analyzing diff & committing", Some(75));
    write_status(&config.repo_path, status);

    let mut diff = crate::diff::analyze_with_plugins(&cluster, &config.repo_path, &config.plugins);
    if config.bundle.command.is_some() && config.capabilities.bundle_scoring {
        diff.bundle = crate::diff::bundle::bundle_score(&config.bundle, &config.repo_path).score;
    }
    crate::daemon::telemetry::update_cache_metrics(
        &config.repo_path,
        diff.ast_cache_hits,
        diff.ast_cache_misses,
        diff.ast_cache_entries,
    );
    tracing::trace!(?diff, "diff analysis complete");
    apply_test_outcome(&mut diff, &test_outcome);
    let weight = crate::weight::compute(&diff, &config.weights);
    tracing::trace!(?weight, "weight computation complete");
    let bump = crate::version::decide(&weight, &config.version_thresholds);

    if bump == Bump::None {
        tracing::debug!("no semantic version bump required");
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "no_bump".to_string(),
            },
            test_outcome.trace_test(),
            agent_event.clone(),
        );
        status.set_idle();
        write_status(&config.repo_path, status);
        return;
    }

    let previous = match crate::version::resolve_baseline(&config.repo_path) {
        Ok(version) => version,
        Err(err) => {
            tracing::error!(error = %err, "cannot resolve baseline version; skipping commit");
            write_trace_if_active(
                &config.repo_path,
                &cluster,
                tracer::TraceResult::Skipped {
                    reason: "version_baseline_unresolvable".to_string(),
                },
                test_outcome.trace_test(),
                agent_event.clone(),
            );
            status.set_failed(err.to_string());
            write_status(&config.repo_path, status);
            return;
        }
    };
    let next = crate::version::apply(previous.clone(), bump);

    // Collect cluster paths early for staging and inference
    let cluster_paths: Vec<PathBuf> = cluster
        .events
        .iter()
        .flat_map(|e| e.paths.iter().cloned())
        .collect();

    // Check selective capture rules
    if let Some(angler) = angler {
        let selective_changes: Vec<crate::angler::FileChange> = cluster_paths
            .iter()
            .map(|p| {
                let mut change =
                    crate::angler::FileChange::new(p, crate::angler::ChangeType::Modified);
                let _ = change.with_metadata(&config.repo_path);
                change
            })
            .collect();

        // Check for blocked changes
        let blocked = angler.would_block_changes(&selective_changes);
        if !blocked.is_empty() {
            for (change, reason) in &blocked {
                tracing::warn!(path = %change.path.display(), reason = %reason, "change blocked by selective rule");
            }
            let title = format!("{} change(s) blocked by selective rules", blocked.len());
            crate::daemon::notification::notify_warning(
                &config.notify,
                &title,
                "Angler selective capture",
                config.capabilities.network_webhooks,
            );
            broadcast_event(
                &event_tx,
                "warning",
                serde_json::json!({
                    "title": title,
                    "source": "Angler selective capture",
                }),
            );
            let err = format!("{} change(s) blocked by selective rules", blocked.len());
            status.set_failed(err);
            write_status(&config.repo_path, status);
            return;
        }

        // Trigger pre-commit bait
        let bait_context = BaitContext {
            event: BaitEvent::PreCommit,
            files: cluster_paths
                .iter()
                .map(|p| crate::angler::FileChangeInfo {
                    path: p.clone(),
                    change_type: "modified".to_string(),
                    size: 0,
                })
                .collect(),
            repo_path: config.repo_path.clone(),
            cluster_id: Some(cluster.id.to_string()),
            version: Some(next.to_string()),
            score: Some(weight.score),
            metadata: std::collections::HashMap::new(),
        };
        let _ = angler
            .trigger_baits(BaitEvent::PreCommit, &bait_context)
            .await;

        // Run pre-commit hooks
        if let Some(hook_result) = angler.run_pre_commit(&cluster_paths).await {
            if !hook_result.success {
                tracing::warn!(stderr = %hook_result.stderr, "pre-commit hook failed");
                if config
                    .angler
                    .git_hooks
                    .pre_commit
                    .as_ref()
                    .map(|c| c.required)
                    .unwrap_or(false)
                {
                    let err = format!("Pre-commit hook failed: {}", hook_result.stderr);
                    status.set_failed(err.clone());
                    write_status(&config.repo_path, status);
                    crate::daemon::notification::notify_warning(
                        &config.notify,
                        &err,
                        "Pre-commit hook",
                        config.capabilities.network_webhooks,
                    );
                    broadcast_event(
                        &event_tx,
                        "warning",
                        serde_json::json!({
                            "title": err,
                            "source": "Pre-commit hook",
                        }),
                    );
                    return;
                }
            }
        }
    }

    let version_path = config.repo_path.join("VERSION");
    if let Err(err) = save_version(&version_path, &next) {
        tracing::error!(error = %err, path = ?version_path, "failed writing VERSION file");
        // Still write trace for visibility
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "version_write_failed".to_string(),
            },
            test_outcome.trace_test(),
            agent_event.clone(),
        );
        status.set_failed(err.to_string());
        write_status(&config.repo_path, status);
        crate::daemon::notification::notify_warning(
            &config.notify,
            &err.to_string(),
            "Version write failed",
            config.capabilities.network_webhooks,
        );
        broadcast_event(
            &event_tx,
            "warning",
            serde_json::json!({
                "title": err.to_string(),
                "source": "Version write failed",
            }),
        );
        return;
    }

    // Record the daemon's own writes so the watcher suppresses the events
    // they generate — otherwise each auto-commit re-clusters its version
    // writeback and cascades. Cargo.lock is included preemptively: the next
    // build may refresh it after the bump.
    self_writes.record([
        config.repo_path.join("VERSION"),
        config.repo_path.join("Cargo.toml"),
        config.repo_path.join("Cargo.lock"),
    ]);

    if let Err(err) = persist_analysis_artifact(
        config,
        &cluster,
        &diff,
        &weight,
        bump,
        &next,
        config.air_gapped,
    ) {
        tracing::warn!(error = %err, "failed to persist analysis artifact");
    }

    let metadata_line = format_commit(&cluster, &diff, &weight, bump, &next, &agent_event);

    let msg = if config.inference.enabled
        && weight.score >= config.inference.min_score_for_inference as f32
    {
        let ctx = crate::inference::CommitContext {
            cluster: &cluster,
            diff: &diff,
            weight: &weight,
            bump,
            previous: &previous,
            next: &next,
            cluster_paths: &cluster_paths,
        };
        match crate::inference::generate_commit_message(&config.inference, &ctx).await {
            Some(narrative) => {
                let subject = sanitize_commit_subject(&narrative);
                if subject.is_empty() {
                    metadata_line
                } else {
                    format!("{subject}\n\n{metadata_line}")
                }
            }
            None => {
                tracing::warn!("ollama inference unavailable; using deterministic message");
                metadata_line
            }
        }
    } else {
        metadata_line
    };

    // Policy signoff
    let msg = if policy.as_ref().map(|p| p.required_signoff).unwrap_or(false) {
        format!("{}\n\nSigned-off-by: kaptaind <kaptaind@localhost>", msg)
    } else {
        msg
    };

    // Abstract token calculation
    let mut input_tokens = 0;
    for event in &cluster.events {
        for path in &event.paths {
            if let Ok(meta) = std::fs::metadata(config.repo_path.join(path)) {
                input_tokens += (meta.len() / 4) as usize;
            }
        }
    }
    let output_tokens = msg.len() / 4;
    let (provider, model) = if config.inference.enabled {
        let provider = crate::inference::resolve_provider(&config.inference);
        let model = crate::inference::resolve_model(&config.inference, provider);
        (provider.to_string(), model.to_string())
    } else {
        ("none".to_string(), "none".to_string())
    };
    let cost_metrics = crate::daemon::telemetry::track_cost(
        &config.repo_path,
        &provider,
        &model,
        input_tokens,
        output_tokens,
    );
    tracing::info!(
        input_tokens = cost_metrics.input_tokens,
        output_tokens = cost_metrics.output_tokens,
        marginal_cost = %cost_metrics.marginal_cost,
        aggregate_cost = %cost_metrics.aggregate_cost,
        "Token usage and cost tracking"
    );

    let repo_ctx = crate::git::repo::RepoContext::new(repo.root(), &config.repo_path);
    if let Err(err) = crate::commit::orchestrator::commit_with_staging(
        &repo_ctx,
        &msg,
        &config.staging,
        &cluster_paths,
        &config.commit,
    ) {
        tracing::error!(error = %err, "commit failed");
        broadcast_event(
            &event_tx,
            "commit_failed",
            serde_json::json!({
                "error": err.to_string(),
                "cluster_id": cluster.id.to_string(),
            }),
        );
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "commit_failed".to_string(),
            },
            test_outcome.trace_test(),
            agent_event.clone(),
        );
        status.set_failed(err.to_string());
        write_status(&config.repo_path, status);
        crate::daemon::notification::notify_warning(
            &config.notify,
            &err.to_string(),
            "Commit failed",
            config.capabilities.network_webhooks,
        );
        broadcast_event(
            &event_tx,
            "warning",
            serde_json::json!({
                "title": err.to_string(),
                "source": "Commit failed",
            }),
        );
        return;
    }

    metrics.commits_made.fetch_add(1, Ordering::Relaxed);

    crate::audit::log_commit(
        &config.repo_path,
        &config.shark_instance_id(),
        &next.to_string(),
        &format!("{bump:?}"),
        weight.score as f64,
        &cluster.id.to_string(),
        cluster_paths.len(),
    );

    broadcast_event(
        &event_tx,
        "commit_succeeded",
        serde_json::json!({
            "version": next.to_string(),
            "cluster_id": cluster.id.to_string(),
            "score": weight.score,
        }),
    );

    // Run post-commit hooks and send webhook events
    if let Some(angler) = angler {
        // Run post-commit hooks
        let _ = angler.run_post_commit().await;

        // Send webhook events (skipped in air-gapped mode)
        if config.capabilities.network_webhooks {
            let webhook_event = WebhookEvent::Commit {
                version: next.to_string(),
                score: weight.score,
                message: msg.clone(),
                files_changed: cluster_paths.len(),
                cluster_id: cluster.id.to_string(),
            };
            let webhook_results = angler
                .broadcast_webhook_event(&webhook_event, &cluster_paths)
                .await;
            for (endpoint_id, result) in webhook_results {
                if !result.success {
                    tracing::warn!(endpoint = %endpoint_id, error = ?result.error, "webhook delivery failed");
                }
            }
        }

        // Trigger post-commit bait
        let bait_context = BaitContext {
            event: BaitEvent::PostCommit,
            files: cluster_paths
                .iter()
                .map(|p| crate::angler::FileChangeInfo {
                    path: p.clone(),
                    change_type: "modified".to_string(),
                    size: 0,
                })
                .collect(),
            repo_path: config.repo_path.clone(),
            cluster_id: Some(cluster.id.to_string()),
            version: Some(next.to_string()),
            score: Some(weight.score),
            metadata: std::collections::HashMap::new(),
        };
        let _ = angler
            .trigger_baits(BaitEvent::PostCommit, &bait_context)
            .await;
    }

    if config.push.enabled && config.capabilities.network_push {
        // Run pre-push hooks if configured
        if let Some(angler) = angler {
            let refs = vec![(
                format!("refs/heads/{}", config.push.branch),
                "HEAD".to_string(),
                format!("refs/heads/{}", config.push.branch),
                "origin/HEAD".to_string(),
            )];
            if let Some(hook_result) = angler
                .run_pre_push(&config.push.remote, "origin", &refs)
                .await
            {
                if !hook_result.success {
                    tracing::warn!(stderr = %hook_result.stderr, "pre-push hook failed");
                    if config
                        .angler
                        .git_hooks
                        .pre_push
                        .as_ref()
                        .map(|c| c.required)
                        .unwrap_or(false)
                    {
                        let err = format!("Pre-push hook failed: {}", hook_result.stderr);
                        status.set_failed(err.clone());
                        write_status(&config.repo_path, status);
                        crate::daemon::notification::notify_warning(
                            &config.notify,
                            &err,
                            "Pre-push hook",
                            config.capabilities.network_webhooks,
                        );
                        broadcast_event(
                            &event_tx,
                            "warning",
                            serde_json::json!({
                                "title": err,
                                "source": "Pre-push hook",
                            }),
                        );
                        return;
                    }
                }
            }
        }

        let push_options = crate::push::PushOptions {
            remote: config.push.remote.clone(),
            branch: config.push.branch.clone(),
            dry_run: config.push.dry_run,
            protect_branches: config.push.safety.protect_branches.clone(),
        };

        if let Err(err) = crate::push::push(
            &config.repo_path,
            &push_options,
            &config.push.retry,
            &config.push.protection,
        )
        .await
        {
            tracing::warn!(error = %err, "push failed");
            write_trace_if_active(
                &config.repo_path,
                &cluster,
                tracer::TraceResult::Skipped {
                    reason: "push_failed".to_string(),
                },
                test_outcome.trace_test(),
                agent_event.clone(),
            );
            status.set_failed(format!("push failed: {err}"));
            write_status(&config.repo_path, status);
            crate::daemon::notification::notify_warning(
                &config.notify,
                &err.to_string(),
                "Push failed",
                config.capabilities.network_webhooks,
            );
            broadcast_event(
                &event_tx,
                "warning",
                serde_json::json!({
                    "title": err.to_string(),
                    "source": "Push failed",
                }),
            );
            return;
        }

        // Send push webhook event
        if let Some(angler) = angler {
            let webhook_event = WebhookEvent::Push {
                branch: config.push.branch.clone(),
                commits: 1,
                remote: config.push.remote.clone(),
            };
            let _ = angler
                .broadcast_webhook_event(&webhook_event, &cluster_paths)
                .await;

            // Trigger post-push bait
            let bait_context = BaitContext {
                event: BaitEvent::PostPush,
                files: cluster_paths
                    .iter()
                    .map(|p| crate::angler::FileChangeInfo {
                        path: p.clone(),
                        change_type: "modified".to_string(),
                        size: 0,
                    })
                    .collect(),
                repo_path: config.repo_path.clone(),
                cluster_id: Some(cluster.id.to_string()),
                version: Some(next.to_string()),
                score: Some(weight.score),
                metadata: std::collections::HashMap::new(),
            };
            let _ = angler
                .trigger_baits(BaitEvent::PostPush, &bait_context)
                .await;
        }

        crate::daemon::notification::notify_push_success(
            &config.notify,
            &next.to_string(),
            &config.push.branch,
            &config.push.remote,
            config.capabilities.network_webhooks,
        );
    }

    let files_changed = cluster_paths.len();
    crate::daemon::notification::notify_commit(
        &config.notify,
        &next.to_string(),
        weight.score,
        &msg,
        files_changed,
        config.capabilities.network_webhooks,
    );

    // Write trace record for successful commit
    write_trace_if_active(
        &config.repo_path,
        &cluster,
        tracer::TraceResult::Committed {
            bump: format!("{bump:?}"),
            version: next.to_string(),
        },
        test_outcome.trace_test(),
        agent_event,
    );

    *last_commit_at = Some(now);
    status.set_idle();
    status.last_version = Some(next.to_string());
    status.last_error = None;
    write_status(&config.repo_path, status);

    // Fire VACS event
    let aoc_id = crate::aoc::session::load_active(&config.repo_path)
        .ok()
        .flatten()
        .map(|s| s.id);
    let vacs_event = crate::vacs::VacsEvent {
        event_type: "commit.created".to_string(),
        timestamp: now,
        project_id: config.repo_path.display().to_string(),
        payload: crate::vacs::VacsPayload {
            files_changed: cluster_paths
                .iter()
                .filter_map(|p| p.to_str().map(|s| s.to_string()))
                .collect(),
            diff_summary: msg.clone(),
            aoc_id,
            complexity_score: weight.score as f64,
        },
    };
    if let Err(err) = vacs_engine.ingest(vacs_event).await {
        tracing::warn!(error = %err, "failed to ingest VACS event");
    }

    // Spawn post-commit qualification and release pipeline (non-blocking)
    if config.qualification.enabled {
        let repo_path = config.repo_path.clone();
        let config_clone = config.clone();
        let version_str = next.to_string();
        let commit_hash = repo
            .head_commit_hash()
            .unwrap_or_else(|_| "unknown".to_string());
        let tests_passed = matches!(test_outcome, TestOutcome::Passed);
        let failed_tests = match &test_outcome {
            TestOutcome::Failed { failed_tests, .. } => failed_tests.clone(),
            _ => Vec::new(),
        };
        let diff_f64 = weight.score as f64;
        let runtime_paths = diff.runtime_paths as u32;
        // Compute mean parse confidence from analysis metadata
        let parse_confidence = if diff.parse_metadata.is_empty() {
            1.0
        } else {
            diff.parse_metadata
                .iter()
                .map(|m| m.confidence)
                .sum::<f64>()
                / diff.parse_metadata.len() as f64
        };
        let shutdown_clone = shutdown.clone_token();
        let event_tx_qual = event_tx.clone();
        let cluster_id = cluster.id.to_string();
        tasks.spawn(async move {
            crate::release::orchestrator::post_commit(
                &repo_path,
                &config_clone,
                &version_str,
                &commit_hash,
                tests_passed,
                failed_tests,
                diff_f64,
                runtime_paths,
                parse_confidence,
                Some(shutdown_clone),
            )
            .await;
            broadcast_event(
                &event_tx_qual,
                "release_qualified",
                serde_json::json!({
                    "version": version_str,
                    "cluster_id": cluster_id,
                }),
            );
        });
    }

    // Run pruning in the background
    let repo_path = config.repo_path.clone();
    tasks.spawn(async move {
        auto_prune(&repo_path).await;
    });
}

async fn auto_prune(repo_path: &Path) {
    // Keep max 1000 items in analysis/ and traces/
    prune_directory(&repo_path.join(".kaptaind").join("analysis"), 1000).await;
    prune_directory(&repo_path.join(".kaptaind").join("traces"), 1000).await;

    // Auto-reap stale AoC sessions (older than 72 hours)
    if let Ok(Some(session)) = crate::aoc::session::load_active(repo_path) {
        if Utc::now()
            .signed_duration_since(session.created_at)
            .num_hours()
            > 72
        {
            tracing::info!(aoc_id = %session.id, "auto-reaping stale AoC session");
            // Perform an auto-ship of the stale session
            let _ = auto_ship_aoc(repo_path, &session).await;
        }
    }
}

async fn auto_ship_aoc(repo_path: &Path, session: &crate::aoc::AocSession) -> anyhow::Result<()> {
    let version_path = repo_path.join("VERSION");
    let final_version = if version_path.exists() {
        std::fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "0.1.0".to_string()
    };

    let traces = crate::aoc::tracer::read_traces_for_aoc(repo_path, &session.id)?;

    let commit_count = traces
        .iter()
        .filter(|t| matches!(t.result, crate::aoc::TraceResult::Committed { .. }))
        .count();

    let test_failures = traces.iter().filter(|t| t.test.outcome == "failed").count();

    let manifest = crate::aoc::AocManifest {
        id: session.id.clone(),
        label: format!("{} (auto-reaped)", session.label),
        created_at: session.created_at,
        shipped_at: Utc::now(),
        initial_version: session.initial_version.clone(),
        final_version,
        cluster_count: traces.len(),
        commit_count,
        test_failures,
        trace_ids: traces.iter().map(|t| t.cluster_id.clone()).collect(),
    };

    crate::aoc::session::save_manifest(repo_path, &manifest)?;
    crate::aoc::session::remove_active(repo_path)?;
    Ok(())
}

fn maybe_run_auto_ship(
    config: &Config,
    now: DateTime<Utc>,
    nightly_next_fire: &mut Option<DateTime<Utc>>,
    stable_next_fire: &mut Option<DateTime<Utc>>,
    ship_running: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    tasks: &mut JoinSet<()>,
) {
    if !config.ship.enabled || ship_running.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }

    if config.ship.auto_nightly.enabled {
        if nightly_next_fire.is_none() {
            *nightly_next_fire = crate::schedule::next_fire_after(
                now,
                &config.ship.auto_nightly.schedule,
                &config.ship.auto_nightly.cron_timezone,
            );
            tracing::info!(
                next_fire = ?nightly_next_fire,
                schedule = %config.ship.auto_nightly.schedule,
                "computed first auto-nightly fire time"
            );
        }
        if nightly_next_fire.is_some_and(|fire| now >= fire) {
            let config_clone = config.clone();
            let running = ship_running.clone();
            tasks.spawn(async move {
                running.store(true, std::sync::atomic::Ordering::Relaxed);
                spawn_auto_ship(&config_clone, ShipKind::Nightly).await;
                running.store(false, std::sync::atomic::Ordering::Relaxed);
            });
            *nightly_next_fire = crate::schedule::next_fire_after(
                now,
                &config.ship.auto_nightly.schedule,
                &config.ship.auto_nightly.cron_timezone,
            );
        }
    }

    if config.ship.auto_stable.enabled {
        if stable_next_fire.is_none() {
            *stable_next_fire = crate::schedule::next_fire_after(
                now,
                &config.ship.auto_stable.schedule,
                &config.ship.auto_stable.cron_timezone,
            );
            tracing::info!(
                next_fire = ?stable_next_fire,
                schedule = %config.ship.auto_stable.schedule,
                "computed first auto-stable fire time"
            );
        }
        if stable_next_fire.is_some_and(|fire| now >= fire) {
            let config_clone = config.clone();
            let running = ship_running.clone();
            tasks.spawn(async move {
                running.store(true, std::sync::atomic::Ordering::Relaxed);
                spawn_auto_ship(&config_clone, ShipKind::Stable).await;
                running.store(false, std::sync::atomic::Ordering::Relaxed);
            });
            *stable_next_fire = crate::schedule::next_fire_after(
                now,
                &config.ship.auto_stable.schedule,
                &config.ship.auto_stable.cron_timezone,
            );
        }
    }
}

async fn spawn_auto_ship(config: &Config, kind: ShipKind) {
    let auto_cfg = match kind {
        ShipKind::Nightly => &config.ship.auto_nightly,
        ShipKind::Stable => &config.ship.auto_stable,
        ShipKind::Manual => return,
    };

    let require_qualification = auto_cfg.require_qualification;
    let opts = ShipOptions {
        kind,
        require_qualification,
        format: OutputFormat::Json,
        ..ShipOptions::default()
    };

    let kind_str = kind.as_str();
    tracing::info!(kind = kind_str, "starting automated ship release");

    let result = match kind {
        ShipKind::Nightly => run_nightly(config, opts).await,
        ShipKind::Stable => run_stable(config, opts).await,
        ShipKind::Manual => unreachable!(),
    };

    match result {
        Ok(ship_result) => {
            tracing::info!(
                kind = kind_str,
                version = %ship_result.version,
                artifacts = ship_result.artifacts.len(),
                channels = ?ship_result.distributed,
                "automated ship release succeeded"
            );
            crate::audit::log_release(
                &config.repo_path,
                "daemon-auto",
                &ship_result.version,
                kind_str,
                &ship_result.distributed,
                true,
            );
            crate::daemon::notification::notify_release_success(
                &config.notify,
                &ship_result.version,
                kind_str,
                &ship_result.distributed,
                config.capabilities.network_webhooks,
            );
        }
        Err(err) => {
            tracing::error!(kind = kind_str, error = %err, "automated ship release failed");
            crate::audit::log_release(
                &config.repo_path,
                "daemon-auto",
                "unknown",
                kind_str,
                &[],
                false,
            );
            crate::daemon::notification::notify_release_failure(
                &config.notify,
                "unknown",
                kind_str,
                &err.to_string(),
                config.capabilities.network_webhooks,
            );
        }
    }
}

async fn prune_directory(dir_path: &Path, max_items: usize) {
    if !dir_path.exists() || !dir_path.is_dir() {
        return;
    }

    let Ok(mut entries) = tokio::fs::read_dir(dir_path).await else {
        return;
    };

    let mut files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(meta) = entry.metadata().await {
            if meta.is_file() {
                if let Ok(modified) = meta.modified() {
                    files.push((entry.path(), modified));
                }
            }
        }
    }

    if files.len() <= max_items {
        return;
    }

    // Sort by modified time, newest first
    files.sort_by_key(|b| std::cmp::Reverse(b.1));

    // Delete everything after max_items
    for (path, _) in files.into_iter().skip(max_items) {
        let _ = tokio::fs::remove_file(path).await;
    }
}

fn rate_limit_allows(
    now: DateTime<Utc>,
    last: Option<DateTime<Utc>>,
    min_interval: Duration,
) -> bool {
    match last {
        Some(last_commit) => {
            let elapsed = now - last_commit;
            elapsed.to_std().map(|d| d >= min_interval).unwrap_or(false)
        }
        None => true,
    }
}

/// Sanitize an LLM-generated commit narrative into a safe single-line subject.
///
/// LLM output is untrusted: it can contain newlines (which would let it forge
/// extra commit headers such as a fake `Signed-off-by`), control characters, or
/// runaway length. Keep the first line only, drop ASCII control characters,
/// trim, and clamp to 72 chars (the conventional subject limit).
fn sanitize_commit_subject(input: &str) -> String {
    let first_line = input.lines().next().unwrap_or("");
    let mut out = String::with_capacity(first_line.len().min(72));
    for ch in first_line.chars() {
        if out.len() >= 72 {
            break;
        }
        if !ch.is_control() {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn format_commit(
    cluster: &Cluster,
    diff: &DiffAnalysis,
    weight: &crate::weight::WeightResult,
    bump: Bump,
    version: &Version,
    agent_event: &Option<crate::aoc::AgentEvent>,
) -> String {
    let api_summary = if diff.api_breaking {
        "breaking-api"
    } else if diff.api_added {
        "api-added"
    } else {
        "api-stable"
    };

    let agent_info = if let Some(agent) = agent_event {
        let model = agent.model.as_deref().unwrap_or("unknown");
        format!("; agent={model}")
    } else {
        String::new()
    };

    format!(
        "kaptaind: {bump:?} -> v{version} [{api_summary}; paths={}; api_touches={}; deps={}; runtime={}; score={:.3}; cluster={}{agent_info}]",
        diff.touched_paths,
        diff.api_touches,
        diff.dependency_nodes,
        diff.runtime_paths,
        weight.score,
        cluster.id,
    )
}

pub(crate) fn load_version(path: &Path) -> Option<Version> {
    let content = std::fs::read_to_string(path).ok()?;
    Version::parse(content.trim()).ok()
}

fn save_version(path: &Path, version: &Version) -> anyhow::Result<()> {
    // Monotonic guard: never write a version below the current baseline
    // (VERSION file or manifest). A missing/unresolvable baseline skips
    // the guard — the first save is what creates VERSION.
    if let Some(repo_path) = path.parent() {
        if let Ok(baseline) = crate::version::resolve_baseline(repo_path) {
            anyhow::ensure!(
                version >= &baseline,
                "refusing version downgrade: next v{version} < current baseline v{baseline}"
            );
        }
    }

    std::fs::write(path, version.to_string())?;

    // Update Cargo.toml version in common locations, and keep Cargo.lock's
    // own-package entry in sync so the version triple (VERSION, Cargo.toml,
    // Cargo.lock) never drifts after an auto-commit.
    if let Some(repo_path) = path.parent() {
        let mut package_name: Option<String> = None;
        for cargo_rel in ["Cargo.toml", "src-tauri/Cargo.toml"] {
            let cargo_toml_path = repo_path.join(cargo_rel);
            if cargo_toml_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&cargo_toml_path) {
                    if let Ok(mut doc) = content.parse::<toml_edit::DocumentMut>() {
                        if let Some(package) = doc.get_mut("package") {
                            if package.get("version").is_some() {
                                if package_name.is_none() {
                                    package_name = package
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .map(str::to_string);
                                }
                                package["version"] = toml_edit::value(version.to_string());
                                let _ = std::fs::write(&cargo_toml_path, doc.to_string());
                            }
                        }
                    }
                }
            }
        }

        if let Some(name) = package_name {
            sync_cargo_lock(&repo_path.join("Cargo.lock"), &name, version);
        }
    }

    Ok(())
}

/// Update `[[package]]` entries named `package_name` in a Cargo.lock to
/// `version`. Best-effort: a missing/unparseable lock or a package absent
/// from it is not an error (e.g. workspace locks that don't list the crate).
fn sync_cargo_lock(lock_path: &Path, package_name: &str, version: &Version) {
    if !lock_path.exists() {
        return;
    }
    let Ok(content) = std::fs::read_to_string(lock_path) else {
        return;
    };
    let Ok(mut doc) = content.parse::<toml_edit::DocumentMut>() else {
        return;
    };
    let mut changed = false;
    if let Some(packages) = doc
        .get_mut("package")
        .and_then(|p| p.as_array_of_tables_mut())
    {
        for pkg in packages.iter_mut() {
            if pkg.get("name").and_then(|n| n.as_str()) == Some(package_name) {
                pkg["version"] = toml_edit::value(version.to_string());
                changed = true;
            }
        }
    }
    if changed {
        let _ = std::fs::write(lock_path, doc.to_string());
    }
}

fn persist_analysis_artifact(
    config: &Config,
    cluster: &Cluster,
    diff: &DiffAnalysis,
    weight: &crate::weight::WeightResult,
    bump: Bump,
    version: &Version,
    air_gapped: bool,
) -> anyhow::Result<()> {
    let dir = config.repo_path.join(".kaptaind").join("analysis");
    std::fs::create_dir_all(&dir)?;

    let artifact = AnalysisArtifact {
        cluster_id: cluster.id.to_string(),
        version: version.to_string(),
        bump: format!("{bump:?}"),
        event_count: cluster.events.len(),
        started_at: cluster.started_at,
        ended_at: cluster.ended_at,
        diff: diff.clone(),
        weight: weight.clone(),
        air_gapped,
    };

    let file_path = dir.join(format!("{}.json", cluster.id));
    let content = serde_json::to_string_pretty(&artifact)?;
    std::fs::write(file_path, content)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct AnalysisArtifact {
    pub cluster_id: String,
    pub version: String,
    pub bump: String,
    pub event_count: usize,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub diff: DiffAnalysis,
    pub weight: crate::weight::WeightResult,
    #[serde(default)]
    pub air_gapped: bool,
}

#[derive(Debug, Clone)]
pub enum TestOutcome {
    Passed,
    Failed {
        code: Option<i32>,
        stderr: String,
        failed_tests: Vec<String>,
    },
    Skipped,
}

impl TestOutcome {
    fn trace_test(&self) -> tracer::TraceTest {
        match self {
            TestOutcome::Passed => tracer::TraceTest {
                outcome: "passed".to_string(),
                stderr: None,
            },
            TestOutcome::Failed { stderr, .. } => tracer::TraceTest {
                outcome: "failed".to_string(),
                stderr: Some(stderr.clone()),
            },
            TestOutcome::Skipped => tracer::TraceTest {
                outcome: "skipped".to_string(),
                stderr: None,
            },
        }
    }
}

async fn run_test_hook(config: &Config) -> TestOutcome {
    run_test_hook_for_config(&config.test, &config.repo_path).await
}

pub async fn run_test_hook_for_config(test: &TestConfig, repo_path: &Path) -> TestOutcome {
    let Some(command) = test.command.as_deref() else {
        return TestOutcome::Skipped;
    };

    if let Err(err) = crate::util::shell_validation::validate_shell_command(command) {
        return TestOutcome::Failed {
            code: None,
            stderr: format!("rejected unsafe test hook command: {err}"),
            failed_tests: Vec::new(),
        };
    }

    match Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(repo_path)
        .output()
        .await
    {
        Ok(output) if output.status.success() => TestOutcome::Passed,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let combined = format!("{stdout}\n{stderr}");
            TestOutcome::Failed {
                code: output.status.code(),
                stderr,
                failed_tests: parse_failed_tests(&combined),
            }
        }
        Err(err) => TestOutcome::Failed {
            code: None,
            stderr: err.to_string(),
            failed_tests: Vec::new(),
        },
    }
}

/// Parse test output for lines like `test foo::bar ... FAILED`.
/// Works for `cargo test` output and similar formats.
fn parse_failed_tests(output: &str) -> Vec<String> {
    let mut tests: Vec<String> = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("test ")?;
            let name = rest.strip_suffix(" ... FAILED")?;
            Some(name.to_string())
        })
        .collect();
    tests.sort();
    tests.dedup();
    tests
}

pub fn should_block_commit(test: &TestConfig, outcome: &TestOutcome) -> bool {
    test.required && matches!(outcome, TestOutcome::Failed { .. })
}

fn apply_test_outcome(diff: &mut DiffAnalysis, outcome: &TestOutcome) {
    match outcome {
        TestOutcome::Passed => {
            diff.runtime = diff.runtime.min(0.1);
        }
        TestOutcome::Failed { .. } => {
            diff.runtime = 1.0;
        }
        TestOutcome::Skipped => {}
    }
}

fn log_test_failure(outcome: &TestOutcome) {
    if let TestOutcome::Failed {
        code,
        stderr,
        failed_tests,
    } = outcome
    {
        tracing::warn!(
            code = ?code,
            stderr = %stderr,
            failed_tests = ?failed_tests,
            "test hook failed; skipping automation"
        );
    }
}

#[derive(Debug)]
struct IgnoreMatcher {
    root: PathBuf,
    exact: HashSet<PathBuf>,
    glob: Option<GlobSet>,
}

impl IgnoreMatcher {
    fn load(root: &Path, path: &Path) -> Self {
        if !path.exists() {
            return Self {
                root: root.to_path_buf(),
                exact: HashSet::new(),
                glob: None,
            };
        }

        let mut exact = HashSet::new();
        let mut builder = GlobSetBuilder::new();

        for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if looks_like_glob(trimmed) {
                if let Ok(glob) = Glob::new(trimmed) {
                    builder.add(glob);
                }
            } else {
                exact.insert(PathBuf::from(trimmed));
            }
        }

        let glob = builder.build().ok();
        Self {
            root: root.to_path_buf(),
            exact,
            glob,
        }
    }

    fn is_ignored(&self, paths: &[PathBuf]) -> bool {
        paths.iter().any(|path| self.matches(path))
    }

    fn matches(&self, path: &Path) -> bool {
        let relative = path.strip_prefix(&self.root).ok().unwrap_or(path);
        let exact_hit = self
            .exact
            .iter()
            .any(|prefix| relative == prefix || relative.starts_with(prefix));

        if exact_hit {
            return true;
        }

        if let Some(glob) = &self.glob {
            glob.is_match(relative)
        } else {
            false
        }
    }
}

fn looks_like_glob(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[') || value.contains('{')
}

#[cfg(test)]
mod tests {
    use super::{
        apply_test_outcome, format_commit, looks_like_glob, parse_failed_tests,
        persist_analysis_artifact, rate_limit_allows, run_test_hook, save_version,
        should_block_commit, AnalysisArtifact, IgnoreMatcher, SelfWriteGuard, TestOutcome,
        SELF_WRITE_CAP, SELF_WRITE_TTL,
    };
    use crate::cluster::engine::Cluster;
    use crate::config::loader::{Config, TestConfig};
    use crate::diff::DiffAnalysis;
    use crate::watcher::{FsEvent, FsEventKind};
    use crate::weight::WeightResult;
    use chrono::{Duration as ChronoDuration, Utc};
    use semver::Version;
    use std::time::Duration;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn rate_limit_allows_first_commit() {
        let now = Utc::now();
        assert!(rate_limit_allows(now, None, Duration::from_secs(30)));
    }

    #[test]
    fn rate_limit_blocks_when_too_soon() {
        let now = Utc::now();
        let last = now - ChronoDuration::seconds(5);
        assert!(!rate_limit_allows(now, Some(last), Duration::from_secs(10)));
    }

    /// Regression (finding #3): daemon-caused writes are recognized within the
    /// TTL so the cascade loop (commit -> writeback -> cluster -> commit)
    /// cannot form.
    #[test]
    fn self_write_guard_suppresses_recorded_paths_within_ttl() {
        let mut guard = SelfWriteGuard::new();
        let version = std::path::PathBuf::from("/repo/VERSION");
        guard.record([version.clone()]);
        assert!(guard.is_self_write(&version));
        assert!(!guard.is_self_write(&std::path::PathBuf::from("/repo/src/main.rs")));
    }

    #[test]
    fn self_write_guard_expires_entries_after_ttl() {
        let mut guard = SelfWriteGuard::new();
        let path = std::path::PathBuf::from("/repo/VERSION");
        // Backdate beyond the TTL instead of sleeping.
        guard.writes.push_back((
            path.clone(),
            std::time::Instant::now() - SELF_WRITE_TTL - Duration::from_secs(1),
        ));
        assert!(!guard.is_self_write(&path));
        assert!(guard.writes.is_empty(), "expired entries are pruned");
    }

    #[test]
    fn self_write_guard_caps_memory() {
        let mut guard = SelfWriteGuard::new();
        for i in 0..(SELF_WRITE_CAP + 10) {
            guard.record([std::path::PathBuf::from(format!("/repo/file{i}"))]);
        }
        assert!(guard.writes.len() <= SELF_WRITE_CAP);
    }

    #[test]
    fn matcher_supports_repo_relative_exact_and_glob_entries() {
        let dir = tempdir().expect("temp dir");
        let repo_root = dir.path().join("repo");
        std::fs::create_dir_all(repo_root.join("target/debug")).expect("create target dir");
        std::fs::create_dir_all(repo_root.join("work/cache")).expect("create cache dir");
        let ignore_file = repo_root.join(".kaptainignore");
        std::fs::write(&ignore_file, "target\n**/*.tmp\n").expect("write ignore file");

        let matcher = IgnoreMatcher::load(&repo_root, &ignore_file);
        assert!(matcher.is_ignored(&[repo_root.join("target/debug/foo")]));
        assert!(matcher.is_ignored(&[repo_root.join("work/cache/file.tmp")]));
        assert!(!matcher.is_ignored(&[repo_root.join("src/main.rs")]));
    }

    #[test]
    fn glob_detection_works() {
        assert!(looks_like_glob("**/*.rs"));
        assert!(!looks_like_glob("src/main.rs"));
    }

    #[test]
    fn failed_required_tests_block_commits() {
        let test = TestConfig {
            command: Some("false".to_string()),
            required: true,
        };
        assert!(should_block_commit(
            &test,
            &TestOutcome::Failed {
                code: Some(1),
                stderr: String::new(),
                failed_tests: Vec::new(),
            }
        ));
    }

    #[test]
    fn parses_cargo_test_failed_lines() {
        let output = r#"
running 3 tests
test foo::bar ... ok
test foo::baz ... FAILED
test foo::qux ... FAILED
failures:
    foo::baz
    foo::qux
test result: FAILED. 1 passed; 2 failed; 0 ignored
"#;
        let failed = parse_failed_tests(output);
        assert_eq!(failed, vec!["foo::baz".to_string(), "foo::qux".to_string()]);
    }

    #[test]
    fn parse_failed_tests_deduplicates() {
        let output = "test foo::bar ... FAILED\ntest foo::bar ... FAILED\n";
        let failed = parse_failed_tests(output);
        assert_eq!(failed, vec!["foo::bar".to_string()]);
    }

    #[test]
    fn passing_test_hook_reduces_runtime_weight() {
        let mut diff = DiffAnalysis {
            runtime: 0.8,
            ..DiffAnalysis::default()
        };
        apply_test_outcome(&mut diff, &TestOutcome::Passed);
        assert_eq!(diff.runtime, 0.1);
    }

    #[test]
    fn commit_message_includes_semantic_summary() {
        let cluster = sample_cluster();
        let diff = DiffAnalysis {
            touched_paths: 4,
            api_touches: 2,
            dependency_nodes: 5,
            runtime_paths: 1,
            api_added: true,
            ..DiffAnalysis::default()
        };
        let weight = WeightResult {
            score: 0.72,
            api_breaking: false,
            api_added: true,
        };

        let message = format_commit(
            &cluster,
            &diff,
            &weight,
            crate::version::Bump::Minor,
            &Version::new(0, 2, 0),
            &None,
        );
        assert!(message.contains("api-added"));
        assert!(message.contains("paths=4"));
        assert!(message.contains("deps=5"));
    }

    #[test]
    fn persists_analysis_artifact_to_repo_state() {
        let dir = tempdir().expect("temp dir");
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        let cluster = sample_cluster();
        let diff = DiffAnalysis {
            touched_paths: 1,
            ..DiffAnalysis::default()
        };
        let weight = WeightResult {
            score: 0.25,
            api_breaking: false,
            api_added: false,
        };

        persist_analysis_artifact(
            &config,
            &cluster,
            &diff,
            &weight,
            crate::version::Bump::Patch,
            &Version::new(0, 1, 1),
            false,
        )
        .expect("persist artifact");

        let artifact_path = dir
            .path()
            .join(".kaptaind")
            .join("analysis")
            .join(format!("{}.json", cluster.id));
        let content = std::fs::read_to_string(artifact_path).expect("read artifact");
        let artifact: AnalysisArtifact = serde_json::from_str(&content).expect("parse artifact");
        assert_eq!(artifact.event_count, 1);
        assert_eq!(artifact.diff.touched_paths, 1);
    }

    #[tokio::test]
    async fn test_hook_runs_in_repo_root() {
        let dir = tempdir().expect("temp dir");
        let repo_root = dir.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create repo root");
        std::fs::write(repo_root.join("marker.txt"), "ok").expect("write marker");

        let config = Config {
            repo_path: repo_root,
            test: TestConfig {
                command: Some("test -f marker.txt".to_string()),
                required: true,
            },
            ..Config::default()
        };

        let outcome = run_test_hook(&config).await;
        assert!(matches!(outcome, TestOutcome::Passed));
    }

    #[test]
    fn test_save_version_updates_cargo_toml() {
        let dir = tempdir().expect("temp dir");
        let repo_root = dir.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create repo root");
        let cargo_toml_path = repo_root.join("Cargo.toml");

        let cargo_toml_content = r#"[package]
name = "kaptaind"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
"#;
        std::fs::write(&cargo_toml_path, cargo_toml_content).expect("write Cargo.toml");

        let version_path = repo_root.join("VERSION");
        let new_version = Version::new(0, 2, 0);

        save_version(&version_path, &new_version).expect("save version");

        let updated_cargo_toml =
            std::fs::read_to_string(&cargo_toml_path).expect("read Cargo.toml");
        assert!(updated_cargo_toml.contains("version = \"0.2.0\""));
        assert!(updated_cargo_toml.contains("anyhow = \"1\""));
    }

    #[test]
    fn test_save_version_refuses_downgrade() {
        let dir = tempdir().expect("temp dir");
        let repo_root = dir.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create repo root");
        std::fs::write(
            repo_root.join("Cargo.toml"),
            "[package]\nname = \"kaptaind\"\nversion = \"1.5.0\"\n",
        )
        .expect("write Cargo.toml");

        let version_path = repo_root.join("VERSION");
        let err = save_version(&version_path, &Version::new(1, 4, 0))
            .expect_err("downgrade must be refused");
        assert!(err.to_string().contains("downgrade"));

        // Neither file may have been touched.
        assert!(!version_path.exists());
        let cargo = std::fs::read_to_string(repo_root.join("Cargo.toml")).expect("read");
        assert!(cargo.contains("version = \"1.5.0\""));
    }

    #[test]
    fn test_save_version_allows_first_write_without_baseline() {
        let dir = tempdir().expect("temp dir");
        let repo_root = dir.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create repo root");

        let version_path = repo_root.join("VERSION");
        save_version(&version_path, &Version::new(0, 1, 0)).expect("first save");
        assert_eq!(
            std::fs::read_to_string(&version_path).expect("read VERSION"),
            "0.1.0"
        );
    }

    #[test]
    fn test_save_version_syncs_cargo_lock_own_package() {
        let dir = tempdir().expect("temp dir");
        let repo_root = dir.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create repo root");
        std::fs::write(
            repo_root.join("Cargo.toml"),
            "[package]\nname = \"kaptaind\"\nversion = \"0.1.0\"\n",
        )
        .expect("write Cargo.toml");
        std::fs::write(
            repo_root.join("Cargo.lock"),
            "# This file is automatically @generated by Cargo.\nversion = 3\n\n\
             [[package]]\nname = \"kaptaind\"\nversion = \"0.1.0\"\n\n\
             [[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n",
        )
        .expect("write Cargo.lock");

        save_version(&repo_root.join("VERSION"), &Version::new(0, 2, 0)).expect("save");

        let lock = std::fs::read_to_string(repo_root.join("Cargo.lock")).expect("read lock");
        assert!(
            lock.contains("name = \"kaptaind\"\nversion = \"0.2.0\""),
            "own package entry not updated:\n{lock}"
        );
        assert!(
            lock.contains("name = \"serde\"\nversion = \"1.0.0\""),
            "dependency entry must be untouched:\n{lock}"
        );
    }

    fn sample_cluster() -> Cluster {
        let timestamp = Utc::now();
        Cluster {
            id: Uuid::new_v4(),
            started_at: timestamp,
            ended_at: timestamp,
            events: vec![FsEvent {
                paths: vec!["src/lib.rs".into()],
                kind: FsEventKind::Modify,
                timestamp,
            }],
        }
    }
}
