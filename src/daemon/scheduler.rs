use crate::aoc::tracer;
use crate::cluster::engine::{Cluster, ClusterEngine};
use crate::config::loader::TestConfig;
use crate::config::Config;
use crate::diff::DiffAnalysis;
use crate::git::repo::Repo;
use crate::version::Bump;
use crate::watcher::{FsEvent, FsEventKind};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use semver::Version;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinSet;

pub async fn run(mut rx: Receiver<FsEvent>, config: Config, mut shutdown: crate::daemon::shutdown::ShutdownToken) {
    let (vacs_engine, vacs_rx) = crate::vacs::VacsEngine::new(&config.repo_path, &config.vacs);
    let vacs_engine_clone = vacs_engine.clone();

    let mut tasks: JoinSet<()> = JoinSet::new();
    tasks.spawn(async move {
        vacs_engine_clone.process_queue(vacs_rx).await;
    });

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

    let mut status = StatusReport {
        status: State::Idle,
        last_version: load_version(&config.repo_path.join("VERSION")).map(|v| v.to_string()),
        last_action_time: Utc::now(),
        last_error: None,
    };
    write_status(&config.repo_path, &status);

    loop {
        tokio::select! {
            maybe_event = rx.recv() => {
                match maybe_event {
                    Some(event) => {
                        if ignore_matcher.is_ignored(&event.paths) {
                            tracing::trace!(?event.paths, "event ignored");
                            continue;
                        }

                        if matches!(status.status, State::Idle) {
                            tracing::trace!("transitioning state to Clustering");
                            status.status = State::Clustering;
                            status.last_action_time = Utc::now();
                            write_status(&config.repo_path, &status);
                        }

                        tracing::trace!(?event.paths, "ingesting event");
                        if let Some(cluster) = cluster_engine.ingest(event) {
                            tracing::info!(cluster_id = %cluster.id, "cluster window expired by new event");
                            process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status, &vacs_engine, &mut tasks).await;
                        }
                    }
                    None => {
                        tracing::trace!("event channel closed");
                        if let Some(cluster) = cluster_engine.flush() {
                            process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status, &vacs_engine, &mut tasks).await;
                        }
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(config.cluster.window) => {
                if let Some(cluster) = cluster_engine.flush() {
                    tracing::info!(cluster_id = %cluster.id, "cluster window expired by timeout");
                    process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status, &vacs_engine, &mut tasks).await;
                }
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {
                // Task completed; reap it without blocking
            }
            _ = shutdown.wait() => {
                tracing::info!("shutdown signal received, draining tasks");
                status.status = State::Stopping;
                write_status(&config.repo_path, &status);
                break;
            }
        }
    }

    // Drain all remaining tasks with a timeout
    let drain = async {
        while tasks.join_next().await.is_some() {}
    };
    let drain_timeout = Duration::from_secs(25);
    if tokio::time::timeout(drain_timeout, drain).await.is_err() {
        tracing::warn!("task drain timeout (25s), aborting remaining tasks");
        tasks.abort_all();
    }

    status.status = State::Stopped;
    write_status(&config.repo_path, &status);
    tracing::info!("scheduler shutdown complete");
}

/// Build and write a trace record for a cluster.
fn write_trace_if_active(
    repo_path: &Path,
    cluster: &Cluster,
    result: tracer::TraceResult,
    test_outcome: &TestOutcome,
    agent_event: Option<crate::aoc::AgentEvent>,
) {
    // Check if an active AoC exists
    match crate::aoc::session::load_active(repo_path) {
        Ok(Some(session)) => {
            // Convert FsEvents to TraceEvents
            let events = cluster
                .events
                .iter()
                .map(|evt| tracer::TraceEvent {
                    paths: evt
                        .paths
                        .iter()
                        .filter_map(|p| p.to_str().map(|s| s.to_string()))
                        .collect(),
                    kind: match evt.kind {
                        FsEventKind::Create => "create".to_string(),
                        FsEventKind::Modify => "modify".to_string(),
                        FsEventKind::Remove => "remove".to_string(),
                        FsEventKind::Other => "other".to_string(),
                    },
                    t: evt.timestamp,
                })
                .collect();

            // Extract test outcome
            let test = match test_outcome {
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
            };

            let duration_ms = (cluster.ended_at - cluster.started_at)
                .num_milliseconds()
                .max(0) as u64;

            let trace = tracer::TraceRecord {
                cluster_id: cluster.id.to_string(),
                aoc_id: session.id.clone(),
                started_at: cluster.started_at,
                ended_at: cluster.ended_at,
                duration_ms,
                events,
                test,
                result,
                analysis_ref: Some(format!(".kaptaind/analysis/{}.json", cluster.id)),
                agent_event,
            };

            if let Err(err) = crate::aoc::db::save_trace(repo_path, &trace) {
                tracing::warn!(error = %err, "failed to write AoC trace to database");
            }
        }
        Ok(None) => {
            // No active AoC, skip trace
        }
        Err(err) => {
            tracing::debug!(error = %err, "failed to load active AoC");
        }
    }
}

async fn process_cluster(
    repo: &mut Repo,
    config: &Config,
    last_commit_at: &mut Option<DateTime<Utc>>,
    cluster: Cluster,
    status: &mut StatusReport,
    vacs_engine: &crate::vacs::VacsEngine,
    tasks: &mut JoinSet<()>,
) {
    let now = Utc::now();
    let mut test_outcome = TestOutcome::Skipped; // Default; will be overwritten if we get to testing

    // Attempt to link interceptor agent events matching this cluster
    let agent_events = crate::aoc::interceptor::consume_events_in_window(
        &config.repo_path,
        cluster.started_at - chrono::Duration::seconds(5),
        cluster.ended_at + chrono::Duration::seconds(5),
    ).unwrap_or_default();
            
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
            &test_outcome,
            agent_event.clone(),
        );
        status.status = State::Idle;
        write_status(&config.repo_path, status);
        return;
    }

    let is_clean = match repo.is_clean() {
        Ok(clean) => clean,
        Err(err) => {
            tracing::error!(error = %err, "failed to inspect working tree");
            status.status = State::Failed;
            status.last_error = Some(err.to_string());
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
            &test_outcome,
            agent_event.clone(),
        );
        status.status = State::Idle;
        write_status(&config.repo_path, status);
        return;
    }

    status.status = State::Testing;
    write_status(&config.repo_path, status);
    tracing::trace!("running test hook");

    test_outcome = run_test_hook(config).await;
    if should_block_commit(&config.test, &test_outcome) {
        log_test_failure(&test_outcome);
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "test_failed".to_string(),
            },
            &test_outcome,
            agent_event.clone(),
        );
        status.status = State::Failed;
        if let TestOutcome::Failed { stderr, .. } = &test_outcome {
            status.last_error = Some(stderr.clone());
        }
        write_status(&config.repo_path, status);
        crate::daemon::notification::notify_error(&config.notify, "Tests failed", Some("Test hook"));
        return;
    }

    status.status = State::Committing;
    write_status(&config.repo_path, status);

    let mut diff = crate::diff::analyze_with_plugins(&cluster, &config.repo_path, &config.plugins);
    if config.bundle.command.is_some() {
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
            &test_outcome,
            agent_event.clone(),
        );
        status.status = State::Idle;
        write_status(&config.repo_path, status);
        return;
    }

    let version_path = config.repo_path.join("VERSION");
    let previous = load_version(&version_path).unwrap_or_else(|| Version::new(0, 1, 0));
    let next = crate::version::apply(previous.clone(), bump);
    if let Err(err) = save_version(&version_path, &next) {
        tracing::error!(error = %err, path = ?version_path, "failed writing VERSION file");
        // Still write trace for visibility
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "version_write_failed".to_string(),
            },
            &test_outcome,
            agent_event.clone(),
        );
        status.status = State::Failed;
        status.last_error = Some(err.to_string());
        write_status(&config.repo_path, status);
        crate::daemon::notification::notify_error(&config.notify, &err.to_string(), None);
        return;
    }

    if let Err(err) = persist_analysis_artifact(config, &cluster, &diff, &weight, bump, &next) {
        tracing::warn!(error = %err, "failed to persist analysis artifact");
    }

    // Collect cluster paths early for staging and inference
    let cluster_paths: Vec<PathBuf> = cluster
        .events
        .iter()
        .flat_map(|e| e.paths.iter().cloned())
        .collect();

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
            Some(narrative) => format!("{narrative}\n\n{metadata_line}"),
            None => {
                tracing::warn!("ollama inference unavailable; using deterministic message");
                metadata_line
            }
        }
    } else {
        metadata_line
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
    let metrics = crate::daemon::telemetry::track_cost(&config.repo_path, input_tokens, output_tokens);
    tracing::info!(
        input_tokens = metrics.input_tokens,
        output_tokens = metrics.output_tokens,
        marginal_cost = %metrics.marginal_cost,
        aggregate_cost = %metrics.aggregate_cost,
        "Token usage and cost tracking"
    );

    if let Err(err) = crate::commit::orchestrator::commit_with_staging(
        &repo.inner,
        &msg,
        &config.staging,
        &cluster_paths,
    ) {
        tracing::error!(error = %err, "commit failed");
        write_trace_if_active(
            &config.repo_path,
            &cluster,
            tracer::TraceResult::Skipped {
                reason: "commit_failed".to_string(),
            },
            &test_outcome,
            agent_event.clone(),
        );
        status.status = State::Failed;
        status.last_error = Some(err.to_string());
        write_status(&config.repo_path, status);
        crate::daemon::notification::notify_error(&config.notify, &err.to_string(), None);
        return;
    }

    if config.push.enabled {
        if let Err(err) = crate::push::push(&repo.inner, &config.push.branch).await {
            tracing::warn!(error = %err, "push failed");
            write_trace_if_active(
                &config.repo_path,
                &cluster,
                tracer::TraceResult::Skipped {
                    reason: "push_failed".to_string(),
                },
                &test_outcome,
                agent_event.clone(),
            );
            status.status = State::Failed;
            status.last_error = Some(format!("push failed: {err}"));
            write_status(&config.repo_path, status);
            crate::daemon::notification::notify_error(&config.notify, &err.to_string(), None);
            return;
        }
    }

    let files_changed = cluster_paths.len();
crate::daemon::notification::notify_commit(&config.notify, &next.to_string(), weight.score, &msg, files_changed);

    // Write trace record for successful commit
    write_trace_if_active(
        &config.repo_path,
        &cluster,
        tracer::TraceResult::Committed {
            bump: format!("{bump:?}"),
            version: next.to_string(),
        },
        &test_outcome,
        agent_event,
    );

    *last_commit_at = Some(now);
    status.status = State::Idle;
    status.last_version = Some(next.to_string());
    status.last_action_time = now;
    status.last_error = None;
    write_status(&config.repo_path, status);

    // Fire VACS event
    let vacs_event = crate::vacs::VacsEvent {
        event_type: "commit.created".to_string(),
        timestamp: now,
        project_id: config.repo_path.display().to_string(),
        payload: crate::vacs::VacsPayload {
            files_changed: cluster_paths.iter().filter_map(|p| p.to_str().map(|s| s.to_string())).collect(),
            diff_summary: msg.clone(),
            aoc_id: None, // TODO: Extract from session if active
            complexity_score: weight.score as f64,
        },
    };
    let _ = vacs_engine.ingest(vacs_event).await;

    // Spawn post-commit qualification and release pipeline (non-blocking)
    if config.qualification.enabled {
        let repo_path = config.repo_path.clone();
        let config_clone = config.clone();
        let version_str = next.to_string();
        let commit_hash = repo
            .inner
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .map(|c| c.id().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let tests_passed = matches!(test_outcome, TestOutcome::Passed);
        let diff_f64 = weight.score as f64;
        let runtime_paths = diff.runtime_paths as u32;
        // Compute mean parse confidence from analysis metadata
        let parse_confidence = if diff.parse_metadata.is_empty() {
            1.0
        } else {
            diff.parse_metadata.iter().map(|m| m.confidence).sum::<f64>() / diff.parse_metadata.len() as f64
        };
        tasks.spawn(async move {
            crate::release::orchestrator::post_commit(
                &repo_path,
                &config_clone,
                &version_str,
                &commit_hash,
                tests_passed,
                diff_f64,
                runtime_paths,
                parse_confidence,
            )
            .await;
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
        if Utc::now().signed_duration_since(session.created_at).num_hours() > 72 {
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
        
    let test_failures = traces
        .iter()
        .filter(|t| t.test.outcome == "failed")
        .count();

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
    files.sort_by(|a, b| b.1.cmp(&a.1));

    // Delete everything after max_items
    for (path, _) in files.into_iter().skip(max_items) {
        let _ = tokio::fs::remove_file(path).await;
    }
}

fn rate_limit_allows(now: DateTime<Utc>, last: Option<DateTime<Utc>>, min_interval: Duration) -> bool {
    match last {
        Some(last_commit) => {
            let elapsed = now - last_commit;
            elapsed.to_std().map(|d| d >= min_interval).unwrap_or(false)
        }
        None => true,
    }
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

fn load_version(path: &Path) -> Option<Version> {
    let content = std::fs::read_to_string(path).ok()?;
    Version::parse(content.trim()).ok()
}

fn save_version(path: &Path, version: &Version) -> anyhow::Result<()> {
    std::fs::write(path, version.to_string())?;

    // If Cargo.toml exists, update its version
    if let Some(repo_path) = path.parent() {
        let cargo_toml_path = repo_path.join("Cargo.toml");
        if cargo_toml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml_path) {
                if let Ok(mut doc) = content.parse::<toml_edit::DocumentMut>() {
                    if let Some(package) = doc.get_mut("package") {
                        if package.get("version").is_some() {
                            package["version"] = toml_edit::value(version.to_string());
                            let _ = std::fs::write(&cargo_toml_path, doc.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn persist_analysis_artifact(
    config: &Config,
    cluster: &Cluster,
    diff: &DiffAnalysis,
    weight: &crate::weight::WeightResult,
    bump: Bump,
    version: &Version,
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
}

#[derive(Debug, Clone)]
enum TestOutcome {
    Passed,
    Failed { code: Option<i32>, stderr: String },
    Skipped,
}

async fn run_test_hook(config: &Config) -> TestOutcome {
    let Some(command) = config.test.command.as_deref() else {
        return TestOutcome::Skipped;
    };

    match Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(&config.repo_path)
        .output()
        .await
    {
        Ok(output) if output.status.success() => TestOutcome::Passed,
        Ok(output) => TestOutcome::Failed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(err) => TestOutcome::Failed {
            code: None,
            stderr: err.to_string(),
        },
    }
}

fn should_block_commit(test: &TestConfig, outcome: &TestOutcome) -> bool {
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
    if let TestOutcome::Failed { code, stderr } = outcome {
        tracing::warn!(code = ?code, stderr = %stderr, "test hook failed; skipping automation");
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

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub enum State {
    Idle,
    Clustering,
    Testing,
    Committing,
    Failed,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct StatusReport {
    pub status: State,
    pub last_version: Option<String>,
    pub last_action_time: DateTime<Utc>,
    pub last_error: Option<String>,
}

fn write_status(repo_path: &Path, report: &StatusReport) {
    let dir = repo_path.join(".kaptaind");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let status_file = dir.join("status.json");
    if let Ok(content) = serde_json::to_string_pretty(report) {
        let tmp = status_file.with_extension("tmp");
        if std::fs::write(&tmp, content).is_ok() {
            let _ = std::fs::rename(&tmp, &status_file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_test_outcome, format_commit, looks_like_glob, persist_analysis_artifact,
        rate_limit_allows, run_test_hook, save_version, should_block_commit, AnalysisArtifact, IgnoreMatcher,
        TestOutcome,
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
            }
        ));
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

        let message = format_commit(&cluster, &diff, &weight, crate::version::Bump::Minor, &Version::new(0, 2, 0), &None);
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
        
        let updated_cargo_toml = std::fs::read_to_string(&cargo_toml_path).expect("read Cargo.toml");
        assert!(updated_cargo_toml.contains("version = \"0.2.0\""));
        assert!(updated_cargo_toml.contains("anyhow = \"1\""));
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