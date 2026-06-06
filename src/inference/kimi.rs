//! Kimi (Moonshot AI) inference provider for commit message generation.
//!
//! Supports three endpoints:
//! - Moonshot Global: https://api.moonshot.ai/v1
//! - Moonshot China: https://api.moonshot.cn/v1
//! - Kimi Coding: https://api.kimi.com/coding/v1
//!
//! Environment variables:
//! - MOONSHOT_API_KEY: For global endpoint
//! - MOONSHOT_CN_API_KEY: For China endpoint
//! - KIMI_CODE_API_KEY: For coding endpoint
//! - KIMI_API_KEY: Fallback for any endpoint

use crate::config::loader::InferenceConfig;
use std::time::Duration;

use super::CommitContext;

/// Kimi API endpoints
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KimiEndpoint {
    /// Global endpoint (api.moonshot.ai)
    MoonshotGlobal,
    /// China endpoint (api.moonshot.cn)
    MoonshotChina,
    /// Coding-optimized endpoint (api.kimi.com)
    KimiCoding,
}

impl KimiEndpoint {
    /// Get the base URL for this endpoint
    pub fn base_url(&self) -> &'static str {
        match self {
            KimiEndpoint::MoonshotGlobal => "https://api.moonshot.ai/v1",
            KimiEndpoint::MoonshotChina => "https://api.moonshot.cn/v1",
            KimiEndpoint::KimiCoding => "https://api.kimi.com/coding/v1",
        }
    }

    /// Get the environment variable name for API key
    pub fn api_key_env(&self) -> &'static str {
        match self {
            KimiEndpoint::MoonshotGlobal => "MOONSHOT_API_KEY",
            KimiEndpoint::MoonshotChina => "MOONSHOT_CN_API_KEY",
            KimiEndpoint::KimiCoding => "KIMI_CODE_API_KEY",
        }
    }

    /// Get the default model for this endpoint
    pub fn default_model(&self) -> &'static str {
        match self {
            KimiEndpoint::MoonshotGlobal | KimiEndpoint::MoonshotChina => "kimi-k2.5",
            KimiEndpoint::KimiCoding => "kimi-for-coding",
        }
    }
}

/// Serde types for Kimi API communication
#[derive(serde::Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

#[derive(serde::Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(serde::Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    thinking_type: String,
}

#[derive(serde::Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(serde::Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[serde(rename = "finish_reason")]
    _finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct ResponseMessage {
    content: String,
    #[serde(rename = "reasoning_content")]
    reasoning_content: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[serde(default)]
    reasoning_tokens: u32,
}

/// Resolve which Kimi endpoint to use based on config and environment
fn resolve_endpoint(config: &InferenceConfig) -> KimiEndpoint {
    // Check explicit config first
    if let Some(endpoint_str) = config.kimi_endpoint.as_deref() {
        return match endpoint_str {
            "global" | "moonshot" => KimiEndpoint::MoonshotGlobal,
            "china" | "cn" => KimiEndpoint::MoonshotChina,
            "coding" | "kimi" => KimiEndpoint::KimiCoding,
            _ => {
                tracing::warn!(
                    endpoint = endpoint_str,
                    "unknown kimi endpoint, using auto-detect"
                );
                auto_detect_endpoint()
            }
        };
    }

    auto_detect_endpoint()
}

/// Auto-detect endpoint based on available API keys
fn auto_detect_endpoint() -> KimiEndpoint {
    // Check for coding endpoint key first (development-focused)
    if std::env::var("KIMI_CODE_API_KEY").is_ok() {
        return KimiEndpoint::KimiCoding;
    }

    // Check for China endpoint
    if std::env::var("MOONSHOT_CN_API_KEY").is_ok() {
        return KimiEndpoint::MoonshotChina;
    }

    // Default to global endpoint
    KimiEndpoint::MoonshotGlobal
}

/// Resolve API key for the given endpoint
fn resolve_api_key(endpoint: KimiEndpoint) -> Option<String> {
    // Try endpoint-specific key first
    if let Ok(key) = std::env::var(endpoint.api_key_env()) {
        return Some(key);
    }

    // Fall back to generic KIMI_API_KEY
    if let Ok(key) = std::env::var("KIMI_API_KEY") {
        return Some(key);
    }

    // Try MOONSHOT_API_KEY as last resort
    if let Ok(key) = std::env::var("MOONSHOT_API_KEY") {
        return Some(key);
    }

    None
}

/// Resolve model name, falling back to endpoint default if needed
fn resolve_model<'a>(config: &'a InferenceConfig, endpoint: KimiEndpoint) -> &'a str {
    if !config.model.is_empty() && config.model != "auto" {
        return &config.model;
    }

    if !config.kimi_model.is_empty() {
        return &config.kimi_model;
    }

    endpoint.default_model()
}

