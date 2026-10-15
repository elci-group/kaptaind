//! Rebuild observations from git history and on-disk Kaptaind records.
//!
//! Checkouts are not used. Before and after file bytes come from `git show`,
//! and the patch comes from `git diff-tree`.

use super::gitio::{self, GitCommitMeta};
use super::label::{self, Scorecard};
use super::represent::{self, SignatureEdit};
use super::schema::*;
use crate::daemon::decisions::DecisionRecord;
use crate::diff::lang::adapter::AstDiff;
use crate::diff::lang::registry::AdapterRegistry;
use anyhow::Context;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const DIFF_CAP: usize = 12_000;
const FILE_CAP: usize = 256_000;
const LABEL_GREP: &str = r"kaptaind: (Major|Minor|Patch|None) ->|kaptaind: no-bump|^Kaptaind-";

#[derive(Debug, Clone)]
pub struct ExtractOptions {
    pub repo: PathBuf,
    pub organisation: Option<String>,
    pub include_weak: bool,
    pub horizon: u32,
    pub limit: Option<usize>,
    pub first_party_orgs: Vec<String>,
}

#[derive(Debug, Clone)]
struct Candidate {
    sha: Option<String>,
    parent: Option<String>,
    timestamp: Option<String>,
    subject: String,
    cluster_id: Option<String>,
    paths: Vec<String>,
    class: Option<SemverClass>,
    source: LabelSource,
    confidence: f32,
    features: DeterministicFeatures,
    api_touches: u32,
    api_signatures: u32,
    dependency_nodes: u32,
    runtime_paths: u32,
}

pub fn extract(options: &ExtractOptions) -> anyhow::Result<Vec<KaptaindObservation>> {
    let repo = &options.repo;
    gitio::head_sha(repo).context("repository has no commits to extract")?;
    let slug = gitio::origin_url(repo).and_then(|url| gitio::github_slug(&url));
    let organisation = options
        .organisation
        .clone()
        .or_else(|| slug.as_ref().map(|(org, _)| org.clone()))
        .unwrap_or_else(|| "local".to_string());
    let name = slug
        .as_ref()
        .map(|(_, name)| name.clone())
        .or_else(|| {
            repo.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "repository".to_string());
    let mut permission = if options
        .first_party_orgs
        .iter()
        .any(|org| org.eq_ignore_ascii_case(&organisation))
    {
        TrainingPermission::FirstParty
    } else {
        TrainingPermission::EvaluationOnly
    };
    if let Ok(text) = std::fs::read_to_string(repo.join(".kaptaind").join("training-permission")) {
        if let Some(parsed) = TrainingPermission::parse_label(text.lines().next().unwrap_or("")) {
            permission = parsed;
        }
    }
    let identity = RepositoryIdentity {
        repository_id: if organisation == "local" {
            format!("local:{name}")
        } else {
            format!("{organisation}/{name}")
        },
        organisation: organisation.clone(),
        repository_name: name,
    };

    let mut candidates = gather_candidates(repo, options.include_weak)?;
    apply_human_labels(repo, &mut candidates);
    apply_release_tags(repo, &mut candidates);
    candidates.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then(left.sha.cmp(&right.sha))
            .then(left.cluster_id.cmp(&right.cluster_id))
    });
    if let Some(limit) = options.limit {
        candidates.truncate(limit);
    }

    let registry = AdapterRegistry::default_registry();
    let mut observations = Vec::new();
    let total = candidates.len();
    for (index, candidate) in candidates.iter().enumerate() {
        observations.extend(observe_candidate(
            repo, &identity, permission, candidate, &registry,
        ));
        if total >= 250 && (index + 1).is_multiple_of(250) {
            eprintln!(
                "kaptaind data extract: {done}/{total} commits, {observations} observations",
                done = index + 1,
                observations = observations.len(),
            );
        }
    }
    if total >= 250 {
        eprintln!(
            "kaptaind data extract: {total}/{total} commits, {} observations, auditing later commits",
            observations.len(),
        );
    }
    stamp_commit_groups(&mut observations);
    attach_future_evidence(repo, options.horizon, &mut observations);
    let _ = std::fs::remove_dir_all(
        std::env::temp_dir().join(format!("kaptaind-kou-{}", std::process::id())),
    );
    Ok(observations)
}

