use crate::angler::config::AnglerConfig;
use crate::collectors::jeenome::JeenomeConfig;
use crate::notify::audio::TtsConfig;
use crate::qualification::policy::QualificationConfig;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub operation: OperationConfig,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub cluster: ClusterConfig,
    #[serde(default)]
    pub weights: crate::weight::WeightConfig,
    #[serde(default)]
    pub push: PushConfig,
    #[serde(default)]
    pub pull: PullConfig,
    #[serde(default)]
    pub ratelimit: RateLimitConfig,
    #[serde(default)]
    pub test: TestConfig,
    #[serde(default)]
    pub jeenome: JeenomeConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub notify: NotifyConfig,
    #[serde(default)]
    pub bundle: BundleConfig,
    #[serde(default)]
    pub staging: StagingConfig,
    #[serde(default)]
    pub commit: CommitConfig,
    #[serde(default)]
    pub inference: InferenceConfig,
    #[serde(default)]
    pub qualification: QualificationConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub release: ReleaseConfig,
    #[serde(default)]
    pub distribution: DistributionConfig,
    #[serde(default)]
    pub ship: ShipConfig,
    #[serde(default)]
    pub version_thresholds: VersionThresholdConfig,
    #[serde(default)]
    pub versioning: VersioningConfig,
    #[serde(default)]
    pub plugins: PluginsConfig,
    #[serde(default)]
    pub vacs: VacsConfig,
    #[serde(default)]
    pub trawl: TrawlConfig,
    #[serde(default)]
    pub angler: AnglerConfig,
    #[serde(default)]
    pub deckhand: DeckhandConfig,
    #[serde(default)]
    pub shark: SharkConfig,
    #[serde(default)]
    pub rbac: RbacConfig,
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub repo_path: PathBuf,
    #[serde(default)]
    pub policy_id: Option<String>,
    #[serde(default)]
    pub policy_trust: PolicyTrustConfig,
    #[serde(default)]
    pub governance: GovernanceConfig,
    #[serde(default = "default_prune_interval_minutes")]
    pub prune_interval_minutes: u64,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default)]
    pub air_gapped: bool,
    #[serde(default = "default_health_port")]
    pub health_port: u16,
    /// Port for the optional WebUI server. 0 means disabled.
    #[serde(default)]
    pub web_port: u16,
    #[serde(default)]
    pub web: WebConfig,
    #[serde(default)]
    pub capabilities: CapabilitiesConfig,
    #[serde(default)]
    pub trust: TrustConfig,
    #[serde(default)]
    pub compliance: ComplianceConfig,
    #[serde(default)]
    pub integrations: IntegrationsConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
}

/// Controls whether the daemon may perform repository mutations.
///
/// Observation is deliberately the default. Actuation requires an explicit
/// `[operation] mode = "actuate"` declaration in the repository profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationMode {
    Observe,
    Actuate,
}

impl Default for OperationMode {
    fn default() -> Self {
        Self::Observe
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct OperationConfig {
    #[serde(default)]
    pub mode: OperationMode,
}

impl Default for OperationConfig {
    fn default() -> Self {
        Self {
            mode: OperationMode::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Daemon runtime config
// ---------------------------------------------------------------------------

/// `[daemon]` block: runtime behavior of the long-running daemon.
#[derive(Debug, Clone, Deserialize)]
pub struct DaemonConfig {
    /// Maximum seconds to wait for in-flight work during graceful shutdown
    /// before remaining tasks are aborted (default 10).
    #[serde(default = "default_shutdown_grace_secs")]
    pub shutdown_grace_secs: u64,
    /// Refuse to start when the worktree has uncommitted changes (default
    /// false). For repos where daemon runs are exceptional — e.g. release
    /// trees a daemon must never casually bump. Bypass with `--force`.
    #[serde(default)]
    pub startup_guard: bool,
    /// Automatically suspend the daemon when an Aim-of-Change session starts.
    #[serde(default = "default_true")]
    pub auto_suspend_on_aoc_start: bool,
    /// Automatically resume the daemon when an Aim-of-Change session is
    /// shipped or cancelled.
    #[serde(default = "default_true")]
    pub auto_resume_on_aoc_end: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            shutdown_grace_secs: default_shutdown_grace_secs(),
            startup_guard: false,
            auto_suspend_on_aoc_start: true,
            auto_resume_on_aoc_end: true,
        }
    }
}

fn default_shutdown_grace_secs() -> u64 {
    10
}

// ---------------------------------------------------------------------------
// Build config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct BuildConfig {
    /// Shell command to run (e.g. `"cargo build --release"`). No build step
    /// when absent.
    pub command: Option<String>,
    /// Path to the produced artifact, relative to the repo root.
    #[serde(default = "default_artifact_path")]
    pub artifact_path: String,
    /// Maximum seconds to wait for the build before aborting.
    #[serde(default = "default_build_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            command: None,
            artifact_path: default_artifact_path(),
            timeout_secs: default_build_timeout_secs(),
        }
    }
}

fn default_artifact_path() -> String {
    "target/release/kaptaind".to_string()
}

fn default_build_timeout_secs() -> u64 {
    600 // 10 minutes
}

// ---------------------------------------------------------------------------
// Audit config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct AuditConfig {
    /// Whether to write structured audit logs to `.kaptaind/audit.jsonl`.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional enterprise-controlled JSONL mirror for audit events.
    #[serde(default)]
    pub export: Option<crate::audit::AuditExportConfig>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Release intent + config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseIntent {
    /// No release pipeline (default). Preserves existing behaviour.
    #[default]
    None,
    /// Build only; no packaging or distribution.
    Preview,
    /// Build + private/local distribution.
    Internal,
    /// Full release pipeline.
    Public,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReleaseConfig {
    #[serde(default)]
    pub intent: ReleaseIntent,
}

// ---------------------------------------------------------------------------
// Distribution config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DistributionConfig {
    #[serde(default)]
    pub local: Option<LocalDistConfig>,
    #[serde(default)]
    pub s3: Option<S3DistConfig>,
    #[serde(default)]
    pub registry: Option<RegistryDistConfig>,
    #[serde(default)]
    pub security: Option<SecurityDistConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalDistConfig {
    #[serde(default = "default_local_dist_path")]
    pub path: String,
}

fn default_prune_interval_minutes() -> u64 {
    60
}

fn default_retention_days() -> u32 {
    30
}

fn default_health_port() -> u16 {
    9090
}

fn default_local_dist_path() -> String {
    ".kaptaind/releases/".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for LocalDistConfig {
    fn default() -> Self {
        Self {
            path: default_local_dist_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct S3DistConfig {
    pub bucket: String,
    pub region: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryDistConfig {
    #[serde(rename = "type")]
    pub kind: String,
    pub image: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityDistConfig {
    #[serde(default)]
    pub encrypt: bool,
    #[serde(default)]
    pub sign: bool,
}

// ---------------------------------------------------------------------------
// Ship config
// ---------------------------------------------------------------------------

/// `[ship]` block in `kaptaind.toml`.
///
/// Configures the `kaptaind-cli ship` command: which targets to build,
/// which installers to produce, and which package managers / app stores
/// to publish to.
#[derive(Debug, Clone, Deserialize)]
pub struct ShipConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ship_targets")]
    pub targets: Vec<String>,
    #[serde(flatten)]
    pub channels: ShipChannelsConfig,
    /// When true, `ship` enforces the daemon `[qualification]` gates.
    #[serde(default = "default_true")]
    pub require_qualification: bool,
    /// GPG-sign git tags and release artifact checksums.
    #[serde(default)]
    pub sign: bool,
    /// Optional GPG key ID or email to use for signing.
    #[serde(default)]
    pub gpg_key_id: Option<String>,
    #[serde(default)]
    pub nightly: ShipKindConfig,
    #[serde(default)]
    pub stable: ShipKindConfig,
    #[serde(default)]
    pub auto_nightly: ShipAutoConfig,
    #[serde(default)]
    pub auto_stable: ShipAutoConfig,
    #[serde(default)]
    pub sbom: ShipSbomConfig,
    #[serde(default)]
    pub provenance: ShipProvenanceConfig,
}

impl Default for ShipConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            targets: default_ship_targets(),
            channels: ShipChannelsConfig::default(),
            require_qualification: true,
            sign: false,
            gpg_key_id: None,
            nightly: ShipKindConfig::default(),
            stable: ShipKindConfig::default(),
            auto_nightly: ShipAutoConfig::default(),
            auto_stable: ShipAutoConfig::default(),
            sbom: ShipSbomConfig::default(),
            provenance: ShipProvenanceConfig::default(),
        }
    }
}

/// `[ship.sbom]` block in `kaptaind.toml`.
///
/// Configures SBOM generation for shipped releases.
#[derive(Debug, Clone, Deserialize)]
pub struct ShipSbomConfig {
    /// Generate an SBOM for every release.
    #[serde(default)]
    pub enabled: bool,
    /// Output format. Currently only `"spdx-json"` is supported.
    #[serde(default = "default_sbom_format")]
    pub format: String,
}

fn default_sbom_format() -> String {
    "spdx-json".to_string()
}

impl Default for ShipSbomConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            format: default_sbom_format(),
        }
    }
}

/// `[ship.provenance]` block in `kaptaind.toml`.
///
/// Configures SLSA provenance attestation generation for shipped releases.
#[derive(Debug, Clone, Deserialize)]
pub struct ShipProvenanceConfig {
    /// Generate an in-toto/SLSA provenance attestation for every release.
    #[serde(default)]
    pub enabled: bool,
    /// Builder ID URI for SLSA runDetails.builder.id.
    #[serde(default = "default_provenance_builder_id")]
    pub builder_id: String,
    /// Build type URI for SLSA buildDefinition.buildType.
    #[serde(default = "default_provenance_build_type")]
    pub build_type: String,
}

fn default_provenance_builder_id() -> String {
    "https://kaptaind.dev/builder".to_string()
}

fn default_provenance_build_type() -> String {
    "https://kaptaind.dev/build".to_string()
}

impl Default for ShipProvenanceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            builder_id: default_provenance_builder_id(),
            build_type: default_provenance_build_type(),
        }
    }
}

/// Per-release-kind overrides for `kaptaind-cli ship stable` and
/// `kaptaind-cli ship nightly`.
#[derive(Debug, Clone, Deserialize)]
pub struct ShipKindConfig {
    /// Override target triples for this release kind.
    #[serde(default)]
    pub targets: Option<Vec<String>>,
    /// Override distribution channels for this release kind.
    #[serde(default)]
    pub channels: Option<Vec<String>>,
    /// Publish as a draft release.
    #[serde(default)]
    pub draft: bool,
    /// Mark release as prerelease.
    #[serde(default)]
    pub prerelease: bool,
    /// Create and optionally push a git tag for this release.
    #[serde(default)]
    pub push_tag: bool,
    /// Override the global `sign` setting for this release kind.
    #[serde(default)]
    pub sign: Option<bool>,
    /// Override the global `require_qualification` setting for this kind.
    #[serde(default)]
    pub require_qualification: Option<bool>,
    /// Generate release notes for GitHub releases.
    #[serde(default = "default_true")]
    pub release_notes: bool,
    /// Number of nightly releases to retain (only applies to `ship nightly`).
    #[serde(default)]
    pub retain_count: Option<usize>,
}

/// Daemon-driven automatic release schedule for a given ship kind.
#[derive(Debug, Clone, Deserialize)]
pub struct ShipAutoConfig {
    /// Enable automated releases on the configured schedule.
    #[serde(default)]
    pub enabled: bool,
    /// Standard 5-field cron expression (e.g. "0 2 * * *").
    #[serde(default = "default_auto_ship_schedule")]
    pub schedule: String,
    /// Timezone interpretation: "local" or "utc".
    #[serde(default = "default_auto_ship_timezone")]
    pub cron_timezone: String,
    /// Whether to require qualification before auto-releasing.
    #[serde(default = "default_auto_ship_require_qualification")]
    pub require_qualification: bool,
}

fn default_auto_ship_schedule() -> String {
    "0 2 * * *".to_string()
}

fn default_auto_ship_timezone() -> String {
    "local".to_string()
}

fn default_auto_ship_require_qualification() -> bool {
    true
}

impl Default for ShipAutoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule: default_auto_ship_schedule(),
            cron_timezone: default_auto_ship_timezone(),
            require_qualification: default_auto_ship_require_qualification(),
        }
    }
}

impl Default for ShipKindConfig {
    fn default() -> Self {
        Self {
            targets: None,
            channels: None,
            draft: false,
            prerelease: false,
            push_tag: false,
            sign: None,
            require_qualification: None,
            release_notes: true,
            retain_count: None,
        }
    }
}

