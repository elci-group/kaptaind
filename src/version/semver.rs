use crate::weight::WeightResult;
use semver::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
    None,
    Patch,
    Minor,
    Major,
}

pub fn decide(weight: &WeightResult) -> Bump {
    if weight.api_breaking {
        Bump::Major
    } else if weight.api_added || weight.score > 0.6 {
        Bump::Minor
    } else if weight.score > 0.1 {
        Bump::Patch
    } else {
        Bump::None
    }
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
    use super::{apply, decide, Bump};
    use crate::weight::WeightResult;
    use semver::Version;

    #[test]
    fn decide_prefers_major_on_breaking_api() {
        let weight = WeightResult {
            score: 0.0,
            api_breaking: true,
            api_added: false,
        };
        assert_eq!(decide(&weight), Bump::Major);
    }

    #[test]
    fn decide_returns_minor_for_api_addition() {
        let weight = WeightResult {
            score: 0.0,
            api_breaking: false,
            api_added: true,
        };
        assert_eq!(decide(&weight), Bump::Minor);
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

        assert_eq!(decide(&patch), Bump::Patch);
        assert_eq!(decide(&minor), Bump::Minor);
    }

    #[test]
    fn apply_updates_semver_components() {
        let base = Version::new(1, 2, 3);
        assert_eq!(apply(base.clone(), Bump::Patch), Version::new(1, 2, 4));
        assert_eq!(apply(base.clone(), Bump::Minor), Version::new(1, 3, 0));
        assert_eq!(apply(base, Bump::Major), Version::new(2, 0, 0));
    }
}