/// Build system prompt optimized for Kimi models
fn build_system_prompt() -> &'static str {
    "You are a precise software commit message author. Write a single subject line (max 72 characters) describing what changed. \
     Use conventional commit format (feat:, fix:, refactor:, chore:, docs:, test:) when it fits. \
     Be specific about the change - mention the component or module affected. \
     Output ONLY the subject line — no body, no explanation, no quotes."
}

/// Build user prompt from commit context with Kimi-optimized formatting
fn build_user_prompt(ctx: &CommitContext<'_>, endpoint: KimiEndpoint) -> String {
    // Collect file paths (up to 25 for Kimi's larger context)
    let file_list = ctx
        .cluster_paths
        .iter()
        .take(25)
        .filter_map(|p| p.to_str())
        .map(|s| format!("  - {}", s))
        .collect::<Vec<_>>()
        .join("\n");

    let api_summary = if ctx.diff.api_breaking {
        "BREAKING CHANGE"
    } else if ctx.diff.api_added {
        "API addition"
    } else {
        "No API change"
    };

    // Include extended analysis for coding endpoint
    let code_context = if endpoint == KimiEndpoint::KimiCoding {
        format!(
            "\n\nCode Analysis:\n\
             - Structural complexity: {:.2}/1.0\n\
             - API surface impact: {:.2}/1.0\n\
             - Dependency changes: {} nodes\n\
             - Runtime config changes: {} files",
            ctx.diff.structural, ctx.diff.api, ctx.diff.dependency_nodes, ctx.diff.runtime_paths
        )
    } else {
        String::new()
    };

    format!(
        "Describe this code change in one subject line (max 72 chars).\n\n\
         Version bump: {:?} ({} → {})\n\
         API status: {}\n\
         Composite score: {:.3}/1.0\n\n\
         Changed files ({} total):\n\
         {}\n\
         {code_context}\n\n\
         Write a concise, specific commit message following conventional commit format.",
        ctx.bump,
        ctx.previous,
        ctx.next,
        api_summary,
        ctx.weight.score,
        ctx.cluster_paths.len(),
        file_list,
    )
}

/// Calls Kimi API to generate a commit message subject line.
/// Returns `None` if API key not set, on any error, or on empty response.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    _model: &str, // Model is resolved internally based on endpoint
) -> Option<String> {
    let endpoint = resolve_endpoint(config);
    let api_key = resolve_api_key(endpoint)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs.max(30)))
        .build()
        .ok()?;

    let model = resolve_model(config, endpoint);
    let system_prompt = build_system_prompt();
    let user_prompt = build_user_prompt(ctx, endpoint);

    // Enable thinking mode if configured and using a thinking-capable model
    let thinking = if config.kimi_thinking && model.contains("thinking") {
        Some(ThinkingConfig {
            thinking_type: "enabled".to_string(),
        })
    } else {
        None
    };

    let request = ChatRequest {
        model,
        messages: vec![
            Message {
                role: "system",
                content: system_prompt,
            },
            Message {
                role: "user",
                content: &user_prompt,
            },
        ],
        max_tokens: Some(150),
        temperature: Some(0.3), // Lower for consistent formatting
        thinking,
    };

    let base_url = config
        .kimi_base_url
        .as_deref()
        .unwrap_or_else(|| endpoint.base_url());

    tracing::info!(
        endpoint = %base_url,
        model = model,
        thinking = config.kimi_thinking,
        "calling kimi API"
    );

    let response = match client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(error = %e, "kimi request failed");
            return None;
        }
    };

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        tracing::warn!(status = %status, body = %body, "kimi returned error");
        return None;
    }

    let chat_response: ChatResponse = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse kimi response");
            return None;
        }
    };

    // Log token usage for telemetry
    if let Some(usage) = &chat_response.usage {
        tracing::debug!(
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            reasoning_tokens = usage.reasoning_tokens,
            "kimi token usage"
        );
    }

    if chat_response.choices.is_empty() {
        tracing::warn!("kimi returned no choices");
        return None;
    }

    let content = chat_response.choices[0].message.content.trim();
    if content.is_empty() {
        tracing::warn!("kimi message content was empty");
        return None;
    }

    // Log reasoning content if available (for debugging)
    if let Some(reasoning) = &chat_response.choices[0].message.reasoning_content {
        tracing::debug!(reasoning = %reasoning, "kimi reasoning");
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
        tracing::warn!("kimi subject line was empty after truncation");
        return None;
    }

    Some(subject)
}

