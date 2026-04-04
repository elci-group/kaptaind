use crate::cluster::engine::Cluster;
use crate::diff::cache::{self, AstCache};
use crate::watcher::FsEventKind;
use crate::diff::lang::{normalize, AdapterRegistry};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ApiAnalysis {
    pub score: f32,
    pub touches: usize,
    pub signatures: usize,
    pub breaking: bool,
    pub added: bool,
    /// Number of files served from cache (skipped re-parsing).
    pub cache_hits: usize,
}

pub fn api_score(cluster: &Cluster, repo_root: &Path) -> ApiAnalysis {
    let mut ast_cache = AstCache::load(repo_root);
    let result = api_score_with_cache(cluster, repo_root, &mut ast_cache);
    ast_cache.save(repo_root);
    result
}

pub fn api_score_with_cache(cluster: &Cluster, repo_root: &Path, ast_cache: &mut AstCache) -> ApiAnalysis {
    let mut touches = 0_usize;
    let mut exported_signatures = HashSet::new();
    let mut api_breaking = false;
    let mut api_added = false;
    let mut cache_hits = 0_usize;

    let registry = AdapterRegistry::default_registry();
    let mut max_score = 0.0_f32;

    for event in &cluster.events {
        for path in &event.paths {
            let resolved = resolve_path(repo_root, path);

            // Check if any adapter handles this file type
            if let Some(adapter) = registry.resolve(&resolved) {
                // Try cache first: hash the file and check for a cached AST
                let relative = path.strip_prefix(repo_root).unwrap_or(path);
                let relative_str = relative.to_string_lossy().to_string();
                let file_hash = cache::hash_file(&resolved);

                let ast = if let Some(ref h) = file_hash {
                    if let Some(cached) = ast_cache.get(&relative_str, h) {
                        cache_hits += 1;
                        cached
                    } else {
                        let parsed = adapter.parse_ast(&resolved).unwrap_or_default();
                        ast_cache.put(&relative_str, h, &parsed);
                        parsed
                    }
                } else {
                    adapter.parse_ast(&resolved).unwrap_or_default()
                };

                let api_surface = adapter.extract_api(&ast);
                let signatures: HashSet<String> = api_surface.public_symbols.into_iter().map(|s| s.name).collect();
                
                // Fallback surface detection (routes, design tokens) still valid across languages
                let is_surface = is_api_surface(path) || !signatures.is_empty();
                if !is_surface {
                    continue;
                }

                touches += 1;
                exported_signatures.extend(signatures.clone());
                
                // Note: Full AST diff requires old state which we approximate here
                match event.kind {
                    FsEventKind::Remove => {
                        api_breaking = true;
                    }
                    FsEventKind::Create => {
                        api_added = true;
                    }
                    FsEventKind::Modify | FsEventKind::Other => {
                        // Rough assumption based on current event capabilities
                    }
                }
                
                let local_touch_score = 0.25_f32; // per file heuristic
                let local_sig_score = (signatures.len() as f32 / 8.0).clamp(0.0, 1.0);
                let local_score: f32 = (0.55 * local_touch_score + 0.45 * local_sig_score).clamp(0.0, 1.0);
                
                let normalized_score = normalize(local_score, adapter.language());
                if normalized_score > max_score {
                    max_score = normalized_score;
                }
            } else {
                // Fallback for languages not explicitly in the adapter registry
                // but identified by path heuristics
                let signatures = extract_signatures_fallback(&resolved);
                let is_surface = is_api_surface(path) || !signatures.is_empty();
                if !is_surface {
                    continue;
                }

                touches += 1;
                exported_signatures.extend(signatures.clone());
                match event.kind {
                    FsEventKind::Remove => {
                        api_breaking = true;
                    }
                    FsEventKind::Create => {
                        api_added = true;
                    }
                    FsEventKind::Modify | FsEventKind::Other => {}
                }
                
                let local_touch_score = 0.25_f32;
                let local_sig_score = (signatures.len() as f32 / 8.0).clamp(0.0, 1.0);
                let local_score: f32 = (0.55 * local_touch_score + 0.45 * local_sig_score).clamp(0.0, 1.0);
                if local_score > max_score {
                    max_score = local_score;
                }
            }
        }
    }

    let touch_score = (touches as f32 / 4.0).clamp(0.0, 1.0);
    let signature_score = (exported_signatures.len() as f32 / 8.0).clamp(0.0, 1.0);
    let combined_global_score = (0.55 * touch_score + 0.45 * signature_score).clamp(0.0, 1.0);
    
    // We blend the max normalized individual file score with the global aggregate score
    let final_score = if touches > 0 {
        ((max_score + combined_global_score) / 2.0).clamp(0.0, 1.0)
    } else {
        0.0
    };

    ApiAnalysis {
        score: final_score,
        touches,
        signatures: exported_signatures.len(),
        breaking: api_breaking,
        added: api_added,
        cache_hits,
    }
}

fn extract_signatures_fallback(path: &Path) -> HashSet<String> {
    use std::io::{BufRead, BufReader};
    let Ok(meta) = std::fs::metadata(path) else {
        return HashSet::new();
    };
    if meta.len() > 5 * 1024 * 1024 {
        return HashSet::new();
    }
    let Ok(file) = std::fs::File::open(path) else {
        return HashSet::new();
    };

    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(Result::ok)
        .map(|s| s.trim().to_string())
        .filter_map(|s| signature_from_line(&s))
        .collect()
}

