//! OpenAI-compatible Chat Completions client shared by the OpenAI, Gemini and
//! OpenRouter providers.

use crate::config::loader::{EgressChannel, InferenceConfig};
use std::time::Duration;

/// System prompt for every subject-line generator on this transport.
pub const SYSTEM_PROMPT: &str = "You are a precise software commit message author. Write a single subject line (max 72 characters) describing what changed. Use conventional commit format (feat:, fix:, refactor:, chore:) when it fits. Output ONLY the subject line — no body, no explanation.";

/// How the endpoint names the output-token ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenLimit {
    /// `max_tokens` (OpenRouter, Gemini, older OpenAI chat models).
    MaxTokens(u32),
    /// `max_completion_tokens` (current OpenAI models, including reasoning
    /// models that reject `max_tokens`).
    MaxCompletionTokens(u32),
}

/// One request target on an OpenAI-compatible endpoint.
pub struct ChatTarget<'a> {
    /// Provider name for logs (`openai`, `gemini`, `openrouter`).
    pub provider: &'a str,
    /// Full `.../chat/completions` URL.
    pub url: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub limit: TokenLimit,
    /// Sent as `reasoning_effort` when set.
    pub reasoning_effort: Option<&'a str>,
}

#[derive(serde::Serialize)]
struct CompletionRequest<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
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
    #[serde(default)]
    content: Option<String>,
}

/// Request one subject line. Returns `None` on egress refusal, transport or
/// HTTP error, or an empty answer; every failure is logged.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn complete(
    config: &InferenceConfig,
    target: &ChatTarget<'_>,
    user_prompt: &str,
) -> Option<String> {
    // traci: allow -- adjacent structured event records this failure path.
    if let Err(error) = crate::compliance::enforce_egress_url(EgressChannel::Inference, target.url)
    {
        tracing::warn!(provider = target.provider, %error, "regional policy blocked inference");
        return None;
    }
    let (max_tokens, max_completion_tokens) = match target.limit {
        TokenLimit::MaxTokens(n) => (Some(n), None),
        TokenLimit::MaxCompletionTokens(n) => (None, Some(n)),
    };
    let request = CompletionRequest {
        model: target.model,
        max_tokens,
        max_completion_tokens,
        reasoning_effort: target.reasoning_effort,
        messages: vec![
            ChatMessage {
                role: "system",
                content: SYSTEM_PROMPT,
            },
            ChatMessage {
                role: "user",
                content: user_prompt,
            },
        ],
    };

    let client =
        crate::util::http::hardened_inference_client(Duration::from_secs(config.timeout_secs));
    let response = match client
        .post(target.url)
        .header("Authorization", format!("Bearer {}", target.api_key))
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(provider = target.provider, error = %e, "inference request failed");
            return None;
        }
    };
    if !response.status().is_success() {
        tracing::warn!(
            provider = target.provider,
            model = target.model,
            status = %response.status(),
            "inference endpoint returned non-200 status"
        );
        return None;
    }
    let parsed: CompletionResponse = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(provider = target.provider, error = %e, "failed to parse inference response");
            return None;
        }
    };
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .unwrap_or_default();
    let subject = first_line_subject(&content);
    if subject.is_none() {
        tracing::warn!(
            provider = target.provider,
            model = target.model,
            "inference returned no subject line"
        );
    }
    subject
}

/// First non-empty line, trimmed and truncated to 72 characters.
pub fn first_line_subject(content: &str) -> Option<String> {
    let subject: String = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .chars()
        .take(72)
        .collect();
    (!subject.is_empty()).then_some(subject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_skips_blank_lines_and_truncates() {
        assert_eq!(
            first_line_subject("\n\n  feat: add routing  \nbody"),
            Some("feat: add routing".to_string())
        );
        assert_eq!(first_line_subject(&"x".repeat(100)).unwrap().len(), 72);
        assert_eq!(first_line_subject("  \n "), None);
    }

    #[test]
    fn token_limit_selects_one_field() {
        let request = CompletionRequest {
            model: "m",
            max_tokens: None,
            max_completion_tokens: Some(64),
            reasoning_effort: Some("none"),
            messages: Vec::new(),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("max_tokens").is_none());
        assert_eq!(json["max_completion_tokens"], 64);
        assert_eq!(json["reasoning_effort"], "none");
    }
}
