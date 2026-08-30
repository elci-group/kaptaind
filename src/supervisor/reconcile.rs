use crate::supervisor::config::SupervisorConfig;
use crate::supervisor::model::{
    DesiredState, FleetStatus, ProjectOutcome, ProjectPlan, ReconcileAction, ReconcilePlan,
    ReconcileReport, SupervisorSnapshot, WorkerObservation,
};
use crate::supervisor::padagonia::PadagoniaClient;
use crate::supervisor::store::AtomicSnapshotStore;
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

pub trait WorkerControl: Send + Sync {
    fn observe(&self, project: &crate::supervisor::model::ProjectSpec) -> WorkerObservation;
    fn start(&self, project: &crate::supervisor::model::ProjectSpec) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct OsWorkerControl {
    worker_binary: PathBuf,
}

impl OsWorkerControl {
    pub fn new(worker_binary: impl Into<PathBuf>) -> Self {
        Self {
            worker_binary: worker_binary.into(),
        }
    }

    fn resolved_worker_binary(&self) -> Result<PathBuf> {
        let resolved = if self.worker_binary.components().count() == 1 {
            std::env::var_os("PATH")
                .into_iter()
                .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
                .filter(|directory| directory.is_absolute())
                .map(|directory| directory.join(&self.worker_binary))
                .find(|candidate| candidate.is_file())
                .with_context(|| {
                    format!(
                        "worker binary {} was not found on the absolute PATH",
                        self.worker_binary.display()
                    )
                })?
        } else {
            self.worker_binary.canonicalize().with_context(|| {
                format!(
                    "worker binary does not exist: {}",
                    self.worker_binary.display()
                )
            })?
        };
        let metadata = fs::metadata(&resolved)
            .with_context(|| format!("worker binary does not exist: {}", resolved.display()))?;
        anyhow::ensure!(metadata.is_file(), "worker binary is not a regular file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            anyhow::ensure!(
                metadata.mode() & 0o6000 == 0,
                "worker binary must not have set-user-ID or set-group-ID bits"
            );
            anyhow::ensure!(
                metadata.mode() & 0o111 != 0,
                "worker binary is not executable"
            );
        }
        Ok(resolved)
    }
}

impl WorkerControl for OsWorkerControl {
    fn observe(&self, project: &crate::supervisor::model::ProjectSpec) -> WorkerObservation {
        if let Err(error) = validate_project_files(project) {
            return WorkerObservation::Invalid {
                reason: error.to_string(),
            };
        }
        observe_pid_file(&project.path.join(".kaptaind").join("daemon.pid"))
    }

    fn start(&self, project: &crate::supervisor::model::ProjectSpec) -> Result<()> {
        validate_project_files(project)?;
        let worker_binary = self.resolved_worker_binary()?;
        let mut command = Command::new(worker_binary);
        command
            .arg("--daemon")
            .arg("--config")
            .arg(&project.config)
            .arg("--health-port")
            .arg(project.health_port.to_string())
            .current_dir(&project.path);
        command
            .spawn()
            .with_context(|| format!("failed to start worker for {}", project.path.display()))?;
        Ok(())
    }
}

pub fn observe_pid_file(path: &Path) -> WorkerObservation {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return WorkerObservation::Stopped;
        }
        Err(error) => {
            return WorkerObservation::Invalid {
                reason: format!("failed to read PID file: {error}"),
            };
        }
    };
    let pid = match content.trim().parse::<u32>() {
        Ok(pid) if pid > 1 && pid != std::process::id() => pid,
        _ => {
            return WorkerObservation::Invalid {
                reason: "PID file is malformed or self-referential".to_string(),
            };
        }
    };
    if process_is_kaptaind(pid) {
        WorkerObservation::Running { pid }
    } else {
        WorkerObservation::Stopped
    }
}

fn process_is_kaptaind(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        let path = PathBuf::from(format!("/proc/{pid}/cmdline"));
        fs::read(path).is_ok_and(|bytes| {
            bytes
                .split(|byte| *byte == 0)
                .filter_map(|part| std::str::from_utf8(part).ok())
                .any(|argument| {
                    Path::new(argument)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name == "kaptaind")
                })
        })
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        (unsafe { libc::kill(pid as i32, 0) }) == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn validate_project_files(project: &crate::supervisor::model::ProjectSpec) -> Result<()> {
    project.validate()?;
    anyhow::ensure!(project.path.is_dir(), "project path is not a directory");
    anyhow::ensure!(project.config.is_file(), "project config does not exist");
    let canonical_project = project.path.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize project path {}",
            project.path.display()
        )
    })?;
    let canonical_config = project.config.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize config path {}",
            project.config.display()
        )
    })?;
    anyhow::ensure!(
        canonical_config.starts_with(&canonical_project),
        "project config must remain inside the project path"
    );
    Ok(())
}

