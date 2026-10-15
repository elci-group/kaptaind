use crate::format_datetime;
use chrono::{DateTime, Utc};
use kaptaind::config::loader::Config;
use kaptaind::daemon::decisions::{outcome, DecisionRecord};
use kaptaind::daemon::scheduler::{State, StatusReport};
use kaptaind::util::style::*;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// Machine-readable daemon status (`kaptaind status --json`, AGX-4.1).
///
/// `alive` is process liveness (pid file present AND that process running);
/// `last_outcome` is the most recent pipeline result. The two are reported
/// separately so a dead daemon with an old result never reads as "running",
/// and a later success never hides behind a stale failure.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatusJson {
    pub alive: bool,
    /// Pid recorded in `.kaptaind/daemon.pid`, whether or not it is running.
    pub pid: Option<i32>,
    /// State last written to `status.json`; historical when `alive` is false.
    pub recorded_state: Option<String>,
    /// Task in progress; always `null` when the daemon is not alive.
    pub current_task: Option<String>,
    pub progress_percent: Option<u8>,
    /// `committed`, `skipped`, `failed`, `suspended`, or `null` (no result yet).
    pub last_outcome: Option<String>,
    pub last_outcome_at: Option<DateTime<Utc>>,
    /// Raw decision code from `.kaptaind/decisions.jsonl` (e.g. `test_failed`).
    pub last_decision: Option<String>,
    /// Error detail for a `failed` last outcome; `null` otherwise.
    pub last_error: Option<String>,
    /// `VERSION` as it is on disk, uncommitted edits included. The merged
    /// `status --json` keeps `version` for the lifecycle's HEAD value.
    pub worktree_version: Option<String>,
    pub suspended: bool,
    pub suspend_reason: Option<String>,
}

/// Classify a decision outcome code into a coarse `last_outcome`.
fn classify_decision(code: &str) -> &'static str {
    match code {
        outcome::COMMIT | outcome::CHORE_COMMIT => "committed",
        outcome::SUSPENDED => "suspended",
        outcome::TEST_FAILED
        | outcome::TEST_HOOK_NOT_FOUND
        | outcome::ERROR
        | outcome::COMMIT_FAILED
        | outcome::PRE_COMMIT_HOOK_FAILED
        | outcome::VERSION_WRITE_FAILED
        | outcome::BASELINE_UNRESOLVABLE
        | outcome::VERSION_MISMATCH
        | outcome::BLOCKED => "failed",
        _ => "skipped",
    }
}

fn state_name(state: &State) -> &'static str {
    match state {
        State::Idle => "idle",
        State::Clustering => "clustering",
        State::Testing => "testing",
        State::Committing => "committing",
        State::Suspended => "suspended",
        State::Failed => "failed",
        State::Stopping => "stopping",
        State::Stopped => "stopped",
    }
}

fn read_pid(repo_path: &Path) -> Option<i32> {
    fs::read_to_string(repo_path.join(".kaptaind").join("daemon.pid"))
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
}

fn read_report(repo_path: &Path) -> Option<StatusReport> {
    let status_json = repo_path.join(".kaptaind").join("status.json");
    match fs::read_to_string(&status_json) {
        Ok(content) => match serde_json::from_str::<StatusReport>(&content) {
            Ok(report) => Some(report),
            Err(error) => {
                tracing::warn!(path = %status_json.display(), error = %error, "status.json is malformed; showing status without daemon report");
                None
            }
        },
        Err(error) => {
            tracing::debug!(path = %status_json.display(), error = %error, "no status.json yet; daemon may never have run");
            None
        }
    }
}