fn default_ship_targets() -> Vec<String> {
    vec![
        "x86_64-unknown-linux-gnu".to_string(),
        "aarch64-unknown-linux-gnu".to_string(),
        "x86_64-apple-darwin".to_string(),
        "aarch64-apple-darwin".to_string(),
        "x86_64-pc-windows-msvc".to_string(),
    ]
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShipChannelsConfig {
    /// Produce per-target binary tarballs.
    #[serde(default = "default_true")]
    pub binaries: bool,
    #[serde(default)]
    pub installers: ShipInstallersConfig,
    #[serde(default)]
    pub package_managers: Vec<ShipPackageManagerConfig>,
    #[serde(default)]
    pub app_stores: Vec<ShipAppStoreConfig>,
}

impl Default for ShipChannelsConfig {
    fn default() -> Self {
        Self {
            binaries: true,
            installers: ShipInstallersConfig::default(),
            package_managers: Vec::new(),
            app_stores: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ShipInstallersConfig {
    /// Bundle `install.sh` with all target binaries.
    #[serde(default)]
    pub shell: bool,
    /// Build Tauri desktop bundles from `apps/desktop`.
    #[serde(default)]
    pub tauri: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShipPackageManagerConfig {
    pub kind: String,
    /// Homebrew tap repo path or URL (e.g. `github.com/elci-group/homebrew-tap`).
    pub tap: Option<String>,
    /// Formula name to generate.
    #[serde(default = "default_formula_name")]
    pub formula_name: String,
    /// Environment variable holding an auth token, if any.
    pub token_env: Option<String>,
}

fn default_formula_name() -> String {
    "kaptaind".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShipAppStoreConfig {
    pub kind: String,
    /// Publish as a draft release.
    #[serde(default)]
    pub draft: bool,
    /// Mark release as prerelease.
    #[serde(default)]
    pub prerelease: bool,
    /// Environment variable holding an auth token, if any.
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WatchConfig {
    #[serde(default = "default_watch_path")]
    pub path: PathBuf,
    #[serde(default = "default_true")]
    pub recursive: bool,
    #[serde(default = "default_ignore_file")]
    pub ignore_file: PathBuf,
    /// On startup, reconcile working-tree changes made while the daemon was
    /// down into a single catch-up cluster (default true).
    #[serde(default = "default_true")]
    pub rescan_on_start: bool,
}

fn default_watch_path() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn default_ignore_file() -> PathBuf {
    PathBuf::from(".kaptainignore")
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            path: default_watch_path(),
            recursive: true,
            ignore_file: default_ignore_file(),
            rescan_on_start: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterConfig {
    #[serde(default = "default_cluster_window", with = "duration_secs")]
    pub window: Duration,
    /// Enable adaptive window sizing based on event burst detection.
    #[serde(default)]
    pub adaptive: bool,
    /// Minimum window when adaptive mode shrinks it (seconds, default 2).
    #[serde(default = "default_min_window_secs")]
    pub min_window_secs: u64,
    /// Maximum window when a burst is detected (seconds, default 30).
    #[serde(default = "default_max_window_secs")]
    pub max_window_secs: u64,
    /// Number of events in a window before it is classified as a burst.
    #[serde(default = "default_burst_threshold")]
    pub burst_threshold: usize,
    /// Maximum number of events/paths in a cluster before flushing (0 = disabled).
    #[serde(default = "default_max_paths")]
    pub max_paths: usize,
    /// Idle timeout after which a cluster is flushed (defaults to `window`).
    #[serde(default, with = "duration_secs_option")]
    pub flush_after: Option<Duration>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            window: default_cluster_window(),
            adaptive: false,
            min_window_secs: default_min_window_secs(),
            max_window_secs: default_max_window_secs(),
            burst_threshold: default_burst_threshold(),
            max_paths: default_max_paths(),
            flush_after: None,
        }
    }
}

fn default_cluster_window() -> Duration {
    Duration::from_secs(5)
}

fn default_min_window_secs() -> u64 {
    2
}
fn default_max_window_secs() -> u64 {
    30
}
fn default_burst_threshold() -> usize {
    10
}
fn default_max_paths() -> usize {
    0
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Branch to push to. The default (`"main"`) is only used when
    /// `push.enabled = true`; with pushing disabled it is inert.
    #[serde(default = "default_branch")]
    pub branch: String,
    /// Primary remote for backward compatibility. Deprecated in favor of remotes array.
    #[serde(default = "default_remote")]
    pub remote: String,
    /// Multiple remotes with different purposes following the Git-provider-saturated stack:
    /// GitHub (global public nexus), GitLab (engineering operations), Bitbucket (enterprise),
    /// Azure/AWS/GCP (cloud infrastructure), Codeberg (independent OSS), SourceHut (minimalist),
    /// Gitea/Forgejo (private sovereign), Gerrit (code review authority), etc.
    #[serde(default)]
    pub remotes: Vec<RemoteConfig>,
    /// Intent-based routing configuration for automatic provider selection
    #[serde(default)]
    pub intent_routing: IntentRouting,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub conflict: ConflictConfig,
    #[serde(default)]
    pub pre_push: PrePushConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub protection: PushProtectionConfig,
}

/// `[pull]` policy for transactional remote acquisition and integration.
///
/// `remote` and `branch` intentionally default to `None`: when CLI arguments
/// are absent the pull engine resolves the current branch's Git upstream and
/// fails safely if that relationship is ambiguous.
#[derive(Debug, Clone, Deserialize)]
pub struct PullConfig {
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "default_pull_strategy")]
    pub default_strategy: String,
    #[serde(default = "default_true")]
    pub prune: bool,
    #[serde(default = "default_fetch_tags")]
    pub tags: String,
    #[serde(default)]
    pub autostash: bool,
    #[serde(default = "default_pull_protected_branches")]
    pub protected_branches: Vec<String>,
    #[serde(default = "default_critical_risk_threshold")]
    pub critical_risk_threshold: f32,
    /// Optional project build command run after integration.
    #[serde(default)]
    pub verify_build: Option<String>,
    /// Optional project test command run after integration.
    #[serde(default)]
    pub verify_tests: Option<String>,
}

impl Default for PullConfig {
    fn default() -> Self {
        Self {
            remote: None,
            branch: None,
            default_strategy: default_pull_strategy(),
            prune: true,
            tags: default_fetch_tags(),
            autostash: false,
            protected_branches: default_pull_protected_branches(),
            critical_risk_threshold: default_critical_risk_threshold(),
            verify_build: None,
            verify_tests: None,
        }
    }
}

fn default_pull_strategy() -> String {
    "merge".to_owned()
}

fn default_fetch_tags() -> String {
    "follow".to_owned()
}

fn default_pull_protected_branches() -> Vec<String> {
    vec![
        "main".to_owned(),
        "master".to_owned(),
        "production".to_owned(),
        "release/*".to_owned(),
    ]
}

fn default_critical_risk_threshold() -> f32 {
    0.85
}

/// Remote configuration with purpose-based roles for Git-provider-saturated stack.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteConfig {
    /// Git remote name (e.g., "github", "gitlab", "codeberg")
    pub name: String,
    /// Provider type: github, gitlab, bitbucket, azure, aws, gcp, codeberg, sourcehut,
    /// gitea, forgejo, gogs, phabricator, gerrit, launchpad, savannah, pagure, perforce
    #[serde(default)]
    pub provider: String,
    /// Role in the saturated stack: "public_nexus", "engineering_ops", "enterprise_collab",
    /// "microsoft_enterprise", "aws_infrastructure", "gcp_integration", "independent_oss",
    /// "minimalist_unix", "private_sovereign", "community_controlled", "ultra_light",
    /// "legacy_review", "code_review_authority", "ubuntu_ecosystem", "fsf_ecosystem",
    /// "fedora_ecosystem", "ethical_oss", "binary_asset"
    #[serde(default)]
    pub role: String,
    /// Whether this remote is enabled for pushing
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Priority order for pushing (lower numbers push first)
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Intent tags that trigger routing to this provider (e.g., ["oss", "public", "community"])
    #[serde(default)]
    pub intents: Vec<String>,
    /// Whether this provider is the canonical source of truth
    #[serde(default)]
    pub canonical: bool,
    /// Whether this provider is a backup/archive mirror
    #[serde(default)]
    pub backup: bool,
    /// Whether this provider is regional/cloud-specific
    #[serde(default)]
    pub regional: bool,
}

/// Intent-based routing configuration for provider selection.
#[derive(Debug, Clone, Deserialize)]
pub struct IntentRouting {
    /// Enable intent-based routing (automatically selects providers based on commit intent)
    #[serde(default)]
    pub enabled: bool,
    /// Default intent if none is specified
    #[serde(default = "default_intent")]
    pub default_intent: String,
    /// Intent tag mappings: file patterns or commit message patterns that trigger intents
    #[serde(default)]
    pub intent_patterns: Vec<IntentPattern>,
}

/// Pattern mapping for intent detection.
#[derive(Debug, Clone, Deserialize)]
pub struct IntentPattern {
    /// Intent tag to assign when pattern matches
    pub intent: String,
    /// File glob patterns that trigger this intent
    #[serde(default)]
    pub file_patterns: Vec<String>,
    /// Commit message regex patterns that trigger this intent
    #[serde(default)]
    pub message_patterns: Vec<String>,
}

fn default_intent() -> String {
    "general".to_string()
}

fn default_priority() -> u32 {
    100
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            branch: default_branch(),
            remote: default_remote(),
            remotes: Vec::new(),
            intent_routing: IntentRouting::default(),
            dry_run: false,
            retry: RetryConfig::default(),
            conflict: ConflictConfig::default(),
            pre_push: PrePushConfig::default(),
            safety: SafetyConfig::default(),
            batch: BatchConfig::default(),
            protection: PushProtectionConfig::default(),
        }
    }
}

impl Default for IntentRouting {
    fn default() -> Self {
        Self {
            enabled: false,
            default_intent: default_intent(),
            intent_patterns: Vec::new(),
        }
    }
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_remote() -> String {
    "origin".to_string()
}

/// `[push.protection]` configuration for pre-push safety gates.
#[derive(Debug, Clone, Deserialize)]
pub struct PushProtectionConfig {
    /// Require configured CI status checks to pass before pushing.
    #[serde(default)]
    pub require_ci_pass: bool,
    /// List of required status check names (e.g. ["ci/tests", "ci/lint"]).
    #[serde(default)]
    pub required_status_checks: Vec<String>,
    /// Environment variable holding a GitHub personal access token for API checks.
    #[serde(default)]
    pub github_token_env: Option<String>,
}

impl Default for PushProtectionConfig {
    fn default() -> Self {
        Self {
            require_ci_pass: false,
            required_status_checks: Vec::new(),
            github_token_env: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_retry_initial_delay_ms")]
    pub initial_delay_ms: u64,
    #[serde(default = "default_retry_backoff_multiplier")]
    pub backoff_multiplier: f64,
    #[serde(default = "default_retry_max_delay_ms")]
    pub max_delay_ms: u64,
    /// Maximum time to wait for a single `git push` attempt to complete before
    /// treating it as failed and moving on to the next retry. Without this, a
    /// hung child process (e.g. a credential helper blocked on interactive
    /// input) can stall the push indefinitely with no error logged.
    #[serde(default = "default_retry_attempt_timeout_secs")]
    pub attempt_timeout_secs: u64,
}

fn default_retry_max_attempts() -> u32 {
    3
}
fn default_retry_initial_delay_ms() -> u64 {
    1000
}
fn default_retry_backoff_multiplier() -> f64 {
    2.0
}
fn default_retry_max_delay_ms() -> u64 {
    30000
}
fn default_retry_attempt_timeout_secs() -> u64 {
    60
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_retry_max_attempts(),
            initial_delay_ms: default_retry_initial_delay_ms(),
            backoff_multiplier: default_retry_backoff_multiplier(),
            max_delay_ms: default_retry_max_delay_ms(),
            attempt_timeout_secs: default_retry_attempt_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConflictConfig {
    #[serde(default)]
    pub auto_rebase: bool,
    #[serde(default = "default_auto_abort_on_conflict")]
    pub auto_abort_on_conflict: bool,
}

fn default_auto_abort_on_conflict() -> bool {
    true
}

impl Default for ConflictConfig {
    fn default() -> Self {
        Self {
            auto_rebase: false,
            auto_abort_on_conflict: default_auto_abort_on_conflict(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrePushConfig {
    #[serde(default)]
    pub enabled: bool,
    pub command: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default = "default_pre_push_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_pre_push_timeout_secs() -> u64 {
    300
}

impl Default for PrePushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: None,
            required: true,
            timeout_secs: default_pre_push_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SafetyConfig {
    #[serde(default)]
    pub allow_force: bool,
    #[serde(default = "default_require_upstream_exist")]
    pub require_upstream_exist: bool,
    #[serde(default)]
    pub protect_branches: Vec<String>,
}

fn default_require_upstream_exist() -> bool {
    true
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allow_force: false,
            require_upstream_exist: default_require_upstream_exist(),
            protect_branches: vec!["main".to_string(), "master".to_string()],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_batch_min_commits")]
    pub min_commits: usize,
    #[serde(default = "default_batch_max_wait_secs")]
    pub max_wait_secs: u64,
    #[serde(default = "default_push_on_quit")]
    pub push_on_quit: bool,
}

fn default_batch_min_commits() -> usize {
    3
}
fn default_batch_max_wait_secs() -> u64 {
    300
}
fn default_push_on_quit() -> bool {
    true
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_commits: default_batch_min_commits(),
            max_wait_secs: default_batch_max_wait_secs(),
            push_on_quit: default_push_on_quit(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_min_commit_interval", with = "duration_secs")]
    pub min_commit_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            min_commit_interval: default_min_commit_interval(),
        }
    }
}

fn default_min_commit_interval() -> Duration {
    Duration::from_secs(10)
}

/// When to run the test hook for a cluster.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TestCommandOn {
    /// Run the test hook for every cluster.
    #[default]
    Always,
    /// Skip the test hook when every path in the cluster is documentation-only
    /// (md/txt/rst/adoc), which cannot break the build.
    CodeOnly,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestConfig {
    #[serde(default = "default_test_command")]
    pub command: Option<String>,
    #[serde(default = "default_true")]
    pub required: bool,
    /// When to run the test hook: "always" (default) or "code_only" (skip for
    /// docs-only clusters, keeping the gate cheap).
    #[serde(default)]
    pub command_on: TestCommandOn,
}

fn default_test_command() -> Option<String> {
    Some("cargo test".to_string())
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            command: default_test_command(),
            required: true,
            command_on: TestCommandOn::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotifyConfig {
    pub on_commit: Option<String>,
    pub on_error: Option<String>,
    pub on_push: Option<String>,
    pub on_start: Option<String>,
    pub on_shutdown: Option<String>,
    #[serde(default)]
    pub on_release: Option<String>,
    #[serde(default)]
    pub on_qualification: Option<String>,
    #[serde(default)]
    pub on_pulse: Option<String>,
    #[serde(default)]
    pub on_flaky_tests: Option<String>,
    pub webhook_url: Option<String>,
    /// Use nautical-themed emoji and phrasing for notifications.
    #[serde(default = "default_true")]
    pub nautical_theme: bool,
    /// Minimum seconds between duplicate event notifications (0 = no rate limit).
    #[serde(default = "default_notify_rate_limit_seconds")]
    pub rate_limit_seconds: u64,
    /// Optional text-to-speech configuration for spoken notifications.
    #[serde(default)]
    pub tts: TtsConfig,
}

fn default_notify_rate_limit_seconds() -> u64 {
    5
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            on_commit: None,
            on_error: None,
            on_push: None,
            on_start: None,
            on_shutdown: None,
            on_release: None,
            on_qualification: None,
            on_pulse: None,
            on_flaky_tests: None,
            webhook_url: None,
            nautical_theme: true,
            rate_limit_seconds: default_notify_rate_limit_seconds(),
            tts: TtsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilitiesConfig {
    #[serde(default = "default_true")]
    pub network_push: bool,
    #[serde(default = "default_true")]
    pub network_webhooks: bool,
    #[serde(default = "default_true")]
    pub network_inference: bool,
    #[serde(default = "default_true")]
    pub bundle_scoring: bool,
    #[serde(default = "default_true")]
    pub external_plugins: bool,
    /// Permit network-connected enterprise integrations. Disable this in
    /// inspection, air-gapped, or explicitly isolated deployments.
    #[serde(default = "default_true")]
    pub network_integrations: bool,
}

impl Default for CapabilitiesConfig {
    fn default() -> Self {
        Self {
            network_push: true,
            network_webhooks: true,
            network_inference: true,
            bundle_scoring: true,
            external_plugins: true,
            network_integrations: true,
        }
    }
}

/// `[integrations]` declares provider connectors without embedding provider
/// secrets. Each connector identifies an externally managed secret reference.
#[derive(Debug, Clone, Deserialize)]
pub struct IntegrationsConfig {
    #[serde(default)]
    pub connectors: Vec<crate::integrations::ConnectorConfig>,
    /// Run Hybreed and Emulsify around daemon commit/push operations.
    #[serde(default = "default_integrations_enabled")]
    pub enabled: bool,
    /// When enabled, an unavailable or failed analyzer blocks the push.
    #[serde(default)]
    pub required: bool,
    /// Executable used for branch-relationship analysis.
    #[serde(default = "default_hybreed_command")]
    pub hybreed_command: String,
    /// Executable used for multi-tree consolidation analysis.
    #[serde(default = "default_emulsify_command")]
    pub emulsify_command: String,
    /// Maximum time allotted to each external analysis command.
    #[serde(default = "default_integration_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for IntegrationsConfig {
    fn default() -> Self {
        Self {
            connectors: Vec::new(),
            enabled: default_integrations_enabled(),
            required: false,
            hybreed_command: default_hybreed_command(),
            emulsify_command: default_emulsify_command(),
            timeout_secs: default_integration_timeout_secs(),
        }
    }
}

fn default_integrations_enabled() -> bool {
    true
}

fn default_hybreed_command() -> String {
    "hybreed".to_owned()
}

fn default_emulsify_command() -> String {
    "emulsify".to_owned()
}

fn default_integration_timeout_secs() -> u64 {
    120
}

// ---------------------------------------------------------------------------
// Regional compliance and data-egress policy
// ---------------------------------------------------------------------------

/// Regional governance profiles. Profiles are additive: when several are
/// selected, the most restrictive egress rule applies. Selecting no profile is
/// deliberately backwards compatible and does not change existing behaviour.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RegionalProfile {
    EuEea,
    Uk,
    UsCalifornia,
    Canada,
    Brazil,
    India,
    Japan,
    China,
    /// Customer-controlled/local deployment with no external data egress.
    Sovereign,
}

/// How a category of repository-derived data may leave Kaptaind.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EgressPolicy {
    /// Preserve historical behaviour. Use only with no regional profile.
    #[default]
    Allow,
    /// Permit only destinations named in `allowed_hosts`.
    ApprovedOnly,
    /// Refuse this category of egress.
    Deny,
}

/// Categories of outbound repository-derived data. Kept public so transport
/// call sites can enforce the same policy after config validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressChannel {
    Inference,
    Webhooks,
    Integrations,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataEgressConfig {
    #[serde(default)]
    pub inference: EgressPolicy,
    #[serde(default)]
    pub webhooks: EgressPolicy,
    #[serde(default)]
    pub integrations: EgressPolicy,
    /// Audit export is currently a local JSONL file sink. `local_only` keeps
    /// it under `repo_path`; `deny` rejects configured mirrors entirely.
    #[serde(default)]
    pub audit_export: AuditEgressPolicy,
    /// Exact DNS host names approved for inference and webhook egress.
    /// Entries are compared case-insensitively; wildcards are intentionally
    /// unsupported so a profile cannot accidentally approve a whole domain.
    #[serde(default)]
    pub allowed_hosts: BTreeSet<String>,
}

impl Default for DataEgressConfig {
    fn default() -> Self {
        Self {
            inference: EgressPolicy::Allow,
            webhooks: EgressPolicy::Allow,
            integrations: EgressPolicy::Allow,
            audit_export: AuditEgressPolicy::Allow,
            allowed_hosts: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuditEgressPolicy {
    #[default]
    Allow,
    LocalOnly,
    Deny,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ComplianceConfig {
    /// Additive regional governance profiles. Empty means no profile selected.
    #[serde(default)]
    pub profiles: BTreeSet<RegionalProfile>,
    #[serde(default)]
    pub egress: DataEgressConfig,
}

// ---------------------------------------------------------------------------
// Configuration trust boundary
// ---------------------------------------------------------------------------

/// `[trust]` block: whether this configuration is allowed to launch programs.
///
/// Kaptaind configuration can name test hooks, notification hooks, bundle
/// commands, language-adapter plugins, and Angler bait/hook programs. Treat a
/// configuration obtained from an unreviewed repository as data, not authority
/// to execute those programs. Repository configuration must opt in explicitly.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTrust {
    /// Permit configured program execution after operator review.
    Trusted,
    /// Refuse configuration entries that can execute a local program.
    #[default]
    Untrusted,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TrustConfig {
    /// Execution authority for commands declared in this configuration.
    ///
    /// Set `execution = "untrusted"` before inspecting a configuration from a
    /// cloned or otherwise unreviewed repository. After review, explicitly set
    /// `execution = "trusted"` to enable its hooks and plugins.
    #[serde(default)]
    pub execution: ExecutionTrust,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BundleConfig {
    pub command: Option<String>,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
}

fn default_output_dir() -> String {
    "dist".to_string()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagingMode {
    /// Stage every modified file (`git add -A`).
    ///
    /// WARNING: in a shared or monorepo checkout this sweeps the whole
    /// worktree, including untracked files. Kept for backwards compatibility;
    /// the daemon logs a loud startup warning when this mode is active, and
    /// commits abort fail-closed if any changed path matches the built-in
    /// secret denylist.
    All,
    /// Only stage files that were part of the detected cluster, plus the
    /// version metadata files (VERSION, Cargo.toml). This is the safe
    /// default: an autonomous committer must never stage files it did not
    /// observe changing.
    #[default]
    Cluster,
    /// Stage files matching include patterns, skip exclude patterns
    Pattern,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StagingConfig {
    #[serde(default)]
    pub mode: StagingMode,
    /// Glob patterns for files to include (only used in Pattern mode)
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns for files to always exclude from staging
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// `[web]` configuration for the optional WebUI server.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WebConfig {
    /// Bearer token required by the WebUI. When unset (or empty), a random
    /// token is generated at startup and printed once to stderr.
    #[serde(default)]
    pub auth_token: Option<String>,
    /// Allow `POST /api/config` to rewrite `kaptaind.toml`. Disabled by default;
    /// enabling it exposes a config-write endpoint and is not recommended.
    #[serde(default)]
    pub allow_config_write: bool,
}

/// `[commit]` configuration for git commit behavior.
#[derive(Debug, Clone, Deserialize)]
pub struct CommitConfig {
    /// Sign commits with GPG (`git commit -S`).
    #[serde(default)]
    pub sign: bool,
    /// Optional GPG key ID or email to use for signing.
    #[serde(default)]
    pub gpg_key_id: Option<String>,
    /// Require a semantic version bump for a cluster to be committed (D1).
    ///
    /// When `true` (the pre-v10 default), below-threshold clusters are logged
    /// as `no_bump` and left uncommitted. When `false` — the default since
    /// v10.0.0 — they are captured with a non-bumping `chore:` commit
    /// instead, leaving VERSION, Cargo.toml and Cargo.lock untouched. The
    /// default flipped in v10.0.0 so work is never silently lost (#7) and
    /// version only moves on threshold-crossing clusters (#17).
    #[serde(default = "default_false")]
    pub require_bump: bool,
    /// `[commit.orb_sanitize]` — redact sensitive values from staged
    /// `.orb` files before they land in a commit a public remote can see.
    #[serde(default)]
    pub orb_sanitize: OrbSanitizeConfig,
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            sign: false,
            gpg_key_id: None,
            require_bump: false,
            orb_sanitize: OrbSanitizeConfig::default(),
        }
    }
}

/// `[commit.orb_sanitize]` configuration. See `angler::orb_sanitize` for
/// the mechanism: sensitive values are redacted in the STAGED content of
/// any `.orb` file only — the working tree file is never touched, so a
/// developer's local `.orb` can carry whatever config they need.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct OrbSanitizeConfig {
    /// Master switch. On by default: an `.orb` file is meant to be shared,
    /// and the cost of checking a handful of staged files for sensitive
    /// key names is negligible next to the cost of a leaked credential.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// When the target remote's visibility can't be confirmed (`gh`
    /// unavailable, not a GitHub remote, network failure), sanitize
    /// anyway rather than skip — fail closed, not open. Setting this to
    /// `false` restricts sanitization to remotes explicitly confirmed
    /// public, which is more permissive and not the recommended setting.
    #[serde(default = "default_true")]
    pub sanitize_when_visibility_unknown: bool,
    /// Extra key-name fragments (case-insensitive substring match, same
    /// as the built-in list) that mark a value as sensitive, beyond the
    /// built-in password/secret/token/api_key/credential/... set.
    #[serde(default)]
    pub extra_sensitive_key_fragments: Vec<String>,
}

impl Default for OrbSanitizeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sanitize_when_visibility_unknown: true,
            extra_sensitive_key_fragments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ValidationMode {
    /// Single provider call (Anthropic → OpenAI → Ollama). Lowest latency.
    #[default]
    Fast,
    /// Parallel multi-model Ollama calls with semantic cross-comparison.
    Consensus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InferenceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_inference_provider")]
    pub provider: String,
    #[serde(default = "default_inference_model")]
    pub model: String,
    #[serde(default = "default_inference_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_ollama_base_url")]
    pub ollama_base_url: String,
    #[serde(default)]
    pub validation_mode: ValidationMode,
    #[serde(default = "default_consensus_models")]
    pub consensus_models: Vec<String>,
    #[serde(default = "default_consensus_threshold")]
    pub consensus_threshold: f64,
    #[serde(default = "default_consensus_min_agreement")]
    pub consensus_min_agreement: usize,
    /// Minimum combined diff score required before the LLM is invoked.
    /// Clusters scoring below this threshold use the deterministic message only.
    /// Default 0.0 (always invoke when inference.enabled = true).
    #[serde(default)]
    pub min_score_for_inference: f64,
    // -----------------------------------------------------------------------
    // Kimi-specific configuration
    // -----------------------------------------------------------------------
    /// Kimi endpoint selection: "global", "china", "coding", or None for auto
    #[serde(default)]
    pub kimi_endpoint: Option<String>,
    /// Override base URL for Kimi API (advanced use)
    #[serde(default)]
    pub kimi_base_url: Option<String>,
    /// Kimi model to use (e.g., "kimi-k2.5", "kimi-for-coding", "kimi-k2-thinking")
    #[serde(default)]
    pub kimi_model: String,
    /// Enable thinking mode for reasoning models (adds reasoning_content to responses)
    #[serde(default)]
    pub kimi_thinking: bool,
    /// Enable extended context mode for K2.5 (up to 2M tokens)
    #[serde(default)]
    pub kimi_extended_context: bool,
    /// Route inference only through a configured Cosine Lumen endpoint. This
    /// is intended for UK-controlled deployments; Kaptaind cannot itself
    /// attest to the endpoint's residency or contractual compliance.
    #[serde(default)]
    pub uk_compliance_mode: bool,
    /// OpenAI-compatible base URL for a controlled Cosine Lumen Outpost
    /// deployment (for example vLLM or SGLang). No public default is assumed.
    #[serde(default)]
    pub cosine_base_url: Option<String>,
    /// Model name exposed by the controlled Cosine endpoint.
    #[serde(default = "default_cosine_model")]
    pub cosine_model: String,
}

fn default_inference_provider() -> String {
    "auto".to_string()
}

fn default_inference_model() -> String {
    "auto".to_string()
}

fn default_inference_timeout_secs() -> u64 {
    15
}

fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_consensus_models() -> Vec<String> {
    vec!["llama3.2".to_string()]
}

fn default_consensus_threshold() -> f64 {
    0.6
}

fn default_consensus_min_agreement() -> usize {
    2
}

fn default_cosine_model() -> String {
    "lumen-outpost".to_string()
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_inference_provider(),
            model: default_inference_model(),
            timeout_secs: default_inference_timeout_secs(),
            ollama_base_url: default_ollama_base_url(),
            validation_mode: ValidationMode::default(),
            consensus_models: default_consensus_models(),
            consensus_threshold: default_consensus_threshold(),
            consensus_min_agreement: default_consensus_min_agreement(),
            min_score_for_inference: 0.0,
            kimi_endpoint: None,
            kimi_base_url: None,
            kimi_model: String::new(),
            kimi_thinking: false,
            kimi_extended_context: false,
            uk_compliance_mode: false,
            cosine_base_url: None,
            cosine_model: default_cosine_model(),
        }
    }
}

// ---------------------------------------------------------------------------
// VACS config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct VacsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_vacs_mode")]
    pub mode: String,
    #[serde(default = "default_vacs_allowed_assets")]
    pub allowed_assets: Vec<String>,
    #[serde(default)]
    pub video_enabled: bool,
    #[serde(default = "default_vacs_max_jobs_per_hour")]
    pub max_jobs_per_hour: u32,
}

fn default_vacs_mode() -> String {
    "balanced".to_string()
}
fn default_vacs_allowed_assets() -> Vec<String> {
    vec!["diagram".to_string(), "chart".to_string()]
}
fn default_vacs_max_jobs_per_hour() -> u32 {
    5
}

impl Default for VacsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: default_vacs_mode(),
            allowed_assets: default_vacs_allowed_assets(),
            video_enabled: false,
            max_jobs_per_hour: default_vacs_max_jobs_per_hour(),
        }
    }
}

// ---------------------------------------------------------------------------
// Trawling config
// ---------------------------------------------------------------------------

/// `[trawl]` block in `kaptaind.toml`.
/// Configures codebase discovery and initialization for `kaptaind-cli trawl`.
#[derive(Debug, Clone, Deserialize)]
pub struct TrawlConfig {
    /// Root directory to trawl from (default: parent of repo_path)
    #[serde(default)]
    pub root: Option<PathBuf>,
    /// Maximum depth to search (default: 3)
    #[serde(default = "default_trawl_depth")]
    pub max_depth: usize,
    /// Skip already initialized projects
    #[serde(default = "default_trawl_skip_initialized")]
    pub skip_initialized: bool,
    /// Only process git repositories
    #[serde(default)]
    pub require_git: bool,
    /// Auto-register discovered projects for monitoring
    #[serde(default = "default_trawl_auto_register")]
    pub auto_register: bool,
    /// Project types to look for (empty = all)
    #[serde(default)]
    pub project_types: Vec<String>,
    /// Extra directory names or globs to skip (layered on the built-in list and ignore files)
    #[serde(default)]
    pub blacklist: Vec<String>,
    /// Honor .gitignore/.ignore files while trawling (default: true)
    #[serde(default = "default_trawl_respect_ignore")]
    pub respect_ignore_files: bool,
    /// Also initialize Cargo workspace member crates (default: false)
    #[serde(default)]
    pub expand_workspaces: bool,
}

fn default_trawl_depth() -> usize {
    3
}
fn default_trawl_skip_initialized() -> bool {
    true
}
fn default_trawl_auto_register() -> bool {
    true
}
fn default_trawl_respect_ignore() -> bool {
    true
}

impl Default for TrawlConfig {
    fn default() -> Self {
        Self {
            root: None,
            max_depth: default_trawl_depth(),
            skip_initialized: default_trawl_skip_initialized(),
            require_git: false,
            auto_register: default_trawl_auto_register(),
            project_types: Vec::new(),
            blacklist: Vec::new(),
            respect_ignore_files: default_trawl_respect_ignore(),
            expand_workspaces: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Storage management (deckhand) config
// ---------------------------------------------------------------------------

/// `[deckhand]` block in `kaptaind.toml`.
/// Configures automatic Cargo workspace storage hygiene.
#[derive(Debug, Clone, Deserialize)]
pub struct DeckhandConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_deckhand_interval_minutes")]
    pub interval_minutes: u64,
    #[serde(default = "default_deckhand_sweep_keep_days")]
    pub sweep_keep_days: u64,
    #[serde(default = "default_deckhand_clean_profiles")]
    pub clean_profiles: Vec<String>,
    #[serde(default)]
    pub clean_older_than_days: Option<u64>,
    #[serde(default = "default_false")]
    pub dry_run: bool,
    #[serde(default = "default_deckhand_min_free_percent")]
    pub min_free_percent: u64,
}

fn default_deckhand_interval_minutes() -> u64 {
    360
}

fn default_deckhand_sweep_keep_days() -> u64 {
    30
}

fn default_deckhand_clean_profiles() -> Vec<String> {
    vec!["debug".to_string()]
}

fn default_deckhand_min_free_percent() -> u64 {
    10
}

fn default_false() -> bool {
    false
}

impl Default for DeckhandConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: default_deckhand_interval_minutes(),
            sweep_keep_days: default_deckhand_sweep_keep_days(),
            clean_profiles: default_deckhand_clean_profiles(),
            clean_older_than_days: None,
            dry_run: false,
            min_free_percent: default_deckhand_min_free_percent(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shark Stating (high availability / zero-downtime upgrades) config
// ---------------------------------------------------------------------------

/// `[shark]` block in `kaptaind.toml`.
/// Configures crash-only dual-instance leadership with a file-based arbiter.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharkMode {
    /// Attempt to become leader if no healthy leader exists (default).
    #[default]
    Auto,
    /// Force leadership attempt on startup.
    Leader,
    /// Never attempt leadership; remain standby/observer.
    Standby,
    /// Monitor only; never take over.
    Observer,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SharkConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_shark_arbiter_path")]
    pub arbiter_path: PathBuf,
    #[serde(default = "default_shark_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,
    #[serde(default = "default_shark_heartbeat_timeout_ms")]
    pub heartbeat_timeout_ms: u64,
    #[serde(default = "default_shark_lease_ttl_ms")]
    pub lease_ttl_ms: u64,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default = "default_shark_upgrade_handoff_timeout_ms")]
    pub upgrade_handoff_timeout_ms: u64,
    #[serde(default)]
    pub mode: SharkMode,
}

fn default_shark_arbiter_path() -> PathBuf {
    PathBuf::from(".kaptaind/shark")
}

fn default_shark_heartbeat_interval_ms() -> u64 {
    1000
}

fn default_shark_heartbeat_timeout_ms() -> u64 {
    5000
}

fn default_shark_lease_ttl_ms() -> u64 {
    10000
}

fn default_shark_upgrade_handoff_timeout_ms() -> u64 {
    30000
}

impl Default for SharkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            arbiter_path: default_shark_arbiter_path(),
            heartbeat_interval_ms: default_shark_heartbeat_interval_ms(),
            heartbeat_timeout_ms: default_shark_heartbeat_timeout_ms(),
            lease_ttl_ms: default_shark_lease_ttl_ms(),
            instance_id: None,
            upgrade_handoff_timeout_ms: default_shark_upgrade_handoff_timeout_ms(),
            mode: SharkMode::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// RBAC config
// ---------------------------------------------------------------------------

/// `[rbac]` block in `kaptaind.toml`.
///
/// Fine-grained access control for multi-user installs. When enabled, CLI
/// commands and daemon startup check the current OS user against role
/// assignments.
#[derive(Debug, Clone, Deserialize)]
pub struct RbacConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub roles: Vec<RbacRoleConfig>,
}

impl Default for RbacConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            roles: vec![RbacRoleConfig {
                name: "admin".to_string(),
                permissions: vec!["*".to_string()],
                users: Vec::new(),
                groups: Vec::new(),
            }],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RbacRoleConfig {
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

/// `[policy_trust]` controls verification of repository policy packs.
///
/// Production deployments should require detached GPG signatures from a
/// dedicated, offline-managed policy signing key. The compatibility default
/// preserves existing unsigned local policy files until an operator opts in.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyTrustConfig {
    #[serde(default)]
    pub require_signature: bool,
    /// Keyring accepted by `gpgv`; relative paths resolve under `repo_path`.
    #[serde(default)]
    pub gpgv_keyring: Option<PathBuf>,
}

/// `[identity]` selects the identity evidence used for protected approval
/// actions. `gpg_signed_assertion` accepts a short-lived, detached-signature
/// assertion emitted by an IdP or CI identity broker.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdentityMode {
    #[default]
    OperatingSystem,
    GpgSignedAssertion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentityConfig {
    #[serde(default)]
    pub mode: IdentityMode,
    #[serde(default)]
    pub gpgv_keyring: Option<PathBuf>,
    #[serde(default)]
    pub assertion_path: Option<PathBuf>,
    /// Durable directory used to reject replayed signed assertion IDs.
    #[serde(default = "default_identity_replay_dir")]
    pub replay_dir: PathBuf,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default = "default_identity_assertion_age_seconds")]
    pub max_assertion_age_seconds: u64,
}

fn default_identity_assertion_age_seconds() -> u64 {
    900
}

fn default_identity_replay_dir() -> PathBuf {
    PathBuf::from(".kaptaind/identity/replay")
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            mode: IdentityMode::OperatingSystem,
            gpgv_keyring: None,
            assertion_path: None,
            replay_dir: default_identity_replay_dir(),
            issuer: None,
            audience: None,
            max_assertion_age_seconds: default_identity_assertion_age_seconds(),
        }
    }
}

/// `[governance]` declares an organization/tenant scope and can enforce the
/// minimum controls needed for governed enterprise operation.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GovernanceConfig {
    /// Stable organization identifier used in policy and audit correlation.
    #[serde(default)]
    pub organization_id: Option<String>,
    /// Stable tenant or business-unit identifier within the organization.
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// When true, fail configuration validation unless RBAC and signed policy
    /// packs are enabled. This turns governance into an explicit posture,
    /// rather than a collection of optional conventions.
    #[serde(default)]
    pub enforce_enterprise_controls: bool,
}

