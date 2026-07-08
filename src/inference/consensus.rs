use super::CommitContext;
use crate::config::loader::InferenceConfig;
use std::collections::HashSet;

/// Content words to strip before Jaccard similarity computation.
/// Only includes very common words that add little semantic meaning.
/// Intentionally excludes commit prefixes (fix, feat, etc.) and action verbs (add, implement, etc.).
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "is", "to", "for", "in", "of", "and", "or", "but", "with", "from", "at",
    "by", "on", "as", "be", "this", "that", "it",
];

/// Tokenizes a subject line into content word tokens for Jaccard comparison.
/// Lowercases, splits on whitespace and punctuation boundaries,
/// strips stop words and short tokens (len <= 1).
fn content_tokens(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split(|c: char| c.is_whitespace() || c == ':' || c == ',' || c == '-' || c == '_')
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| t.len() > 1 && !STOP_WORDS.contains(t))
        .map(String::from)
        .collect()
}

/// Computes unigram Jaccard similarity between two token sets.
/// Returns 0.0 when both sets are empty (degenerate case — no consensus possible).
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        return 0.0;
    }
    intersection / union
}

/// Returns the mean Jaccard similarity of `candidate` against all other candidates.
/// If there is only one candidate, returns 1.0 (vacuous consensus).
fn mean_similarity(candidate_tokens: &HashSet<String>, others: &[HashSet<String>]) -> f64 {
    let comparisons: Vec<f64> = others
        .iter()
        .map(|other| jaccard(candidate_tokens, other))
        .collect();
    if comparisons.is_empty() {
        return 1.0;
    }
    comparisons.iter().sum::<f64>() / comparisons.len() as f64
}

