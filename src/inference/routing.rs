//! Model routing: explicit provider/model pins, named presets, and
//! first-match rules (`[inference.routing]`).

use crate::config::loader::{InferenceConfig, ModelRoute, RuleCondition};
use crate::version::Bump;

use super::CommitContext;

/// Every preset name accepted in `preset = "..."`.
pub const PRESETS: &[&str] = &[
    OPENROUTER_FREE_TIER,
    ALL_PROVIDER_FLASH,
    OPENAI_SMALLEST_FRONTIER,
];

/// Currently free OpenRouter chat models, discovered from its catalog.
pub const OPENROUTER_FREE_TIER: &str = "openrouter-free-tier";
/// The fast, low-cost tier of each hosted provider with credentials.
pub const ALL_PROVIDER_FLASH: &str = "all-provider-flash";
/// The smallest model of OpenAI's current frontier family.
pub const OPENAI_SMALLEST_FRONTIER: &str = "openai-smallest-frontier";

fn route(provider: &str, model: &str, input: u64, output: u64, effort: &str) -> ModelRoute {
    ModelRoute {
        provider: provider.to_string(),
        model: model.to_string(),
        input_micros_per_million: input,
        output_micros_per_million: output,
        latency_ms: 0,
        reasoning_effort: Some(effort.to_string()),
    }
}

/// Fixed preset members (list prices, USD micro-units per million tokens,
/// as of 2026-10). Model IDs are pinned here; override with `models` pins.
fn static_preset(name: &str) -> Option<Vec<ModelRoute>> {
    // Thinking off for one-line subjects; Gemini 3 models cannot disable it.
    let luna = || route("openai", "gpt-6-luna", 100_000, 500_000, "none");
    match name {
        ALL_PROVIDER_FLASH => Some(vec![
            route("anthropic", "claude-haiku-5-5", 100_000, 500_000, "none"),
            luna(),
            route("gemini", "gemini-3.8-flash", 750_000, 3_750_000, "low"),
        ]),
        OPENAI_SMALLEST_FRONTIER => Some(vec![luna()]),
        _ => None,
    }
}

/// Expand a preset into routes. `limit` caps catalog-discovered presets.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn expand_preset(config: &InferenceConfig, name: &str, limit: usize) -> Vec<ModelRoute> {
    if let Some(routes) = static_preset(name) {
        return routes;
    }
    if name == OPENROUTER_FREE_TIER {
        if !super::openrouter::is_available() {
            tracing::warn!(
                preset = name,
                "OPENROUTER_API_KEY not set; preset contributes no routes"
            );
            return Vec::new();
        }
        return super::openrouter::free_models(config, limit).await;
    }
    tracing::warn!(
        preset = name,
        known = ?PRESETS,
        "unknown routing preset; it contributes no routes"
    );
    Vec::new()
}

/// Whether `provider` has the credentials or endpoint it needs.
pub fn provider_available(config: &InferenceConfig, provider: &str) -> bool {
    match provider {
        "anthropic" => super::anthropic::is_available(),
        "openai" => super::openai::is_available(),
        "gemini" => super::gemini::is_available(),
        "openrouter" => super::openrouter::is_available(),
        "kimi" => super::kimi::is_available(),
        "cosine" => config.cosine_base_url.is_some(),
        "ollama" => true,
        _ => false,
    }
}

fn bump_name(bump: Bump) -> &'static str {
    match bump {
        Bump::None => "none",
        Bump::Patch => "patch",
        Bump::Minor => "minor",
        Bump::Major => "major",
    }
}

/// True when every set field of `when` matches the cluster.
pub fn rule_matches(when: &RuleCondition, ctx: &CommitContext<'_>) -> bool {
    if !when.bump.is_empty()
        && !when
            .bump
            .iter()
            .any(|b| b.eq_ignore_ascii_case(bump_name(ctx.bump)))
    {
        return false;
    }
    let score = f64::from(ctx.weight.score);
    if when.min_score.is_some_and(|min| score < min)
        || when.max_score.is_some_and(|max| score > max)
    {
        return false;
    }
    if when
        .min_paths
        .is_some_and(|min| ctx.cluster_paths.len() < min)
    {
        return false;
    }
    if !when.paths.is_empty() {
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in &when.paths {
            match globset::Glob::new(pattern) {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(error) => {
                    tracing::warn!(pattern, %error, "invalid routing rule glob; rule does not match");
                    return false;
                }
            }
        }
        let Ok(set) = builder.build() else {
            return false;
        };
        let any = ctx.cluster_paths.iter().any(|path| {
            let rel = path.strip_prefix(ctx.repo_path).unwrap_or(path);
            set.is_match(rel)
        });
        if !any {
            return false;
        }
    }
    true
}

