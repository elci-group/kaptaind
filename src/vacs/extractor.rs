use crate::vacs::engine::VacsEvent;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
    pub concept_type: String, // architecture | flow | api | performance | security | state
    pub description: String,
    pub source_refs: SourceRefs,
    pub features: ConceptFeatures,
}

/// Detects concept type based on file patterns and change characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ConceptType {
    Architecture,
    Flow,
    Api,
    Performance,
    Security,
    State,
    Dependency,
    Configuration,
}

impl ConceptType {
    fn as_str(&self) -> &'static str {
        match self {
            ConceptType::Architecture => "architecture",
            ConceptType::Flow => "flow",
            ConceptType::Api => "api",
            ConceptType::Performance => "performance",
            ConceptType::Security => "security",
            ConceptType::State => "state",
            ConceptType::Dependency => "dependency",
            ConceptType::Configuration => "configuration",
        }
    }

    /// Determine concept type from file path and change description
    fn detect(path: &str, description: &str) -> Self {
        let path_lower = path.to_lowercase();
        let desc_lower = description.to_lowercase();

        // Security patterns
        if path_lower.contains("auth") 
            || path_lower.contains("security")
            || path_lower.contains("crypto")
            || path_lower.contains("encrypt")
            || path_lower.contains("password")
            || path_lower.contains("secret")
            || path_lower.contains("token")
            || desc_lower.contains("security")
            || desc_lower.contains("vulnerability")
            || desc_lower.contains("auth") {
            return ConceptType::Security;
        }

        // Performance patterns
        if path_lower.contains("perf")
            || path_lower.contains("benchmark")
            || path_lower.contains("cache")
            || path_lower.contains("optimize")
            || path_lower.contains("memory")
            || desc_lower.contains("performance")
            || desc_lower.contains("optimize")
            || desc_lower.contains("speed")
            || desc_lower.contains("cache") {
            return ConceptType::Performance;
        }

        // API patterns
        if path_lower.contains("/api/")
            || path_lower.contains("route")
            || path_lower.contains("endpoint")
            || path_lower.contains("controller")
            || path_lower.contains("handler")
            || path_lower.contains("proto")
            || path_lower.contains("graphql")
            || path_lower.contains("openapi")
            || desc_lower.contains("api")
            || desc_lower.contains("endpoint")
            || desc_lower.contains("breaking") {
            return ConceptType::Api;
        }

        // Dependency patterns
        if path_lower.contains("cargo.toml")
            || path_lower.contains("package.json")
            || path_lower.contains("requirements.txt")
            || path_lower.contains("dockerfile")
            || path_lower.contains("docker-compose")
            || path_lower.contains("go.mod")
            || path_lower.contains("podfile")
            || path_lower.contains("gemfile")
            || desc_lower.contains("dependency")
            || desc_lower.contains("upgrade")
            || desc_lower.contains("bump") {
            return ConceptType::Dependency;
        }

        // Configuration patterns
        if path_lower.contains("config")
            || path_lower.contains(".env")
            || path_lower.contains(".yaml")
            || path_lower.contains(".yml")
            || path_lower.contains(".toml")
            || path_lower.contains("settings")
            || path_lower.contains("deployment")
            || path_lower.contains("k8s")
            || path_lower.contains("kubernetes")
            || path_lower.contains("helm")
            || path_lower.contains("terraform")
            || desc_lower.contains("config")
            || desc_lower.contains("deploy") {
            return ConceptType::Configuration;
        }

        // Flow patterns (control flow, workflow)
        if path_lower.contains("workflow")
            || path_lower.contains("pipeline")
            || path_lower.contains("state machine")
            || path_lower.contains("orchestrat")
            || desc_lower.contains("flow")
            || desc_lower.contains("workflow")
            || desc_lower.contains("sequence")
            || desc_lower.contains("state") {
            return ConceptType::Flow;
        }

        // State patterns (data models, persistence)
        if path_lower.contains("model")
            || path_lower.contains("entity")
            || path_lower.contains("schema")
            || path_lower.contains("migration")
            || path_lower.contains("database")
            || path_lower.contains("db/")
            || path_lower.contains("store")
            || desc_lower.contains("database")
            || desc_lower.contains("schema")
            || desc_lower.contains("migration")
            || desc_lower.contains("state") {
            return ConceptType::State;
        }

        // Architecture patterns (core structure)
        if path_lower.contains("mod.rs")
            || path_lower.contains("lib.rs")
            || path_lower.contains("main.rs")
            || path_lower.contains("core/")
            || path_lower.contains("arch")
            || path_lower.contains("structure")
            || desc_lower.contains("refactor")
            || desc_lower.contains("restructure")
            || desc_lower.contains("architecture")
            || desc_lower.contains("organize") {
            return ConceptType::Architecture;
        }

        // Default to architecture for significant changes
        ConceptType::Architecture
    }

    /// Visual affinity score - how well this concept type translates to visual representation
    fn visual_affinity(&self) -> f64 {
        match self {
            ConceptType::Flow => 0.95,        // Flowcharts are natural
            ConceptType::Architecture => 0.90, // Diagrams work well
            ConceptType::State => 0.85,       // State machines
            ConceptType::Api => 0.80,         // API diagrams
            ConceptType::Dependency => 0.75,  // Dependency graphs
            ConceptType::Configuration => 0.60, // Less visual
            ConceptType::Performance => 0.70, // Charts/graphs
            ConceptType::Security => 0.65,    // Flow diagrams
        }
    }
}