fn gather_candidates(repo: &Path, include_weak: bool) -> anyhow::Result<Vec<Candidate>> {
    let mut by_key: BTreeMap<String, Candidate> = BTreeMap::new();
    let labelled = gitio::log_matching(repo, LABEL_GREP).unwrap_or_default();
    for commit in &labelled {
        if let Some(card) = label::parse_scorecard(&commit.message()) {
            let key = card
                .cluster_id
                .clone()
                .unwrap_or_else(|| format!("commit:{}", commit.sha));
            let candidate = candidate_from_scorecard(commit, &card);
            by_key.insert(key, candidate);
        } else if label::has_kaptaind_trailer(&commit.message()) {
            let key = format!("commit:{}", commit.sha);
            by_key.entry(key).or_insert_with(|| Candidate {
                sha: Some(commit.sha.clone()),
                parent: commit.parent().map(str::to_string),
                timestamp: Some(commit.committed_at.clone()),
                subject: commit.subject.clone(),
                cluster_id: None,
                paths: Vec::new(),
                class: None,
                source: LabelSource::Unknown,
                confidence: 0.0,
                features: DeterministicFeatures::default(),
                api_touches: 0,
                api_signatures: 0,
                dependency_nodes: 0,
                runtime_paths: 0,
            });
        }
    }

    for record in read_decisions(repo) {
        if record.scores.is_none() && record.bump.is_none() {
            continue;
        }
        let key = record.cluster_id.clone();
        let incoming = candidate_from_decision(&record);
        match by_key.get_mut(&key) {
            Some(existing) => merge_candidate(existing, incoming),
            None => {
                by_key.insert(key, incoming);
            }
        }
    }

    for artifact in read_artifacts(repo) {
        let key = artifact.cluster_id.clone();
        let incoming = candidate_from_artifact(&artifact);
        match by_key.get_mut(&key) {
            Some(existing) => merge_candidate(existing, incoming),
            None => {
                by_key.insert(key, incoming);
            }
        }
    }

    if include_weak {
        let commits = gitio::log_all_subjects(repo, Some(2_000)).unwrap_or_default();
        for commit in commits {
            if label::parse_scorecard(&commit.message()).is_some() {
                continue;
            }
            let Some(class) = label::conventional_class(&commit.message()) else {
                continue;
            };
            let key = format!("commit:{}", commit.sha);
            by_key.entry(key).or_insert_with(|| Candidate {
                sha: Some(commit.sha.clone()),
                parent: commit.parent().map(str::to_string),
                timestamp: Some(commit.committed_at.clone()),
                subject: commit.subject.clone(),
                cluster_id: None,
                paths: Vec::new(),
                class: Some(class),
                source: LabelSource::ConventionalCommitInference,
                confidence: label::confidence(LabelSource::ConventionalCommitInference),
                features: DeterministicFeatures::default(),
                api_touches: 0,
                api_signatures: 0,
                dependency_nodes: 0,
                runtime_paths: 0,
            });
        }
    }

    Ok(by_key.into_values().collect())
}

fn candidate_from_scorecard(commit: &GitCommitMeta, card: &Scorecard) -> Candidate {
    let source = LabelSource::MatureKaptaindDecision;
    Candidate {
        sha: Some(commit.sha.clone()),
        parent: commit.parent().map(str::to_string),
        timestamp: Some(commit.committed_at.clone()),
        subject: commit.subject.clone(),
        cluster_id: card.cluster_id.clone(),
        paths: Vec::new(),
        class: Some(card.class),
        source,
        confidence: label::confidence(source),
        features: DeterministicFeatures {
            score: card.score,
            runtime_change: card.runtime > 0,
            ..DeterministicFeatures::default()
        },
        api_touches: card.api_touches,
        api_signatures: 0,
        dependency_nodes: card.deps,
        runtime_paths: card.runtime,
    }
}

