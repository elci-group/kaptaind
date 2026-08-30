use crate::supervisor::config::SupervisorConfig;
use crate::supervisor::reconcile::{OsWorkerControl, Supervisor};
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};
use tracing::Instrument;

pub type SharedSupervisor = Arc<Mutex<Supervisor>>;

#[derive(Clone)]
struct ApiState {
    supervisor: SharedSupervisor,
    mutation_token: Option<Arc<str>>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

pub fn router(supervisor: SharedSupervisor, mutation_token: Option<String>) -> Router {
    let state = ApiState {
        supervisor,
        mutation_token: mutation_token.map(Arc::from),
    };
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/status", get(status))
        .route("/api/v1/projects", get(projects))
        .route("/api/v1/reconcile", post(reconcile))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "kaptaind-supervisor",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn ready(State(state): State<ApiState>) -> (StatusCode, Json<serde_json::Value>) {
    let supervisor = state.supervisor.lock().await;
    if supervisor.ready() {
        (StatusCode::OK, Json(json!({ "status": "ready" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "reason": "required Padagonia projection is unavailable"
            })),
        )
    }
}

async fn status(State(state): State<ApiState>) -> Json<crate::supervisor::model::FleetStatus> {
    Json(state.supervisor.lock().await.status())
}

async fn projects(
    State(state): State<ApiState>,
) -> Json<Vec<crate::supervisor::model::ProjectSpec>> {
    let supervisor = state.supervisor.lock().await;
    Json(supervisor.snapshot().projects.values().cloned().collect())
}

async fn reconcile(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<crate::supervisor::model::ReconcileReport>, ApiError> {
    authorize_mutation(&headers, state.mutation_token.as_deref())?;
    let mut supervisor = state.supervisor.lock().await;
    supervisor
        .reconcile(false)
        .await
        .map(Json)
        .map_err(|error| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        })
}

fn authorize_mutation(headers: &HeaderMap, expected: Option<&str>) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let supplied = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    let matches = supplied.len() == expected.len()
        && supplied.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() == 1;
    if matches {
        Ok(())
    } else {
        Err(ApiError {
            status: StatusCode::UNAUTHORIZED,
            message: "valid supervisor bearer token required".to_string(),
        })
    }
}

pub async fn serve_api(
    listener: TcpListener,
    supervisor: SharedSupervisor,
    mutation_token: Option<String>,
    shutdown: oneshot::Receiver<()>,
) -> Result<()> {
    axum::serve(listener, router(supervisor, mutation_token))
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await
        .context("supervisor API server failed")
}

pub async fn run(config: SupervisorConfig) -> Result<()> {
    let token = match config.api_token_env.as_deref() {
        Some(name) => Some(std::env::var(name).with_context(|| {
            format!("supervisor API token environment variable {name} is unset")
        })?),
        None => None,
    };
    if let Some(value) = token.as_deref() {
        anyhow::ensure!(
            value.len() >= 16,
            "supervisor API bearer token is too short"
        );
    }
    let worker = Arc::new(OsWorkerControl::new(config.worker_binary.clone()));
    let mut supervisor = Supervisor::bootstrap(config.clone(), worker).await?;
    let initial = supervisor.reconcile(false).await?;
    tracing::info!(
        reconcile_id = %initial.reconcile_id,
        projects = initial.outcomes.len(),
        admitted_starts = initial.admitted_starts,
        "initial supervisor reconciliation completed"
    );

    let shared = Arc::new(Mutex::new(supervisor));
    let listener = TcpListener::bind(config.listen_addr)
        .await
        .with_context(|| format!("failed to bind supervisor API at {}", config.listen_addr))?;
    let (api_shutdown_tx, api_shutdown_rx) = oneshot::channel();
    let api_task = tokio::spawn(
        serve_api(listener, shared.clone(), token, api_shutdown_rx)
            .instrument(tracing::info_span!("supervisor_api")),
    );
    let mut interval = tokio::time::interval(Duration::from_secs(config.reconcile_interval_secs));
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let mut supervisor = shared.lock().await;
                if let Err(error) = supervisor.reconcile(false).await {
                    tracing::error!(error = %error, "supervisor reconciliation failed");
                }
            }
            signal = shutdown_signal() => {
                signal?;
                tracing::info!("supervisor shutdown requested");
                break;
            }
        }
    }
    let _ = api_shutdown_tx.send(());
    api_task
        .await
        .context("supervisor API task failed to join")??;
    Ok(())
}

async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate())
            .context("failed to install supervisor SIGTERM handler")?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.context("failed to install supervisor SIGINT handler"),
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .context("failed to install supervisor shutdown handler")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_auth_is_constant_shape_and_optional() {
        let headers = HeaderMap::new();
        assert!(authorize_mutation(&headers, None).is_ok());
        assert!(authorize_mutation(&headers, Some("0123456789abcdef")).is_err());
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer 0123456789abcdef".parse().unwrap(),
        );
        assert!(authorize_mutation(&headers, Some("0123456789abcdef")).is_ok());
    }
}
