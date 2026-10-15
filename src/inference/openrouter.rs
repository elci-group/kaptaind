//! OpenRouter (OpenAI-compatible) and its model catalog, which backs the
//! `openrouter-free-tier` routing preset.

use crate::config::loader::{EgressChannel, InferenceConfig, ModelRoute};
use std::time::Duration;

use super::compat::{ChatTarget, TokenLimit};
use super::CommitContext;

const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// `OPENROUTER_API_KEY`, falling back to the key file `~/.keys/openrouter`.
fn api_key() -> Option<String> {
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if !key.trim().is_empty() {
            return Some(key.trim().to_string());
        }
    }
    let home = std::env::var_os("HOME")?;
    std::fs::read_to_string(std::path::Path::new(&home).join(".keys/openrouter"))
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

pub fn is_available() -> bool {
    api_key().is_some()
}

// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
    max_tokens: u32,
    reasoning_effort: Option<&str>,
) -> Option<String> {
    let Some(api_key) = api_key() else {
        tracing::warn!(
            component = module_path!(),
            "OPENROUTER_API_KEY not set (and no ~/.keys/openrouter); skipping OpenRouter inference"
        );
        return None;
    };
    let target = ChatTarget {
        provider: "openrouter",
        url: CHAT_URL,
        api_key: &api_key,
        model,
        limit: TokenLimit::MaxTokens(max_tokens),
        reasoning_effort,
    };
    super::compat::complete(config, &target, &super::ollama::build_user_prompt(ctx)).await
}

#[derive(serde::Deserialize)]
struct Catalog {
    data: Vec<CatalogModel>,
}

#[derive(serde::Deserialize)]
struct CatalogModel {
    id: String,
    #[serde(default)]
    pricing: Pricing,
    #[serde(default)]
    architecture: Architecture,
}

#[derive(serde::Deserialize, Default)]
struct Pricing {
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    completion: String,
}

#[derive(serde::Deserialize, Default)]
struct Architecture {
    #[serde(default)]
    output_modalities: Vec<String>,
}

/// Currently free chat models from the OpenRouter catalog, in catalog order,
/// at most `limit`. Empty on any error (logged): free availability changes,
/// so this is discovered per cluster rather than pinned.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn free_models(config: &InferenceConfig, limit: usize) -> Vec<ModelRoute> {
    // traci: allow -- adjacent structured event records this failure path.
    if let Err(error) = crate::compliance::enforce_egress_url(EgressChannel::Inference, MODELS_URL)
    {
        tracing::warn!(%error, "regional policy blocked the OpenRouter model catalog");
        return Vec::new();
    }
    let client =
        crate::util::http::hardened_inference_client(Duration::from_secs(config.timeout_secs));
    let catalog: Catalog = match client.get(MODELS_URL).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json().await {
            Ok(catalog) => catalog,
            Err(error) => {
                tracing::warn!(%error, "failed to parse the OpenRouter model catalog");
                return Vec::new();
            }
        },
        Ok(resp) => {
            tracing::warn!(status = %resp.status(), "OpenRouter model catalog returned non-200 status");
            return Vec::new();
        }
        Err(error) => {
            tracing::warn!(%error, "OpenRouter model catalog request failed");
            return Vec::new();
        }
    };
    select_free(catalog.data, limit)
}

fn select_free(models: Vec<CatalogModel>, limit: usize) -> Vec<ModelRoute> {
    models
        .into_iter()
        .filter(|m| m.pricing.prompt == "0" && m.pricing.completion == "0")
        .filter(|m| m.architecture.output_modalities.iter().any(|o| o == "text"))
        // Classifiers are free but cannot write commit subjects.
        .filter(|m| {
            let id = m.id.to_lowercase();
            !["safety", "guard", "moderation", "embed"]
                .iter()
                .any(|word| id.contains(word))
        })
        .take(limit)
        .map(|m| ModelRoute {
            provider: "openrouter".to_string(),
            model: m.id,
            input_micros_per_million: 0,
            output_micros_per_million: 0,
            latency_ms: 0,
            reasoning_effort: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_selection_skips_paid_classifiers_and_non_text() {
        let catalog: Catalog = serde_json::from_value(serde_json::json!({"data": [
            {"id": "a/chat:free", "pricing": {"prompt": "0", "completion": "0"},
             "architecture": {"output_modalities": ["text"]}},
            {"id": "b/paid", "pricing": {"prompt": "0.000001", "completion": "0.000002"},
             "architecture": {"output_modalities": ["text"]}},
            {"id": "c/content-safety:free", "pricing": {"prompt": "0", "completion": "0"},
             "architecture": {"output_modalities": ["text"]}},
            {"id": "d/image:free", "pricing": {"prompt": "0", "completion": "0"},
             "architecture": {"output_modalities": ["image"]}},
            {"id": "e/chat:free", "pricing": {"prompt": "0", "completion": "0"},
             "architecture": {"output_modalities": ["text"]}}
        ]}))
        .unwrap();
        let routes = select_free(catalog.data, 5);
        let ids: Vec<_> = routes.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(ids, ["a/chat:free", "e/chat:free"]);
        assert!(routes.iter().all(|r| r.provider == "openrouter"));
        assert_eq!(select_free(Vec::new(), 3).len(), 0);
    }
}
