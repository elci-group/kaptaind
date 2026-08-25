//! Jeenome collector for strace-based behavioral analysis.
//!
//! This collector runs strace on test/benchmark commands, processes the output
//! through jeenome for semantic analysis, and attaches the behavioral profile
//! to commits as metadata.

use crate::collectors::{Artifact, Collector, CollectorContext};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

/// Jeenome collector configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JeenomeConfig {
    /// Enable jeenome collection
    pub enabled: bool,
    /// Path to jeenome binary (default: search PATH)
    pub jeenome_path: Option<String>,
    /// Analysis mode: test, benchmark, or both
    pub mode: JeenomeMode,
    /// Minimum importance threshold (0.0-1.0)
    pub threshold: f32,
    /// Whether to retain raw strace logs
    pub retain_strace: bool,
    /// Groq API key for LLM analysis (optional)
    pub groq_api_key: Option<String>,
    /// Groq model to use
    pub groq_model: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JeenomeMode {
    Test,
    Benchmark,
    Both,
}

impl Default for JeenomeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jeenome_path: None,
            mode: JeenomeMode::Test,
            threshold: 0.4,
            retain_strace: false,
            groq_api_key: None,
            groq_model: "llama-3.3-70b-versatile".to_string(),
        }
    }
}

/// Jeenome collector implementation.
pub struct JeenomeCollector {
    config: JeenomeConfig,
    evidence_dir: PathBuf,
}

impl JeenomeCollector {
    pub fn new(config: JeenomeConfig, repo_path: &Path) -> Self {
        let evidence_dir = repo_path.join(".kaptaind").join("evidence");
        Self {
            config,
            evidence_dir,
        }
    }

    /// Find jeenome binary in PATH or use configured path.
    fn find_jeenome_binary(&self) -> Result<PathBuf> {
        if let Some(path) = &self.config.jeenome_path {
            return Ok(PathBuf::from(path));
        }

        // Search in PATH
        if let Ok(output) = Command::new("which").arg("jeenome").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                return Ok(PathBuf::from(path));
            }
        }

        anyhow::bail!("jeenome binary not found in PATH; configure jeenome_path or install jeenome")
    }

    /// Run strace on a command and capture output.
    async fn run_strace(&self, command: &str, repo_path: &Path) -> Result<PathBuf> {
        let strace_dir = self.evidence_dir.join("strace");
        fs::create_dir_all(&strace_dir).await?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let strace_file = strace_dir.join(format!("trace_{}.log", timestamp));

        tracing::info!(command, strace_file = %strace_file.display(), "running strace");

        let output = Command::new("strace")
            .args([
                "-f",  // follow forks
                "-tt", // timestamp with microseconds
                "-T",  // time spent in syscall
                "-o",  // output file
            ])
            .arg(&strace_file) // passed as OsStr; evidence_dir need not be valid UTF-8
            .args(["sh", "-lc", command])
            .current_dir(repo_path)
            .output()
            .context("failed to run strace")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("strace failed: {}", stderr);
        }

        Ok(strace_file)
    }

    /// Run jeenome on a strace file and return JSON output.
    async fn run_jeenome(&self, strace_file: &Path) -> Result<JeenomeOutput> {
        let jeenome_bin = self.find_jeenome_binary()?;

        let mut cmd = Command::new(&jeenome_bin);
        cmd.arg("-i").arg(strace_file);
        cmd.arg("--format").arg("json");
        cmd.arg("--threshold")
            .arg(self.config.threshold.to_string());

        if let Some(api_key) = &self.config.groq_api_key {
            cmd.arg("--groq-api-key").arg(api_key);
            cmd.arg("--groq-model").arg(&self.config.groq_model);
        }

        let output = cmd.output().context("failed to run jeenome")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("jeenome failed: {}", stderr);
        }

        // Parse jeenome JSON output (NDJSON format)
        let stdout = String::from_utf8_lossy(&output.stdout);
        let events: Vec<serde_json::Value> = stdout
            .lines()
            .filter_map(|line| match serde_json::from_str(line) {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::debug!(error = %error, line, "skipping malformed jeenome NDJSON line");
                    None
                }
            })
            .collect();

        Ok(JeenomeOutput::from_events(events))
    }

    /// Store jeenome output as an artifact.
    async fn store_artifact(&self, output: &JeenomeOutput, commit_sha: &str) -> Result<Artifact> {
        let artifact_dir = self.evidence_dir.join(commit_sha);
        fs::create_dir_all(&artifact_dir).await?;

        let artifact_path = artifact_dir.join("jeenome.json");
        let json = serde_json::to_string_pretty(output)?;
        fs::write(&artifact_path, json.clone()).await?;

        // Compute hash
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        // Generate summary for commit trailer
        let summary = format!(
            "events={}, filesystem={}, network={}, processes={}, confidence={:.2}",
            output.total_events,
            output.event_counts.filesystem,
            output.event_counts.network,
            output.event_counts.process,
            output.confidence
        );

        Ok(Artifact {
            artifact_type: "jeenome".to_string(),
            content_hash: hash,
            storage_path: artifact_path
                .strip_prefix(self.evidence_dir.parent().unwrap_or(Path::new(".")))?
                .to_path_buf(),
            metadata: serde_json::to_value(output)?,
            summary,
        })
    }
}

