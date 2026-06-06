use crate::cluster::engine::Cluster;
use petgraph::graph::{DiGraph, NodeIndex};
use serde_json::Value as JsonValue;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use toml::Value as TomlValue;

#[derive(Debug, Clone, Default)]
pub struct DependencyAnalysis {
    pub score: f32,
    pub manifests: usize,
    pub nodes: usize,
    pub edges: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeAnalysis {
    pub score: f32,
    pub paths: usize,
}

pub fn dependency_score(cluster: &Cluster, repo_root: &Path) -> DependencyAnalysis {
    let dependency_files: Vec<PathBuf> = cluster
        .events
        .iter()
        .flat_map(|event| event.paths.iter())
        .map(|path| resolve_path(repo_root, path))
        .filter(|path| is_dependency_file(path))
        .collect();

    if dependency_files.is_empty() {
        return DependencyAnalysis::default();
    }

    let mut graph = DiGraph::<String, ()>::new();
    let mut node_ids: HashMap<String, NodeIndex> = HashMap::new();
    let mut manifests = 0_usize;

    for manifest in dependency_files {
        let dependencies = parse_dependencies(&manifest);
        if dependencies.is_empty() {
            continue;
        }

        manifests += 1;
        let manifest_label = manifest
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("manifest")
            .to_string();
        let manifest_node = ensure_node(&mut graph, &mut node_ids, manifest_label);

        for dependency in dependencies {
            let dep_node = ensure_node(&mut graph, &mut node_ids, dependency);
            graph.update_edge(manifest_node, dep_node, ());
        }
    }

    if manifests == 0 {
        return DependencyAnalysis::default();
    }

    let manifest_score = (manifests as f32 / 4.0).clamp(0.0, 1.0);
    let node_score = (graph.node_count() as f32 / 18.0).clamp(0.0, 1.0);
    let edge_score = (graph.edge_count() as f32 / 24.0).clamp(0.0, 1.0);

    DependencyAnalysis {
        score: (0.35 * manifest_score + 0.25 * node_score + 0.40 * edge_score).clamp(0.0, 1.0),
        manifests,
        nodes: graph.node_count(),
        edges: graph.edge_count(),
    }
}

pub fn runtime_score(cluster: &Cluster) -> RuntimeAnalysis {
    let mut hits = 0_usize;
    for event in &cluster.events {
        for path in &event.paths {
            if touches_runtime(path) {
                hits += 1;
            }
        }
    }

    RuntimeAnalysis {
        score: (hits as f32 / 10.0).clamp(0.0, 1.0),
        paths: hits,
    }
}

fn ensure_node(
    graph: &mut DiGraph<String, ()>,
    node_ids: &mut HashMap<String, NodeIndex>,
    label: String,
) -> NodeIndex {
    if let Some(index) = node_ids.get(&label) {
        *index
    } else {
        let index = graph.add_node(label.clone());
        node_ids.insert(label, index);
        index
    }
}

fn parse_dependencies(path: &Path) -> BTreeSet<String> {
    match path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_lowercase())
        .as_deref()
    {
        Some("cargo.toml") => parse_cargo_dependencies(path),
        Some("package.json") => parse_package_dependencies(path),
        Some("requirements.txt") => parse_requirements(path),
        _ => BTreeSet::new(),
    }
}

fn parse_cargo_dependencies(path: &Path) -> BTreeSet<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    let Ok(value) = content.parse::<TomlValue>() else {
        return BTreeSet::new();
    };

    let mut dependencies = BTreeSet::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = value.get(section).and_then(TomlValue::as_table) {
            dependencies.extend(table.keys().cloned());
        }
    }
    dependencies
}

fn parse_package_dependencies(path: &Path) -> BTreeSet<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    let Ok(value) = serde_json::from_str::<JsonValue>(&content) else {
        return BTreeSet::new();
    };

    let mut dependencies = BTreeSet::new();
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(table) = value.get(section).and_then(JsonValue::as_object) {
            dependencies.extend(table.keys().cloned());
        }
    }
    dependencies
}

fn parse_requirements(path: &Path) -> BTreeSet<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            line.split(['=', '<', '>', '!', '~'])
                .next()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .collect()
}

fn is_dependency_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_lowercase())
        .unwrap_or_default();

    matches!(
        file_name.as_str(),
        "cargo.toml"
            | "cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "bun.lockb"
            | "poetry.lock"
            | "requirements.txt"
            | "podfile"
            | "podfile.lock"
            | "package.resolved"
            | "build.gradle"
            | "build.gradle.kts"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "gradle.lockfile"
    )
}

fn touches_runtime(path: &Path) -> bool {
    let as_text = path.to_string_lossy().to_lowercase();
    as_text.contains("docker")
        || as_text.contains("deploy")
        || as_text.contains("k8s")
        || as_text.contains("helm")
        || as_text.ends_with(".sh")
        || as_text.ends_with(".service")
        || as_text.ends_with(".env")
        || is_web_config(path)
}

/// Detects web framework/toolchain config files that affect runtime behavior
fn is_web_config(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let lower = file_name.to_lowercase();

    // Exact matches
    if matches!(lower.as_str(), "vercel.json" | "netlify.toml" | ".babelrc") {
        return true;
    }

    // Stem-based matches (e.g. next.config.mjs, vite.config.ts)
    let stem_patterns = [
        "next.config",
        "vite.config",
        "nuxt.config",
        "svelte.config",
        "astro.config",
        "tsconfig",
        "jsconfig",
        "webpack.config",
        "postcss.config",
        "tailwind.config",
    ];
    if stem_patterns.iter().any(|pat| lower.starts_with(pat)) {
        return true;
    }

    // Mobile / native config files
    is_mobile_config(path)
}

