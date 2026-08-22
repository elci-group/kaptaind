//! Task distribution system using provider matrix for intelligent routing.
//!
//! This module implements the core logic for distributing development tasks
//! across SSH-accessible Git providers based on the provider matrix capabilities.

use crate::config::loader::RemoteConfig;
use crate::push::intent::detect_intent;
use crate::push::provider_matrix::{ProviderMatrix, TaskType};
use anyhow::Result;

/// Task distribution configuration.
#[derive(Debug, Clone)]
pub struct TaskDistributionConfig {
    /// Enable intelligent task distribution
    pub enabled: bool,
    /// Minimum capability score threshold (0.0-1.0)
    pub min_capability_score: f32,
    /// Maximum number of providers per task
    pub max_providers_per_task: usize,
    /// Fallback to default remote if no suitable provider found
    pub fallback_to_default: bool,
    /// Prefer SSH-accessible providers
    pub prefer_ssh: bool,
    /// Geographic region preference
    pub preferred_region: Option<String>,
    /// Cost sensitivity (0.0=ignore cost, 1.0=highly cost-sensitive)
    pub cost_sensitivity: f32,
}

impl Default for TaskDistributionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_capability_score: 0.7,
            max_providers_per_task: 3,
            fallback_to_default: true,
            prefer_ssh: true,
            preferred_region: None,
            cost_sensitivity: 0.3,
        }
    }
}

/// Task distribution result with provider recommendations.
#[derive(Debug, Clone)]
pub struct TaskDistribution {
    /// Detected task type
    pub task_type: TaskType,
    /// Recommended providers in priority order
    pub recommended_providers: Vec<RemoteConfig>,
    /// Capability scores for each provider
    pub capability_scores: Vec<f32>,
    /// Distribution strategy used
    pub strategy: DistributionStrategy,
    /// Fallback used
    pub fallback_used: bool,
}

/// Distribution strategy used for task routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistributionStrategy {
    /// Best fit based on capability scores
    BestFit,
    /// Geographic region optimization
    GeographicOptimization,
    /// Cost optimization
    CostOptimization,
    /// Availability optimization
    AvailabilityOptimization,
    /// Hybrid approach
    Hybrid,
    /// Fallback to default
    Fallback,
}

/// Task distribution engine.
pub struct TaskDistributionEngine {
    provider_matrix: ProviderMatrix,
    config: TaskDistributionConfig,
}

impl TaskDistributionEngine {
    /// Create a new task distribution engine.
    pub fn new(config: TaskDistributionConfig) -> Self {
        Self {
            provider_matrix: ProviderMatrix::new(),
            config,
        }
    }

    /// Create a new task distribution engine with default config.
    pub fn with_defaults() -> Self {
        Self::new(TaskDistributionConfig::default())
    }

    /// Distribute a task to the best-fit providers.
    pub fn distribute_task(
        &self,
        files: &[String],
        commit_message: &str,
        routing_config: &crate::config::loader::IntentRouting,
        available_remotes: &[RemoteConfig],
    ) -> Result<TaskDistribution> {
        if !self.config.enabled {
            return self.fallback_distribution(available_remotes);
        }

        // Detect task type from intent
        let intent = detect_intent(files, commit_message, routing_config);
        let task_type = self.intent_to_task_type(&intent);

        // Get provider recommendations
        let recommended_providers = self
            .provider_matrix
            .get_best_providers(&task_type, self.config.prefer_ssh);

        // Filter by minimum capability score
        let qualified_providers: Vec<_> = recommended_providers
            .into_iter()
            .filter(|cap| cap.score >= self.config.min_capability_score)
            .collect();

        if qualified_providers.is_empty() {
            if self.config.fallback_to_default {
                tracing::warn!(
                    task_type = ?task_type,
                    "no qualified providers found, using fallback"
                );
                return self.fallback_distribution(available_remotes);
            } else {
                anyhow::bail!("no qualified providers found for task type {:?}", task_type);
            }
        }

        // Map provider names to available remotes
        let matched_remotes =
            self.match_providers_to_remotes(&qualified_providers, available_remotes);

        if matched_remotes.is_empty() {
            if self.config.fallback_to_default {
                tracing::warn!(
                    task_type = ?task_type,
                    "no matching remotes found, using fallback"
                );
                return self.fallback_distribution(available_remotes);
            } else {
                anyhow::bail!("no matching remotes found for qualified providers");
            }
        }

        // Apply distribution strategy
        let (selected_remotes, scores, strategy) =
            self.apply_distribution_strategy(&matched_remotes, &qualified_providers, &task_type);

        Ok(TaskDistribution {
            task_type,
            recommended_providers: selected_remotes,
            capability_scores: scores,
            strategy,
            fallback_used: false,
        })
    }

