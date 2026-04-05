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
    pub notify: NotifyConfig,
    #[serde(default)]
    pub bundle: BundleConfig,
    #[serde(default)]
    pub staging: StagingConfig,
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
    pub version_thresholds: VersionThresholdConfig,
    #[serde(default)]
    pub plugins: PluginsConfig,
    pub repo_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Build config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Default)]
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

fn default_artifact_path() -> String {
    "target/release/kaptaind".to_string()
}

fn default_build_timeout_secs() -> u64 {
    600 // 10 minutes
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

fn default_local_dist_path() -> String {
    ".kaptaind/releases/".to_string()
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

#[derive(Debug, Clone, Deserialize)]
pub struct WatchConfig {
    pub path: PathBuf,
    pub recursive: bool,
    pub ignore_file: PathBuf,
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
}

fn default_min_window_secs() -> u64 { 2 }
fn default_max_window_secs() -> u64 { 30 }
fn default_burst_threshold() -> usize { 10 }

#[derive(Debug, Clone, Deserialize)]
pub struct PushConfig {
    pub enabled: bool,
    pub branch: String,
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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NotifyConfig {
    pub on_commit: Option<String>,
    pub on_error: Option<String>,
    pub webhook_url: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagingMode {
    /// Stage all modified files (current behavior, default)
    All,
    /// Only stage files that were part of the detected cluster
    Cluster,
    /// Stage files matching include patterns, skip exclude patterns
    Pattern,
}

impl Default for StagingMode {
    fn default() -> Self {
        StagingMode::All
    }
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
        }
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
            },
            cluster: ClusterConfig {
                window: Duration::from_secs(5),
                adaptive: false,
                min_window_secs: default_min_window_secs(),
                max_window_secs: default_max_window_secs(),
                burst_threshold: default_burst_threshold(),
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
            },
            ratelimit: RateLimitConfig {
                min_commit_interval: Duration::from_secs(10),
            },
            test: TestConfig {
                command: Some("cargo test".to_string()),
                required: true,
            },
            notify: NotifyConfig::default(),
            bundle: BundleConfig::default(),
            staging: StagingConfig::default(),
            inference: InferenceConfig::default(),
            qualification: QualificationConfig::default(),
            build: BuildConfig::default(),
            release: ReleaseConfig::default(),
            distribution: DistributionConfig::default(),
            version_thresholds: VersionThresholdConfig::default(),
            plugins: PluginsConfig::default(),
            repo_path: cwd,
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

    let content = std::fs::read_to_string(&path)?;
    let cfg: Config = toml::from_str(&content)?;
    Ok(finalize_config(repo_root, cfg))
}

fn find_repo_root(start: &Path) -> PathBuf {
    let mut current = start;
    loop {
        if current.join(".git").exists() || current.join("kaptaind.toml").exists() || current.join("Cargo.toml").exists() {
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

fn default_minor_threshold() -> f32 { 0.6 }
fn default_patch_threshold() -> f32 { 0.1 }

impl Default for VersionThresholdConfig {
    fn default() -> Self {
        Self { minor: 0.6, patch: 0.1 }
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

fn default_plugin_confidence() -> f32 { 0.8 }

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

#[cfg(test)]
mod tests {
    use super::{finalize_config, Config};
    use std::path::PathBuf;

    #[test]
    fn finalizes_relative_paths_against_repo_root() {
        let base = PathBuf::from("/tmp/kaptaind-config");
        let config = Config {
            repo_path: PathBuf::from("repo"),
            watch: super::WatchConfig {
                path: PathBuf::from("src"),
                recursive: true,
                ignore_file: PathBuf::from("config/.kaptainignore"),
            },
            ..Config::default()
        };

        let finalized = finalize_config(base, config);
        assert_eq!(finalized.repo_path, PathBuf::from("/tmp/kaptaind-config/repo"));
        assert_eq!(finalized.watch.path, PathBuf::from("/tmp/kaptaind-config/repo/src"));
        assert_eq!(
            finalized.watch.ignore_file,
            PathBuf::from("/tmp/kaptaind-config/repo/config/.kaptainignore")
        );
    }

    #[test]
    fn staging_defaults_to_all_mode() {
        let config = Config::default();
        assert!(matches!(config.staging.mode, super::StagingMode::All));
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
    fn staging_missing_from_toml_defaults_to_all() {
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
        assert!(matches!(config.staging.mode, super::StagingMode::All));
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
}