/// Check if Kimi is available (has API key configured)
pub fn is_available() -> bool {
    auto_detect_endpoint();
    resolve_api_key(KimiEndpoint::MoonshotGlobal).is_some()
        || resolve_api_key(KimiEndpoint::MoonshotChina).is_some()
        || resolve_api_key(KimiEndpoint::KimiCoding).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::engine::Cluster;
    use crate::diff::DiffAnalysis;
    use crate::version::Bump;
    use crate::weight::WeightResult;
    use semver::Version;
    use std::path::PathBuf;

    #[test]
    fn test_endpoint_base_urls() {
        assert_eq!(
            KimiEndpoint::MoonshotGlobal.base_url(),
            "https://api.moonshot.ai/v1"
        );
        assert_eq!(
            KimiEndpoint::MoonshotChina.base_url(),
            "https://api.moonshot.cn/v1"
        );
        assert_eq!(
            KimiEndpoint::KimiCoding.base_url(),
            "https://api.kimi.com/coding/v1"
        );
    }

    #[test]
    fn test_endpoint_default_models() {
        assert_eq!(KimiEndpoint::MoonshotGlobal.default_model(), "kimi-k2.5");
        assert_eq!(KimiEndpoint::MoonshotChina.default_model(), "kimi-k2.5");
        assert_eq!(KimiEndpoint::KimiCoding.default_model(), "kimi-for-coding");
    }

    #[test]
    fn test_build_user_prompt_includes_bump_type() {
        let cluster = Cluster {
            id: uuid::Uuid::new_v4(),
            events: vec![],
            started_at: chrono::Utc::now(),
            ended_at: chrono::Utc::now(),
        };
        let mut diff = DiffAnalysis::default();
        diff.api_added = true;
        let weight = WeightResult {
            score: 0.5,
            api_breaking: false,
            api_added: true,
        };
        let previous = Version::parse("0.1.0").unwrap();
        let next = Version::parse("0.2.0").unwrap();
        let paths: Vec<PathBuf> = vec![];

        let ctx = CommitContext {
            cluster: &cluster,
            diff: &diff,
            weight: &weight,
            bump: Bump::Minor,
            previous: &previous,
            next: &next,
            cluster_paths: &paths,
        };

        let prompt = build_user_prompt(&ctx, KimiEndpoint::MoonshotGlobal);
        assert!(prompt.contains("Minor"));
        assert!(prompt.contains("0.1.0"));
        assert!(prompt.contains("0.2.0"));
        assert!(prompt.contains("API addition"));
    }

    #[test]
    fn test_build_user_prompt_includes_file_paths() {
        let cluster = Cluster {
            id: uuid::Uuid::new_v4(),
            events: vec![],
            started_at: chrono::Utc::now(),
            ended_at: chrono::Utc::now(),
        };
        let diff = DiffAnalysis::default();
        let weight = WeightResult {
            score: 0.5,
            api_breaking: false,
            api_added: false,
        };
        let previous = Version::parse("0.1.0").unwrap();
        let next = Version::parse("0.1.1").unwrap();
        let paths = vec![PathBuf::from("src/main.rs"), PathBuf::from("src/lib.rs")];

        let ctx = CommitContext {
            cluster: &cluster,
            diff: &diff,
            weight: &weight,
            bump: Bump::Patch,
            previous: &previous,
            next: &next,
            cluster_paths: &paths,
        };

        let prompt = build_user_prompt(&ctx, KimiEndpoint::MoonshotGlobal);
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("src/lib.rs"));
    }

    #[test]
    fn test_build_user_prompt_coding_endpoint_includes_code_analysis() {
        let cluster = Cluster {
            id: uuid::Uuid::new_v4(),
            events: vec![],
            started_at: chrono::Utc::now(),
            ended_at: chrono::Utc::now(),
        };
        let diff = DiffAnalysis {
            structural: 0.7,
            api: 0.5,
            dependency_nodes: 10,
            runtime_paths: 3,
            ..DiffAnalysis::default()
        };
        let weight = WeightResult {
            score: 0.6,
            api_breaking: false,
            api_added: false,
        };
        let previous = Version::parse("0.1.0").unwrap();
        let next = Version::parse("0.1.1").unwrap();
        let paths: Vec<PathBuf> = vec![];

        let ctx = CommitContext {
            cluster: &cluster,
            diff: &diff,
            weight: &weight,
            bump: Bump::Patch,
            previous: &previous,
            next: &next,
            cluster_paths: &paths,
        };

        let prompt = build_user_prompt(&ctx, KimiEndpoint::KimiCoding);
        assert!(prompt.contains("Code Analysis"));
        assert!(prompt.contains("Structural complexity"));
        assert!(prompt.contains("0.70"));
    }

    #[test]
    fn test_subject_line_truncation_at_72_chars() {
        let long_line = "a".repeat(100);
        let truncated = long_line.chars().take(72).collect::<String>();
        assert_eq!(truncated.len(), 72);
    }
}
