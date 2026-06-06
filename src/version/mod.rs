pub use kaptaind_diff::version::{apply, Bump};

/// Decide the version bump using configurable score thresholds.
pub fn decide(weight: &crate::weight::WeightResult, thresholds: &crate::config::loader::VersionThresholdConfig) -> Bump {
    kaptaind_diff::version::decide(
        weight.score,
        weight.api_breaking,
        weight.api_added,
        thresholds.minor,
        thresholds.patch,
    )
}

/// Convenience wrapper using legacy hardcoded thresholds (0.6 / 0.1).
pub fn decide_default(weight: &crate::weight::WeightResult) -> Bump {
    decide(weight, &crate::config::loader::VersionThresholdConfig::default())
}
