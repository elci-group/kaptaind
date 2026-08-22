//! The `.kaptaind` semantic-state document (format family `semantic-state`).
//!
//! The document is a compact, persistent representation of what kaptaind
//! should not have to rediscover on every run: semantic surfaces, invariants,
//! negative-space exceptions, versioning policy, baseline fingerprints, and a
//! memory budget. Serialization is canonical (deterministic field and entry
//! ordering) so documents can be digested and migrations tested at the byte
//! level.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::version::SchemaVersion;

/// Minimum kaptaind software version able to interpret a semantic-state
/// document. Kept independent of the format version on purpose.
pub const MINIMUM_KAPTAIND: &str = "0.8.0";

/// Canonicalization scheme identifier embedded in digests.
pub const CANONICALIZATION: &str = "kaptaind-c14n-v1";

/// Format identity header (`[kaptaind]` table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KaptaindHeader {
    pub format: SchemaVersion,
    pub schema: String,
    #[serde(default = "default_minimum_kaptaind")]
    pub minimum_kaptaind: String,
}

fn default_minimum_kaptaind() -> String {
    MINIMUM_KAPTAIND.to_string()
}

/// Which semantic facilities this document actually contains.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub semantic_surfaces: bool,
    #[serde(default)]
    pub invariants: bool,
    #[serde(default)]
    pub negative_space: bool,
    #[serde(default)]
    pub fingerprints: bool,
    #[serde(default)]
    pub decision_memory: bool,
    #[serde(default)]
    pub branch_lifecycle: bool,
}

impl Capabilities {
    /// Capabilities implied by a fully populated semantic-state document.
    pub fn full() -> Self {
        Self {
            semantic_surfaces: true,
            invariants: true,
            negative_space: true,
            fingerprints: true,
            decision_memory: true,
            branch_lifecycle: true,
        }
    }
}

/// Declarative names for Kaptaind's governed Git topology. Operational
/// candidate and release records live in `.kaptaind/lifecycle.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchTopology {
    pub desktop_development: String,
    pub desktop_production: String,
    pub mobile_development: String,
    pub mobile_production: String,
    pub integration: String,
    pub local_staging: String,
    pub server_staging: String,
    pub release_pattern: String,
}

impl Default for BranchTopology {
    fn default() -> Self {
        Self {
            desktop_development: "desktop/development".into(),
            desktop_production: "desktop/production".into(),
            mobile_development: "mobile/development".into(),
            mobile_production: "mobile/production".into(),
            integration: "integration".into(),
            local_staging: "local/staging".into(),
            server_staging: "server/staging".into(),
            release_pattern: "release/{version}".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsumerChannels {
    pub stable: String,
    pub bleeding: String,
}

impl Default for ConsumerChannels {
    fn default() -> Self {
        Self {
            stable: "production".into(),
            bleeding: "development".into(),
        }
    }
}

/// A declared semantic surface: a path pattern whose changes carry
/// contract-level meaning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surface {
    pub id: String,
    pub paths: Vec<String>,
    /// `public`, `internal`, `cli`, ...
    pub kind: String,
    /// Added in 2.1; `None` means "unknown", a valid semantic state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
}

/// Something that must remain true; violating it carries `severity`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invariant {
    pub id: String,
    pub scope: Vec<String>,
    pub rule: String,
    /// `breaking`, `minor`, `patch`.
    pub severity: String,
}

/// Known negative space: things that look important but are declared not to
/// matter — a persistent anti-reasoning cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exception {
    pub pattern: String,
    pub impact: String,
    #[serde(default)]
    pub reason: String,
}

/// SemVer decision algebra declared by the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersioningPolicy {
    #[serde(default = "default_rules")]
    pub rules: BTreeMap<String, String>,
    #[serde(default = "default_thresholds")]
    pub thresholds: BTreeMap<String, f64>,
}

fn default_rules() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("breaking_surface_change".into(), "major".into()),
        ("new_public_capability".into(), "minor".into()),
        ("internal_change".into(), "patch".into()),
        ("documentation_only".into(), "none".into()),
        ("dependency_patch".into(), "patch".into()),
        ("dependency_major".into(), "major".into()),
    ])
}

