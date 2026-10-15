//! Validate observations, score the corpus, and publish an immutable manifest.
//!
//! A corpus name is written once. A different checksum for the same name is
//! refused; publish the next version instead.

use super::dedupe::{self, PartitionPlan};
use super::kds::{self, KdsReport};
use super::schema::{ClassCounts, KaptaindObservation, LabelSource, KOU_SCHEMA_VERSION};
use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationReport {
    pub read: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub high_confidence: u64,
    pub disputed: u64,
    pub training_eligible: u64,
    pub reasons: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorpusManifest {
    pub name: String,
    pub schema_version: u16,
    pub created_at: String,
    pub extractor_version: String,
    pub observations: u64,
    pub deduplicated: u64,
    pub high_confidence: u64,
    pub training_eligible: u64,
    pub kds: KdsReport,
    pub readiness: ReadinessManifest,
    pub cutoff: String,
    pub class_counts: ClassCounts,
    pub checksum_sha256: String,
    pub pattern_holdout: Vec<String>,
    pub repository_holdout: Vec<String>,
    pub language_holdout: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadinessManifest {
    pub high_confidence: u64,
    pub gate: String,
    pub slm0_ready: bool,
}

pub fn read_jsonl(path: &Path) -> anyhow::Result<Vec<KaptaindObservation>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut observations = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let obs: KaptaindObservation = serde_json::from_str(line).with_context(|| {
            format!(
                "{}:{} is not a Kaptaind observation",
                path.display(),
                index + 1
            )
        })?;
        observations.push(obs);
    }
    Ok(observations)
}

pub fn write_jsonl(path: &Path, observations: &[KaptaindObservation]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = String::new();
    for obs in observations {
        body.push_str(&serde_json::to_string(obs)?);
        body.push('\n');
    }
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))
}

pub fn validate(
    observations: &[KaptaindObservation],
) -> (Vec<KaptaindObservation>, ValidationReport) {
    let mut accepted = Vec::new();
    let mut reasons: Vec<(String, u64)> = Vec::new();
    let mut bump = |reason: &str| {
        if let Some(slot) = reasons.iter_mut().find(|(name, _)| name == reason) {
            slot.1 += 1;
        } else {
            reasons.push((reason.to_string(), 1));
        }
    };
    for obs in observations {
        if obs.schema_version != KOU_SCHEMA_VERSION {
            bump("schema_version");
            continue;
        }
        if obs.observation_id.is_empty() {
            bump("missing_id");
            continue;
        }
        if obs.provenance.label_source == LabelSource::Unknown
            || !super::label::admitted(obs.provenance.label_source)
        {
            bump("unknown_label");
            continue;
        }
        if !(0.0..=1.0).contains(&obs.confidence) {
            bump("confidence_range");
            continue;
        }
        if obs.authoritative_semver.is_none() {
            bump("missing_authoritative_label");
            continue;
        }
        accepted.push(obs.clone());
    }
    let high_confidence = accepted
        .iter()
        .filter(|obs| obs.training_eligible && obs.confidence >= 0.9)
        .count() as u64;
    let report = ValidationReport {
        read: observations.len() as u64,
        accepted: accepted.len() as u64,
        rejected: observations.len() as u64 - accepted.len() as u64,
        high_confidence,
        disputed: accepted.iter().filter(|obs| obs.label_disputed).count() as u64,
        training_eligible: accepted.iter().filter(|obs| obs.training_eligible).count() as u64,
        reasons,
    };
    (accepted, report)
}

pub fn dedupe(
    observations: &mut [KaptaindObservation],
    plan: &PartitionPlan,
) -> (Vec<KaptaindObservation>, dedupe::DedupeReport, KdsReport) {
    let report = dedupe::cluster_and_partition(observations, plan);
    let reps = dedupe::representatives(observations);
    let kds = kds::score(&reps);
    (reps, report, kds)
}

pub fn class_counts(observations: &[KaptaindObservation]) -> ClassCounts {
    let mut counts = ClassCounts::default();
    for obs in observations {
        if let Some(class) = obs.authoritative_semver {
            counts.add(class);
        }
    }
    counts
}

pub fn build_manifest(
    corpus_dir: &Path,
    version: &str,
    cutoff: &str,
) -> anyhow::Result<CorpusManifest> {
    let deduped_path = corpus_dir.join("deduped.jsonl");
    let observations = read_jsonl(&deduped_path)?;
    let bytes = fs::read(&deduped_path)?;
    let (pattern_holdout, repository_holdout, language_holdout) = read_holdouts(corpus_dir);
    let high_confidence = observations
        .iter()
        .filter(|obs| obs.training_eligible && obs.confidence >= 0.9)
        .count() as u64;
    let readiness = kds::readiness(high_confidence);
    let manifest = CorpusManifest {
        name: format!("kaptaind-corpus-{version}"),
        schema_version: KOU_SCHEMA_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        extractor_version: env!("CARGO_PKG_VERSION").to_string(),
        observations: observations.len() as u64,
        deduplicated: observations.len() as u64,
        high_confidence,
        training_eligible: observations
            .iter()
            .filter(|obs| obs.training_eligible)
            .count() as u64,
        kds: kds::score(&observations),
        readiness: ReadinessManifest {
            high_confidence: readiness.high_confidence,
            gate: readiness.gate.to_string(),
            slm0_ready: readiness.slm0_ready,
        },
        cutoff: cutoff.to_string(),
        class_counts: class_counts(&observations),
        checksum_sha256: super::represent::sha256_hex(&bytes),
        pattern_holdout,
        repository_holdout,
        language_holdout,
    };
    let manifest_path = corpus_dir.join("manifest.json");
    if manifest_path.exists() {
        let existing: CorpusManifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
        if existing.name == manifest.name && existing.checksum_sha256 != manifest.checksum_sha256 {
            return Err(anyhow!(
                "{} already exists with a different checksum; publish a new --version",
                existing.name
            ));
        }
        if existing.name == manifest.name && existing.checksum_sha256 == manifest.checksum_sha256 {
            return Ok(existing);
        }
    }
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(manifest)
}

fn read_holdouts(corpus_dir: &Path) -> (Vec<String>, Vec<String>, Option<String>) {
    let Ok(text) = fs::read_to_string(corpus_dir.join("dedupe.json")) else {
        return (Vec::new(), Vec::new(), None);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (Vec::new(), Vec::new(), None);
    };
    let strings = |key: &str| {
        value
            .get(key)
            .and_then(|item| item.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let language = value
        .get("language_holdout")
        .and_then(|item| item.as_str().map(str::to_string));
    (
        strings("pattern_holdout"),
        strings("repository_holdout"),
        language,
    )
}

pub fn load_manifest(path: &Path) -> anyhow::Result<CorpusManifest> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).context("corpus manifest is not valid JSON")
}

pub fn export_corpus(corpus_dir: &Path, out: &Path) -> anyhow::Result<PathBuf> {
    let manifest = load_manifest(&corpus_dir.join("manifest.json"))?;
    fs::create_dir_all(out)?;
    fs::copy(corpus_dir.join("deduped.jsonl"), out.join("deduped.jsonl"))?;
    fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(out.to_path_buf())
}