/// The ordered, de-duplicated, credential-filtered routes for this cluster.
///
/// Without `[inference.routing]` this is the single legacy route from
/// `provider` / `model`. UK compliance mode always routes to Cosine only.
/// An empty result means nothing routable is available (fail closed: the
/// deterministic message is used).
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn resolve(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    limit: usize,
) -> Vec<ModelRoute> {
    if config.uk_compliance_mode || !config.routing.is_configured() {
        let provider = super::resolve_provider(config);
        let model = super::resolve_model(config, provider);
        return vec![ModelRoute {
            provider: provider.to_string(),
            model: model.to_string(),
            input_micros_per_million: 0,
            output_micros_per_million: 0,
            latency_ms: 0,
            reasoning_effort: None,
        }];
    }
    let routing = &config.routing;
    let (rule_name, models, preset) = match routing
        .rules
        .iter()
        .enumerate()
        .find(|(_, rule)| rule_matches(&rule.when, ctx))
    {
        Some((index, rule)) => (
            Some(
                rule.name
                    .clone()
                    .unwrap_or_else(|| format!("rule {}", index + 1)),
            ),
            &rule.models,
            rule.preset.as_deref(),
        ),
        None => (None, &routing.models, routing.preset.as_deref()),
    };
    let mut routes = models.clone();
    if let Some(name) = preset {
        routes.extend(expand_preset(config, name, limit).await);
    }
    let mut seen = std::collections::BTreeSet::new();
    routes.retain(|r| seen.insert((r.provider.clone(), r.model.clone())));
    routes.retain(|r| {
        let available = provider_available(config, &r.provider);
        if !available {
            tracing::warn!(
                provider = %r.provider,
                model = %r.model,
                "route skipped: provider credentials or endpoint not configured"
            );
        }
        available
    });
    tracing::info!(
        rule = rule_name.as_deref().unwrap_or("default"),
        routes = ?routes.iter().map(|r| format!("{}/{}", r.provider, r.model)).collect::<Vec<_>>(),
        "inference routing resolved"
    );
    routes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::engine::Cluster;
    use crate::diff::DiffAnalysis;
    use crate::weight::WeightResult;
    use semver::Version;
    use std::path::{Path, PathBuf};

    struct Fixture {
        cluster: Cluster,
        diff: DiffAnalysis,
        weight: WeightResult,
        bump: Bump,
        previous: Version,
        next: Version,
        paths: Vec<PathBuf>,
    }

    impl Fixture {
        fn new(bump: Bump, score: f32, paths: &[&str]) -> Self {
            Self {
                cluster: Cluster {
                    id: uuid::Uuid::new_v4(),
                    started_at: chrono::Utc::now(),
                    ended_at: chrono::Utc::now(),
                    events: Vec::new(),
                },
                diff: DiffAnalysis::default(),
                weight: WeightResult {
                    score,
                    api_breaking: false,
                    api_added: false,
                },
                bump,
                previous: Version::new(1, 0, 0),
                next: Version::new(1, 1, 0),
                paths: paths.iter().map(|p| Path::new("/repo").join(p)).collect(),
            }
        }

        fn ctx(&self) -> CommitContext<'_> {
            CommitContext {
                cluster: &self.cluster,
                diff: &self.diff,
                weight: &self.weight,
                bump: self.bump,
                previous: &self.previous,
                next: &self.next,
                cluster_paths: &self.paths,
                repo_path: Path::new("/repo"),
            }
        }
    }

    fn matches(when: &RuleCondition, bump: Bump, score: f32, paths: &[&str]) -> bool {
        rule_matches(when, &Fixture::new(bump, score, paths).ctx())
    }

    #[test]
    fn presets_resolve_to_pinned_models() {
        let flash = static_preset(ALL_PROVIDER_FLASH).unwrap();
        let keys: Vec<_> = flash
            .iter()
            .map(|r| format!("{}/{}", r.provider, r.model))
            .collect();
        assert_eq!(
            keys,
            [
                "anthropic/claude-haiku-5-5",
                "openai/gpt-6-luna",
                "gemini/gemini-3.8-flash"
            ]
        );
        let smallest = static_preset(OPENAI_SMALLEST_FRONTIER).unwrap();
        assert_eq!(smallest.len(), 1);
        assert_eq!(smallest[0].model, "gpt-6-luna");
        assert_eq!(smallest[0].reasoning_effort.as_deref(), Some("none"));
        assert!(
            static_preset(OPENROUTER_FREE_TIER).is_none(),
            "discovered at runtime"
        );
    }

    #[test]
    fn rule_conditions_all_must_match() {
        let when = RuleCondition {
            bump: vec!["major".into()],
            min_score: Some(0.5),
            paths: vec!["src/api/**".into()],
            ..RuleCondition::default()
        };
        assert!(matches(&when, Bump::Major, 0.8, &["src/api/v1.rs"]));
        assert!(!matches(&when, Bump::Minor, 0.8, &["src/api/v1.rs"]));
        assert!(!matches(&when, Bump::Major, 0.2, &["src/api/v1.rs"]));
        assert!(!matches(&when, Bump::Major, 0.8, &["docs/a.md"]));
        let empty = RuleCondition::default();
        assert!(matches(&empty, Bump::Patch, 0.0, &[]));
        let many = RuleCondition {
            min_paths: Some(2),
            max_score: Some(0.3),
            ..RuleCondition::default()
        };
        assert!(matches(&many, Bump::Patch, 0.1, &["a", "b"]));
        assert!(!matches(&many, Bump::Patch, 0.1, &["a"]));
    }

    #[tokio::test]
    async fn first_matching_rule_wins_and_unavailable_routes_are_dropped() {
        let mut config = InferenceConfig::default();
        config.routing.models = vec![route("ollama", "default-local", 0, 0, "none")];
        config.routing.rules = vec![crate::config::loader::RoutingRule {
            name: Some("breaking".into()),
            when: RuleCondition {
                bump: vec!["major".into()],
                ..RuleCondition::default()
            },
            preset: None,
            models: vec![
                route("ollama", "big-local", 0, 0, "none"),
                route("ollama", "big-local", 0, 0, "none"),
                route("no-such-provider", "x", 0, 0, "none"),
            ],
        }];
        let major = Fixture::new(Bump::Major, 0.9, &[]);
        let routes = resolve(&config, &major.ctx(), 3).await;
        let models: Vec<_> = routes.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(
            models,
            ["big-local"],
            "deduplicated, unknown provider dropped"
        );
        let patch = Fixture::new(Bump::Patch, 0.1, &[]);
        let routes = resolve(&config, &patch.ctx(), 3).await;
        assert_eq!(
            routes[0].model, "default-local",
            "no rule matched: default models"
        );
    }

    #[tokio::test]
    async fn unconfigured_routing_keeps_the_legacy_single_route() {
        let config = InferenceConfig {
            provider: "ollama".into(),
            model: "llama3.2".into(),
            ..InferenceConfig::default()
        };
        let fixture = Fixture::new(Bump::Patch, 0.1, &[]);
        let routes = resolve(&config, &fixture.ctx(), 3).await;
        assert_eq!(routes.len(), 1);
        assert_eq!(
            (routes[0].provider.as_str(), routes[0].model.as_str()),
            ("ollama", "llama3.2")
        );
    }

    #[test]
    fn documented_toml_shape_parses() {
        let toml = r#"
            enabled = true
            validation_mode = "colony"

            [routing]
            preset = "all-provider-flash"

            [[routing.models]]
            provider = "openrouter"
            model = "nvidia/nemotron-3.5-lightning:free"

            [[routing.rules]]
            name = "breaking changes"
            when = { bump = ["major"], paths = ["src/api/**"] }
            preset = "openai-smallest-frontier"

            [[routing.rules]]
            name = "docs only"
            when = { paths = ["docs/**", "*.md"], max_score = 0.3 }
            preset = "openrouter-free-tier"

            [colony]
            max_candidates = 3
            budget_micros = 20000
            classification = "internal"
        "#;
        let config: InferenceConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            config.validation_mode,
            crate::config::loader::ValidationMode::Colony
        );
        assert_eq!(config.routing.preset.as_deref(), Some(ALL_PROVIDER_FLASH));
        assert_eq!(config.routing.models[0].provider, "openrouter");
        assert_eq!(config.routing.rules.len(), 2);
        assert_eq!(config.routing.rules[0].when.bump, ["major"]);
        assert_eq!(config.routing.rules[1].when.max_score, Some(0.3));
        assert_eq!(config.colony.max_candidates, 3);
        for rule in &config.routing.rules {
            assert!(PRESETS.contains(&rule.preset.as_deref().unwrap()));
        }
        assert!(
            toml::from_str::<InferenceConfig>("[routing]\nprest = \"x\"").is_err(),
            "typos are rejected"
        );
    }
}
