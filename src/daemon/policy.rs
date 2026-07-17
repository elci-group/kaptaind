use crate::config::loader::PolicyTrustConfig;
use anyhow::Context;
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSetBuilder};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Policy {
    #[serde(default)]
    pub min_test_coverage: bool,
    #[serde(default)]
    pub required_signoff: bool,
    #[serde(default)]
    pub branch_protection: Vec<String>,
    #[serde(default)]
    pub file_pattern_allowlist: Vec<String>,
    /// Number of distinct approvers required before a non-dry-run shipment.
    /// Approval records are bound to the exact policy digest and release
    /// version, so changing either invalidates a previously granted approval.
    #[serde(default)]
    pub required_release_approvals: usize,
    /// Require the release requester and every approver to be different
    /// authenticated subjects. Use this for protected environments.
    #[serde(default)]
    pub require_requester_approver_separation: bool,
    /// Prevent Kaptaind from performing a shipment. Useful for high-assurance
    /// repositories where it may collect evidence but an approved tool owns
    /// the final production promotion.
    #[serde(default)]
    pub advisory_only: bool,
    /// Provider-neutral evidence categories required before shipment.
    #[serde(default)]
    pub required_evidence: Vec<String>,
    /// Domain-specific evidence gates. Each gate only applies when the
    /// matching files are present in the repository, avoiding irrelevant
    /// release friction for projects that do not use that domain.
    #[serde(default)]
    pub domain_evidence: DomainEvidenceRequirements,
    /// Require approval records to carry an HMAC using the externally supplied
    /// `KAPTAIND_APPROVAL_HMAC_KEY` secret.
    #[serde(default)]
    pub require_approval_hmac: bool,
    /// Bind approval evidence to the Git commit that is eligible for release.
    /// Enabling this prevents an approval for one revision being replayed for
    /// a later revision carrying the same version string.
    #[serde(default)]
    pub require_approval_commit_binding: bool,
    /// Optional bounded validity period for approval records. A new request
    /// receives an expiry timestamp that is HMAC-covered when enabled.
    #[serde(default)]
    pub approval_validity_hours: Option<u32>,
    /// Maximum permitted age for release evidence. New records created by the
    /// evidence CLI inherit this as their expiry when configured.
    #[serde(default)]
    pub evidence_validity_hours: Option<u32>,
    /// Require each evidence record to state an explicit expiry.
    #[serde(default)]
    pub require_evidence_expiry: bool,
    /// Require release evidence metadata to be authenticated with the
    /// externally managed `KAPTAIND_EVIDENCE_HMAC_KEY` secret.
    #[serde(default)]
    pub require_evidence_hmac: bool,
    /// Explicit tenant-bound authority for connector side effects. This list
    /// is part of the signed policy pack and is checked by integration
    /// adapters immediately before a write, identity, or infrastructure call.
    #[serde(default)]
    pub integration_grants: Vec<crate::integrations::Grant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DomainEvidenceRequirements {
    #[serde(default)]
    pub terraform_plan: bool,
    #[serde(default)]
    pub kubernetes_validation: bool,
    #[serde(default)]
    pub database_migration_review: bool,
    #[serde(default)]
    pub openapi_compatibility: bool,
    #[serde(default)]
    pub protobuf_compatibility: bool,
}

impl Policy {
    /// Enforce the signed-policy grant boundary for an integration action.
    pub fn authorize_integration(
        &self,
        connector: &crate::integrations::ConnectorConfig,
        capability: crate::integrations::Capability,
    ) -> anyhow::Result<()> {
        for grant in &self.integration_grants {
            grant.validate()?;
        }
        crate::integrations::authorize(connector, capability, &self.integration_grants)
    }

    /// Validate the release-control baseline required when enterprise
    /// governance enforcement is enabled. This deliberately checks the
    /// signed, loaded policy at the authorization boundary rather than merely
    /// trusting that a policy identifier was configured.
    pub fn validate_enterprise_release_controls(&self) -> anyhow::Result<()> {
        if self.required_release_approvals < 2 {
            anyhow::bail!("enterprise governance requires at least two distinct release approvers");
        }
        if !self.require_requester_approver_separation {
            anyhow::bail!("enterprise governance requires requester/approver separation");
        }
        if !self.require_approval_hmac {
            anyhow::bail!("enterprise governance requires approval HMAC integrity");
        }
        if !self.require_approval_commit_binding {
            anyhow::bail!("enterprise governance requires approval candidate commit binding");
        }
        if !matches!(self.approval_validity_hours, Some(1..=168)) {
            anyhow::bail!(
                "enterprise governance requires approval_validity_hours between 1 and 168"
            );
        }
        if !self.require_evidence_expiry || !matches!(self.evidence_validity_hours, Some(1..=168)) {
            anyhow::bail!(
                "enterprise governance requires evidence expiry and evidence_validity_hours between 1 and 168"
            );
        }
        if !self.require_evidence_hmac {
            anyhow::bail!("enterprise governance requires evidence HMAC integrity");
        }
        if self.required_evidence.is_empty()
            && !self.domain_evidence.terraform_plan
            && !self.domain_evidence.kubernetes_validation
            && !self.domain_evidence.database_migration_review
            && !self.domain_evidence.openapi_compatibility
            && !self.domain_evidence.protobuf_compatibility
        {
            anyhow::bail!(
                "enterprise governance requires explicit or domain release evidence gates"
            );
        }
        Ok(())
    }

    /// Verify release evidence freshness after its digest and basic schema
    /// have been validated by `evidence::verify_required`.
    pub fn validate_evidence_freshness(
        &self,
        records: &[crate::evidence::EvidenceRecord],
    ) -> anyhow::Result<()> {
        let now = Utc::now();
        if self
            .evidence_validity_hours
            .is_some_and(|hours| hours == 0 || hours > 8760)
        {
            anyhow::bail!("evidence_validity_hours must be between 1 and 8760 when configured");
        }
        for record in records {
            if record.issued_at > now + chrono::Duration::seconds(60) {
                anyhow::bail!("evidence {:?} is issued in the future", record.kind);
            }
            if self.require_evidence_expiry && record.expires_at.is_none() {
                anyhow::bail!("evidence {:?} is missing a required expiry", record.kind);
            }
            if let Some(hours) = self.evidence_validity_hours {
                if now - record.issued_at > chrono::Duration::hours(i64::from(hours)) {
                    anyhow::bail!(
                        "evidence {:?} exceeds the configured freshness window",
                        record.kind
                    );
                }
            }
        }
        Ok(())
    }

    /// Return the union of explicitly required evidence and evidence inferred
    /// from enabled domain gates plus repository file types.
    pub fn required_evidence_for_repo(&self, repo_path: &Path) -> Vec<String> {
        let mut required: std::collections::BTreeSet<String> =
            self.required_evidence.iter().cloned().collect();
        let mut terraform = false;
        let mut kubernetes = false;
        let mut migrations = false;
        let mut openapi = false;
        let mut protobuf = false;
        for entry in ignore::WalkBuilder::new(repo_path)
            .hidden(false)
            .git_ignore(true)
            .build()
            .flatten()
        {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(repo_path)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .to_ascii_lowercase();
            let filename = entry
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            terraform |= filename.ends_with(".tf") || filename == "terraform.lock.hcl";
            kubernetes |= relative.contains("k8s/")
                || relative.contains("kubernetes/")
                || relative.contains("helm/")
                || filename == "chart.yaml"
                || filename == "kustomization.yaml";
            migrations |= relative.contains("migration/")
                || relative.contains("migrations/")
                || (filename.ends_with(".sql")
                    && filename
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_digit()));
            openapi |= filename.starts_with("openapi")
                || filename == "swagger.yaml"
                || filename == "swagger.json";
            protobuf |= filename.ends_with(".proto");
        }
        if self.domain_evidence.terraform_plan && terraform {
            required.insert("terraform_plan".to_string());
        }
        if self.domain_evidence.kubernetes_validation && kubernetes {
            required.insert("kubernetes_validation".to_string());
        }
        if self.domain_evidence.database_migration_review && migrations {
            required.insert("database_migration_review".to_string());
        }
        if self.domain_evidence.openapi_compatibility && openapi {
            required.insert("openapi_compatibility".to_string());
        }
        if self.domain_evidence.protobuf_compatibility && protobuf {
            required.insert("protobuf_compatibility".to_string());
        }
        required.into_iter().collect()
    }
}