fn candidate_from_decision(record: &DecisionRecord) -> Candidate {
    let class = record.bump.as_deref().and_then(SemverClass::parse_label);
    let (source, confidence) = if class.is_some() && record.scores.is_some() {
        let source = LabelSource::MatureKaptaindDecision;
        (source, label::confidence(source))
    } else if record.scores.is_some() || class.is_some() {
        let source = LabelSource::EarlierKaptaindGeneration;
        (source, 0.75)
    } else {
        (LabelSource::Unknown, 0.0)
    };
    let scores = record
        .scores
        .unwrap_or(crate::daemon::decisions::DecisionScores {
            score: 0.0,
            api: 0.0,
            deps: 0.0,
            runtime: 0.0,
        });
    Candidate {
        sha: None,
        parent: None,
        timestamp: Some(record.timestamp.to_rfc3339()),
        subject: record.reason.clone(),
        cluster_id: Some(record.cluster_id.clone()),
        paths: record.paths.clone(),
        class,
        source,
        confidence,
        features: DeterministicFeatures {
            api: scores.api,
            deps: scores.deps,
            runtime: scores.runtime,
            score: scores.score,
            runtime_change: scores.runtime > 0.0,
            ..DeterministicFeatures::default()
        },
        api_touches: 0,
        api_signatures: 0,
        dependency_nodes: 0,
        runtime_paths: 0,
    }
}

#[derive(serde::Deserialize)]
struct LooseArtifact {
    cluster_id: String,
    #[serde(default)]
    bump: String,
    #[serde(default)]
    diff: LooseDiff,
    #[serde(default)]
    weight: LooseWeight,
}

#[derive(serde::Deserialize, Default)]
#[allow(dead_code)]
struct LooseDiff {
    #[serde(default)]
    structural: f32,
    #[serde(default)]
    api: f32,
    #[serde(default)]
    deps: f32,
    #[serde(default)]
    runtime: f32,
    #[serde(default)]
    bundle: f32,
    #[serde(default)]
    api_breaking: bool,
    #[serde(default)]
    api_added: bool,
    #[serde(default)]
    api_touches: usize,
    #[serde(default)]
    api_signatures: usize,
    #[serde(default)]
    dependency_nodes: usize,
    #[serde(default)]
    runtime_paths: usize,
}

#[derive(serde::Deserialize, Default)]
struct LooseWeight {
    #[serde(default)]
    score: f32,
}

fn candidate_from_artifact(artifact: &LooseArtifact) -> Candidate {
    let class = SemverClass::parse_label(&artifact.bump);
    let source = if class.is_some() {
        LabelSource::MatureKaptaindDecision
    } else {
        LabelSource::EarlierKaptaindGeneration
    };
    Candidate {
        sha: None,
        parent: None,
        timestamp: None,
        subject: String::new(),
        cluster_id: Some(artifact.cluster_id.clone()),
        paths: Vec::new(),
        class,
        source,
        confidence: if class.is_some() {
            label::confidence(source)
        } else {
            0.75
        },
        features: DeterministicFeatures {
            structural: artifact.diff.structural,
            api: artifact.diff.api,
            deps: artifact.diff.deps,
            runtime: artifact.diff.runtime,
            bundle: artifact.diff.bundle,
            score: artifact.weight.score,
            runtime_change: artifact.diff.runtime_paths > 0,
            ..DeterministicFeatures::default()
        },
        api_touches: artifact.diff.api_touches as u32,
        api_signatures: artifact.diff.api_signatures as u32,
        dependency_nodes: artifact.diff.dependency_nodes as u32,
        runtime_paths: artifact.diff.runtime_paths as u32,
    }
}

fn merge_candidate(existing: &mut Candidate, incoming: Candidate) {
    if existing.sha.is_none() {
        existing.sha = incoming.sha;
    }
    if existing.parent.is_none() {
        existing.parent = incoming.parent;
    }
    if existing.timestamp.is_none() {
        existing.timestamp = incoming.timestamp;
    }
    if existing.subject.is_empty() {
        existing.subject = incoming.subject;
    }
    if existing.paths.is_empty() {
        existing.paths = incoming.paths;
    }
    if existing.class.is_none() {
        existing.class = incoming.class;
    }
    if label::confidence(incoming.source) > existing.confidence {
        existing.source = incoming.source;
        existing.confidence = incoming.confidence;
    }
    if incoming.features.score > 0.0 || incoming.features.structural > 0.0 {
        existing.features = incoming.features;
    }
    existing.api_touches = existing.api_touches.max(incoming.api_touches);
    existing.api_signatures = existing.api_signatures.max(incoming.api_signatures);
    existing.dependency_nodes = existing.dependency_nodes.max(incoming.dependency_nodes);
    existing.runtime_paths = existing.runtime_paths.max(incoming.runtime_paths);
}

