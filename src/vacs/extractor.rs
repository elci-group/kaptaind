use crate::vacs::engine::VacsEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRefs {
    pub commits: Vec<String>,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptFeatures {
    pub complexity: f64,
    pub recurrence: u32,
    pub change_magnitude: f64,
    pub explanation_gap: f64,
    pub visual_affinity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub concept_id: String,
    pub concept_type: String, // architecture | flow | api | performance | state
    pub description: String,
    pub source_refs: SourceRefs,
    pub features: ConceptFeatures,
}

pub struct ConceptExtractor {}

impl ConceptExtractor {
    pub fn new() -> Self {
        Self {}
    }

    pub fn extract(&self, event: &VacsEvent) -> Vec<Concept> {
        // Basic heuristic extraction for MVP
        if event.payload.complexity_score < 0.2 {
            return vec![];
        }

        let id_base = format!("{}{}", event.timestamp, event.payload.diff_summary);
        
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&id_base, &mut hasher);
        let concept_id = format!("{:x}", std::hash::Hasher::finish(&hasher));

        vec![Concept {
            concept_id,
            concept_type: "architecture".to_string(), // MVP default
            description: event.payload.diff_summary.clone(),
            source_refs: SourceRefs {
                commits: vec![],
                files: event.payload.files_changed.clone(),
                symbols: vec![],
            },
            features: ConceptFeatures {
                complexity: event.payload.complexity_score,
                recurrence: 1,
                change_magnitude: event.payload.complexity_score, // Simplified MVP
                explanation_gap: 0.8, // Assume high gap for MVP
                visual_affinity: 0.7, // Basic default
            },
        }]
    }
}