/// Independently produced release approval record.
///
/// Store this at `.kaptaind/approvals/<version>.json`. A future identity
/// provider can produce the record; this compact format keeps the release
/// gate useful in disconnected and self-hosted environments today.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReleaseApproval {
    /// Authenticated subject that requested the release. Required when the
    /// policy enables separation of duties.
    #[serde(default)]
    pub requester: Option<String>,
    #[serde(default)]
    pub requested_at: Option<DateTime<Utc>>,
    pub policy_sha256: String,
    pub approvers: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub change_ticket: Option<String>,
    /// Immutable Git object identifier for the candidate release revision.
    #[serde(default)]
    pub commit_sha: Option<String>,
    #[serde(default)]
    pub hmac_sha256: Option<String>,
}

/// Immutable controls used when a release approval request is created.
#[derive(Debug, Clone, Default)]
pub struct ApprovalRequestOptions {
    pub change_ticket: Option<String>,
    pub require_hmac: bool,
    pub require_commit_binding: bool,
    pub approval_validity_hours: Option<u32>,
}

type HmacSha256 = Hmac<Sha256>;

fn approval_hmac(approval: &ReleaseApproval, key: &[u8]) -> anyhow::Result<String> {
    let mut unsigned = approval.clone();
    unsigned.hmac_sha256 = None;
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|error| anyhow::anyhow!("invalid approval HMAC key: {error}"))?;
    mac.update(&serde_json::to_vec(&unsigned)?);
    Ok(crate::util::hex::encode(mac.finalize().into_bytes()))
}

