use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, Request, StatusCode},
    middleware::{from_fn_with_state, Next},
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
    pub auth_token: String,
    pub allow_config_write: bool,
}

// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn start_web_server(port: u16, state: WebState) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(component = module_path!(), %addr, "web UI server listening");

    let app = routes()
        .layer(from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state);
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

/// Authentication + CSRF middleware.
///
/// Every route except the static HTML shell (`GET /`) requires a valid bearer
/// token (header `Authorization: Bearer <token>`, or `?token=` for EventSource,
/// which cannot set headers). State-changing requests (POST) must additionally
/// present a loopback `Origin` when one is supplied, which a cross-origin page
/// cannot forge. No CORS headers are emitted, so cross-origin browser access is
/// blocked outright.
async fn auth_middleware(
    State(state): State<WebState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.uri().path() == "/" {
        return next.run(request).await;
    }

    let authorized = extract_token(&request)
        .as_deref()
        .map(|t| {
            crate::util::constant_time::constant_time_eq(t.as_bytes(), state.auth_token.as_bytes())
        })
        .unwrap_or(false);
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    if request.method() == axum::http::Method::POST {
        let origin_ok = request
            .headers()
            .get(header::ORIGIN)
            // traci: allow -- optional failure is represented by None and handled by the caller.
            .and_then(|v| v.to_str().ok())
            .map(origin_is_loopback)
            .unwrap_or(true);
        if !origin_ok {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "cross-origin request blocked" })),
            )
                .into_response();
        }
    }

    next.run(request).await
}

