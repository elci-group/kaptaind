use serde::{Deserialize, Serialize};
use std::path::Path;

fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenMetrics {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub marginal_cost: f64,
    pub aggregate_cost: f64,
    /// Latest stability score from the qualification engine.
    #[serde(default)]
    pub stability: f64,
    /// Total successful releases emitted.
    #[serde(default)]
    pub releases: u64,
    /// Total failed release attempts.
    #[serde(default)]
    pub failed_releases: u64,
    /// Cumulative AST cache hits across analyses.
    #[serde(default)]
    pub ast_cache_hits: u64,
    /// Cumulative AST cache misses across analyses.
    #[serde(default)]
    pub ast_cache_misses: u64,
    /// Latest observed entry count in the AST cache.
    #[serde(default)]
    pub ast_cache_entries: u64,
}

pub fn track_cost(repo_path: &Path, input_tokens: usize, output_tokens: usize) -> TokenMetrics {
    let telemetry_file = repo_path.join(".kaptaind").join("telemetry.json");
    let mut existing = load(repo_path);

    // Abstract token estimation: $0.0005 per 1k input, $0.0015 per 1k output
    let marginal_cost =
        (input_tokens as f64 / 1000.0) * 0.0005 + (output_tokens as f64 / 1000.0) * 0.0015;
    existing.aggregate_cost += marginal_cost;

    let metrics = TokenMetrics {
        input_tokens,
        output_tokens,
        marginal_cost,
        aggregate_cost: existing.aggregate_cost,
        stability: existing.stability,
        releases: existing.releases,
        failed_releases: existing.failed_releases,
        ast_cache_hits: existing.ast_cache_hits,
        ast_cache_misses: existing.ast_cache_misses,
        ast_cache_entries: existing.ast_cache_entries,
    };

    if let Ok(content) = serde_json::to_string_pretty(&metrics) {
        let _ = write_atomic(&telemetry_file, content.as_bytes());
    }

    metrics
}

/// Update the stability score and release counters in telemetry.
///
/// `release_succeeded` – pass `true` on a successful release, `false` on a
/// failed attempt.  Pass `false` with no other release intent just to update
/// the stability score without incrementing any counter.
pub fn update_release_metrics(repo_path: &Path, stability: f64, release_succeeded: bool) {
    let telemetry_file = repo_path.join(".kaptaind").join("telemetry.json");
    let mut metrics = load(repo_path);
    metrics.stability = stability;
    if release_succeeded {
        metrics.releases += 1;
    } else {
        metrics.failed_releases += 1;
    }
    if let Ok(content) = serde_json::to_string_pretty(&metrics) {
        let _ = write_atomic(&telemetry_file, content.as_bytes());
    }
}

/// Update cumulative AST cache metrics from an analysis run.
pub fn update_cache_metrics(repo_path: &Path, hits: usize, misses: usize, entries: usize) {
    let telemetry_file = repo_path.join(".kaptaind").join("telemetry.json");
    let mut metrics = load(repo_path);
    metrics.ast_cache_hits = metrics.ast_cache_hits.saturating_add(hits as u64);
    metrics.ast_cache_misses = metrics.ast_cache_misses.saturating_add(misses as u64);
    metrics.ast_cache_entries = entries as u64;
    if let Some(parent) = telemetry_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(&metrics) {
        let _ = write_atomic(&telemetry_file, content.as_bytes());
    }
}

fn load(repo_path: &Path) -> TokenMetrics {
    let path = repo_path.join(".kaptaind").join("telemetry.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}