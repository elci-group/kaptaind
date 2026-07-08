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
        let tmp = status_file.with_extension("tmp");
        if std::fs::write(&tmp, content).is_ok() {
            let _ = std::fs::rename(&tmp, &status_file);
        }
    }
}
