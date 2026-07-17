//! Structured audit logging for compliance and incident response.
//!
//! Writes append-only JSON Lines to `.kaptaind/audit.jsonl`. Each entry records
//! a security-relevant event (commit, push, release, qualification decision,
//! config change) with actor, timestamp, outcome, and contextual details.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};

/// Process-local export configuration installed during daemon or CLI startup.
///
/// Keeping this at the audit boundary lets existing audit call sites retain a
/// small, fallible API while ensuring a configured export applies uniformly.
/// It is intentionally empty until `configure_export` is called.
static EXPORT_CONFIG: OnceLock<RwLock<Option<AuditExportConfig>>> = OnceLock::new();
static GOVERNANCE_CONTEXT: OnceLock<RwLock<Option<AuditGovernanceContext>>> = OnceLock::new();
static AUDIT_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Optional provider-neutral audit export configuration.
///
/// When configured, every audit entry is additionally appended as one JSON
/// object per line at `jsonl_path`. The destination is deliberately a local
/// file sink: forwarding to a SIEM is the responsibility of the operator's
/// existing collector (for example Fluent Bit, Filebeat, or an OS log agent),
/// so Kaptaind neither stores collector credentials nor performs network I/O.
///
/// Relative paths must be normalized by the configuration loader before this
/// value reaches the audit writer.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AuditExportConfig {
    /// Additional append-only JSON Lines destination for audit events.
    #[serde(default)]
    pub jsonl_path: Option<PathBuf>,
}

/// Non-secret organization scope attached to every audit record produced by a
/// configured process. It lets downstream collectors partition evidence by
/// organization and tenant without inferring scope from filesystem paths.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditGovernanceContext {
    pub organization_id: String,
    pub tenant_id: String,
}

pub fn configure_governance_context(organization_id: Option<String>, tenant_id: Option<String>) {
    let context = match (organization_id, tenant_id) {
        (Some(organization_id), Some(tenant_id)) => Some(AuditGovernanceContext {
            organization_id,
            tenant_id,
        }),
        _ => None,
    };
    let slot = GOVERNANCE_CONTEXT.get_or_init(|| RwLock::new(None));
    *slot
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = context;
}

/// Install or clear the process-local JSONL export sink.
///
/// Startup should call this once after configuration paths have been
/// normalized. Reconfiguration is supported for in-process configuration
/// reloads; writes take a snapshot of the setting before touching disk.
pub fn configure_export(export: Option<AuditExportConfig>) {
    let config = EXPORT_CONFIG.get_or_init(|| RwLock::new(None));
    let mut configured = config
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *configured = export;
}

fn configured_export() -> Option<AuditExportConfig> {
    EXPORT_CONFIG.get().and_then(|config| {
        config
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    })
}

fn configured_governance_context() -> Option<AuditGovernanceContext> {
    GOVERNANCE_CONTEXT.get().and_then(|context| {
        context
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    })
}

/// One audit record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO-8601 timestamp in UTC.
    pub timestamp: DateTime<Utc>,
    /// Category of event, e.g. `commit`, `push`, `release`, `qualification`.
    pub event_type: String,
    /// Actor that triggered the event (instance_id for daemon actions).
    pub actor: String,
    /// Event outcome: `success`, `failure`, `blocked`, `skipped`.
    pub result: String,
    /// Free-form structured details.
    pub details: serde_json::Value,
}

/// Tamper-evident companion record for an audit entry. It is a hash chain, not
/// a claim of immutable storage: configure an external WORM/object-lock sink
/// for protection against a filesystem administrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditChainEntry {
    pub sequence: u64,
    pub previous_sha256: Option<String>,
    pub entry_sha256: String,
}

/// Versioned record written to an optional audit-export sink. It retains the
/// event shape under `audit` while carrying the exact primary-chain position
/// and digest required for an independent collector to verify provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportRecord {
    pub schema_version: u8,
    pub audit: AuditEntry,
    pub integrity: AuditChainEntry,
}