fn apply_human_labels(repo: &Path, candidates: &mut [Candidate]) {
    let path = repo.join(".kaptaind").join("human-semver.jsonl");
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let mut by_sha: BTreeMap<String, SemverClass> = BTreeMap::new();
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(sha) = value.get("commit").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(class) = value
            .get("class")
            .and_then(|v| v.as_str())
            .and_then(SemverClass::parse_label)
        else {
            continue;
        };
        by_sha.insert(sha.to_string(), class);
    }
    for candidate in candidates.iter_mut() {
        if let Some(sha) = &candidate.sha {
            if let Some(class) = by_sha.get(sha) {
                candidate.class = Some(*class);
                candidate.source = LabelSource::HumanReviewedRelease;
                candidate.confidence = label::confidence(candidate.source);
            }
        }
    }
}

fn apply_release_tags(repo: &Path, candidates: &mut [Candidate]) {
    let Ok(tags) = gitio::tag_commits(repo) else {
        return;
    };
    let tagged: BTreeSet<String> = tags
        .into_iter()
        .filter(|(name, _)| is_release_tag(name))
        .map(|(_, sha)| sha)
        .collect();
    for candidate in candidates.iter_mut() {
        let Some(sha) = &candidate.sha else {
            continue;
        };
        if !tagged.contains(sha) {
            continue;
        }
        if matches!(
            candidate.source,
            LabelSource::MatureKaptaindDecision | LabelSource::EarlierKaptaindGeneration
        ) {
            candidate.source = LabelSource::ReleaseTagWithDeterministic;
            candidate.confidence = label::confidence(candidate.source);
        }
    }
}

fn is_release_tag(tag: &str) -> bool {
    let bare = tag.strip_prefix('v').unwrap_or(tag);
    semver::Version::parse(bare).is_ok()
        || tag
            .rfind("-v")
            .is_some_and(|index| semver::Version::parse(&tag[index + 2..]).is_ok())
}

fn observe_candidate(
    repo: &Path,
    identity: &RepositoryIdentity,
    permission: TrainingPermission,
    candidate: &Candidate,
    registry: &AdapterRegistry,
) -> Vec<KaptaindObservation> {
    let paths = if !candidate.paths.is_empty() {
        candidate.paths.clone()
    } else if let Some(sha) = &candidate.sha {
        gitio::changed_paths(repo, sha).unwrap_or_default()
    } else {
        Vec::new()
    };
    if paths.is_empty() {
        return Vec::new();
    }
    let parent_state = StateFingerprint {
        commit_sha: candidate.parent.clone().unwrap_or_default(),
        tree_sha: candidate
            .parent
            .as_deref()
            .and_then(|parent| gitio::tree_sha(repo, parent)),
    };
    let mut built = Vec::new();
    for path in paths {
        built.extend(observe_path(
            repo,
            identity,
            permission,
            candidate,
            &parent_state,
            &path,
            registry,
        ));
    }
    built
}

