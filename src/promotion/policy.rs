//! Declarative branch-role and transition policy (ELCI KAPTAIND-RTL-001 §5, §23).
//!
//! Configuration lives under `[lifecycle]` in `kaptaind.toml`. Branch names
//! never carry lifecycle meaning on their own: a role or a glob pattern
//! always names the mapping explicitly, so `kaptaind lifecycle` never
//! requires branches called `dev`, `staging`, or `main`. Zero configuration
//! falls back to `development`/`staging`/`main` plus a `hotfix/*` pattern so
//! the command works out of the box.

use super::model::Operation;
use anyhow::{Context, Result};
use globset::Glob;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct BranchPolicy {
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    pub role: String,
    #[serde(default)]
    pub protected: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransitionPolicy {
    pub from: String,
    pub to: String,
    #[serde(default = "default_operation")]
    pub operation: Operation,
    #[serde(default)]
    pub requires: Vec<String>,
}

fn default_operation() -> Operation {
    Operation::Merge
}

/// Optional outbound event feed (ELCI KAPTAIND-RTL-001 §18, §19): a Vamos or
/// Zebra endpoint that receives the same JSON events written locally to
/// `.kaptaind/lifecycle-events.jsonl`. Absent by default — Kaptaind's
/// lifecycle engine is fully functional with no feed configured at all.
#[derive(Debug, Clone, Default, Deserialize)]
struct FeedPolicy {
    #[serde(default)]
    webhook_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LifecycleTable {
    #[serde(default)]
    branch: BTreeMap<String, BranchPolicy>,
    #[serde(default)]
    transitions: BTreeMap<String, TransitionPolicy>,
    #[serde(default)]
    feed: FeedPolicy,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PolicyFile {
    #[serde(default)]
    lifecycle: LifecycleTable,
}

#[derive(Debug, Clone)]
pub struct LifecyclePolicy {
    pub branches: Vec<BranchPolicy>,
    pub transitions: Vec<TransitionPolicy>,
    pub feed_webhook_url: Option<String>,
}

impl LifecyclePolicy {
    pub fn load(repo_path: &Path) -> Result<Self> {
        let path = repo_path.join("kaptaind.toml");
        let file: PolicyFile = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            toml::from_str(&content)
                .with_context(|| format!("malformed [lifecycle] policy in {}", path.display()))?
        } else {
            PolicyFile::default()
        };
        let mut policy =
            if file.lifecycle.branch.is_empty() && file.lifecycle.transitions.is_empty() {
                Self::default_policy()
            } else {
                Self {
                    // The TOML table key is the branch name unless a literal
                    // `branch = "..."` overrides it (needed when the branch name
                    // itself is not a valid TOML key, e.g. contains a `/`).
                    branches: file
                        .lifecycle
                        .branch
                        .into_iter()
                        .map(|(key, mut policy)| {
                            if policy.branch.is_none() && policy.pattern.is_none() {
                                policy.branch = Some(key);
                            }
                            policy
                        })
                        .collect(),
                    transitions: file.lifecycle.transitions.into_values().collect(),
                    feed_webhook_url: None,
                }
            };
        policy.feed_webhook_url = file.lifecycle.feed.webhook_url;
        Ok(policy)
    }

    fn default_policy() -> Self {
        Self {
            branches: vec![
                BranchPolicy {
                    branch: Some("development".into()),
                    pattern: None,
                    role: "development".into(),
                    protected: false,
                },
                BranchPolicy {
                    branch: Some("staging".into()),
                    pattern: None,
                    role: "staging".into(),
                    protected: false,
                },
                BranchPolicy {
                    branch: Some("main".into()),
                    pattern: None,
                    role: "production".into(),
                    protected: true,
                },
                BranchPolicy {
                    branch: None,
                    pattern: Some("hotfix/*".into()),
                    role: "hotfix".into(),
                    protected: false,
                },
            ],
            transitions: vec![
                TransitionPolicy {
                    from: "development".into(),
                    to: "staging".into(),
                    operation: Operation::Merge,
                    requires: vec!["clean_source".into(), "validation".into()],
                },
                TransitionPolicy {
                    from: "staging".into(),
                    to: "production".into(),
                    operation: Operation::Merge,
                    requires: vec![
                        "clean_source".into(),
                        "clean_target".into(),
                        "validation".into(),
                        "approval".into(),
                    ],
                },
                TransitionPolicy {
                    from: "hotfix".into(),
                    to: "production".into(),
                    operation: Operation::Merge,
                    requires: vec!["validation".into(), "approval".into()],
                },
            ],
            feed_webhook_url: None,
        }
    }

    /// Resolve a role to its single configured branch. Pattern-only roles
    /// (e.g. `hotfix/*`) have no fixed branch and resolve to `None`; callers
    /// must address them by literal branch name instead.
    pub fn branch_for_role(&self, role: &str) -> Option<&str> {
        self.branches
            .iter()
            .find(|policy| policy.role == role)
            .and_then(|policy| policy.branch.as_deref())
    }

    /// Resolve a literal branch name to its configured role: exact names
    /// first, then glob patterns. An unmapped branch resolves to
    /// `"unmanaged"` so unrelated branches never silently gain lifecycle
    /// authority.
    pub fn role_for_branch(&self, branch: &str) -> String {
        for policy in &self.branches {
            if policy.branch.as_deref() == Some(branch) {
                return policy.role.clone();
            }
        }
        for policy in &self.branches {
            if let Some(pattern) = &policy.pattern {
                if Glob::new(pattern)
                    .map(|glob| glob.compile_matcher().is_match(branch))
                    .unwrap_or(false)
                {
                    return policy.role.clone();
                }
            }
        }
        "unmanaged".to_string()
    }

    pub fn is_protected(&self, branch: &str) -> bool {
        let role = self.role_for_branch(branch);
        self.branches
            .iter()
            .any(|policy| policy.role == role && policy.protected)
    }

    pub fn transition_for(
        &self,
        source_role: &str,
        target_role: &str,
    ) -> Option<&TransitionPolicy> {
        self.transitions
            .iter()
            .find(|transition| transition.from == source_role && transition.to == target_role)
    }

    /// Resolve a CLI-provided value (a role name or a literal branch) to an
    /// actual branch. Falls back to the literal value so ephemeral branches
    /// (e.g. a specific `hotfix/*` branch) work without a fixed mapping.
    pub fn resolve_branch(&self, value: &str) -> String {
        self.branch_for_role(value)
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_resolves_roles_by_name_and_pattern() {
        let policy = LifecyclePolicy::default_policy();
        assert_eq!(policy.role_for_branch("development"), "development");
        assert_eq!(policy.role_for_branch("hotfix/urgent"), "hotfix");
        assert_eq!(policy.role_for_branch("random/topic"), "unmanaged");
        assert!(policy.is_protected("main"));
        assert!(!policy.is_protected("development"));
    }

    #[test]
    fn zero_config_falls_back_to_default_policy() {
        let dir = tempfile::tempdir().unwrap();
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        assert!(policy.transition_for("development", "staging").is_some());
    }

    #[test]
    fn configured_branch_names_are_independent_of_role_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("kaptaind.toml"),
            r#"
[lifecycle.branch.develop]
role = "development"

[lifecycle.branch.preprod]
role = "staging"

[lifecycle.transitions.development_to_staging]
from = "development"
to = "staging"
operation = "fast-forward"
requires = ["clean_source"]
"#,
        )
        .unwrap();
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        assert_eq!(policy.role_for_branch("develop"), "development");
        assert_eq!(policy.branch_for_role("staging"), Some("preprod"));
        assert_eq!(
            policy
                .transition_for("development", "staging")
                .unwrap()
                .operation,
            Operation::FastForward
        );
    }
}
