use crate::config::loader::EgressChannel;
use crate::config::loader::InferenceConfig;
use std::time::Duration;

use super::CommitContext;

/// Serde types for OpenAI API communication
#[derive(serde::Serialize)]
struct CompletionRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<ChatMessage<'a>>,
}

#[derive(serde::Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(serde::Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(serde::Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(serde::Deserialize)]
struct ChoiceMessage {
    content: String,
}

/// Calls OpenAI Chat Completions API to generate a commit message subject line.
/// Returns `None` if API key not set, on any error, or on empty response.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
) -> Option<String> {
    if let Err(error) = crate::compliance::enforce_egress_url(
        EgressChannel::Inference,
        "https://api.openai.com/v1/chat/completions",
    ) {
        tracing::warn!(%error, "regional policy blocked OpenAI inference");
        return None;
    }
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            tracing::warn!("OPENAI_API_KEY not set; skipping OpenAI inference");
            return None;
        }
    };

    let client = crate::util::http::hardened_client(Duration::from_secs(config.timeout_secs));

    let system_prompt = "You are a precise software commit message author. Write a single subject line (max 72 characters) describing what changed. Use conventional commit format (feat:, fix:, refactor:, chore:) when it fits. Output ONLY the subject line — no body, no explanation.";

    let user_prompt = super::ollama::build_user_prompt(ctx);

    let request = CompletionRequest {
        model,
        max_tokens: 100,
        messages: vec![
            ChatMessage {
                role: "system",
                content: system_prompt,
            },
            ChatMessage {
                role: "user",
                content: &user_prompt,
            },
        ],
    };

    let response = match client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(error = %e, "openai request failed");
            return None;
        }
    };

    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "openai returned non-200 status");
        return None;
    }

    let completion_response: CompletionResponse = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse openai response");
            return None;
        }
    };

    if completion_response.choices.is_empty() {
        tracing::warn!("openai returned no choices");
        return None;
    }

    let content = completion_response.choices[0].message.content.trim();
    if content.is_empty() {
        tracing::warn!("openai message content was empty");
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
        tracing::warn!("openai subject line was empty after truncation");
        return None;
    }

    Some(subject)
}