fn default_thresholds() -> BTreeMap<String, f64> {
    BTreeMap::from([
        ("major".into(), 0.80),
        ("minor".into(), 0.45),
        ("patch".into(), 0.10),
    ])
}

impl Default for VersioningPolicy {
    fn default() -> Self {
        Self {
            rules: default_rules(),
            thresholds: default_thresholds(),
        }
    }
}

/// Baseline commit/version plus per-dimension fingerprints for
/// delta-targeted analysis.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Baseline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fingerprint: BTreeMap<String, String>,
}

/// Explicit memory budget: what the semantic document retains versus what
/// must stay in disposable artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBudget {
    pub retain: Vec<String>,
    pub discard: Vec<String>,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self {
            retain: vec![
                "project_identity".into(),
                "semantic_surfaces".into(),
                "invariants".into(),
                "dependency_graph".into(),
                "release_decisions".into(),
                "negative_space".into(),
            ],
            discard: vec![
                "raw_ast".into(),
                "temporary_traces".into(),
                "duplicate_evidence".into(),
            ],
        }
    }
}

/// The whole semantic-state document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDocument {
    pub kaptaind: KaptaindHeader,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface: Vec<Surface>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariant: Vec<Invariant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exception: Vec<Exception>,
    #[serde(default)]
    pub versioning: VersioningPolicy,
    #[serde(default)]
    pub baseline: Baseline,
    #[serde(default)]
    pub branches: BranchTopology,
    #[serde(default)]
    pub channels: ConsumerChannels,
    #[serde(default)]
    pub memory: MemoryBudget,
}

impl SemanticDocument {
    /// A minimal valid document at the given format version.
    pub fn empty(format: SchemaVersion) -> Self {
        Self {
            kaptaind: KaptaindHeader {
                format,
                schema: "semantic-state".into(),
                minimum_kaptaind: MINIMUM_KAPTAIND.into(),
            },
            capabilities: Capabilities::full(),
            surface: Vec::new(),
            invariant: Vec::new(),
            exception: default_exceptions(),
            versioning: VersioningPolicy::default(),
            baseline: Baseline::default(),
            branches: BranchTopology::default(),
            channels: ConsumerChannels::default(),
            memory: MemoryBudget::default(),
        }
    }

    pub fn format(&self) -> SchemaVersion {
        self.kaptaind.format
    }

    /// Parse a document from TOML text.
    pub fn parse(text: &str) -> Result<Self> {
        let doc: SemanticDocument =
            toml::from_str(text).context("failed to parse .kaptaind semantic document")?;
        if doc.kaptaind.schema != "semantic-state" {
            bail!(
                "unsupported schema family `{}` (expected `semantic-state`)",
                doc.kaptaind.schema
            );
        }
        Ok(doc)
    }

    /// Canonical TOML form: stable field ordering plus entries sorted by
    /// identifier/pattern so equal documents always serialize identically.
    pub fn to_canonical_toml(&self) -> String {
        let mut canonical = self.clone();
        canonical.surface.sort_by(|a, b| a.id.cmp(&b.id));
        canonical.invariant.sort_by(|a, b| a.id.cmp(&b.id));
        canonical
            .exception
            .sort_by(|a, b| a.pattern.cmp(&b.pattern));
        // traci: allow -- every field is a string, enum, or Vec of those; toml
        // serialization of this struct cannot fail (no non-string map keys, no
        // floats). Round-trip tests below guard the invariant.
        toml::to_string_pretty(&canonical).expect("semantic document serializes")
    }

    /// Digest over the canonical form (`sha256:<hex>`), excluding any
    /// embedded integrity data so it can anchor a before/after comparison.
    pub fn digest(&self) -> String {
        let bytes = self.to_canonical_toml();
        let hash = Sha256::digest(bytes.as_bytes());
        format!("sha256:{hash:x}")
    }