/// Build the daemon status from the on-disk state under `repo_path`.
pub fn daemon_status(repo_path: &Path) -> DaemonStatusJson {
    let pid = read_pid(repo_path);
    let alive = pid.map(pid_is_running).unwrap_or(false);
    let report = read_report(repo_path);
    let last_decision: Option<DecisionRecord> =
        match kaptaind::daemon::decisions::tail_decisions(repo_path, 1) {
            Ok(mut records) => records.pop(),
            Err(error) => {
                tracing::debug!(%error, "decision log unreadable");
                None
            }
        };

    // A Failed status.json at least as new as the last decision wins (e.g. a
    // push failure after a commit decision); otherwise the durable decision
    // log is authoritative (status.json is reset to Idle on daemon start).
    let failed_report = report
        .as_ref()
        .filter(|r| matches!(r.status, State::Failed))
        .filter(|r| {
            last_decision
                .as_ref()
                .map(|d| r.last_action_time >= d.timestamp)
                .unwrap_or(true)
        });
    let (last_outcome, last_outcome_at, last_error) = if let Some(r) = failed_report {
        (
            Some("failed".to_string()),
            Some(r.last_action_time),
            r.last_error.clone(),
        )
    } else if let Some(d) = &last_decision {
        let class = classify_decision(&d.outcome);
        let error = (class == "failed" && !d.reason.is_empty()).then(|| d.reason.clone());
        (Some(class.to_string()), Some(d.timestamp), error)
    } else {
        (None, None, None)
    };

    let suspend = match kaptaind::daemon::suspend::load(repo_path) {
        Ok(state) => state,
        Err(error) => {
            tracing::debug!(%error, "suspend marker unreadable");
            None
        }
    };
    let worktree_version = fs::read_to_string(repo_path.join("VERSION"))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    DaemonStatusJson {
        alive,
        pid,
        recorded_state: report.as_ref().map(|r| state_name(&r.status).to_string()),
        current_task: report
            .as_ref()
            .filter(|_| alive)
            .and_then(|r| r.current_task.clone()),
        progress_percent: report
            .as_ref()
            .filter(|_| alive)
            .and_then(|r| r.progress_percent),
        last_outcome,
        last_outcome_at,
        last_decision: last_decision.map(|d| d.outcome),
        last_error,
        worktree_version,
        suspended: suspend.is_some(),
        suspend_reason: suspend.and_then(|s| s.reason),
    }
}

/// `kaptaind status --json`: daemon liveness/outcome fields merged over the
/// branch lifecycle report (when the repository has one).
pub fn handle_status_json(config: &Config) -> anyhow::Result<()> {
    let daemon = serde_json::to_value(daemon_status(&config.repo_path))?;
    let lifecycle = match kaptaind::lifecycle::status(
        &config.repo_path,
        kaptaind::lifecycle::Platform::Desktop,
    ) {
        Ok(report) => serde_json::to_value(report)?,
        Err(error) => serde_json::json!({ "lifecycle_error": error.to_string() }),
    };
    let merged = merge_status_json(lifecycle, daemon);
    println!("{}", serde_json::to_string_pretty(&merged)?);
    Ok(())
}

/// Add daemon fields to the lifecycle report without replacing any key the
/// lifecycle report already defines.
fn merge_status_json(
    mut lifecycle: serde_json::Value,
    daemon: serde_json::Value,
) -> serde_json::Value {
    if let (Some(target), serde_json::Value::Object(fields)) = (lifecycle.as_object_mut(), daemon) {
        for (key, value) in fields {
            target.entry(key).or_insert(value);
        }
    }
    lifecycle
}