#[async_trait]
impl Collector for JeenomeCollector {
    fn name(&self) -> &str {
        "jeenome"
    }

    async fn collect(&self, ctx: &CollectorContext) -> Result<Option<Artifact>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let Some(command) = &ctx.test_command else {
            tracing::debug!(
                collector = "jeenome",
                "no test command configured, skipping collection"
            );
            return Ok(None);
        };

        // Check if mode matches (test vs benchmark)
        let is_test = command.contains("test") || command.contains("check");
        let matches_mode = match self.config.mode {
            JeenomeMode::Test => is_test,
            JeenomeMode::Benchmark => !is_test,
            JeenomeMode::Both => true,
        };

        if !matches_mode {
            tracing::debug!(mode = ?self.config.mode, "command does not match jeenome mode, skipping");
            return Ok(None);
        }

        let commit_sha = ctx.commit_sha.as_deref().unwrap_or("unknown");

        // Run strace
        let strace_file = self.run_strace(command, &ctx.repo_path).await?;

        // Run jeenome
        let output = self.run_jeenome(&strace_file).await?;

        // Clean up strace file if not configured to retain
        if !self.config.retain_strace {
            fs::remove_file(&strace_file).await?;
        }

        // Store artifact
        let artifact = self.store_artifact(&output, commit_sha).await?;

        tracing::info!(
            events = output.total_events,
            confidence = output.confidence,
            "jeenome collection complete"
        );

        Ok(Some(artifact))
    }
}

/// Parsed jeenome output for kaptaind integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JeenomeOutput {
    pub schema: String,
    pub trace_hash: String,
    pub total_events: usize,
    pub event_counts: EventCounts,
    pub alerts: Vec<String>,
    pub summary: String,
    pub confidence: f32,
    pub high_confidence_hypotheses: Vec<Hypothesis>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventCounts {
    pub filesystem: usize,
    pub network: usize,
    pub process: usize,
    pub memory: usize,
    pub signals: usize,
    pub timing: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub claim: String,
    pub confidence: f32,
    pub category: String,
}

impl JeenomeOutput {
    fn from_events(events: Vec<serde_json::Value>) -> Self {
        let mut counts = EventCounts::default();
        let mut alerts = Vec::new();
        let mut hypotheses = Vec::new();
        let mut confidence: f32 = 0.0;

        for event in &events {
            // Parse behavioral events
            if let Some(_event_type) = event.get("event_type").and_then(|v| v.as_str()) {
                match event.get("category").and_then(|v| v.as_str()) {
                    Some("Filesystem") => counts.filesystem += 1,
                    Some("Network") => counts.network += 1,
                    Some("Process") => counts.process += 1,
                    Some("Memory") => counts.memory += 1,
                    Some("Signals") => counts.signals += 1,
                    Some("Timing") => counts.timing += 1,
                    Some("Errors") => counts.errors += 1,
                    _ => {}
                }
            }

            // Parse LLM deltas (if Groq was enabled)
            if let Some(delta) = event.get("delta") {
                if let Some(new_hypotheses) = delta.get("new_hypotheses").and_then(|v| v.as_array())
                {
                    for h in new_hypotheses {
                        if let (Some(id), Some(claim), Some(conf), Some(cat)) = (
                            h.get("id").and_then(|v| v.as_str()),
                            h.get("claim").and_then(|v| v.as_str()),
                            h.get("confidence").and_then(|v| v.as_f64()),
                            h.get("category").and_then(|v| v.as_str()),
                        ) {
                            hypotheses.push(Hypothesis {
                                id: id.to_string(),
                                claim: claim.to_string(),
                                confidence: conf as f32,
                                category: cat.to_string(),
                            });
                            confidence = confidence.max(conf as f32);
                        }
                    }
                }
            }

            // Check for high-importance events that should be alerts
            if let Some(imp) = event.get("importance").and_then(|v| v.as_f64()) {
                if imp > 0.8 {
                    if let Some(details) = event.get("details") {
                        alerts.push(details.to_string());
                    }
                }
            }
        }

        let summary = if counts.filesystem > 1000 {
            "High filesystem activity detected".to_string()
        } else if counts.network > 10 {
            "Significant network activity".to_string()
        } else if counts.errors > 0 {
            format!("{} errors detected", counts.errors)
        } else {
            "Normal runtime profile".to_string()
        };

        let trace_hash = {
            let mut hasher = Sha256::new();
            let json = serde_json::to_string(&events).unwrap_or_default();
            hasher.update(json.as_bytes());
            format!("{:x}", hasher.finalize())
        };

        Self {
            schema: "jeenome.v1".to_string(),
            trace_hash,
            total_events: events.len(),
            event_counts: counts,
            alerts,
            summary,
            confidence,
            high_confidence_hypotheses: hypotheses,
        }
    }
}
