//! Deterministic stepwise migrator for `.kaptaind` semantic documents.
//!
//! Migrations form a graph of discrete, testable transformations. A document
//! is never migrated by an opaque jump to "current": it walks one step at a
//! time, each step validating its input and output. Upgrades must preserve
//! semantics; downgrades are only allowed through reversible steps and
//! require explicit loss acknowledgement.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::Path;

use super::document::SemanticDocument;
use super::registry;
use super::version::SchemaVersion;

const LEGACY: SchemaVersion = SchemaVersion::new(1, 0);

/// One discrete migration step.
#[derive(Clone)]
pub struct Migration {
    pub from: SchemaVersion,
    pub to: SchemaVersion,
    /// A deterministic reverse transform exists.
    pub reversible: bool,
    /// The forward transform drops information.
    pub lossy: bool,
    pub notes: &'static str,
    pub transform: fn(&mut SemanticDocument) -> Result<()>,
    pub reverse: Option<fn(&mut SemanticDocument) -> Result<()>>,
}

/// A single step recorded during a migration run.
#[derive(Debug, Clone, Serialize)]
pub struct AppliedStep {
    pub from: SchemaVersion,
    pub to: SchemaVersion,
    pub lossy: bool,
    pub digest_before: String,
    pub digest_after: String,
}

/// All registered migrations, in graph order.
pub fn migrations() -> &'static [Migration] {
    MIGRATIONS
}

static MIGRATIONS: &[Migration] = &[
    Migration {
        from: LEGACY,
        to: SchemaVersion::new(2, 0),
        reversible: false,
        lossy: false,
        notes: "bootstrap a semantic-state document from implicit v1 state",
        transform: migrate_1_0_to_2_0,
        reverse: None,
    },
    Migration {
        from: SchemaVersion::new(2, 0),
        to: SchemaVersion::new(2, 1),
        reversible: true,
        lossy: false,
        notes: "enrich surfaces with audience/stability/compatibility; unknown is a valid state",
        transform: migrate_2_0_to_2_1,
        reverse: Some(reverse_2_1_to_2_0),
    },
    Migration {
        from: SchemaVersion::new(2, 1),
        to: SchemaVersion::new(2, 2),
        reversible: true,
        lossy: false,
        notes: "declare governed branch topology and stable/bleeding channels",
        transform: migrate_2_1_to_2_2,
        reverse: Some(reverse_2_2_to_2_1),
    },
];

/// The implicit document representing pre-semantic-state repositories
/// (no `.kaptaind/state.toml`).
pub fn legacy_document() -> SemanticDocument {
    let mut doc = SemanticDocument::empty(LEGACY);
    doc.kaptaind.schema = "legacy".into();
    doc
}

/// Detect a document's format version: `None` when there is no state file.
pub fn detect(text: Option<&str>) -> Result<SemanticDocument> {
    match text {
        None => Ok(legacy_document()),
        Some(text) => SemanticDocument::parse(text),
    }
}

/// Ordered migration steps (and whether each is lossy in the chosen
/// direction) needed to move `from` to `to`. Empty when already there.
pub fn plan(from: SchemaVersion, to: SchemaVersion) -> Result<Vec<&'static Migration>> {
    if from == to {
        return Ok(Vec::new());
    }
    let mut steps = Vec::new();
    if from < to {
        let mut current = from;
        while current < to {
            let step = migrations()
                .iter()
                .filter(|m| m.from == current && m.to <= to)
                .min_by_key(|m| m.to);
            let Some(step) = step else {
                bail!("no migration path from {from} to {to} (stuck at {current})");
            };
            current = step.to;
            steps.push(step);
        }
    } else {
        let mut current = from;
        while current > to {
            let step = migrations()
                .iter()
                .find(|m| m.to == current && m.reversible && m.from >= to);
            let Some(step) = step else {
                bail!(
                    "no reversible downgrade path from {from} to {to} (stuck at {current}); \
                     downgrades may require --allow-lossy"
                );
            };
            current = step.from;
            steps.push(step);
        }
    }
    Ok(steps)
}

