use crate::cluster::engine::Cluster;
use crate::diff::cache::{self, AstCache};
use crate::diff::lang::adapter::FileParseMetadata;
use crate::diff::lang::{normalize, AdapterRegistry};
use crate::diff::version::{detect_all, VersionCache};
use crate::watcher::FsEventKind;
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
    /// Number of files that missed cache and were parsed fresh.
    pub cache_misses: usize,
    /// Per-file LV-SCL parse metadata.
    pub parse_metadata: Vec<FileParseMetadata>,
}

pub fn api_score(cluster: &Cluster, repo_root: &Path) -> ApiAnalysis {
    let mut ast_cache = AstCache::load(repo_root);
    let result = api_score_with_cache(cluster, repo_root, &mut ast_cache);
    ast_cache.save(repo_root);
    result
}

fn load_versions(
    repo_root: &Path,
) -> std::collections::HashMap<String, crate::diff::version::detector::LanguageVersion> {
    let mut version_cache = VersionCache::load(repo_root);
    let versions = detect_all(&mut version_cache, repo_root);
    version_cache.save(repo_root);
    versions
}

pub fn api_score_with_cache(
    cluster: &Cluster,
    repo_root: &Path,
    ast_cache: &mut AstCache,
) -> ApiAnalysis {
    api_score_inner(
        cluster,
        repo_root,
        ast_cache,
        AdapterRegistry::default_registry(),
    )
}

pub fn api_score_with_plugins(
    cluster: &Cluster,
    repo_root: &Path,
    ast_cache: &mut AstCache,
    plugins: &crate::config::loader::PluginsConfig,
) -> ApiAnalysis {
    api_score_inner(
        cluster,
        repo_root,
        ast_cache,
        AdapterRegistry::default_registry_with_plugins(plugins),
    )
}

