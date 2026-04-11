//! Configuration for the Angler hook and selective capture system.
//!
//! Angler provides four main capabilities:
//! 1. Git Hooks - Client-side git hook management
//! 2. Webhooks - Enhanced external webhook system
//! 3. Selective Capture - Pattern-based change filtering
//! 4. Bait Plugins - External plugin hook system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level Angler configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnglerConfig {
    /// Git hooks configuration
    #[serde(default)]
    pub git_hooks: GitHooksConfig,

    /// Enhanced webhooks configuration
    #[serde(default)]
    pub webhooks: WebhooksConfig,

    /// Selective change capture configuration
    #[serde(default)]
    pub selective: SelectiveConfig,

    /// Bait plugin system configuration
    #[serde(default)]
    pub bait: BaitConfig,
}

impl Default for AnglerConfig {
    fn default() -> Self {
        Self {
            git_hooks: GitHooksConfig::default(),
            webhooks: WebhooksConfig::default(),
            selective: SelectiveConfig::default(),
            bait: BaitConfig::default(),
        }
    }
}

// =============================================================================
// Git Hooks Configuration
// =============================================================================

/// Git client-side hooks configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHooksConfig {
    /// Enable git hooks management
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Path to hooks directory (default: .git/hooks)
    pub hooks_dir: Option<PathBuf>,

    /// Pre-commit hook configuration
    #[serde(default)]
    pub pre_commit: Option<HookConfig>,

    /// Prepare-commit-msg hook configuration
    #[serde(default)]
    pub prepare_commit_msg: Option<HookConfig>,

    /// Commit-msg hook configuration
    #[serde(default)]
    pub commit_msg: Option<HookConfig>,

    /// Post-commit hook configuration
    #[serde(default)]
    pub post_commit: Option<HookConfig>,

    /// Pre-push hook configuration
    #[serde(default)]
    pub pre_push: Option<HookConfig>,

    /// Post-push hook configuration
    #[serde(default)]
    pub post_push: Option<HookConfig>,

    /// Post-checkout hook configuration
    #[serde(default)]
    pub post_checkout: Option<HookConfig>,

    /// Post-merge hook configuration
    #[serde(default)]
    pub post_merge: Option<HookConfig>,

    /// Custom hooks not in standard git set
    #[serde(default)]
    pub custom: HashMap<String, HookConfig>,
}

impl Default for GitHooksConfig {
    fn default() -> Self {
        Self {
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
            custom: HashMap::new(),
        }
    }
}

/// Individual hook configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookConfig {
    /// Command to execute
    pub command: String,

    /// Whether the hook is required (failure blocks the operation)
    #[serde(default = "default_true")]
    pub required: bool,

    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Environment variables to set
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Working directory (relative to repo root)
    pub working_dir: Option<PathBuf>,

    /// Only run on specific file patterns
    #[serde(default)]
    pub file_patterns: Vec<String>,
}

// =============================================================================
// Webhooks Configuration
// =============================================================================

/// Enhanced webhook system configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhooksConfig {
    /// Enable enhanced webhooks
    #[serde(default)]
    pub enabled: bool,

    /// Webhook endpoints
    #[serde(default)]
    pub endpoints: Vec<WebhookEndpoint>,

    /// Default retry configuration
    #[serde(default)]
    pub default_retry: RetryConfig,

    /// Signature verification settings
    #[serde(default)]
    pub signature: SignatureConfig,
}

impl Default for WebhooksConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoints: Vec::new(),
            default_retry: RetryConfig::default(),
            signature: SignatureConfig::default(),
        }
    }
}

/// Individual webhook endpoint configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookEndpoint {
    /// Unique identifier for this endpoint
    pub id: String,

    /// Target URL
    pub url: String,

    /// Events to subscribe to (empty = all events)
    #[serde(default)]
    pub events: Vec<String>,

    /// Custom headers
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Retry configuration (overrides default)
    #[serde(default)]
    pub retry: Option<RetryConfig>,

    /// Enable signature verification
    #[serde(default)]
    pub verify_signature: bool,

    /// Secret for HMAC signature
    pub secret: Option<String>,

    /// Filter patterns (only send if files match)
    #[serde(default)]
    pub file_filters: Vec<String>,

    /// Rate limit (max requests per minute)
    #[serde(default)]
    pub rate_limit_per_min: Option<u32>,
}

/// Retry configuration for webhooks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    #[serde(default = "default_retry_attempts")]
    pub max_attempts: u32,

    /// Initial delay between retries (milliseconds)
    #[serde(default = "default_retry_delay")]
    pub initial_delay_ms: u64,

    /// Backoff multiplier
    #[serde(default = "default_backoff")]
    pub backoff_multiplier: f64,

    /// Maximum delay between retries (milliseconds)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }
}

/// Signature verification configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignatureConfig {
    /// Signature header name
    #[serde(default = "default_signature_header")]
    pub header_name: String,

    /// Signature algorithm
    #[serde(default = "default_signature_algo")]
    pub algorithm: SignatureAlgorithm,

    /// Include timestamp in signature
    #[serde(default = "default_true")]
    pub include_timestamp: bool,
}

impl Default for SignatureConfig {
    fn default() -> Self {
        Self {
            header_name: "X-Webhook-Signature".to_string(),
            algorithm: SignatureAlgorithm::HmacSha256,
            include_timestamp: true,
        }
    }
}