impl Config {
    fn validate_governance_identifier(name: &str, value: &str) -> anyhow::Result<()> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '_' || character == '-'
            });
        if !valid {
            anyhow::bail!("governance.{name} must be 1-64 ASCII letters, digits, '_' or '-'");
        }
        Ok(())
    }

    fn validate_governance(&self) -> anyhow::Result<()> {
        if let Some(id) = &self.governance.organization_id {
            Self::validate_governance_identifier("organization_id", id)?;
        }
        if let Some(id) = &self.governance.tenant_id {
            Self::validate_governance_identifier("tenant_id", id)?;
        }
        if self.governance.enforce_enterprise_controls {
            if self.governance.organization_id.is_none() || self.governance.tenant_id.is_none() {
                anyhow::bail!("enterprise governance requires organization_id and tenant_id");
            }
            if !self.rbac.enabled {
                anyhow::bail!("enterprise governance requires [rbac].enabled = true");
            }
            if !self.audit.enabled {
                anyhow::bail!("enterprise governance requires [audit].enabled = true");
            }
            if self.trust.execution != ExecutionTrust::Trusted {
                anyhow::bail!(
                    "enterprise governance requires [trust].execution = \"trusted\" after configuration review"
                );
            }
            if !self.test.required
                || self
                    .test
                    .command
                    .as_deref()
                    .is_none_or(|command| command.trim().is_empty())
                || self.test.command_on != TestCommandOn::Always
            {
                anyhow::bail!(
                    "enterprise governance requires a non-empty mandatory test.command with test.command_on = \"always\""
                );
            }
            if !self.commit.sign {
                anyhow::bail!("enterprise governance requires [commit].sign = true");
            }
            if self.push.enabled
                && (!self.push.protection.require_ci_pass
                    || self.push.protection.required_status_checks.is_empty())
            {
                anyhow::bail!(
                    "enterprise governance requires CI-protected pushes with required status checks"
                );
            }
            if self.ship.enabled
                && (!self.ship.require_qualification
                    || !self.ship.sign
                    || !self.ship.sbom.enabled
                    || !self.ship.provenance.enabled)
            {
                anyhow::bail!(
                    "enterprise governance requires qualified, signed releases with SBOM and provenance"
                );
            }
            if self.policy_id.is_none() {
                anyhow::bail!("enterprise governance requires policy_id");
            }
            if !self.policy_trust.require_signature || self.policy_keyring_path().is_none() {
                anyhow::bail!("enterprise governance requires signed policy packs and policy_trust.gpgv_keyring");
            }
            if self.identity.mode != IdentityMode::GpgSignedAssertion
                || self.identity_keyring_path().is_none()
                || self.identity_assertion_path().is_none()
                || self.identity.replay_dir.as_os_str().is_empty()
                || self.identity.issuer.as_deref().is_none_or(str::is_empty)
                || self.identity.audience.as_deref().is_none_or(str::is_empty)
                || !(60..=3600).contains(&self.identity.max_assertion_age_seconds)
            {
                anyhow::bail!("enterprise governance requires a 60-3600 second gpg_signed_assertion identity configuration with keyring, assertion path, issuer, and audience");
            }
            let export_path = self
                .audit
                .export
                .as_ref()
                .and_then(|export| export.jsonl_path.as_ref())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "enterprise governance requires an independent [audit.export].jsonl_path"
                    )
                })?;
            if export_path == &crate::audit::default_path(&self.repo_path) {
                anyhow::bail!(
                    "enterprise governance audit export must not reuse the primary audit path"
                );
            }
            crate::audit::verify_chain(&self.repo_path).map_err(|error| {
                anyhow::anyhow!("enterprise governance rejected audit integrity state: {error}")
            })?;
        }
        Ok(())
    }

    pub fn policy_keyring_path(&self) -> Option<PathBuf> {
        self.policy_trust.gpgv_keyring.as_ref().map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                self.repo_path.join(path)
            }
        })
    }
    pub fn identity_keyring_path(&self) -> Option<PathBuf> {
        self.identity.gpgv_keyring.as_ref().map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                self.repo_path.join(path)
            }
        })
    }
    pub fn identity_assertion_path(&self) -> Option<PathBuf> {
        self.identity.assertion_path.as_ref().map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                self.repo_path.join(path)
            }
        })
    }
    pub fn identity_replay_dir(&self) -> PathBuf {
        if self.identity.replay_dir.is_absolute() {
            self.identity.replay_dir.clone()
        } else {
            self.repo_path.join(&self.identity.replay_dir)
        }
    }
    /// Resolve the shark arbiter path relative to the repo root.
    pub fn shark_arbiter_path(&self) -> PathBuf {
        if self.shark.arbiter_path.is_absolute() {
            self.shark.arbiter_path.clone()
        } else {
            self.repo_path.join(&self.shark.arbiter_path)
        }
    }

    /// Stable identifier for this kaptaind instance.
    pub fn shark_instance_id(&self) -> String {
        self.shark
            .instance_id
            .clone()
            .unwrap_or_else(default_instance_id)
    }

    /// Returns the configuration fields that would launch a local program.
    ///
    /// Callers that accept configuration from an unreviewed source can use
    /// this to present a review prompt before opting into
    /// `[trust].execution = "trusted"`.
    pub fn execution_trust_violations(&self) -> Vec<&'static str> {
        if self.trust.execution == ExecutionTrust::Trusted {
            return Vec::new();
        }

        let mut violations = Vec::new();
        if self.test.command.is_some() {
            violations.push("test.command");
        }
        if self.bundle.command.is_some() {
            violations.push("bundle.command");
        }
        if self.build.command.is_some() {
            violations.push("build.command");
        }
        if self.push.pre_push.enabled && self.push.pre_push.command.is_some() {
            violations.push("push.pre_push.command");
        }
        if !self.plugins.adapters.is_empty() {
            violations.push("plugins.adapters");
        }

        let notification_hooks = [
            self.notify.on_commit.as_ref(),
            self.notify.on_error.as_ref(),
            self.notify.on_push.as_ref(),
            self.notify.on_start.as_ref(),
            self.notify.on_shutdown.as_ref(),
            self.notify.on_release.as_ref(),
            self.notify.on_qualification.as_ref(),
            self.notify.on_pulse.as_ref(),
            self.notify.on_flaky_tests.as_ref(),
        ];
        if notification_hooks.iter().any(Option::is_some) {
            violations.push("notify.on_*");
        }

        let hooks = &self.angler.git_hooks;
        if hooks.enabled
            && (hooks.pre_commit.is_some()
                || hooks.prepare_commit_msg.is_some()
                || hooks.commit_msg.is_some()
                || hooks.post_commit.is_some()
                || hooks.pre_push.is_some()
                || hooks.post_push.is_some()
                || hooks.post_checkout.is_some()
                || hooks.post_merge.is_some()
                || !hooks.custom.is_empty())
        {
            violations.push("angler.git_hooks");
        }
        if self.angler.bait.enabled
            && (self.angler.bait.auto_discover || !self.angler.bait.baits.is_empty())
        {
            violations.push("angler.bait");
        }

        violations
    }

    /// Fails closed when an untrusted configuration tries to authorize program
    /// execution. This is intentionally separate from TOML parsing so status
    /// and review tools can inspect an untrusted configuration without running
    /// anything; daemon/automation entry points must call `validate()` first.
    pub fn validate_execution_trust(&self) -> anyhow::Result<()> {
        let violations = self.execution_trust_violations();
        if !violations.is_empty() {
            anyhow::bail!(
                "untrusted configuration cannot enable program execution ({}) — review the configuration and set [trust].execution = \"trusted\" to opt in",
                violations.join(", ")
            );
        }
        Ok(())
    }
}