    /// Canonicalization scheme this digest uses.
    pub fn canonicalization(&self) -> &'static str {
        CANONICALIZATION
    }

    /// Version-aware validation. Returns an error listing every violation.
    pub fn validate(&self) -> Result<()> {
        let mut errors = Vec::new();
        let format = self.kaptaind.format;

        let family_ok = self.kaptaind.schema == "semantic-state"
            || (self.kaptaind.schema == "legacy" && format == SchemaVersion::new(1, 0));
        if !family_ok {
            errors.push(format!(
                "schema family must be `semantic-state`, got `{}`",
                self.kaptaind.schema
            ));
        }
        for rule in ["major", "minor", "patch"] {
            if !self.versioning.thresholds.contains_key(rule) {
                errors.push(format!("versioning.thresholds missing `{rule}`"));
            }
        }
        for surface in &self.surface {
            if surface.paths.is_empty() {
                errors.push(format!("surface `{}` has no paths", surface.id));
            }
            if format >= SchemaVersion::new(2, 1) && surface.stability.is_none() {
                errors.push(format!(
                    "surface `{}` is missing `stability` (required by format 2.1)",
                    surface.id
                ));
            }
        }
        for invariant in &self.invariant {
            if invariant.scope.is_empty() {
                errors.push(format!("invariant `{}` has no scope", invariant.id));
            }
        }
        if format >= SchemaVersion::new(2, 2) {
            let expected = BranchTopology::default();
            if self.branches != expected {
                errors.push("format 2.2 requires the canonical Kaptaind branch topology".into());
            }
            if self.channels != ConsumerChannels::default() {
                errors
                    .push("format 2.2 requires stable=production and bleeding=development".into());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            bail!(
                "semantic document validation failed:\n{}",
                errors.join("\n")
            );
        }
    }
}

fn default_exceptions() -> Vec<Exception> {
    vec![
        Exception {
            pattern: "tests/**".into(),
            impact: "none".into(),
            reason: "test-only changes do not affect release semantics".into(),
        },
        Exception {
            pattern: "docs/**".into(),
            impact: "none".into(),
            reason: "documentation does not alter runtime behaviour".into(),
        },
        Exception {
            pattern: "benches/**".into(),
            impact: "none".into(),
            reason: "benchmark-only".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SemanticDocument {
        let mut doc = SemanticDocument::empty(SchemaVersion::new(2, 1));
        doc.surface = vec![Surface {
            id: "api".into(),
            paths: vec!["src/api/**".into()],
            kind: "public".into(),
            audience: Some(vec!["external".into()]),
            stability: Some("stable".into()),
            compatibility: Some("unspecified".into()),
        }];
        doc.invariant = vec![Invariant {
            id: "api-backwards-compatible".into(),
            scope: vec!["surface.api".into()],
            rule: "existing exported interfaces remain callable".into(),
            severity: "breaking".into(),
        }];
        doc.baseline.version = Some("0.8.4".into());
        doc
    }

    #[test]
    fn round_trips_through_canonical_toml() {
        let doc = sample();
        let text = doc.to_canonical_toml();
        let parsed = SemanticDocument::parse(&text).unwrap();
        assert_eq!(parsed.digest(), doc.digest());
        parsed.validate().unwrap();
    }

    #[test]
    fn canonical_form_is_order_insensitive() {
        let mut a = sample();
        let mut b = sample();
        b.surface.reverse();
        b.invariant.reverse();
        assert_eq!(a.to_canonical_toml(), b.to_canonical_toml());
        a.surface.extend(b.surface.clone());
        a.to_canonical_toml();
    }

    #[test]
    fn digest_is_stable() {
        let doc = sample();
        assert_eq!(doc.digest(), sample().digest());
        assert!(doc.digest().starts_with("sha256:"));
    }

    #[test]
    fn validation_requires_stability_at_2_1() {
        let mut doc = sample();
        doc.surface[0].stability = None;
        assert!(doc.validate().is_err());
        doc.kaptaind.format = SchemaVersion::new(2, 0);
        doc.validate().unwrap();
    }

    #[test]
    fn rejects_wrong_schema_family() {
        let text = r#"
[kaptaind]
format = "2.1"
schema = "cache"

[versioning]
"#;
        assert!(SemanticDocument::parse(text).is_err());
    }
}
