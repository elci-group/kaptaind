pub mod anthropic;
pub mod consensus;
pub mod kimi;
pub mod ollama;
pub mod openai;

use crate::cluster::engine::Cluster;
use crate::config::loader::{InferenceConfig, ValidationMode};
use crate::diff::DiffAnalysis;
use crate::version::Bump;
use crate::weight::WeightResult;
use semver::Version;
use std::path::PathBuf;

/// Contextual data available at commit time for inference.
pub struct CommitContext<'a> {
    pub cluster: &'a Cluster,
    pub diff: &'a DiffAnalysis,
    pub weight: &'a WeightResult,
    pub bump: Bump,
    pub previous: &'a Version,
    pub next: &'a Version,
    pub cluster_paths: &'a [PathBuf],
}

/// Selects the active provider based on config + env vars.
fn resolve_provider(config: &InferenceConfig) -> &str {
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
fn resolve_model<'a>(config: &'a InferenceConfig, provider: &str) -> &'a str {
    if config.model != "auto" {
        return &config.model;
    }
    match provider {
        "anthropic" => "claude-haiku-4-5-20251001",
        "openai" => "gpt-4o-mini",
        _ => "llama3.2",
    }
}

/// Top-level dispatcher — routes to the selected validation mode and provider.
pub async fn generate_commit_message(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
) -> Option<String> {
    if !config.enabled {
        return None;
    }

    tracing::info!(mode = ?config.validation_mode, "inference mode selected");

    match config.validation_mode {
        ValidationMode::Fast => {
            let provider = resolve_provider(config);
            let model = resolve_model(config, provider);
            tracing::info!(provider, model, "fast mode: inference provider selected");

            match provider {
                "anthropic" => anthropic::generate(config, ctx, model).await,
                "openai" => openai::generate(config, ctx, model).await,
                "kimi" => kimi::generate(config, ctx, model).await,
                _ => ollama::generate(config, ctx, model).await,
            }
        }
        ValidationMode::Consensus => consensus::generate(config, ctx).await,
    }
}
