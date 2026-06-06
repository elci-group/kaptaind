use crate::diff::DiffAnalysis;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct WeightConfig {
    pub s: f32,
    pub a: f32,
    pub d: f32,
    pub r: f32,
    #[serde(default)]
    pub b: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightResult {
    pub score: f32,
    pub api_breaking: bool,
    pub api_added: bool,
}

pub fn compute(diff: &DiffAnalysis, cfg: &WeightConfig) -> WeightResult {
    let score = cfg.s * diff.structural
        + cfg.a * diff.api
        + cfg.d * diff.deps
        + cfg.r * diff.runtime
        + cfg.b * diff.bundle;

    WeightResult {
        score,
        api_breaking: diff.api_breaking,
        api_added: diff.api_added,
    }
}