fn extract_token(request: &Request<Body>) -> Option<String> {
    if let Some(v) = request
        .headers()
        .get(header::AUTHORIZATION)
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .and_then(|v| v.to_str().ok())
    {
        if let Some(t) = v.strip_prefix("Bearer ") {
            return Some(t.to_string());
        }
    }
    // EventSource cannot set headers, so the SSE deep-link carries the token in
    // the query string. Generated tokens are hex and need no decoding.
    if let Some(q) = request.uri().query() {
        for pair in q.split('&') {
            if let Some(t) = pair.strip_prefix("token=") {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn origin_is_loopback(origin: &str) -> bool {
    let lower = origin.to_ascii_lowercase();
    for host in [
        "http://127.0.0.1",
        "http://localhost",
        "http://[::1]",
        "https://127.0.0.1",
        "https://localhost",
        "https://[::1]",
    ] {
        if let Some(rest) = lower.strip_prefix(host) {
            if rest.is_empty() || rest.starts_with(':') {
                return true;
            }
        }
    }
    false
}

/// Recursively redact secret-shaped keys in a JSON value before it leaves the
/// daemon. A key is treated as secret if its lowercase name contains
/// `secret`/`token`/`password`/`api_key`/`apikey`, ends with `_key`, or is `key`.
fn redact_value(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                let kl = k.to_ascii_lowercase();
                let secret = kl.contains("secret")
                    || kl.contains("token")
                    || kl.contains("password")
                    || kl.contains("api_key")
                    || kl.contains("apikey")
                    || kl.ends_with("_key")
                    || kl == "key";
                if secret {
                    *val = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_value(val);
                }
            }
        }
        serde_json::Value::Array(arr) => arr.iter_mut().for_each(redact_value),
        _ => {}
    }
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
            { "method": "GET", "path": "/api/config", "description": "Current (redacted) parsed config" },
            { "method": "POST", "path": "/api/config", "description": "Validate and save config TOML (requires [web].allow_config_write)" },
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
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        // traci: allow -- optional failure is represented by None and handled by the caller.
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
    // Reject path-traversal / unusual ids: only a conservative allowlist is
    // accepted, and the resolved path must stay inside the analysis dir.
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        tracing::error!(
            operation = "commit_detail_handler",
            source_line = line!(),
            "commit detail handler returned an error"
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    let base = state.repo_path.join(".kaptaind").join("analysis");
    let path = base.join(format!("{id}.json"));
    std::fs::read_to_string(&path)
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .and_then(|c| serde_json::from_str(&c).ok())
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn config_handler(State(state): State<WebState>) -> Json<serde_json::Value> {
    let path = state.repo_path.join("kaptaind.toml");
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut parsed: serde_json::Value = toml::from_str(&raw)
        .map(|v: toml::Value| serde_json::to_value(v).unwrap_or_default())
        .unwrap_or_default();
    redact_value(&mut parsed);
    // Raw TOML may contain secrets; never return it over the API.
    Json(json!({ "raw": "<redacted: read kaptaind.toml locally>", "parsed": parsed }))
}

async fn config_update_handler(
    State(state): State<WebState>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !state.allow_config_write {
        tracing::error!(
            operation = "config_update_handler",
            source_line = line!(),
            "config update handler returned an error"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                json!({ "error": "config writes disabled (set [web].allow_config_write = true to enable)" }),
            ),
        ));
    }

    // Ensure the content is valid TOML.
    if let Err(err) = body.parse::<toml_edit::DocumentMut>() {
        tracing::error!(
            ?err,
            operation = "config_update_handler",
            source_line = line!(),
            "config update handler returned an error"
        );
        tracing::error!(
            operation = "config_update_handler",
            source_line = line!(),
            "config update handler returned an error"
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid TOML: {err}") })),
        ));
    }

    let path = state.repo_path.join("kaptaind.toml");
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let original = std::fs::read_to_string(&path).ok();

    if let Err(err) = std::fs::write(&path, &body) {
        tracing::error!(
            ?err,
            operation = "config_update_handler",
            source_line = line!(),
            "config update handler returned an error"
        );
        tracing::error!(
            operation = "config_update_handler",
            source_line = line!(),
            "config update handler returned an error"
        );
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        ));
    }

    // Validate by loading the updated config; restore the previous content on failure.
    if let Err(err) = crate::config::loader::load_from_path(&path) {
        tracing::error!(
            ?err,
            operation = "config_update_handler",
            source_line = line!(),
            "config update handler returned an error"
        );
        if let Some(orig) = original {
            if let Err(error) = std::fs::write(&path, orig) {
                tracing::warn!(
                    ?error,
                    operation = "config_update_handler",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
        } else {
            if let Err(error) = std::fs::remove_file(&path) {
                tracing::warn!(
                    ?error,
                    operation = "config_update_handler",
                    source_line = line!(),
                    "best-effort operation failed"
                );
            }
        }
        tracing::error!(
            operation = "config_update_handler",
            source_line = line!(),
            "config update handler returned an error"
        );
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
        // traci: allow -- optional failure is represented by None and handled by the caller.
        let event: DaemonEvent = result.ok()?;
        // traci: allow -- optional failure is represented by None and handled by the caller.
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
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn list_analysis_artifacts(repo_path: &StdPath, limit: usize) -> Vec<serde_json::Value> {
    let dir = repo_path.join(".kaptaind").join("analysis");
    let mut entries: Vec<(std::time::SystemTime, serde_json::Value)> = std::fs::read_dir(&dir)
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        .map(|rd| {
            // traci: allow -- optional failure is represented by None and handled by the caller.
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
                .filter_map(|e| {
                    // traci: allow -- optional failure is represented by None and handled by the caller.
                    let meta = e.metadata().ok()?;
                    let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                    // traci: allow -- optional failure is represented by None and handled by the caller.
                    let content = std::fs::read_to_string(e.path()).ok()?;
                    // traci: allow -- optional failure is represented by None and handled by the caller.
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
                label: name.clone(),
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
        label: root_name.clone(),
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
            auth_token: "test-token".to_string(),
            allow_config_write: true,
        }
    }

    #[tokio::test]
    async fn index_handler_serves_html() {
        let response = index_handler().await;
        assert!(response.0.contains("Kaptaind WebUI"));
    }

    #[test]
    fn web_ui_consumes_launch_token_from_fragment() {
        assert!(WEB_UI_HTML.contains("location.hash.slice(1)"));
        assert!(WEB_UI_HTML.contains("history.replaceState"));
        assert!(!WEB_UI_HTML.contains("location.search).get('token')"));
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
    async fn config_handler_redacts_secrets() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("kaptaind.toml"),
            "repo_path = \"./\"\n[notify]\nwebhook_url = \"https://x\"\nsecret = \"TOPSECRET\"\n",
        )
        .unwrap();
        let state = test_state(dir.path().to_path_buf());
        let Json(body) = config_handler(State(state)).await;
        // Raw TOML is never returned.
        assert_ne!(body["raw"].as_str().unwrap(), "");
        assert!(!body["raw"].as_str().unwrap().contains("TOPSECRET"));
        // Secret-shaped key is redacted in parsed form.
        let serialized = serde_json::to_string(&body).unwrap();
        assert!(!serialized.contains("TOPSECRET"));
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
    async fn config_update_forbidden_when_disabled() {
        let dir = tempdir().unwrap();
        let mut state = test_state(dir.path().to_path_buf());
        state.allow_config_write = false;
        let result = config_update_handler(State(state), "repo_path = \"./\"\n".to_string()).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
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

    #[tokio::test]
    async fn commit_detail_rejects_traversal_ids() {
        let dir = tempdir().unwrap();
        let state = test_state(dir.path().to_path_buf());
        for bad in ["../etc/passwd", "..%2F..%2Fetc%2Ffoo", "a/b", "x y", ""] {
            let result = commit_detail_handler(State(state.clone()), Path(bad.to_string())).await;
            assert!(
                matches!(
                    result,
                    Err(StatusCode::BAD_REQUEST) | Err(StatusCode::NOT_FOUND)
                ),
                "id {bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn origin_check_accepts_loopback_only() {
        assert!(origin_is_loopback("http://127.0.0.1:8080"));
        assert!(origin_is_loopback("http://localhost:8080"));
        assert!(!origin_is_loopback("http://evil.example"));
        assert!(!origin_is_loopback("http://127.0.0.1.evil.example"));
    }

    #[test]
    fn redact_value_masks_secret_keys() {
        let mut v = json!({
            "webhook_url": "https://hooks.example/abc",
            "secret": "S",
            "nested": { "api_key": "K", "name": "keep" },
            "list": [{ "token": "T" }],
        });
        redact_value(&mut v);
        let s = serde_json::to_string(&v).unwrap();
        assert!(!s.contains("\"S\""));
        assert!(!s.contains("\"K\""));
        assert!(!s.contains("\"T\""));
        assert!(s.contains("keep"));
        assert!(s.contains("https://hooks.example/abc"));
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
