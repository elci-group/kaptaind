use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::{from_fn, Next},
    response::{sse::Event as SseEvent, Html, IntoResponse, Json, Response, Sse},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path as StdPath, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::daemon::health::{DaemonEvent, Metrics};
use crate::daemon::scheduler::AnalysisArtifact;
use crate::daemon::telemetry::TokenMetrics;

const WEB_UI_HTML: &str = include_str!("web_ui.html");

#[derive(Clone)]
pub struct WebState {
    pub repo_path: PathBuf,
    pub metrics: Arc<Metrics>,
    pub event_tx: broadcast::Sender<DaemonEvent>,
    pub version: String,
}

pub async fn start_web_server(port: u16, state: WebState) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "web UI server listening");

    let app = routes().layer(from_fn(cors_middleware)).with_state(state);
    axum::serve(listener, app).await?;
    Ok(())
}

fn routes() -> Router<WebState> {
    Router::new()
        .route("/", get(index_handler))
        .route("/api", get(api_handler))
        .route("/api/status", get(status_handler))
        .route("/api/telemetry", get(telemetry_handler))
        .route("/api/usage", get(usage_handler))
        .route("/api/commits", get(commits_handler))
        .route("/api/commits/:id", get(commit_detail_handler))
        .route(
            "/api/config",
            get(config_handler).post(config_update_handler),
        )
        .route("/api/metrics", get(metrics_handler))
        .route("/api/events", get(events_handler))
        .route("/api/version", get(version_handler))
        .route("/api/graph/dependencies", get(dependency_graph_handler))
        .route("/api/graph/commits", get(commit_graph_handler))
}

async fn cors_middleware(request: Request<Body>, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        return cors_headers().into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    append_cors(headers);
    response
}

fn cors_headers() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Content-Type"),
            ),
        ],
    )
}

fn append_cors(headers: &mut axum::http::HeaderMap) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type"),
    );
}

async fn index_handler() -> Html<&'static str> {
    Html(WEB_UI_HTML)
}

async fn api_handler() -> Json<serde_json::Value> {
    Json(json!({
        "openapi": "3.0.0",
        "info": { "title": "Kaptaind WebUI API", "version": env!("CARGO_PKG_VERSION") },
        "endpoints": [
            { "method": "GET", "path": "/", "description": "Single-page WebUI" },
            { "method": "GET", "path": "/api", "description": "This OpenAPI-style listing" },
            { "method": "GET", "path": "/api/status", "description": "Current daemon status" },
            { "method": "GET", "path": "/api/telemetry", "description": "Full token/cost telemetry" },
            { "method": "GET", "path": "/api/usage", "description": "Per-provider and per-model usage" },
            { "method": "GET", "path": "/api/commits?limit=N", "description": "Commit history summary" },
            { "method": "GET", "path": "/api/commits/:id", "description": "Single analysis artifact" },
            { "method": "GET", "path": "/api/config", "description": "Current raw and parsed config" },
            { "method": "POST", "path": "/api/config", "description": "Validate and save config TOML" },
            { "method": "GET", "path": "/api/metrics", "description": "Daemon metrics counters" },
            { "method": "GET", "path": "/api/events", "description": "SSE stream of daemon events" },
            { "method": "GET", "path": "/api/version", "description": "Repository VERSION and daemon version" },
            { "method": "GET", "path": "/api/graph/dependencies", "description": "3D dependency graph data" },
            { "method": "GET", "path": "/api/graph/commits", "description": "3D commit graph data" },
        ]
    }))
}

async fn status_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    let path = state.repo_path.join(".kaptaind").join("status.json");
    let default = json!({
        "status": "Idle",
        "last_version": null,
        "last_action_time": chrono::Utc::now().to_rfc3339(),
        "last_error": null,
    });
    let value: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or(default);
    Json(value)
}

async fn telemetry_handler(State(state): State<WebState>) -> Json<TokenMetrics> {
    Json(load_telemetry(&state.repo_path))
}

async fn usage_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    let metrics = load_telemetry(&state.repo_path);
    Json(json!({
        "per_provider": metrics.per_provider,
        "per_model": metrics.per_model,
    }))
}

#[derive(Debug, Deserialize, Default)]
struct CommitsQuery {
    #[serde(default = "default_commit_limit")]
    limit: usize,
}

fn default_commit_limit() -> usize {
    50
}

async fn commits_handler(
    State(state): State<WebState>,
    Query(query): Query<CommitsQuery>,
) -> Json<Vec<serde_json::Value>> {
    let artifacts = list_analysis_artifacts(&state.repo_path, query.limit);
    Json(artifacts)
}

