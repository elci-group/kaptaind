use crate::config::loader::InferenceConfig;

use super::compat::{ChatTarget, TokenLimit};
use super::CommitContext;

const URL: &str = "https://api.openai.com/v1/chat/completions";

/// True when `OPENAI_API_KEY` is set.
pub fn is_available() -> bool {
    std::env::var("OPENAI_API_KEY").is_ok()
}

/// Calls OpenAI Chat Completions API to generate a commit message subject line.
/// Returns `None` if API key not set, on any error, or on empty response.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
) -> Option<String> {
    generate_with(config, ctx, model, TokenLimit::MaxTokens(100), None).await
}

/// [`generate`] with an explicit token limit field and reasoning effort, for
/// routed models (current OpenAI models take `max_completion_tokens`).
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate_with(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
    limit: TokenLimit,
    reasoning_effort: Option<&str>,
) -> Option<String> {
    let Ok(api_key) = std::env::var("OPENAI_API_KEY") else {
        tracing::warn!(
            component = module_path!(),
            "OPENAI_API_KEY not set; skipping OpenAI inference"
        );
        return None;
    };
    let target = ChatTarget {
        provider: "openai",
        url: URL,
        api_key: &api_key,
        model,
        limit,
        reasoning_effort,
    };
    super::compat::complete(config, &target, &super::ollama::build_user_prompt(ctx)).await
}