fn observe_path(
    repo: &Path,
    identity: &RepositoryIdentity,
    mut permission: TrainingPermission,
    candidate: &Candidate,
    parent_state: &StateFingerprint,
    path: &str,
    registry: &AdapterRegistry,
) -> Vec<KaptaindObservation> {
    let secret = represent::is_secret_path(path);
    let generated = represent::is_generated_path(path);
    if secret {
        permission = TrainingPermission::Prohibited;
    }
    let raw_diff = candidate
        .sha
        .as_deref()
        .and_then(|sha| gitio::diff_path(repo, sha, path).ok())
        .unwrap_or_default();
    let raw_diff = cap(&raw_diff, DIFF_CAP);
    let (textual, redacted) = if secret {
        ("[redacted secret path]".to_string(), true)
    } else {
        represent::redact_secrets(&raw_diff)
    };
    let (structural, edits) = if secret {
        (Vec::new(), Vec::new())
    } else {
        represent::structural_from_diff(path, &textual)
    };
    let ast = if secret || generated {
        AstDiff::default()
    } else {
        ast_diff(repo, candidate, path, registry)
    };
    let edit_names: BTreeSet<String> = edits.iter().map(|edit| edit.name.clone()).collect();

    let mut pieces: Vec<Piece> = edits
        .iter()
        .map(|edit| Piece {
            symbol: Some(edit.name.clone()),
            kind: edit.kind.clone(),
            change: edit.change.clone(),
            edit: Some(edit),
            structural: structural.clone(),
        })
        .collect();
    for symbol in ast
        .added
        .iter()
        .map(|symbol| (symbol, "added"))
        .chain(ast.removed.iter().map(|symbol| (symbol, "removed")))
        .chain(ast.modified.iter().map(|symbol| (symbol, "modified")))
    {
        if edit_names.contains(&symbol.0.name) {
            continue;
        }
        pieces.push(Piece {
            symbol: Some(symbol.0.name.clone()),
            kind: symbol.0.kind.clone(),
            change: symbol.1.to_string(),
            edit: None,
            structural: structural.clone(),
        });
    }
    if pieces.is_empty() {
        pieces.push(Piece {
            symbol: None,
            kind: "file".to_string(),
            change: "modified".to_string(),
            edit: None,
            structural,
        });
    }

    let file = FileContext {
        identity,
        permission,
        candidate,
        parent_state,
        path,
        textual: &textual,
        redacted,
        generated,
        secret,
    };
    pieces
        .into_iter()
        .map(|piece| build_observation(&file, piece))
        .collect()
}

struct Piece<'a> {
    symbol: Option<String>,
    kind: String,
    change: String,
    edit: Option<&'a SignatureEdit>,
    structural: Vec<StructuralFact>,
}

/// Shared inputs for every symbol KOU cut from one changed file.
#[derive(Clone, Copy)]
struct FileContext<'a> {
    identity: &'a RepositoryIdentity,
    permission: TrainingPermission,
    candidate: &'a Candidate,
    parent_state: &'a StateFingerprint,
    path: &'a str,
    textual: &'a str,
    redacted: bool,
    generated: bool,
    secret: bool,
}

