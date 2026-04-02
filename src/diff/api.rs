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
            | "poetry.lock"
            | "requirements.txt"
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
    use super::{dependency_score, parse_cargo_dependencies, parse_package_dependencies, parse_requirements};
    use crate::cluster::engine::Cluster;
    use crate::watcher::{FsEvent, FsEventKind};
    use chrono::Utc;
    use std::path::PathBuf;
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
        std::fs::write(&path, "[dependencies]\nserde = \"1\"\nanyhow = \"1\"\n").expect("write cargo manifest");

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
}
