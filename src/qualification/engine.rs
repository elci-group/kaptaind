use crate::qualification::policy::QualificationConfig;
use crate::stability::model::StabilityRecord;

/// Outcome of a qualification check.
#[derive(Debug, Clone, PartialEq)]
pub enum QualificationResult {
    Qualified,
    Rejected(RejectionReason),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RejectionReason {
    ScoreBelowThreshold { score: f64, required: f64 },
    StreakTooShort { streak: usize, required: usize },
    RecentHighDiffSpike { diff_score: f64, max: f64 },
    CooldownActive { seconds_remaining: u64 },
    TestsFailed,
    BuildFailed,
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScoreBelowThreshold { score, required } => {
                write!(f, "stability {score:.3} < threshold {required:.3}")
            }
            Self::StreakTooShort { streak, required } => {
                write!(f, "pass streak {streak} < required {required}")
            }
            Self::RecentHighDiffSpike { diff_score, max } => {
                write!(f, "recent diff spike {diff_score:.3} > max {max:.3}")
            }
            Self::CooldownActive { seconds_remaining } => {
                write!(f, "cooldown: {seconds_remaining}s remaining")
            }
            Self::TestsFailed => write!(f, "tests failed"),
            Self::BuildFailed => write!(f, "build failed"),
        }
    }
}

/// Evaluate whether the current state qualifies for release.
///
/// Parameters:
/// - `record`         – current stability record
/// - `config`         – qualification policy
/// - `tests_passed`   – did the test hook pass for the latest commit?
/// - `build_passed`   – did the build step succeed?
/// - `latest_diff`    – diff score of the most recent commit
/// - `last_release_ts` – Unix timestamp of the last release (for cooldown)
pub fn evaluate(
    record: &StabilityRecord,
    config: &QualificationConfig,
    tests_passed: bool,
    build_passed: bool,
    latest_diff: f64,
    last_release_ts: Option<i64>,
) -> QualificationResult {
    if !tests_passed {
        return QualificationResult::Rejected(RejectionReason::TestsFailed);
    }

    if !build_passed {
        return QualificationResult::Rejected(RejectionReason::BuildFailed);
    }

    if record.score < config.stability_threshold {
        return QualificationResult::Rejected(RejectionReason::ScoreBelowThreshold {
            score: record.score,
            required: config.stability_threshold,
        });
    }

    let streak = crate::stability::engine::pass_streak(record);
    if streak < config.min_pass_streak {
        return QualificationResult::Rejected(RejectionReason::StreakTooShort {
            streak,
            required: config.min_pass_streak,
        });
    }

    if latest_diff > config.max_allowed_diff {
        return QualificationResult::Rejected(RejectionReason::RecentHighDiffSpike {
            diff_score: latest_diff,
            max: config.max_allowed_diff,
        });
    }

    if let Some(last_ts) = last_release_ts {
        let now = chrono::Utc::now().timestamp();
        let elapsed_secs = (now - last_ts).max(0) as u64;
        let cooldown_secs = config.cooldown.as_secs();
        if elapsed_secs < cooldown_secs {
            return QualificationResult::Rejected(RejectionReason::CooldownActive {
                seconds_remaining: cooldown_secs - elapsed_secs,
            });
        }
    }

    QualificationResult::Qualified
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qualification::policy::QualificationConfig;
    use crate::stability::model::{StabilityEntry, StabilityRecord};

    fn passing_record(score: f64, streak: usize) -> StabilityRecord {
        let mut rec = StabilityRecord {
            score,
            ..Default::default()
        };
        for _ in 0..streak {
            rec.history.push(StabilityEntry {
                commit: "abc".into(),
                delta_score: 0.1,
                tests: "pass".into(),
                build: "pass".into(),
                runtime_flags: 0,
                resulting_score: score,
                timestamp: chrono::Utc::now().timestamp(),
            });
        }
        rec
    }

    #[test]
    fn qualifies_when_all_criteria_met() {
        let rec = passing_record(0.9, 3);
        let cfg = QualificationConfig::default();
        assert_eq!(evaluate(&rec, &cfg, true, true, 0.3, None), QualificationResult::Qualified);
    }

    #[test]
    fn rejects_low_stability() {
        let rec = passing_record(0.5, 3);
        let cfg = QualificationConfig::default();
        assert!(matches!(
            evaluate(&rec, &cfg, true, true, 0.3, None),
            QualificationResult::Rejected(RejectionReason::ScoreBelowThreshold { .. })
        ));
    }

    #[test]
    fn rejects_short_streak() {
        let rec = passing_record(0.9, 1);
        let cfg = QualificationConfig::default();
        assert!(matches!(
            evaluate(&rec, &cfg, true, true, 0.3, None),
            QualificationResult::Rejected(RejectionReason::StreakTooShort { .. })
        ));
    }

    #[test]
    fn rejects_diff_spike() {
        let rec = passing_record(0.9, 3);
        let cfg = QualificationConfig::default();
        assert!(matches!(
            evaluate(&rec, &cfg, true, true, 0.8, None),
            QualificationResult::Rejected(RejectionReason::RecentHighDiffSpike { .. })
        ));
    }

    #[test]
    fn rejects_during_cooldown() {
        let rec = passing_record(0.9, 3);
        let cfg = QualificationConfig::default();
        let last_ts = chrono::Utc::now().timestamp() - 10; // 10s ago, cooldown=5m
        assert!(matches!(
            evaluate(&rec, &cfg, true, true, 0.3, Some(last_ts)),
            QualificationResult::Rejected(RejectionReason::CooldownActive { .. })
        ));
    }
}
