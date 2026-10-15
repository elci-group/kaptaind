//! Google Gemini through its OpenAI-compatible endpoint.

use crate::config::loader::InferenceConfig;

use super::compat::{ChatTarget, TokenLimit};
use super::CommitContext;

const URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";

/// `GEMINI_API_KEY`, falling back to `GOOGLE_API_KEY`.
fn api_key() -> Option<String> {
    std::env::var("GEMINI_API_KEY")
        .or_else(|_| std::env::var("GOOGLE_API_KEY"))
        .ok()
        .filter(|key| !key.trim().is_empty())
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
            "GEMINI_API_KEY / GOOGLE_API_KEY not set; skipping Gemini inference"
        );
        return None;
    };
    let target = ChatTarget {
        provider: "gemini",
        url: URL,
        api_key: &api_key,
        model,
        limit: TokenLimit::MaxTokens(max_tokens),
        reasoning_effort,
    };
    super::compat::complete(config, &target, &super::ollama::build_user_prompt(ctx)).await
}
