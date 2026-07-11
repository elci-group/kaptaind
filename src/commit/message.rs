//! Deterministic commit message formatting.
//!
//! Extracted from the scheduler so both the daemon pipeline and
//! `kaptaind --dry-run` render the exact same message for a given cluster.

use crate::cluster::engine::Cluster;
use crate::diff::DiffAnalysis;
use crate::version::Bump;
use crate::weight::WeightResult;
use semver::Version;

/// Render the deterministic kaptaind commit message for a cluster decision.
pub fn format_commit(
    cluster: &Cluster,
    diff: &DiffAnalysis,
    weight: &WeightResult,
    bump: Bump,
    version: &Version,
    agent_event: &Option<crate::aoc::AgentEvent>,
) -> String {
    let api_summary = if diff.api_breaking {
        "breaking-api"
    } else if diff.api_added {
        "api-added"
    } else {
        "api-stable"
    };

    let agent_info = if let Some(agent) = agent_event {
        let model = agent.model.as_deref().unwrap_or("unknown");
        format!("; agent={model}")
    } else {
        String::new()
    };

    format!(
        "kaptaind: {bump:?} -> v{version} [{api_summary}; paths={}; api_touches={}; deps={}; runtime={}; score={:.3}; cluster={}{agent_info}]",
        diff.touched_paths,
        diff.api_touches,
        diff.dependency_nodes,
        diff.runtime_paths,
        weight.score,
        cluster.id,
    )
}

/// Render the deterministic non-bumping chore message used when
/// `[commit] require_bump = false` and the cluster scores below the patch
/// threshold (D1): the work is still captured, but VERSION, Cargo.toml and
/// Cargo.lock are left untouched.
///
/// The subject stays ≤72 chars and conventional-commit parseable; the body
/// keeps the same scorecard block as [`format_commit`].
pub fn format_chore_commit(
    cluster: &Cluster,
    diff: &DiffAnalysis,
    weight: &WeightResult,
    agent_event: &Option<crate::aoc::AgentEvent>,
) -> String {
    let api_summary = if diff.api_breaking {
        "breaking-api"
    } else if diff.api_added {
        "api-added"
    } else {
        "api-stable"
    };

    let agent_info = if let Some(agent) = agent_event {
        let model = agent.model.as_deref().unwrap_or("unknown");
        format!("; agent={model}")
    } else {
        String::new()
    };

    let subject = format!(
        "chore: capture {} path(s) below bump threshold",
        diff.touched_paths
    );
    let body = format!(
        "kaptaind: no-bump [{api_summary}; paths={}; api_touches={}; deps={}; runtime={}; score={:.3}; cluster={}{agent_info}]",
        diff.touched_paths,
        diff.api_touches,
        diff.dependency_nodes,
        diff.runtime_paths,
        weight.score,
        cluster.id,
    );
    format!("{subject}\n\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::{FsEvent, FsEventKind};
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_cluster() -> Cluster {
        let timestamp = Utc::now();
        Cluster {
            id: Uuid::new_v4(),
            started_at: timestamp,
            ended_at: timestamp,
            events: vec![FsEvent {
                paths: vec!["README.md".into()],
                kind: FsEventKind::Modify,
                timestamp,
            }],
        }
    }

    #[test]
    fn chore_subject_is_conventional_and_short() {
        let cluster = sample_cluster();
        let diff = DiffAnalysis {
            touched_paths: 3,
            ..DiffAnalysis::default()
        };
        let weight = WeightResult {
            score: 0.042,
            api_breaking: false,
            api_added: false,
        };

        let message = format_chore_commit(&cluster, &diff, &weight, &None);
        let subject = message.lines().next().expect("subject line");
        assert!(subject.starts_with("chore: "), "subject: {subject}");
        assert!(subject.len() <= 72, "subject too long: {subject}");
        assert!(message.contains("score=0.042"));
    }
}
