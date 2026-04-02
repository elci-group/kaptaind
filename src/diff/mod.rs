pub mod api;
pub mod ast;
pub mod text;

use crate::cluster::engine::Cluster;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffAnalysis {
    pub structural: f32,
    pub api: f32,
    pub deps: f32,
    pub runtime: f32,
    pub api_breaking: bool,
    pub api_added: bool,
    pub touched_paths: usize,
    pub api_touches: usize,
    pub api_signatures: usize,
    pub dependency_manifests: usize,
    pub dependency_nodes: usize,
    pub dependency_edges: usize,
    pub runtime_paths: usize,
}

pub fn analyze(cluster: &Cluster, repo_root: &Path) -> DiffAnalysis {
    let structural = text::structural_score(cluster);
    let api = ast::api_score(cluster, repo_root);
    let deps = api::dependency_score(cluster, repo_root);
    let runtime = api::runtime_score(cluster);

    DiffAnalysis {
        structural,
        api: api.score,
        deps: deps.score,
        runtime: runtime.score,
        api_breaking: api.breaking,
        api_added: api.added,
        touched_paths: touched_paths(cluster),
        api_touches: api.touches,
        api_signatures: api.signatures,
        dependency_manifests: deps.manifests,
        dependency_nodes: deps.nodes,
        dependency_edges: deps.edges,
        runtime_paths: runtime.paths,
    }
}

fn touched_paths(cluster: &Cluster) -> usize {
    cluster
        .events
        .iter()
        .flat_map(|event| event.paths.iter().cloned())
        .collect::<HashSet<_>>()
        .len()
}
