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
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<serde_json::Value>,
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
    #[serde(rename = "type", default)]
    kind: String,
    /// Absent on `thinking` blocks, which current models may return first.
    #[serde(default)]
    text: String,
}

/// Map a routed `reasoning_effort` onto the Messages API: `none` disables
/// thinking; any other value becomes `output_config.effort`.
fn reasoning_fields(
    reasoning_effort: Option<&str>,
) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
    match reasoning_effort {
        None => (None, None),
        Some("none") => (Some(serde_json::json!({"type": "disabled"})), None),
        Some(effort) => (None, Some(serde_json::json!({ "effort": effort }))),
    }
}

/// True when `ANTHROPIC_API_KEY` is set.
pub fn is_available() -> bool {
    std::env::var("ANTHROPIC_API_KEY").is_ok()
}

/// Calls Anthropic Messages API to generate a commit message subject line.
/// Returns `None` if API key not set, on any error, or on empty response.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
) -> Option<String> {
    generate_with(config, ctx, model, 100, None).await
}

/// [`generate`] with an explicit output ceiling and reasoning effort, for
/// routed models.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate_with(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
    max_tokens: u32,
    reasoning_effort: Option<&str>,
) -> Option<String> {
    // traci: allow -- adjacent structured event records this failure path.
    if let Err(error) = crate::compliance::enforce_egress_url(
        EgressChannel::Inference,
        "https://api.anthropic.com/v1/messages",
    ) {
        tracing::warn!(component = module_path!(), %error, "regional policy blocked Anthropic inference");
        return None;
    }
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            tracing::warn!(
                component = module_path!(),
                "ANTHROPIC_API_KEY not set; skipping Anthropic inference"
            );
            return None;
        }
    };

    let client =
        crate::util::http::hardened_inference_client(Duration::from_secs(config.timeout_secs));

    let user_prompt = super::ollama::build_user_prompt(ctx);
    let (thinking, output_config) = reasoning_fields(reasoning_effort);

    let request = MessageRequest {
        model,
        max_tokens,
        system: super::compat::SYSTEM_PROMPT,
        messages: vec![MessageContent {
            role: "user",
            content: &user_prompt,
        }],
        thinking,
        output_config,
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

    let Some(block) = msg_response
        .content
        .iter()
        .find(|block| block.kind == "text" || block.kind.is_empty())
    else {
        tracing::warn!(
            component = module_path!(),
            "anthropic returned no text block"
        );
        return None;
    };

    let subject = super::compat::first_line_subject(&block.text);
    if subject.is_none() {
        tracing::warn!(
            component = module_path!(),
            "anthropic subject line was empty after truncation"
        );
    }
    subject
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_effort_disables_thinking_and_others_set_effort() {
        assert_eq!(reasoning_fields(None), (None, None));
        let (thinking, effort) = reasoning_fields(Some("none"));
        assert_eq!(thinking.unwrap()["type"], "disabled");
        assert!(effort.is_none());
        let (thinking, effort) = reasoning_fields(Some("low"));
        assert!(thinking.is_none());
        assert_eq!(effort.unwrap()["effort"], "low");
    }

    #[test]
    fn text_block_is_found_after_a_thinking_block() {
        let response: MessageResponse = serde_json::from_value(serde_json::json!({
            "content": [
                {"type": "thinking", "thinking": ""},
                {"type": "text", "text": "feat: route models"}
            ]
        }))
        .unwrap();
        let block = response.content.iter().find(|b| b.kind == "text").unwrap();
        assert_eq!(block.text, "feat: route models");
    }
}
