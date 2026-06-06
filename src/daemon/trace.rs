use crate::aoc::tracer;
use crate::cluster::engine::Cluster;
use crate::watcher::FsEventKind;
use std::path::Path;

/// Build and write an Aim-of-Change trace record for a cluster when a session is active.
pub fn write_trace_if_active(
    repo_path: &Path,
    cluster: &Cluster,
    result: tracer::TraceResult,
    test: tracer::TraceTest,
    agent_event: Option<crate::aoc::AgentEvent>,
) {
    match crate::aoc::session::load_active(repo_path) {
        Ok(Some(session)) => {
            let events = cluster
                .events
                .iter()
                .map(|evt| tracer::TraceEvent {
                    paths: evt
                        .paths
                        .iter()
                        .filter_map(|p| p.to_str().map(|s| s.to_string()))
                        .collect(),
                    kind: match evt.kind {
                        FsEventKind::Create => "create".to_string(),
                        FsEventKind::Modify => "modify".to_string(),
                        FsEventKind::Remove => "remove".to_string(),
                        FsEventKind::Other => "other".to_string(),
                    },
                    t: evt.timestamp,
                })
                .collect();

            let duration_ms = (cluster.ended_at - cluster.started_at)
                .num_milliseconds()
                .max(0) as u64;

            let trace = tracer::TraceRecord {
                cluster_id: cluster.id.to_string(),
                aoc_id: session.id.clone(),
                started_at: cluster.started_at,
                ended_at: cluster.ended_at,
                duration_ms,
                events,
                test,
                result,
                analysis_ref: Some(format!(".kaptaind/analysis/{}.json", cluster.id)),
                agent_event,
            };

            if let Err(err) = crate::aoc::db::save_trace(repo_path, &trace) {
                tracing::warn!(error = %err, "failed to write AoC trace to database");
            }
        }
        Ok(None) => {}
        Err(err) => {
            tracing::debug!(error = %err, "failed to load active AoC");
        }
    }
}