fn sign_approval(approval: &mut ReleaseApproval, required: bool) -> anyhow::Result<()> {
    match std::env::var("KAPTAIND_APPROVAL_HMAC_KEY") {
        Ok(key) if !key.is_empty() => {
            approval.hmac_sha256 = Some(approval_hmac(approval, key.as_bytes())?);
            Ok(())
        }
        _ if required => {
            anyhow::bail!("approval HMAC required but KAPTAIND_APPROVAL_HMAC_KEY is unavailable")
        }
        _ => Ok(()),
    }
}

fn verify_approval_hmac(approval: &ReleaseApproval, required: bool) -> anyhow::Result<()> {
    let key = std::env::var("KAPTAIND_APPROVAL_HMAC_KEY")
        .ok()
        .filter(|key| !key.is_empty());
    verify_approval_hmac_with_key(approval, required, key.as_deref().map(str::as_bytes))
}

/// Verify a release-approval HMAC against an explicitly supplied key. Keeping
/// the cryptographic primitive separate from environment resolution makes the
/// fail-closed behaviour testable without mutating process-global state.
fn verify_approval_hmac_with_key(
    approval: &ReleaseApproval,
    required: bool,
    key: Option<&[u8]>,
) -> anyhow::Result<()> {
    let Some(signature) = approval.hmac_sha256.as_deref() else {
        if required {
            anyhow::bail!("approval HMAC is required but missing");
        }
        return Ok(());
    };
    let key = key.ok_or_else(|| {
        anyhow::anyhow!("approval HMAC is present but KAPTAIND_APPROVAL_HMAC_KEY is unavailable")
    })?;
    let expected = approval_hmac(approval, key)?;
    if signature.len() != expected.len() {
        anyhow::bail!("approval HMAC verification failed");
    }
    if !bool::from(subtle::ConstantTimeEq::ct_eq(
        signature.as_bytes(),
        expected.as_bytes(),
    )) {
        anyhow::bail!("approval HMAC verification failed");
    }
    Ok(())
}

/// Validate a policy identifier before it reaches a filesystem path. Policy
/// identifiers are labels, not paths.
pub fn validate_policy_id(policy_id: &str) -> anyhow::Result<()> {
    let valid = !policy_id.is_empty()
        && policy_id.len() <= 64
        && policy_id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
        });
    if !valid {
        anyhow::bail!("policy_id must be 1-64 lowercase ASCII letters, digits, '_' or '-'");
    }
    Ok(())
}

fn policy_path(repo_path: &Path, policy_id: &str) -> anyhow::Result<PathBuf> {
    validate_policy_id(policy_id)?;
    Ok(repo_path
        .join(".kaptaind")
        .join("policies")
        .join(format!("{policy_id}.json")))
}

fn safe_version_component(version: &str) -> anyhow::Result<()> {
    let valid = !version.is_empty()
        && version.len() <= 128
        && version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        });
    if !valid {
        anyhow::bail!("release version is not a safe managed-path component");
    }
    Ok(())
}

/// Create a requester-bound approval record. The requested version and policy
/// digest are immutable inputs to every later approval.
pub fn request_release_approval(
    repo_path: &Path,
    policy_id: &str,
    version: &str,
    requester: &str,
    options: ApprovalRequestOptions,
) -> anyhow::Result<ReleaseApproval> {
    if requester.trim().is_empty() {
        anyhow::bail!("authenticated release requester is required");
    }
    if options
        .approval_validity_hours
        .is_some_and(|hours| hours == 0 || hours > 8760)
    {
        anyhow::bail!("approval_validity_hours must be between 1 and 8760 when configured");
    }
    let requested_at = Utc::now();
    let mut approval = ReleaseApproval {
        requester: Some(requester.trim().to_string()),
        requested_at: Some(requested_at),
        policy_sha256: policy_sha256(repo_path, policy_id)?,
        approvers: Vec::new(),
        expires_at: options
            .approval_validity_hours
            .map(|hours| requested_at + chrono::Duration::hours(i64::from(hours))),
        change_ticket: options.change_ticket,
        commit_sha: options
            .require_commit_binding
            .then(|| current_commit_sha(repo_path))
            .transpose()?,
        hmac_sha256: None,
    };
    sign_approval(&mut approval, options.require_hmac)?;
    save_release_approval(repo_path, version, &approval)?;
    Ok(approval)
}