pub fn handle_status(config: &Config) -> anyhow::Result<()> {
    let version_path = config.repo_path.join("VERSION");
    let version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "None (no VERSION file)".to_string()
    };

    println!("{} {}", "🚢".blue(), "Kaptaind Status".bold().blue());
    println!("{}", "=================".blue());
    println!(
        "{} {}",
        "📂 Repository: ".bold().cyan(),
        config.repo_path.display().to_string().blue()
    );
    println!("{} {}", "🏷️  Version:    ".bold().cyan(), version.magenta());

    let pid_running = get_daemon_pid(config);
    let report = read_report(&config.repo_path);
    let summary = daemon_status(&config.repo_path);

    let is_suspended = report
        .as_ref()
        .map(|r| matches!(r.status, kaptaind::daemon::scheduler::State::Suspended))
        .unwrap_or(false);

    if is_suspended {
        // Suspended state is shown even when the daemon is not running,
        // because CLI suspend/resume write status.json directly.
        println!(
            "{} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            "[⏸️  Suspended]".yellow()
        );
        if let Some(report) = report {
            if let Some(ref err) = report.last_error {
                println!("{} {}", "   Reason:    ".cyan(), err.magenta());
            }
        }
        if let Ok(Some(suspend)) = kaptaind::daemon::suspend::load(&config.repo_path) {
            println!("{} {}", "   Source:    ".cyan(), suspend.source.yellow());
            println!(
                "{} {}",
                "   Since:     ".cyan(),
                format_datetime(suspend.since).blue()
            );
        }
        if let Some(pid) = pid_running {
            println!("{}", format!("   PID:       {pid}").blue());
        }
    } else if let Some(pid) = pid_running {
        let state_display = report
            .map(|r| match r.status {
                kaptaind::daemon::scheduler::State::Idle => "[💤 Idle]".blue().to_string(),
                kaptaind::daemon::scheduler::State::Clustering => {
                    "[🔍 Clustering]".cyan().to_string()
                }
                kaptaind::daemon::scheduler::State::Testing => "[🧪 Testing]".yellow().to_string(),
                kaptaind::daemon::scheduler::State::Committing => {
                    "[🚢 Committing]".magenta().to_string()
                }
                kaptaind::daemon::scheduler::State::Failed => "[🛑 Failed]".red().to_string(),
                kaptaind::daemon::scheduler::State::Stopping => {
                    "[⏹️  Stopping]".yellow().to_string()
                }
                kaptaind::daemon::scheduler::State::Stopped => {
                    "[⏹️  Stopped]".bright_black().to_string()
                }
                kaptaind::daemon::scheduler::State::Suspended => {
                    "[⏸️  Suspended]".yellow().to_string()
                }
            })
            .unwrap_or_else(|| "[🟢 Running]".green().to_string());

        println!(
            "{} {} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            state_display,
            format!("(PID: {})", pid).blue()
        );
    } else {
        println!(
            "{} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            "🛑 Stopped".red()
        );
    }

    // Liveness and last pipeline outcome are separate facts (AGX-4.1).
    let liveness = match (summary.alive, summary.pid) {
        (true, Some(pid)) => format!("alive (PID {pid})").green().to_string(),
        (false, Some(pid)) => format!("not running (stale pid file: {pid})")
            .red()
            .to_string(),
        _ => "not running (no pid file)".red().to_string(),
    };
    println!("{} {}", "💓 Liveness:   ".bold().cyan(), liveness);
    let outcome_line = match (&summary.last_outcome, summary.last_outcome_at) {
        (Some(outcome), at) => {
            let mut line = outcome.clone();
            if let Some(code) = summary.last_decision.as_deref().filter(|c| c != outcome) {
                line.push_str(&format!(" ({code})"));
            }
            if let Some(at) = at {
                line.push_str(&format!(" at {}", format_datetime(at)));
            }
            if outcome == "failed" {
                line.red().to_string()
            } else {
                line.green().to_string()
            }
        }
        (None, _) => "none recorded".bright_black().to_string(),
    };
    println!("{} {}", "📋 Last result:".bold().cyan(), outcome_line);
    if let Some(error) = &summary.last_error {
        let first = error.lines().next().unwrap_or_default();
        println!("{} {}", "   Error:     ".cyan(), first.magenta());
    }

    Ok(())
}

fn get_daemon_pid(config: &Config) -> Option<i32> {
    read_pid(&config.repo_path).filter(|pid| pid_is_running(*pid))
}

