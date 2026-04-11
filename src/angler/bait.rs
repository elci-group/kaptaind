//! Bait plugin system for Angler.
//!
//! The bait system allows external tools and scripts to "bite" on specific
//! change patterns, similar to how bait attracts fish. Plugins can respond
//! to various events in the kaptaind lifecycle.

use crate::angler::config::{BaitConfig, BaitDefinition, BaitEvent, BaitType};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

/// Result of executing a bait.
#[derive(Debug, Clone)]
pub struct BaitResult {
    /// Whether the bait succeeded
    pub success: bool,
    /// Exit code (if available)
    pub exit_code: Option<i32>,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Whether the bait timed out
    pub timed_out: bool,
    /// Data returned by the bait (if JSON)
    pub data: Option<serde_json::Value>,
}

impl BaitResult {
    /// Create a successful result.
    pub fn success() -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
            timed_out: false,
            data: None,
        }
    }

    /// Create a failure result.
    pub fn failure(stderr: impl Into<String>) -> Self {
        Self {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: stderr.into(),
            duration_ms: 0,
            timed_out: false,
            data: None,
        }
    }

    /// Create a timeout result.
    pub fn timeout() -> Self {
        Self {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: "Bait timed out".to_string(),
            duration_ms: 0,
            timed_out: true,
            data: None,
        }
    }
}

/// Context passed to bait plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaitContext {
    /// Event that triggered the bait
    pub event: BaitEvent,
    /// File changes (if applicable)
    pub files: Vec<FileChangeInfo>,
    /// Repository path
    pub repo_path: PathBuf,
    /// Cluster ID (if applicable)
    pub cluster_id: Option<String>,
    /// Version (if applicable)
    pub version: Option<String>,
    /// Score (if applicable)
    pub score: Option<f32>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// File change information for bait context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeInfo {
    pub path: PathBuf,
    pub change_type: String,
    pub size: u64,
}

/// Bait plugin manager.
pub struct BaitManager {
    config: BaitConfig,
    baits: Vec<BaitDefinition>,
    repo_path: PathBuf,
}

impl BaitManager {
    /// Create a new bait manager.
    pub fn new(config: &BaitConfig, repo_path: &Path) -> Result<Self> {
        let mut baits = config.baits.clone();

        // Auto-discover baits from plugins directory
        if config.auto_discover {
            let plugins_dir = if config.plugins_dir.is_absolute() {
                config.plugins_dir.clone()
            } else {
                repo_path.join(&config.plugins_dir)
            };

            if plugins_dir.exists() {
                match Self::discover_baits(&plugins_dir) {
                    Ok(discovered) => {
                        info!("Discovered {} baits from {}", discovered.len(), plugins_dir.display());
                        baits.extend(discovered);
                    }
                    Err(e) => {
                        warn!("Failed to discover baits: {}", e);
                    }
                }
            }
        }

        Ok(Self {
            config: config.clone(),
            baits,
            repo_path: repo_path.to_path_buf(),
        })
    }

    /// Execute all baits that match the given event and files.
    pub async fn trigger_event(
        &self,
        event: BaitEvent,
        context: &BaitContext,
    ) -> Vec<(String, BaitResult)> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();

        for bait in &self.baits {
            if !bait.enabled {
                continue;
            }

            if !bait.events.contains(&event) {
                continue;
            }

            // Check file patterns if specified
            if !bait.file_patterns.is_empty() && !context.files.is_empty() {
                let matches = context.files.iter().any(|file| {
                    bait.file_patterns.iter().any(|pattern| {
                        glob::Pattern::new(pattern)
                            .map(|p| p.matches_path(&file.path))
                            .unwrap_or(false)
                    })
                });

                if !matches {
                    continue;
                }
            }

            let result = self.execute_bait(bait, context).await;
            results.push((bait.id.clone(), result));
        }