/// Signature algorithm options.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureAlgorithm {
    HmacSha256,
    HmacSha512,
    Ed25519,
}

// =============================================================================
// Selective Change Capture Configuration
// =============================================================================

/// Selective change capture configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SelectiveConfig {
    /// Enable selective capture
    #[serde(default)]
    pub enabled: bool,

    /// Capture rules
    #[serde(default)]
    pub rules: Vec<CaptureRule>,

    /// Default action when no rules match
    #[serde(default)]
    pub default_action: CaptureAction,
}

impl Default for SelectiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rules: Vec::new(),
            default_action: CaptureAction::Pass,
        }
    }
}

/// Individual capture rule.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CaptureRule {
    /// Rule identifier
    pub id: String,

    /// Rule name/description
    pub name: String,

    /// File patterns to match (glob syntax)
    pub patterns: Vec<String>,

    /// Content patterns (regex) - optional additional filtering
    #[serde(default)]
    pub content_patterns: Vec<String>,

    /// Change types to capture
    #[serde(default)]
    pub change_types: Vec<ChangeType>,

    /// Action to take when rule matches
    pub action: CaptureAction,

    /// Priority (higher = evaluated first)
    #[serde(default)]
    pub priority: i32,

    /// Whether this rule is active
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum file size to scan (bytes, 0 = unlimited)
    #[serde(default)]
    pub max_file_size: u64,

    /// Associated metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Types of changes that can be captured.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// Action to take when a capture rule matches.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptureAction {
    /// Pass the change through normally
    #[default]
    Pass,
    /// Block the change (fails the commit)
    Block,
    /// Quarantine for review (special handling)
    Quarantine,
    /// Tag with metadata
    Tag { tags: Vec<String> },
    /// Trigger a specific webhook
    Webhook { endpoint_id: String },
    /// Execute a custom command
    Execute { command: String },
}

// =============================================================================
// Bait Plugin Configuration
// =============================================================================

/// Bait plugin system configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BaitConfig {
    /// Enable bait plugin system
    #[serde(default)]
    pub enabled: bool,

    /// Plugin directory
    #[serde(default = "default_bait_dir")]
    pub plugins_dir: PathBuf,

    /// Registered baits
    #[serde(default)]
    pub baits: Vec<BaitDefinition>,

    /// Auto-discover baits from plugins directory
    #[serde(default = "default_true")]
    pub auto_discover: bool,
}

impl Default for BaitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            plugins_dir: default_bait_dir(),
            baits: Vec::new(),
            auto_discover: true,
        }
    }
}

/// Individual bait definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BaitDefinition {
    /// Bait identifier
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description
    pub description: String,

    /// Plugin type
    #[serde(rename = "type")]
    pub bait_type: BaitType,

    /// Command/script to execute
    pub command: String,

    /// File patterns this bait responds to
    #[serde(default)]
    pub file_patterns: Vec<String>,

    /// Events this bait responds to
    #[serde(default)]
    pub events: Vec<BaitEvent>,

    /// Whether bait is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Timeout in seconds
    #[serde(default = "default_bait_timeout")]
    pub timeout_secs: u64,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Type of bait plugin.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaitType {
    /// External script/binary
    External,
    /// Shell command
    Shell,
    /// Webhook call
    Webhook,
    /// Internal Rust function (requires compilation)
    Native,
}

/// Events that can trigger a bait.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaitEvent {
    PreCommit,
    PostCommit,
    PrePush,
    PostPush,
    FileChange,
    ClusterComplete,
    AnalysisComplete,
}

// =============================================================================
// Helper Functions
// =============================================================================

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    60
}

fn default_bait_timeout() -> u64 {
    30
}

fn default_retry_attempts() -> u32 {
    3
}

fn default_retry_delay() -> u64 {
    1000
}

fn default_backoff() -> f64 {
    2.0
}

fn default_max_delay() -> u64 {
    30000
}

fn default_signature_header() -> String {
    "X-Webhook-Signature".to_string()
}

fn default_signature_algo() -> SignatureAlgorithm {
    SignatureAlgorithm::HmacSha256
}

fn default_bait_dir() -> PathBuf {
    PathBuf::from(".kaptaind/baits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AnglerConfig::default();
        assert!(config.git_hooks.enabled);
        assert!(!config.webhooks.enabled);
        assert!(!config.selective.enabled);
        assert!(!config.bait.enabled);
    }

    #[test]
    fn test_hook_config_defaults() {
        let hook = HookConfig {
            command: "echo test".to_string(),
            required: true,
            timeout_secs: default_timeout(),
            env: HashMap::new(),
            working_dir: None,
            file_patterns: vec![],
        };
        assert_eq!(hook.timeout_secs, 60);
        assert!(hook.required);
    }

    #[test]
    fn test_capture_action_serialization() {
        let action = CaptureAction::Tag {
            tags: vec!["security".to_string(), "critical".to_string()],
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("tag"));
    }

    #[test]
    fn test_webhook_endpoint_validation() {
        let endpoint = WebhookEndpoint {
            id: "test".to_string(),
            url: "https://example.com/webhook".to_string(),
            events: vec!["commit".to_string()],
            headers: HashMap::new(),
            retry: None,
            verify_signature: true,
            secret: Some("secret123".to_string()),
            file_filters: vec![],
            rate_limit_per_min: Some(60),
        };
        assert_eq!(endpoint.id, "test");
        assert!(endpoint.verify_signature);
    }
}
