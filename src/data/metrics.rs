//! Evaluation quantities for a later model. Defined now so extraction and
//! shadow reports share one meaning of "how wrong".

use super::schema::SemverClass;

/// Absolute distance on the NoBump < Patch < Minor < Major line.
/// Ambiguous predictions are excluded (`None`).
pub fn semver_distance(predicted: SemverClass, truth: SemverClass) -> Option<u32> {
    let predicted = predicted.rank()?;
    let truth = truth.rank()?;
    Some(predicted.abs_diff(truth))
}

/// Penalty for understating compatibility impact. Over-versioning scores 0.
/// Predicting Patch or NoBump for a Major change is weighted above the raw
/// squared gap.
pub fn under_version_weight(truth: SemverClass, predicted: SemverClass) -> Option<f32> {
    let truth_rank = truth.rank()?;
    let predicted_rank = predicted.rank()?;
    if predicted_rank >= truth_rank {
        return Some(0.0);
    }
    let gap = (truth_rank - predicted_rank) as f32;
    let mut weight = gap * gap;
    if truth == SemverClass::Major && predicted_rank <= SemverClass::Patch.rank().unwrap_or(1) {
        weight *= 2.0;
    }
    Some(weight)
}

pub fn is_false_safe(truth: SemverClass, predicted: SemverClass) -> Option<bool> {
    let truth_rank = truth.rank()?;
    let predicted_rank = predicted.rank()?;
    Some(predicted_rank < truth_rank)
}

/// False-safe rate over pairs whose classes both have a rank.
pub fn false_safe_rate(pairs: &[(SemverClass, SemverClass)]) -> Option<f32> {
    let mut judged = 0u32;
    let mut unsafe_ok = 0u32;
    for &(truth, predicted) in pairs {
        if let Some(false_safe) = is_false_safe(truth, predicted) {
            judged += 1;
            if false_safe {
                unsafe_ok += 1;
            }
        }
    }
    if judged == 0 {
        None
    } else {
        Some(unsafe_ok as f32 / judged as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_versioning_a_break_costs_more_than_a_patch_miss() {
        let patch_as_major_gap = semver_distance(SemverClass::Patch, SemverClass::Major).unwrap();
        let minor_as_patch_gap = semver_distance(SemverClass::Minor, SemverClass::Patch).unwrap();
        assert_eq!(patch_as_major_gap, 2);
        assert_eq!(minor_as_patch_gap, 1);

        let break_called_patch =
            under_version_weight(SemverClass::Major, SemverClass::Patch).unwrap();
        let minor_called_patch =
            under_version_weight(SemverClass::Minor, SemverClass::Patch).unwrap();
        let patch_called_minor =
            under_version_weight(SemverClass::Patch, SemverClass::Minor).unwrap();
        assert!(break_called_patch > minor_called_patch);
        assert_eq!(patch_called_minor, 0.0);
        assert_eq!(
            is_false_safe(SemverClass::Major, SemverClass::Patch),
            Some(true)
        );
        assert_eq!(
            false_safe_rate(&[
                (SemverClass::Major, SemverClass::Patch),
                (SemverClass::Patch, SemverClass::Patch),
                (SemverClass::Ambiguous, SemverClass::Minor),
            ]),
            Some(0.5)
        );
    }
}
