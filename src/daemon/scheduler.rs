use crate::cluster::engine::{Cluster, ClusterEngine};
use crate::config::loader::TestConfig;
use crate::config::Config;
use crate::diff::DiffAnalysis;
use crate::git::repo::Repo;
use crate::version::Bump;
use crate::watcher::FsEvent;
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use semver::Version;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc::Receiver;

pub async fn run(mut rx: Receiver<FsEvent>, config: Config) {
    let mut cluster_engine = ClusterEngine::new(config.cluster.window);
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
                            continue;
                        }

                        if matches!(status.status, State::Idle) {
                            status.status = State::Clustering;
                            status.last_action_time = Utc::now();
                            write_status(&config.repo_path, &status);
                        }

                        if let Some(cluster) = cluster_engine.ingest(event) {
                            process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status).await;
                        }
                    }
                    None => {
                        if let Some(cluster) = cluster_engine.flush() {
                            process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status).await;
                        }
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(config.cluster.window) => {
                if let Some(cluster) = cluster_engine.flush() {
                    process_cluster(&mut repo, &config, &mut last_commit_at, cluster, &mut status).await;
                }
            }
        }
    }
}

async fn process_cluster(
    repo: &mut Repo,
    config: &Config,
    last_commit_at: &mut Option<DateTime<Utc>>,
    cluster: Cluster,
    status: &mut StatusReport,
) {
    let now = Utc::now();
    if !rate_limit_allows(now, *last_commit_at, config.ratelimit.min_commit_interval) {
        tracing::debug!("commit rate-limited");
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
        status.status = State::Idle;
        write_status(&config.repo_path, status);
        return;
    }

    status.status = State::Testing;
    write_status(&config.repo_path, status);

    let test_outcome = run_test_hook(config).await;
    if should_block_commit(&config.test, &test_outcome) {
        log_test_failure(&test_outcome);
        status.status = State::Failed;
        if let TestOutcome::Failed { stderr, .. } = &test_outcome {
            status.last_error = Some(stderr.clone());
        }
        write_status(&config.repo_path, status);
        notify_error(config, "Tests failed");
        return;
    }

    status.status = State::Committing;
    write_status(&config.repo_path, status);

    let mut diff = crate::diff::analyze(&cluster, &config.repo_path);
    apply_test_outcome(&mut diff, &test_outcome);
    let weight = crate::weight::compute(&diff, &config.weights);
    let bump = crate::version::decide(&weight);

    if bump == Bump::None {
        tracing::debug!("no semantic version bump required");
        status.status = State::Idle;
        write_status(&config.repo_path, status);
        return;
    }

    let version_path = config.repo_path.join("VERSION");
    let previous = load_version(&version_path).unwrap_or_else(|| Version::new(0, 1, 0));
    let next = crate::version::apply(previous, bump);
    if let Err(err) = save_version(&version_path, &next) {
        tracing::error!(error = %err, path = ?version_path, "failed writing VERSION file");
        status.status = State::Failed;
        status.last_error = Some(err.to_string());
        write_status(&config.repo_path, status);
        notify_error(config, &err.to_string());
        return;
    }

    if let Err(err) = persist_analysis_artifact(config, &cluster, &diff, &weight, bump, &next) {
        tracing::warn!(error = %err, "failed to persist analysis artifact");
    }

    let msg = format_commit(&cluster, &diff, &weight, bump, &next);
    if let Err(err) = crate::commit::commit(&repo.inner, &msg) {
        tracing::error!(error = %err, "commit failed");
        status.status = State::Failed;
        status.last_error = Some(err.to_string());
        write_status(&config.repo_path, status);
        notify_error(config, &err.to_string());
        return;
    }

    if config.push.enabled {
        if let Err(err) = crate::push::push(&repo.inner, &config.push.branch) {
            tracing::warn!(error = %err, "push failed");
            status.status = State::Failed;
            status.last_error = Some(format!("push failed: {err}"));
            write_status(&config.repo_path, status);
            notify_error(config, &err.to_string());
            return;
        }
    }

    notify_commit(config, &next.to_string(), weight.score, &msg);
    
    *last_commit_at = Some(now);
    status.status = State::Idle;
    status.last_version = Some(next.to_string());
    status.last_action_time = now;
    status.last_error = None;
    write_status(&config.repo_path, status);
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
) -> String {
    let api_summary = if diff.api_breaking {
        "breaking-api"
    } else if diff.api_added {
        "api-added"
    } else {
        "api-stable"
    };

    format!(
        "kaptaind: {bump:?} -> v{version} [{api_summary}; paths={}; api_touches={}; deps={}; runtime={}; score={:.3}; cluster={}]",
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
    if let Ok(content) = serde_json::to_string_pretty(report) {
        let _ = std::fs::write(dir.join("status.json"), content);
    }
}

fn notify_commit(config: &Config, version: &str, score: f32, msg: &str) {
    if let Some(cmd) = &config.notify.on_commit {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("KAPTAIND_VERSION", version)
            .env("KAPTAIND_SCORE", score.to_string())
            .env("KAPTAIND_MSG", msg)
            .spawn();
    }
}

fn notify_error(config: &Config, error: &str) {
    if let Some(cmd) = &config.notify.on_error {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("KAPTAIND_ERROR", error)
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_test_outcome, format_commit, looks_like_glob, persist_analysis_artifact,
        rate_limit_allows, run_test_hook, should_block_commit, AnalysisArtifact, IgnoreMatcher,
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

        let message = format_commit(&cluster, &diff, &weight, crate::version::Bump::Minor, &Version::new(0, 2, 0));
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