/// Parallel multi-model consensus generator.
/// Spawns one Ollama task per model in `config.consensus_models`,
/// collects responses, applies semantic similarity scoring, and elects the best candidate.
/// Returns `None` if quorum is not reached or similarity threshold is not met.
pub async fn generate(config: &InferenceConfig, ctx: &CommitContext<'_>) -> Option<String> {
    if config.consensus_models.is_empty() {
        tracing::warn!("consensus_models is empty; cannot run consensus inference");
        return None;
    }

    tracing::info!(
        models = ?config.consensus_models,
        min_agreement = config.consensus_min_agreement,
        threshold = config.consensus_threshold,
        "consensus mode: spawning parallel model calls"
    );

    // Pre-build the user prompt once before spawning tasks.
    // CommitContext<'_> is not 'static, so we resolve it to a String here.
    let user_prompt = super::ollama::build_user_prompt(ctx);

    // Spawn parallel tasks — one per model.
    let mut handles: Vec<tokio::task::JoinHandle<Option<String>>> = Vec::new();

    for model_name in &config.consensus_models {
        // Clone data needed to cross the async task boundary.
        let config_clone = config.clone();
        let model_clone = model_name.clone();
        let user_prompt_clone = user_prompt.clone();

        handles.push(tokio::spawn(async move {
            super::ollama::generate_with_model_and_prompt(
                &config_clone,
                &user_prompt_clone,
                &model_clone,
            )
            .await
        }));
    }

    // Collect successful responses.
    let mut candidates: Vec<String> = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Some(msg)) => candidates.push(msg),
            Ok(None) => {} // model failed or returned empty
            Err(e) => tracing::warn!(error = %e, "consensus task panicked"),
        }
    }

    tracing::info!(
        total_models = config.consensus_models.len(),
        responding = candidates.len(),
        "consensus: collected responses"
    );

    // Quorum check.
    if candidates.len() < config.consensus_min_agreement {
        tracing::warn!(
            responding = candidates.len(),
            required = config.consensus_min_agreement,
            "consensus: insufficient responses; falling back to deterministic"
        );
        return None;
    }

    // Tokenize all candidates once.
    let token_sets: Vec<HashSet<String>> = candidates.iter().map(|c| content_tokens(c)).collect();

    // Score each candidate by mean similarity to all others.
    let scores: Vec<f64> = token_sets
        .iter()
        .enumerate()
        .map(|(i, tokens)| {
            let others: Vec<HashSet<String>> = token_sets
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, t)| t.clone())
                .collect();
            mean_similarity(tokens, &others)
        })
        .collect();

    // Log scores for observability.
    for (candidate, score) in candidates.iter().zip(scores.iter()) {
        tracing::info!(
            candidate = %candidate,
            score = score,
            "consensus: candidate similarity score"
        );
    }

    // Find the highest-scoring candidate.
    let (best_idx, &best_score) = scores
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;

    if best_score < config.consensus_threshold {
        tracing::warn!(
            best_score = best_score,
            threshold = config.consensus_threshold,
            "consensus: best score below threshold; no consensus reached"
        );
        return None;
    }

    let elected = candidates[best_idx].clone();
    tracing::info!(
        elected = %elected,
        score = best_score,
        "consensus: elected candidate"
    );

    Some(elected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_tokens_strips_stop_words() {
        let s = "feat: add OAuth2 provider support";
        let tokens = content_tokens(s);
        // Common prepositions and articles are stripped
        assert!(!tokens.contains("the"));
        assert!(!tokens.contains("a"));
        // But domain-specific words are retained
        assert!(tokens.contains("feat"));
        assert!(tokens.contains("add"));
        assert!(tokens.contains("oauth2"));
        assert!(tokens.contains("provider"));
        assert!(tokens.contains("support"));
    }

    #[test]
    fn content_tokens_filters_short() {
        let s = "fix a b issue";
        let tokens = content_tokens(s);
        // "fix" (3), "issue" (5) pass; "a" (1), "b" (1) filtered
        assert!(tokens.contains("fix"));
        assert!(tokens.contains("issue"));
        assert!(!tokens.contains("a"));
        assert!(!tokens.contains("b"));
    }

    #[test]
    fn jaccard_identical() {
        let a: HashSet<String> = ["oauth", "provider"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let b: HashSet<String> = ["oauth", "provider"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!((jaccard(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_disjoint() {
        let a: HashSet<String> = ["oauth", "provider"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let b: HashSet<String> = ["auth", "middleware"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!((jaccard(&a, &b) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_partial() {
        // a: {oauth, provider}
        // b: {oauth, middleware}
        // intersection: {oauth}
        // union: {oauth, provider, middleware}
        // jaccard: 1/3 ≈ 0.333
        let a: HashSet<String> = ["oauth", "provider"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let b: HashSet<String> = ["oauth", "middleware"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let sim = jaccard(&a, &b);
        assert!((sim - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn jaccard_both_empty() {
        let a: HashSet<String> = HashSet::new();
        let b: HashSet<String> = HashSet::new();
        assert!((jaccard(&a, &b) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn mean_similarity_empty_others() {
        let tokens: HashSet<String> = ["oauth", "provider"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let others: Vec<HashSet<String>> = vec![];
        assert!((mean_similarity(&tokens, &others) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn mean_similarity_single_other() {
        // tokens: {oauth, provider}
        // other: {oauth, middleware}
        // jaccard: 1/3
        // mean: 1/3
        let tokens: HashSet<String> = ["oauth", "provider"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let others: Vec<HashSet<String>> = vec![["oauth", "middleware"]
            .iter()
            .map(|s| s.to_string())
            .collect()];
        let sim = mean_similarity(&tokens, &others);
        assert!((sim - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn consensus_elects_most_similar() {
        // Three candidates:
        // A: "feat: add oauth2 provider"
        // B: "feat: implement oauth2 authentication"
        // C: "fix: broken imports"
        // A and B should be close (both about oauth2); C is an outlier.
        let a = "feat: add oauth2 provider";
        let b = "feat: implement oauth2 authentication";
        let c = "fix: broken imports";

        let a_tokens = content_tokens(a);
        let b_tokens = content_tokens(b);
        let c_tokens = content_tokens(c);

        // Score A (vs [B, C])
        let score_a = mean_similarity(&a_tokens, &[b_tokens.clone(), c_tokens.clone()]);
        // Score B (vs [A, C])
        let score_b = mean_similarity(&b_tokens, &[a_tokens.clone(), c_tokens.clone()]);
        // Score C (vs [A, B])
        let score_c = mean_similarity(&c_tokens, &[a_tokens.clone(), b_tokens.clone()]);

        // A and B should have similar high scores; C should be low.
        // The important property: A and B > C
        assert!(score_a > score_c);
        assert!(score_b > score_c);
    }
}