pub fn plan_reconciliation(
    snapshot: &SupervisorSnapshot,
    observations: &BTreeMap<String, WorkerObservation>,
    max_starts: usize,
) -> ReconcilePlan {
    let mut port_counts: BTreeMap<u16, usize> = BTreeMap::new();
    for project in snapshot.projects.values() {
        *port_counts.entry(project.health_port).or_default() += 1;
    }
    let duplicate_ports: BTreeSet<u16> = port_counts
        .into_iter()
        .filter_map(|(port, count)| (count > 1).then_some(port))
        .collect();

    let mut admitted_starts = 0usize;
    let mut deferred_starts = 0usize;
    let mut projects = Vec::with_capacity(snapshot.projects.len());
    for project in snapshot.projects.values() {
        let observation = observations
            .get(&project.project_id)
            .cloned()
            .unwrap_or_else(|| WorkerObservation::Invalid {
                reason: "worker observation is missing".to_string(),
            });
        let (action, reason) = if duplicate_ports.contains(&project.health_port) {
            (
                ReconcileAction::Blocked,
                Some(format!(
                    "health port {} is assigned more than once",
                    project.health_port
                )),
            )
        } else {
            match (project.desired_state, &observation) {
                (DesiredState::Enabled, WorkerObservation::Running { .. })
                | (DesiredState::Disabled, WorkerObservation::Stopped) => {
                    (ReconcileAction::Retain, None)
                }
                (DesiredState::Enabled, WorkerObservation::Stopped)
                    if admitted_starts < max_starts =>
                {
                    admitted_starts += 1;
                    (ReconcileAction::Start, None)
                }
                (DesiredState::Enabled, WorkerObservation::Stopped) => {
                    deferred_starts += 1;
                    (
                        ReconcileAction::DeferredCapacity,
                        Some("start admission budget exhausted".to_string()),
                    )
                }
                (DesiredState::Disabled, WorkerObservation::Running { .. }) => (
                    ReconcileAction::DisablePending,
                    Some("worker remains running; explicit termination is required".to_string()),
                ),
                (_, WorkerObservation::Invalid { reason }) => {
                    (ReconcileAction::Blocked, Some(reason.clone()))
                }
            }
        };
        projects.push(ProjectPlan {
            project_id: project.project_id.clone(),
            path: project.path.clone(),
            desired_state: project.desired_state,
            observation,
            action,
            reason,
        });
    }
    ReconcilePlan {
        reconcile_id: uuid::Uuid::new_v4().to_string(),
        generated_at_ms: Utc::now().timestamp_millis(),
        projects,
        admitted_starts,
        deferred_starts,
    }
}

pub struct Supervisor {
    config: SupervisorConfig,
    store: AtomicSnapshotStore,
    padagonia: Option<PadagoniaClient>,
    worker: Arc<dyn WorkerControl>,
    snapshot: SupervisorSnapshot,
    projected_revisions: BTreeMap<String, u64>,
}

impl std::fmt::Debug for Supervisor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Supervisor")
            .field("config", &self.config)
            .field("store", &self.store)
            .field("padagonia_enabled", &self.padagonia.is_some())
            .field("snapshot", &self.snapshot)
            .finish_non_exhaustive()
    }
}

impl Supervisor {
    pub async fn bootstrap(
        config: SupervisorConfig,
        worker: Arc<dyn WorkerControl>,
    ) -> Result<Self> {
        config.validate()?;
        let store = AtomicSnapshotStore::new(config.state_path.clone());
        let mut snapshot = store.load()?;
        if config.legacy_registry_path.exists() {
            let registry = crate::monitor::load_registry_at(&config.legacy_registry_path)?;
            store.import_legacy(&mut snapshot, &registry)?;
        }

        let padagonia = match PadagoniaClient::from_config(&config.padagonia) {
            Ok(client) => client,
            Err(error) if !config.padagonia.required => {
                snapshot.projection.last_error = Some(error.to_string());
                None
            }
            Err(error) => {
                snapshot.projection.last_error = Some(error.to_string());
                None
            }
        };
        let mut projected_revisions = BTreeMap::new();
        if let Some(client) = &padagonia {
            match client.fetch_projects().await {
                Ok(projects) => {
                    for project in projects {
                        projected_revisions.insert(project.project_id.clone(), project.revision);
                        snapshot.merge_project(project);
                    }
                    snapshot.projection.last_success_at_ms = Some(Utc::now().timestamp_millis());
                    snapshot.projection.last_error = None;
                }
                Err(error) => snapshot.projection.last_error = Some(error.to_string()),
            }
        }
        store.save(&snapshot)?;
        Ok(Self {
            config,
            store,
            padagonia,
            worker,
            snapshot,
            projected_revisions,
        })
    }

