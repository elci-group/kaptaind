//! Versioned Kaptaind Observation Unit (KOU), schema 1.
//!
//! A commit is not a training example. One commit may yield many observations,
//! each tracing a single symbol or file back to its repository, decision, and
//! whatever happened afterwards. Ambiguous history stays `Ambiguous`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Increment only when a field changes meaning. Adding a serde-default field
/// does not require a bump; renaming or redefining one does.
pub const KOU_SCHEMA_VERSION: u16 = 1;

/// Census token estimate used before real before/after text has been measured.
/// Extraction replaces this with a character-based count.
pub const CENSUS_TOKENS_PER_CANDIDATE: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemverClass {
    NoBump,
    Patch,
    Minor,
    Major,
    Ambiguous,
}

impl SemverClass {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::NoBump => "NO_BUMP",
            Self::Patch => "PATCH",
            Self::Minor => "MINOR",
            Self::Major => "MAJOR",
            Self::Ambiguous => "AMBIGUOUS",
        }
    }

    /// Ordered compatibility impact. `Ambiguous` has no rank.
    pub fn rank(self) -> Option<i32> {
        match self {
            Self::NoBump => Some(0),
            Self::Patch => Some(1),
            Self::Minor => Some(2),
            Self::Major => Some(3),
            Self::Ambiguous => None,
        }
    }

    pub fn parse_label(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "nobump" | "no_bump" | "no-bump" | "none" | "bump::none" => Some(Self::NoBump),
            "patch" => Some(Self::Patch),
            "minor" => Some(Self::Minor),
            "major" => Some(Self::Major),
            "ambiguous" => Some(Self::Ambiguous),
            _ => None,
        }
    }
}

/// Who may learn from a repository. Decided at ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingPermission {
    FirstParty,
    ExplicitlyLicensed,
    ResearchOnly,
    EvaluationOnly,
    Prohibited,
}

