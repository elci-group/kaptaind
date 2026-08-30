use kaptaind::config::loader::Config;
use std::path::Path;

/// Validate that a file is a well-formed `bound-snapshot/v1` JSON document.
pub fn validate_snapshot(path: &Path) -> anyhow::Result<bound_core::Snapshot> {
    let bytes = std::fs::read(path)
        .map_err(|error| anyhow::anyhow!("failed to read snapshot {}: {error}", path.display()))?;
    let snapshot: bound_core::Snapshot = serde_json::from_slice(&bytes)
        .map_err(|error| anyhow::anyhow!("invalid bound snapshot {}: {error}", path.display()))?;
    if snapshot.schema != "bound-snapshot" {
        anyhow::bail!(
            "snapshot {} has unsupported schema '{}', expected 'bound-snapshot'",
            path.display(),
            snapshot.schema
        );
    }
    if snapshot.schema_version != "1" {
        anyhow::bail!(
            "snapshot {} has unsupported schema version '{}', expected '1'",
            path.display(),
            snapshot.schema_version
        );
    }
    Ok(snapshot)
}

/// Record a `bound-snapshot/v1` artifact as release evidence, validating the
/// schema before hashing so kaptaind never attaches a malformed snapshot.
pub fn record_snapshot(config: &Config, version: &str, file: &Path) -> anyhow::Result<()> {
    let snapshot = validate_snapshot(file)?;
    let source = format!(
        "bound-snapshot/v1 {} files from {}",
        snapshot.summary.file_count,
        snapshot.target.display()
    );
    record(config, version, "bound-snapshot", &source, file)
}

/// Record externally produced evidence by digest and provenance only.
pub fn record(
    config: &Config,
    version: &str,
    kind: &str,
    source: &str,
    file: &std::path::Path,
) -> anyhow::Result<()> {
    if source.trim().is_empty() {
        anyhow::bail!("evidence source must not be empty");
    }
    let bytes = std::fs::read(file).map_err(|error| {
        anyhow::anyhow!(
            "failed to read evidence artifact {}: {error}",
            file.display()
        )
    })?;
    let mut record = kaptaind::evidence::EvidenceRecord::for_bytes(kind, source, &bytes);
    if let Some(policy_id) = config.policy_id.as_deref() {
        let policy = kaptaind::daemon::policy::Policy::load_with_trust(
            &config.repo_path,
            policy_id,
            &config.policy_trust,
            config.policy_keyring_path().as_deref(),
        )?;
        if policy.require_evidence_expiry {
            let hours = policy.evidence_validity_hours.ok_or_else(|| {
                anyhow::anyhow!(
                    "policy requires evidence expiry but does not define evidence_validity_hours"
                )
            })?;
            if hours == 0 || hours > 8760 {
                anyhow::bail!("policy evidence_validity_hours must be between 1 and 8760");
            }
            record.expires_at = Some(record.issued_at + chrono::Duration::hours(i64::from(hours)));
        }
        kaptaind::evidence::sign_record(&mut record, policy.require_evidence_hmac)?;
    }
    kaptaind::evidence::save(&config.repo_path, version, &record)?;
    kaptaind::audit::log_event(
        &config.repo_path,
        "evidence-cli",
        "evidence_recorded",
        true,
        serde_json::json!({"version": version, "kind": kind, "source": source, "sha256": record.sha256}),
    );
    println!("recorded {kind} evidence for v{version}: {}", record.sha256);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_stores_digest_metadata_without_copying_the_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = dir.path().join("results.sarif");
        std::fs::write(&artifact, "sensitive scanner payload").unwrap();
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        record(&config, "1.2.3", "sarif", "ci", &artifact).unwrap();
        let record_path = dir.path().join(".kaptaind/evidence/1.2.3/sarif.json");
        let stored = std::fs::read_to_string(record_path).unwrap();
        assert!(stored.contains("sha256"));
        assert!(stored.contains("ci"));
        assert!(!stored.contains("sensitive scanner payload"));
    }

    #[test]
    fn record_rejects_an_empty_provenance_source() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = dir.path().join("results.sarif");
        std::fs::write(&artifact, "scanner payload").unwrap();
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        assert!(record(&config, "1.2.3", "sarif", " ", &artifact).is_err());
    }

    #[test]
    fn record_applies_the_configured_policy_evidence_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let policies = dir.path().join(".kaptaind/policies");
        std::fs::create_dir_all(&policies).unwrap();
        std::fs::write(
            policies.join("production.json"),
            r#"{"require_evidence_expiry":true,"evidence_validity_hours":24}"#,
        )
        .unwrap();
        let artifact = dir.path().join("results.sarif");
        std::fs::write(&artifact, "scanner payload").unwrap();
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            policy_id: Some("production".to_string()),
            ..Config::default()
        };
        record(&config, "1.2.3", "sarif", "ci", &artifact).unwrap();
        let record: kaptaind::evidence::EvidenceRecord = serde_json::from_slice(
            &std::fs::read(dir.path().join(".kaptaind/evidence/1.2.3/sarif.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            record.expires_at.unwrap() - record.issued_at,
            chrono::Duration::hours(24)
        );
    }
}