/// Record one distinct approver against an existing request. The record is
/// atomically replaced and remains bound to its original requester/policy.
pub fn approve_release(
    repo_path: &Path,
    version: &str,
    approver: &str,
    require_separation: bool,
    require_hmac: bool,
    require_commit_binding: bool,
) -> anyhow::Result<ReleaseApproval> {
    let mut approval = load_release_approval(repo_path, version)?;
    if approver.trim().is_empty() {
        anyhow::bail!("authenticated release approver is required");
    }
    if require_commit_binding && approval.commit_sha.is_none() {
        anyhow::bail!("approval is missing required candidate commit binding");
    }
    if require_separation {
        crate::rbac::enforce_requester_approver_separation(
            approval.requester.as_deref().unwrap_or_default(),
            [approver],
        )?;
    }
    if !approval
        .approvers
        .iter()
        .any(|existing| existing == approver.trim())
    {
        approval.approvers.push(approver.trim().to_string());
        sign_approval(&mut approval, require_hmac)?;
        save_release_approval(repo_path, version, &approval)?;
    }
    Ok(approval)
}

fn approval_path(repo_path: &Path, version: &str) -> anyhow::Result<PathBuf> {
    safe_version_component(version)?;
    Ok(repo_path
        .join(".kaptaind")
        .join("approvals")
        .join(format!("{version}.json")))
}

fn current_commit_sha(repo_path: &Path) -> anyhow::Result<String> {
    let commit = crate::git::repo::Repo::open(repo_path)?.head_commit_hash()?;
    let valid = (commit.len() == 40 || commit.len() == 64)
        && commit
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if !valid {
        anyhow::bail!("repository HEAD is not a supported Git object identifier");
    }
    Ok(commit)
}

fn load_release_approval(repo_path: &Path, version: &str) -> anyhow::Result<ReleaseApproval> {
    let path = approval_path(repo_path, version)?;
    serde_json::from_slice(&std::fs::read(&path)?)
        .with_context(|| format!("invalid release approval record {}", path.display()))
}

