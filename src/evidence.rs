//! Provider-neutral release evidence records.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub schema_version: u8,
    pub kind: String,
    pub source: String,
    pub sha256: String,
    pub issued_at: DateTime<Utc>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// HMAC over the canonical record, excluding this field. When required by
    /// policy this proves that a holder of the externally managed evidence
    /// signing secret authorized this exact digest/provenance/validity tuple.
    #[serde(default)]
    pub hmac_sha256: Option<String>,
}

impl EvidenceRecord {
    pub fn for_bytes(kind: impl Into<String>, source: impl Into<String>, bytes: &[u8]) -> Self {
        Self {
            schema_version: 1,
            kind: kind.into(),
            source: source.into(),
            sha256: crate::util::hex::encode(Sha256::digest(bytes)),
            issued_at: Utc::now(),
            expires_at: None,
            hmac_sha256: None,
        }
    }
}

/// Sign or clear the policy-controlled evidence authenticator. The secret is
/// intentionally read only from process environment, so it can be supplied by
/// a CI secret manager rather than committed configuration.
pub fn sign_record(record: &mut EvidenceRecord, required: bool) -> anyhow::Result<()> {
    let key = std::env::var("KAPTAIND_EVIDENCE_HMAC_KEY")
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        .filter(|key| !key.is_empty());
    sign_record_with_key(record, required, key.as_deref().map(str::as_bytes))
}

fn sign_record_with_key(
    record: &mut EvidenceRecord,
    required: bool,
    key: Option<&[u8]>,
) -> anyhow::Result<()> {
    let Some(key) = key else {
        if required {
            anyhow::bail!(
                "policy requires evidence HMAC but KAPTAIND_EVIDENCE_HMAC_KEY is unavailable"
            );
        }
        record.hmac_sha256 = None;
        return Ok(());
    };
    record.hmac_sha256 = Some(record_hmac(record, key)?);
    Ok(())
}

fn record_hmac(record: &EvidenceRecord, key: &[u8]) -> anyhow::Result<String> {
    let mut canonical = record.clone();
    canonical.hmac_sha256 = None;
    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .map_err(|error| anyhow::anyhow!("invalid evidence HMAC key: {error}"))?;
    mac.update(&serde_json::to_vec(&canonical)?);
    Ok(crate::util::hex::encode(mac.finalize().into_bytes()))
}

fn verify_record_hmac(record: &EvidenceRecord, required: bool) -> anyhow::Result<()> {
    if !required {
        return Ok(());
    }
    let key = std::env::var("KAPTAIND_EVIDENCE_HMAC_KEY").map_err(|error| {
        anyhow::anyhow!(
            "policy requires evidence HMAC but KAPTAIND_EVIDENCE_HMAC_KEY is unavailable: {error}"
        )
    })?;
    verify_record_hmac_with_key(record, required, Some(key.as_bytes()))
}

fn verify_record_hmac_with_key(
    record: &EvidenceRecord,
    required: bool,
    key: Option<&[u8]>,
) -> anyhow::Result<()> {
    if !required {
        return Ok(());
    }
    let signature = record
        .hmac_sha256
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("required evidence is missing its HMAC"))?;
    let key = key.ok_or_else(|| {
        anyhow::anyhow!(
            "policy requires evidence HMAC but KAPTAIND_EVIDENCE_HMAC_KEY is unavailable"
        )
    })?;
    let expected = record_hmac(record, key)?;
    if crate::util::constant_time::constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        anyhow::bail!("required evidence HMAC verification failed")
    }
}

pub fn evidence_path(repo_path: &Path, version: &str, kind: &str) -> anyhow::Result<PathBuf> {
    if version.is_empty()
        || version.len() > 128
        || !version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
    {
        anyhow::bail!("evidence version is not a safe managed-path component");
    }
    if kind.is_empty()
        || !kind.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        anyhow::bail!("evidence kind must contain only ASCII letters, digits, '_' or '-'");
    }
    Ok(repo_path
        .join(".kaptaind")
        .join("evidence")
        .join(version)
        .join(format!("{kind}.json")))
}

pub fn save(repo_path: &Path, version: &str, record: &EvidenceRecord) -> anyhow::Result<()> {
    let path = evidence_path(repo_path, version, &record.kind)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("evidence path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(record)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

pub fn verify_required(
    repo_path: &Path,
    version: &str,
    kinds: &[String],
    require_hmac: bool,
) -> anyhow::Result<Vec<EvidenceRecord>> {
    let mut verified = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let path = evidence_path(repo_path, version, kind)?;
        let bytes = std::fs::read(&path).map_err(|error| {
            anyhow::anyhow!(
                "required evidence {kind:?} missing at {}: {error}",
                path.display()
            )
        })?;
        let record: EvidenceRecord = serde_json::from_slice(&bytes)
            .map_err(|error| anyhow::anyhow!("required evidence {kind:?} is invalid: {error}"))?;
        if record.schema_version != 1
            || record.kind != *kind
            || record.source.trim().is_empty()
            || record.sha256.len() != 64
        {
            anyhow::bail!("required evidence {kind:?} has an invalid identity or digest");
        }
        if record.expires_at.is_some_and(|expiry| expiry <= Utc::now()) {
            anyhow::bail!("required evidence {kind:?} has expired");
        }
        verify_record_hmac(&record, require_hmac)?;
        verified.push(record);
    }
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_evidence_is_present_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let record = EvidenceRecord::for_bytes("ci_attestation", "github-actions", b"attestation");
        save(dir.path(), "1.2.3", &record).unwrap();
        assert_eq!(
            verify_required(dir.path(), "1.2.3", &["ci_attestation".to_string()], false).unwrap(),
            vec![record]
        );
        assert!(verify_required(dir.path(), "1.2.3", &["sarif".to_string()], false).is_err());
    }

    #[test]
    fn evidence_path_rejects_version_traversal() {
        let dir = tempfile::tempdir().unwrap();
        assert!(evidence_path(dir.path(), "../../outside", "sarif").is_err());
    }

    #[test]
    fn required_evidence_hmac_rejects_unsigned_and_tampered_records() {
        let mut record = EvidenceRecord::for_bytes("sarif", "ci", b"scanner result");
        assert!(verify_record_hmac_with_key(&record, true, Some(b"evidence-key")).is_err());
        sign_record_with_key(&mut record, true, Some(b"evidence-key")).unwrap();
        assert!(verify_record_hmac_with_key(&record, true, Some(b"evidence-key")).is_ok());
        record.source = "forged-source".to_string();
        assert!(verify_record_hmac_with_key(&record, true, Some(b"evidence-key")).is_err());
    }
}