impl TrainingPermission {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::FirstParty => "first_party",
            Self::ExplicitlyLicensed => "explicitly_licensed",
            Self::ResearchOnly => "research_only",
            Self::EvaluationOnly => "evaluation_only",
            Self::Prohibited => "prohibited",
        }
    }

    pub fn parse_label(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "first_party" | "first-party" | "firstparty" => Some(Self::FirstParty),
            "explicitly_licensed" | "explicitly-licensed" => Some(Self::ExplicitlyLicensed),
            "research_only" | "research-only" => Some(Self::ResearchOnly),
            "evaluation_only" | "evaluation-only" => Some(Self::EvaluationOnly),
            "prohibited" => Some(Self::Prohibited),
            _ => None,
        }
    }

    pub fn allows_training(self) -> bool {
        matches!(self, Self::FirstParty | Self::ExplicitlyLicensed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelSource {
    HumanReviewedRelease,
    ReleaseTagWithDeterministic,
    MatureKaptaindDecision,
    EarlierKaptaindGeneration,
    CommitMessageInference,
    ConventionalCommitInference,
    HeuristicReconstruction,
    Unknown,
}

impl LabelSource {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::HumanReviewedRelease => "human_reviewed_release",
            Self::ReleaseTagWithDeterministic => "release_tag_with_deterministic",
            Self::MatureKaptaindDecision => "mature_kaptaind_decision",
            Self::EarlierKaptaindGeneration => "earlier_kaptaind_generation",
            Self::CommitMessageInference => "commit_message_inference",
            Self::ConventionalCommitInference => "conventional_commit_inference",
            Self::HeuristicReconstruction => "heuristic_reconstruction",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compatibility {
    Breaking,
    Additive,
    Compatible,
    Internal,
    Unknown,
}

impl Compatibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::Additive => "additive",
            Self::Compatible => "compatible",
            Self::Internal => "internal",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Train,
    Test,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelScope {
    Commit,
    File,
    Symbol,
}

/// Open language tag. Stored as a lowercase string so a new extension does not
/// require a schema migration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Language(pub String);

impl Language {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into().to_ascii_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassCounts {
    pub no_bump: u64,
    pub patch: u64,
    pub minor: u64,
    pub major: u64,
    pub ambiguous: u64,
}

impl ClassCounts {
    pub fn add(&mut self, class: SemverClass) {
        match class {
            SemverClass::NoBump => self.no_bump += 1,
            SemverClass::Patch => self.patch += 1,
            SemverClass::Minor => self.minor += 1,
            SemverClass::Major => self.major += 1,
            SemverClass::Ambiguous => self.ambiguous += 1,
        }
    }

    pub fn total(&self) -> u64 {
        self.no_bump + self.patch + self.minor + self.major + self.ambiguous
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageShare {
    pub language: String,
    pub bytes: u64,
    pub share: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryIdentity {
    pub organisation: String,
    pub repository_id: String,
    pub repository_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitIdentity {
    pub sha: String,
    pub parent_sha: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateFingerprint {
    pub commit_sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuralFact {
    pub kind: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticFact {
    pub kind: String,
    pub consumer_compatibility: Compatibility,
    pub source_compatibility: Compatibility,
    pub behavioural_compatibility: Compatibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeRepresentation {
    pub textual: String,
    pub structural: Vec<StructuralFact>,
    pub semantic: Vec<SemanticFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolChange {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub visibility: String,
    pub change: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiImpact {
    pub breaking: bool,
    pub added: bool,
    pub touches: u32,
    pub signatures: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyImpact {
    pub manifests: u32,
    pub nodes: u32,
    pub edges: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeImpact {
    pub paths: u32,
    pub changed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaImpact {
    pub changed: bool,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DeterministicFeatures {
    pub structural: f32,
    pub api: f32,
    pub deps: f32,
    pub runtime: f32,
    pub bundle: f32,
    pub score: f32,
    pub visibility_change: bool,
    pub required_parameter_added: bool,
    pub return_type_change: bool,
    pub runtime_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub organisation: String,
    pub repository: String,
    pub commit_sha: String,
    pub parent_sha: Option<String>,
    pub file: Option<String>,
    pub symbol: Option<String>,
    /// Daemon version, when the historical record actually carried one.
    pub kaptaind_version: Option<String>,
    pub extractor_version: String,
    pub schema_version: u16,
    pub label_source: LabelSource,
    pub label_confidence: f32,
    /// Confidence before a future-evidence dispute discounted it.
    pub label_confidence_raw: f32,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FutureEvidence {
    pub downstream_fix: bool,
    pub revert: bool,
    pub followup_breakage: bool,
    pub release_changed: bool,
    pub consumer_failure: Option<bool>,
    pub human_override: Option<SemverClass>,
    pub horizon_commits: u32,
    /// The look-ahead suggests the stored label understated compatibility impact.
    pub contradicts_label: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fingerprints {
    /// Empty when the hashed input was empty, so unrelated observations do not
    /// collapse into one cluster.
    pub commit: String,
    pub diff: String,
    pub ast: String,
    pub symbol: String,
    pub semantic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Partitions {
    pub temporal: Split,
    pub repository: Split,
    pub pattern: Split,
    pub language: Split,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KaptaindObservation {
    pub schema_version: u16,
    pub observation_id: String,
    pub repository: RepositoryIdentity,
    pub commit: CommitIdentity,
    pub language: Language,
    pub parent_state: StateFingerprint,
    pub change: ChangeRepresentation,
    pub symbols: Vec<SymbolChange>,
    pub api: ApiImpact,
    pub dependencies: DependencyImpact,
    pub runtime: RuntimeImpact,
    pub schema: SchemaImpact,
    pub deterministic_features: DeterministicFeatures,
    pub predicted_semver: SemverClass,
    pub authoritative_semver: Option<SemverClass>,
    pub confidence: f32,
    pub provenance: Provenance,
    pub future_evidence: Option<FutureEvidence>,
    pub training_permission: TrainingPermission,
    pub label_scope: LabelScope,
    pub label_disputed: bool,
    pub generated: bool,
    pub secret_redacted: bool,
    /// Eligible to teach a later model. Commit-level labels are not copied
    /// onto disagreeing sibling observations.
    pub training_eligible: bool,
    pub observations_in_commit: u32,
    pub fingerprints: Fingerprints,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partitions: Option<Partitions>,
}

impl KaptaindObservation {
    pub fn provenance_complete(&self) -> bool {
        let sha_ok = is_git_sha(&self.provenance.commit_sha);
        let parent_ok = self
            .provenance
            .parent_sha
            .as_deref()
            .is_some_and(is_git_sha);
        let file_ok = self
            .provenance
            .file
            .as_deref()
            .is_some_and(|file| !file.is_empty());
        let symbol_ok = self.label_scope != LabelScope::Symbol
            || self
                .provenance
                .symbol
                .as_deref()
                .is_some_and(|symbol| !symbol.is_empty());
        sha_ok
            && parent_ok
            && file_ok
            && symbol_ok
            && !self.provenance.organisation.is_empty()
            && !self.provenance.repository.is_empty()
            && !self.provenance.extractor_version.is_empty()
            && self.provenance.schema_version == KOU_SCHEMA_VERSION
            && self.provenance.label_source != LabelSource::Unknown
            && self
                .provenance
                .timestamp
                .as_deref()
                .is_some_and(|t| !t.is_empty())
            && (0.0..=1.0).contains(&self.provenance.label_confidence)
    }
}

pub fn is_git_sha(value: &str) -> bool {
    let len = value.len();
    (7..=64).contains(&len) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepositoryCensus {
    pub repository_id: String,
    pub repository_name: String,
    pub organisation: String,
    pub availability: String,
    pub local_path: Option<String>,
    pub remote_url: Option<String>,
    pub training_permission: TrainingPermission,
    pub first_commit: String,
    pub first_kaptaind_commit: Option<String>,
    pub latest_commit: String,
    pub total_commits: u64,
    /// Commits at or after Kaptaind became operational in this repository.
    pub kaptaind_era_commits: u64,
    /// Commits that carry an explicit Kaptaind scorecard or `Kaptaind-` trailer.
    pub explicitly_labelled_commits: u64,
    /// Changes with a recoverable analysis. Not the same count as labelled commits.
    pub kaptaind_observed: u64,
    pub languages: Vec<LanguageShare>,
    pub merges: u64,
    pub reverts: u64,
    pub releases: u64,
    pub tags: u64,
    pub estimated_observations: u64,
    pub estimated_training_tokens: u64,
    /// One class per labelled commit, taken from the scorecard. Not a KOU count.
    pub labelled_classes: ClassCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InaccessibleRepository {
    pub name: String,
    pub path: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CensusTotals {
    pub repositories: u64,
    pub cloned: u64,
    pub remote_only: u64,
    pub inaccessible: u64,
    pub kaptaind_era_repositories: u64,
    pub commits: u64,
    pub kaptaind_era_commits: u64,
    pub explicitly_labelled_commits: u64,
    pub kaptaind_observed: u64,
    pub estimated_observations: u64,
    pub estimated_training_tokens: u64,
    pub merges: u64,
    pub reverts: u64,
    pub releases: u64,
    pub tags: u64,
    pub labelled_classes: ClassCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EstateCensus {
    pub organisation: Option<String>,
    pub repositories: Vec<RepositoryCensus>,
    pub inaccessible: Vec<InaccessibleRepository>,
    pub totals: CensusTotals,
    /// Populated only after validation. Census itself does not invent a KDS.
    pub model_readiness: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_labels_round_trip_and_ambiguous_has_no_rank() {
        for class in [
            SemverClass::NoBump,
            SemverClass::Patch,
            SemverClass::Minor,
            SemverClass::Major,
            SemverClass::Ambiguous,
        ] {
            assert_eq!(SemverClass::parse_label(class.as_label()), Some(class));
        }
        assert_eq!(SemverClass::parse_label("None"), Some(SemverClass::NoBump));
        assert_eq!(
            SemverClass::parse_label("no-bump"),
            Some(SemverClass::NoBump)
        );
        assert_eq!(SemverClass::Ambiguous.rank(), None);
        assert_eq!(SemverClass::Major.rank(), Some(3));
    }

    #[test]
    fn training_permission_blocks_external_by_default() {
        assert!(TrainingPermission::FirstParty.allows_training());
        assert!(TrainingPermission::ExplicitlyLicensed.allows_training());
        assert!(!TrainingPermission::EvaluationOnly.allows_training());
        assert!(!TrainingPermission::Prohibited.allows_training());
        assert_eq!(
            TrainingPermission::parse_label("evaluation-only"),
            Some(TrainingPermission::EvaluationOnly)
        );
    }
}
