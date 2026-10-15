//! Cluster near-identical changes before any train/test split.
//!
//! A cluster is never allowed to sit on both sides of a split.

use super::schema::{KaptaindObservation, Partitions, Split};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct PartitionPlan {
    /// Explicit temporal cutoff (`YYYY-MM-DD` or RFC3339). Empty selects an
    /// 80/20 cluster split by commit time.
    pub temporal_cutoff: Option<String>,
    pub repository_holdout_ratio: f64,
    pub pattern_holdout: Vec<String>,
    pub language_holdout: Option<String>,
}

impl Default for PartitionPlan {
    fn default() -> Self {
        Self {
            temporal_cutoff: None,
            repository_holdout_ratio: 0.2,
            pattern_holdout: Vec::new(),
            language_holdout: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupeReport {
    pub input: u64,
    pub clusters: u64,
    pub representatives: u64,
    pub pattern_holdout: Vec<String>,
    pub temporal_cutoff: String,
    pub repository_holdout: Vec<String>,
    pub language_holdout: Option<String>,
}

pub fn cluster_and_partition(
    observations: &mut [KaptaindObservation],
    plan: &PartitionPlan,
) -> DedupeReport {
    let clusters = union_clusters(observations);
    for obs in observations.iter_mut() {
        obs.cluster_id = Some(
            clusters
                .get(&obs.observation_id)
                .cloned()
                .unwrap_or_else(|| obs.observation_id.clone()),
        );
    }

    let mut members: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, obs) in observations.iter().enumerate() {
        if let Some(cluster) = &obs.cluster_id {
            members.entry(cluster.clone()).or_default().push(index);
        }
    }

    let pattern_holdout = if plan.pattern_holdout.is_empty() {
        auto_pattern_holdout(observations)
    } else {
        plan.pattern_holdout.clone()
    };
    let (temporal_cutoff, temporal_test) = temporal_assignment(observations, &members, plan);
    let (repository_holdout, repository_test) =
        repository_assignment(observations, plan.repository_holdout_ratio);
    let language_holdout = plan
        .language_holdout
        .as_ref()
        .map(|language| language.to_ascii_lowercase());

    for (cluster, indexes) in &members {
        let temporal = if members.len() < 2 {
            Split::NotApplicable
        } else if temporal_test.contains(cluster) {
            Split::Test
        } else {
            Split::Train
        };
        let repository = if repository_holdout.is_empty() {
            Split::NotApplicable
        } else if indexes
            .iter()
            .any(|index| repository_test.contains(&observations[*index].repository.repository_id))
        {
            Split::Test
        } else {
            Split::Train
        };
        let pattern = if pattern_holdout.is_empty() {
            Split::NotApplicable
        } else if indexes.iter().any(|index| {
            observations[*index]
                .change
                .semantic
                .iter()
                .any(|fact| pattern_holdout.iter().any(|held| held == &fact.kind))
        }) {
            Split::Test
        } else {
            Split::Train
        };
        let language = match &language_holdout {
            None => Split::NotApplicable,
            Some(held) => {
                if indexes
                    .iter()
                    .any(|index| observations[*index].language.as_str() == held)
                {
                    Split::Test
                } else {
                    Split::Train
                }
            }
        };
        // A cluster that any regime marks as test stays entirely in test for
        // that regime. Members never disagree.
        let partitions = Partitions {
            temporal,
            repository,
            pattern,
            language,
        };
        for index in indexes {
            observations[*index].partitions = Some(partitions.clone());
        }
    }

    DedupeReport {
        input: observations.len() as u64,
        clusters: members.len() as u64,
        representatives: members.len() as u64,
        pattern_holdout,
        temporal_cutoff,
        repository_holdout: repository_holdout.into_iter().collect(),
        language_holdout,
    }
}

/// One observation per cluster: highest confidence, then earliest timestamp,
/// then lowest observation id.
pub fn representatives(observations: &[KaptaindObservation]) -> Vec<KaptaindObservation> {
    let mut best: BTreeMap<String, KaptaindObservation> = BTreeMap::new();
    for obs in observations {
        let key = obs
            .cluster_id
            .clone()
            .unwrap_or_else(|| obs.observation_id.clone());
        match best.get(&key) {
            None => {
                best.insert(key, obs.clone());
            }
            Some(current) => {
                let better = obs.confidence > current.confidence
                    || (obs.confidence == current.confidence
                        && (obs.commit.timestamp < current.commit.timestamp
                            || (obs.commit.timestamp == current.commit.timestamp
                                && obs.observation_id < current.observation_id)));
                if better {
                    best.insert(key, obs.clone());
                }
            }
        }
    }
    best.into_values().collect()
}

fn union_clusters(observations: &[KaptaindObservation]) -> BTreeMap<String, String> {
    let mut parent: Vec<usize> = (0..observations.len()).collect();
    fn find(parent: &mut [usize], mut index: usize) -> usize {
        while parent[index] != index {
            parent[index] = parent[parent[index]];
            index = parent[index];
        }
        index
    }
    fn unite(parent: &mut [usize], left: usize, right: usize) {
        let left = find(parent, left);
        let right = find(parent, right);
        if left != right {
            let (lo, hi) = if left < right {
                (left, right)
            } else {
                (right, left)
            };
            parent[hi] = lo;
        }
    }

    let mut by_fingerprint: BTreeMap<String, usize> = BTreeMap::new();
    for (index, obs) in observations.iter().enumerate() {
        // Semantic class alone is not an identity: every public removal would
        // collapse into one cluster. Diff text and structural facts are.
        for key in [
            obs.fingerprints.diff.as_str(),
            obs.fingerprints.ast.as_str(),
        ] {
            if key.is_empty() {
                continue;
            }
            if let Some(previous) = by_fingerprint.insert(key.to_string(), index) {
                unite(&mut parent, previous, index);
            }
        }
    }

    let mut ids = BTreeMap::new();
    for index in 0..observations.len() {
        let root = find(&mut parent, index);
        let id = format!("cluster-{}", observations[root].observation_id);
        ids.insert(observations[index].observation_id.clone(), id);
    }
    ids
}

fn auto_pattern_holdout(observations: &[KaptaindObservation]) -> Vec<String> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for obs in observations {
        if let Some(kind) = obs.change.semantic.first() {
            *counts.entry(kind.kind.clone()).or_insert(0) += 1;
        }
    }
    if counts.len() < 2 {
        return Vec::new();
    }
    counts
        .into_iter()
        .min_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)))
        .map(|(kind, _)| vec![kind])
        .unwrap_or_default()
}