fn default_instance_id() -> String {
    let pid = std::process::id();
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string());
    format!("{}@{}", pid, host)
}

fn gpg_available() -> bool {
    std::process::Command::new("gpg")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl Config {
    /// Return whether a configured regional policy permits an outbound URL for
    /// a particular data channel. Runtime transport code should call this
    /// immediately before its existing SSRF/TLS validation.
    pub fn allows_egress_url(&self, channel: EgressChannel, raw_url: &str) -> anyhow::Result<()> {
        let policy = match channel {
            EgressChannel::Inference => self.compliance.egress.inference,
            EgressChannel::Webhooks => self.compliance.egress.webhooks,
            EgressChannel::Integrations => self.compliance.egress.integrations,
        };
        if policy == EgressPolicy::Deny {
            anyhow::bail!("compliance policy denies {:?} data egress", channel);
        }
        if policy == EgressPolicy::ApprovedOnly {
            let host = Url::parse(raw_url)
                .map_err(|error| {
                    anyhow::anyhow!("invalid configured egress URL {raw_url:?}: {error}")
                })?
                .host_str()
                .ok_or_else(|| anyhow::anyhow!("configured egress URL has no host: {raw_url:?}"))?
                .to_ascii_lowercase();
            if !self.compliance.egress.allowed_hosts.contains(&host) {
                anyhow::bail!(
                    "compliance policy does not approve {channel:?} host {host:?}; add its exact hostname to [compliance.egress].allowed_hosts"
                );
            }
        }
        Ok(())
    }

    fn validate_compliance_profile(&self) -> anyhow::Result<()> {
        let profiles = &self.compliance.profiles;
        if profiles.is_empty() {
            return Ok(());
        }

        let egress = &self.compliance.egress;
        let sovereign = profiles.contains(&RegionalProfile::Sovereign);
        if sovereign
            && (egress.inference != EgressPolicy::Deny
                || egress.webhooks != EgressPolicy::Deny
                || egress.integrations != EgressPolicy::Deny
                || egress.audit_export != AuditEgressPolicy::LocalOnly)
        {
            anyhow::bail!(
                "sovereign compliance profile requires inference = \"deny\", webhooks = \"deny\", integrations = \"deny\", and audit_export = \"local_only\""
            );
        }

        // A profile is an explicit governance opt-in, so external traffic may
        // not retain the permissive compatibility default. Operators must
        // either deny a channel or name its approved destinations.
        for (name, policy, active) in [
            (
                "inference",
                egress.inference,
                self.inference.enabled && self.capabilities.network_inference,
            ),
            (
                "webhooks",
                egress.webhooks,
                self.has_configured_webhooks() && self.capabilities.network_webhooks,
            ),
            (
                "integrations",
                egress.integrations,
                self.has_enabled_integrations() && self.capabilities.network_integrations,
            ),
        ] {
            if active && policy == EgressPolicy::Deny {
                anyhow::bail!(
                    "compliance policy denies {name} data egress, but {name} remains enabled; disable the feature/capability before startup"
                );
            }
            if active && policy == EgressPolicy::Allow {
                anyhow::bail!(
                    "regional compliance profiles require [compliance.egress].{name} = \"approved_only\" or \"deny\" when {name} is enabled"
                );
            }
            if active && policy == EgressPolicy::ApprovedOnly && egress.allowed_hosts.is_empty() {
                anyhow::bail!(
                    "[compliance.egress].allowed_hosts must name at least one exact hostname when {name} uses approved_only"
                );
            }
        }

        if egress.audit_export == AuditEgressPolicy::Deny && self.audit.export.is_some() {
            anyhow::bail!(
                "compliance policy denies audit export, but [audit].export is configured"
            );
        }
        if egress.audit_export == AuditEgressPolicy::LocalOnly {
            if let Some(path) = self
                .audit
                .export
                .as_ref()
                .and_then(|export| export.jsonl_path.as_ref())
            {
                if !path.starts_with(&self.repo_path) {
                    anyhow::bail!(
                        "local_only audit export must remain under repo_path; configured path is {path:?}"
                    );
                }
            }
        }

        if profiles.contains(&RegionalProfile::Uk)
            && self.inference.enabled
            && (!(self.inference.provider == "cosine"
                || (self.inference.provider == "auto" && self.inference.uk_compliance_mode))
                || self.inference.cosine_base_url.is_none())
        {
            anyhow::bail!(
                "UK compliance profile requires inference.provider = \"cosine\" or uk_compliance_mode with provider = \"auto\", plus inference.cosine_base_url"
            );
        }

        if self.inference.enabled && egress.inference != EgressPolicy::Deny {
            for url in self.configured_inference_urls()? {
                self.allows_egress_url(EgressChannel::Inference, url)?;
            }
        }
        if self.has_configured_webhooks() && egress.webhooks != EgressPolicy::Deny {
            for url in self.configured_webhook_urls() {
                self.allows_egress_url(EgressChannel::Webhooks, url)?;
            }
        }
        if self.has_enabled_integrations() && egress.integrations != EgressPolicy::Deny {
            for connector in &self.integrations.connectors {
                if connector.mode == crate::integrations::Mode::Disabled {
                    continue;
                }
                self.allows_egress_url(
                    EgressChannel::Integrations,
                    crate::integrations::endpoint(connector)?,
                )?;
            }
        }
        Ok(())
    }

    fn has_enabled_integrations(&self) -> bool {
        self.integrations
            .connectors
            .iter()
            .any(|connector| connector.mode != crate::integrations::Mode::Disabled)
    }

    fn validate_integrations(&self) -> anyhow::Result<()> {
        let mut configured = BTreeSet::new();
        for connector in &self.integrations.connectors {
            connector.validate()?;
            let key = (connector.provider, connector.tenant_id.as_str());
            if !configured.insert(key) {
                anyhow::bail!(
                    "integration {} is configured more than once for tenant {:?}",
                    connector.provider,
                    connector.tenant_id
                );
            }
            if connector.mode != crate::integrations::Mode::Disabled
                && !self.capabilities.network_integrations
            {
                anyhow::bail!(
                    "integration {} is enabled but capabilities.network_integrations is false",
                    connector.provider
                );
            }
        }
        Ok(())
    }

    fn has_configured_webhooks(&self) -> bool {
        self.notify.webhook_url.is_some()
            || (self.angler.webhooks.enabled && !self.angler.webhooks.endpoints.is_empty())
    }

    fn configured_webhook_urls(&self) -> Vec<&str> {
        self.notify
            .webhook_url
            .iter()
            .map(String::as_str)
            .chain(
                self.angler
                    .webhooks
                    .endpoints
                    .iter()
                    .map(|endpoint| endpoint.url.as_str()),
            )
            .collect()
    }

    fn configured_inference_urls(&self) -> anyhow::Result<Vec<&str>> {
        match self.inference.provider.as_str() {
            "cosine" => self.inference.cosine_base_url.as_deref().map(|url| vec![url]).ok_or_else(|| anyhow::anyhow!("inference.provider = \"cosine\" requires inference.cosine_base_url")),
            "ollama" => Ok(vec![self.inference.ollama_base_url.as_str()]),
            // UK mode resolves `auto` to Cosine in the inference module.
            "auto" if self.inference.uk_compliance_mode => self.inference.cosine_base_url.as_deref().map(|url| vec![url]).ok_or_else(|| anyhow::anyhow!("UK compliance inference requires inference.cosine_base_url")),
            // Other providers use fixed provider URLs or environment-specific
            // endpoints; they cannot be attested by a config-only policy.
            other => anyhow::bail!(
                "regional compliance inference requires provider \"cosine\" or \"ollama\" with an explicit configured endpoint; provider {other:?} cannot be host-attested"
            ),
        }
    }

    /// Validate cross-field constraints and invariants.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(policy_id) = &self.policy_id {
            crate::daemon::policy::validate_policy_id(policy_id)?;
        }
        self.validate_execution_trust()?;
        self.validate_integrations()?;
        self.validate_compliance_profile()?;
        self.validate_governance()?;

        if self.inference.timeout_secs == 0 {
            anyhow::bail!("inference.timeout_secs must be greater than 0");
        }
        if self.inference.uk_compliance_mode {
            if !matches!(self.inference.provider.as_str(), "auto" | "cosine") {
                anyhow::bail!(
                    "UK compliance mode requires inference.provider = \"auto\" or \"cosine\""
                );
            }
            if self.inference.enabled && self.inference.cosine_base_url.is_none() {
                anyhow::bail!(
                    "UK compliance mode requires inference.cosine_base_url for the approved Cosine Lumen deployment"
                );
            }
        }
        if self.build.timeout_secs == 0 {
            anyhow::bail!("build.timeout_secs must be greater than 0");
        }
        if self.push.retry.attempt_timeout_secs == 0 {
            anyhow::bail!("push.retry.attempt_timeout_secs must be greater than 0");
        }
        if !matches!(
            self.pull.default_strategy.as_str(),
            "auto" | "fast-forward" | "merge" | "rebase" | "hybreed" | "emulsify" | "manual"
        ) {
            anyhow::bail!(
                "pull.default_strategy must be auto, fast-forward, merge, rebase, hybreed, emulsify, or manual"
            );
        }
        if !matches!(self.pull.tags.as_str(), "follow" | "all" | "none") {
            anyhow::bail!("pull.tags must be follow, all, or none");
        }
        if !(0.0..=1.0).contains(&self.pull.critical_risk_threshold) {
            anyhow::bail!("pull.critical_risk_threshold must be between 0.0 and 1.0");
        }
        if self.health_port == 0 {
            anyhow::bail!("health_port must be greater than 0");
        }

        if self.shark.enabled {
            let heartbeat = self.shark.heartbeat_interval_ms;
            let ttl = self.shark.lease_ttl_ms;
            if ttl < heartbeat.saturating_mul(3) {
                anyhow::bail!(
                    "shark.lease_ttl_ms ({}) must be at least 3x heartbeat_interval_ms ({})",
                    ttl,
                    heartbeat
                );
            }
        }

        if self.air_gapped
            && (self.capabilities.network_push
                || self.capabilities.network_webhooks
                || self.capabilities.network_inference)
        {
            anyhow::bail!(
                "air_gapped=true is incompatible with network_push, network_webhooks, or network_inference capabilities"
            );
        }

        if self.ship.enabled {
            let signing_requested = self.ship.sign
                || self.ship.nightly.sign == Some(true)
                || self.ship.stable.sign == Some(true);
            if signing_requested && !gpg_available() {
                tracing::warn!(
                    component = module_path!(),
                    "ship signing is enabled but gpg is not available; signing will fail at runtime"
                );
            }

            if self.ship.auto_nightly.enabled {
                crate::schedule::validate_schedule(&self.ship.auto_nightly.schedule)
                    .map_err(|e| anyhow::anyhow!("ship.auto_nightly.schedule is invalid: {}", e))?;
                if !matches!(
                    self.ship
                        .auto_nightly
                        .cron_timezone
                        .to_ascii_lowercase()
                        .as_str(),
                    "local" | "utc"
                ) {
                    anyhow::bail!("ship.auto_nightly.cron_timezone must be 'local' or 'utc'");
                }
            }
            if self.ship.auto_stable.enabled {
                crate::schedule::validate_schedule(&self.ship.auto_stable.schedule)
                    .map_err(|e| anyhow::anyhow!("ship.auto_stable.schedule is invalid: {}", e))?;
                if !matches!(
                    self.ship
                        .auto_stable
                        .cron_timezone
                        .to_ascii_lowercase()
                        .as_str(),
                    "local" | "utc"
                ) {
                    anyhow::bail!("ship.auto_stable.cron_timezone must be 'local' or 'utc'");
                }
            }
        }

        if self.commit.sign && !gpg_available() {
            tracing::warn!(
                component = module_path!(),
                "commit.sign is enabled but gpg is not available; signed commits will fail at runtime"
            );
        }

        if self.push.protection.require_ci_pass
            && self.push.protection.required_status_checks.is_empty()
        {
            anyhow::bail!(
                "push.protection.require_ci_pass is true but required_status_checks is empty"
            );
        }

        if self.rbac.enabled {
            let valid_permissions: std::collections::HashSet<&str> = [
                "*",
                "daemon.start",
                "daemon.stop",
                "ship.run",
                "ship.approve",
                "ship.auto",
                "push.force",
                "shark.release",
                "shark.upgrade",
                "config.edit",
            ]
            .iter()
            .copied()
            .collect();
            for role in &self.rbac.roles {
                for perm in &role.permissions {
                    if !valid_permissions.contains(perm.as_str()) {
                        anyhow::bail!(
                            "rbac role '{}' contains unknown permission '{}'",
                            role.name,
                            perm
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            operation: OperationConfig::default(),
            watch: WatchConfig::default(),
            cluster: ClusterConfig::default(),
            weights: crate::weight::WeightConfig::default(),
            push: PushConfig::default(),
            pull: PullConfig::default(),
            ratelimit: RateLimitConfig::default(),
            test: TestConfig::default(),
            jeenome: JeenomeConfig::default(),
            audit: AuditConfig::default(),
            notify: NotifyConfig::default(),
            bundle: BundleConfig::default(),
            staging: StagingConfig::default(),
            commit: CommitConfig::default(),
            inference: InferenceConfig::default(),
            qualification: QualificationConfig::default(),
            build: BuildConfig::default(),
            release: ReleaseConfig::default(),
            distribution: DistributionConfig::default(),
            ship: ShipConfig::default(),
            version_thresholds: VersionThresholdConfig::default(),
            versioning: VersioningConfig::default(),
            plugins: PluginsConfig::default(),
            vacs: VacsConfig::default(),
            trawl: TrawlConfig::default(),
            angler: AnglerConfig::default(),
            deckhand: DeckhandConfig::default(),
            shark: SharkConfig::default(),
            rbac: RbacConfig::default(),
            identity: IdentityConfig::default(),
            repo_path: cwd,
            policy_id: None,
            policy_trust: PolicyTrustConfig::default(),
            governance: GovernanceConfig::default(),
            prune_interval_minutes: default_prune_interval_minutes(),
            retention_days: default_retention_days(),
            air_gapped: false,
            health_port: default_health_port(),
            web_port: 0,
            web: WebConfig::default(),
            capabilities: CapabilitiesConfig::default(),
            // With no repository config these are Kaptaind's built-in
            // commands, not repository-supplied execution authority.
            trust: TrustConfig {
                execution: ExecutionTrust::Trusted,
            },
            compliance: ComplianceConfig::default(),
            integrations: IntegrationsConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

pub fn load() -> anyhow::Result<Config> {
    let cwd = std::env::current_dir()?;
    let repo_root = find_repo_root(&cwd);
    let path = repo_root.join("kaptaind.toml");
    if !path.exists() {
        return Ok(finalize_config(repo_root, Config::default()));
    }

    load_from_path(&path)
}

pub fn load_from_path(path: &Path) -> anyhow::Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let cfg: Config = toml::from_str(&content)?;
    if !matches!(cfg.versioning.mode, VersioningMode::Root) {
        anyhow::bail!(
            "[versioning].mode = {:?} is not implemented yet — only \"root\" (a single \
             VERSION at the repository root) is supported. For per-member versioning today, \
             run `kaptaind-cli trawl --expand-workspaces` so each member crate gets its own \
             kaptaind.toml and VERSION lifecycle.",
            cfg.versioning.mode
        );
    }
    let base_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(finalize_config(base_dir, cfg))
}

fn find_repo_root(start: &Path) -> PathBuf {
    let mut current = start;
    loop {
        if current.join(".git").exists()
            || current.join("kaptaind.toml").exists()
            || current.join("Cargo.toml").exists()
        {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    start.to_path_buf()
}

fn finalize_config(base_dir: PathBuf, mut config: Config) -> Config {
    config.repo_path = absolutize(&base_dir, &config.repo_path);
    config.watch.path = absolutize(&config.repo_path, &config.watch.path);
    config.watch.ignore_file = absolutize(&config.repo_path, &config.watch.ignore_file);
    if let Some(path) = config.identity.gpgv_keyring.as_mut() {
        *path = absolutize(&config.repo_path, path);
    }
    if let Some(path) = config.identity.assertion_path.as_mut() {
        *path = absolutize(&config.repo_path, path);
    }
    config.identity.replay_dir = absolutize(&config.repo_path, &config.identity.replay_dir);
    if let Some(path) = config
        .audit
        .export
        .as_mut()
        .and_then(|export| export.jsonl_path.as_mut())
    {
        *path = absolutize(&config.repo_path, path);
    }

    // Backward compatibility: air_gapped=true disables all network capabilities
    if config.air_gapped {
        config.capabilities.network_push = false;
        config.capabilities.network_webhooks = false;
        config.capabilities.network_inference = false;
        config.capabilities.network_integrations = false;
    }

    config
}

fn absolutize(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

// ---------------------------------------------------------------------------
// Version bump threshold config
// ---------------------------------------------------------------------------

/// `[version_thresholds]` block in `kaptaind.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionThresholdConfig {
    /// Score above which a commit is bumped Minor (default 0.6).
    #[serde(default = "default_minor_threshold")]
    pub minor: f32,
    /// Score above which a commit is bumped Patch (default 0.1).
    #[serde(default = "default_patch_threshold")]
    pub patch: f32,
}

fn default_minor_threshold() -> f32 {
    0.6
}
fn default_patch_threshold() -> f32 {
    0.1
}

impl Default for VersionThresholdConfig {
    fn default() -> Self {
        Self {
            minor: 0.6,
            patch: 0.1,
        }
    }
}

// ---------------------------------------------------------------------------
// Versioning policy config
// ---------------------------------------------------------------------------

/// `[versioning]` block in `kaptaind.toml`.
///
/// Versioning *policy* lives here rather than being baked into project
/// discovery: the trawler identifies structure (workspace roots, member
/// crates), and this section determines how versions are owned and written.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct VersioningConfig {
    /// How versions are owned within the repository.
    ///
    /// Only `Root` is implemented: a single `VERSION` at the repo root, with
    /// the root `Cargo.toml`/`Cargo.lock` kept in sync. `Members`/`Hybrid`
    /// parse but are rejected at load time; use
    /// `kaptaind-cli trawl --expand-workspaces` for per-member versioning.
    #[serde(default)]
    pub mode: VersioningMode,
    /// How to react when `VERSION` and root `Cargo.toml [package].version`
    /// disagree (default `strict`: refuse to commit).
    #[serde(default)]
    pub consistency: VersionConsistency,
    /// How to keep `Cargo.lock` in sync after a version bump (default
    /// `patch`: update the own-package entry in place).
    #[serde(default)]
    pub lock_sync: LockSyncMode,
    /// Which manifests a bump is written to when the repository is a Cargo
    /// workspace (default `root_only`). See
    /// `docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md`.
    #[serde(default)]
    pub workspace: WorkspacePolicy,
}

/// Version ownership model (`[versioning].mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersioningMode {
    /// One `VERSION` at the repository root; member crates are not bumped.
    #[default]
    Root,
    /// Reserved: each workspace member versioned independently.
    Members,
    /// Reserved: named version domains spanning groups of crates.
    Hybrid,
}

/// Policy for `VERSION` vs `Cargo.toml [package].version` disagreement
/// (`[versioning].consistency`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionConsistency {
    /// Refuse to commit while the two sources disagree (default). The daemon
    /// writes both together, so drift means a manual edit that should surface.
    #[default]
    Strict,
    /// Log a warning and proceed with `VERSION` taking precedence.
    Warn,
    /// Silently use `VERSION` precedence (legacy behavior).
    Off,
}

/// Which manifests a version bump is written to when the repository is a
/// Cargo workspace (`[versioning].workspace`). Non-workspace projects are
/// unaffected by every setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePolicy {
    /// Bump only the root `VERSION`/manifest (default through v10.x; the
    /// pre-workspace behavior, kept for compatibility).
    #[default]
    RootOnly,
    /// Bump only the members the cluster touched, plus the root crate when
    /// the cluster touched paths outside every member subtree.
    Touched,
    /// Every bump applies to every member plus the root.
    Lockstep,
}

/// How `Cargo.lock` is kept in sync after a version bump
/// (`[versioning].lock_sync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockSyncMode {
    /// Update the own-package `[[package]]` entry in place (default).
    #[default]
    Patch,
    /// Regenerate via `cargo metadata --offline`; falls back to `Patch` on
    /// failure so the version triple never drifts.
    Cargo,
    /// Leave `Cargo.lock` untouched (e.g. CI regenerates it).
    Off,
}

// ---------------------------------------------------------------------------
// Plugin architecture
// ---------------------------------------------------------------------------

/// `[plugins]` block in `kaptaind.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginsConfig {
    #[serde(default)]
    pub adapters: Vec<PluginAdapterConfig>,
}

/// One external language adapter entry under `[[plugins.adapters]]`.
///
/// The command is called with the file path as its sole argument.
/// It must print JSON to stdout: `{"symbols":[{"name":"...","kind":"..."}]}`.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginAdapterConfig {
    pub name: String,
    pub command: String,
    pub extensions: Vec<String>,
    #[serde(default = "default_plugin_confidence")]
    pub language_confidence: f32,
}