        results
    }

    /// Execute a specific bait by ID.
    pub async fn execute_bait_by_id(&self, bait_id: &str, context: &BaitContext) -> Option<BaitResult> {
        let bait = self.baits.iter().find(|b| b.id == bait_id)?;
        Some(self.execute_bait(bait, context).await)
    }

    /// Execute a single bait.
    async fn execute_bait(&self, bait: &BaitDefinition, context: &BaitContext) -> BaitResult {
        let start = std::time::Instant::now();

        debug!(
            "Executing bait {} ({}) of type {:?}",
            bait.id, bait.name, bait.bait_type
        );

        let result = match bait.bait_type {
            BaitType::External => self.execute_external_bait(bait, context).await,
            BaitType::Shell => self.execute_shell_bait(bait, context).await,
            BaitType::Webhook => self.execute_webhook_bait(bait, context).await,
            BaitType::Native => {
                // Native baits would require dynamic loading or registration
                warn!("Native bait type not yet implemented: {}", bait.id);
                BaitResult::failure("Native bait type not implemented")
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        if result.success {
            info!(
                "Bait {} completed successfully in {}ms",
                bait.id, duration_ms
            );
        } else {
            warn!(
                "Bait {} failed: exit_code={:?}, error={}",
                bait.id, result.exit_code, result.stderr
            );
        }

        result
    }

    /// List all registered baits.
    pub fn list_baits(&self) -> Vec<&BaitDefinition> {
        self.baits.iter().collect()
    }

    /// Get a specific bait by ID.
    pub fn get_bait(&self, id: &str) -> Option<&BaitDefinition> {
        self.baits.iter().find(|b| b.id == id)
    }

    /// Add a new bait at runtime.
    pub fn add_bait(&mut self, bait: BaitDefinition) {
        self.baits.push(bait);
    }

    /// Remove a bait by ID.
    pub fn remove_bait(&mut self, id: &str) -> bool {
        let initial_len = self.baits.len();
        self.baits.retain(|b| b.id != id);
        self.baits.len() < initial_len
    }

    /// Enable a bait.
    pub fn enable_bait(&mut self, id: &str) -> bool {
        if let Some(bait) = self.baits.iter_mut().find(|b| b.id == id) {
            bait.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable a bait.
    pub fn disable_bait(&mut self, id: &str) -> bool {
        if let Some(bait) = self.baits.iter_mut().find(|b| b.id == id) {
            bait.enabled = false;
            true
        } else {
            false
        }
    }

    /// Create the plugins directory if it doesn't exist.
    pub fn ensure_plugins_dir(&self) -> Result<()> {
        let plugins_dir = if self.config.plugins_dir.is_absolute() {
            self.config.plugins_dir.clone()
        } else {
            self.repo_path.join(&self.config.plugins_dir)
        };

        std::fs::create_dir_all(&plugins_dir)?;
        Ok(())
    }

    /// Install a bait from a file.
    pub fn install_bait(&mut self, source: &Path, name: Option<&str>) -> Result<BaitDefinition> {
        let plugins_dir = if self.config.plugins_dir.is_absolute() {
            self.config.plugins_dir.clone()
        } else {
            self.repo_path.join(&self.config.plugins_dir)
        };

        std::fs::create_dir_all(&plugins_dir)?;

        let bait_name = name.unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
        });

        let dest_path = plugins_dir.join(format!("{}.sh", bait_name));
        std::fs::copy(source, &dest_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest_path, perms)?;
        }

        let bait = BaitDefinition {
            id: bait_name.to_string(),
            name: bait_name.to_string(),
            description: format!("Installed bait from {}", source.display()),
            bait_type: BaitType::Shell,
            command: dest_path.to_string_lossy().to_string(),
            file_patterns: vec![],
            events: vec![
                BaitEvent::PreCommit,
                BaitEvent::PostCommit,
            ],
            enabled: true,
            timeout_secs: 30,
            env: HashMap::new(),
        };

        self.add_bait(bait.clone());
        info!("Installed bait {} from {}", bait_name, source.display());

        Ok(bait)
    }

    // =============================================================================
    // Internal Methods
    // =============================================================================

    async fn execute_external_bait(
        &self,
        bait: &BaitDefinition,
        context: &BaitContext,
    ) -> BaitResult {
        let context_json = match serde_json::to_string(context) {
            Ok(json) => json,
            Err(e) => return BaitResult::failure(format!("Failed to serialize context: {}", e)),
        };

        let mut cmd = Command::new(&bait.command);
        cmd.arg("run")
            .current_dir(&self.repo_path)
            .env("KAPTAIND_BAIT_ID", &bait.id)
            .env("KAPTAIND_BAIT_EVENT", format!("{:?}", context.event))
            .env("KAPTAIND_CONTEXT", &context_json);

        // Add custom environment variables
        for (key, value) in &bait.env {
            cmd.env(key, value);
        }

        self.run_command_with_timeout(cmd, bait.timeout_secs).await
    }

    async fn execute_shell_bait(
        &self,
        bait: &BaitDefinition,
        context: &BaitContext,
    ) -> BaitResult {
        let context_json = match serde_json::to_string(context) {
            Ok(json) => json,
            Err(e) => return BaitResult::failure(format!("Failed to serialize context: {}", e)),
        };

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&bait.command)
            .current_dir(&self.repo_path)
            .env("KAPTAIND_BAIT_ID", &bait.id)
            .env("KAPTAIND_BAIT_EVENT", format!("{:?}", context.event))
            .env("KAPTAIND_CONTEXT", &context_json);

        // Add custom environment variables
        for (key, value) in &bait.env {
            cmd.env(key, value);
        }

        self.run_command_with_timeout(cmd, bait.timeout_secs).await
    }

    async fn execute_webhook_bait(
        &self,
        bait: &BaitDefinition,
        context: &BaitContext,
    ) -> BaitResult {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(bait.timeout_secs))
            .build()
        {
            Ok(c) => c,
            Err(e) => return BaitResult::failure(format!("Failed to create HTTP client: {}", e)),
        };

        let payload = match serde_json::to_value(context) {
            Ok(p) => p,
            Err(e) => return BaitResult::failure(format!("Failed to serialize context: {}", e)),
        };

        let start = std::time::Instant::now();

        match client
            .post(&bait.command)
            .json(&payload)
            .header("X-Kaptaind-Bait-ID", &bait.id)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                BaitResult {
                    success: status.is_success(),
                    exit_code: Some(status.as_u16() as i32),
                    stdout: body.clone(),
                    stderr: String::new(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                    data: serde_json::from_str(&body).ok(),
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    BaitResult::timeout()
                } else {
                    BaitResult::failure(format!("HTTP error: {}", e))
                }
            }
        }
    }

    async fn run_command_with_timeout(
        &self,
        mut cmd: Command,
        timeout_secs: u64,
    ) -> BaitResult {
        let timeout_duration = Duration::from_secs(timeout_secs);

        match timeout(timeout_duration, cmd.output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                let data = serde_json::from_str(&stdout).ok();

                BaitResult {
                    success: output.status.success(),
                    exit_code: output.status.code(),
                    stdout,
                    stderr,
                    duration_ms: 0, // Would need actual timing
                    timed_out: false,
                    data,
                }
            }
            Ok(Err(e)) => BaitResult::failure(format!("Process error: {}", e)),
            Err(_) => BaitResult::timeout(),
        }
    }

    fn discover_baits(plugins_dir: &Path) -> Result<Vec<BaitDefinition>> {
        let mut baits = Vec::new();

        if !plugins_dir.exists() {
            return Ok(baits);
        }

        for entry in std::fs::read_dir(plugins_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            // Check for bait manifest
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(manifest) = std::fs::read_to_string(&path) {
                    if let Ok(bait) = serde_json::from_str::<BaitDefinition>(&manifest) {
                        baits.push(bait);
                        continue;
                    }
                }
            }

            // Auto-discover executable scripts
            let file_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string();

            if file_name.starts_with("bait-") || file_name.ends_with("-bait") {
                let bait = BaitDefinition {
                    id: file_name.clone(),
                    name: file_name.clone(),
                    description: format!("Auto-discovered bait from {}", path.display()),
                    bait_type: BaitType::Shell,
                    command: path.to_string_lossy().to_string(),
                    file_patterns: vec![],
                    events: vec![
                        BaitEvent::PreCommit,
                        BaitEvent::PostCommit,
                        BaitEvent::FileChange,
                    ],
                    enabled: true,
                    timeout_secs: 30,
                    env: HashMap::new(),
                };
                baits.push(bait);
            }
        }

        Ok(baits)
    }
}

