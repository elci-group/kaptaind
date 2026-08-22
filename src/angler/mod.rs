//! Angler - Hook and selective capture system for kaptaind.
//!
//! Angler provides four main capabilities:
//!
//! 1. **Git Hooks Integration** - Manage client-side git hooks (pre-commit, post-commit, etc.)
//!    with configurable commands, timeouts, and pattern matching.
//!
//! 2. **Webhook Enhancement System** - Send HTTP webhooks with HMAC signature verification,
//!    exponential backoff retries, rate limiting, and event filtering.
//!
//! 3. **Selective Change Capture** - Pattern-based filtering and capture of file changes,
//!    allowing fine-grained control over which changes trigger specific actions.
//!
//! 4. **Bait Plugin System** - External plugin system allowing custom scripts and webhooks
//!    to respond to kaptaind lifecycle events.
//!
//! # Example Configuration
//!
//! ```toml
//! [angler]
//! enabled = true
//!
//! [angler.git_hooks]
//! enabled = true
//!
//! [angler.git_hooks.pre_commit]
//! command = "cargo fmt --check"
//! required = true
//! timeout_secs = 30
//!
//! [angler.webhooks]
//! enabled = true
//!
//! [[angler.webhooks.endpoints]]
//! id = "slack"
//! url = "https://hooks.slack.com/services/..."
//! events = ["commit", "error"]
//!
//! [angler.selective]
//! enabled = true
//!
//! [[angler.selective.rules]]
//! id = "security"
//! name = "Security Files"
//! patterns = ["**/.env*", "**/secrets*"]
//! action = "block"
//! priority = 100
//!
//! [angler.bait]
//! enabled = true
//!
//! [[angler.bait.baits]]
//! id = "notify"
//! name = "Notification"
//! type = "webhook"
//! command = "https://example.com/webhook"
//! events = ["post_commit"]
//! ```

pub mod bait;
pub mod config;
pub mod git_hooks;
pub mod orb_sanitize;
pub mod selective;
pub mod webhooks;

// Re-export main types
pub use bait::{BaitContext, BaitManager, BaitResult, FileChangeInfo};
pub use config::ChangeType;
pub use config::{
    AnglerConfig, BaitConfig, BaitDefinition, BaitEvent, BaitType, CaptureAction, CaptureRule,
    GitHooksConfig, HookConfig, RetryConfig, SelectiveConfig, SignatureAlgorithm, WebhookEndpoint,
    WebhooksConfig,
};
pub use git_hooks::{GitHookManager, HookResult};
pub use selective::{CaptureResult, FileChange, SelectiveEngine};
pub use webhooks::{DeliveryResult, WebhookEvent, WebhookManager};

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{error, info};

/// Main Angler system that coordinates all four capabilities.
pub struct AnglerSystem {
    /// Git hooks manager
    pub git_hooks: Option<GitHookManager>,
    /// Webhook manager
    pub webhooks: Option<WebhookManager>,
    /// Selective capture engine
    pub selective: Option<SelectiveEngine>,
    /// Bait plugin manager
    pub bait: Option<BaitManager>,
    /// Configuration
    config: AnglerConfig,
}

impl AnglerSystem {
    /// Create and initialize the Angler system.
    pub fn new(config: &AnglerConfig, repo_path: &Path) -> Result<Self> {
        let mut system = Self {
            git_hooks: None,
            webhooks: None,
            selective: None,
            bait: None,
            config: config.clone(),
        };

        // Initialize git hooks
        if config.git_hooks.enabled {
            match GitHookManager::new(&config.git_hooks, repo_path) {
                Ok(manager) => {
                    // Install hooks
                    if let Err(e) = manager.install_hooks() {
                        error!(
                            component = module_path!(),
                            "Failed to install git hooks: {}", e
                        );
                    } else {
                        info!(
                            component = module_path!(),
                            "Git hooks installed successfully"
                        );
                    }
                    system.git_hooks = Some(manager);
                }
                Err(e) => {
                    error!(
                        component = module_path!(),
                        "Failed to create git hook manager: {}", e
                    );
                }
            }
        }

        // Initialize webhooks
        if config.webhooks.enabled {
            match WebhookManager::new(&config.webhooks) {
                Ok(manager) => {
                    info!(component = module_path!(), "Webhook manager initialized");
                    system.webhooks = Some(manager);
                }
                Err(e) => {
                    error!(
                        component = module_path!(),
                        "Failed to create webhook manager: {}", e
                    );
                }
            }
        }

        // Initialize selective capture
        if config.selective.enabled {
            match SelectiveEngine::new(&config.selective) {
                Ok(engine) => {
                    info!(
                        component = module_path!(),
                        "Selective capture engine initialized with {} rules",
                        engine.list_rules().len()
                    );
                    system.selective = Some(engine);
                }
                Err(e) => {
                    error!(
                        component = module_path!(),
                        "Failed to create selective engine: {}", e
                    );
                }
            }
        }

        // Initialize bait system
        if config.bait.enabled {
            match BaitManager::new(&config.bait, repo_path) {
                Ok(manager) => {
                    info!(
                        component = module_path!(),
                        "Bait manager initialized with {} baits",
                        manager.list_baits().len()
                    );
                    system.bait = Some(manager);
                }
                Err(e) => {
                    error!(
                        component = module_path!(),
                        "Failed to create bait manager: {}", e
                    );
                }
            }
        }

        Ok(system)
    }

