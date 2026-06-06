use semver::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
    None,
    Patch,
    Minor,
    Major,
}

/// Decide the version bump using configurable score thresholds.
pub fn decide(
    score: f32,
    api_breaking: bool,
    api_added: bool,
    minor_threshold: f32,
    patch_threshold: f32,
) -> Bump {
    if api_breaking {
        Bump::Major
    } else if api_added || score > minor_threshold {
        Bump::Minor
    } else if score > patch_threshold {
        Bump::Patch
    } else {
        Bump::None
    }
}

/// Convenience wrapper using legacy hardcoded thresholds (0.6 / 0.1).
pub fn decide_default(score: f32, api_breaking: bool, api_added: bool) -> Bump {
    decide(score, api_breaking, api_added, 0.6, 0.1)
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
    use semver::Version;

    #[test]
    fn decide_prefers_major_on_breaking_api() {
        assert_eq!(decide_default(0.0, true, false), Bump::Major);
    }

    #[test]
    fn decide_returns_minor_for_api_addition() {
        assert_eq!(decide_default(0.0, false, true), Bump::Minor);
    }

    #[test]
    fn decide_uses_score_thresholds() {
        assert_eq!(decide_default(0.2, false, false), Bump::Patch);
        assert_eq!(decide_default(0.7, false, false), Bump::Minor);
    }

    #[test]
    fn apply_updates_semver_components() {
        let base = Version::new(1, 2, 3);
        assert_eq!(apply(base.clone(), Bump::Patch), Version::new(1, 2, 4));
        assert_eq!(apply(base.clone(), Bump::Minor), Version::new(1, 3, 0));
        assert_eq!(apply(base, Bump::Major), Version::new(2, 0, 0));
    }
}
