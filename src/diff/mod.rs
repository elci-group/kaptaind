pub mod api;
pub mod ast;
pub mod bundle;
pub mod cache;
pub mod lang;
pub mod text;
pub mod version;

use crate::cluster::engine::Cluster;
use crate::diff::lang::adapter::FileParseMetadata;
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
    #[serde(default)]
    pub bundle: f32,
    /// Per-file parse metadata emitted by the LV-SCL layer.
    #[serde(default)]
    pub parse_metadata: Vec<FileParseMetadata>,
    /// AST cache hits this analysis (files served from cache).
    #[serde(default)]
    pub ast_cache_hits: usize,
    /// AST cache misses this analysis (files parsed fresh).
    #[serde(default)]
    pub ast_cache_misses: usize,
    /// Total entries in the AST cache after this analysis.
    #[serde(default)]
    pub ast_cache_entries: usize,
}

pub fn analyze_with_plugins(
    cluster: &Cluster,
    repo_root: &Path,
    plugins: &crate::config::loader::PluginsConfig,
) -> DiffAnalysis {
    let mut ast_cache = crate::diff::cache::AstCache::load(repo_root);
    let api = ast::api_score_with_plugins(cluster, repo_root, &mut ast_cache, plugins);
    let cache_entries = ast_cache.len();
    ast_cache.save(repo_root);
    let deps = api::dependency_score(cluster, repo_root);
    let runtime = api::runtime_score(cluster);
    let structural = text::structural_score(cluster);
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
        bundle: 0.0,
        parse_metadata: api.parse_metadata,
        ast_cache_hits: api.cache_hits,
        ast_cache_misses: api.cache_misses,
        ast_cache_entries: cache_entries,
    }
}

pub fn analyze(cluster: &Cluster, repo_root: &Path) -> DiffAnalysis {
    let structural = text::structural_score(cluster);
    let mut ast_cache = crate::diff::cache::AstCache::load(repo_root);
    let api = ast::api_score_with_cache(cluster, repo_root, &mut ast_cache);
    let cache_entries = ast_cache.len();
    ast_cache.save(repo_root);
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
        bundle: 0.0, // Bundle score is calculated after diff analysis
        parse_metadata: api.parse_metadata,
        ast_cache_hits: api.cache_hits,
        ast_cache_misses: api.cache_misses,
        ast_cache_entries: cache_entries,
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
