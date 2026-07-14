//! Decision transparency: every cluster decision — commit or skip — appends
//! one JSON line to `.kaptaind/decisions.jsonl` (C4).
//!
//! `kaptaind-cli explain` renders the tail of that log in human form; skip
//! decisions name the exact threshold that was not met and the achieved score.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Outcome strings used in [`DecisionRecord::outcome`].
pub mod outcome {
    pub const COMMIT: &str = "commit";
    pub const NO_BUMP: &str = "no_bump";
    /// Below-threshold cluster captured with a non-bumping `chore:` commit
    /// (`[commit] require_bump = false`, D1).
    pub const CHORE_COMMIT: &str = "chore_commit";
    pub const TEST_FAILED: &str = "test_failed";
    pub const BLOCKED: &str = "blocked";
    pub const VERSION_WRITE_FAILED: &str = "version_write_failed";
    pub const BASELINE_UNRESOLVABLE: &str = "baseline_unresolvable";
    /// `VERSION` and `Cargo.toml [package].version` disagree while
    /// `[versioning].consistency = "strict"` — commit refused.
    pub const VERSION_MISMATCH: &str = "version_mismatch";
    pub const RATE_LIMITED: &str = "rate_limited";
    pub const CLEAN_TREE: &str = "clean_tree";
    pub const ERROR: &str = "error";
    pub const PRE_COMMIT_HOOK_FAILED: &str = "pre_commit_hook_failed";
    pub const COMMIT_FAILED: &str = "commit_failed";
}

/// Score breakdown at decision time. Absent for exits that happen before diff
/// analysis (rate limit, clean tree, test failure, ...).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DecisionScores {
    pub score: f32,
    pub api: f32,
    pub deps: f32,
    pub runtime: f32,
}

/// Version thresholds in effect when the decision was made.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DecisionThresholds {
    pub minor: f32,
    pub patch: f32,
}

/// One cluster decision, appended as a single JSON line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub timestamp: DateTime<Utc>,
    pub cluster_id: String,
    /// See [`outcome`].
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scores: Option<DecisionScores>,
    pub thresholds: DecisionThresholds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Human-readable detail (e.g. error text, blocking rule).
    pub reason: String,
    /// Project-relative paths in the cluster.
    pub paths: Vec<String>,
}

/// Append one JSON line to `.kaptaind/decisions.jsonl`, creating the state
/// directory if needed.
pub fn append_decision(repo_path: &Path, record: &DecisionRecord) -> std::io::Result<()> {
    let dir = repo_path.join(".kaptaind");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("decisions.jsonl");
    let mut line = serde_json::to_string(record)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    line.push('\n');
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())
}

/// Read the last `n` decision records, oldest first.
pub fn tail_decisions(repo_path: &Path, n: usize) -> std::io::Result<Vec<DecisionRecord>> {
    let path = repo_path.join(".kaptaind").join("decisions.jsonl");
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    let records: Vec<DecisionRecord> = content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Ok(records.into_iter().rev().take(n).rev().collect())
}