fn default_plugin_confidence() -> f32 {
    0.8
}

mod duration_secs {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

mod duration_secs_option {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = Option::<u64>::deserialize(deserializer)?;
        Ok(secs.map(Duration::from_secs))
    }
}

#[cfg(test)]
mod tests {
    use super::{finalize_config, CapabilitiesConfig, Config};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn operation_defaults_to_observe() {
        assert_eq!(
            Config::default().operation.mode,
            super::OperationMode::Observe
        );
        let parsed: Config = toml::from_str("[operation]\nmode = \"actuate\"\n").unwrap();
        assert_eq!(parsed.operation.mode, super::OperationMode::Actuate);
    }

    #[test]
    fn finalizes_relative_paths_against_repo_root() {
        let base = PathBuf::from("/tmp/kaptaind-config");
        let config = Config {
            repo_path: PathBuf::from("repo"),
            watch: super::WatchConfig {
                path: PathBuf::from("src"),
                recursive: true,
                ignore_file: PathBuf::from("config/.kaptainignore"),
                rescan_on_start: true,
            },
            ..Config::default()
        };

        let finalized = finalize_config(base, config);
        assert_eq!(
            finalized.repo_path,
            PathBuf::from("/tmp/kaptaind-config/repo")
        );
        assert_eq!(
            finalized.watch.path,
            PathBuf::from("/tmp/kaptaind-config/repo/src")
        );
        assert_eq!(
            finalized.watch.ignore_file,
            PathBuf::from("/tmp/kaptaind-config/repo/config/.kaptainignore")
        );
    }