async fn commit_detail_handler(
    State(state): State<WebState>,
    Path(id): Path<String>,
) -> Result<Json<AnalysisArtifact>, StatusCode> {
    let path = state
        .repo_path
        .join(".kaptaind")
        .join("analysis")
        .join(format!("{id}.json"));
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn config_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    let path = state.repo_path.join("kaptaind.toml");
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let parsed: serde_json::Value = toml::from_str(&raw)
        .map(|v: toml::Value| serde_json::to_value(v).unwrap_or_default())
        .unwrap_or_default();
    Json(json!({ "raw": raw, "parsed": parsed }))
}

async fn config_update_handler(
    State(state): State<WebState>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Ensure the content is valid TOML.
    if let Err(err) = body.parse::<toml_edit::DocumentMut>() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid TOML: {err}") })),
        ));
    }

    let path = state.repo_path.join("kaptaind.toml");
    let original = std::fs::read_to_string(&path).ok();

    if let Err(err) = std::fs::write(&path, &body) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        ));
    }

    // Validate by loading the updated config; restore the previous content on failure.
    if let Err(err) = crate::config::loader::load_from_path(&path) {
        if let Some(orig) = original {
            let _ = std::fs::write(&path, orig);
        } else {
            let _ = std::fs::remove_file(&path);
        }
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("config validation failed: {err}") })),
        ));
    }

    Ok(Json(json!({ "saved": true })))
}

async fn metrics_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    Json(json!({
        "clusters_processed": state.metrics.clusters_processed.load(Ordering::Relaxed),
        "commits_made": state.metrics.commits_made.load(Ordering::Relaxed),
        "artifacts_pruned": state.metrics.artifacts_pruned.load(Ordering::Relaxed),
        "test_hook_failures": state.metrics.test_hook_failures.load(Ordering::Relaxed),
        "storage_cleaned_bytes": state.metrics.storage_cleaned_bytes.load(Ordering::Relaxed),
        "storage_cleaned_files": state.metrics.storage_cleaned_files.load(Ordering::Relaxed),
        "shark_leadership_acquired": state.metrics.shark_leadership_acquired.load(Ordering::Relaxed),
        "shark_leadership_lost": state.metrics.shark_leadership_lost.load(Ordering::Relaxed),
    }))
}

async fn events_handler(
    State(state): State<WebState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        let event: DaemonEvent = result.ok()?;
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok(SseEvent::default().data(json)))
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn version_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    let version_path = state.repo_path.join("VERSION");
    let version = std::fs::read_to_string(&version_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    Json(json!({
        "version": version,
        "daemon_version": state.version,
    }))
}

async fn dependency_graph_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    let nodes_edges = build_dependency_graph(&state.repo_path);
    Json(serde_json::to_value(nodes_edges).unwrap_or_default())
}

async fn commit_graph_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    let nodes_edges = build_commit_graph(&state.repo_path);
    Json(serde_json::to_value(nodes_edges).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_telemetry(repo_path: &StdPath) -> TokenMetrics {
    let path = repo_path.join(".kaptaind").join("telemetry.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn list_analysis_artifacts(repo_path: &StdPath, limit: usize) -> Vec<serde_json::Value> {
    let dir = repo_path.join(".kaptaind").join("analysis");
    let mut entries: Vec<(std::time::SystemTime, serde_json::Value)> = std::fs::read_dir(&dir)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
                .filter_map(|e| {
                    let meta = e.metadata().ok()?;
                    let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                    let content = std::fs::read_to_string(e.path()).ok()?;
                    let artifact: AnalysisArtifact = serde_json::from_str(&content).ok()?;
                    let summary = artifact_summary(&artifact);
                    Some((modified, summary))
                })
                .collect()
        })
        .unwrap_or_default();

    entries.sort_by_key(|b| std::cmp::Reverse(b.0));
    entries.into_iter().take(limit).map(|(_, v)| v).collect()
}

fn artifact_summary(artifact: &AnalysisArtifact) -> serde_json::Value {
    json!({
        "id": artifact.cluster_id,
        "cluster_id": artifact.cluster_id,
        "version": artifact.version,
        "bump": artifact.bump,
        "score": artifact.weight.score,
        "timestamp": artifact.ended_at.to_rfc3339(),
        "started_at": artifact.started_at.to_rfc3339(),
        "ended_at": artifact.ended_at.to_rfc3339(),
        "paths": artifact.diff.touched_paths,
        "message": format!("{} -> v{} [score={:.2}; paths={}; api={}; deps={}; runtime={}]",
            artifact.bump, artifact.version, artifact.weight.score,
            artifact.diff.touched_paths, artifact.diff.api_touches,
            artifact.diff.dependency_nodes, artifact.diff.runtime_paths),
        "diff": artifact.diff,
        "weight": artifact.weight,
    })
}