    pub fn snapshot(&self) -> &SupervisorSnapshot {
        &self.snapshot
    }

    pub fn status(&self) -> FleetStatus {
        FleetStatus::from(&self.snapshot)
    }

    pub fn ready(&self) -> bool {
        !self.config.padagonia.required || self.snapshot.projection.last_error.is_none()
    }

    pub fn plan(&self) -> ReconcilePlan {
        let observations = self.observe_all();
        plan_reconciliation(
            &self.snapshot,
            &observations,
            self.config.max_starts_per_cycle,
        )
    }

    #[tracing::instrument(skip_all, fields(reconcile_id))]
    pub async fn reconcile(&mut self, dry_run: bool) -> Result<ReconcileReport> {
        let started_at_ms = Utc::now().timestamp_millis();
        self.refresh_control_state().await?;
        if !dry_run {
            self.project_desired_state().await?;
        }
        let plan = self.plan();
        tracing::Span::current().record("reconcile_id", &plan.reconcile_id);
        let mut outcomes = Vec::with_capacity(plan.projects.len());
        let mut projection_error: Option<String> = None;
        for project_plan in &plan.projects {
            let project = self
                .snapshot
                .projects
                .get(&project_plan.project_id)
                .context("reconciliation plan references unknown project")?;
            let (success, detail) = match project_plan.action {
                ReconcileAction::Start if dry_run => (true, "start planned".to_string()),
                ReconcileAction::Start => match self.worker.start(project) {
                    Ok(()) => (true, "worker start requested".to_string()),
                    Err(error) => (false, error.to_string()),
                },
                ReconcileAction::Retain => (true, "state retained".to_string()),
                ReconcileAction::DeferredCapacity => {
                    (true, "start deferred by admission budget".to_string())
                }
                ReconcileAction::DisablePending => (
                    true,
                    "worker left running pending explicit termination".to_string(),
                ),
                ReconcileAction::Blocked => (
                    false,
                    project_plan
                        .reason
                        .clone()
                        .unwrap_or_else(|| "project blocked".to_string()),
                ),
            };
            if !dry_run {
                if let Some(client) = &self.padagonia {
                    if let Err(error) = client
                        .project_observation(&plan.reconcile_id, project_plan, success, &detail)
                        .await
                    {
                        tracing::warn!(
                            project_id = %project_plan.project_id,
                            error = %error,
                            "failed to project worker observation to Padagonia"
                        );
                        projection_error = Some(error.to_string());
                    }
                }
            }
            outcomes.push(ProjectOutcome {
                project_id: project_plan.project_id.clone(),
                action: project_plan.action.clone(),
                success,
                detail,
            });
        }
        let report = ReconcileReport {
            reconcile_id: plan.reconcile_id,
            started_at_ms,
            completed_at_ms: Utc::now().timestamp_millis(),
            dry_run,
            outcomes,
            admitted_starts: plan.admitted_starts,
            deferred_starts: plan.deferred_starts,
        };
        if !dry_run {
            if let Some(error) = projection_error {
                self.snapshot.projection.last_error = Some(error);
            } else if self.padagonia.is_some() {
                self.snapshot.projection.last_success_at_ms = Some(Utc::now().timestamp_millis());
                self.snapshot.projection.last_error = None;
            }
            self.snapshot.last_reconcile = Some(report.clone());
            self.snapshot.generation = self.snapshot.generation.saturating_add(1);
            self.store.save(&self.snapshot)?;
        }
        Ok(report)
    }