/// Whether a plan executes every step in reverse (downgrade) direction.
pub fn plan_is_downgrade(from: SchemaVersion, to: SchemaVersion) -> bool {
    to < from
}

/// Execute a migration plan. Validates before and after every step, refuses
/// lossy steps unless `allow_lossy` is set, and returns the applied steps
/// with before/after digests for the ledger. Idempotent: migrating a document
/// already at `to` returns it unchanged.
pub fn migrate_document(
    document: &SemanticDocument,
    to: SchemaVersion,
    allow_lossy: bool,
) -> Result<(SemanticDocument, Vec<AppliedStep>)> {
    if registry::find(to).is_none() {
        bail!("unknown schema version {to}");
    }
    document.validate()?;
    let steps = plan(document.format(), to)?;
    let mut current = document.clone();
    let mut applied = Vec::new();
    let downgrade = plan_is_downgrade(document.format(), to);

    for step in steps {
        let lossy = if downgrade { true } else { step.lossy };
        if lossy && !allow_lossy {
            let (from, to) = if downgrade {
                (step.to, step.from)
            } else {
                (step.from, step.to)
            };
            bail!("migration {from} -> {to} is lossy; re-run with --allow-lossy");
        }
        let digest_before = current.digest();
        let transform = if downgrade {
            let reverse = step.reverse.ok_or_else(|| {
                anyhow::anyhow!("migration {} -> {} has no reverse", step.from, step.to)
            })?;
            reverse
        } else {
            step.transform
        };
        transform(&mut current)?;
        current.kaptaind.format = if downgrade { step.from } else { step.to };
        if downgrade {
            current.kaptaind.schema = if step.from == LEGACY {
                "legacy".into()
            } else {
                "semantic-state".into()
            };
        } else {
            current.kaptaind.schema = if step.to == LEGACY {
                "legacy".into()
            } else {
                "semantic-state".into()
            };
        }
        current.validate().context(format!(
            "migration {} -> {} produced an invalid document",
            step.from, step.to
        ))?;
        let digest_after = current.digest();
        applied.push(AppliedStep {
            from: step.from,
            to: step.to,
            lossy,
            digest_before,
            digest_after,
        });
    }
    Ok((current, applied))
}

fn migrate_1_0_to_2_0(doc: &mut SemanticDocument) -> Result<()> {
    let seeded = SemanticDocument::empty(SchemaVersion::new(2, 0));
    // Preserve any baseline version observed in the legacy state.
    let baseline_version = doc.baseline.version.clone();
    *doc = seeded;
    doc.baseline.version = baseline_version;
    Ok(())
}

fn migrate_2_0_to_2_1(doc: &mut SemanticDocument) -> Result<()> {
    for surface in &mut doc.surface {
        if surface.audience.is_none() {
            let audience = if surface.kind == "public" {
                vec!["external".to_string()]
            } else {
                vec!["internal".to_string()]
            };
            surface.audience = Some(audience);
        }
        // Never manufacture certainty: unknown/unspecified are valid states.
        surface.stability.get_or_insert_with(|| "unknown".into());
        surface
            .compatibility
            .get_or_insert_with(|| "unspecified".into());
    }
    Ok(())
}

fn reverse_2_1_to_2_0(doc: &mut SemanticDocument) -> Result<()> {
    for surface in &mut doc.surface {
        surface.audience = None;
        surface.stability = None;
        surface.compatibility = None;
    }
    Ok(())
}

fn migrate_2_1_to_2_2(doc: &mut SemanticDocument) -> Result<()> {
    doc.capabilities.branch_lifecycle = true;
    doc.branches = super::document::BranchTopology::default();
    doc.channels = super::document::ConsumerChannels::default();
    Ok(())
}

fn reverse_2_2_to_2_1(doc: &mut SemanticDocument) -> Result<()> {
    doc.capabilities.branch_lifecycle = false;
    Ok(())
}

/// One entry in the append-only migration ledger under
/// `.kaptaind/migrations/`.
#[derive(Debug, Serialize)]
pub struct LedgerEntry {
    pub from: SchemaVersion,
    pub to: SchemaVersion,
    pub tool: String,
    pub timestamp: DateTime<Utc>,
    pub canonicalization: String,
    pub digest_before: String,
    pub digest_after: String,
    pub lossy: bool,
}

