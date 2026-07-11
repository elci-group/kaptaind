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
