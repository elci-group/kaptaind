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
