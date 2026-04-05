use crate::config::loader::VersionThresholdConfig;
use crate::weight::WeightResult;
use semver::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
    None,
    Patch,
    Minor,
    Major,
}

/// Decide the version bump using configurable score thresholds.
///
/// Legacy callers that have no config can use `decide_default(weight)`.
pub fn decide(weight: &WeightResult, thresholds: &VersionThresholdConfig) -> Bump {
    if weight.api_breaking {
        Bump::Major
    } else if weight.api_added || weight.score > thresholds.minor {
        Bump::Minor
    } else if weight.score > thresholds.patch {
        Bump::Patch
    } else {
        Bump::None
    }
}

/// Convenience wrapper using legacy hardcoded thresholds (0.6 / 0.1).
pub fn decide_default(weight: &WeightResult) -> Bump {
    decide(weight, &VersionThresholdConfig::default())
}

pub fn apply(mut v: Version, bump: Bump) -> Version {
    match bump {
        Bump::Major => {
            v.major += 1;
            v.minor = 0;
            v.patch = 0;
        }
        Bump::Minor => {
            v.minor += 1;
            v.patch = 0;
        }
        Bump::Patch => {
            v.patch += 1;
        }
        Bump::None => {}
    }
    v
}

#[cfg(test)]
mod tests {
    use super::{apply, decide_default, Bump};
    use crate::weight::WeightResult;
    use semver::Version;

    #[test]
    fn decide_prefers_major_on_breaking_api() {
        let weight = WeightResult {
            score: 0.0,
            api_breaking: true,
            api_added: false,
        };
        assert_eq!(decide_default(&weight), Bump::Major);
    }

    #[test]
    fn decide_returns_minor_for_api_addition() {
        let weight = WeightResult {
            score: 0.0,
            api_breaking: false,
            api_added: true,
        };
        assert_eq!(decide_default(&weight), Bump::Minor);
    }

    #[test]
    fn decide_uses_score_thresholds() {
        let patch = WeightResult {
            score: 0.2,
            api_breaking: false,
            api_added: false,
        };
        let minor = WeightResult {
            score: 0.7,
            api_breaking: false,
            api_added: false,
        };

        assert_eq!(decide_default(&patch), Bump::Patch);
        assert_eq!(decide_default(&minor), Bump::Minor);
    }

    #[test]
    fn apply_updates_semver_components() {
        let base = Version::new(1, 2, 3);
        assert_eq!(apply(base.clone(), Bump::Patch), Version::new(1, 2, 4));
        assert_eq!(apply(base.clone(), Bump::Minor), Version::new(1, 3, 0));
        assert_eq!(apply(base, Bump::Major), Version::new(2, 0, 0));
    }
}