#[derive(Debug, Serialize, Default)]
struct GraphData {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Debug, Serialize)]
struct GraphNode {
    id: usize,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct GraphEdge {
    source: usize,
    target: usize,
}

fn build_dependency_graph(repo_path: &StdPath) -> GraphData {
    let cargo_lock = repo_path.join("Cargo.lock");
    if cargo_lock.exists() {
        if let Ok(content) = std::fs::read_to_string(&cargo_lock) {
            if let Ok(lock) = content.parse::<toml::Value>() {
                return parse_cargo_lock(&lock);
            }
        }
    }

    let package_json = repo_path.join("package.json");
    if package_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&package_json) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                return parse_package_json(&pkg);
            }
        }
    }

    GraphData::default()
}

fn parse_cargo_lock(lock: &toml::Value) -> GraphData {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut id_by_name: HashMap<String, usize> = HashMap::new();

    if let Some(packages) = lock.get("package").and_then(|p| p.as_array()) {
        for (idx, pkg) in packages.iter().enumerate() {
            let name = pkg
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let version = pkg
                .get("version")
                .and_then(|v| v.as_str())
                .map(String::from);
            id_by_name.insert(name.clone(), idx);
            nodes.push(GraphNode {
                id: idx,
                label: name,
                version,
            });
        }

        for (idx, pkg) in packages.iter().enumerate() {
            if let Some(deps) = pkg.get("dependencies").and_then(|d| d.as_array()) {
                for dep in deps {
                    let dep_name = dep
                        .as_str()
                        .map(String::from)
                        .or_else(|| dep.get("name").and_then(|n| n.as_str()).map(String::from))
                        .unwrap_or_default();
                    if let Some(&target) = id_by_name.get(&dep_name) {
                        edges.push(GraphEdge {
                            source: idx,
                            target,
                        });
                    }
                }
            }
        }
    }

    GraphData { nodes, edges }
}

fn parse_package_json(pkg: &serde_json::Value) -> GraphData {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut id_by_name: HashMap<String, usize> = HashMap::new();

    let root_name = pkg
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("root")
        .to_string();
    id_by_name.insert(root_name.clone(), 0);
    nodes.push(GraphNode {
        id: 0,
        label: root_name,
        version: pkg
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
    });

    let mut next_id = 1;
    let sections = ["dependencies", "devDependencies", "peerDependencies"];
    for section in sections {
        if let Some(map) = pkg.get(section).and_then(|m| m.as_object()) {
            for (name, _version) in map {
                if !id_by_name.contains_key(name) {
                    id_by_name.insert(name.clone(), next_id);
                    nodes.push(GraphNode {
                        id: next_id,
                        label: name.clone(),
                        version: None,
                    });
                    next_id += 1;
                }
                edges.push(GraphEdge {
                    source: 0,
                    target: id_by_name[name],
                });
            }
        }
    }

    GraphData { nodes, edges }
}