/// Predefined bait templates.
pub mod templates {
    use super::*;

    /// Create a notification bait.
    pub fn notification_bait(webhook_url: &str) -> BaitDefinition {
        BaitDefinition {
            id: "notify".to_string(),
            name: "Notification".to_string(),
            description: "Send notifications on commit events".to_string(),
            bait_type: BaitType::Webhook,
            command: webhook_url.to_string(),
            file_patterns: vec![],
            events: vec![BaitEvent::PostCommit],
            enabled: true,
            timeout_secs: 10,
            env: HashMap::new(),
        }
    }

    /// Create a metrics collection bait.
    pub fn metrics_bait(endpoint: &str) -> BaitDefinition {
        BaitDefinition {
            id: "metrics".to_string(),
            name: "Metrics Collection".to_string(),
            description: "Send metrics to external endpoint".to_string(),
            bait_type: BaitType::Webhook,
            command: endpoint.to_string(),
            file_patterns: vec![],
            events: vec![
                BaitEvent::ClusterComplete,
                BaitEvent::AnalysisComplete,
            ],
            enabled: true,
            timeout_secs: 15,
            env: HashMap::new(),
        }
    }

    /// Create a custom command bait.
    pub fn custom_command_bait(id: &str, command: &str) -> BaitDefinition {
        BaitDefinition {
            id: id.to_string(),
            name: format!("Custom: {}", id),
            description: format!("Run custom command: {}", command),
            bait_type: BaitType::Shell,
            command: command.to_string(),
            file_patterns: vec![],
            events: vec![BaitEvent::PostCommit],
            enabled: true,
            timeout_secs: 30,
            env: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bait_result_success() {
        let result = BaitResult::success();
        assert!(result.success);
        assert!(!result.timed_out);
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_bait_result_failure() {
        let result = BaitResult::failure("test error");
        assert!(!result.success);
        assert_eq!(result.stderr, "test error");
    }

    #[test]
    fn test_bait_context_serialization() {
        let context = BaitContext {
            event: BaitEvent::PostCommit,
            files: vec![FileChangeInfo {
                path: PathBuf::from("test.rs"),
                change_type: "modified".to_string(),
                size: 100,
            }],
            repo_path: PathBuf::from("/tmp/repo"),
            cluster_id: Some("abc123".to_string()),
            version: Some("1.0.0".to_string()),
            score: Some(0.5),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&context).unwrap();
        assert!(json.contains("PostCommit"));
        assert!(json.contains("test.rs"));
    }

    #[test]
    fn test_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = BaitConfig {
            enabled: true,
            plugins_dir: temp_dir.path().join("baits"),
            baits: vec![],
            auto_discover: false,
        };

        let manager = BaitManager::new(&config, temp_dir.path()).unwrap();
        assert!(manager.list_baits().is_empty());
    }

    #[test]
    fn test_add_remove_baits() {
        let temp_dir = TempDir::new().unwrap();
        let config = BaitConfig {
            enabled: true,
            plugins_dir: temp_dir.path().join("baits"),
            baits: vec![],
            auto_discover: false,
        };

        let mut manager = BaitManager::new(&config, temp_dir.path()).unwrap();

        let bait = BaitDefinition {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test bait".to_string(),
            bait_type: BaitType::Shell,
            command: "echo test".to_string(),
            file_patterns: vec![],
            events: vec![BaitEvent::PostCommit],
            enabled: true,
            timeout_secs: 5,
            env: HashMap::new(),
        };

        manager.add_bait(bait);
        assert_eq!(manager.list_baits().len(), 1);
        assert!(manager.get_bait("test").is_some());

        assert!(manager.remove_bait("test"));
        assert!(manager.get_bait("test").is_none());
        assert!(!manager.remove_bait("test"));
    }

    #[test]
    fn test_enable_disable_baits() {
        let temp_dir = TempDir::new().unwrap();
        let config = BaitConfig {
            enabled: true,
            plugins_dir: temp_dir.path().join("baits"),
            baits: vec![BaitDefinition {
                id: "test".to_string(),
                name: "Test".to_string(),
                description: "Test bait".to_string(),
                bait_type: BaitType::Shell,
                command: "echo test".to_string(),
                file_patterns: vec![],
                events: vec![BaitEvent::PostCommit],
                enabled: true,
                timeout_secs: 5,
                env: HashMap::new(),
            }],
            auto_discover: false,
        };

        let mut manager = BaitManager::new(&config, temp_dir.path()).unwrap();
        assert!(manager.get_bait("test").unwrap().enabled);

        assert!(manager.disable_bait("test"));
        assert!(!manager.get_bait("test").unwrap().enabled);

        assert!(manager.enable_bait("test"));
        assert!(manager.get_bait("test").unwrap().enabled);

        assert!(!manager.disable_bait("nonexistent"));
    }

    #[test]
    fn test_templates() {
        let notify = templates::notification_bait("https://example.com/webhook");
        assert_eq!(notify.id, "notify");
        assert!(matches!(notify.bait_type, BaitType::Webhook));

        let metrics = templates::metrics_bait("https://metrics.example.com/collect");
        assert_eq!(metrics.id, "metrics");

        let custom = templates::custom_command_bait("lint", "cargo clippy");
        assert_eq!(custom.id, "lint");
        assert!(matches!(custom.bait_type, BaitType::Shell));
    }

    #[test]
    fn test_ensure_plugins_dir() {
        let temp_dir = TempDir::new().unwrap();
        let config = BaitConfig {
            enabled: true,
            plugins_dir: temp_dir.path().join("my-baits"),
            baits: vec![],
            auto_discover: false,
        };

        let manager = BaitManager::new(&config, temp_dir.path()).unwrap();
        manager.ensure_plugins_dir().unwrap();

        assert!(temp_dir.path().join("my-baits").exists());
    }
}