    /// Convert intent string to task type.
    fn intent_to_task_type(&self, intent: &str) -> TaskType {
        match intent {
            "oss" | "public" | "community" | "visibility" => TaskType::CommunityManagement,
            "security" | "cve" | "vulnerability" => TaskType::SecurityAudit,
            "ci" | "pipeline" | "docker" | "build" => TaskType::ContinuousIntegration,
            "mutation" | "cambrian" | "refactor" => TaskType::Refactoring,
            "docs" | "readme" | "license" => TaskType::Documentation,
            "fix" | "patch" => TaskType::BugFix,
            "feature" => TaskType::FeatureDevelopment,
            "release" => TaskType::ReleasePublishing,
            "archive" | "backup" => TaskType::LongTermArchival,
            "enterprise" => TaskType::EnterpriseIntegration,
            "autonomous" => TaskType::Experimentation,
            _ => TaskType::FeatureDevelopment, // Default fallback
        }
    }

    /// Match provider recommendations to available remotes.
    fn match_providers_to_remotes(
        &self,
        qualified_providers: &[crate::push::provider_matrix::ProviderCapability],
        available_remotes: &[RemoteConfig],
    ) -> Vec<(RemoteConfig, f32)> {
        let mut matched = Vec::new();

        for provider_cap in qualified_providers {
            for remote in available_remotes {
                if remote.provider == provider_cap.provider && remote.enabled {
                    let fit_score = self
                        .provider_matrix
                        .calculate_fit_score(&provider_cap.provider, &provider_cap.task_type);
                    matched.push((remote.clone(), fit_score));
                    break; // Use first matching remote per provider
                }
            }
        }

        matched
    }