fn build_observation(file: &FileContext<'_>, piece: Piece<'_>) -> KaptaindObservation {
    let FileContext {
        identity,
        permission,
        candidate,
        parent_state,
        path,
        textual,
        redacted,
        generated,
        secret,
    } = *file;
    let mut features = candidate.features.clone();
    if let Some(edit) = piece.edit {
        features.required_parameter_added = edit.required_parameter_added;
    }
    let (predicted, reason) = if let Some(edit) = piece.edit {
        represent::predict_class(path, Some(edit), &features)
    } else if piece.symbol.is_some() {
        let class = match piece.change.as_str() {
            "removed" => SemverClass::Major,
            "added" => SemverClass::Minor,
            _ => SemverClass::Ambiguous,
        };
        let reason = match class {
            SemverClass::Major => "PUBLIC_SYMBOL_REMOVED",
            SemverClass::Minor => "PUBLIC_SYMBOL_ADDED",
            _ => "PUBLIC_SYMBOL_MODIFIED",
        };
        (class, reason.to_string())
    } else {
        represent::predict_class(path, None, &features)
    };
    let semantic = vec![represent::semantic_fact(path, predicted, &reason)];
    let structural = if piece.symbol.is_some() {
        piece
            .structural
            .into_iter()
            .filter(|fact| fact.attributes.get("name") == piece.symbol.as_ref())
            .collect::<Vec<_>>()
    } else {
        piece.structural
    };
    let symbols = piece
        .symbol
        .as_ref()
        .map(|name| SymbolChange {
            path: path.to_string(),
            name: name.clone(),
            kind: piece.kind.clone(),
            visibility: "public".to_string(),
            change: piece.change.clone(),
        })
        .into_iter()
        .collect::<Vec<_>>();
    let scope = if piece.symbol.is_some() {
        LabelScope::Symbol
    } else {
        LabelScope::File
    };
    let sha = candidate.sha.clone().unwrap_or_default();
    let observation_id = represent::observation_id(
        &identity.repository_id,
        if sha.is_empty() {
            candidate.cluster_id.as_deref().unwrap_or("unresolved")
        } else {
            &sha
        },
        path,
        piece.symbol.as_deref().unwrap_or("-"),
    );
    KaptaindObservation {
        schema_version: KOU_SCHEMA_VERSION,
        observation_id,
        repository: identity.clone(),
        commit: CommitIdentity {
            sha: sha.clone(),
            parent_sha: candidate.parent.clone(),
            timestamp: candidate.timestamp.clone(),
        },
        language: represent::language_of_path(path),
        parent_state: parent_state.clone(),
        change: ChangeRepresentation {
            textual: textual.to_string(),
            structural,
            semantic,
        },
        symbols,
        api: ApiImpact {
            breaking: predicted == SemverClass::Major,
            added: predicted == SemverClass::Minor,
            touches: candidate.api_touches,
            signatures: candidate.api_signatures,
        },
        dependencies: DependencyImpact {
            manifests: u32::from(represent::is_dependency_path(path)),
            nodes: candidate.dependency_nodes,
            edges: 0,
        },
        runtime: RuntimeImpact {
            paths: candidate.runtime_paths,
            changed: represent::is_runtime_path(path),
        },
        schema: SchemaImpact {
            changed: path.ends_with(".graphql")
                || path.ends_with(".proto")
                || path.ends_with(".json") && path.contains("schema"),
            kind: None,
        },
        deterministic_features: features,
        predicted_semver: predicted,
        authoritative_semver: candidate.class,
        confidence: candidate.confidence,
        provenance: Provenance {
            organisation: identity.organisation.clone(),
            repository: identity.repository_name.clone(),
            commit_sha: sha,
            parent_sha: candidate.parent.clone(),
            file: Some(path.to_string()),
            symbol: piece.symbol.clone(),
            kaptaind_version: None,
            extractor_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: KOU_SCHEMA_VERSION,
            label_source: candidate.source,
            label_confidence: candidate.confidence,
            label_confidence_raw: candidate.confidence,
            timestamp: candidate.timestamp.clone(),
        },
        future_evidence: None,
        training_permission: permission,
        label_scope: scope,
        label_disputed: false,
        generated,
        secret_redacted: redacted || secret,
        training_eligible: false,
        observations_in_commit: 1,
        fingerprints: Fingerprints::default(),
        cluster_id: None,
        partitions: None,
    }
}

fn stamp_commit_groups(observations: &mut [KaptaindObservation]) {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for obs in observations.iter() {
        let key = commit_key(obs);
        *counts.entry(key).or_insert(0) += 1;
    }
    for obs in observations.iter_mut() {
        let key = commit_key(obs);
        obs.observations_in_commit = counts.get(&key).copied().unwrap_or(1);
        obs.fingerprints = represent::fingerprint_set(
            &obs.commit.sha,
            &obs.change.textual,
            &obs.change.structural,
            obs.symbols
                .first()
                .map(|symbol| format!("{}:{}:{}", symbol.path, symbol.name, symbol.change))
                .unwrap_or_default()
                .as_str(),
            &obs.change.semantic,
        );
        obs.training_eligible = eligibility(obs);
    }
}

fn commit_key(obs: &KaptaindObservation) -> String {
    if !obs.commit.sha.is_empty() {
        obs.commit.sha.clone()
    } else {
        format!("cluster:{}", obs.observation_id)
    }
}

pub fn eligibility(obs: &KaptaindObservation) -> bool {
    let aligned = match (obs.predicted_semver, obs.authoritative_semver) {
        (_, None) => false,
        (_, Some(SemverClass::Ambiguous)) => false,
        (SemverClass::Ambiguous, Some(_)) => obs.observations_in_commit == 1,
        (predicted, Some(authoritative)) => predicted == authoritative,
    };
    obs.training_permission.allows_training()
        && !obs.generated
        && !obs.secret_redacted
        && !obs.label_disputed
        && obs.confidence >= 0.9
        && obs.provenance_complete()
        && aligned
}