fn temporal_assignment(
    observations: &[KaptaindObservation],
    members: &BTreeMap<String, Vec<usize>>,
    plan: &PartitionPlan,
) -> (String, BTreeSet<String>) {
    if members.len() < 2 {
        return ("not-applicable".to_string(), BTreeSet::new());
    }
    let mut ordered: Vec<(String, String)> = members
        .keys()
        .map(|cluster| {
            let stamp = members[cluster]
                .iter()
                .filter_map(|index| observations[*index].commit.timestamp.clone())
                .max()
                .unwrap_or_default();
            (stamp, cluster.clone())
        })
        .collect();
    ordered.sort();
    if let Some(cutoff) = &plan.temporal_cutoff {
        let test = ordered
            .iter()
            .filter(|(stamp, _)| stamp.as_str() >= cutoff.as_str())
            .map(|(_, cluster)| cluster.clone())
            .collect();
        return (cutoff.clone(), test);
    }
    let test_from = ((ordered.len() as f64) * 0.8).floor() as usize;
    let test_from = test_from.clamp(1, ordered.len() - 1);
    let cutoff = ordered[test_from].0.clone();
    let test = ordered
        .iter()
        .skip(test_from)
        .map(|(_, cluster)| cluster.clone())
        .collect();
    (cutoff, test)
}

fn repository_assignment(
    observations: &[KaptaindObservation],
    ratio: f64,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let repos: BTreeSet<String> = observations
        .iter()
        .map(|obs| obs.repository.repository_id.clone())
        .collect();
    // A ratio of 0 opts out of the repository regime entirely.
    if repos.len() < 2 || ratio <= 0.0 {
        return (BTreeSet::new(), BTreeSet::new());
    }
    let mut held: BTreeSet<String> = repos
        .iter()
        .filter(|repo| repository_holdout(repo, ratio))
        .cloned()
        .collect();
    if held.is_empty() || held.len() == repos.len() {
        held.clear();
        if let Some(last) = repos.iter().next_back() {
            held.insert(last.clone());
        }
    }
    (held.clone(), held)
}

