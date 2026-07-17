use crate::config::loader::EgressChannel;
use crate::config::loader::InferenceConfig;
use std::time::Duration;

use super::CommitContext;

/// Serde types for Anthropic API communication
#[derive(serde::Serialize)]
struct MessageRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<MessageContent<'a>>,
}

#[derive(serde::Serialize)]
struct MessageContent<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(serde::Deserialize)]
struct MessageResponse {
    content: Vec<ContentBlock>,
}

#[derive(serde::Deserialize)]
struct ContentBlock {
    text: String,
}

/// Calls Anthropic Messages API to generate a commit message subject line.
/// Returns `None` if API key not set, on any error, or on empty response.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
) -> Option<String> {
    if let Err(error) = crate::compliance::enforce_egress_url(
        EgressChannel::Inference,
        "https://api.anthropic.com/v1/messages",
    ) {
        tracing::warn!(%error, "regional policy blocked Anthropic inference");
        return None;
    }
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            tracing::warn!("ANTHROPIC_API_KEY not set; skipping Anthropic inference");
            return None;
        }
    };

    let client = crate::util::http::hardened_client(Duration::from_secs(config.timeout_secs));

    let system_prompt = "You are a precise software commit message author. Write a single subject line (max 72 characters) describing what changed. Use conventional commit format (feat:, fix:, refactor:, chore:) when it fits. Output ONLY the subject line — no body, no explanation.";

    let user_prompt = super::ollama::build_user_prompt(ctx);

    let request = MessageRequest {
        model,
        max_tokens: 100,
        system: system_prompt,
        messages: vec![MessageContent {
            role: "user",
            content: &user_prompt,
        }],
    };

    let response = match client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(error = %e, "anthropic request failed");
            return None;
        }
    };

    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "anthropic returned non-200 status");
        return None;
    }

    let msg_response: MessageResponse = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse anthropic response");
            return None;
        }
    };

    if msg_response.content.is_empty() {
        tracing::warn!("anthropic returned empty content");
        return None;
    }

    let content = msg_response.content[0].text.trim();
    if content.is_empty() {
        tracing::warn!("anthropic text was empty");
        return None;
    }

    // Take first line and truncate to 72 chars
    let subject = content
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(72)
        .collect::<String>();

    if subject.is_empty() {
        tracing::warn!("anthropic subject line was empty after truncation");
        return None;
    }

    Some(subject)
}