impl AuditEntry {
    pub fn new(
        event_type: impl Into<String>,
        actor: impl Into<String>,
        result: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: event_type.into(),
            actor: actor.into(),
            result: result.into(),
            details: serde_json::Value::Null,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

/// Append a single audit entry to `.kaptaind/audit.jsonl`.
pub fn append(repo_path: &Path, entry: &AuditEntry) -> anyhow::Result<()> {
    let export = configured_export();
    let entry = scoped_entry(entry, configured_governance_context());
    append_with_export(repo_path, export.as_ref(), &entry)
}

fn scoped_entry(entry: &AuditEntry, context: Option<AuditGovernanceContext>) -> AuditEntry {
    let Some(context) = context else {
        return entry.clone();
    };
    let mut scoped = entry.clone();
    match &mut scoped.details {
        serde_json::Value::Object(details) => {
            details.insert(
                "_kaptaind_governance".to_string(),
                serde_json::json!(context),
            );
        }
        details => {
            *details = serde_json::json!({
                "_kaptaind_governance": context,
                "event_details": details,
            });
        }
    }
    scoped
}

/// Append an entry to the primary audit log and, when configured, a local
/// JSONL export sink. The primary log is always written first so an export
/// outage cannot become the system of record.
pub fn append_with_export(
    repo_path: &Path,
    export: Option<&AuditExportConfig>,
    entry: &AuditEntry,
) -> anyhow::Result<()> {
    let _guard = AUDIT_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let primary_path = default_path(repo_path);
    append_to_path(&primary_path, entry)?;
    let integrity = append_chain_entry(&primary_path, entry)?;

    if let Some(export_path) = export.and_then(|config| config.jsonl_path.as_deref()) {
        // Avoid writing duplicate records when an operator explicitly selects
        // Kaptaind's built-in audit path as the export destination.
        if export_path != primary_path {
            append_export_record(export_path, entry, &integrity)?;
        }
    }
    Ok(())
}

/// Verify that an optional audit-export sink has a one-for-one, integrity
/// linked copy of the primary audit chain. This validates the local hand-off
/// boundary before a collector forwards it to a SIEM or immutable archive.
pub fn verify_export(repo_path: &Path, export: &AuditExportConfig) -> anyhow::Result<()> {
    let export_path = export.jsonl_path.as_deref().ok_or_else(|| {
        anyhow::anyhow!("audit export verification requires audit.export.jsonl_path")
    })?;
    let primary_path = default_path(repo_path);
    if export_path == primary_path {
        return verify_chain(repo_path);
    }
    // A new repository has no audit history to hand off yet. Treat an absent
    // primary log and absent/empty export as healthy so the first audit write
    // can establish both files atomically from Kaptaind's perspective.
    if !primary_path.exists() {
        if export_path.exists()
            && std::fs::read_to_string(export_path)?
                .lines()
                .any(|line| !line.trim().is_empty())
        {
            anyhow::bail!("audit export contains records but primary audit log is absent");
        }
        return Ok(());
    }
    verify_chain(repo_path)?;
    let primary: Vec<AuditEntry> = std::fs::read_to_string(&primary_path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let chain: Vec<AuditChainEntry> = std::fs::read_to_string(chain_path(&primary_path))?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let exported: Vec<AuditExportRecord> = std::fs::read_to_string(export_path)
        .map_err(|error| {
            anyhow::anyhow!("audit export missing at {}: {error}", export_path.display())
        })?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    if primary.len() != exported.len() || chain.len() != exported.len() {
        anyhow::bail!("audit export length does not match primary audit chain");
    }
    for (index, ((entry, chain), exported)) in primary
        .iter()
        .zip(chain.iter())
        .zip(exported.iter())
        .enumerate()
    {
        if exported.schema_version != 1
            || serde_json::to_vec(&exported.audit)? != serde_json::to_vec(entry)?
            || exported.integrity.sequence != chain.sequence
            || exported.integrity.previous_sha256 != chain.previous_sha256
            || exported.integrity.entry_sha256 != chain.entry_sha256
        {
            anyhow::bail!("audit export verification failed at sequence {}", index + 1);
        }
    }
    Ok(())
}

/// Verify that every audit line has an unbroken companion hash-chain record.
/// This detects truncation, insertion, reordering, and modification unless an
/// attacker rewrites both files; external immutable storage remains required
/// for administrator-resistant evidence.
pub fn verify_chain(repo_path: &Path) -> anyhow::Result<()> {
    let audit_path = default_path(repo_path);
    if !audit_path.exists() {
        return Ok(());
    }
    let audit_lines: Vec<String> = std::fs::read_to_string(&audit_path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect();
    let chain_path = chain_path(&audit_path);
    let chain_lines: Vec<AuditChainEntry> = std::fs::read_to_string(&chain_path)
        .map_err(|error| {
            anyhow::anyhow!("audit chain missing at {}: {error}", chain_path.display())
        })?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    if audit_lines.len() != chain_lines.len() {
        anyhow::bail!("audit chain length does not match audit log");
    }
    let mut previous: Option<String> = None;
    for (index, (line, chain)) in audit_lines.iter().zip(chain_lines.iter()).enumerate() {
        if chain.sequence != index as u64 + 1
            || chain.previous_sha256 != previous
            || chain.entry_sha256 != digest(line.as_bytes())
        {
            anyhow::bail!("audit chain verification failed at sequence {}", index + 1);
        }
        previous = Some(chain.entry_sha256.clone());
    }
    Ok(())
}

/// Return Kaptaind's local system-of-record audit path.
pub fn default_path(repo_path: &Path) -> PathBuf {
    repo_path.join(".kaptaind").join("audit.jsonl")
}

fn chain_path(audit_path: &Path) -> PathBuf {
    audit_path.with_file_name("audit.chain.jsonl")
}

fn digest(bytes: &[u8]) -> String {
    crate::util::hex::encode(Sha256::digest(bytes))
}

fn append_chain_entry(audit_path: &Path, entry: &AuditEntry) -> anyhow::Result<AuditChainEntry> {
    let chain_path = chain_path(audit_path);
    let previous_entries: Vec<AuditChainEntry> = if chain_path.exists() {
        std::fs::read_to_string(&chain_path)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?
    } else {
        Vec::new()
    };
    let chain = AuditChainEntry {
        sequence: previous_entries.len() as u64 + 1,
        previous_sha256: previous_entries
            .last()
            .map(|record| record.entry_sha256.clone()),
        entry_sha256: digest(&serde_json::to_vec(entry)?),
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(chain_path)?;
    file.write_all(format!("{}\n", serde_json::to_string(&chain)?).as_bytes())?;
    file.sync_data()?;
    Ok(chain)
}

/// Append one JSON object and a newline to an explicit local file sink.
///
/// The caller controls the destination; no network transports are supported
/// here. Parent directories are created to support managed log directories.
pub fn append_to_path(path: &Path, entry: &AuditEntry) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(entry)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(format!("{}\n", line).as_bytes())?;
    file.sync_data()?;
    Ok(())
}

fn append_export_record(
    path: &Path,
    entry: &AuditEntry,
    integrity: &AuditChainEntry,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let record = AuditExportRecord {
        schema_version: 1,
        audit: entry.clone(),
        integrity: integrity.clone(),
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(format!("{}\n", serde_json::to_string(&record)?).as_bytes())?;
    file.sync_data()?;
    Ok(())
}

/// Convenience: log a successful commit.
pub fn log_commit(
    repo_path: &Path,
    actor: &str,
    version: &str,
    bump: &str,
    score: f64,
    cluster_id: &str,
    files_changed: usize,
) {
    let entry = AuditEntry::new("commit", actor, "success").with_details(serde_json::json!({
        "version": version,
        "bump": bump,
        "score": score,
        "cluster_id": cluster_id,
        "files_changed": files_changed,
    }));
    append_or_warn(repo_path, entry, "commit");
}

/// Convenience: log a push attempt.
pub fn log_push(
    repo_path: &Path,
    actor: &str,
    version: &str,
    branch: &str,
    remote: &str,
    success: bool,
    error: Option<&str>,
) {
    let entry = AuditEntry::new("push", actor, if success { "success" } else { "failure" })
        .with_details(serde_json::json!({
            "version": version,
            "branch": branch,
            "remote": remote,
            "error": error,
        }));
    append_or_warn(repo_path, entry, "push");
}

/// Convenience: log a generic event.
pub fn log_event(
    repo_path: &Path,
    actor: &str,
    event_type: &str,
    success: bool,
    details: serde_json::Value,
) {
    let entry = AuditEntry::new(
        event_type,
        actor,
        if success { "success" } else { "failure" },
    )
    .with_details(details);
    append_or_warn(repo_path, entry, event_type);
}

/// Convenience: log a release/shipment.
pub fn log_release(
    repo_path: &Path,
    actor: &str,
    version: &str,
    kind: &str,
    channels: &[String],
    success: bool,
) {
    let entry = AuditEntry::new(
        "release",
        actor,
        if success { "success" } else { "failure" },
    )
    .with_details(serde_json::json!({
        "version": version,
        "kind": kind,
        "channels": channels,
    }));
    append_or_warn(repo_path, entry, "release");
}

/// Convenience: log a qualification decision.
pub fn log_qualification(
    repo_path: &Path,
    actor: &str,
    version: &str,
    stability: f64,
    decision: &str,
    reason: Option<String>,
) {
    let entry = AuditEntry::new("qualification", actor, decision).with_details(serde_json::json!({
        "version": version,
        "stability": stability,
        "reason": reason,
    }));
    append_or_warn(repo_path, entry, "qualification");
}

fn append_or_warn(repo_path: &Path, entry: AuditEntry, kind: &str) {
    if let Err(err) = append(repo_path, &entry) {
        tracing::warn!(error = %err, kind, "failed to write audit entry");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;

    // Process-local configuration is shared by test threads; serialize only
    // tests which install it and always clear it on scope exit.
    static EXPORT_CONFIG_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct ResetExportConfig;

    impl Drop for ResetExportConfig {
        fn drop(&mut self) {
            configure_export(None);
        }
    }

    #[test]
    fn audit_entry_roundtrips() {
        let dir = tempdir().unwrap();
        let entry = AuditEntry::new("commit", "test@localhost", "success")
            .with_details(serde_json::json!({"version": "1.2.3"}));
        append(dir.path(), &entry).unwrap();

        let path = dir.path().join(".kaptaind").join("audit.jsonl");
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: AuditEntry = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed.event_type, "commit");
        assert_eq!(parsed.result, "success");
        assert_eq!(parsed.details["version"], "1.2.3");
    }

    #[test]
    fn log_commit_appends_record() {
        let dir = tempdir().unwrap();
        log_commit(dir.path(), "daemon", "1.2.3", "Minor", 0.75, "cluster-1", 4);
        let path = dir.path().join(".kaptaind").join("audit.jsonl");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"event_type\":\"commit\""));
        assert!(content.contains("\"version\":\"1.2.3\""));
    }

    #[test]
    fn configured_jsonl_export_receives_an_integrity_linked_record() {
        let dir = tempdir().unwrap();
        let export_path = dir.path().join("siem").join("kaptaind.jsonl");
        let config = AuditExportConfig {
            jsonl_path: Some(export_path.clone()),
        };
        let entry = AuditEntry::new("release", "daemon", "success")
            .with_details(serde_json::json!({"version": "1.2.3"}));

        append_with_export(dir.path(), Some(&config), &entry).unwrap();

        let exported = std::fs::read_to_string(&export_path).unwrap();
        let parsed: AuditExportRecord = serde_json::from_str(exported.trim()).unwrap();
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.audit.event_type, "release");
        assert_eq!(parsed.integrity.sequence, 1);
        assert!(verify_export(dir.path(), &config).is_ok());
    }

    #[test]
    fn export_to_primary_path_does_not_duplicate_the_record() {
        let dir = tempdir().unwrap();
        let config = AuditExportConfig {
            jsonl_path: Some(default_path(dir.path())),
        };

        append_with_export(
            dir.path(),
            Some(&config),
            &AuditEntry::new("commit", "daemon", "success"),
        )
        .unwrap();

        let content = std::fs::read_to_string(default_path(dir.path())).unwrap();
        assert_eq!(content.lines().count(), 1);
    }

    #[test]
    fn process_local_export_applies_to_existing_append_callers() {
        let _lock = EXPORT_CONFIG_TEST_LOCK.lock().unwrap();
        let _reset = ResetExportConfig;
        let dir = tempdir().unwrap();
        let export_path = dir.path().join("collector").join("audit.jsonl");
        configure_export(Some(AuditExportConfig {
            jsonl_path: Some(export_path.clone()),
        }));

        append(dir.path(), &AuditEntry::new("push", "daemon", "success")).unwrap();

        assert!(export_path.exists());
        let exported = std::fs::read_to_string(&export_path).unwrap();
        assert!(exported.contains("\"schema_version\":1"));
        assert!(verify_export(
            dir.path(),
            &AuditExportConfig {
                jsonl_path: Some(export_path),
            },
        )
        .is_ok());
    }

    #[test]
    fn export_verifier_rejects_a_tampered_collector_record() {
        let dir = tempdir().unwrap();
        let export_path = dir.path().join("collector/audit.jsonl");
        let config = AuditExportConfig {
            jsonl_path: Some(export_path.clone()),
        };
        append_with_export(
            dir.path(),
            Some(&config),
            &AuditEntry::new("release", "ci", "success"),
        )
        .unwrap();
        let mut record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&export_path).unwrap()).unwrap();
        record["audit"]["result"] = serde_json::json!("failure");
        std::fs::write(&export_path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(verify_export(dir.path(), &config).is_err());
    }

    #[test]
    fn export_verifier_allows_a_pristine_uninitialized_audit_store() {
        let dir = tempdir().unwrap();
        let config = AuditExportConfig {
            jsonl_path: Some(dir.path().join("collector/audit.jsonl")),
        };
        assert!(verify_export(dir.path(), &config).is_ok());
    }

    #[test]
    fn governance_context_is_embedded_without_overwriting_event_details() {
        let entry = AuditEntry::new("release", "ci", "success")
            .with_details(serde_json::json!({"version": "1.2.3"}));
        let scoped = scoped_entry(
            &entry,
            Some(AuditGovernanceContext {
                organization_id: "acme".to_string(),
                tenant_id: "payments".to_string(),
            }),
        );
        assert_eq!(scoped.details["version"], "1.2.3");
        assert_eq!(
            scoped.details["_kaptaind_governance"]["organization_id"],
            "acme"
        );
        assert_eq!(
            scoped.details["_kaptaind_governance"]["tenant_id"],
            "payments"
        );
    }
}