fn api_score_inner(
    cluster: &Cluster,
    repo_root: &Path,
    ast_cache: &mut AstCache,
    registry: AdapterRegistry,
) -> ApiAnalysis {
    let mut touches = 0_usize;
    let mut exported_signatures = HashSet::new();
    let mut api_breaking = false;
    let mut api_added = false;
    let mut cache_hits = 0_usize;
    let mut cache_misses = 0_usize;
    let mut parse_metadata: Vec<FileParseMetadata> = Vec::new();
    let mut max_score = 0.0_f32;

    // Detect language versions once per analysis run
    let lang_versions = load_versions(repo_root);

    for event in &cluster.events {
        for path in &event.paths {
            let resolved = resolve_path(repo_root, path);

            // Check if any adapter handles this file type
            if let Some(adapter) = registry.resolve(&resolved) {
                // Try cache first: hash the file and check for a cached AST
                let relative = path.strip_prefix(repo_root).unwrap_or(path);
                let relative_str = relative.to_string_lossy().to_string();
                let file_hash = cache::hash_file(&resolved);

                // Resolve version string for this language
                let lang_version = lang_versions.get(adapter.language().as_str()).cloned();
                let version_str = lang_version
                    .as_ref()
                    .map(|lv| lv.version.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let version_source = lang_version.as_ref().map(|lv| lv.source);

                let ast = if let Some(ref h) = file_hash {
                    if let Some(cached) = ast_cache.get(&relative_str, h) {
                        cache_hits += 1;
                        cached
                    } else {
                        cache_misses += 1;
                        let parsed = adapter
                            .parse_ast_versioned(&resolved, &version_str)
                            .unwrap_or_default();
                        ast_cache.put(&relative_str, h, &parsed);
                        parsed
                    }
                } else {
                    cache_misses += 1;
                    adapter
                        .parse_ast_versioned(&resolved, &version_str)
                        .unwrap_or_default()
                };

                // Compute confidence: start at 1.0 for AST, reduce if fallback or version uncertain
                let mut confidence: f64 = if ast.fallback_used { 0.65 } else { 0.95 };
                let version_match_enum = match version_source {
                    Some(crate::diff::version::detector::VersionSource::Runtime) => {
                        confidence = confidence.max(0.98);
                        crate::diff::lang::adapter::VersionMatch::Runtime
                    }
                    Some(crate::diff::version::detector::VersionSource::Manifest) => {
                        crate::diff::lang::adapter::VersionMatch::Manifest
                    }
                    Some(crate::diff::version::detector::VersionSource::Inferred) => {
                        confidence *= 0.85;
                        crate::diff::lang::adapter::VersionMatch::Inferred
                    }
                    None => {
                        confidence *= 0.70;
                        crate::diff::lang::adapter::VersionMatch::Unknown
                    }
                };

                // Record LV-SCL metadata for this file
                parse_metadata.push(FileParseMetadata {
                    file: relative_str.clone(),
                    lang: adapter.name().to_string(),
                    version: version_str,
                    parser_used: format!("{:?}", ast.parser_kind),
                    fallback_used: ast.fallback_used,
                    confidence: confidence.clamp(0.0, 1.0),
                    version_match: version_match_enum,
                });

                let api_surface = adapter.extract_api(&ast);
                let signatures: HashSet<String> = api_surface
                    .public_symbols
                    .into_iter()
                    .map(|s| s.name)
                    .collect();

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
                let local_score: f32 =
                    (0.55 * local_touch_score + 0.45 * local_sig_score).clamp(0.0, 1.0);

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

                // Record fallback metadata
                let relative = path.strip_prefix(repo_root).unwrap_or(path);
                parse_metadata.push(FileParseMetadata {
                    file: relative.to_string_lossy().to_string(),
                    lang: "unknown".to_string(),
                    version: "unknown".to_string(),
                    parser_used: "FallbackLineScanner".to_string(),
                    fallback_used: true,
                    confidence: 0.45,
                    version_match: crate::diff::lang::adapter::VersionMatch::Unknown,
                });

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
                let local_score: f32 =
                    (0.55 * local_touch_score + 0.45 * local_sig_score).clamp(0.0, 1.0);
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
        cache_misses,
        parse_metadata,
    }
}

fn extract_signatures_fallback(path: &Path) -> HashSet<String> {
    use std::io::{BufRead, BufReader};
    let Ok(meta) = std::fs::metadata(path) else {
        return HashSet::new();
    };
    if meta.len() > 5 * 1024 * 1024 || meta.is_dir() {
        return HashSet::new();
    }
    let Ok(file) = std::fs::File::open(path) else {
        return HashSet::new();
    };

    let reader = BufReader::new(file);
    reader
        .lines()
        .map_while(Result::ok)
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

    PREFIXES.iter().find_map(|prefix| {
        line.strip_prefix(prefix)
            .map(|rest| format!("{prefix}{rest}"))
    })
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
    let has_route_dir = route_dirs
        .iter()
        .any(|dir| as_text.contains(&format!("/{dir}")) || as_text.starts_with(dir));
    if !has_route_dir {
        return false;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "tsx" | "ts" | "jsx" | "js" | "svelte" | "vue" | "astro"
    )
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
        std::fs::write(&file_path, "pub fn expose() {}\npub struct Api;\n")
            .expect("write api file");

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/api.rs")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        assert!(analysis.score > 0.15);
        // syn parser extracts "expose()" as function + "Api" as struct = 2 symbols
        assert!(analysis.signatures >= 2);
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
        std::fs::write(
            &file_path,
            "export default function App() {}\nexport const API_URL = \"test\";\n",
        )
        .unwrap();

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

    #[test]
    fn syn_parses_multiline_rust_function() {
        let dir = tempdir().expect("temp dir");
        let file = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"
pub fn complex_function(
    name: &str,
    count: usize,
    options: Option<Config>,
) -> Result<Vec<String>, Error> {
    todo!()
}
"#,
        )
        .unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/lib.rs")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        // syn should parse multi-line signature correctly
        assert!(analysis.signatures >= 1);
        assert!(analysis.added);
    }

    #[test]
    fn syn_parses_trait_with_methods() {
        let dir = tempdir().expect("temp dir");
        let file = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"
pub trait Service {
    type Error;
    fn call(&self, req: Request) -> Response;
    fn health(&self) -> bool;
}
"#,
        )
        .unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/lib.rs")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        // trait + 2 methods + 1 associated type = 4 symbols
        assert!(analysis.signatures >= 4);
    }

    #[test]
    fn syn_parses_enum_with_variants() {
        let dir = tempdir().expect("temp dir");
        let file = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"
pub enum Color {
    Red,
    Green,
    Blue,
}
"#,
        )
        .unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/lib.rs")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        // enum + 3 variants = 4 symbols
        assert!(analysis.signatures >= 4);
    }

    #[test]
    fn syn_parses_impl_methods() {
        let dir = tempdir().expect("temp dir");
        let file = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"
pub struct Server {
    pub port: u16,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Server { port }
    }
    pub fn start(&self) {}
    fn internal(&self) {} // not pub, should not appear
}
"#,
        )
        .unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/lib.rs")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        // struct + pub field + 2 pub methods = 4 symbols (internal() excluded)
        assert!(analysis.signatures >= 4);
    }

    #[test]
    fn syn_ignores_private_items() {
        let dir = tempdir().expect("temp dir");
        let file = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"
fn private_fn() {}
struct PrivateStruct { field: i32 }
enum PrivateEnum { A, B }
trait PrivateTrait { fn method(&self); }
pub fn public_fn() {}
"#,
        )
        .unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/lib.rs")],
            kind: FsEventKind::Create,
            timestamp: Utc::now(),
        });

        let analysis = api_score(&cluster, dir.path());
        // Only public_fn should be detected
        assert_eq!(analysis.signatures, 1);
    }

    #[test]
    fn cache_is_used_on_second_analysis() {
        let dir = tempdir().expect("temp dir");
        let file = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "pub fn hello() {}\n").unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/lib.rs")],
            kind: FsEventKind::Modify,
            timestamp: Utc::now(),
        });

        // First call: populates cache
        let mut cache = crate::diff::cache::AstCache::default();
        let a1 = super::api_score_with_cache(&cluster, dir.path(), &mut cache);
        assert_eq!(a1.cache_hits, 0);
        assert!(!cache.is_empty());

        // Second call with same file: should hit cache
        let a2 = super::api_score_with_cache(&cluster, dir.path(), &mut cache);
        assert_eq!(a2.cache_hits, 1);
        assert_eq!(a1.signatures, a2.signatures);
    }

    #[test]
    fn cache_invalidated_on_file_change() {
        let dir = tempdir().expect("temp dir");
        let file = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "pub fn hello() {}\n").unwrap();

        let cluster = cluster_with_event(FsEvent {
            paths: vec![PathBuf::from("src/lib.rs")],
            kind: FsEventKind::Modify,
            timestamp: Utc::now(),
        });

        // First call
        let mut cache = crate::diff::cache::AstCache::default();
        let a1 = super::api_score_with_cache(&cluster, dir.path(), &mut cache);
        assert_eq!(a1.signatures, 1);

        // Modify the file
        std::fs::write(&file, "pub fn hello() {}\npub fn world() {}\n").unwrap();

        // Second call: cache miss due to changed hash
        let a2 = super::api_score_with_cache(&cluster, dir.path(), &mut cache);
        assert_eq!(a2.cache_hits, 0);
        assert_eq!(a2.signatures, 2);
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