fn pid_is_running(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        // Signal 0 checks if the process is running and we have permissions to signal it.
        unsafe { libc::kill(pid, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{daemon_status, merge_status_json};
    use chrono::{Duration, Utc};
    use kaptaind::daemon::decisions::{
        append_decision, outcome, DecisionRecord, DecisionThresholds,
    };
    use kaptaind::daemon::scheduler::{State, StatusReport};
    use kaptaind::daemon::status::write_status;
    use tempfile::tempdir;

    fn decision(code: &str, at: chrono::DateTime<Utc>) -> DecisionRecord {
        DecisionRecord {
            timestamp: at,
            cluster_id: "c".to_string(),
            outcome: code.to_string(),
            scores: None,
            thresholds: DecisionThresholds {
                minor: 0.6,
                patch: 0.1,
            },
            bump: None,
            version: None,
            members_bumped: None,
            exit_code: None,
            output_tail: None,
            missing_program: None,
            reason: String::new(),
            paths: Vec::new(),
        }
    }

    fn report(state: State, at: chrono::DateTime<Utc>) -> StatusReport {
        StatusReport {
            status: state,
            last_version: Some("1.0.0".to_string()),
            last_action_time: at,
            last_error: None,
            current_task: Some("Running tests".to_string()),
            progress_percent: Some(50),
        }
    }

    #[test]
    fn dead_pid_with_old_failure_is_not_alive_and_failed() {
        let dir = tempdir().expect("temp dir");
        let kd = dir.path().join(".kaptaind");
        std::fs::create_dir_all(&kd).unwrap();
        // Above any kernel pid_max (4_194_304): never a live process.
        std::fs::write(kd.join("daemon.pid"), "999999999\n").unwrap();
        std::fs::write(dir.path().join("VERSION"), "1.0.0\n").unwrap();
        let mut r = report(State::Idle, Utc::now() - Duration::days(3));
        r.set_failed("tests failed: exit 101".to_string());
        r.last_action_time = Utc::now() - Duration::days(3);
        write_status(dir.path(), &r);

        let status = daemon_status(dir.path());
        assert!(!status.alive);
        assert_eq!(status.pid, Some(999_999_999));
        assert_eq!(status.last_outcome.as_deref(), Some("failed"));
        assert_eq!(status.last_error.as_deref(), Some("tests failed: exit 101"));
        assert_eq!(status.recorded_state.as_deref(), Some("failed"));
        assert_eq!(status.worktree_version.as_deref(), Some("1.0.0"));
        assert!(!status.suspended);
        assert_eq!(status.current_task, None, "dead daemon has no current task");

        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["alive"], false);
        assert_eq!(json["last_outcome"], "failed");
        assert!(json.get("last_outcome_at").is_some());
    }

    #[test]
    fn later_commit_decision_supersedes_stale_failed_status() {
        let dir = tempdir().expect("temp dir");
        let mut r = report(State::Idle, Utc::now());
        r.set_failed("boom".to_string());
        r.last_action_time = Utc::now() - Duration::hours(1);
        write_status(dir.path(), &r);
        append_decision(dir.path(), &decision(outcome::COMMIT, Utc::now())).unwrap();

        let status = daemon_status(dir.path());
        assert!(!status.alive, "no pid file means not alive");
        assert_eq!(status.pid, None);
        assert_eq!(status.last_outcome.as_deref(), Some("committed"));
        assert_eq!(status.last_decision.as_deref(), Some("commit"));
        assert_eq!(status.last_error, None);
    }

    #[test]
    fn live_pid_reports_alive_and_task() {
        let dir = tempdir().expect("temp dir");
        let kd = dir.path().join(".kaptaind");
        std::fs::create_dir_all(&kd).unwrap();
        std::fs::write(kd.join("daemon.pid"), std::process::id().to_string()).unwrap();
        write_status(dir.path(), &report(State::Testing, Utc::now()));
        append_decision(
            dir.path(),
            &decision(
                outcome::TEST_HOOK_NOT_FOUND,
                Utc::now() - Duration::minutes(5),
            ),
        )
        .unwrap();

        let status = daemon_status(dir.path());
        if cfg!(unix) {
            assert!(status.alive);
            assert_eq!(status.current_task.as_deref(), Some("Running tests"));
        }
        assert_eq!(status.last_outcome.as_deref(), Some("failed"));
        assert_eq!(status.last_decision.as_deref(), Some("test_hook_not_found"));
    }

    #[test]
    fn merged_json_keeps_lifecycle_head_version() {
        let lifecycle = serde_json::json!({ "version": "1.0.0", "current_branch": "main" });
        let daemon = serde_json::json!({ "alive": false, "worktree_version": "1.1.0" });
        let merged = merge_status_json(lifecycle, daemon);
        assert_eq!(
            merged["version"], "1.0.0",
            "HEAD version must not be overwritten"
        );
        assert_eq!(merged["worktree_version"], "1.1.0");
        assert_eq!(merged["alive"], false);
        assert_eq!(merged["current_branch"], "main");
    }
}
