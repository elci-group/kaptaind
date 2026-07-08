use crate::vacs::extractor::Concept;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredConcept {
    pub concept: Concept,
    pub score: f64,
    pub recommended_asset: String,
    pub priority: String,
}

pub struct ScoringEngine {}

impl Default for ScoringEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoringEngine {
    pub fn new() -> Self {
        Self {}
    }

    pub fn score(&self, concept: Concept) -> ScoredConcept {
        let f = &concept.features;
        let score = (f.complexity * 0.35)
            + (f.explanation_gap * 0.30)
            + (f.visual_affinity * 0.20)
            + ((f.recurrence as f64 * 0.1).min(1.0) * 0.15); // normalized recurrence MVP

        let recommended_asset = if score >= 0.85 {
            "video".to_string()
        } else {
            "diagram".to_string()
        };

        let priority = if score >= 0.80 {
            "high".to_string()
        } else {
            "normal".to_string()
        };

        ScoredConcept {
            concept,
            score,
            recommended_asset,
            priority,
        }
    }
}
