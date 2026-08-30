use anyhow::{anyhow, Result};
use kaptaind::monitor::{save_registry_at, MonitorEntry, MonitorRegistry};
use kaptaind::supervisor::config::SupervisorConfig;
use kaptaind::supervisor::model::{ReconcileAction, WorkerObservation};
use kaptaind::supervisor::reconcile::{Supervisor, WorkerControl};
use kaptaind::supervisor::runtime::serve_api;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[derive(Debug)]
struct FakeWorkers {
    started: Mutex<Vec<PathBuf>>,
    fail_path: Option<PathBuf>,
}

impl WorkerControl for FakeWorkers {
    fn observe(&self, _project: &kaptaind::supervisor::model::ProjectSpec) -> WorkerObservation {
        WorkerObservation::Stopped
    }

    fn start(&self, project: &kaptaind::supervisor::model::ProjectSpec) -> Result<()> {
        self.started.lock().unwrap().push(project.path.clone());
        if self.fail_path.as_ref() == Some(&project.path) {
            Err(anyhow!("injected worker failure"))
        } else {
            Ok(())
        }
    }
}

fn entry(path: PathBuf, port: u16) -> MonitorEntry {
    MonitorEntry {
        config: path.join("kaptaind.toml"),
        path,
        enabled: true,
        health_port: port,
        last_active: None,
    }
}

#[tokio::test]
async fn legacy_import_and_reconciliation_preserve_worker_failure_isolation() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("a");
    let second = dir.path().join("b");
    for path in [&first, &second] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("kaptaind.toml"), "repo_path = \".\"\n").unwrap();
    }
    let legacy_path = dir.path().join("monitored.json");
    save_registry_at(
        &legacy_path,
        &MonitorRegistry {
            projects: vec![entry(first.clone(), 4100), entry(second.clone(), 4101)],
        },
    )
    .unwrap();

    let workers = Arc::new(FakeWorkers {
        started: Mutex::new(Vec::new()),
        fail_path: Some(first.clone()),
    });
    let config = SupervisorConfig {
        state_path: dir.path().join("supervisor-state.json"),
        legacy_registry_path: legacy_path,
        max_starts_per_cycle: 4,
        ..SupervisorConfig::default()
    };
    let mut supervisor = Supervisor::bootstrap(config, workers.clone())
        .await
        .unwrap();
    let report = supervisor.reconcile(false).await.unwrap();

    assert_eq!(supervisor.snapshot().projects.len(), 2);
    assert_eq!(workers.started.lock().unwrap().len(), 2);
    assert_eq!(
        report
            .outcomes
            .iter()
            .filter(|outcome| outcome.action == ReconcileAction::Start)
            .count(),
        2
    );
    assert_eq!(
        report
            .outcomes
            .iter()
            .filter(|outcome| outcome.success)
            .count(),
        1
    );
}

#[tokio::test]
async fn duplicate_health_ports_are_blocked_without_starting_workers() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("a");
    let second = dir.path().join("b");
    for path in [&first, &second] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("kaptaind.toml"), "repo_path = \".\"\n").unwrap();
    }
    let legacy_path = dir.path().join("monitored.json");
    save_registry_at(
        &legacy_path,
        &MonitorRegistry {
            projects: vec![entry(first, 4100), entry(second, 4100)],
        },
    )
    .unwrap();
    let workers = Arc::new(FakeWorkers {
        started: Mutex::new(Vec::new()),
        fail_path: None,
    });
    let config = SupervisorConfig {
        state_path: dir.path().join("supervisor-state.json"),
        legacy_registry_path: legacy_path,
        ..SupervisorConfig::default()
    };
    let mut supervisor = Supervisor::bootstrap(config, workers.clone())
        .await
        .unwrap();
    let report = supervisor.reconcile(false).await.unwrap();

    assert!(workers.started.lock().unwrap().is_empty());
    assert!(report
        .outcomes
        .iter()
        .all(|outcome| outcome.action == ReconcileAction::Blocked));
}

#[tokio::test]
async fn loopback_api_exposes_health_and_protects_reconciliation() {
    let dir = tempdir().unwrap();
    let workers = Arc::new(FakeWorkers {
        started: Mutex::new(Vec::new()),
        fail_path: None,
    });
    let config = SupervisorConfig {
        state_path: dir.path().join("supervisor-state.json"),
        legacy_registry_path: dir.path().join("missing-registry.json"),
        ..SupervisorConfig::default()
    };
    let supervisor = Supervisor::bootstrap(config, workers).await.unwrap();
    let shared = Arc::new(tokio::sync::Mutex::new(supervisor));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_api(
        listener,
        shared,
        Some("0123456789abcdef".to_string()),
        shutdown_rx,
    ));
    let client = reqwest::Client::new();
    let health = client
        .get(format!("http://{address}/health"))
        .send()
        .await
        .unwrap();
    assert!(health.status().is_success());
    let unauthorized = client
        .post(format!("http://{address}/api/v1/reconcile"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);
    let authorized = client
        .post(format!("http://{address}/api/v1/reconcile"))
        .bearer_auth("0123456789abcdef")
        .send()
        .await
        .unwrap();
    assert!(authorized.status().is_success());
    shutdown_tx.send(()).unwrap();
    server.await.unwrap().unwrap();
}