fn attach_future_evidence(repo: &Path, horizon: u32, observations: &mut [KaptaindObservation]) {
    let mut cache: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut looked_up = 0usize;
    for obs in observations.iter_mut() {
        if obs.commit.sha.is_empty() {
            continue;
        }
        let messages = cache.entry(obs.commit.sha.clone()).or_insert_with(|| {
            looked_up += 1;
            if looked_up.is_multiple_of(500) {
                eprintln!("kaptaind data extract: audited {looked_up} commits");
            }
            gitio::descendant_messages(repo, &obs.commit.sha, horizon)
        });
        let evidence = judge_future(obs, messages);
        if evidence.contradicts_label {
            obs.label_disputed = true;
            obs.confidence = (obs.provenance.label_confidence_raw * 0.5).clamp(0.0, 1.0);
            obs.provenance.label_confidence = obs.confidence;
            obs.training_eligible = false;
        }
        obs.future_evidence = Some(evidence);
    }
}

fn judge_future(obs: &KaptaindObservation, messages: &[String]) -> FutureEvidence {
    let api_related = obs
        .change
        .semantic
        .iter()
        .any(|fact| fact.kind == "API_CHANGE" || fact.kind == "DEPENDENCY_CHANGE")
        || obs.predicted_semver == SemverClass::Major
        || obs.predicted_semver == SemverClass::Minor;
    let mut downstream_fix = false;
    let mut revert = false;
    let mut followup_breakage = false;
    let mut release_changed = false;
    for message in messages {
        let lower = message.to_ascii_lowercase();
        let subject = message.lines().next().unwrap_or("").to_ascii_lowercase();
        if subject.starts_with("revert ") || subject.starts_with("revert:") {
            let short = &obs.commit.sha[..obs.commit.sha.len().min(12)];
            if message.contains(short) || message.contains(&obs.commit.sha) {
                revert = true;
            }
        }
        if api_related {
            if lower.contains("fix")
                && (lower.contains("break")
                    || lower.contains("downstream")
                    || lower.contains("compat"))
            {
                downstream_fix = true;
            }
            if lower.contains("breakage")
                || lower.contains("incompatible")
                || lower.contains("breaking change")
            {
                followup_breakage = true;
            }
            if let Some(card) = label::parse_scorecard(message) {
                if let (Some(later), Some(current)) = (
                    card.class.rank(),
                    obs.authoritative_semver.and_then(SemverClass::rank),
                ) {
                    if later > current {
                        release_changed = true;
                    }
                }
            }
        }
    }
    let label_under = obs
        .authoritative_semver
        .and_then(SemverClass::rank)
        .is_some_and(|rank| rank < SemverClass::Major.rank().unwrap_or(3));
    let contradicts_label =
        label_under && (downstream_fix || followup_breakage || release_changed || revert);
    FutureEvidence {
        downstream_fix,
        revert,
        followup_breakage,
        release_changed,
        consumer_failure: None,
        human_override: None,
        horizon_commits: messages.len() as u32,
        contradicts_label,
    }
}

