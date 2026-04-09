use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A single file change event within a cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub paths: Vec<String>,
    pub kind: String,                 // "create" | "modify" | "remove" | "other"
    pub t: DateTime<Utc>,
}

/// Test outcome recorded in a trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTest {
    pub outcome: String,              // "passed" | "failed" | "skipped" | "none"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

/// The result of cluster processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TraceResult {
    #[serde(rename = "committed")]
    Committed {
        bump: String,                 // "Major" | "Minor" | "Patch"
        version: String,
    },
    #[serde(rename = "skipped")]
    Skipped {
        reason: String,               // "rate_limited" | "clean_tree" | "no_bump" | "test_failed"
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub model: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub tools: Vec<String>,
    pub latency_ms: u64,
}

/// A complete trace record for a cluster processed during an AoC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRecord {
    pub cluster_id: String,
    pub aoc_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub events: Vec<TraceEvent>,
    pub test: TraceTest,
    pub result: TraceResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_event: Option<AgentEvent>,
}

/// Write a trace record to disk.
pub fn write_trace(repo_path: &Path, record: &TraceRecord) -> anyhow::Result<()> {
    crate::aoc::db::init_db(repo_path)?;
    crate::aoc::db::save_trace(repo_path, record)
}

/// Read all trace records for a specific AoC.
pub fn read_traces_for_aoc(repo_path: &Path, aoc_id: &str) -> anyhow::Result<Vec<TraceRecord>> {
    crate::aoc::db::get_traces_for_aoc(repo_path, aoc_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn test_write_and_read_trace() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path();

        let trace = TraceRecord {
            cluster_id: Uuid::new_v4().to_string(),
            aoc_id: Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
            duration_ms: 1000,
            events: vec![TraceEvent {
                paths: vec!["src/lib.rs".to_string()],
                kind: "modify".to_string(),
                t: Utc::now(),
            }],
            test: TraceTest {
                outcome: "passed".to_string(),
                stderr: None,
            },
            result: TraceResult::Committed {
                bump: "Patch".to_string(),
                version: "0.1.1".to_string(),
            },
            analysis_ref: None,
            agent_event: None,
        };

        write_trace(repo_path, &trace).unwrap();
        let loaded = read_traces_for_aoc(repo_path, &trace.aoc_id).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].cluster_id, trace.cluster_id);
    }

    #[test]
    fn test_trace_result_serialization() {
        let committed = TraceResult::Committed {
            bump: "Minor".to_string(),
            version: "0.2.0".to_string(),
        };
        let json = serde_json::to_value(&committed).unwrap();
        assert_eq!(json["type"], "committed");

        let skipped = TraceResult::Skipped {
            reason: "rate_limited".to_string(),
        };
        let json = serde_json::to_value(&skipped).unwrap();
        assert_eq!(json["type"], "skipped");
    }
}