    #[test]
    fn finalizes_relative_audit_export_path_against_repo_root() {
        let config = Config {
            repo_path: PathBuf::from("repo"),
            audit: super::AuditConfig {
                enabled: true,
                export: Some(crate::audit::AuditExportConfig {
                    jsonl_path: Some(PathBuf::from("managed/audit.jsonl")),
                }),
            },
            ..Config::default()
        };

        let finalized = finalize_config(PathBuf::from("/tmp/kaptaind-config"), config);
        assert_eq!(
            finalized.audit.export.and_then(|export| export.jsonl_path),
            Some(PathBuf::from(
                "/tmp/kaptaind-config/repo/managed/audit.jsonl"
            ))
        );
    }

    #[test]
    fn staging_defaults_to_cluster_mode() {
        // Since v9.7.17 the safe default is cluster staging: an autonomous
        // committer must never stage files it did not observe changing.
        let config = Config::default();
        assert!(matches!(config.staging.mode, super::StagingMode::Cluster));
        assert!(config.staging.include.is_empty());
        assert!(config.staging.exclude.is_empty());
    }

    #[test]
    fn daemon_shutdown_grace_defaults_to_ten_seconds() {
        assert_eq!(Config::default().daemon.shutdown_grace_secs, 10);
    }

    #[test]
    fn versioning_defaults_to_root_strict_patch() {
        let config = Config::default();
        assert_eq!(config.versioning.mode, super::VersioningMode::Root);
        assert_eq!(
            config.versioning.consistency,
            super::VersionConsistency::Strict
        );
        assert_eq!(config.versioning.lock_sync, super::LockSyncMode::Patch);
        assert_eq!(
            config.versioning.workspace,
            super::WorkspacePolicy::RootOnly
        );
    }

