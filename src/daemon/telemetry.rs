use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenMetrics {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub marginal_cost: f64,
    pub aggregate_cost: f64,
}

pub fn track_cost(repo_path: &Path, input_tokens: usize, output_tokens: usize) -> TokenMetrics {
    let telemetry_file = repo_path.join(".kaptaind").join("telemetry.json");
    let mut aggregate_cost = 0.0;
    
    if let Ok(content) = std::fs::read_to_string(&telemetry_file) {
        if let Ok(metrics) = serde_json::from_str::<TokenMetrics>(&content) {
            aggregate_cost = metrics.aggregate_cost;
        }
    }
    
    // Abstract token estimation: $0.0005 per 1k input, $0.0015 per 1k output
    let marginal_cost = (input_tokens as f64 / 1000.0) * 0.0005 + (output_tokens as f64 / 1000.0) * 0.0015;
    aggregate_cost += marginal_cost;
    
    let metrics = TokenMetrics {
        input_tokens,
        output_tokens,
        marginal_cost,
        aggregate_cost,
    };
    
    if let Ok(content) = serde_json::to_string_pretty(&metrics) {
        let _ = std::fs::write(&telemetry_file, content);
    }
    
    metrics
}