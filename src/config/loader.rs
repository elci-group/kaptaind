use crate::angler::config::AnglerConfig;
use crate::notify::audio::TtsConfig;
use crate::qualification::policy::QualificationConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub watch: WatchConfig,
    pub cluster: ClusterConfig,
    pub weights: crate::weight::WeightConfig,
    pub push: PushConfig,
    pub ratelimit: RateLimitConfig,
    pub test: TestConfig,
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
    pub repo_path: PathBuf,
    #[serde(default)]
    pub policy_id: Option<String>,
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
    pub strict_shell_validation: bool,
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
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self { enabled: true }
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
    pub path: PathBuf,
    pub recursive: bool,
    pub ignore_file: PathBuf,
    /// On startup, reconcile working-tree changes made while the daemon was
    /// down into a single catch-up cluster (default true).
    #[serde(default = "default_true")]
    pub rescan_on_start: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterConfig {
    #[serde(with = "duration_secs")]
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
    pub enabled: bool,
    pub branch: String,
    #[serde(default = "default_remote")]
    pub remote: String,
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

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_retry_max_attempts(),
            initial_delay_ms: default_retry_initial_delay_ms(),
            backoff_multiplier: default_retry_backoff_multiplier(),
            max_delay_ms: default_retry_max_delay_ms(),
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
    #[serde(with = "duration_secs")]
    pub min_commit_interval: Duration,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestConfig {
    pub command: Option<String>,
    pub required: bool,
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

#[derive(Debug, Clone, Deserialize, Default)]
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
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            sign: false,
            gpg_key_id: None,
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

impl Config {
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
    /// Validate cross-field constraints and invariants.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.inference.timeout_secs == 0 {
            anyhow::bail!("inference.timeout_secs must be greater than 0");
        }
        if self.build.timeout_secs == 0 {
            anyhow::bail!("build.timeout_secs must be greater than 0");
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
            watch: WatchConfig {
                path: cwd.clone(),
                recursive: true,
                ignore_file: PathBuf::from(".kaptainignore"),
                rescan_on_start: true,
            },
            cluster: ClusterConfig {
                window: Duration::from_secs(5),
                adaptive: false,
                min_window_secs: default_min_window_secs(),
                max_window_secs: default_max_window_secs(),
                burst_threshold: default_burst_threshold(),
                max_paths: default_max_paths(),
                flush_after: None,
            },
            weights: crate::weight::WeightConfig {
                s: 0.35,
                a: 0.3,
                d: 0.2,
                r: 0.15,
                b: 0.0,
            },
            push: PushConfig {
                enabled: false,
                branch: "main".to_string(),
                remote: "origin".to_string(),
                dry_run: false,
                retry: RetryConfig::default(),
                conflict: ConflictConfig::default(),
                pre_push: PrePushConfig::default(),
                safety: SafetyConfig::default(),
                batch: BatchConfig::default(),
                protection: PushProtectionConfig::default(),
            },
            ratelimit: RateLimitConfig {
                min_commit_interval: Duration::from_secs(10),
            },
            test: TestConfig {
                command: Some("cargo test".to_string()),
                required: true,
            },
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
            plugins: PluginsConfig::default(),
            vacs: VacsConfig::default(),
            trawl: TrawlConfig::default(),
            angler: AnglerConfig::default(),
            deckhand: DeckhandConfig::default(),
            shark: SharkConfig::default(),
            rbac: RbacConfig::default(),
            repo_path: cwd,
            policy_id: None,
            prune_interval_minutes: default_prune_interval_minutes(),
            retention_days: default_retention_days(),
            air_gapped: false,
            health_port: default_health_port(),
            web_port: 0,
            web: WebConfig::default(),
            capabilities: CapabilitiesConfig::default(),
            strict_shell_validation: false,
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

    // Backward compatibility: air_gapped=true disables all network capabilities
    if config.air_gapped {
        config.capabilities.network_push = false;
        config.capabilities.network_webhooks = false;
        config.capabilities.network_inference = false;
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
    fn staging_defaults_to_cluster_mode() {
        // Since v9.7.17 the safe default is cluster staging: an autonomous
        // committer must never stage files it did not observe changing.
        let config = Config::default();
        assert!(matches!(config.staging.mode, super::StagingMode::Cluster));
        assert!(config.staging.include.is_empty());
        assert!(config.staging.exclude.is_empty());
    }

    #[test]
    fn staging_deserializes_from_toml() {
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
            [staging]
            mode = "cluster"
            exclude = ["*.log", ".env*"]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(matches!(config.staging.mode, super::StagingMode::Cluster));
        assert_eq!(config.staging.exclude, vec!["*.log", ".env*"]);
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
    fn validate_rejects_zero_inference_timeout() {
        let mut config = Config::default();
        config.inference.timeout_secs = 0;
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