fn ast_diff(repo: &Path, candidate: &Candidate, path: &str, registry: &AdapterRegistry) -> AstDiff {
    let Some(sha) = &candidate.sha else {
        return AstDiff::default();
    };
    let after = gitio::show_path(repo, &format!("{sha}:{path}")).unwrap_or_default();
    if after.len() > FILE_CAP {
        return AstDiff::default();
    }
    let before = candidate
        .parent
        .as_deref()
        .and_then(|parent| gitio::show_path(repo, &format!("{parent}:{path}")))
        .unwrap_or_default();
    if before.len() > FILE_CAP {
        return AstDiff::default();
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("txt");
    let dir = std::env::temp_dir().join(format!("kaptaind-kou-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let token = represent::sha256_hex(format!("{sha}:{path}").as_bytes());
    let before_path = dir.join(format!("{token}-before.{ext}"));
    let after_path = dir.join(format!("{token}-after.{ext}"));
    if std::fs::write(&before_path, &before).is_err()
        || std::fs::write(&after_path, &after).is_err()
    {
        return AstDiff::default();
    }
    let diff = registry.resolve(Path::new(path)).map(|adapter| {
        let old = adapter.parse_ast(&before_path).ok();
        let new = adapter.parse_ast(&after_path).ok();
        match (old, new) {
            (Some(old), Some(new)) => adapter.diff_ast(&old, &new),
            (None, Some(new)) => AstDiff {
                added: new.symbols,
                ..AstDiff::default()
            },
            (Some(old), None) => AstDiff {
                removed: old.symbols,
                ..AstDiff::default()
            },
            (None, None) => AstDiff::default(),
        }
    });
    let _ = std::fs::remove_file(&before_path);
    let _ = std::fs::remove_file(&after_path);
    diff.unwrap_or_default()
}

fn cap(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

fn read_decisions(path: &Path) -> Vec<DecisionRecord> {
    let file = path.join(".kaptaind").join("decisions.jsonl");
    let Ok(content) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn read_artifacts(path: &Path) -> Vec<LooseArtifact> {
    let dir = path.join(".kaptaind").join("analysis");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            serde_json::from_str(&text).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "KOU Test")
            .env("GIT_AUTHOR_EMAIL", "kou@example.com")
            .env("GIT_COMMITTER_NAME", "KOU Test")
            .env("GIT_COMMITTER_EMAIL", "kou@example.com")
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn extraction_splits_a_commit_and_disputes_an_under_call() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo).unwrap();
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "kou@example.com"]);
        git(repo, &["config", "user.name", "KOU Test"]);
        git(repo, &["config", "commit.gpgsign", "false"]);

        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(
            repo.join("src/lib.rs"),
            "pub fn connect(host: &str) -> String { host.to_string() }\n",
        )
        .unwrap();
        git(repo, &["add", "src/lib.rs"]);
        git(repo, &["commit", "-m", "initial import"]);

        std::fs::write(repo.join("README.md"), "notes\n").unwrap();
        git(repo, &["add", "README.md"]);
        git(
            repo,
            &[
                "commit",
                "-m",
                "docs: notes\n\nkaptaind: no-bump [api-stable; paths=1; api_touches=0; deps=0; runtime=0; score=0.010; cluster=docs-1]\n",
            ],
        );

        std::fs::write(
            repo.join("src/lib.rs"),
            "pub fn connect(host: &str, timeout: u64) -> String { host.to_string() }\n",
        )
        .unwrap();
        git(repo, &["add", "src/lib.rs"]);
        git(
            repo,
            &[
                "commit",
                "-m",
                "fix: connect\n\nkaptaind: Patch -> v0.1.1 [api-stable; paths=1; api_touches=1; deps=0; runtime=0; score=0.200; cluster=api-1]\n",
            ],
        );

        std::fs::write(repo.join("README.md"), "notes\n\nfixed the break\n").unwrap();
        git(repo, &["add", "README.md"]);
        git(repo, &["commit", "-m", "fix downstream API break"]);

        let observations = extract(&ExtractOptions {
            repo: repo.to_path_buf(),
            organisation: Some("elci-group".to_string()),
            include_weak: false,
            horizon: 5,
            limit: None,
            first_party_orgs: vec!["elci-group".to_string()],
        })
        .unwrap();

        let docs = observations
            .iter()
            .find(|obs| obs.provenance.file.as_deref() == Some("README.md"))
            .expect("docs observation");
        assert_eq!(docs.authoritative_semver, Some(SemverClass::NoBump));
        assert_eq!(docs.predicted_semver, SemverClass::NoBump);
        assert!(docs.training_eligible);
        assert!(!docs.label_disputed);
        assert_eq!(docs.training_permission, TrainingPermission::FirstParty);
        assert!(docs.provenance_complete());

        let api = observations
            .iter()
            .find(|obs| obs.symbols.iter().any(|symbol| symbol.name == "connect"))
            .expect("connect observation");
        assert_eq!(api.predicted_semver, SemverClass::Major);
        assert_eq!(api.authoritative_semver, Some(SemverClass::Patch));
        assert!(
            !api.training_eligible,
            "disagreement must not become ground truth"
        );
        assert!(api.label_disputed);
        assert!(api
            .future_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.downstream_fix && evidence.contradicts_label));
        assert!(api
            .change
            .structural
            .iter()
            .any(|fact| fact.kind == "FUNCTION_CHANGE"));
        assert!(api
            .change
            .semantic
            .iter()
            .any(|fact| fact.kind == "API_CHANGE"));
        assert_ne!(api.observation_id, docs.observation_id);
    }
}
