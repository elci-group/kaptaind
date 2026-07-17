//! Cosine Lumen provider for controlled OpenAI-compatible deployments.
//!
//! Lumen Outpost is served by operators via vLLM/SGLang; no public Cosine
//! endpoint is assumed. Set `inference.cosine_base_url` to the approved
//! deployment URL and optionally `COSINE_API_KEY` for its bearer credential.

use crate::config::loader::{EgressChannel, InferenceConfig};
use std::time::Duration;

use super::CommitContext;

#[derive(serde::Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<Message<'a>>,
}

#[derive(serde::Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(serde::Deserialize)]
struct Response {
    choices: Vec<Choice>,
}
#[derive(serde::Deserialize)]
struct Choice {
    message: ResponseMessage,
}
#[derive(serde::Deserialize)]
struct ResponseMessage {
    content: String,
}

pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
) -> Option<String> {
    let base = config.cosine_base_url.as_deref()?;
    if let Err(error) = crate::compliance::enforce_egress_url(EgressChannel::Inference, base) {
        tracing::warn!(%error, "regional policy blocked Cosine Lumen inference");
        return None;
    }
    if let Err(error) = crate::util::http::validate_inference_url(base) {
        tracing::warn!(%error, "refusing unsafe Cosine Lumen endpoint");
        return None;
    }
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let user_prompt = super::ollama::build_user_prompt(ctx);
    let request = Request {
        model,
        max_tokens: 100,
        messages: vec![
            Message { role: "system", content: "Write one precise conventional-commit subject line, at most 72 characters. Output only the subject." },
            Message { role: "user", content: &user_prompt },
        ],
    };
    let client = crate::util::http::hardened_client(Duration::from_secs(config.timeout_secs));
    let mut call = client
        .post(url)
        .header("content-type", "application/json")
        .json(&request);
    if let Ok(key) = std::env::var("COSINE_API_KEY") {
        call = call.bearer_auth(key);
    }
    let response = match call.send().await {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            tracing::warn!(status = %response.status(), "Cosine Lumen returned non-success");
            return None;
        }
        Err(error) => {
            tracing::warn!(%error, "Cosine Lumen request failed");
            return None;
        }
    };
    let response: Response = match response.json().await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "Cosine Lumen response was invalid");
            return None;
        }
    };
    Some(
        response
            .choices
            .first()?
            .message
            .content
            .lines()
            .next()?
            .trim()
            .chars()
            .take(72)
            .collect(),
    )
}
