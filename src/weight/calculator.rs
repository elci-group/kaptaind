use crate::diff::DiffAnalysis;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct WeightConfig {
    #[serde(default = "default_weight_s")]
    pub s: f32,
    #[serde(default = "default_weight_a")]
    pub a: f32,
    #[serde(default = "default_weight_d")]
    pub d: f32,
    #[serde(default = "default_weight_r")]
    pub r: f32,
    #[serde(default)]
    pub b: f32,
}

fn default_weight_s() -> f32 {
    0.35
}
fn default_weight_a() -> f32 {
    0.3
}
fn default_weight_d() -> f32 {
    0.2
}
fn default_weight_r() -> f32 {
    0.15
}

impl Default for WeightConfig {
    fn default() -> Self {
        Self {
            s: default_weight_s(),
            a: default_weight_a(),
            d: default_weight_d(),
            r: default_weight_r(),
            b: 0.0,
        }
    }
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
