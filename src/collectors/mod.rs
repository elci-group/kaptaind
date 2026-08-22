//! Artifact collectors for commit metadata.
//!
//! Collectors gather evidence (test results, benchmarks, strace analysis, etc.)
//! and produce structured artifacts that can be attached to commits as metadata.

pub mod jeenome;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Context provided to collectors during collection.
#[derive(Debug, Clone)]
pub struct CollectorContext {
    /// Repository root path
    pub repo_path: PathBuf,
    /// Current commit SHA (if available)
    pub commit_sha: Option<String>,
    /// Paths changed in the current cluster
    pub cluster_paths: Vec<PathBuf>,
    /// Test command that was run (if any)
    pub test_command: Option<String>,
}

/// Artifact produced by a collector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Artifact type (e.g., "jeenome", "benchmark", "test")
    pub artifact_type: String,
    /// SHA256 hash of the artifact content
    pub content_hash: String,
    /// Path where the artifact is stored (relative to .kaptaind/)
    pub storage_path: PathBuf,
    /// Machine-readable metadata (JSON)
    pub metadata: serde_json::Value,
    /// Human-readable summary for commit trailers
    pub summary: String,
}

/// Trait for collecting artifacts during the commit lifecycle.
#[async_trait]
pub trait Collector: Send + Sync {
    /// Collector name (e.g., "jeenome", "benchmark")
    fn name(&self) -> &str;

    /// Collect evidence and produce an artifact.
    async fn collect(&self, ctx: &CollectorContext) -> anyhow::Result<Option<Artifact>>;
}
