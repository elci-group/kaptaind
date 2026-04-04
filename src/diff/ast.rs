use crate::cluster::engine::Cluster;
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
}

pub fn api_score(cluster: &Cluster, repo_root: &Path) -> ApiAnalysis {
    let mut touches = 0_usize;
    let mut exported_signatures = HashSet::new();
    let mut api_breaking = false;
    let mut api_added = false;

    for event in &cluster.events {
        for path in &event.paths {
            let resolved = resolve_path(repo_root, path);
            let signatures = extract_signatures(&resolved);
            let is_surface = is_api_surface(path) || !signatures.is_empty();
            if !is_surface {
                continue;
            }

            touches += 1;
            exported_signatures.extend(signatures);
            match event.kind {
                FsEventKind::Remove => {
                    api_breaking = true;
                }
                FsEventKind::Create => {
                    api_added = true;
                }
                FsEventKind::Modify | FsEventKind::Other => {}
            }
        }
    }

    let touch_score = (touches as f32 / 4.0).clamp(0.0, 1.0);
    let signature_score = (exported_signatures.len() as f32 / 8.0).clamp(0.0, 1.0);

    ApiAnalysis {
        score: (0.55 * touch_score + 0.45 * signature_score).clamp(0.0, 1.0),
        touches,
        signatures: exported_signatures.len(),
        breaking: api_breaking,
        added: api_added,
    }
}

fn extract_signatures(path: &Path) -> HashSet<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return HashSet::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter_map(signature_from_line)
        .collect()
}

fn signature_from_line(line: &str) -> Option<String> {
    const PREFIXES: &[&str] = &[
        "pub fn ",
        "pub async fn ",
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "pub type ",
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
        "def ",
        "class ",
    ];

    // CSS custom property (design token): `--primary: #000;`
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
    // Handle both absolute-ish paths (/app/) and relative (app/)
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
        assert!(analysis.score > 0.2);
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
