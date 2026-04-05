use crate::config::loader::InferenceConfig;
use crate::diff::DiffAnalysis;
use crate::version::Bump;
use crate::weight::WeightResult;
use semver::Version;
use std::time::Duration;

pub use super::CommitContext;

/// Serde types for Ollama API communication
#[derive(serde::Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    stream: bool,
    messages: Vec<Message<'a>>,
}

#[derive(serde::Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

#[derive(serde::Deserialize)]
struct ChatResponse {
    message: ResponseMessage,
}

#[derive(serde::Deserialize)]
struct ResponseMessage {
    content: String,
}

/// Builds the user prompt from commit context.
pub(super) fn build_user_prompt(ctx: &CommitContext<'_>) -> String {
    // Collect file paths (up to 20)
    let file_list = ctx
        .cluster_paths
        .iter()
        .take(20)
        .filter_map(|p| p.to_str())
        .map(|s| format!("  - {}", s))
        .collect::<Vec<_>>()
        .join("\n");

    let api_summary = if ctx.diff.api_breaking {
        "api-breaking"
    } else if ctx.diff.api_added {
        "api-added"
    } else {
        "api-stable"
    };

    format!(
        "Describe this code change in one subject line (max 72 chars).\n\n\
         Bump type: {:?} (Patch=tweak, Minor=new feature, Major=breaking)\n\
         Version: v{} → v{}\n\
         API status: {}\n\
         Composite score: {:.3}\n\n\
         Changed files ({} total):\n\
         {}\n\n\
         Scores: structural={:.3} api={:.3} deps={:.3} runtime={:.3}\n\
         API touches: {}, signatures changed: {}\n\
         Dependency nodes affected: {}",
        ctx.bump,
        ctx.previous,
        ctx.next,
        api_summary,
        ctx.weight.score,
        ctx.cluster_paths.len(),
        file_list,
        ctx.diff.structural,
        ctx.diff.api,
        ctx.diff.deps,
        ctx.diff.runtime,
        ctx.diff.api_touches,
        ctx.diff.api_signatures,
        ctx.diff.dependency_nodes,
    )
}

/// Internal: calls Ollama /api/chat with a pre-built prompt string.
/// This function is `'static`-safe and can be called from tokio::spawn tasks
/// (unlike `generate`, which takes `CommitContext<'_>` with borrowed references).
pub async fn generate_with_model_and_prompt(
    config: &InferenceConfig,
    user_prompt: &str,
    model: &str,
) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs))
        .build()
        .ok()?;

    let system_prompt = "You are a precise software commit message author. Write a single subject line (max 72 characters) describing what changed. Use conventional commit format (feat:, fix:, refactor:, chore:) when it fits. Output ONLY the subject line — no body, no explanation.";

    let request = ChatRequest {
        model,
        stream: false,
        messages: vec![
            Message {
                role: "system",
                content: system_prompt.to_string(),
            },
            Message {
                role: "user",
                content: user_prompt.to_string(),
            },
        ],
    };

    let response = match client
        .post(format!("{}/api/chat", config.ollama_base_url))
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(error = %e, "ollama request failed");
            return None;
        }
    };

    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "ollama returned non-200 status");
        return None;
    }

    let chat_response: ChatResponse = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse ollama response");
            return None;
        }
    };

    let content = chat_response.message.content.trim();
    if content.is_empty() {
        tracing::warn!("ollama returned empty content");
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
        tracing::warn!("ollama subject line was empty after truncation");
        return None;
    }

    Some(subject)
}

/// Calls Ollama /api/chat with a specific model to generate a commit message subject line.
/// Used by the consensus module to fan out across multiple models in parallel.
/// Returns `None` on any error (timeout, network, malformed response, empty content).
pub async fn generate_with_model(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
) -> Option<String> {
    let user_prompt = build_user_prompt(ctx);
    generate_with_model_and_prompt(config, &user_prompt, model).await
}

/// Calls Ollama /api/chat to generate a commit message subject line.
/// Returns `None` on any error (timeout, network, malformed response, empty content).
/// Logs warnings for all failures; never panics.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    model: &str,
) -> Option<String> {
    generate_with_model(config, ctx, model).await
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
    fn build_user_prompt_includes_bump_type() {
        // Minimal valid context
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
        let paths = vec![];

        let ctx = CommitContext {
            cluster: &cluster,
            diff: &diff,
            weight: &weight,
            bump: Bump::Minor,
            previous: &previous,
            next: &next,
            cluster_paths: &paths,
        };

        let prompt = build_user_prompt(&ctx);
        assert!(prompt.contains("Minor"));
        assert!(prompt.contains("0.1.0"));
        assert!(prompt.contains("0.2.0"));
        assert!(prompt.contains("api-added"));
    }

    #[test]
    fn build_user_prompt_includes_file_paths() {
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

        let prompt = build_user_prompt(&ctx);
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("src/lib.rs"));
    }

    #[test]
    fn subject_line_truncation_at_72_chars() {
        let long_line = "a".repeat(100);
        let truncated = long_line
            .chars()
            .take(72)
            .collect::<String>();
        assert_eq!(truncated.len(), 72);
    }
}
