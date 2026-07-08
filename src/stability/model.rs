use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Per-test outcome history used to detect flaky tests.
    #[serde(default)]
    pub test_outcomes: HashMap<String, Vec<TestOutcomeRecord>>,
    /// Cached list of currently detected flaky tests.
    #[serde(default)]
    pub flaky_tests: Vec<String>,
}

/// A single pass/fail observation for a named test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestOutcomeRecord {
    /// Outcome string: "pass" or "fail".
    pub outcome: String,
    /// Short commit hash the observation belongs to.
    pub commit: String,
    /// Unix timestamp of the observation.
    pub timestamp: i64,
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
    /// Names of tests that failed in this commit (empty if tests passed).
    #[serde(default)]
    pub failed_tests: Vec<String>,
}

fn default_confidence() -> f64 {
    1.0
}
