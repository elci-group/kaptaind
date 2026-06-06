use axum::{
    extract::State,
    response::sse::{Event as SseEvent, Sse},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Default)]
pub struct Metrics {
    pub clusters_processed: AtomicUsize,
    pub commits_made: AtomicUsize,
    pub artifacts_pruned: AtomicUsize,
    pub test_hook_failures: AtomicUsize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DaemonEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Clone)]
pub struct HealthState {
    pub version: String,
    pub metrics: Arc<Metrics>,
    pub event_tx: broadcast::Sender<DaemonEvent>,
}

pub async fn start_health_server(port: u16, state: HealthState) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "health server listening");

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/events", get(events_handler))
        .with_state(state);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_handler(State(state): State<HealthState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": state.version,
    }))
}

async fn metrics_handler(State(state): State<HealthState>) -> Json<serde_json::Value> {
    Json(json!({
        "clusters_processed": state.metrics.clusters_processed.load(Ordering::Relaxed),
        "commits_made": state.metrics.commits_made.load(Ordering::Relaxed),
        "artifacts_pruned": state.metrics.artifacts_pruned.load(Ordering::Relaxed),
        "test_hook_failures": state.metrics.test_hook_failures.load(Ordering::Relaxed),
    }))
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
            metrics: Arc::new(Metrics::default()),
            event_tx: tx,
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
            metrics,
            event_tx: tx,
        };
        let Json(body) = metrics_handler(State(state)).await;
        assert_eq!(body["clusters_processed"], 5);
        assert_eq!(body["commits_made"], 3);
        assert_eq!(body["artifacts_pruned"], 0);
        assert_eq!(body["test_hook_failures"], 0);
    }
}