    pub async fn project_desired_state(&mut self) -> Result<()> {
        let Some(client) = &self.padagonia else {
            return Ok(());
        };
        let mut last_error = None;
        for project in self.snapshot.projects.values() {
            if self
                .projected_revisions
                .get(&project.project_id)
                .is_some_and(|revision| *revision >= project.revision)
            {
                continue;
            }
            if let Err(error) = client.project_desired_state(project).await {
                last_error = Some(error.to_string());
                tracing::warn!(
                    project_id = %project.project_id,
                    error = %error,
                    "failed to project desired state to Padagonia"
                );
            } else {
                self.projected_revisions
                    .insert(project.project_id.clone(), project.revision);
            }
        }
        match last_error {
            Some(error) => self.snapshot.projection.last_error = Some(error),
            None => {
                self.snapshot.projection.last_success_at_ms = Some(Utc::now().timestamp_millis());
                self.snapshot.projection.last_error = None;
            }
        }
        self.store.save(&self.snapshot)
    }

    async fn refresh_control_state(&mut self) -> Result<()> {
        let local = self.store.load()?;
        for project in local.projects.into_values() {
            self.snapshot.merge_project(project);
        }
        if let Some(client) = &self.padagonia {
            match client.fetch_projects().await {
                Ok(projects) => {
                    for project in projects {
                        self.projected_revisions
                            .entry(project.project_id.clone())
                            .and_modify(|revision| *revision = (*revision).max(project.revision))
                            .or_insert(project.revision);
                        self.snapshot.merge_project(project);
                    }
                    self.snapshot.projection.last_success_at_ms =
                        Some(Utc::now().timestamp_millis());
                    self.snapshot.projection.last_error = None;
                }
                Err(error) => {
                    tracing::warn!(error = %error, "failed to refresh desired state from Padagonia");
                    self.snapshot.projection.last_error = Some(error.to_string());
                }
            }
        }
        Ok(())
    }

    fn observe_all(&self) -> BTreeMap<String, WorkerObservation> {
        self.snapshot
            .projects
            .iter()
            .map(|(project_id, project)| (project_id.clone(), self.worker.observe(project)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::model::{project_id_for_path, ProjectSpec};

    fn project(path: &str, enabled: bool, port: u16) -> ProjectSpec {
        let path = PathBuf::from(path);
        ProjectSpec {
            project_id: project_id_for_path(&path),
            config: path.join("kaptaind.toml"),
            path,
            desired_state: if enabled {
                DesiredState::Enabled
            } else {
                DesiredState::Disabled
            },
            health_port: port,
            revision: 1,
            updated_at_ms: 1,
            source: "test".to_string(),
        }
    }

    #[test]
    fn planning_is_deterministic_and_bounded() {
        let mut snapshot = SupervisorSnapshot::default();
        let first = project("/a", true, 3000);
        let second = project("/b", true, 3001);
        snapshot.projects.insert(first.project_id.clone(), first);
        snapshot.projects.insert(second.project_id.clone(), second);
        let observations = snapshot
            .projects
            .keys()
            .map(|id| (id.clone(), WorkerObservation::Stopped))
            .collect();
        let plan = plan_reconciliation(&snapshot, &observations, 1);
        assert_eq!(plan.admitted_starts, 1);
        assert_eq!(plan.deferred_starts, 1);
        assert_eq!(plan.projects[0].action, ReconcileAction::Start);
        assert_eq!(plan.projects[1].action, ReconcileAction::DeferredCapacity);
    }

    #[test]
    fn disabled_running_worker_is_never_terminated_by_plan() {
        let project = project("/a", false, 3000);
        let mut snapshot = SupervisorSnapshot::default();
        snapshot
            .projects
            .insert(project.project_id.clone(), project.clone());
        let observations =
            BTreeMap::from([(project.project_id, WorkerObservation::Running { pid: 42 })]);
        let plan = plan_reconciliation(&snapshot, &observations, 4);
        assert_eq!(plan.projects[0].action, ReconcileAction::DisablePending);
    }

    #[test]
    fn duplicate_ports_block_every_conflicting_project() {
        let first = project("/a", true, 3000);
        let second = project("/b", true, 3000);
        let mut snapshot = SupervisorSnapshot::default();
        snapshot.projects.insert(first.project_id.clone(), first);
        snapshot.projects.insert(second.project_id.clone(), second);
        let observations = snapshot
            .projects
            .keys()
            .map(|id| (id.clone(), WorkerObservation::Stopped))
            .collect();
        let plan = plan_reconciliation(&snapshot, &observations, 4);
        assert!(plan
            .projects
            .iter()
            .all(|project| project.action == ReconcileAction::Blocked));
    }
}
