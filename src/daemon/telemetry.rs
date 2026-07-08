use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub requests: u64,
    pub marginal_cost: f64,
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
    /// Usage aggregated by inference provider.
    #[serde(default)]
    pub per_provider: HashMap<String, ProviderUsage>,
    /// Usage aggregated by model name.
    #[serde(default)]
    pub per_model: HashMap<String, ProviderUsage>,
}

pub fn track_cost(
    repo_path: &Path,
    provider: &str,
    model: &str,
    input_tokens: usize,
    output_tokens: usize,
) -> TokenMetrics {
    let telemetry_file = repo_path.join(".kaptaind").join("telemetry.json");
    if let Some(parent) = telemetry_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut existing = load(repo_path);

    // Abstract token estimation: $0.0005 per 1k input, $0.0015 per 1k output
    let marginal_cost =
        (input_tokens as f64 / 1000.0) * 0.0005 + (output_tokens as f64 / 1000.0) * 0.0015;
    existing.aggregate_cost += marginal_cost;

    let provider_key = provider.to_lowercase();
    let model_key = model.to_lowercase();

    let update_usage = |map: &mut HashMap<String, ProviderUsage>, key: String| {
        let entry = map.entry(key).or_default();
        entry.input_tokens += input_tokens;
        entry.output_tokens += output_tokens;
        entry.requests += 1;
        entry.marginal_cost += marginal_cost;
    };

    update_usage(&mut existing.per_provider, provider_key);
    update_usage(&mut existing.per_model, model_key);

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
        per_provider: existing.per_provider.clone(),
        per_model: existing.per_model.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn track_cost_aggregates_per_provider_and_model() {
        let dir = tempdir().unwrap();
        let m1 = track_cost(dir.path(), "openai", "gpt-4o", 1000, 500);
        assert_eq!(m1.per_provider["openai"].requests, 1);
        assert_eq!(m1.per_model["gpt-4o"].requests, 1);

        let m2 = track_cost(dir.path(), "openai", "gpt-4o", 2000, 1000);
        assert_eq!(m2.per_provider["openai"].requests, 2);
        assert_eq!(m2.per_provider["openai"].input_tokens, 3000);
        assert_eq!(m2.per_model["gpt-4o"].output_tokens, 1500);
        assert!(m2.aggregate_cost > m1.aggregate_cost);

        let _m3 = track_cost(dir.path(), "anthropic", "claude", 500, 250);
        let metrics = load(dir.path());
        assert_eq!(metrics.per_provider.len(), 2);
        assert_eq!(metrics.per_model.len(), 2);
        assert_eq!(metrics.per_provider["anthropic"].requests, 1);
        assert_eq!(metrics.per_model["claude"].input_tokens, 500);
    }

    #[test]
    fn load_returns_defaults_when_missing() {
        let dir = tempdir().unwrap();
        let metrics = load(dir.path());
        assert_eq!(metrics.input_tokens, 0);
        assert_eq!(metrics.aggregate_cost, 0.0);
        assert!(metrics.per_provider.is_empty());
        assert!(metrics.per_model.is_empty());
    }

    #[test]
    fn legacy_telemetry_without_usage_maps_defaults_to_empty() {
        let dir = tempdir().unwrap();
        let kaptaind_dir = dir.path().join(".kaptaind");
        std::fs::create_dir_all(&kaptaind_dir).unwrap();
        let legacy = r#"{"input_tokens":100,"output_tokens":50,"marginal_cost":0.0001,"aggregate_cost":0.0001}"#;
        std::fs::write(kaptaind_dir.join("telemetry.json"), legacy).unwrap();
        let metrics = load(dir.path());
        assert_eq!(metrics.input_tokens, 100);
        assert!(metrics.per_provider.is_empty());
    }
}