    /// Check if the angler system is effectively enabled (has any active component).
    pub fn is_active(&self) -> bool {
        self.git_hooks.is_some()
            || self.webhooks.is_some()
            || self.selective.is_some()
            || self.bait.is_some()
    }

    /// Return the configuration used to initialize this Angler system.
    pub fn config(&self) -> &AnglerConfig {
        &self.config
    }

    /// Run pre-commit hooks.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_pre_commit(&self, staged_files: &[std::path::PathBuf]) -> Option<HookResult> {
        if let Some(ref manager) = self.git_hooks {
            match manager.run_pre_commit(staged_files).await {
                Ok(result) => return Some(result),
                Err(e) => {
                    error!(component = module_path!(), "Pre-commit hook error: {}", e);
                    return Some(HookResult::failure(format!("Error: {}", e)));
                }
            }
        }
        None
    }

    /// Run post-commit hooks.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_post_commit(&self) -> Option<HookResult> {
        if let Some(ref manager) = self.git_hooks {
            match manager.run_post_commit().await {
                Ok(result) => return Some(result),
                Err(e) => {
                    error!(component = module_path!(), "Post-commit hook error: {}", e);
                }
            }
        }
        None
    }

    /// Run pre-push hooks.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_pre_push(
        &self,
        remote_name: &str,
        remote_url: &str,
        refs: &[(String, String, String, String)],
    ) -> Option<HookResult> {
        if let Some(ref manager) = self.git_hooks {
            match manager.run_pre_push(remote_name, remote_url, refs).await {
                Ok(result) => return Some(result),
                Err(e) => {
                    error!(component = module_path!(), "Pre-push hook error: {}", e);
                }
            }
        }
        None
    }

    /// Broadcast an event to all webhooks.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn broadcast_webhook_event(
        &self,
        event: &WebhookEvent,
        file_changes: &[std::path::PathBuf],
    ) -> Vec<(String, DeliveryResult)> {
        if let Some(ref manager) = self.webhooks {
            manager.broadcast_event(event, file_changes).await
        } else {
            Vec::new()
        }
    }

    /// Evaluate file changes against selective rules.
    pub fn evaluate_changes(
        &self,
        changes: &[selective::FileChange],
    ) -> Vec<(selective::FileChange, CaptureResult)> {
        if let Some(ref engine) = self.selective {
            engine.evaluate_batch(changes)
        } else {
            changes
                .iter()
                .map(|c| (c.clone(), CaptureResult::no_match(CaptureAction::Pass)))
                .collect()
        }
    }

    /// Check if changes would be blocked.
    pub fn would_block_changes(
        &self,
        changes: &[selective::FileChange],
    ) -> Vec<(selective::FileChange, String)> {
        if let Some(ref engine) = self.selective {
            engine.get_blocked_changes(changes)
        } else {
            Vec::new()
        }
    }

    /// Trigger bait plugins for an event.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn trigger_baits(
        &self,
        event: BaitEvent,
        context: &BaitContext,
    ) -> Vec<(String, BaitResult)> {
        if let Some(ref manager) = self.bait {
            manager.trigger_event(event, context).await
        } else {
            Vec::new()
        }
    }

    /// Shutdown the angler system and clean up.
    pub fn shutdown(&self) -> Result<()> {
        if let Some(ref manager) = self.git_hooks {
            if let Err(e) = manager.uninstall_hooks() {
                error!(
                    component = module_path!(),
                    "Failed to uninstall git hooks: {}", e
                );
            }
        }
        Ok(())
    }
}

/// Get the default angler configuration with all features enabled.
pub fn default_config() -> AnglerConfig {
    AnglerConfig {
        git_hooks: GitHooksConfig {
            enabled: true,
            hooks_dir: None,
            pre_commit: None,
            prepare_commit_msg: None,
            commit_msg: None,
            post_commit: None,
            pre_push: None,
            post_push: None,
            post_checkout: None,
            post_merge: None,
            custom: std::collections::HashMap::new(),
        },
        webhooks: WebhooksConfig {
            enabled: false,
            endpoints: Vec::new(),
            default_retry: RetryConfig::default(),
            signature: config::SignatureConfig::default(),
        },
        selective: SelectiveConfig {
            enabled: false,
            rules: Vec::new(),
            default_action: CaptureAction::Pass,
        },
        bait: BaitConfig {
            enabled: false,
            plugins_dir: PathBuf::from(".kaptaind/baits"),
            baits: Vec::new(),
            auto_discover: true,
        },
    }
}

