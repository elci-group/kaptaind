//! Kaptaind Dataset Score. Size without quality is not progress.
//!
//! `KDS = Q × D × P × L × T`, each factor in `[0, 1]`.

use super::schema::{KaptaindObservation, SemverClass, Split};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KdsReport {
    pub score: f32,
    pub quality: f32,
    pub diversity: f32,
    pub provenance: f32,
    pub leakage: f32,
    pub temporal: f32,
    pub provisional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    pub high_confidence: u64,
    pub gate: &'static str,
    pub slm0_ready: bool,
}

pub fn readiness(high_confidence: u64) -> Readiness {
    let (gate, slm0_ready) = match high_confidence {
        0..=4_999 => ("extraction/evaluation only", false),
        5_000..=14_999 => ("classifier / embedding proof of concept", false),
        15_000..=49_999 => ("small specialist model", false),
        50_000..=149_999 => ("credible KAPTAIND-SLM/0", true),
        150_000..=499_999 => ("stronger generative specialist", true),
        _ => ("architecture scaling study", true),
    };
    Readiness {
        high_confidence,
        gate,
        slm0_ready,
    }
}

pub fn score(observations: &[KaptaindObservation]) -> KdsReport {
    if observations.is_empty() {
        return KdsReport {
            score: 0.0,
            quality: 0.0,
            diversity: 0.0,
            provenance: 0.0,
            leakage: 0.0,
            temporal: 0.0,
            provisional: false,
        };
    }
    let n = observations.len() as f32;
    let quality = observations
        .iter()
        .map(|obs| obs.provenance.label_confidence_raw.clamp(0.0, 1.0))
        .sum::<f32>()
        / n;
    let diversity = diversity(observations);
    let provenance = observations
        .iter()
        .filter(|obs| obs.provenance_complete())
        .count() as f32
        / n;
    let leakage = leakage_resistance(observations);
    let temporal = observations
        .iter()
        .filter(|obs| obs.future_evidence.is_some())
        .count() as f32
        / n;
    let raw = quality * diversity * provenance * leakage * temporal;
    KdsReport {
        score: raw.clamp(0.0, 1.0),
        quality,
        diversity,
        provenance,
        leakage,
        temporal,
        provisional: false,
    }
}

fn diversity(observations: &[KaptaindObservation]) -> f32 {
    let class = simpson_diversity(observations.iter().map(|obs| {
        obs.authoritative_semver
            .unwrap_or(SemverClass::Ambiguous)
            .as_label()
            .to_string()
    }));
    let language = simpson_diversity(
        observations
            .iter()
            .map(|obs| obs.language.as_str().to_string()),
    );
    let kind = simpson_diversity(observations.iter().map(|obs| {
        obs.change
            .semantic
            .first()
            .map(|fact| fact.kind.clone())
            .unwrap_or_else(|| "unclassified".to_string())
    }));
    (class + language + kind) / 3.0
}

fn simpson_diversity<I>(items: I) -> f32
where
    I: IntoIterator<Item = String>,
{
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut total = 0u64;
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
        total += 1;
    }
    if total == 0 || counts.len() < 2 {
        return 0.0;
    }
    let total = total as f32;
    let herfindahl: f32 = counts
        .values()
        .map(|count| {
            let p = *count as f32 / total;
            p * p
        })
        .sum();
    (1.0 - herfindahl).clamp(0.0, 1.0)
}

/// 1 when no fingerprint cluster sits on both sides of the same regime.
/// A cluster may be train for one regime and test for another. Those are
/// different questions, not leakage. Unpartitioned observations do not count.
pub(super) fn leakage_resistance(observations: &[KaptaindObservation]) -> f32 {
    let mut sides: BTreeMap<(String, &'static str), (bool, bool)> = BTreeMap::new();
    for obs in observations {
        let Some(cluster) = obs.cluster_id.as_deref() else {
            continue;
        };
        let Some(parts) = obs.partitions.as_ref() else {
            continue;
        };
        for (regime, split) in [
            ("temporal", parts.temporal),
            ("repository", parts.repository),
            ("pattern", parts.pattern),
            ("language", parts.language),
        ] {
            let (train, test) = match split {
                Split::Train => (true, false),
                Split::Test => (false, true),
                Split::NotApplicable => continue,
            };
            let entry = sides
                .entry((cluster.to_string(), regime))
                .or_insert((false, false));
            entry.0 |= train;
            entry.1 |= test;
        }
    }
    if sides.is_empty() {
        return 1.0;
    }
    let leaking = sides
        .values()
        .filter(|(train, test)| *train && *test)
        .count();
    1.0 - (leaking as f32 / sides.len() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_gate_follows_validated_volume() {
        assert!(!readiness(4_999).slm0_ready);
        assert!(!readiness(43_872).slm0_ready);
        assert_eq!(readiness(43_872).gate, "small specialist model");
        assert!(readiness(50_000).slm0_ready);
        assert_eq!(readiness(0).gate, "extraction/evaluation only");
    }

    #[test]
    fn empty_corpus_scores_zero() {
        let report = score(&[]);
        assert_eq!(report.score, 0.0);
    }
}