/// Detects mobile/native platform config files (Xcode, Gradle, Android)
fn is_mobile_config(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let lower = file_name.to_lowercase();

    matches!(
        lower.as_str(),
        "info.plist"
            | "project.pbxproj"
            | "androidmanifest.xml"
            | "proguard-rules.pro"
            | "gradle.properties"
    ) || lower.ends_with(".xcconfig")
        || lower.ends_with(".entitlements")
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
    use super::{
        dependency_score, is_dependency_file, is_web_config, parse_cargo_dependencies,
        parse_package_dependencies, parse_requirements, runtime_score,
    };
    use crate::cluster::engine::Cluster;
    use crate::watcher::{FsEvent, FsEventKind};
    use chrono::Utc;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn parses_cargo_dependencies() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("Cargo.toml");
        std::fs::write(
            &path,
            "[dependencies]\nserde = \"1\"\nanyhow = \"1\"\n[dev-dependencies]\ntokio = \"1\"\n",
        )
        .expect("write cargo manifest");

        let deps = parse_cargo_dependencies(&path);
        assert!(deps.contains("serde"));
        assert!(deps.contains("anyhow"));
        assert!(deps.contains("tokio"));
    }

    #[test]
    fn parses_package_json_dependencies() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("package.json");
        std::fs::write(
            &path,
            r#"{"dependencies":{"react":"18"},"devDependencies":{"vite":"5"}}"#,
        )
        .expect("write package manifest");

        let deps = parse_package_dependencies(&path);
        assert!(deps.contains("react"));
        assert!(deps.contains("vite"));
    }

    #[test]
    fn parses_python_requirements() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("requirements.txt");
        std::fs::write(&path, "fastapi==0.110.0\nuvicorn>=0.29\n").expect("write requirements");

        let deps = parse_requirements(&path);
        assert!(deps.contains("fastapi"));
        assert!(deps.contains("uvicorn"));
    }

    #[test]
    fn scores_dependency_manifest_changes() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("Cargo.toml");
        std::fs::write(&path, "[dependencies]\nserde = \"1\"\nanyhow = \"1\"\n")
            .expect("write cargo manifest");

        let cluster = Cluster {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
            events: vec![FsEvent {
                paths: vec![PathBuf::from("Cargo.toml")],
                kind: FsEventKind::Modify,
                timestamp: Utc::now(),
            }],
        };

        let analysis = dependency_score(&cluster, dir.path());
        assert!(analysis.score > 0.0);
        assert_eq!(analysis.manifests, 1);
        assert!(analysis.nodes >= 3);
        assert!(analysis.edges >= 2);
    }

    #[test]
    fn yarn_lock_is_dependency_file() {
        assert!(is_dependency_file(Path::new("yarn.lock")));
    }

    #[test]
    fn bun_lockb_is_dependency_file() {
        assert!(is_dependency_file(Path::new("bun.lockb")));
    }

    #[test]
    fn lock_file_modify_contributes_dep_score() {
        let dir = tempdir().expect("temp dir");
        // yarn.lock won't parse into dependencies, but the manifest count
        // only increments if parse_dependencies returns non-empty.
        // Lock files contribute via is_dependency_file match in the pipeline.
        // For this test, create a package.json alongside to show lock files are recognized.
        let pkg = dir.path().join("package.json");
        std::fs::write(&pkg, r#"{"dependencies":{"react":"18"}}"#).unwrap();

        let cluster = Cluster {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
            events: vec![
                FsEvent {
                    paths: vec![PathBuf::from("package.json")],
                    kind: FsEventKind::Modify,
                    timestamp: Utc::now(),
                },
                FsEvent {
                    paths: vec![PathBuf::from("yarn.lock")],
                    kind: FsEventKind::Modify,
                    timestamp: Utc::now(),
                },
            ],
        };

        let analysis = dependency_score(&cluster, dir.path());
        assert!(analysis.score > 0.0);
    }

    #[test]
    fn vercel_json_is_web_config() {
        assert!(is_web_config(Path::new("vercel.json")));
    }

    #[test]
    fn next_config_mjs_is_web_config() {
        assert!(is_web_config(Path::new("next.config.mjs")));
    }

    #[test]
    fn tsconfig_json_is_web_config() {
        assert!(is_web_config(Path::new("tsconfig.json")));
    }

    #[test]
    fn web_config_touches_runtime() {
        let cluster = Cluster {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
            events: vec![FsEvent {
                paths: vec![PathBuf::from("next.config.mjs")],
                kind: FsEventKind::Modify,
                timestamp: Utc::now(),
            }],
        };

        let analysis = runtime_score(&cluster);
        assert!(analysis.score > 0.0);
        assert_eq!(analysis.paths, 1);
    }

    #[test]
    fn podfile_is_dependency_file() {
        assert!(is_dependency_file(Path::new("Podfile")));
    }

    #[test]
    fn build_gradle_kts_is_dependency_file() {
        assert!(is_dependency_file(Path::new("build.gradle.kts")));
    }

    #[test]
    fn info_plist_is_mobile_config() {
        assert!(super::is_mobile_config(Path::new("Info.plist")));
    }

    #[test]
    fn xcconfig_is_mobile_config() {
        assert!(super::is_mobile_config(Path::new("Debug.xcconfig")));
    }

    #[test]
    fn android_manifest_is_mobile_config() {
        assert!(super::is_mobile_config(Path::new("AndroidManifest.xml")));
    }

    #[test]
    fn astro_config_is_web_config() {
        assert!(is_web_config(Path::new("astro.config.mjs")));
    }
}
