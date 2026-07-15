//! Startup / CLI capability gating for kaptaind.
//!
//! RBAC here is a *process-local* gate: it checks whether the local OS user is
//! permitted to invoke a kaptaind command (start/stop the daemon, run a
//! release, etc.). It is **not** an HTTP request-authorization layer and does
//! not protect the WebUI or any network endpoint. The WebUI has its own
//! bearer-token authentication (see `crate::daemon::web`); do not assume RBAC
//! covers remote callers.

use crate::config::loader::RbacConfig;
use anyhow::bail;
#[cfg(unix)]
use std::ffi::CStr;

/// Permissions enforced by kaptaind's fine-grained RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Start the daemon.
    DaemonStart,
    /// Stop the daemon.
    DaemonStop,
    /// Run a manual ship release.
    ShipRun,
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
/// Falls back to the `USER` environment variable and finally `"unknown"` if
/// neither source is available.
pub fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
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

    let mut user_matched_roles: Vec<&str> = Vec::new();
    for role in &config.roles {
        let user_matches = role.users.iter().any(|u| u == &user);
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
    fn permission_enum_strings_are_stable() {
        assert_eq!(Permission::DaemonStart.as_str(), "daemon.start");
        assert_eq!(Permission::DaemonStop.as_str(), "daemon.stop");
        assert_eq!(Permission::ShipRun.as_str(), "ship.run");
        assert_eq!(Permission::ShipAuto.as_str(), "ship.auto");
        assert_eq!(Permission::PushForce.as_str(), "push.force");
        assert_eq!(Permission::SharkRelease.as_str(), "shark.release");
        assert_eq!(Permission::SharkUpgrade.as_str(), "shark.upgrade");
        assert_eq!(Permission::ConfigEdit.as_str(), "config.edit");
    }
}
