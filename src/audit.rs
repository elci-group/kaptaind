//! Structured audit logging for compliance and incident response.
//!
//! Writes append-only JSON Lines to `.kaptaind/audit.jsonl`. Each entry records
//! a security-relevant event (commit, push, release, qualification decision,
//! config change) with actor, timestamp, outcome, and contextual details.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

/// One audit record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO-8601 timestamp in UTC.
    pub timestamp: DateTime<Utc>,
    /// Category of event, e.g. `commit`, `push`, `release`, `qualification`.
    pub event_type: String,
    /// Actor that triggered the event (instance_id for daemon actions).
    pub actor: String,
    /// Event outcome: `success`, `failure`, `blocked`, `skipped`.
    pub result: String,
    /// Free-form structured details.
    pub details: serde_json::Value,
}

impl AuditEntry {
    pub fn new(
        event_type: impl Into<String>,
        actor: impl Into<String>,
        result: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: event_type.into(),
            actor: actor.into(),
            result: result.into(),
            details: serde_json::Value::Null,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

/// Append a single audit entry to `.kaptaind/audit.jsonl`.
pub fn append(repo_path: &Path, entry: &AuditEntry) -> anyhow::Result<()> {
    let dir = repo_path.join(".kaptaind");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("audit.jsonl");
    let line = serde_json::to_string(entry)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?
        .write_all(format!("{}\n", line).as_bytes())?;
    Ok(())
}

/// Convenience: log a successful commit.
pub fn log_commit(
    repo_path: &Path,
    actor: &str,
    version: &str,
    bump: &str,
    score: f64,
    cluster_id: &str,
    files_changed: usize,
) {
    let entry = AuditEntry::new("commit", actor, "success").with_details(serde_json::json!({
        "version": version,
        "bump": bump,
        "score": score,
        "cluster_id": cluster_id,
        "files_changed": files_changed,
    }));
    append_or_warn(repo_path, entry, "commit");
}

/// Convenience: log a push attempt.
pub fn log_push(
    repo_path: &Path,
    actor: &str,
    version: &str,
    branch: &str,
    remote: &str,
    success: bool,
    error: Option<&str>,
) {
    let entry = AuditEntry::new("push", actor, if success { "success" } else { "failure" })
        .with_details(serde_json::json!({
            "version": version,
            "branch": branch,
            "remote": remote,
            "error": error,
        }));
    append_or_warn(repo_path, entry, "push");
}

/// Convenience: log a generic event.
pub fn log_event(
    repo_path: &Path,
    actor: &str,
    event_type: &str,
    success: bool,
    details: serde_json::Value,
) {
    let entry = AuditEntry::new(
        event_type,
        actor,
        if success { "success" } else { "failure" },
    )
    .with_details(details);
    append_or_warn(repo_path, entry, event_type);
}

/// Convenience: log a release/shipment.
pub fn log_release(
    repo_path: &Path,
    actor: &str,
    version: &str,
    kind: &str,
    channels: &[String],
    success: bool,
) {
    let entry = AuditEntry::new(
        "release",
        actor,
        if success { "success" } else { "failure" },
    )
    .with_details(serde_json::json!({
        "version": version,
        "kind": kind,
        "channels": channels,
    }));
    append_or_warn(repo_path, entry, "release");
}

/// Convenience: log a qualification decision.
pub fn log_qualification(
    repo_path: &Path,
    actor: &str,
    version: &str,
    stability: f64,
    decision: &str,
    reason: Option<String>,
) {
    let entry = AuditEntry::new("qualification", actor, decision).with_details(serde_json::json!({
        "version": version,
        "stability": stability,
        "reason": reason,
    }));
    append_or_warn(repo_path, entry, "qualification");
}

fn append_or_warn(repo_path: &Path, entry: AuditEntry, kind: &str) {
    if let Err(err) = append(repo_path, &entry) {
        tracing::warn!(error = %err, kind, "failed to write audit entry");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn audit_entry_roundtrips() {
        let dir = tempdir().unwrap();
        let entry = AuditEntry::new("commit", "test@localhost", "success")
            .with_details(serde_json::json!({"version": "1.2.3"}));
        append(dir.path(), &entry).unwrap();

        let path = dir.path().join(".kaptaind").join("audit.jsonl");
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: AuditEntry = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed.event_type, "commit");
        assert_eq!(parsed.result, "success");
        assert_eq!(parsed.details["version"], "1.2.3");
    }

    #[test]
    fn log_commit_appends_record() {
        let dir = tempdir().unwrap();
        log_commit(dir.path(), "daemon", "1.2.3", "Minor", 0.75, "cluster-1", 4);
        let path = dir.path().join(".kaptaind").join("audit.jsonl");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"event_type\":\"commit\""));
        assert!(content.contains("\"version\":\"1.2.3\""));
    }
}
