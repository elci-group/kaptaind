use crate::monitor::MonitorEntry;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SUPERVISOR_SCHEMA_VERSION: &str = "kaptaind.supervisor/v1";
pub const CONTROL_SCHEMA_VERSION: &str = "kaptaind.control/v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    Enabled,
    Disabled,
}

impl DesiredState {
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSpec {
    pub project_id: String,
    pub path: PathBuf,
    pub config: PathBuf,
    pub desired_state: DesiredState,
    pub health_port: u16,
    pub revision: u64,
    pub updated_at_ms: i64,
    pub source: String,
}

impl ProjectSpec {
    pub fn from_monitor(entry: &MonitorEntry, revision: u64, source: &str) -> Self {
        Self {
            project_id: project_id_for_path(&entry.path),
            path: entry.path.clone(),
            config: entry.config.clone(),
            desired_state: if entry.enabled {
                DesiredState::Enabled
            } else {
                DesiredState::Disabled
            },
            health_port: entry.health_port,
            revision,
            updated_at_ms: Utc::now().timestamp_millis(),
            source: source.to_string(),
        }
    }

    pub fn same_control_intent(&self, entry: &MonitorEntry) -> bool {
        self.path == entry.path
            && self.config == entry.config
            && self.desired_state.is_enabled() == entry.enabled
            && self.health_port == entry.health_port
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.project_id.is_empty(), "project_id must not be empty");
        anyhow::ensure!(self.path.is_absolute(), "project path must be absolute");
        anyhow::ensure!(self.config.is_absolute(), "config path must be absolute");
        anyhow::ensure!(self.health_port != 0, "health port must not be zero");
        anyhow::ensure!(self.revision > 0, "project revision must be positive");
        anyhow::ensure!(
            self.project_id == project_id_for_path(&self.path),
            "project identity does not match canonical path"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionStatus {
    pub last_success_at_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupervisorSnapshot {
    pub schema_version: String,
    pub generation: u64,
    pub instance_id: String,
    pub projects: BTreeMap<String, ProjectSpec>,
    #[serde(default)]
    pub projection: ProjectionStatus,
    #[serde(default)]
    pub last_reconcile: Option<ReconcileReport>,
}

impl Default for SupervisorSnapshot {
    fn default() -> Self {
        Self {
            schema_version: SUPERVISOR_SCHEMA_VERSION.to_string(),
            generation: 0,
            instance_id: uuid::Uuid::new_v4().to_string(),
            projects: BTreeMap::new(),
            projection: ProjectionStatus::default(),
            last_reconcile: None,
        }
    }
}

impl SupervisorSnapshot {
    pub fn validate(&self) -> anyhow::Result<Vec<SnapshotWarning>> {
        anyhow::ensure!(
            self.schema_version == SUPERVISOR_SCHEMA_VERSION,
            "unsupported supervisor snapshot schema: {}",
            self.schema_version
        );
        anyhow::ensure!(
            !self.instance_id.is_empty(),
            "instance_id must not be empty"
        );

        let mut warnings = Vec::new();
        let mut ports: BTreeMap<u16, Vec<String>> = BTreeMap::new();
        let mut paths = BTreeSet::new();
        for (key, project) in &self.projects {
            project.validate()?;
            anyhow::ensure!(
                key == &project.project_id,
                "project map key does not match project_id"
            );
            anyhow::ensure!(
                paths.insert(project.path.clone()),
                "duplicate project path: {}",
                project.path.display()
            );
            ports
                .entry(project.health_port)
                .or_default()
                .push(project.project_id.clone());
        }
        for (port, project_ids) in ports {
            if project_ids.len() > 1 {
                warnings.push(SnapshotWarning::DuplicateHealthPort { port, project_ids });
            }
        }
        Ok(warnings)
    }

    pub fn merge_project(&mut self, candidate: ProjectSpec) -> bool {
        let replace = self
            .projects
            .get(&candidate.project_id)
            .is_none_or(|current| {
                candidate.revision > current.revision
                    || (candidate.revision == current.revision
                        && candidate.updated_at_ms > current.updated_at_ms)
            });
        if replace {
            self.projects
                .insert(candidate.project_id.clone(), candidate);
            self.generation = self.generation.saturating_add(1);
        }
        replace
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapshotWarning {
    DuplicateHealthPort { port: u16, project_ids: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WorkerObservation {
    Running { pid: u32 },
    Stopped,
    Invalid { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileAction {
    Retain,
    Start,
    DeferredCapacity,
    DisablePending,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectPlan {
    pub project_id: String,
    pub path: PathBuf,
    pub desired_state: DesiredState,
    pub observation: WorkerObservation,
    pub action: ReconcileAction,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconcilePlan {
    pub reconcile_id: String,
    pub generated_at_ms: i64,
    pub projects: Vec<ProjectPlan>,
    pub admitted_starts: usize,
    pub deferred_starts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectOutcome {
    pub project_id: String,
    pub action: ReconcileAction,
    pub success: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconcileReport {
    pub reconcile_id: String,
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
    pub dry_run: bool,
    pub outcomes: Vec<ProjectOutcome>,
    pub admitted_starts: usize,
    pub deferred_starts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetStatus {
    pub schema_version: String,
    pub generation: u64,
    pub instance_id: String,
    pub projects: usize,
    pub enabled: usize,
    pub disabled: usize,
    pub projection: ProjectionStatus,
    pub last_reconcile: Option<ReconcileReport>,
}

impl From<&SupervisorSnapshot> for FleetStatus {
    fn from(snapshot: &SupervisorSnapshot) -> Self {
        let enabled = snapshot
            .projects
            .values()
            .filter(|project| project.desired_state.is_enabled())
            .count();
        Self {
            schema_version: snapshot.schema_version.clone(),
            generation: snapshot.generation,
            instance_id: snapshot.instance_id.clone(),
            projects: snapshot.projects.len(),
            enabled,
            disabled: snapshot.projects.len().saturating_sub(enabled),
            projection: snapshot.projection.clone(),
            last_reconcile: snapshot.last_reconcile.clone(),
        }
    }
}

pub fn project_id_for_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    format!("project-{}", crate::util::hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(path: &str, port: u16, revision: u64) -> ProjectSpec {
        let path = PathBuf::from(path);
        ProjectSpec {
            project_id: project_id_for_path(&path),
            config: path.join("kaptaind.toml"),
            path,
            desired_state: DesiredState::Enabled,
            health_port: port,
            revision,
            updated_at_ms: revision as i64,
            source: "test".to_string(),
        }
    }

    #[test]
    fn highest_revision_wins() {
        let mut snapshot = SupervisorSnapshot::default();
        assert!(snapshot.merge_project(project("/repo", 3000, 2)));
        assert!(!snapshot.merge_project(project("/repo", 3001, 1)));
        assert_eq!(snapshot.projects.values().next().unwrap().health_port, 3000);
    }

    #[test]
    fn duplicate_ports_are_visible_warnings() {
        let mut snapshot = SupervisorSnapshot::default();
        snapshot.merge_project(project("/repo/a", 3000, 1));
        snapshot.merge_project(project("/repo/b", 3000, 1));
        assert!(matches!(
            snapshot.validate().unwrap().as_slice(),
            [SnapshotWarning::DuplicateHealthPort { port: 3000, .. }]
        ));
    }
}
