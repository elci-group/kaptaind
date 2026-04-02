use crate::cluster::engine::Cluster;
use std::collections::HashSet;

pub fn structural_score(cluster: &Cluster) -> f32 {
    if cluster.events.is_empty() {
        return 0.0;
    }

    let mut unique_paths = HashSet::new();
    let mut weighted_events = 0.0_f32;

    for event in &cluster.events {
        for path in &event.paths {
            unique_paths.insert(path.clone());
        }
        weighted_events += 1.0;
    }

    let span_ms = (cluster.ended_at - cluster.started_at)
        .num_milliseconds()
        .max(1) as f32;
    let event_density = (weighted_events / 24.0).clamp(0.0, 1.0);
    let path_spread = (unique_paths.len() as f32 / 16.0).clamp(0.0, 1.0);
    let churn = ((span_ms / 1000.0) / 20.0).clamp(0.0, 1.0);

    (0.5 * event_density + 0.35 * path_spread + 0.15 * churn).clamp(0.0, 1.0)
}
