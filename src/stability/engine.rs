use crate::stability::model::{StabilityEntry, StabilityRecord, TestOutcomeRecord};
use std::collections::HashSet;
use std::path::Path;

/// Number of recent per-test outcomes inspected when detecting flakiness.
const FLAKY_WINDOW: usize = 10;
/// Maximum outcomes retained per test to bound memory/storage growth.
const MAX_TEST_OUTCOMES: usize = 100;

// Directive §2.3 weights
const W1_TESTS: f64 = 0.25;
const W2_BUILD: f64 = 0.15;
const W3_DIFF: f64 = 0.30;
const W4_RUNTIME: f64 = 0.20;
const W5_CONFIDENCE: f64 = 0.10;
/// Decay per minute of wall-clock time between commits.
const LAMBDA: f64 = 0.01;

/// Apply one commit's evidence to the stability record.
///
/// `delta_t_mins` is minutes elapsed since the previous update (for time decay).
///
/// ```text
/// Sₙ = clamp(Sₙ₋₁ + w₁·T + w₂·B − w₃·Δ − w₄·R − w₅·(1−C) − λ·Δt, 0, 1)
/// ```
/// where C is the parse confidence (0–1).
pub fn update(record: &mut StabilityRecord, entry: StabilityEntry, delta_t_mins: f64) {
    let t: f64 = if entry.tests == "pass" { 1.0 } else { 0.0 };
    let b: f64 = if entry.build == "pass" { 1.0 } else { 0.0 };
    // Normalise runtime_flags into a 0–1 penalty (saturates at 5 paths).
    let r: f64 = (entry.runtime_flags as f64 / 5.0).min(1.0);
    // Parse confidence penalty: low confidence (0–1) becomes a penalty (0–1).
    let confidence_penalty: f64 = 1.0 - entry.parse_confidence;

    let was = record.score;
    let new_score = (record.score + W1_TESTS * t + W2_BUILD * b
        - W3_DIFF * entry.delta_score
        - W4_RUNTIME * r
        - W5_CONFIDENCE * confidence_penalty
        - LAMBDA * delta_t_mins.max(0.0))
    .clamp(0.0, 1.0);

    // Track regressions (score dropped by more than 0.1)
    if new_score < was - 0.1 {
        record.last_regression = Some(entry.timestamp);
    }

    let mut entry = entry;
    entry.resulting_score = new_score;

    update_test_outcomes(record, &entry);
    record.flaky_tests = detect_flaky_tests(&record.test_outcomes);

    record.score = new_score;
    record.last_updated = entry.timestamp;
    record.history.push(entry);

    // Keep history bounded to last 500 entries
    if record.history.len() > 500 {
        record.history.drain(0..record.history.len() - 500);
    }
}

/// Record per-test outcomes for the current commit and transition any
/// previously-failing tests that are now passing to a "pass" record.
fn update_test_outcomes(record: &mut StabilityRecord, entry: &StabilityEntry) {
    let current_failures: HashSet<String> = entry.failed_tests.iter().cloned().collect();

    // Mark previously-failing tests as passing if they are not in the
    // current failure set.
    for (test_name, outcomes) in &mut record.test_outcomes {
        if current_failures.contains(test_name) {
            continue;
        }
        if let Some(last) = outcomes.last() {
            if last.outcome == "fail" {
                outcomes.push(TestOutcomeRecord {
                    outcome: "pass".to_string(),
                    commit: entry.commit.clone(),
                    timestamp: entry.timestamp,
                });
            }
        }
    }

    // Record failures for tests that failed this commit.
    for test_name in &entry.failed_tests {
        record
            .test_outcomes
            .entry(test_name.clone())
            .or_default()
            .push(TestOutcomeRecord {
                outcome: "fail".to_string(),
                commit: entry.commit.clone(),
                timestamp: entry.timestamp,
            });
    }

    // Bound per-test history.
    for outcomes in record.test_outcomes.values_mut() {
        if outcomes.len() > MAX_TEST_OUTCOMES {
            outcomes.drain(0..outcomes.len() - MAX_TEST_OUTCOMES);
        }
    }
}

