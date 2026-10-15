pub mod anthropic;
pub mod colony;
pub mod compat;
pub mod consensus;
pub mod cosine;
pub mod gemini;
pub mod kimi;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod routing;

use crate::cluster::engine::Cluster;
use crate::config::loader::{InferenceConfig, ModelRoute, ValidationMode};
use crate::diff::DiffAnalysis;
use crate::version::Bump;
use crate::weight::WeightResult;
use semver::Version;
use std::path::{Path, PathBuf};

/// Output-token ceiling for a routed fast-mode call (room for models that
/// reason before answering).
const ROUTED_MAX_TOKENS: u32 = 512;

/// Contextual data available at commit time for inference.
pub struct CommitContext<'a> {
    pub cluster: &'a Cluster,
    pub diff: &'a DiffAnalysis,
    pub weight: &'a WeightResult,
    pub bump: Bump,
    pub previous: &'a Version,
    pub next: &'a Version,
    pub cluster_paths: &'a [PathBuf],
    /// Repository root, for repo-relative routing rule globs and reports.
    pub repo_path: &'a Path,
}

/// Selects the active provider based on config + env vars.
pub fn resolve_provider(config: &InferenceConfig) -> &str {
    if config.uk_compliance_mode {
        return "cosine";
    }
    if config.provider != "auto" {
        return &config.provider;
    }
    // Check for Kimi API keys first (prioritize newer providers)
    if kimi::is_available() {
        return "kimi";
    }
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        return "anthropic";
    }
    if std::env::var("OPENAI_API_KEY").is_ok() {
        return "openai";
    }
    "ollama"
}

/// Resolves "auto" model to the provider default.
pub fn resolve_model<'a>(config: &'a InferenceConfig, provider: &str) -> &'a str {
    if config.model != "auto" {
        return &config.model;
    }
    match provider {
        "cosine" => config.cosine_model.as_str(),
        "anthropic" => "claude-haiku-4-5-20251001",
        "openai" => "gpt-4o-mini",
        _ => "llama3.2",
    }
}

/// Top-level dispatcher — routes to the selected validation mode and provider.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate_commit_message(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
) -> Option<String> {
    if !config.enabled {
        return None;
    }

    tracing::info!(mode = ?config.validation_mode, "inference mode selected");

    match config.validation_mode {
        ValidationMode::Fast if config.routing.is_configured() && !config.uk_compliance_mode => {
            generate_routed_fast(config, ctx).await
        }
        ValidationMode::Fast => {
            let provider = resolve_provider(config);
            let model = resolve_model(config, provider);
            tracing::info!(
                component = module_path!(),
                provider,
                model,
                "fast mode: inference provider selected"
            );

            match provider {
                "anthropic" => anthropic::generate(config, ctx, model).await,
                "openai" => openai::generate(config, ctx, model).await,
                "kimi" => kimi::generate(config, ctx, model).await,
                "cosine" => cosine::generate(config, ctx, model).await,
                _ => ollama::generate(config, ctx, model).await,
            }
        }
        ValidationMode::Consensus => consensus::generate(config, ctx).await,
        ValidationMode::Colony => {
            let limit = config.colony.max_candidates.max(1);
            let routes = routing::resolve(config, ctx, limit).await;
            colony::generate(config, ctx, &routes).await
        }
    }
}

/// Fast mode under `[inference.routing]`: try each resolved route in order
/// until one returns a subject.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
async fn generate_routed_fast(config: &InferenceConfig, ctx: &CommitContext<'_>) -> Option<String> {
    for route in routing::resolve(config, ctx, 1).await {
        tracing::info!(
            component = module_path!(),
            provider = %route.provider,
            model = %route.model,
            "fast mode: routed inference"
        );
        if let Some(subject) = generate_with_route(config, ctx, &route, ROUTED_MAX_TOKENS).await {
            return Some(subject);
        }
    }
    None
}

/// Generate a subject on one routed provider/model.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate_with_route(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    route: &ModelRoute,
    max_tokens: u32,
) -> Option<String> {
    let effort = route.reasoning_effort.as_deref();
    let model = route.model.as_str();
    match route.provider.as_str() {
        "anthropic" => anthropic::generate_with(config, ctx, model, max_tokens, effort).await,
        "openai" => {
            let limit = compat::TokenLimit::MaxCompletionTokens(max_tokens);
            openai::generate_with(config, ctx, model, limit, effort).await
        }
        "gemini" => gemini::generate(config, ctx, model, max_tokens, effort).await,
        "openrouter" => openrouter::generate(config, ctx, model, max_tokens, effort).await,
        // Kimi resolves its model from `[inference] kimi_*` settings.
        "kimi" => kimi::generate(config, ctx, model).await,
        "cosine" => cosine::generate(config, ctx, model).await,
        "ollama" => ollama::generate(config, ctx, model).await,
        other => {
            tracing::warn!(provider = other, "unknown inference provider in route");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uk_compliance_mode_always_routes_to_cosine() {
        let config = InferenceConfig {
            provider: "openai".to_string(),
            uk_compliance_mode: true,
            ..InferenceConfig::default()
        };
        assert_eq!(resolve_provider(&config), "cosine");
        assert_eq!(resolve_model(&config, "cosine"), "lumen-outpost");
    }
}
