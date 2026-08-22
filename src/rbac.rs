//! Startup / CLI capability gating for kaptaind.
//!
//! RBAC here is a *process-local* gate: it checks whether the local OS user is
//! permitted to invoke a kaptaind command (start/stop the daemon, run a
//! release, etc.). It is **not** an HTTP request-authorization layer and does
//! not protect the WebUI or any network endpoint. The WebUI has its own
//! bearer-token authentication (see `crate::daemon::web`); do not assume RBAC
//! covers remote callers.

use crate::config::loader::{IdentityConfig, IdentityMode, RbacConfig};
use anyhow::{anyhow, bail};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
#[cfg(unix)]
use std::ffi::CStr;

/// Locally authenticated actor identity.
///
/// This deliberately represents the account which owns the running process,
/// rather than a caller-controlled display name.  It is suitable for binding
/// local approvals and audit events in disconnected deployments.  Remote
/// callers must be authenticated by an identity provider before they are
/// allowed to create approval evidence; an environment variable is not an
/// identity provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedActor {
    /// Stable local subject, currently the operating-system account name.
    pub subject: String,
    /// How the subject was authenticated. Kept explicit so an audit record
    /// cannot imply a stronger identity assurance level than was available.
    pub source: ActorSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorSource {
    /// Resolved from the effective OS user of the running process.
    OperatingSystem,
    /// Detached GPG signature verified against an operator-controlled keyring.
    GpgSignedAssertion,
}

impl ActorSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OperatingSystem => "operating_system",
            Self::GpgSignedAssertion => "gpg_signed_assertion",
        }
    }
}