/// Return tests that have at least one pass and one fail within the last
/// `FLAKY_WINDOW` outcomes.
fn detect_flaky_tests(
    test_outcomes: &std::collections::HashMap<String, Vec<TestOutcomeRecord>>,
) -> Vec<String> {
    let mut flaky = Vec::new();
    for (test_name, outcomes) in test_outcomes {
        let recent_pass = outcomes
            .iter()
            .rev()
            .take(FLAKY_WINDOW)
            .any(|r| r.outcome == "pass");
        let recent_fail = outcomes
            .iter()
            .rev()
            .take(FLAKY_WINDOW)
            .any(|r| r.outcome == "fail");
        if recent_pass && recent_fail {
            flaky.push(test_name.clone());
        }
    }
    flaky.sort();
    flaky
}

/// Load stability record from `.kaptaind/stability.json`, returning a default
/// zero-score record if the file does not yet exist.
pub fn load(repo_path: &Path) -> anyhow::Result<StabilityRecord> {
    let path = repo_path.join(".kaptaind").join("stability.json");
    if !path.exists() {
        return Ok(StabilityRecord::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let record = serde_json::from_str(&content)?;
    Ok(record)
}

/// Atomically write the stability record.
pub fn save(repo_path: &Path, record: &StabilityRecord) -> anyhow::Result<()> {
    let dir = repo_path.join(".kaptaind");
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("stability.json.tmp");
    let content = serde_json::to_string_pretty(record)?;
    std::fs::write(&tmp, &content)?;
    std::fs::rename(&tmp, dir.join("stability.json"))?;
    Ok(())
}

/// Load the stability record and return the cached list of flaky tests.
pub fn flaky_tests(repo_path: &Path) -> Vec<String> {
    match load(repo_path) {
        Ok(record) => record.flaky_tests,
        Err(err) => {
            tracing::debug!(error = %err, "failed to load stability record for flaky tests");
            Vec::new()
        }
    }
}

/// Return the length of the trailing streak of entries where both tests
/// and build passed (used by the qualification engine).
pub fn pass_streak(record: &StabilityRecord) -> usize {
    record
        .history
        .iter()
        .rev()
        .take_while(|e| e.tests == "pass" && e.build == "pass")
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stability::model::StabilityEntry;

    fn entry(tests: &str, build: &str, delta: f64, runtime: u32) -> StabilityEntry {
        StabilityEntry {
            commit: "abc123".into(),
            delta_score: delta,
            tests: tests.into(),
            build: build.into(),
            runtime_flags: runtime,
            resulting_score: 0.0,
            timestamp: chrono::Utc::now().timestamp(),
            parse_confidence: 0.95,
            failed_tests: Vec::new(),
        }
    }

    fn entry_with_failures(
        tests: &str,
        build: &str,
        delta: f64,
        runtime: u32,
        failed_tests: Vec<String>,
    ) -> StabilityEntry {
        StabilityEntry {
            commit: "abc123".into(),
            delta_score: delta,
            tests: tests.into(),
            build: build.into(),
            runtime_flags: runtime,
            resulting_score: 0.0,
            timestamp: chrono::Utc::now().timestamp(),
            parse_confidence: 0.95,
            failed_tests,
        }
    }

    #[test]
    fn score_rises_on_clean_low_churn_commit() {
        let mut rec = StabilityRecord::default();
        update(&mut rec, entry("pass", "pass", 0.1, 0), 1.0);
        // T=0.25, B=0.15, diff=-0.03, decay=-0.01 → Δ=+0.36
        assert!(rec.score > 0.0);
        assert!(rec.score <= 1.0);
    }

    #[test]
    fn score_stays_in_bounds() {
        let mut rec = StabilityRecord::default();
        for _ in 0..20 {
            update(&mut rec, entry("pass", "pass", 0.0, 0), 0.0);
        }
        assert!(rec.score <= 1.0);
    }

    #[test]
    fn failed_tests_reduce_score() {
        let mut rec = StabilityRecord {
            score: 0.8,
            ..Default::default()
        };
        update(&mut rec, entry("fail", "fail", 0.9, 5), 0.0);
        assert!(rec.score < 0.8);
    }

    #[test]
    fn pass_streak_counts_trailing_passes() {
        let mut rec = StabilityRecord::default();
        update(&mut rec, entry("fail", "pass", 0.1, 0), 0.0);
        update(&mut rec, entry("pass", "pass", 0.1, 0), 0.0);
        update(&mut rec, entry("pass", "pass", 0.1, 0), 0.0);
        assert_eq!(pass_streak(&rec), 2);
    }

    #[test]
    fn flaky_test_detected_after_pass_and_fail() {
        let mut rec = StabilityRecord::default();
        update(
            &mut rec,
            entry_with_failures("fail", "pass", 0.1, 0, vec!["foo::bar".to_string()]),
            0.0,
        );
        update(&mut rec, entry("pass", "pass", 0.1, 0), 0.0);
        assert_eq!(rec.flaky_tests, vec!["foo::bar".to_string()]);
    }

    #[test]
    fn flaky_test_not_detected_when_only_failing() {
        let mut rec = StabilityRecord::default();
        for _ in 0..3 {
            update(
                &mut rec,
                entry_with_failures("fail", "pass", 0.1, 0, vec!["foo::bar".to_string()]),
                0.0,
            );
        }
        assert!(rec.flaky_tests.is_empty());
    }

    #[test]
    fn flaky_test_detected_within_window() {
        let mut rec = StabilityRecord::default();
        // 9 alternating failures/passes to stay within the default window of 10.
        for i in 0..5 {
            update(
                &mut rec,
                entry_with_failures("fail", "pass", 0.1, 0, vec!["foo::bar".to_string()]),
                0.0,
            );
            update(&mut rec, entry("pass", "pass", 0.1, 0), 0.0);
            rec.history.last_mut().unwrap().commit = format!("pass-{i}");
        }
        assert_eq!(rec.flaky_tests, vec!["foo::bar".to_string()]);
    }

    #[test]
    fn flaky_detection_ignores_outside_window() {
        let mut rec = StabilityRecord::default();
        // Build a long alternating history, then overwrite outcomes so only the
        // oldest record is a pass and the rest are fails.
        for i in 0..12 {
            update(
                &mut rec,
                entry_with_failures("fail", "pass", 0.1, 0, vec!["foo::bar".to_string()]),
                0.0,
            );
            rec.history.last_mut().unwrap().commit = format!("fail-{i}");
        }
        // Insert a single old pass at the beginning of the outcome list.
        rec.test_outcomes.get_mut("foo::bar").unwrap().insert(
            0,
            TestOutcomeRecord {
                outcome: "pass".to_string(),
                commit: "old-pass".to_string(),
                timestamp: 0,
            },
        );
        rec.flaky_tests = detect_flaky_tests(&rec.test_outcomes);
        // The only pass is now outside the trailing window of 10.
        assert!(rec.flaky_tests.is_empty());
    }

    #[test]
    fn multiple_flaky_tests_detected() {
        let mut rec = StabilityRecord::default();
        update(
            &mut rec,
            entry_with_failures(
                "fail",
                "pass",
                0.1,
                0,
                vec!["a".to_string(), "b".to_string()],
            ),
            0.0,
        );
        update(
            &mut rec,
            entry_with_failures("pass", "pass", 0.1, 0, vec![]),
            0.0,
        );
        assert_eq!(rec.flaky_tests, vec!["a".to_string(), "b".to_string()]);
    }
}