fn save_release_approval(
    repo_path: &Path,
    version: &str,
    approval: &ReleaseApproval,
) -> anyhow::Result<()> {
    let path = approval_path(repo_path, version)?;
    std::fs::create_dir_all(path.parent().expect("approval path has parent"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(approval)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

impl Policy {
    pub fn load_or_default(repo_path: &Path, policy_id: &str) -> anyhow::Result<Self> {
        let path = policy_path(repo_path, policy_id)?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read policy file {}", path.display()))?;
            let policy: Policy = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse policy file {}", path.display()))?;
            Ok(policy)
        } else {
            tracing::info!(policy_id = %policy_id, "policy file not found, using default policy");
            Ok(Policy::default())
        }
    }

    /// Load a policy and, when configured, require a detached ASCII-armored
    /// GPG signature at `<policy>.asc`. `gpgv` is used rather than `gpg` so
    /// verification cannot consult a user's ambient trust database.
    pub fn load_with_trust(
        repo_path: &Path,
        policy_id: &str,
        trust: &PolicyTrustConfig,
        keyring: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let path = policy_path(repo_path, policy_id)?;
        if trust.require_signature {
            let signature = path.with_extension("json.asc");
            let keyring = keyring.ok_or_else(|| {
                anyhow::anyhow!("policy signature verification requires policy_trust.gpgv_keyring")
            })?;
            if !signature.exists() {
                anyhow::bail!(
                    "signed policy required, but {} is missing",
                    signature.display()
                );
            }
            let status = std::process::Command::new("gpgv")
                .arg("--keyring")
                .arg(keyring)
                .arg(&signature)
                .arg(&path)
                .status()
                .with_context(|| "failed to execute gpgv for policy verification")?;
            if !status.success() {
                anyhow::bail!("policy signature verification failed for {policy_id:?}");
            }
        }
        Self::load_or_default(repo_path, policy_id)
    }
}

pub fn policy_sha256(repo_path: &Path, policy_id: &str) -> anyhow::Result<String> {
    let path = policy_path(repo_path, policy_id)?;
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read policy file {}", path.display()))?;
    Ok(crate::util::hex::encode(Sha256::digest(bytes)))
}

/// Verify approval evidence for a release. This is deliberately fail-closed:
/// a missing, stale, malformed, policy-mismatched, or under-quorum record
/// cannot authorize a shipment.
pub fn verify_release_approval(
    repo_path: &Path,
    policy_id: &str,
    version: &str,
    required: usize,
    require_separation: bool,
    require_hmac: bool,
    require_commit_binding: bool,
) -> anyhow::Result<Option<ReleaseApproval>> {
    if required == 0 {
        return Ok(None);
    }
    let path = approval_path(repo_path, version)?;
    let approval = load_release_approval(repo_path, version).with_context(|| {
        format!(
            "release approval required by policy {policy_id:?}, but {} is missing",
            path.display()
        )
    })?;
    let expected = policy_sha256(repo_path, policy_id)?;
    if approval.policy_sha256 != expected {
        anyhow::bail!("release approval is bound to a different policy revision");
    }
    verify_approval_hmac(&approval, require_hmac)?;
    if require_commit_binding {
        let expected_commit = current_commit_sha(repo_path)?;
        if approval.commit_sha.as_deref() != Some(expected_commit.as_str()) {
            anyhow::bail!("release approval is not bound to the current candidate commit");
        }
    }
    if approval
        .expires_at
        .is_some_and(|expires_at| expires_at <= Utc::now())
    {
        anyhow::bail!("release approval has expired");
    }
    let distinct: std::collections::BTreeSet<_> = approval
        .approvers
        .iter()
        .map(|approver| approver.trim())
        .filter(|approver| !approver.is_empty())
        .collect();
    if distinct.len() < required {
        anyhow::bail!(
            "release approval quorum not met: policy requires {required} distinct approvers, found {}",
            distinct.len()
        );
    }
    if require_separation {
        crate::rbac::enforce_requester_approver_separation(
            approval.requester.as_deref().unwrap_or_default(),
            &approval.approvers,
        )?;
    }
    Ok(Some(approval))
}

pub fn current_branch(repo_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn is_branch_protected(repo_path: &Path, protected_branches: &[String]) -> bool {
    if protected_branches.is_empty() {
        return false;
    }
    let Some(current) = current_branch(repo_path) else {
        return false;
    };
    protected_branches.iter().any(|b| b == &current)
}

pub fn cluster_matches_allowlist(paths: &[PathBuf], allowlist: &[String]) -> bool {
    if allowlist.is_empty() {
        return true;
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in allowlist {
        // A policy typo must not silently widen the set of files that an
        // unattended daemon may commit.  Fail this cluster closed instead.
        let Ok(glob) = Glob::new(pattern) else {
            tracing::warn!(pattern, "invalid policy allowlist glob; blocking cluster");
            return false;
        };
        builder.add(glob);
    }
    let Ok(globset) = builder.build() else {
        return false;
    };
    // A cluster is committed as one atomic unit.  Permit it only when every
    // touched file is allowed; accepting one matching path would otherwise
    // let a disallowed file ride along in the same automatic commit.
    !paths.is_empty() && paths.iter().all(|path| globset.is_match(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn policy_loads_from_disk() {
        let dir = tempdir().unwrap();
        let policies_dir = dir.path().join(".kaptaind").join("policies");
        std::fs::create_dir_all(&policies_dir).unwrap();
        std::fs::write(
            policies_dir.join("prod.json"),
            r#"{"min_test_coverage":true,"required_signoff":true,"branch_protection":["main"],"file_pattern_allowlist":["**/*.rs"]}"#,
        )
        .unwrap();

        let policy = Policy::load_or_default(dir.path(), "prod").unwrap();
        assert!(policy.min_test_coverage);
        assert!(policy.required_signoff);
        assert_eq!(policy.branch_protection, vec!["main"]);
        assert_eq!(policy.file_pattern_allowlist, vec!["**/*.rs"]);
    }

    #[test]
    fn enterprise_release_policy_requires_quorum_integrity_and_evidence() {
        let mut policy = Policy::default();
        assert!(policy.validate_enterprise_release_controls().is_err());
        policy.required_release_approvals = 2;
        policy.require_requester_approver_separation = true;
        policy.require_approval_hmac = true;
        policy.require_approval_commit_binding = true;
        policy.approval_validity_hours = Some(24);
        policy.require_evidence_expiry = true;
        policy.evidence_validity_hours = Some(24);
        policy.require_evidence_hmac = true;
        policy.required_evidence = vec!["ci_attestation".to_string()];
        assert!(policy.validate_enterprise_release_controls().is_ok());
    }

    #[test]
    fn policy_grants_connector_writes_only_to_the_named_tenant() {
        let connector = crate::integrations::ConnectorConfig {
            provider: crate::integrations::Provider::Hetzner,
            mode: crate::integrations::Mode::GovernedWrite,
            tenant_id: "production".to_string(),
            endpoint: Some("https://93.184.216.34/v1".to_string()),
            credential_ref: Some("vault:hetzner".to_string()),
            capabilities: [crate::integrations::Capability::WriteInfrastructure]
                .into_iter()
                .collect(),
        };
        let mut policy = Policy::default();
        assert!(policy
            .authorize_integration(
                &connector,
                crate::integrations::Capability::WriteInfrastructure
            )
            .is_err());
        policy.integration_grants.push(crate::integrations::Grant {
            provider: crate::integrations::Provider::Hetzner,
            tenant_id: "production".to_string(),
            capabilities: [crate::integrations::Capability::WriteInfrastructure]
                .into_iter()
                .collect(),
        });
        assert!(policy
            .authorize_integration(
                &connector,
                crate::integrations::Capability::WriteInfrastructure
            )
            .is_ok());
    }

    #[test]
    fn release_approval_requires_matching_policy_digest_and_quorum() {
        let dir = tempdir().unwrap();
        let policies_dir = dir.path().join(".kaptaind/policies");
        let approvals_dir = dir.path().join(".kaptaind/approvals");
        std::fs::create_dir_all(&policies_dir).unwrap();
        std::fs::create_dir_all(&approvals_dir).unwrap();
        std::fs::write(policies_dir.join("production.json"), "{}").unwrap();
        let digest = policy_sha256(dir.path(), "production").unwrap();
        let approval = ReleaseApproval {
            requester: Some("release-owner".to_string()),
            requested_at: Some(Utc::now()),
            policy_sha256: digest,
            approvers: vec!["alice".to_string(), "bob".to_string()],
            expires_at: None,
            change_ticket: Some("CHG-123".to_string()),
            commit_sha: None,
            hmac_sha256: None,
        };
        std::fs::write(
            approvals_dir.join("1.2.3.json"),
            serde_json::to_vec(&approval).unwrap(),
        )
        .unwrap();
        assert!(
            verify_release_approval(dir.path(), "production", "1.2.3", 2, true, false, false)
                .is_ok()
        );
        assert!(
            verify_release_approval(dir.path(), "production", "1.2.3", 3, true, false, false)
                .is_err()
        );
        std::fs::write(policies_dir.join("production.json"), "{\"changed\":true}").unwrap();
        assert!(
            verify_release_approval(dir.path(), "production", "1.2.3", 2, true, false, false)
                .is_err()
        );
    }

    #[test]
    fn release_approval_rejects_requester_self_approval_when_required() {
        let dir = tempdir().unwrap();
        let policies_dir = dir.path().join(".kaptaind/policies");
        let approvals_dir = dir.path().join(".kaptaind/approvals");
        std::fs::create_dir_all(&policies_dir).unwrap();
        std::fs::create_dir_all(&approvals_dir).unwrap();
        std::fs::write(policies_dir.join("production.json"), "{}").unwrap();
        let approval = ReleaseApproval {
            requester: Some("alice".to_string()),
            requested_at: Some(Utc::now()),
            policy_sha256: policy_sha256(dir.path(), "production").unwrap(),
            approvers: vec!["alice".to_string(), "bob".to_string()],
            expires_at: None,
            change_ticket: None,
            commit_sha: None,
            hmac_sha256: None,
        };
        std::fs::write(
            approvals_dir.join("1.2.3.json"),
            serde_json::to_vec(&approval).unwrap(),
        )
        .unwrap();
        assert!(
            verify_release_approval(dir.path(), "production", "1.2.3", 2, true, false, false)
                .is_err()
        );
    }

    #[test]
    fn approval_workflow_binds_requester_and_deduplicates_approvers() {
        let dir = tempdir().unwrap();
        let policies_dir = dir.path().join(".kaptaind/policies");
        std::fs::create_dir_all(&policies_dir).unwrap();
        std::fs::write(policies_dir.join("production.json"), "{}").unwrap();
        let requested = request_release_approval(
            dir.path(),
            "production",
            "1.2.3",
            "alice",
            ApprovalRequestOptions {
                change_ticket: Some("CHG-123".to_string()),
                ..ApprovalRequestOptions::default()
            },
        )
        .unwrap();
        assert_eq!(requested.requester.as_deref(), Some("alice"));
        assert!(approve_release(dir.path(), "1.2.3", "alice", true, false, false).is_err());
        let approved = approve_release(dir.path(), "1.2.3", "bob", true, false, false).unwrap();
        assert_eq!(approved.approvers, vec!["bob"]);
        let approved_again =
            approve_release(dir.path(), "1.2.3", "bob", true, false, false).unwrap();
        assert_eq!(approved_again.approvers, vec!["bob"]);
    }

    #[test]
    fn approval_request_applies_a_bounded_policy_lifetime() {
        let dir = tempdir().unwrap();
        let policies_dir = dir.path().join(".kaptaind/policies");
        std::fs::create_dir_all(&policies_dir).unwrap();
        std::fs::write(policies_dir.join("production.json"), "{}").unwrap();
        let requested = request_release_approval(
            dir.path(),
            "production",
            "1.2.3",
            "alice",
            ApprovalRequestOptions {
                approval_validity_hours: Some(24),
                ..ApprovalRequestOptions::default()
            },
        )
        .unwrap();
        let issued = requested.requested_at.unwrap();
        let expires = requested
            .expires_at
            .expect("policy lifetime must set expiry");
        assert_eq!(expires - issued, chrono::Duration::hours(24));
        assert!(request_release_approval(
            dir.path(),
            "production",
            "1.2.4",
            "alice",
            ApprovalRequestOptions {
                approval_validity_hours: Some(0),
                ..ApprovalRequestOptions::default()
            },
        )
        .is_err());
    }

    #[test]
    fn evidence_freshness_requires_expiry_and_rejects_stale_records() {
        let policy = Policy {
            require_evidence_expiry: true,
            evidence_validity_hours: Some(24),
            ..Policy::default()
        };
        let current = crate::evidence::EvidenceRecord {
            schema_version: 1,
            kind: "sarif".to_string(),
            source: "ci".to_string(),
            sha256: "a".repeat(64),
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            hmac_sha256: None,
        };
        assert!(policy
            .validate_evidence_freshness(std::slice::from_ref(&current))
            .is_ok());
        let missing_expiry = crate::evidence::EvidenceRecord {
            expires_at: None,
            ..current.clone()
        };
        assert!(policy
            .validate_evidence_freshness(&[missing_expiry])
            .is_err());
        let stale = crate::evidence::EvidenceRecord {
            issued_at: Utc::now() - chrono::Duration::hours(25),
            ..current
        };
        assert!(policy.validate_evidence_freshness(&[stale]).is_err());
    }

    #[test]
    fn commit_bound_approval_cannot_be_reused_after_head_moves() {
        let dir = tempdir().unwrap();
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "Kaptaind Test"],
        ] {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        }
        std::fs::write(dir.path().join("tracked.txt"), "first\n").unwrap();
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "tracked.txt"])
            .status()
            .unwrap();
        assert!(status.success());
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["commit", "-m", "initial"])
            .status()
            .unwrap();
        assert!(status.success());

        let policies_dir = dir.path().join(".kaptaind/policies");
        std::fs::create_dir_all(&policies_dir).unwrap();
        std::fs::write(policies_dir.join("production.json"), "{}").unwrap();
        request_release_approval(
            dir.path(),
            "production",
            "1.2.3",
            "alice",
            ApprovalRequestOptions {
                require_commit_binding: true,
                ..ApprovalRequestOptions::default()
            },
        )
        .unwrap();
        approve_release(dir.path(), "1.2.3", "bob", true, false, true).unwrap();
        assert!(
            verify_release_approval(dir.path(), "production", "1.2.3", 1, true, false, true,)
                .is_ok()
        );

        std::fs::write(dir.path().join("tracked.txt"), "second\n").unwrap();
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "tracked.txt"])
            .status()
            .unwrap();
        assert!(status.success());
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["commit", "-m", "candidate changed"])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(
            verify_release_approval(dir.path(), "production", "1.2.3", 1, true, false, true,)
                .is_err()
        );
    }

    #[test]
    fn approval_hmac_detects_tampering_and_requires_its_verification_key() {
        let mut approval = ReleaseApproval {
            requester: Some("alice".to_string()),
            requested_at: Some(Utc::now()),
            policy_sha256: "a".repeat(64),
            approvers: vec!["bob".to_string()],
            expires_at: None,
            change_ticket: Some("CHG-123".to_string()),
            commit_sha: None,
            hmac_sha256: None,
        };
        let key = b"test-only-approval-integrity-key";
        approval.hmac_sha256 = Some(approval_hmac(&approval, key).unwrap());

        assert!(verify_approval_hmac_with_key(&approval, true, Some(key)).is_ok());
        assert!(verify_approval_hmac_with_key(&approval, true, None).is_err());

        approval.approvers.push("mallory".to_string());
        assert!(verify_approval_hmac_with_key(&approval, true, Some(key)).is_err());
    }

    #[test]
    fn domain_evidence_only_applies_to_matching_repository_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("infra.tf"), "terraform {}\n").unwrap();
        std::fs::create_dir_all(dir.path().join("migrations")).unwrap();
        std::fs::write(dir.path().join("migrations/001_init.sql"), "select 1;\n").unwrap();
        let policy = Policy {
            domain_evidence: DomainEvidenceRequirements {
                terraform_plan: true,
                database_migration_review: true,
                kubernetes_validation: true,
                ..DomainEvidenceRequirements::default()
            },
            ..Policy::default()
        };
        assert_eq!(
            policy.required_evidence_for_repo(dir.path()),
            vec!["database_migration_review", "terraform_plan"]
        );
    }

    #[test]
    fn managed_policy_and_approval_paths_reject_traversal() {
        let dir = tempdir().unwrap();
        assert!(validate_policy_id("../production").is_err());
        assert!(Policy::load_or_default(dir.path(), "../production").is_err());
        assert!(request_release_approval(
            dir.path(),
            "production",
            "../../outside",
            "alice",
            ApprovalRequestOptions::default(),
        )
        .is_err());
    }

    #[test]
    fn policy_defaults_when_file_missing() {
        let dir = tempdir().unwrap();
        let policy = Policy::load_or_default(dir.path(), "missing").unwrap();
        assert!(!policy.min_test_coverage);
        assert!(!policy.required_signoff);
        assert!(policy.branch_protection.is_empty());
        assert!(policy.file_pattern_allowlist.is_empty());
    }

    #[test]
    fn signed_policy_requirement_fails_closed_before_gpg_is_invoked() {
        let dir = tempdir().unwrap();
        let trust = PolicyTrustConfig {
            require_signature: true,
            gpgv_keyring: Some(PathBuf::from("trustedkeys.gpg")),
        };
        let keyring = dir.path().join("trustedkeys.gpg");
        let error = Policy::load_with_trust(dir.path(), "production", &trust, Some(&keyring))
            .expect_err("missing signature must block policy load");
        assert!(error.to_string().contains("signed policy required"));
    }

    #[test]
    fn branch_protection_detects_protected_branch() {
        let dir = tempdir().unwrap();
        // init git repo with an initial commit so HEAD resolves to a real branch
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["init"])
            .output();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["config", "user.email", "test@test.com"])
            .output();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["config", "user.name", "Test"])
            .output();
        std::fs::write(dir.path().join("file.txt"), "x").unwrap();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["commit", "-m", "init"])
            .output();

        assert!(is_branch_protected(
            dir.path(),
            &["main".to_string(), "master".to_string()]
        ));
        assert!(!is_branch_protected(dir.path(), &["develop".to_string()]));
    }

    #[test]
    fn allowlist_matches_paths() {
        let paths = vec![PathBuf::from("src/main.rs"), PathBuf::from("README.md")];
        assert!(!cluster_matches_allowlist(&paths, &["**/*.rs".to_string()]));
        assert!(!cluster_matches_allowlist(&paths, &["**/*.go".to_string()]));
        assert!(cluster_matches_allowlist(&paths, &[]));
    }

    #[test]
    fn allowlist_requires_every_path_in_an_automatic_commit_to_match() {
        let paths = vec![PathBuf::from("src/main.rs"), PathBuf::from("src/lib.rs")];
        assert!(cluster_matches_allowlist(
            &paths,
            &["src/**/*.rs".to_string()]
        ));
    }

    #[test]
    fn invalid_allowlist_is_fail_closed() {
        let paths = vec![PathBuf::from("src/main.rs")];
        assert!(!cluster_matches_allowlist(
            &paths,
            &["[not-a-glob".to_string()]
        ));
    }

    #[test]
    fn audit_log_chain_detects_tampering() {
        let dir = tempdir().unwrap();
        crate::audit::append(
            dir.path(),
            &crate::audit::AuditEntry::new("commit_blocked", "daemon", "blocked")
                .with_details(serde_json::json!({"reason": "coverage"})),
        )
        .unwrap();
        let content = std::fs::read_to_string(dir.path().join(".kaptaind/audit.jsonl")).unwrap();
        assert!(content.contains("commit_blocked"));
        assert!(content.contains("coverage"));
        assert!(crate::audit::verify_chain(dir.path()).is_ok());
        std::fs::write(dir.path().join(".kaptaind/audit.jsonl"), "{}\n").unwrap();
        assert!(crate::audit::verify_chain(dir.path()).is_err());
    }
}