impl AuthenticatedActor {
    /// Resolve the effective OS identity for this process.
    ///
    /// Unlike `USER`, which is inherited mutable process state, this uses the
    /// effective Unix UID where available. It therefore fails closed rather
    /// than accepting an arbitrary environment-provided approver identity.
    pub fn current() -> anyhow::Result<Self> {
        let subject = os_user_name()
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| anyhow!("unable to resolve authenticated OS actor"))?;
        Ok(Self {
            subject,
            source: ActorSource::OperatingSystem,
        })
    }

    /// Resolve the configured actor evidence. A signed assertion is useful
    /// when an upstream IdP or CI broker establishes a subject outside the
    /// local host; it is verified with `gpgv`, never with ambient GPG trust.
    pub fn from_identity_config(config: &IdentityConfig) -> anyhow::Result<Self> {
        match config.mode {
            IdentityMode::OperatingSystem => Self::current(),
            IdentityMode::GpgSignedAssertion => signed_assertion_actor(config),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SignedActorAssertion {
    jti: String,
    subject: String,
    issuer: String,
    audience: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

fn signed_assertion_actor(config: &IdentityConfig) -> anyhow::Result<AuthenticatedActor> {
    let keyring = config
        .gpgv_keyring
        .as_ref()
        .ok_or_else(|| anyhow!("gpg_signed_assertion identity requires identity.gpgv_keyring"))?;
    let assertion_path = config
        .assertion_path
        .as_ref()
        .ok_or_else(|| anyhow!("gpg_signed_assertion identity requires identity.assertion_path"))?;
    let signature_path = assertion_path.with_extension("json.asc");
    if !signature_path.exists() {
        bail!(
            "signed identity assertion is missing detached signature {}",
            signature_path.display()
        );
    }
    let status = std::process::Command::new("gpgv")
        .arg("--keyring")
        .arg(keyring)
        .arg(&signature_path)
        .arg(assertion_path)
        .status()
        .map_err(|error| anyhow!("failed to execute gpgv for actor assertion: {error}"))?;
    if !status.success() {
        bail!("actor assertion signature verification failed");
    }
    let assertion: SignedActorAssertion = serde_json::from_slice(&std::fs::read(assertion_path)?)?;
    validate_signed_assertion(&assertion, config, Utc::now())?;
    consume_assertion_id(config, &assertion.jti, assertion.expires_at)?;
    Ok(AuthenticatedActor {
        subject: assertion.subject,
        source: ActorSource::GpgSignedAssertion,
    })
}

fn validate_signed_assertion(
    assertion: &SignedActorAssertion,
    config: &IdentityConfig,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let subject_valid = !assertion.subject.trim().is_empty()
        && assertion.subject.len() <= 256
        && assertion
            .subject
            .chars()
            .all(|character| !character.is_control());
    if !subject_valid {
        bail!("actor assertion subject is empty, oversized, or contains control characters");
    }
    if assertion.jti.is_empty()
        || assertion.jti.len() > 256
        || assertion.jti.chars().any(char::is_control)
    {
        bail!("actor assertion jti is empty, oversized, or contains control characters");
    }
    if config.issuer.as_deref() != Some(assertion.issuer.as_str())
        || config.audience.as_deref() != Some(assertion.audience.as_str())
    {
        bail!("actor assertion issuer or audience does not match configured identity trust");
    }
    if assertion.issued_at > now + Duration::seconds(60) || assertion.expires_at <= now {
        bail!("actor assertion is not currently valid");
    }
    let lifetime = assertion.expires_at - assertion.issued_at;
    if lifetime <= Duration::zero()
        || lifetime > Duration::seconds(config.max_assertion_age_seconds as i64)
    {
        bail!("actor assertion lifetime exceeds configured identity maximum");
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct ConsumedAssertion {
    expires_at: DateTime<Utc>,
}

fn consume_assertion_id(
    config: &IdentityConfig,
    jti: &str,
    expires_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let replay_dir = &config.replay_dir;
    std::fs::create_dir_all(replay_dir)?;
    for entry in std::fs::read_dir(replay_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let expired = std::fs::read(entry.path())
            // traci: allow -- optional failure is represented by None and handled by the caller.
            .ok()
            // traci: allow -- optional failure is represented by None and handled by the caller.
            .and_then(|bytes| serde_json::from_slice::<ConsumedAssertion>(&bytes).ok())
            .is_some_and(|record| record.expires_at <= Utc::now());
        if expired {
            if let Err(error) = std::fs::remove_file(entry.path()) {
                tracing::warn!(
                    ?error,
                    operation = "consume_assertion_id",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
        }
    }
    let filename = crate::util::hex::encode(sha2::Sha256::digest(jti.as_bytes()));
    let path = replay_dir.join(filename);
    let record = ConsumedAssertion { expires_at };
    let serialized = serde_json::to_vec(&record)?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(&serialized)?;
            file.sync_all()?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            tracing::error!(
                ?error,
                operation = "consume_assertion_id",
                source_line = line!(),
                "consume assertion id returned an error"
            );
            bail!("actor assertion has already been consumed")
        }
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "consume_assertion_id",
                source_line = line!(),
                "consume assertion id returned an error"
            );
            Err(error.into())
        }
    }
}

/// Reject approval evidence where the person requesting a release also
/// appears in its approver set.
///
/// Identity comparison is exact after trimming whitespace: callers should
/// pass canonical subjects from their identity provider (or
/// [`AuthenticatedActor::current`]), not human display names. Empty subjects
/// are rejected, because accepting them would silently defeat separation of
/// duties.
pub fn enforce_requester_approver_separation(
    requester: &str,
    approvers: impl IntoIterator<Item = impl AsRef<str>>,
) -> anyhow::Result<()> {
    let requester = requester.trim();
    if requester.is_empty() {
        bail!("release requester identity is required when separation of duties is enabled");
    }

    for approver in approvers {
        let approver = approver.as_ref().trim();
        if approver.is_empty() {
            bail!("release approval contains an empty approver identity");
        }
        if approver == requester {
            bail!(
                "release requester '{requester}' cannot approve their own release when separation of duties is enabled"
            );
        }
    }
    Ok(())
}

/// Permissions enforced by kaptaind's fine-grained RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Start the daemon.
    DaemonStart,
    /// Stop the daemon.
    DaemonStop,
    /// Run a manual ship release.
    ShipRun,
    /// Approve a protected release requested by another actor.
    ShipApprove,
    /// Enable or perform automatic ship releases.
    ShipAuto,
    /// Force-push changes.
    PushForce,
    /// Release Shark Stating leadership.
    SharkRelease,
    /// Perform a Shark Stating zero-downtime upgrade.
    SharkUpgrade,
    /// Modify kaptaind configuration (e.g. `init`, `trawl`).
    ConfigEdit,
}

impl Permission {
    /// Canonical dotted permission string used in configuration files.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Permission::DaemonStart => "daemon.start",
            Permission::DaemonStop => "daemon.stop",
            Permission::ShipRun => "ship.run",
            Permission::ShipApprove => "ship.approve",
            Permission::ShipAuto => "ship.auto",
            Permission::PushForce => "push.force",
            Permission::SharkRelease => "shark.release",
            Permission::SharkUpgrade => "shark.upgrade",
            Permission::ConfigEdit => "config.edit",
        }
    }
}