/// Create a security-focused angler configuration.
pub fn security_config() -> AnglerConfig {
    AnglerConfig {
        git_hooks: GitHooksConfig {
            enabled: true,
            hooks_dir: None,
            pre_commit: Some(HookConfig {
                command: "cargo audit".to_string(),
                required: false,
                timeout_secs: 60,
                env: std::collections::HashMap::new(),
                working_dir: None,
                file_patterns: vec!["**/Cargo.toml".to_string()],
            }),
            prepare_commit_msg: None,
            commit_msg: None,
            post_commit: None,
            pre_push: Some(HookConfig {
                command: "cargo test".to_string(),
                required: true,
                timeout_secs: 300,
                env: std::collections::HashMap::new(),
                working_dir: None,
                file_patterns: vec![],
            }),
            post_push: None,
            post_checkout: None,
            post_merge: None,
            custom: std::collections::HashMap::new(),
        },
        webhooks: WebhooksConfig::default(),
        selective: SelectiveConfig {
            enabled: true,
            rules: vec![selective::templates::security_sensitive_rule()],
            default_action: CaptureAction::Pass,
        },
        bait: BaitConfig::default(),
    }
}

/// Create a CI/CD focused angler configuration.
pub fn cicd_config(webhook_url: &str) -> AnglerConfig {
    AnglerConfig {
        git_hooks: GitHooksConfig {
            enabled: false,
            ..Default::default()
        },
        webhooks: WebhooksConfig {
            enabled: true,
            endpoints: vec![WebhookEndpoint {
                id: "ci-cd".to_string(),
                url: webhook_url.to_string(),
                events: vec![
                    "commit".to_string(),
                    "push".to_string(),
                    "error".to_string(),
                ],
                headers: std::collections::HashMap::new(),
                retry: None,
                verify_signature: false,
                secret: None,
                file_filters: vec![],
                rate_limit_per_min: Some(60),
            }],
            default_retry: RetryConfig::default(),
            signature: config::SignatureConfig::default(),
        },
        selective: SelectiveConfig {
            enabled: true,
            rules: vec![
                selective::templates::documentation_rule(),
                selective::templates::test_files_rule(),
                selective::templates::config_files_rule(),
            ],
            default_action: CaptureAction::Pass,
        },
        bait: BaitConfig {
            enabled: true,
            plugins_dir: PathBuf::from(".kaptaind/baits"),
            baits: vec![bait::templates::metrics_bait(&format!(
                "{}/metrics",
                webhook_url
            ))],
            auto_discover: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert!(config.git_hooks.enabled);
        assert!(!config.webhooks.enabled);
        assert!(!config.selective.enabled);
        assert!(!config.bait.enabled);
    }

    #[test]
    fn test_security_config() {
        let config = security_config();
        assert!(config.git_hooks.enabled);
        assert!(config.selective.enabled);
        assert!(!config.bait.enabled);
        assert!(config.git_hooks.pre_push.is_some());
    }

    #[test]
    fn test_cicd_config() {
        let config = cicd_config("https://ci.example.com/webhook");
        assert!(!config.git_hooks.enabled);
        assert!(config.webhooks.enabled);
        assert!(config.selective.enabled);
        assert!(config.bait.enabled);
    }

    #[test]
    fn test_angler_system_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = default_config();

        let system = AnglerSystem::new(&config, temp_dir.path()).unwrap();
        assert!(system.is_active());
    }

    #[test]
    fn test_empty_system_not_active() {
        let temp_dir = TempDir::new().unwrap();
        let config = AnglerConfig {
            git_hooks: GitHooksConfig {
                enabled: false,
                ..Default::default()
            },
            webhooks: WebhooksConfig {
                enabled: false,
                ..Default::default()
            },
            selective: SelectiveConfig {
                enabled: false,
                ..Default::default()
            },
            bait: BaitConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let system = AnglerSystem::new(&config, temp_dir.path()).unwrap();
        assert!(!system.is_active());
    }

    #[tokio::test]
    async fn test_trigger_baits_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = default_config();

        let system = AnglerSystem::new(&config, temp_dir.path()).unwrap();
        let context = BaitContext {
            event: BaitEvent::PostCommit,
            files: vec![],
            repo_path: temp_dir.path().to_path_buf(),
            cluster_id: None,
            version: None,
            score: None,
            metadata: HashMap::new(),
        };

        let results = system.trigger_baits(BaitEvent::PostCommit, &context).await;
        assert!(results.is_empty());
    }
}