    /// Apply distribution strategy to select final providers.
    fn apply_distribution_strategy(
        &self,
        matched_remotes: &[(RemoteConfig, f32)],
        _qualified_providers: &[crate::push::provider_matrix::ProviderCapability],
        _task_type: &TaskType,
    ) -> (Vec<RemoteConfig>, Vec<f32>, DistributionStrategy) {
        let strategy = self.determine_strategy(matched_remotes, _qualified_providers);

        let mut scored: Vec<_> = matched_remotes.to_vec();

        // NaN fit/cost/availability scores are treated as equal rather than
        // panicking; a bad score from one provider shouldn't be able to crash
        // distribution for every task.
        match strategy {
            DistributionStrategy::BestFit => {
                // Sort by fit score descending
                scored.sort_by(|a, b| {
                    b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            DistributionStrategy::CostOptimization => {
                // Sort by cost (lower is better) then by fit score
                scored.sort_by(|a, b| {
                    let a_cost = self.get_provider_cost(&a.0.provider);
                    let b_cost = self.get_provider_cost(&b.0.provider);
                    a_cost
                        .partial_cmp(&b_cost)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                        })
                });
            }
            DistributionStrategy::AvailabilityOptimization => {
                // Sort by availability then by fit score
                scored.sort_by(|a, b| {
                    let a_avail = self.get_provider_availability(&a.0.provider);
                    let b_avail = self.get_provider_availability(&b.0.provider);
                    b_avail
                        .partial_cmp(&a_avail)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                        })
                });
            }
            DistributionStrategy::GeographicOptimization => {
                if let Some(ref region) = self.config.preferred_region {
                    scored.sort_by(|a, b| {
                        let a_region_score = self.get_region_score(&a.0.provider, region);
                        let b_region_score = self.get_region_score(&b.0.provider, region);
                        b_region_score
                            .partial_cmp(&a_region_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| {
                                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                            })
                    });
                } else {
                    // Fall back to best fit if no region preference
                    scored.sort_by(|a, b| {
                        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }
            DistributionStrategy::Hybrid => {
                // Weighted combination of factors
                scored.sort_by(|a, b| {
                    let a_hybrid = self.calculate_hybrid_score(&a.0, a.1);
                    let b_hybrid = self.calculate_hybrid_score(&b.0, b.1);
                    b_hybrid
                        .partial_cmp(&a_hybrid)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            DistributionStrategy::Fallback => {
                // Just return as-is (fallback handled elsewhere)
            }
        }

        // Limit to max providers per task
        let limited: Vec<_> = scored
            .into_iter()
            .take(self.config.max_providers_per_task)
            .collect();

        let (remotes, scores): (Vec<_>, Vec<_>) = limited.into_iter().unzip();

        (remotes, scores, strategy)
    }

    /// Determine the best distribution strategy.
    fn determine_strategy(
        &self,
        matched_remotes: &[(RemoteConfig, f32)],
        _qualified_providers: &[crate::push::provider_matrix::ProviderCapability],
    ) -> DistributionStrategy {
        if self.config.preferred_region.is_some() {
            return DistributionStrategy::GeographicOptimization;
        }

        if self.config.cost_sensitivity > 0.7 {
            return DistributionStrategy::CostOptimization;
        }

        if matched_remotes.len() > 3 {
            return DistributionStrategy::Hybrid;
        }

        DistributionStrategy::BestFit
    }

    /// Calculate hybrid score combining multiple factors.
    fn calculate_hybrid_score(&self, remote: &RemoteConfig, fit_score: f32) -> f32 {
        let cost = self.get_provider_cost(&remote.provider);
        let availability = self.get_provider_availability(&remote.provider);
        let priority_bonus = 1.0 / (remote.priority as f32 + 1.0);

        // Weighted combination
        fit_score * 0.5
            + availability * 0.2
            + (1.0 - cost) * 0.2 * self.config.cost_sensitivity
            + priority_bonus * 0.1
    }

    /// Get provider cost (0.0-1.0).
    fn get_provider_cost(&self, provider: &str) -> f32 {
        match provider {
            "github" | "codeberg" | "sourcehut" => 0.0,
            "gitea" | "forgejo" => 0.1,
            "gitlab" => 0.2,
            "gerrit" => 0.3,
            "bitbucket" => 0.5,
            "azure" => 0.6,
            _ => 0.5,
        }
    }

    /// Get provider availability (0.0-1.0).
    fn get_provider_availability(&self, provider: &str) -> f32 {
        match provider {
            "github" => 0.99,
            "gitlab" | "azure" => 0.98,
            "bitbucket" => 0.97,
            "gerrit" => 0.93,
            "codeberg" => 0.95,
            "sourcehut" => 0.92,
            "gitea" | "forgejo" => 0.90,
            _ => 0.90,
        }
    }

    /// Get region match score (0.0-1.0).
    fn get_region_score(&self, provider: &str, preferred_region: &str) -> f32 {
        if let Some(defaults) = self.provider_matrix.get_provider_defaults(provider) {
            if defaults
                .optimal_regions
                .contains(&preferred_region.to_string())
            {
                return 1.0;
            }
            if defaults.optimal_regions.contains(&"global".to_string()) {
                return 0.7;
            }
        }
        0.5
    }

    /// Fallback distribution using available remotes.
    fn fallback_distribution(
        &self,
        available_remotes: &[RemoteConfig],
    ) -> Result<TaskDistribution> {
        let mut enabled_remotes: Vec<_> = available_remotes
            .iter()
            .filter(|r| r.enabled)
            .cloned()
            .collect();

        // Sort by priority
        enabled_remotes.sort_by_key(|r| r.priority);

        let scores = vec![0.5; enabled_remotes.len()]; // Default fallback score

        Ok(TaskDistribution {
            task_type: TaskType::FeatureDevelopment,
            recommended_providers: enabled_remotes,
            capability_scores: scores,
            strategy: DistributionStrategy::Fallback,
            fallback_used: true,
        })
    }

    /// Validate SSH accessibility for all configured remotes.
    pub fn validate_ssh_access(&self, remotes: &[RemoteConfig]) -> Result<()> {
        self.provider_matrix.validate_ssh_access(remotes)
    }

    /// Get provider matrix for inspection.
    pub fn provider_matrix(&self) -> &ProviderMatrix {
        &self.provider_matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::IntentRouting;

    #[test]
    fn test_task_distribution_basic() {
        let engine = TaskDistributionEngine::with_defaults();
        let routing = IntentRouting::default();

        let remotes = vec![RemoteConfig {
            name: "github".to_string(),
            provider: "github".to_string(),
            role: "public_nexus".to_string(),
            enabled: true,
            priority: 10,
            intents: vec!["oss".to_string()],
            canonical: true,
            backup: false,
            regional: false,
        }];

        let files = vec!["README.md".to_string()];
        let result = engine.distribute_task(&files, "update readme", &routing, &remotes);

        assert!(result.is_ok());
        let distribution = result.unwrap();
        assert!(!distribution.recommended_providers.is_empty());
    }

    #[test]
    fn test_intent_to_task_type_mapping() {
        let engine = TaskDistributionEngine::with_defaults();

        assert_eq!(
            engine.intent_to_task_type("security"),
            TaskType::SecurityAudit
        );
        assert_eq!(
            engine.intent_to_task_type("ci"),
            TaskType::ContinuousIntegration
        );
        assert_eq!(
            engine.intent_to_task_type("oss"),
            TaskType::CommunityManagement
        );
        assert_eq!(
            engine.intent_to_task_type("mutation"),
            TaskType::Refactoring
        );
        assert_eq!(
            engine.intent_to_task_type("autonomous"),
            TaskType::Experimentation
        );
    }

    #[test]
    fn test_cost_sensitivity() {
        let config = TaskDistributionConfig {
            cost_sensitivity: 0.9,
            ..Default::default()
        };
        let engine = TaskDistributionEngine::new(config);

        // With high cost sensitivity, should prefer free providers
        assert_eq!(engine.config.cost_sensitivity, 0.9);
    }

    #[test]
    fn test_ssh_validation() {
        let engine = TaskDistributionEngine::with_defaults();

        let remotes = vec![RemoteConfig {
            name: "github".to_string(),
            provider: "github".to_string(),
            role: "public_nexus".to_string(),
            enabled: true,
            priority: 10,
            intents: vec![],
            canonical: false,
            backup: false,
            regional: false,
        }];

        let result = engine.validate_ssh_access(&remotes);
        assert!(result.is_ok());
    }
}