    #[test]
    fn versioning_workspace_policy_deserializes() {
        let toml_str = r#"
            [versioning]
            workspace = "touched"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.versioning.workspace, super::WorkspacePolicy::Touched);
        let toml_str = r#"
            [versioning]
            workspace = "lockstep"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.versioning.workspace,
            super::WorkspacePolicy::Lockstep
        );
    }

    #[test]
    fn daemon_startup_guard_defaults_and_deserializes() {
        let config: Config = toml::from_str("").unwrap();
        assert!(!config.daemon.startup_guard);
        let toml_str = r#"
            [daemon]
            startup_guard = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.daemon.startup_guard);
        assert_eq!(config.daemon.shutdown_grace_secs, 10);
    }

    #[test]
    fn versioning_deserializes_from_toml() {
        let toml_str = r#"
            [versioning]
            mode = "root"
            consistency = "warn"
            lock_sync = "cargo"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.versioning.mode, super::VersioningMode::Root);
        assert_eq!(
            config.versioning.consistency,
            super::VersionConsistency::Warn
        );
        assert_eq!(config.versioning.lock_sync, super::LockSyncMode::Cargo);
    }

    #[test]
    fn versioning_members_mode_rejected_at_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kaptaind.toml");
        std::fs::write(&path, "[versioning]\nmode = \"members\"\n").unwrap();
        let err = super::load_from_path(&path).expect_err("members mode must fail");
        let msg = err.to_string();
        assert!(msg.contains("not implemented"));
        assert!(msg.contains("--expand-workspaces"));
    }

    #[test]
    fn staging_deserializes_from_toml() {
        let toml_str = r#"
            repo_path = "."
            [trust]
            execution = "trusted"
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
            [staging]
            mode = "cluster"
            exclude = ["*.log", ".env*"]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.staging.mode, super::StagingMode::Cluster));
        assert_eq!(config.staging.exclude, vec!["*.log", ".env*"]);
    }

    /// Since v10.0.0 the formerly mandatory sections ([watch], [weights],
    /// [cluster], [push], [test], [ratelimit]) carry serde defaults, so a
    /// partial kaptaind.toml parses instead of failing with a cryptic
    /// "missing field" error (dogfooding finding).
    #[test]
    fn minimal_toml_parses_with_documented_defaults() {
        let toml_str = r#"
            [trust]
            execution = "trusted"
            [watch]
            path = "."
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.watch.recursive);
        assert_eq!(config.watch.ignore_file, PathBuf::from(".kaptainignore"));
        assert!(config.watch.rescan_on_start);
        assert_eq!(config.cluster.window, Duration::from_secs(5));
        assert!((config.weights.s - 0.35).abs() < f32::EPSILON);
        assert!((config.weights.a - 0.30).abs() < f32::EPSILON);
        assert!((config.weights.d - 0.20).abs() < f32::EPSILON);
        assert!((config.weights.r - 0.15).abs() < f32::EPSILON);
        assert!(!config.push.enabled);
        assert_eq!(config.push.branch, "main");
        assert_eq!(config.push.remote, "origin");
        assert_eq!(
            config.ratelimit.min_commit_interval,
            Duration::from_secs(10)
        );
        assert_eq!(config.test.command.as_deref(), Some("cargo test"));
        assert!(config.test.required);
        // v10.0.0: require_bump defaults to false — below-threshold clusters
        // are captured as chore commits rather than skipped.
        assert!(!config.commit.require_bump);
        // The defaults themselves must satisfy validation.
        config.validate().unwrap();

        // Empty repository configuration is inspectable but cannot execute
        // the default hook until trust is explicit.
        let empty: Config = toml::from_str("").unwrap();
        assert_eq!(empty.push.branch, "main");
        assert!(!empty.commit.require_bump);
        assert!(empty.validate().is_err());
    }

    #[test]
    fn explicitly_invalid_health_port_still_fails_validation() {
        let toml_str = "health_port = 0\n[trust]\nexecution = \"trusted\"";
        let config: Config = toml::from_str(toml_str).unwrap();
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("health_port"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn staging_missing_from_toml_defaults_to_cluster() {
        let toml_str = r#"
            repo_path = "."
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.staging.mode, super::StagingMode::Cluster));
    }

    #[test]
    fn inference_defaults_to_fast_mode() {
        let config = Config::default();
        assert!(matches!(
            config.inference.validation_mode,
            super::ValidationMode::Fast
        ));
        assert_eq!(config.inference.consensus_models, vec!["llama3.2"]);
        assert!((config.inference.consensus_threshold - 0.6).abs() < f64::EPSILON);
        assert_eq!(config.inference.consensus_min_agreement, 2);
    }

    #[test]
    fn inference_deserializes_consensus_mode() {
        let toml_str = r#"
            repo_path = "."
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
            [inference]
            enabled = true
            validation_mode = "consensus"
            consensus_models = ["llama3.2", "mistral", "codellama"]
            consensus_threshold = 0.7
            consensus_min_agreement = 3
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            config.inference.validation_mode,
            super::ValidationMode::Consensus
        ));
        assert_eq!(config.inference.consensus_models.len(), 3);
        assert!((config.inference.consensus_threshold - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.inference.consensus_min_agreement, 3);
    }

    #[test]
    fn cluster_defaults_disable_dynamic_clustering() {
        let config = Config::default();
        assert_eq!(config.cluster.max_paths, 0);
        assert!(config.cluster.flush_after.is_none());
    }

    #[test]
    fn cluster_deserializes_dynamic_options() {
        let toml_str = r#"
            repo_path = "."
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            max_paths = 25
            flush_after = 15
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.cluster.max_paths, 25);
        assert_eq!(config.cluster.flush_after, Some(Duration::from_secs(15)));
    }

    #[test]
    fn deckhand_defaults_are_conservative() {
        let config = Config::default();
        assert!(!config.deckhand.enabled);
        assert_eq!(config.deckhand.interval_minutes, 360);
        assert_eq!(config.deckhand.sweep_keep_days, 30);
        assert_eq!(config.deckhand.clean_profiles, vec!["debug"]);
        assert!(config.deckhand.clean_older_than_days.is_none());
        assert!(!config.deckhand.dry_run);
        assert_eq!(config.deckhand.min_free_percent, 10);
    }

    #[test]
    fn deckhand_deserializes_from_toml() {
        let toml_str = r#"
            repo_path = "."
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
            [deckhand]
            enabled = true
            interval_minutes = 120
            sweep_keep_days = 7
            clean_profiles = ["debug", "release"]
            clean_older_than_days = 14
            dry_run = true
            min_free_percent = 5
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.deckhand.enabled);
        assert_eq!(config.deckhand.interval_minutes, 120);
        assert_eq!(config.deckhand.sweep_keep_days, 7);
        assert_eq!(
            config.deckhand.clean_profiles,
            vec!["debug".to_string(), "release".to_string()]
        );
        assert_eq!(config.deckhand.clean_older_than_days, Some(14));
        assert!(config.deckhand.dry_run);
        assert_eq!(config.deckhand.min_free_percent, 5);
    }

    #[test]
    fn shark_defaults_are_conservative() {
        let config = Config::default();
        assert!(!config.shark.enabled);
        assert_eq!(config.shark.arbiter_path, PathBuf::from(".kaptaind/shark"));
        assert_eq!(config.shark.heartbeat_interval_ms, 1000);
        assert_eq!(config.shark.heartbeat_timeout_ms, 5000);
        assert_eq!(config.shark.lease_ttl_ms, 10000);
        assert!(config.shark.instance_id.is_none());
        assert_eq!(config.shark.upgrade_handoff_timeout_ms, 30000);
        assert!(matches!(config.shark.mode, super::SharkMode::Auto));
    }

    #[test]
    fn shark_deserializes_from_toml() {
        let toml_str = r#"
            repo_path = "."
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
            [shark]
            enabled = true
            arbiter_path = "/tmp/shark"
            heartbeat_interval_ms = 500
            heartbeat_timeout_ms = 2000
            lease_ttl_ms = 4000
            instance_id = "instance-a"
            upgrade_handoff_timeout_ms = 60000
            mode = "standby"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.shark.enabled);
        assert_eq!(config.shark.arbiter_path, PathBuf::from("/tmp/shark"));
        assert_eq!(config.shark.heartbeat_interval_ms, 500);
        assert_eq!(config.shark.heartbeat_timeout_ms, 2000);
        assert_eq!(config.shark.lease_ttl_ms, 4000);
        assert_eq!(config.shark.instance_id, Some("instance-a".to_string()));
        assert_eq!(config.shark.upgrade_handoff_timeout_ms, 60000);
        assert!(matches!(config.shark.mode, super::SharkMode::Standby));
    }

    #[test]
    fn ship_defaults_are_sensible() {
        let config = Config::default();
        assert!(!config.ship.enabled);
        assert!(config.ship.require_qualification);
        assert_eq!(config.ship.targets.len(), 5);
        assert!(config.ship.channels.binaries);
        assert!(!config.ship.channels.installers.shell);
        assert!(!config.ship.channels.installers.tauri);
        assert!(config.ship.channels.package_managers.is_empty());
        assert!(config.ship.channels.app_stores.is_empty());
        assert!(!config.ship.sign);
        assert!(config.ship.gpg_key_id.is_none());
        assert!(config.ship.nightly.sign.is_none());
        assert!(config.ship.stable.sign.is_none());
        assert!(!config.ship.sbom.enabled);
        assert_eq!(config.ship.sbom.format, "spdx-json");
    }

    #[test]
    fn ship_deserializes_from_toml() {
        let toml_str = r#"
            repo_path = "."
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
            [ship]
            enabled = true
            targets = ["x86_64-unknown-linux-gnu"]
            require_qualification = false
            [ship.installers]
            shell = true
            tauri = false
            [[ship.package_managers]]
            kind = "homebrew"
            formula_name = "kaptaind"
            [[ship.app_stores]]
            kind = "github-releases"
            draft = true
            prerelease = false
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.ship.enabled);
        assert!(!config.ship.require_qualification);
        assert_eq!(config.ship.targets, vec!["x86_64-unknown-linux-gnu"]);
        assert!(config.ship.channels.installers.shell);
        assert!(!config.ship.channels.installers.tauri);
        assert_eq!(config.ship.channels.package_managers.len(), 1);
        assert_eq!(config.ship.channels.package_managers[0].kind, "homebrew");
        assert_eq!(config.ship.channels.app_stores.len(), 1);
        assert!(config.ship.channels.app_stores[0].draft);
    }

    #[test]
    fn ship_deserializes_signing_fields() {
        let toml_str = r#"
            repo_path = "."
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
            [ship]
            enabled = true
            sign = true
            gpg_key_id = "releases@example.com"
            [ship.stable]
            sign = false
            [ship.nightly]
            sign = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.ship.sign);
        assert_eq!(
            config.ship.gpg_key_id,
            Some("releases@example.com".to_string())
        );
        assert_eq!(config.ship.stable.sign, Some(false));
        assert_eq!(config.ship.nightly.sign, Some(true));
    }

    #[test]
    fn validate_accepts_default_config() {
        let config = Config::default();
        let result = config.validate();
        assert!(
            result.is_ok(),
            "default config should validate: {:?}",
            result
        );
    }

    #[test]
    fn regional_profiles_are_opt_in_and_preserve_default_behavior() {
        let config = Config::default();
        assert!(config.compliance.profiles.is_empty());
        assert_eq!(
            config.compliance.egress.inference,
            super::EgressPolicy::Allow
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn regional_profile_rejects_permissive_active_inference() {
        let mut config = Config::default();
        config
            .compliance
            .profiles
            .insert(super::RegionalProfile::EuEea);
        config.inference.enabled = true;
        config.inference.provider = "ollama".to_string();
        config.capabilities.network_inference = true;
        let error = config.validate().expect_err("profile must fail closed");
        assert!(error.to_string().contains("approved_only"));
    }

    #[test]
    fn regional_profile_allows_only_exact_attested_inference_host() {
        let mut config = Config::default();
        config
            .compliance
            .profiles
            .insert(super::RegionalProfile::EuEea);
        config.compliance.egress.inference = super::EgressPolicy::ApprovedOnly;
        config
            .compliance
            .egress
            .allowed_hosts
            .insert("models.example.test".to_string());
        config.inference.enabled = true;
        config.inference.provider = "cosine".to_string();
        config.capabilities.network_inference = true;
        config.inference.cosine_base_url = Some("https://models.example.test/v1".to_string());
        assert!(config.validate().is_ok());

        config.inference.cosine_base_url = Some("https://unapproved.example.test/v1".to_string());
        let error = config
            .validate()
            .expect_err("unapproved host must be rejected");
        assert!(error.to_string().contains("does not approve"));
    }

    #[test]
    fn uk_profile_requires_cosine_routing_and_approved_host() {
        let mut config = Config::default();
        config
            .compliance
            .profiles
            .insert(super::RegionalProfile::Uk);
        config.compliance.egress.inference = super::EgressPolicy::ApprovedOnly;
        config
            .compliance
            .egress
            .allowed_hosts
            .insert("lumen.uk.example.test".to_string());
        config.inference.enabled = true;
        config.inference.provider = "cosine".to_string();
        config.capabilities.network_inference = true;
        config.inference.cosine_base_url = Some("https://lumen.uk.example.test/v1".to_string());
        assert!(config.validate().is_ok());

        config.inference.provider = "openai".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn sovereign_profile_requires_local_only_and_disabled_egress() {
        let mut config = Config::default();
        config
            .compliance
            .profiles
            .insert(super::RegionalProfile::Sovereign);
        assert!(config.validate().is_err());

        config.compliance.egress.inference = super::EgressPolicy::Deny;
        config.compliance.egress.webhooks = super::EgressPolicy::Deny;
        config.compliance.egress.integrations = super::EgressPolicy::Deny;
        config.compliance.egress.audit_export = super::AuditEgressPolicy::LocalOnly;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn regional_profiles_gate_enabled_integration_endpoints_and_capability() {
        let mut config = Config::default();
        config
            .compliance
            .profiles
            .insert(super::RegionalProfile::EuEea);
        config.compliance.egress.integrations = super::EgressPolicy::ApprovedOnly;
        config
            .compliance
            .egress
            .allowed_hosts
            .insert("93.184.216.34".to_string());
        config
            .integrations
            .connectors
            .push(crate::integrations::ConnectorConfig {
                provider: crate::integrations::Provider::Kubernetes,
                mode: crate::integrations::Mode::ReadOnly,
                tenant_id: "acme".to_string(),
                endpoint: Some("https://93.184.216.34/api".to_string()),
                credential_ref: Some("vault:kubernetes-readonly".to_string()),
                capabilities: [crate::integrations::Capability::ReadState]
                    .into_iter()
                    .collect(),
            });
        assert!(config.validate().is_ok());
        config.capabilities.network_integrations = false;
        assert!(config.validate().is_err());
    }

    #[test]
    fn local_only_audit_export_cannot_escape_repo_root() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config {
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        config
            .compliance
            .profiles
            .insert(super::RegionalProfile::Canada);
        config.compliance.egress.audit_export = super::AuditEgressPolicy::LocalOnly;
        config.audit.export = Some(crate::audit::AuditExportConfig {
            jsonl_path: Some(PathBuf::from("/tmp/outside-audit.jsonl")),
        });
        let error = config
            .validate()
            .expect_err("export outside repository must fail");
        assert!(error.to_string().contains("remain under repo_path"));
    }

    #[test]
    fn repository_config_defaults_to_untrusted() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.trust.execution, super::ExecutionTrust::Untrusted);
        assert!(!config.execution_trust_violations().is_empty());
    }

    #[test]
    fn built_in_config_remains_trusted() {
        let config = Config::default();
        assert_eq!(config.trust.execution, super::ExecutionTrust::Trusted);
        assert!(config.execution_trust_violations().is_empty());
    }

    #[test]
    fn untrusted_config_rejects_configured_program_execution() {
        let mut config = Config::default();
        config.trust.execution = super::ExecutionTrust::Untrusted;
        config.bundle.command = Some("npm run build".to_string());
        config.plugins.adapters.push(super::PluginAdapterConfig {
            name: "example".to_string(),
            command: "example-adapter".to_string(),
            extensions: vec!["example".to_string()],
            language_confidence: 0.8,
        });
        config.angler.bait.enabled = true;

        let error = config
            .validate()
            .expect_err("untrusted execution must fail");
        let message = error.to_string();
        assert!(message.contains("test.command"));
        assert!(message.contains("bundle.command"));
        assert!(message.contains("plugins.adapters"));
        assert!(message.contains("angler.bait"));
        assert!(message.contains("[trust].execution = \"trusted\""));
    }

    #[test]
    fn untrusted_passive_config_is_valid_and_inspectable() {
        let mut config = Config::default();
        config.trust.execution = super::ExecutionTrust::Untrusted;
        config.test.command = None;

        assert!(config.execution_trust_violations().is_empty());
        assert!(config.validate_execution_trust().is_ok());
    }

    #[test]
    fn validate_rejects_zero_inference_timeout() {
        let mut config = Config::default();
        config.inference.timeout_secs = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn enterprise_governance_requires_identity_rbac_and_signed_policy_controls() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config {
            governance: super::GovernanceConfig {
                organization_id: Some("acme".to_string()),
                tenant_id: Some("payments".to_string()),
                enforce_enterprise_controls: true,
            },
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        assert!(config.validate().is_err());
        config.rbac.enabled = true;
        config.policy_id = Some("production".to_string());
        config.policy_trust.require_signature = true;
        config.policy_trust.gpgv_keyring = Some(PathBuf::from("policy-keys.gpg"));
        config.identity = super::IdentityConfig {
            mode: super::IdentityMode::GpgSignedAssertion,
            gpgv_keyring: Some(PathBuf::from("identity-keys.gpg")),
            assertion_path: Some(PathBuf::from("identity.json")),
            replay_dir: PathBuf::from(".kaptaind/identity/replay"),
            issuer: Some("https://id.example".to_string()),
            audience: Some("kaptaind".to_string()),
            max_assertion_age_seconds: 900,
        };
        config.audit.export = Some(crate::audit::AuditExportConfig {
            jsonl_path: Some(dir.path().join("collector/audit.jsonl")),
        });
        config.commit.sign = true;
        assert!(config.validate().is_ok());

        config.test.command_on = super::TestCommandOn::CodeOnly;
        assert!(config
            .validate()
            .expect_err("enterprise test gate must run for every cluster")
            .to_string()
            .contains("mandatory test.command"));
        config.test.command_on = super::TestCommandOn::Always;

        config.commit.sign = false;
        assert!(config
            .validate()
            .expect_err("enterprise commits must be signed")
            .to_string()
            .contains("commit].sign"));
        config.commit.sign = true;

        config.push.enabled = true;
        assert!(config
            .validate()
            .expect_err("enterprise pushes must be CI-protected")
            .to_string()
            .contains("CI-protected pushes"));
        config.push.protection.require_ci_pass = true;
        config.push.protection.required_status_checks = vec!["ci/test".to_string()];
        assert!(config.validate().is_ok());

        config.ship.enabled = true;
        assert!(config
            .validate()
            .expect_err("enterprise releases require supply-chain evidence")
            .to_string()
            .contains("SBOM and provenance"));
        config.ship.sign = true;
        config.ship.sbom.enabled = true;
        config.ship.provenance.enabled = true;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn governance_identifiers_reject_path_like_values() {
        let mut config = Config::default();
        config.governance.organization_id = Some("../acme".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn enterprise_governance_rejects_an_unverifiable_existing_audit_log() {
        let dir = tempfile::tempdir().unwrap();
        let audit_dir = dir.path().join(".kaptaind");
        std::fs::create_dir_all(&audit_dir).unwrap();
        std::fs::write(audit_dir.join("audit.jsonl"), "{}\n").unwrap();
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            governance: super::GovernanceConfig {
                organization_id: Some("acme".to_string()),
                tenant_id: Some("payments".to_string()),
                enforce_enterprise_controls: true,
            },
            rbac: super::RbacConfig {
                enabled: true,
                ..super::RbacConfig::default()
            },
            policy_id: Some("production".to_string()),
            policy_trust: super::PolicyTrustConfig {
                require_signature: true,
                gpgv_keyring: Some(PathBuf::from("policy-keys.gpg")),
            },
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_insufficient_shark_ttl() {
        let mut config = Config::default();
        config.shark.enabled = true;
        config.shark.heartbeat_interval_ms = 1000;
        config.shark.lease_ttl_ms = 2000;
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_air_gapped_with_network_capabilities() {
        let config = Config {
            air_gapped: true,
            capabilities: CapabilitiesConfig {
                network_push: true,
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn ship_auto_deserializes_from_toml() {
        let toml_str = r#"
            repo_path = "."
            [trust]
            execution = "trusted"
            [watch]
            path = "."
            recursive = true
            ignore_file = ".kaptainignore"
            [cluster]
            window = 5
            [weights]
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
            [push]
            enabled = false
            branch = "main"
            [ratelimit]
            min_commit_interval = 10
            [test]
            command = "cargo test"
            required = true
            [ship]
            enabled = true
            [ship.auto_nightly]
            enabled = true
            schedule = "0 3 * * *"
            cron_timezone = "utc"
            require_qualification = false
            [ship.auto_stable]
            enabled = true
            schedule = "0 9 * * 1"
            cron_timezone = "local"
            require_qualification = true
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.ship.auto_nightly.enabled);
        assert_eq!(config.ship.auto_nightly.schedule, "0 3 * * *");
        assert_eq!(config.ship.auto_nightly.cron_timezone, "utc");
        assert!(!config.ship.auto_nightly.require_qualification);
        assert!(config.ship.auto_stable.enabled);
        assert_eq!(config.ship.auto_stable.schedule, "0 9 * * 1");
        assert_eq!(config.ship.auto_stable.cron_timezone, "local");
        assert!(config.ship.auto_stable.require_qualification);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_rejects_invalid_auto_ship_schedule() {
        let config = Config {
            ship: super::ShipConfig {
                enabled: true,
                auto_nightly: super::ShipAutoConfig {
                    enabled: true,
                    schedule: "not a cron".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_invalid_auto_ship_timezone() {
        let config = Config {
            ship: super::ShipConfig {
                enabled: true,
                auto_stable: super::ShipAutoConfig {
                    enabled: true,
                    cron_timezone: "mars".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }
}