fn build_commit_graph(repo_path: &StdPath) -> GraphData {
    let output = std::process::Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap_or("."),
            "log",
            "--pretty=format:%H|%P|%s|%ct",
            "-n",
            "100",
        ])
        .output();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut id_by_hash: HashMap<String, usize> = HashMap::new();

    let lines: Vec<String> = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect(),
        _ => return GraphData::default(),
    };

    for (idx, line) in lines.iter().enumerate() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 4 {
            continue;
        }
        let hash = parts[0].to_string();
        id_by_hash.insert(hash.clone(), idx);
        nodes.push(GraphNode {
            id: idx,
            label: format!("{:.7}", hash),
            version: Some(parts[2].to_string()),
        });
    }

    for (idx, line) in lines.iter().enumerate() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 4 {
            continue;
        }
        for parent in parts[1].split_whitespace() {
            if let Some(&target) = id_by_hash.get(parent) {
                edges.push(GraphEdge {
                    source: idx,
                    target,
                });
            }
        }
    }

    GraphData { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_state(repo_path: PathBuf) -> WebState {
        let (tx, _rx) = broadcast::channel(8);
        WebState {
            repo_path,
            metrics: Arc::new(Metrics::default()),
            event_tx: tx,
            version: "9.6.6".to_string(),
        }
    }

    #[tokio::test]
    async fn index_handler_serves_html() {
        let response = index_handler().await;
        assert!(response.0.contains("Kaptaind WebUI"));
    }

    #[tokio::test]
    async fn api_handler_lists_endpoints() {
        let Json(body) = api_handler().await;
        assert_eq!(body["openapi"], "3.0.0");
        let endpoints = body["endpoints"].as_array().unwrap();
        assert!(endpoints
            .iter()
            .any(|e| e["path"].as_str().unwrap().starts_with("/api/commits")));
    }

    #[tokio::test]
    async fn status_handler_returns_default_when_missing() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        let Json(body) = status_handler(State(state)).await;
        assert_eq!(body["status"], "Idle");
    }

    #[tokio::test]
    async fn telemetry_handler_returns_defaults_when_missing() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        let Json(metrics) = telemetry_handler(State(state)).await;
        assert_eq!(metrics.input_tokens, 0);
        assert!(metrics.per_provider.is_empty());
    }

    #[tokio::test]
    async fn usage_handler_returns_empty_maps() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        let Json(body) = usage_handler(State(state)).await;
        assert!(body["per_provider"].as_object().unwrap().is_empty());
        assert!(body["per_model"].as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn metrics_handler_returns_counters() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        state.metrics.clusters_processed.store(3, Ordering::Relaxed);
        let Json(body) = metrics_handler(State(state)).await;
        assert_eq!(body["clusters_processed"], 3);
    }

    #[tokio::test]
    async fn version_handler_reads_version_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("VERSION"), "1.2.3").unwrap();
        let state = test_state(dir.path().to_path_buf());
        let Json(body) = version_handler(State(state)).await;
        assert_eq!(body["version"], "1.2.3");
        assert_eq!(body["daemon_version"], "9.6.6");
    }

    #[tokio::test]
    async fn config_handler_reads_toml() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("kaptaind.toml"), "repo_path = \"./\"\n").unwrap();
        let state = test_state(dir.path().to_path_buf());
        let Json(body) = config_handler(State(state)).await;
        assert!(body["raw"].as_str().unwrap().contains("repo_path"));
        assert_eq!(body["parsed"]["repo_path"], "./");
    }

    #[tokio::test]
    async fn config_update_rejects_invalid_toml() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        let result = config_update_handler(State(state), "not valid toml [[".to_string()).await;
        assert!(result.is_err());
        let (status, Json(body)) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("invalid TOML"));
    }

    #[tokio::test]
    async fn config_update_writes_valid_toml() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        let toml = r#"repo_path = "./"

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5

[weights]
s = 0.35
a = 0.3
d = 0.2
r = 0.15
b = 0.0

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10

[test]
command = "cargo test"
required = true
"#;
        let result = config_update_handler(State(state), toml.to_string()).await;
        assert!(result.is_ok(), "{:?}", result);
        let written = std::fs::read_to_string(dir.path().join("kaptaind.toml")).unwrap();
        assert!(written.contains("repo_path"));
    }

    #[tokio::test]
    async fn commit_detail_returns_404_when_missing() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        let result = commit_detail_handler(State(state), Path("no-such-id".to_string())).await;
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn dependency_graph_parses_package_json() {
        let pkg = serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "dependencies": { "axum": "0.7", "tokio": "1" }
        });
        let graph = parse_package_json(&pkg);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn dependency_graph_parses_cargo_lock() {
        let lock = "[[package]]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"dep1\"\nversion = \"1.0.0\"\n\n[[package]]\nname = \"dep2\"\nversion = \"2.0.0\"\ndependencies = [\"dep1\"]\n"
            .parse::<toml::Value>()
            .unwrap();
        let graph = parse_cargo_lock(&lock);
        assert!(graph.nodes.len() >= 2);
        assert!(!graph.edges.is_empty());
    }

    #[test]
    fn telemetry_usage_aggregation_tracks_provider_and_model() {
        let dir = tempdir().unwrap();
        crate::daemon::telemetry::track_cost(dir.path(), "openai", "gpt-4o", 1000, 500);
        crate::daemon::telemetry::track_cost(dir.path(), "openai", "gpt-4o", 2000, 1000);
        crate::daemon::telemetry::track_cost(dir.path(), "anthropic", "claude", 500, 250);
        let metrics = load_telemetry(dir.path());
        assert_eq!(metrics.per_provider["openai"].requests, 2);
        assert_eq!(metrics.per_provider["anthropic"].requests, 1);
        assert_eq!(metrics.per_model["gpt-4o"].input_tokens, 3000);
        assert_eq!(metrics.per_model["claude"].output_tokens, 250);
    }
}
