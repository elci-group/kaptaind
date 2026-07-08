use axum::{
    extract::State,
    response::{
        sse::{Event as SseEvent, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::daemon::shark::SharkRuntime;

#[derive(Default)]
pub struct Metrics {
    pub clusters_processed: AtomicUsize,
    pub commits_made: AtomicUsize,
    pub artifacts_pruned: AtomicUsize,
    pub test_hook_failures: AtomicUsize,
    pub storage_cleaned_bytes: AtomicU64,
    pub storage_cleaned_files: AtomicU64,
    pub shark_leadership_acquired: AtomicUsize,
    pub shark_leadership_lost: AtomicUsize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DaemonEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Clone)]
pub struct HealthState {
    pub version: String,
    pub repo_path: PathBuf,
    pub metrics: Arc<Metrics>,
    pub event_tx: broadcast::Sender<DaemonEvent>,
    pub shark: Option<Arc<SharkRuntime>>,
}

pub async fn start_health_server(port: u16, state: HealthState) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "health server listening");

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/metrics/prometheus", get(prometheus_metrics_handler))
        .route("/events", get(events_handler))
        .with_state(state);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_handler(State(state): State<HealthState>) -> Json<serde_json::Value> {
    let shark_info = state.shark.as_ref().map(|shark| {
        let lease = shark.current_lease().ok().flatten();
        json!({
            "enabled": true,
            "role": shark.current_role().to_string(),
            "instance_id": shark.instance_id,
            "leader_id": lease.as_ref().map(|l| l.instance_id.clone()),
            "lease_renewed_at": lease.as_ref().map(|l| l.renewed_at.to_rfc3339()),
        })
    });

    Json(json!({
        "status": "ok",
        "version": state.version,
        "shark": shark_info,
    }))
}

async fn metrics_handler(State(state): State<HealthState>) -> Json<serde_json::Value> {
    Json(json!({
        "clusters_processed": state.metrics.clusters_processed.load(Ordering::Relaxed),
        "commits_made": state.metrics.commits_made.load(Ordering::Relaxed),
        "artifacts_pruned": state.metrics.artifacts_pruned.load(Ordering::Relaxed),
        "test_hook_failures": state.metrics.test_hook_failures.load(Ordering::Relaxed),
        "storage_cleaned_bytes": state.metrics.storage_cleaned_bytes.load(Ordering::Relaxed),
        "storage_cleaned_files": state.metrics.storage_cleaned_files.load(Ordering::Relaxed),
    }))
}

async fn prometheus_metrics_handler(State(state): State<HealthState>) -> Response {
    let body = render_prometheus_metrics(&state);
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

fn render_prometheus_metrics(state: &HealthState) -> String {
    let mut lines = Vec::new();

    let help_type =
        |name: &str, help: &str, kind: &str| format!("# HELP {name} {help}\n# TYPE {name} {kind}");

    lines.push(help_type(
        "kaptaind_clusters_processed_total",
        "Total number of event clusters processed",
        "counter",
    ));
    lines.push(format!(
        "kaptaind_clusters_processed_total {}",
        state.metrics.clusters_processed.load(Ordering::Relaxed)
    ));

    lines.push(help_type(
        "kaptaind_commits_made_total",
        "Total number of automated commits made",
        "counter",
    ));
    lines.push(format!(
        "kaptaind_commits_made_total {}",
        state.metrics.commits_made.load(Ordering::Relaxed)
    ));

    lines.push(help_type(
        "kaptaind_artifacts_pruned_total",
        "Total number of analysis artifacts pruned",
        "counter",
    ));
    lines.push(format!(
        "kaptaind_artifacts_pruned_total {}",
        state.metrics.artifacts_pruned.load(Ordering::Relaxed)
    ));

    lines.push(help_type(
        "kaptaind_test_hook_failures_total",
        "Total number of test hook failures",
        "counter",
    ));
    lines.push(format!(
        "kaptaind_test_hook_failures_total {}",
        state.metrics.test_hook_failures.load(Ordering::Relaxed)
    ));

    lines.push(help_type(
        "kaptaind_storage_cleaned_bytes_total",
        "Total bytes reclaimed by storage cleanup",
        "counter",
    ));
    lines.push(format!(
        "kaptaind_storage_cleaned_bytes_total {}",
        state.metrics.storage_cleaned_bytes.load(Ordering::Relaxed)
    ));

    lines.push(help_type(
        "kaptaind_storage_cleaned_files_total",
        "Total files removed by storage cleanup",
        "counter",
    ));
    lines.push(format!(
        "kaptaind_storage_cleaned_files_total {}",
        state.metrics.storage_cleaned_files.load(Ordering::Relaxed)
    ));

    // Dynamic metrics read from on-disk state.
    let stability = crate::stability::engine::load(&state.repo_path).unwrap_or_default();
    lines.push(help_type(
        "kaptaind_stability_score",
        "Current repository stability score",
        "gauge",
    ));
    lines.push(format!("kaptaind_stability_score {:.6}", stability.score));

    let release_index = crate::release::index::load_index(&state.repo_path);
    lines.push(help_type(
        "kaptaind_releases_total",
        "Total number of releases recorded",
        "counter",
    ));
    lines.push(format!(
        "kaptaind_releases_total {}",
        release_index.releases.len()
    ));

    let last_version = crate::daemon::scheduler::load_version(&state.repo_path.join("VERSION"))
        .map(|v| v.to_string())
        .unwrap_or_default();
    lines.push(help_type(
        "kaptaind_version_info",
        "Version information for the monitored repository",
        "gauge",
    ));
    lines.push(format!(
        "kaptaind_version_info{{version=\"{}\",daemon_version=\"{}\"}} 1",
        escape_prometheus_label(&last_version),
        escape_prometheus_label(&state.version)
    ));

    lines.join("\n") + "\n"
}

fn escape_prometheus_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

async fn events_handler(
    State(state): State<HealthState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        let event: DaemonEvent = result.ok()?;
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok(SseEvent::default().data(json)))
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_handler_returns_ok() {
        let (tx, _rx) = broadcast::channel(1);
        let state = HealthState {
            version: "1.0.0".to_string(),
            repo_path: std::env::temp_dir(),
            metrics: Arc::new(Metrics::default()),
            event_tx: tx,
            shark: None,
        };
        let Json(body) = health_handler(State(state)).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], "1.0.0");
    }

    #[tokio::test]
    async fn metrics_handler_returns_counters() {
        let (tx, _rx) = broadcast::channel(1);
        let metrics = Arc::new(Metrics::default());
        metrics.clusters_processed.store(5, Ordering::Relaxed);
        metrics.commits_made.store(3, Ordering::Relaxed);
        let state = HealthState {
            version: "1.0.0".to_string(),
            repo_path: std::env::temp_dir(),
            metrics,
            event_tx: tx,
            shark: None,
        };
        let Json(body) = metrics_handler(State(state)).await;
        assert_eq!(body["clusters_processed"], 5);
        assert_eq!(body["commits_made"], 3);
        assert_eq!(body["artifacts_pruned"], 0);
        assert_eq!(body["test_hook_failures"], 0);
        assert_eq!(body["storage_cleaned_bytes"], 0);
        assert_eq!(body["storage_cleaned_files"], 0);
    }

    #[tokio::test]
    async fn prometheus_metrics_handler_returns_text() {
        let (tx, _rx) = broadcast::channel(1);
        let metrics = Arc::new(Metrics::default());
        metrics.clusters_processed.store(7, Ordering::Relaxed);
        let state = HealthState {
            version: "1.0.0".to_string(),
            repo_path: std::env::temp_dir(),
            metrics,
            event_tx: tx,
            shark: None,
        };
        let response = prometheus_metrics_handler(State(state)).await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("# HELP kaptaind_clusters_processed_total"));
        assert!(text.contains("kaptaind_clusters_processed_total 7"));
        assert!(text.contains("kaptaind_stability_score"));
        assert!(text.contains("kaptaind_version_info"));
    }
}