/// Append a migration record to the ledger directory.
pub fn append_ledger(dir: &Path, entry: &LedgerEntry) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let stamp = entry.timestamp.format("%Y%m%dT%H%M%S");
    let name = format!("{stamp}-v{}-v{}.json", entry.from, entry.to);
    let path = dir.join(name);
    let body = serde_json::to_string_pretty(entry)?;
    Ok(std::fs::write(path, body)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstraps_legacy_to_latest_and_is_idempotent() {
        let legacy = legacy_document();
        let (migrated, steps) = migrate_document(&legacy, registry::latest_version(), false)
            .expect("legacy migrates losslessly");
        assert_eq!(migrated.format(), registry::latest_version());
        assert_eq!(steps.len(), 3);

        // Idempotence: migrating again applies nothing.
        let (again, steps) =
            migrate_document(&migrated, registry::latest_version(), false).unwrap();
        assert!(steps.is_empty());
        assert_eq!(again.digest(), migrated.digest());

        migrated.validate().unwrap();
    }

    #[test]
    fn upgrade_2_0_to_2_1_enriches_without_manufacturing_certainty() {
        let mut doc = SemanticDocument::empty(SchemaVersion::new(2, 0));
        doc.surface.push(super::super::document::Surface {
            id: "api".into(),
            paths: vec!["src/api/**".into()],
            kind: "public".into(),
            audience: None,
            stability: None,
            compatibility: None,
        });
        let (migrated, _) = migrate_document(&doc, SchemaVersion::new(2, 1), false).unwrap();
        let surface = &migrated.surface[0];
        assert_eq!(surface.stability.as_deref(), Some("unknown"));
        assert_eq!(surface.compatibility.as_deref(), Some("unspecified"));
        assert_eq!(
            surface.audience.as_deref(),
            Some(&["external".to_string()][..])
        );
    }

    #[test]
    fn downgrade_requires_allow_lossy() {
        let doc = SemanticDocument::empty(SchemaVersion::new(2, 1));
        let err = migrate_document(&doc, SchemaVersion::new(2, 0), false).unwrap_err();
        assert!(err.to_string().contains("allow-lossy"));
        let (downgraded, steps) = migrate_document(&doc, SchemaVersion::new(2, 0), true).unwrap();
        assert_eq!(downgraded.format(), SchemaVersion::new(2, 0));
        assert!(steps.iter().all(|s| s.lossy));
        downgraded.validate().unwrap();
    }

    #[test]
    fn downgrade_to_legacy_is_refused() {
        let doc = SemanticDocument::empty(SchemaVersion::new(2, 1));
        assert!(plan(doc.format(), LEGACY).is_err());
    }

    #[test]
    fn unknown_target_is_rejected() {
        let doc = SemanticDocument::empty(SchemaVersion::new(2, 1));
        let err = migrate_document(&doc, SchemaVersion::new(9, 9), false).unwrap_err();
        assert!(err.to_string().contains("unknown schema version"));
    }

    #[test]
    fn plan_walks_one_step_at_a_time() {
        let steps = plan(LEGACY, registry::latest_version()).unwrap();
        let versions: Vec<String> = steps
            .iter()
            .map(|s| format!("{}->{}", s.from, s.to))
            .collect();
        assert_eq!(versions, vec!["1.0->2.0", "2.0->2.1", "2.1->2.2"]);
    }

    #[test]
    fn ledger_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let entry = LedgerEntry {
            from: SchemaVersion::new(2, 0),
            to: SchemaVersion::new(2, 1),
            tool: "kaptaind test".into(),
            timestamp: DateTime::parse_from_rfc3339("2026-08-22T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            canonicalization: "kaptaind-c14n-v1".into(),
            digest_before: "sha256:a".into(),
            digest_after: "sha256:b".into(),
            lossy: false,
        };
        append_ledger(dir.path(), &entry).unwrap();
        let written =
            std::fs::read_to_string(dir.path().join("20260822T000000-v2.0-v2.1.json")).unwrap();
        assert!(written.contains("\"from\": \"2.0\""));
    }
}