/// Render decisions in human form for `kaptaind-cli explain`.
///
/// Skip decisions name the exact threshold that was not met and the achieved
/// score, e.g. `skip: no_bump — score 0.042 below patch threshold 0.100`.
pub fn render_decisions(records: &[DecisionRecord]) -> String {
    if records.is_empty() {
        return "No decisions recorded yet.".to_string();
    }

    let mut out = String::new();
    for record in records {
        let ts = record.timestamp.format("%Y-%m-%d %H:%M:%S UTC");
        let score = record.scores.map(|s| s.score);
        let line = match record.outcome.as_str() {
            outcome::COMMIT => format!(
                "[{ts}] commit: {} -> v{} (score {}, {} path{})",
                record.bump.as_deref().unwrap_or("?"),
                record.version.as_deref().unwrap_or("?"),
                score
                    .map(|s| format!("{s:.3}"))
                    .unwrap_or_else(|| "?".to_string()),
                record.paths.len(),
                if record.paths.len() == 1 { "" } else { "s" },
            ),
            outcome::NO_BUMP => match score {
                Some(s) => format!(
                    "[{ts}] skip: no_bump — score {s:.3} below patch threshold {:.3}",
                    record.thresholds.patch
                ),
                None => format!("[{ts}] skip: no_bump — {}", record.reason),
            },
            outcome::CHORE_COMMIT => match score {
                Some(s) => format!(
                    "[{ts}] commit: chore (no bump) — score {s:.3} below patch threshold {:.3}",
                    record.thresholds.patch
                ),
                None => format!("[{ts}] commit: chore (no bump) — {}", record.reason),
            },
            other => {
                let mut line = format!("[{ts}] skip: {other}");
                if !record.reason.is_empty() {
                    line.push_str(&format!(" — {}", record.reason));
                }
                if let Some(s) = score {
                    line.push_str(&format!(" (score {s:.3})"));
                }
                line
            }
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        append_decision, outcome, render_decisions, tail_decisions, DecisionRecord, DecisionScores,
        DecisionThresholds,
    };
    use chrono::Utc;
    use tempfile::tempdir;

    fn record(outcome: &str, reason: &str, score: Option<f32>) -> DecisionRecord {
        DecisionRecord {
            timestamp: Utc::now(),
            cluster_id: "cluster-1".to_string(),
            outcome: outcome.to_string(),
            scores: score.map(|s| DecisionScores {
                score: s,
                api: 0.0,
                deps: 0.0,
                runtime: 0.0,
            }),
            thresholds: DecisionThresholds {
                minor: 0.6,
                patch: 0.1,
            },
            bump: None,
            version: None,
            reason: reason.to_string(),
            paths: vec!["src/main.rs".to_string()],
        }
    }

    #[test]
    fn append_and_tail_roundtrip() {
        let dir = tempdir().expect("temp dir");
        for i in 0..5 {
            let mut r = record(outcome::NO_BUMP, "", Some(0.01 * i as f32));
            r.cluster_id = format!("cluster-{i}");
            append_decision(dir.path(), &r).expect("append");
        }

        let all = tail_decisions(dir.path(), 10).expect("tail");
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].cluster_id, "cluster-0");

        let last_two = tail_decisions(dir.path(), 2).expect("tail");
        assert_eq!(last_two.len(), 2);
        assert_eq!(last_two[0].cluster_id, "cluster-3");
        assert_eq!(last_two[1].cluster_id, "cluster-4");

        assert!(
            tail_decisions(dir.path().join("missing").as_path(), 5)
                .expect("tail")
                .is_empty(),
            "missing log yields no records"
        );
    }

    #[test]
    fn render_names_threshold_for_no_bump() {
        let r = record(outcome::NO_BUMP, "below threshold", Some(0.042));
        let rendered = render_decisions(&[r]);
        assert!(
            rendered.contains("skip: no_bump — score 0.042 below patch threshold 0.100"),
            "unexpected render: {rendered}"
        );
    }

    #[test]
    fn render_chore_commit_line() {
        let r = record(outcome::CHORE_COMMIT, "", Some(0.042));
        let rendered = render_decisions(&[r]);
        assert!(
            rendered.contains("commit: chore (no bump) — score 0.042 below patch threshold 0.100"),
            "unexpected render: {rendered}"
        );
    }

    #[test]
    fn render_commit_line() {
        let mut r = record(outcome::COMMIT, "", Some(0.432));
        r.bump = Some("Patch".to_string());
        r.version = Some("0.2.1".to_string());
        let rendered = render_decisions(&[r]);
        assert!(
            rendered.contains("commit: Patch -> v0.2.1 (score 0.432, 1 path)"),
            "unexpected render: {rendered}"
        );
    }

    #[test]
    fn render_skip_with_reason_and_no_scores() {
        let r = record(outcome::RATE_LIMITED, "min interval 10s", None);
        let rendered = render_decisions(&[r]);
        assert!(
            rendered.contains("skip: rate_limited — min interval 10s"),
            "unexpected render: {rendered}"
        );
    }

    #[test]
    fn render_empty_log() {
        assert_eq!(render_decisions(&[]), "No decisions recorded yet.");
    }
}