pub fn repository_holdout(repository_id: &str, ratio: f64) -> bool {
    if ratio <= 0.0 {
        return false;
    }
    if ratio >= 1.0 {
        return true;
    }
    let digest = super::represent::sha256_hex(repository_id.as_bytes());
    let sample = u32::from_str_radix(&digest[..8], 16).unwrap_or(0);
    (sample as f64 / u32::MAX as f64) < ratio
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::extract::eligibility;
    use crate::data::represent::{fingerprint_set, observation_id};
    use crate::data::schema::*;

    fn sample(repo: &str, commit: &str, file: &str, diff: &str) -> KaptaindObservation {
        let mut obs = blank(repo, commit, file);
        obs.change.textual = diff.to_string();
        obs.change.semantic = vec![SemanticFact {
            kind: if file.ends_with(".toml") {
                "DEPENDENCY_CHANGE".to_string()
            } else {
                "API_CHANGE".to_string()
            },
            consumer_compatibility: Compatibility::Breaking,
            source_compatibility: Compatibility::Breaking,
            behavioural_compatibility: Compatibility::Unknown,
        }];
        obs.fingerprints = fingerprint_set(commit, diff, &[], "", &obs.change.semantic);
        obs.training_permission = TrainingPermission::FirstParty;
        obs.authoritative_semver = Some(SemverClass::Major);
        obs.predicted_semver = SemverClass::Major;
        obs.confidence = 0.95;
        obs.provenance.label_confidence = 0.95;
        obs.provenance.label_confidence_raw = 0.95;
        obs.provenance.label_source = LabelSource::ReleaseTagWithDeterministic;
        obs.observations_in_commit = 1;
        obs.training_eligible = eligibility(&obs);
        obs
    }

    fn blank(repo: &str, commit: &str, file: &str) -> KaptaindObservation {
        KaptaindObservation {
            schema_version: KOU_SCHEMA_VERSION,
            observation_id: observation_id(repo, commit, file, "-"),
            repository: RepositoryIdentity {
                organisation: "elci-group".to_string(),
                repository_id: repo.to_string(),
                repository_name: repo.to_string(),
            },
            commit: CommitIdentity {
                sha: commit.to_string(),
                parent_sha: Some("parentparentparentparentparentparentpa".to_string()),
                timestamp: Some(format!("2024-01-0{commit}T00:00:00Z")),
            },
            language: Language::new("rust"),
            parent_state: StateFingerprint {
                commit_sha: "parent".to_string(),
                tree_sha: None,
            },
            change: ChangeRepresentation {
                textual: String::new(),
                structural: Vec::new(),
                semantic: Vec::new(),
            },
            symbols: Vec::new(),
            api: ApiImpact::default(),
            dependencies: DependencyImpact::default(),
            runtime: RuntimeImpact::default(),
            schema: SchemaImpact::default(),
            deterministic_features: DeterministicFeatures::default(),
            predicted_semver: SemverClass::Ambiguous,
            authoritative_semver: None,
            confidence: 0.9,
            provenance: Provenance {
                organisation: "elci-group".to_string(),
                repository: repo.to_string(),
                commit_sha: commit.to_string(),
                parent_sha: Some("parentparentparentparentparentparentpa".to_string()),
                file: Some(file.to_string()),
                symbol: None,
                kaptaind_version: None,
                extractor_version: "test".to_string(),
                schema_version: KOU_SCHEMA_VERSION,
                label_source: LabelSource::MatureKaptaindDecision,
                label_confidence: 0.9,
                label_confidence_raw: 0.9,
                timestamp: Some("2024-01-01T00:00:00Z".to_string()),
            },
            future_evidence: Some(FutureEvidence {
                downstream_fix: false,
                revert: false,
                followup_breakage: false,
                release_changed: false,
                consumer_failure: None,
                human_override: None,
                horizon_commits: 0,
                contradicts_label: false,
            }),
            training_permission: TrainingPermission::EvaluationOnly,
            label_scope: LabelScope::File,
            label_disputed: false,
            generated: false,
            secret_redacted: false,
            training_eligible: false,
            observations_in_commit: 1,
            fingerprints: Fingerprints::default(),
            cluster_id: None,
            partitions: None,
        }
    }

    #[test]
    fn identical_diffs_share_a_cluster_and_a_split() {
        let diff = "-pub fn a()\n+pub fn a(timeout: u64)\n";
        let mut observations = vec![
            sample(
                "alpha",
                "1111111111111111111111111111111111111111",
                "src/a.rs",
                diff,
            ),
            sample(
                "beta",
                "2222222222222222222222222222222222222222",
                "src/a.rs",
                diff,
            ),
            sample(
                "alpha",
                "3333333333333333333333333333333333333333",
                "Cargo.toml",
                "-version = 1\n+version = 2\n",
            ),
        ];
        observations[0].commit.timestamp = Some("2024-01-01T00:00:00Z".to_string());
        observations[1].commit.timestamp = Some("2024-06-01T00:00:00Z".to_string());
        observations[2].commit.timestamp = Some("2024-03-01T00:00:00Z".to_string());
        let report = cluster_and_partition(
            &mut observations,
            &PartitionPlan {
                // Small enough that sampling may select no repository; the
                // fallback still holds one out.
                repository_holdout_ratio: 0.01,
                ..PartitionPlan::default()
            },
        );
        assert_eq!(report.clusters, 2, "the repeated diff must collapse");
        assert_eq!(observations[0].cluster_id, observations[1].cluster_id);
        assert_ne!(observations[0].cluster_id, observations[2].cluster_id);
        let left = observations[0].partitions.clone().unwrap();
        let right = observations[1].partitions.clone().unwrap();
        assert_eq!(left.temporal, right.temporal);
        assert_eq!(left.repository, right.repository);
        assert!(
            report.repository_holdout.iter().any(|repo| repo == "beta")
                || report.repository_holdout.iter().any(|repo| repo == "alpha")
        );
        let reps = representatives(&observations);
        assert_eq!(reps.len(), 2);
    }

    #[test]
    fn validator_rejects_an_unknown_label() {
        let mut obs = blank(
            "alpha",
            "1111111111111111111111111111111111111111",
            "src/a.rs",
        );
        obs.provenance.label_source = LabelSource::Unknown;
        obs.authoritative_semver = Some(SemverClass::Patch);
        let (_, report) = crate::data::corpus::validate(&[obs]);
        assert_eq!(report.accepted, 0);
        assert_eq!(report.rejected, 1);
    }

    #[test]
    fn cross_regime_assignment_is_not_leakage() {
        let mut train = blank(
            "alpha",
            "1111111111111111111111111111111111111111",
            "src/a.rs",
        );
        train.cluster_id = Some("cluster-a".to_string());
        train.partitions = Some(Partitions {
            temporal: Split::Train,
            repository: Split::NotApplicable,
            pattern: Split::Test,
            language: Split::NotApplicable,
        });
        assert_eq!(
            crate::data::kds::leakage_resistance(std::slice::from_ref(&train)),
            1.0
        );

        let mut held = train.clone();
        held.observation_id = "kou-other".to_string();
        held.partitions = Some(Partitions {
            temporal: Split::Test,
            repository: Split::NotApplicable,
            pattern: Split::Test,
            language: Split::NotApplicable,
        });
        let score = crate::data::kds::leakage_resistance(&[train, held]);
        assert!(
            (score - 0.5).abs() < 1e-5,
            "temporal is split, pattern is not: {score}"
        );
    }

    #[test]
    fn zero_repository_holdout_holds_out_no_repository() {
        let observations = vec![
            blank("repo-a", "1", "a.rs"),
            blank("repo-b", "2", "b.rs"),
            blank("repo-c", "3", "c.rs"),
        ];
        let (held, _) = repository_assignment(&observations, 0.0);
        assert!(held.is_empty(), "ratio 0 must not force a holdout repo");
        let (held, _) = repository_assignment(&observations, 0.5);
        assert!(!held.is_empty() && held.len() < 3);
    }
}