pub struct ConceptExtractor {
    /// Historical concepts for detecting recurrence
    history: std::sync::Mutex<Vec<(String, ConceptType)>>,
}

impl ConceptExtractor {
    pub fn new() -> Self {
        Self {
            history: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn extract(&self, event: &VacsEvent) -> Vec<Concept> {
        // Skip low-complexity events
        if event.payload.complexity_score < 0.15 {
            return vec![];
        }

        let files = &event.payload.files_changed;
        if files.is_empty() {
            return vec![];
        }

        // Group files by detected concept type
        let mut type_groups: std::collections::HashMap<ConceptType, Vec<String>> = std::collections::HashMap::new();
        
        for file in files {
            let concept_type = ConceptType::detect(file, &event.payload.diff_summary);
            type_groups.entry(concept_type).or_default().push(file.clone());
        }

        // Create a concept for each significant group
        let mut concepts = Vec::new();
        
        for (concept_type, type_files) in type_groups {
            // Skip groups with too few files (likely noise)
            if type_files.len() < 2 && event.payload.complexity_score < 0.4 {
                continue;
            }

            let concept = self.create_concept(
                event,
                concept_type,
                &type_files,
                &files,
            );
            
            concepts.push(concept);
        }

        // If no concepts extracted, create a general one
        if concepts.is_empty() && event.payload.complexity_score >= 0.3 {
            concepts.push(self.create_concept(
                event,
                ConceptType::Architecture,
                files,
                files,
            ));
        }

        // Track concepts for recurrence detection
        {
            let mut history = self.history.lock().unwrap();
            for concept in &concepts {
                history.push((concept.concept_id.clone(), 
                    match concept.concept_type.as_str() {
                        "flow" => ConceptType::Flow,
                        "api" => ConceptType::Api,
                        "performance" => ConceptType::Performance,
                        "security" => ConceptType::Security,
                        "state" => ConceptType::State,
                        "dependency" => ConceptType::Dependency,
                        "configuration" => ConceptType::Configuration,
                        _ => ConceptType::Architecture,
                    }));
            }
            // Keep last 100 entries
            let len = history.len();
            if len > 100 {
                history.drain(0..len - 100);
            }
        }

        concepts
    }

    fn create_concept(
        &self,
        event: &VacsEvent,
        concept_type: ConceptType,
        type_files: &[String],
        all_files: &[String],
    ) -> Concept {
        // Generate unique concept ID
        let id_base = format!("{}{}{}", event.timestamp, concept_type.as_str(), type_files.join(","));
        let mut hasher = DefaultHasher::new();
        std::hash::Hash::hash(&id_base, &mut hasher);
        let concept_id = format!("{:x}", std::hash::Hasher::finish(&hasher))[..16].to_string();

        // Calculate recurrence
        let recurrence = self.calculate_recurrence(concept_type, type_files);

        // Calculate explanation gap
        let explanation_gap = self.calculate_explanation_gap(event, concept_type, type_files);

        // Calculate change magnitude
        let change_magnitude = (type_files.len() as f64 / all_files.len().max(1) as f64)
            * event.payload.complexity_score;

        // Generate description based on concept type
        let description = self.generate_description(concept_type, type_files, &event.payload.diff_summary);

        Concept {
            concept_id,
            concept_type: concept_type.as_str().to_string(),
            description,
            source_refs: SourceRefs {
                commits: vec![], // Will be populated by caller
                files: type_files.to_vec(),
                symbols: self.extract_symbols(&event.payload.diff_summary),
            },
            features: ConceptFeatures {
                complexity: event.payload.complexity_score,
                recurrence,
                change_magnitude: change_magnitude.min(1.0),
                explanation_gap,
                visual_affinity: concept_type.visual_affinity(),
            },
        }
    }

    fn calculate_recurrence(&self, concept_type: ConceptType, files: &[String]) -> u32 {
        let history = self.history.lock().unwrap();
        
        let type_matches = history.iter()
            .filter(|(_, ct)| *ct == concept_type)
            .count();
        
        let file_matches: usize = history.iter()
            .filter(|(id, _)| files.iter().any(|f| id.contains(&f.replace('/', "_"))))
            .count();
        
        (type_matches + file_matches).min(10) as u32
    }

    fn calculate_explanation_gap(
        &self,
        event: &VacsEvent,
        concept_type: ConceptType,
        files: &[String],
    ) -> f64 {
        // Higher gap for:
        // - Complex changes without clear descriptions
        // - Security/Performance changes (need explanation)
        // - Many files changed
        
        let mut gap = 0.5; // Base gap

        // Adjust based on complexity
        gap += event.payload.complexity_score * 0.3;

        // Security and performance need more explanation
        gap += match concept_type {
            ConceptType::Security => 0.2,
            ConceptType::Performance => 0.15,
            ConceptType::Api => 0.1,
            _ => 0.0,
        };

        // More files = higher potential for confusion
        gap += (files.len() as f64 * 0.02).min(0.15);

        // Reduce gap if commit message is descriptive
        let summary_lower = event.payload.diff_summary.to_lowercase();
        let descriptive_words = ["add", "fix", "refactor", "optimize", "implement", "remove"];
        if descriptive_words.iter().any(|w| summary_lower.contains(w)) {
            gap -= 0.1;
        }

        gap.clamp(0.1, 0.95)
    }

    fn generate_description(
        &self,
        concept_type: ConceptType,
        files: &[String],
        summary: &str,
    ) -> String {
        let type_name = match concept_type {
            ConceptType::Architecture => "Architectural",
            ConceptType::Flow => "Control flow",
            ConceptType::Api => "API surface",
            ConceptType::Performance => "Performance",
            ConceptType::Security => "Security",
            ConceptType::State => "Data model",
            ConceptType::Dependency => "Dependency",
            ConceptType::Configuration => "Configuration",
        };

        let scope = if files.len() == 1 {
            format!("in {}", files[0])
        } else if files.len() <= 3 {
            format!("across {}", files.join(", "))
        } else {
            format!("spanning {} files", files.len())
        };

        // Try to extract action from summary
        let action = if summary.to_lowercase().contains("add") {
            "addition"
        } else if summary.to_lowercase().contains("remove") || summary.to_lowercase().contains("delete") {
            "removal"
        } else if summary.to_lowercase().contains("fix") {
            "fix"
        } else if summary.to_lowercase().contains("refactor") {
            "refactoring"
        } else if summary.to_lowercase().contains("update") || summary.to_lowercase().contains("upgrade") {
            "update"
        } else {
            "change"
        };

        format!("{} {} {}", type_name, action, scope)
    }

    fn extract_symbols(&self, summary: &str) -> Vec<String> {
        // Extract function names, struct names, etc. from summary
        // Simple heuristic: look for backticks and quoted strings
        let mut symbols = Vec::new();
        
        // Match backtick-quoted identifiers
        for cap in summary.split('`').skip(1).step_by(2) {
            let symbol = cap.trim();
            if !symbol.is_empty() && symbol.len() < 100 {
                symbols.push(symbol.to_string());
            }
        }
        
        // Match quoted strings
        for cap in summary.split('"').skip(1).step_by(2) {
            let symbol = cap.trim();
            if !symbol.is_empty() && symbol.len() < 100 && !symbol.contains(' ') {
                symbols.push(symbol.to_string());
            }
        }

        symbols.truncate(10);
        symbols
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_event(files: Vec<String>, summary: String, complexity: f64) -> VacsEvent {
        VacsEvent {
            event_type: "commit.created".to_string(),
            timestamp: Utc::now(),
            project_id: "test".to_string(),
            payload: crate::vacs::engine::VacsPayload {
                files_changed: files,
                diff_summary: summary,
                aoc_id: None,
                complexity_score: complexity,
            },
        }
    }

    #[test]
    fn test_detect_api_concept() {
        let extractor = ConceptExtractor::new();
        let event = create_test_event(
            vec!["src/api/routes.rs".to_string(), "src/api/handlers.rs".to_string()],
            "Add new API endpoints for user management".to_string(),
            0.6,
        );
        
        let concepts = extractor.extract(&event);
        assert!(!concepts.is_empty());
        assert!(concepts.iter().any(|c| c.concept_type == "api"));
    }

    #[test]
    fn test_detect_security_concept() {
        let extractor = ConceptExtractor::new();
        let event = create_test_event(
            vec!["src/auth/login.rs".to_string()],
            "Fix authentication vulnerability".to_string(),
            0.5,
        );
        
        let concepts = extractor.extract(&event);
        assert!(!concepts.is_empty());
        assert!(concepts.iter().any(|c| c.concept_type == "security"));
    }

    #[test]
    fn test_low_complexity_skipped() {
        let extractor = ConceptExtractor::new();
        let event = create_test_event(
            vec!["src/main.rs".to_string()],
            "Minor update".to_string(),
            0.05,
        );
        
        let concepts = extractor.extract(&event);
        assert!(concepts.is_empty());
    }

    #[test]
    fn test_concept_features_calculated() {
        let extractor = ConceptExtractor::new();
        let event = create_test_event(
            vec!["src/core/engine.rs".to_string(), "src/core/worker.rs".to_string()],
            "Refactor core architecture".to_string(),
            0.8,
        );
        
        let concepts = extractor.extract(&event);
        assert!(!concepts.is_empty());
        
        let concept = &concepts[0];
        assert!(concept.features.complexity > 0.0);
        assert!(concept.features.visual_affinity > 0.0);
        assert!(concept.features.explanation_gap > 0.0);
    }
}
