use serde::{Deserialize, Serialize};

/// Persistent stability record stored at `.kaptaind/stability.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StabilityRecord {
    /// Current stability score in [0, 1].
    pub score: f64,
    /// Unix timestamp of the last update.
    pub last_updated: i64,
    /// Per-commit history entries (most recent last).
    pub history: Vec<StabilityEntry>,
    /// Unix timestamp of the last regression event, if any.
    pub last_regression: Option<i64>,
}

/// A single commit's contribution to stability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityEntry {
    /// Short commit hash.
    pub commit: String,
    /// Diff score that was applied as a penalty (Δ_score, 0–1).
    pub delta_score: f64,
    /// Test result: "pass" | "fail" | "skip".
    pub tests: String,
    /// Build result: "pass" | "fail" | "skip".
    pub build: String,
    /// Number of runtime-impact paths touched (used as R proxy).
    pub runtime_flags: u32,
    /// Resulting stability score after this entry.
    pub resulting_score: f64,
    /// Unix timestamp.
    pub timestamp: i64,
    /// Mean parse confidence across all files in this commit (0.0–1.0).
    /// Used to penalize commits with uncertain parsing.
    #[serde(default = "default_confidence")]
    pub parse_confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}