impl AsRef<str> for Permission {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Return the name of the OS user running this process.
///
/// This compatibility helper preserves the historical non-fallible RBAC API.
/// New approval and audit code should use [`AuthenticatedActor::current`] and
/// handle identity-resolution failure explicitly.
pub fn current_user() -> String {
    AuthenticatedActor::current()
        .map(|actor| actor.subject)
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(unix)]
fn os_user_name() -> Option<String> {
    // SAFETY: getpwuid returns a process-owned libc record. We copy its name
    // immediately and make no attempt to retain the pointer.
    unsafe {
        let passwd = libc::getpwuid(libc::geteuid());
        if passwd.is_null() || (*passwd).pw_name.is_null() {
            return None;
        }
        CStr::from_ptr((*passwd).pw_name)
            .to_str()
            // traci: allow -- optional failure is represented by None and handled by the caller.
            .ok()
            .map(str::to_owned)
    }
}

#[cfg(not(unix))]
fn os_user_name() -> Option<String> {
    // Windows does not expose a portable stdlib account lookup. This is only
    // a compatibility fallback; enterprise Windows deployments should bind a
    // remote, authenticated identity before creating approval evidence.
    // traci: allow -- optional failure is represented by None and handled by the caller.
    std::env::var("USERNAME").ok()
}

/// Return the supplementary group names of the OS user running this process.
///
/// Uses `libc::getgroups` and `libc::getgrgid` to resolve numeric group IDs to
/// names. Any resolution errors result in an empty vector rather than a failure.
#[cfg(unix)]
pub fn current_groups() -> Vec<String> {
    unsafe {
        let mut count = libc::getgroups(0, std::ptr::null_mut());
        if count <= 0 {
            return Vec::new();
        }

        let mut groups = vec![0 as libc::gid_t; count as usize];
        count = libc::getgroups(count, groups.as_mut_ptr());
        if count <= 0 {
            return Vec::new();
        }
        groups.truncate(count as usize);

        groups
            .into_iter()
            .filter_map(|gid| group_name_from_gid(gid))
            .collect()
    }
}

/// Non-Unix platforms have no getgroups(2): report no supplementary groups,
/// so group-based RBAC rules simply do not match.
#[cfg(not(unix))]
pub fn current_groups() -> Vec<String> {
    Vec::new()
}

#[cfg(unix)]
unsafe fn group_name_from_gid(gid: libc::gid_t) -> Option<String> {
    let gr = libc::getgrgid(gid);
    if gr.is_null() {
        return None;
    }

    let name_ptr = (*gr).gr_name;
    if name_ptr.is_null() {
        return None;
    }

    CStr::from_ptr(name_ptr)
        .to_str()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        .map(|s| s.to_string())
}

/// Check whether the current OS user is allowed to perform `permission` under
/// the supplied RBAC configuration.
///
/// If RBAC is disabled the check always succeeds. Otherwise, the current user
/// must belong to at least one role that either lists the requested permission
/// explicitly or grants all permissions via `"*"`.
pub fn check_permission(config: &RbacConfig, permission: &str) -> anyhow::Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let user = current_user();
    let groups = current_groups();
    check_permission_for_identity(config, permission, &user, &groups)
}

/// Check a named, already-authenticated subject against user-bound RBAC roles.
///
/// This is for an identity verified by an external trust boundary, such as a
/// signed short-lived assertion. Unlike [`check_permission`], it deliberately
/// does not consult local supplementary groups: groups belong to the local
/// operating-system account and must never be implicitly attributed to a
/// remote subject. Enterprise release operations use this in addition to the
/// local process gate, so both the invoking host account and the asserted
/// identity need the appropriate permission.
pub fn check_permission_for_subject(
    config: &RbacConfig,
    permission: &str,
    subject: &str,
) -> anyhow::Result<()> {
    if !config.enabled {
        return Ok(());
    }
    let subject = subject.trim();
    if subject.is_empty() {
        bail!("RBAC denied: authenticated subject is empty");
    }
    check_permission_for_identity(config, permission, subject, &[])
}