fn signature_from_line(line: &str) -> Option<String> {
    const PREFIXES: &[&str] = &[
        // JS/TS
        "export function ",
        "export async function ",
        "export class ",
        "export interface ",
        "export type ",
        "export default function ",
        "export default class ",
        "export default ",
        "export const ",
        "export let ",
        "export var ",
        // Python
        "def ",
        "class ",
        // Swift
        "public func ",
        "open func ",
        "public class ",
        "open class ",
        "public struct ",
        "public enum ",
        "public protocol ",
        // Kotlin
        "fun ",
        "data class ",
        "sealed class ",
        "enum class ",
        "object ",
        "interface ",
        "suspend fun ",
        "annotation class ",
    ];

    if line.starts_with("--") && line.contains(':') {
        return Some(line.to_string());
    }

    PREFIXES
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix).map(|rest| format!("{prefix}{rest}")))
}

fn is_api_surface(path: &Path) -> bool {
    let as_text = path.to_string_lossy().to_lowercase();
    as_text.contains("/api/")
        || as_text.contains("/public/")
        || as_text.ends_with(".proto")
        || as_text.ends_with(".graphql")
        || as_text.ends_with("openapi.yaml")
        || as_text.ends_with("openapi.yml")
        || is_route_file(path)
        || is_design_token_file(path)
}

/// Detects framework route files (Next.js, Remix, SvelteKit, etc.)
fn is_route_file(path: &Path) -> bool {
    let as_text = path.to_string_lossy().to_lowercase();
    let route_dirs = ["app/", "pages/", "routes/", "src/routes/"];
    let has_route_dir = route_dirs.iter().any(|dir| {
        as_text.contains(&format!("/{dir}")) || as_text.starts_with(dir)
    });
    if !has_route_dir {
        return false;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "tsx" | "ts" | "jsx" | "js" | "svelte" | "vue" | "astro")
}

/// Detects design token / theme config files
fn is_design_token_file(path: &Path) -> bool {
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    matches!(
        file_stem.as_str(),
        "tailwind.config" | "theme" | "tokens" | "design-tokens"
    )
}

fn resolve_path(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::api_score;
    use crate::cluster::engine::Cluster;
    use crate::watcher::{FsEvent, FsEventKind};
    use chrono::Utc;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn detects_added_rust_public_api() {
        let dir = tempdir().expect("temp dir");
        let file_path = dir.path().join("src/api.rs");
        std::fs::create_dir_all(file_path.parent().expect("parent")).expect("create parent");
        std::fs::write(&file_path, "pub fn expose() {}\npub struct Api;\n").expect("write api file");

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/api.rs")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        assert!(analysis.score > 0.15); // with normalization and averaging, score > 0.15
        assert_eq!(analysis.signatures, 2);
        assert!(!analysis.breaking);
        assert!(analysis.added);
    }

    #[test]
    fn detects_removed_api_surface_as_breaking() {
        let dir = tempdir().expect("temp dir");
        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("schemas/openapi.yaml")],
            kind: FsEventKind::Remove,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        assert!(analysis.breaking);
        assert!(!analysis.added);
    }

    #[test]
    fn detects_export_default_function_signature() {
        let dir = tempdir().expect("temp dir");
        let file_path = dir.path().join("src/component.tsx");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "export default function App() {}\nexport const API_URL = \"test\";\n").unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/component.tsx")],
            kind: FsEventKind::Modify,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        // Typescript adapter handles exports
        assert_eq!(analysis.signatures, 2);
    }

    #[test]
    fn detects_route_file_as_api_surface() {
        let dir = tempdir().expect("temp dir");
        let file_path = dir.path().join("app/dashboard/page.tsx");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "// empty page\n").unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("app/dashboard/page.tsx")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        assert_eq!(analysis.touches, 1);
        assert!(analysis.added);
    }

    #[test]
    fn detects_removed_pages_route_as_breaking() {
        let dir = tempdir().expect("temp dir");
        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("pages/about.tsx")],
            kind: FsEventKind::Remove,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        assert!(analysis.breaking);
    }

    #[test]
    fn detects_design_token_file_as_api_surface() {
        let dir = tempdir().expect("temp dir");
        let file_path = dir.path().join("tailwind.config.ts");
        std::fs::write(&file_path, "export default {}\n").unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("tailwind.config.ts")],
            kind: FsEventKind::Modify,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        assert_eq!(analysis.touches, 1);
    }

    #[test]
    fn detects_css_custom_property_as_signature() {
        let dir = tempdir().expect("temp dir");
        let file_path = dir.path().join("tokens.css");
        std::fs::write(&file_path, "--primary: #000;\n--spacing-lg: 2rem;\n").unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("tokens.css")],
            kind: FsEventKind::Modify,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        assert_eq!(analysis.signatures, 2);
    }

    fn cluster_with_event(event: FsEvent) -> Cluster {
        Cluster {
            id: Uuid::new_v4(),
            started_at: event.timestamp,
            ended_at: event.timestamp,
            events: vec![event],
        }
    }
}
