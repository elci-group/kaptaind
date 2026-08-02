use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum State {
    Idle,
    Clustering,
    Testing,
    Committing,
    Failed,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub status: State,
    pub last_version: Option<String>,
    pub last_action_time: DateTime<Utc>,
    pub last_error: Option<String>,
    /// Human-readable label for the task currently in progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    /// Approximate completion percentage (0–100) for the current task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<u8>,
}

impl StatusReport {
    /// Mark the daemon as idle.
    pub fn set_idle(&mut self) {
        self.status = State::Idle;
        self.current_task = None;
        self.progress_percent = None;
        self.last_action_time = Utc::now();
    }

    /// Mark the daemon as working on a task.
    pub fn set_task(&mut self, state: State, task: &str, progress_percent: Option<u8>) {
        self.status = state;
        self.current_task = Some(task.to_string());
        self.progress_percent = progress_percent.map(|p| p.min(100));
        self.last_action_time = Utc::now();
    }

    /// Mark the daemon as failed with an error message.
    pub fn set_failed(&mut self, error: String) {
        self.status = State::Failed;
        self.last_error = Some(error);
        self.current_task = None;
        self.progress_percent = None;
        self.last_action_time = Utc::now();
    }
}

pub(crate) fn write_status(repo_path: &Path, report: &StatusReport) {
    let dir = repo_path.join(".kaptaind");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let status_file = dir.join("status.json");
    if let Ok(content) = serde_json::to_string_pretty(report) {
        // Atomic write: serialize to a temp file in the same directory, then
        // rename over the target so a crash mid-write can never leave a
        // truncated status.json (C2).
        let tmp = status_file.with_extension("tmp");
        if std::fs::write(&tmp, content).is_ok() {
            if let Err(error) = std::fs::rename(&tmp, &status_file) {
                tracing::warn!(
                    ?error,
                    operation = "write_status",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{write_status, State, StatusReport};
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn write_status_roundtrips_and_leaves_no_tmp_file() {
        let dir = tempdir().expect("temp dir");
        let report = StatusReport {
            status: State::Idle,
            last_version: Some("1.2.3".to_string()),
            last_action_time: Utc::now(),
            last_error: None,
            current_task: None,
            progress_percent: None,
        };

        write_status(dir.path(), &report);

        let status_file = dir.path().join(".kaptaind").join("status.json");
        let content = std::fs::read_to_string(&status_file).expect("read status");
        let parsed: StatusReport = serde_json::from_str(&content).expect("parse status");
        assert!(matches!(parsed.status, State::Idle));
        assert_eq!(parsed.last_version.as_deref(), Some("1.2.3"));
        assert!(
            !dir.path().join(".kaptaind").join("status.tmp").exists(),
            "temp file must be renamed away"
        );

        // A second write overwrites atomically.
        let mut failed = report.clone();
        failed.set_failed("boom".to_string());
        write_status(dir.path(), &failed);
        let content = std::fs::read_to_string(&status_file).expect("read status");
        let parsed: StatusReport = serde_json::from_str(&content).expect("parse status");
        assert!(matches!(parsed.status, State::Failed));
        assert_eq!(parsed.last_error.as_deref(), Some("boom"));
    }
}