fn check_permission_for_identity(
    config: &RbacConfig,
    permission: &str,
    user: &str,
    groups: &[String],
) -> anyhow::Result<()> {
    let mut user_matched_roles: Vec<&str> = Vec::new();
    for role in &config.roles {
        let user_matches = role.users.iter().any(|u| u == user);
        let group_matches = role.groups.iter().any(|g| groups.iter().any(|cg| cg == g));
        if user_matches || group_matches {
            user_matched_roles.push(&role.name);
            if role.permissions.iter().any(|p| p == "*" || p == permission) {
                return Ok(());
            }
        }
    }

    let allowed_roles: Vec<&str> = config
        .roles
        .iter()
        .filter(|role| role.permissions.iter().any(|p| p == "*" || p == permission))
        .map(|role| role.name.as_str())
        .collect();

    if user_matched_roles.is_empty() {
        bail!(
            "RBAC denied: permission '{}' required. No matching role found for user '{}' (groups: [{}]). Allowed roles: [{}]",
            permission,
            user,
            groups.join(", "),
            allowed_roles.join(", ")
        );
    }

    bail!(
        "RBAC denied: permission '{}' required. User '{}' is in roles [{}] but none grant this permission. Allowed roles: [{}]",
        permission,
        user,
        user_matched_roles.join(", "),
        allowed_roles.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::RbacRoleConfig;

    fn signed_identity_config() -> IdentityConfig {
        IdentityConfig {
            mode: IdentityMode::GpgSignedAssertion,
            gpgv_keyring: None,
            assertion_path: None,
            replay_dir: std::path::PathBuf::from(".kaptaind/identity/replay"),
            issuer: Some("https://id.example".to_string()),
            audience: Some("kaptaind".to_string()),
            max_assertion_age_seconds: 900,
        }
    }

    fn sample_config() -> RbacConfig {
        RbacConfig {
            enabled: true,
            roles: vec![
                RbacRoleConfig {
                    name: "admin".to_string(),
                    permissions: vec!["*".to_string()],
                    users: vec!["root".to_string()],
                    groups: vec![],
                },
                RbacRoleConfig {
                    name: "shipper".to_string(),
                    permissions: vec!["ship.run".to_string(), "ship.auto".to_string()],
                    users: vec!["alice".to_string()],
                    groups: vec!["release".to_string()],
                },
                RbacRoleConfig {
                    name: "operator".to_string(),
                    permissions: vec!["daemon.start".to_string(), "daemon.stop".to_string()],
                    users: vec![],
                    groups: vec!["ops".to_string()],
                },
                RbacRoleConfig {
                    name: "configurator".to_string(),
                    permissions: vec!["config.edit".to_string()],
                    users: vec![],
                    groups: vec!["dev".to_string()],
                },
            ],
        }
    }

    #[test]
    fn disabled_rbac_allows_everything() {
        let config = RbacConfig {
            enabled: false,
            roles: vec![],
        };
        assert!(check_permission(&config, "ship.run").is_ok());
        assert!(check_permission(&config, "daemon.start").is_ok());
    }

    #[test]
    fn user_role_match_grants_permission() {
        let mut config = sample_config();
        let user = current_user();
        config.roles[1].users.push(user);

        assert!(check_permission(&config, "ship.run").is_ok());
        assert!(check_permission(&config, "ship.auto").is_ok());
    }

    #[test]
    fn user_role_match_without_permission_is_denied() {
        let mut config = sample_config();
        let user = current_user();
        config.roles[1].users.push(user);

        let err = check_permission(&config, "daemon.start").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("daemon.start"));
        assert!(msg.contains("operator")); // allowed roles for daemon.start
    }

    #[test]
    fn group_role_match_grants_permission() {
        let groups = current_groups();
        if groups.is_empty() {
            // Cannot test group matching for a user with no supplementary groups.
            return;
        }

        let mut config = sample_config();
        config.roles[2].groups.push(groups[0].clone());
        assert!(check_permission(&config, "daemon.stop").is_ok());
    }

    #[test]
    fn wildcard_permission_grants_all() {
        let mut config = sample_config();
        let user = current_user();
        config.roles[0].users.push(user);

        assert!(check_permission(&config, "ship.run").is_ok());
        assert!(check_permission(&config, "shark.upgrade").is_ok());
        assert!(check_permission(&config, "config.edit").is_ok());
    }

    #[test]
    fn asserted_subject_must_be_independently_authorized() {
        let config = RbacConfig {
            enabled: true,
            roles: vec![RbacRoleConfig {
                name: "release-approvers".to_string(),
                permissions: vec!["ship.approve".to_string()],
                users: vec!["idp:approver-42".to_string()],
                groups: vec!["local-ops".to_string()],
            }],
        };
        assert!(check_permission_for_subject(&config, "ship.approve", "idp:approver-42").is_ok());
        let error = check_permission_for_subject(&config, "ship.approve", "idp:requester-7")
            .expect_err("an asserted subject without a role must be denied");
        assert!(error.to_string().contains("No matching role"));
        assert!(check_permission_for_subject(&config, "ship.approve", " ").is_err());
    }

    #[test]
    fn permission_enum_strings_are_stable() {
        assert_eq!(Permission::DaemonStart.as_str(), "daemon.start");
        assert_eq!(Permission::DaemonStop.as_str(), "daemon.stop");
        assert_eq!(Permission::ShipRun.as_str(), "ship.run");
        assert_eq!(Permission::ShipApprove.as_str(), "ship.approve");
        assert_eq!(Permission::ShipAuto.as_str(), "ship.auto");
        assert_eq!(Permission::PushForce.as_str(), "push.force");
        assert_eq!(Permission::SharkRelease.as_str(), "shark.release");
        assert_eq!(Permission::SharkUpgrade.as_str(), "shark.upgrade");
        assert_eq!(Permission::ConfigEdit.as_str(), "config.edit");
    }

    #[test]
    fn authenticated_actor_uses_the_effective_os_identity() {
        let actor = AuthenticatedActor::current().expect("test process has an OS identity");
        assert!(!actor.subject.trim().is_empty());
        assert_eq!(actor.source, ActorSource::OperatingSystem);
        assert_eq!(current_user(), actor.subject);
        assert_eq!(actor.source.as_str(), "operating_system");
    }

    #[test]
    fn signed_actor_assertion_requires_matching_issuer_audience_and_lifetime() {
        let config = signed_identity_config();
        let now = Utc::now();
        let assertion = SignedActorAssertion {
            jti: "assertion-1".to_string(),
            subject: "user:alice".to_string(),
            issuer: "https://id.example".to_string(),
            audience: "kaptaind".to_string(),
            issued_at: now - Duration::seconds(10),
            expires_at: now + Duration::seconds(600),
        };
        assert!(validate_signed_assertion(&assertion, &config, now).is_ok());

        let mut wrong_audience = assertion;
        wrong_audience.audience = "other-service".to_string();
        assert!(validate_signed_assertion(&wrong_audience, &config, now).is_err());

        let expired = SignedActorAssertion {
            jti: "assertion-2".to_string(),
            subject: "user:bob".to_string(),
            issuer: "https://id.example".to_string(),
            audience: "kaptaind".to_string(),
            issued_at: now - Duration::seconds(1000),
            expires_at: now - Duration::seconds(1),
        };
        assert!(validate_signed_assertion(&expired, &config, now).is_err());
    }

    #[test]
    fn signed_actor_resolution_fails_closed_without_a_detached_signature() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("actor.json");
        std::fs::write(&assertion_path, "{}").unwrap();
        let config = IdentityConfig {
            gpgv_keyring: Some(dir.path().join("identity-keys.gpg")),
            assertion_path: Some(assertion_path),
            ..signed_identity_config()
        };
        let error = AuthenticatedActor::from_identity_config(&config).unwrap_err();
        assert!(error.to_string().contains("missing detached signature"));
    }

    #[test]
    fn signed_assertion_id_is_consumed_once_and_expired_ledger_entries_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let config = IdentityConfig {
            replay_dir: dir.path().to_path_buf(),
            ..signed_identity_config()
        };
        let expiry = Utc::now() + Duration::minutes(5);
        consume_assertion_id(&config, "assertion-1", expiry).unwrap();
        assert!(consume_assertion_id(&config, "assertion-1", expiry).is_err());

        let stale = ConsumedAssertion {
            expires_at: Utc::now() - Duration::minutes(1),
        };
        std::fs::write(
            dir.path().join("stale"),
            serde_json::to_vec(&stale).unwrap(),
        )
        .unwrap();
        consume_assertion_id(&config, "assertion-2", expiry).unwrap();
        assert!(!dir.path().join("stale").exists());
    }

    #[test]
    fn separation_of_duties_accepts_independent_approvers() {
        let approvers = ["bob", "carol"];
        assert!(enforce_requester_approver_separation("alice", approvers).is_ok());
    }

    #[test]
    fn separation_of_duties_rejects_self_approval_after_whitespace_normalization() {
        let error = enforce_requester_approver_separation(" alice ", ["bob", "alice "])
            .expect_err("requester must not approve their own release");
        assert!(error
            .to_string()
            .contains("cannot approve their own release"));
    }

    #[test]
    fn separation_of_duties_rejects_missing_identities() {
        assert!(enforce_requester_approver_separation("", ["bob"]).is_err());
        assert!(enforce_requester_approver_separation("alice", [""]).is_err());
    }
}
