use crate::supervisor::config::PadagoniaConfig;
use crate::supervisor::model::{
    project_id_for_path, DesiredState, ProjectPlan, ProjectSpec, CONTROL_SCHEMA_VERSION,
};
use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

const PROJECT_LABEL: &str = "KaptaindProjectControl";
const OBSERVATION_LABEL: &str = "KaptaindWorkerObservation";

#[derive(Debug, Clone)]
pub struct PadagoniaClient {
    endpoint: Url,
    namespace: String,
    token: String,
    client: Client,
}

impl PadagoniaClient {
    pub fn from_config(config: &PadagoniaConfig) -> Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }
        let endpoint = validate_endpoint(&config.endpoint)?;
        let token = std::env::var(&config.token_env).with_context(|| {
            format!(
                "Padagonia token environment variable {} is not set",
                config.token_env
            )
        })?;
        anyhow::ensure!(token.len() >= 16, "Padagonia bearer token is too short");
        anyhow::ensure!(
            !config.namespace.trim().is_empty(),
            "Padagonia namespace is empty"
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("failed to build Padagonia HTTP client")?;
        Ok(Some(Self {
            endpoint,
            namespace: config.namespace.clone(),
            token,
            client,
        }))
    }

    pub async fn health(&self) -> Result<()> {
        let response = self
            .client
            .get(self.url("ready")?)
            .send()
            .await
            .context("Padagonia readiness request failed")?;
        anyhow::ensure!(response.status().is_success(), "Padagonia is not ready");
        Ok(())
    }

    pub async fn project_desired_state(&self, project: &ProjectSpec) -> Result<()> {
        let external_id = format!(
            "kaptaind-project:{}:{}",
            project.project_id, project.revision
        );
        let properties = json!({
            "schema_version": CONTROL_SCHEMA_VERSION,
            "project_id": project.project_id,
            "path": project.path,
            "config": project.config,
            "desired_state": match project.desired_state {
                DesiredState::Enabled => "enabled",
                DesiredState::Disabled => "disabled",
            },
            "health_port": project.health_port,
            "revision": project.revision,
            "updated_at_ms": project.updated_at_ms,
            "source": project.source,
        });
        self.create_node(PROJECT_LABEL, &external_id, properties)
            .await
    }

    pub async fn project_observation(
        &self,
        reconcile_id: &str,
        plan: &ProjectPlan,
        success: bool,
        detail: &str,
    ) -> Result<()> {
        let external_id = format!(
            "kaptaind-observation:{}:{}",
            plan.project_id,
            uuid::Uuid::new_v4()
        );
        let properties = json!({
            "schema_version": CONTROL_SCHEMA_VERSION,
            "project_id": plan.project_id,
            "path": plan.path,
            "reconcile_id": reconcile_id,
            "desired_state": plan.desired_state,
            "observed_state": plan.observation,
            "action": plan.action,
            "success": success,
            "detail": detail,
            "observed_at_ms": Utc::now().timestamp_millis(),
        });
        self.create_node(OBSERVATION_LABEL, &external_id, properties)
            .await
    }

    pub async fn fetch_projects(&self) -> Result<Vec<ProjectSpec>> {
        let mut cursor: Option<String> = None;
        let mut projects: BTreeMap<String, ProjectSpec> = BTreeMap::new();
        for _ in 0..10_000usize {
            let body = json!({
                "namespace": self.namespace,
                "limit": 1000,
                "cursor": cursor,
            });
            let response = self
                .authorized(self.client.post(self.url("api/v1/query/nodes")?))
                .json(&body)
                .send()
                .await
                .context("Padagonia project query failed")?;
            let page = decode_response::<NodePage>(response).await?;
            for node in page.nodes {
                if node.label != PROJECT_LABEL {
                    continue;
                }
                if let Some(project) = parse_project(node.properties) {
                    let replace = projects.get(&project.project_id).is_none_or(|current| {
                        project.revision > current.revision
                            || (project.revision == current.revision
                                && project.updated_at_ms > current.updated_at_ms)
                    });
                    if replace {
                        projects.insert(project.project_id.clone(), project);
                    }
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(projects.into_values().collect());
            }
        }
        anyhow::bail!("Padagonia pagination exceeded safety limit")
    }

    async fn create_node(&self, label: &str, external_id: &str, properties: Value) -> Result<()> {
        let body = json!({
            "namespace": self.namespace,
            "external_id": external_id,
            "idempotency_key": external_id,
            "label": label,
            "properties": properties,
            "provenance": {
                "agent": "kaptaind-supervisor",
                "model": env!("CARGO_PKG_VERSION"),
                "confidence": 1.0,
                "cost": 0.0,
                "evidence": [CONTROL_SCHEMA_VERSION],
            }
        });
        let response = self
            .authorized(self.client.post(self.url("api/v1/nodes")?))
            .json(&body)
            .send()
            .await
            .context("Padagonia projection request failed")?;
        if response.status() == StatusCode::BAD_REQUEST {
            let text = response.text().await.unwrap_or_default();
            if text.contains("already exists") || text.contains("idempotency") {
                return Ok(());
            }
            anyhow::bail!("Padagonia rejected control record: {}", bounded(&text));
        }
        anyhow::ensure!(
            response.status().is_success(),
            "Padagonia projection failed with status {}",
            response.status()
        );
        Ok(())
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .bearer_auth(&self.token)
            .header("x-padagonia-namespace", &self.namespace)
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.endpoint
            .join(path)
            .with_context(|| format!("invalid Padagonia route {path}"))
    }
}

pub fn validate_endpoint(raw: &str) -> Result<Url> {
    let mut endpoint = Url::parse(raw).context("invalid Padagonia endpoint")?;
    anyhow::ensure!(
        endpoint.username().is_empty() && endpoint.password().is_none(),
        "Padagonia endpoint must not contain credentials"
    );
    anyhow::ensure!(
        endpoint.query().is_none() && endpoint.fragment().is_none(),
        "Padagonia endpoint must not contain query or fragment"
    );
    let host = endpoint
        .host_str()
        .context("Padagonia endpoint has no host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    match endpoint.scheme() {
        "https" => {}
        "http" if loopback => {}
        "http" => anyhow::bail!("remote Padagonia endpoints require HTTPS"),
        scheme => anyhow::bail!("unsupported Padagonia endpoint scheme: {scheme}"),
    }
    if !endpoint.path().ends_with('/') {
        let path = format!("{}/", endpoint.path());
        endpoint.set_path(&path);
    }
    Ok(endpoint)
}

#[derive(Debug, Deserialize)]
struct NodePage {
    nodes: Vec<NodeRecord>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NodeRecord {
    label: String,
    properties: Value,
}

fn parse_project(properties: Value) -> Option<ProjectSpec> {
    if properties.get("schema_version")?.as_str()? != CONTROL_SCHEMA_VERSION {
        return None;
    }
    let path = PathBuf::from(properties.get("path")?.as_str()?);
    let project_id = properties
        .get("project_id")
        .and_then(Value::as_str)
        .map_or_else(|| project_id_for_path(&path), str::to_string);
    let config = PathBuf::from(properties.get("config")?.as_str()?);
    let desired_state = match properties.get("desired_state")?.as_str()? {
        "enabled" => DesiredState::Enabled,
        "disabled" => DesiredState::Disabled,
        _ => return None,
    };
    let health_port = u16::try_from(properties.get("health_port")?.as_u64()?).ok()?;
    let project = ProjectSpec {
        project_id,
        path,
        config,
        desired_state,
        health_port,
        revision: properties.get("revision")?.as_u64()?,
        updated_at_ms: properties.get("updated_at_ms")?.as_i64()?,
        source: properties.get("source")?.as_str()?.to_string(),
    };
    project.validate().ok()?;
    Some(project)
}

async fn decode_response<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .context("failed to read Padagonia response")?;
    anyhow::ensure!(
        status.is_success(),
        "Padagonia request failed with status {status}: {}",
        bounded(&String::from_utf8_lossy(&bytes))
    );
    serde_json::from_slice(&bytes).context("invalid Padagonia response")
}

fn bounded(value: &str) -> String {
    value.chars().take(512).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeState {
        writes: Arc<Mutex<Vec<Value>>>,
    }

    async fn fake_query(headers: HeaderMap) -> Json<Value> {
        assert_eq!(
            headers
                .get("x-padagonia-namespace")
                .and_then(|value| value.to_str().ok()),
            Some("kaptaind")
        );
        let path = PathBuf::from("/repo");
        Json(json!({
            "api_version": "v1",
            "node_ids": [1],
            "nodes": [{
                "id": 1,
                "label": PROJECT_LABEL,
                "properties": {
                    "schema_version": CONTROL_SCHEMA_VERSION,
                    "project_id": project_id_for_path(&path),
                    "path": "/repo",
                    "config": "/repo/kaptaind.toml",
                    "desired_state": "enabled",
                    "health_port": 3000,
                    "revision": 4,
                    "updated_at_ms": 10,
                    "source": "operator"
                },
                "provenance": {"agent":"test","model":"test","confidence":1.0,"cost":0.0,"timestamp":0,"evidence":[]}
            }],
            "next_cursor": null
        }))
    }

    async fn fake_create(
        State(state): State<FakeState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        assert!(headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == "Bearer 0123456789abcdef"));
        state.writes.lock().unwrap().push(body);
        (StatusCode::CREATED, Json(json!({"id": 2})))
    }

    #[test]
    fn endpoint_policy_allows_loopback_http_and_remote_https() {
        assert!(validate_endpoint("http://127.0.0.1:7373").is_ok());
        assert!(validate_endpoint("http://localhost:7373").is_ok());
        assert!(validate_endpoint("https://padagonia.example.test").is_ok());
    }

    #[test]
    fn endpoint_policy_rejects_remote_plaintext_and_credentials() {
        assert!(validate_endpoint("http://padagonia.example.test").is_err());
        assert!(validate_endpoint("https://token@padagonia.example.test").is_err());
    }

    #[test]
    fn parses_versioned_project_control_record() {
        let value = json!({
            "schema_version": CONTROL_SCHEMA_VERSION,
            "project_id": project_id_for_path(std::path::Path::new("/repo")),
            "path": "/repo",
            "config": "/repo/kaptaind.toml",
            "desired_state": "enabled",
            "health_port": 3000,
            "revision": 1,
            "updated_at_ms": 1,
            "source": "test"
        });
        assert_eq!(parse_project(value).unwrap().health_port, 3000);
    }

    #[tokio::test]
    async fn padagonia_wire_contract_recovers_and_projects_control_records() {
        let state = FakeState::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/api/v1/query/nodes", post(fake_query))
            .route("/api/v1/nodes", post(fake_create))
            .with_state(state.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let client = PadagoniaClient {
            endpoint: validate_endpoint(&format!("http://{address}")).unwrap(),
            namespace: "kaptaind".to_string(),
            token: "0123456789abcdef".to_string(),
            client: Client::builder().build().unwrap(),
        };
        let projects = client.fetch_projects().await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].revision, 4);
        client.project_desired_state(&projects[0]).await.unwrap();
        let writes = state.writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0]["label"], PROJECT_LABEL);
        assert!(writes[0].to_string().find("0123456789abcdef").is_none());
        server.abort();
    }
}
