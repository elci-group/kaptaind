use crate::monitor::MonitorRegistry;
use crate::supervisor::model::{ProjectSpec, SupervisorSnapshot};
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AtomicSnapshotStore {
    path: PathBuf,
}

impl AtomicSnapshotStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<SupervisorSnapshot> {
        if !self.path.exists() {
            return Ok(SupervisorSnapshot::default());
        }
        let bytes = fs::read(&self.path).with_context(|| {
            format!("failed to read supervisor state at {}", self.path.display())
        })?;
        let snapshot: SupervisorSnapshot = serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid supervisor state at {}", self.path.display()))?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn save(&self, snapshot: &SupervisorSnapshot) -> Result<()> {
        snapshot.validate()?;
        let parent = self
            .path
            .parent()
            .context("supervisor state path has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create supervisor state directory {}",
                parent.display()
            )
        })?;

        let temporary = self.path.with_extension(format!(
            "json.tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let payload = serde_json::to_vec_pretty(snapshot)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("failed to create temporary state {}", temporary.display()))?;
        file.write_all(&payload)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &self.path).with_context(|| {
            format!(
                "failed to atomically replace supervisor state {}",
                self.path.display()
            )
        })?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }

    pub fn import_legacy(
        &self,
        snapshot: &mut SupervisorSnapshot,
        registry: &MonitorRegistry,
    ) -> Result<ImportSummary> {
        let mut imported = 0usize;
        let mut unchanged = 0usize;
        for entry in &registry.projects {
            let project_id = crate::supervisor::model::project_id_for_path(&entry.path);
            if snapshot
                .projects
                .get(&project_id)
                .is_some_and(|project| project.same_control_intent(entry))
            {
                unchanged += 1;
                continue;
            }
            let revision = snapshot
                .projects
                .get(&project_id)
                .map_or(1, |project| project.revision.saturating_add(1));
            snapshot.merge_project(ProjectSpec::from_monitor(entry, revision, "legacy_import"));
            imported += 1;
        }
        let warnings = snapshot.validate()?;
        if imported > 0 || !self.path.exists() {
            self.save(snapshot)?;
        }
        Ok(ImportSummary {
            imported,
            unchanged,
            warnings: warnings.len(),
        })
    }

    pub fn synchronize_legacy(
        &self,
        snapshot: &mut SupervisorSnapshot,
        registry: &MonitorRegistry,
    ) -> Result<ImportSummary> {
        let mut summary = self.import_legacy(snapshot, registry)?;
        let present: std::collections::BTreeSet<String> = registry
            .projects
            .iter()
            .map(|entry| crate::supervisor::model::project_id_for_path(&entry.path))
            .collect();
        let removed: Vec<String> = snapshot
            .projects
            .keys()
            .filter(|project_id| !present.contains(*project_id))
            .cloned()
            .collect();
        let mut changed = false;
        for project_id in removed {
            let Some(current) = snapshot.projects.get(&project_id).cloned() else {
                continue;
            };
            if current.desired_state == crate::supervisor::model::DesiredState::Disabled
                && current.source == "legacy_remove"
            {
                continue;
            }
            let mut disabled = current;
            disabled.desired_state = crate::supervisor::model::DesiredState::Disabled;
            disabled.revision = disabled.revision.saturating_add(1);
            disabled.updated_at_ms = chrono::Utc::now().timestamp_millis();
            disabled.source = "legacy_remove".to_string();
            snapshot.merge_project(disabled);
            changed = true;
            summary.imported = summary.imported.saturating_add(1);
        }
        if changed {
            summary.warnings = snapshot.validate()?.len();
            self.save(snapshot)?;
        }
        Ok(summary)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ImportSummary {
    pub imported: usize,
    pub unchanged: usize,
    pub warnings: usize,
}

/// Compatibility dual-write used by existing monitor commands. The legacy
/// registry remains the first write during the migration window; this function
/// updates the supervisor's atomic continuity snapshot and surfaces failures.
pub fn sync_default_from_legacy(registry: &MonitorRegistry) -> Result<ImportSummary> {
    let config = crate::supervisor::config::SupervisorConfig::default();
    let store = AtomicSnapshotStore::new(config.state_path);
    let mut snapshot = store.load()?;
    store.synchronize_legacy(&mut snapshot, registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::MonitorEntry;
    use chrono::Utc;
    use tempfile::tempdir;

    fn registry(path: &Path) -> MonitorRegistry {
        MonitorRegistry {
            projects: vec![MonitorEntry {
                path: path.to_path_buf(),
                config: path.join("kaptaind.toml"),
                enabled: true,
                health_port: 3000,
                last_active: Some(Utc::now()),
            }],
        }
    }

    #[test]
    fn snapshot_round_trip_is_valid() {
        let dir = tempdir().unwrap();
        let store = AtomicSnapshotStore::new(dir.path().join("state.json"));
        let mut snapshot = store.load().unwrap();
        store
            .import_legacy(&mut snapshot, &registry(&dir.path().join("repo")))
            .unwrap();
        assert_eq!(store.load().unwrap().projects.len(), 1);
    }

    #[test]
    fn legacy_import_is_idempotent() {
        let dir = tempdir().unwrap();
        let store = AtomicSnapshotStore::new(dir.path().join("state.json"));
        let legacy = registry(&dir.path().join("repo"));
        let mut snapshot = SupervisorSnapshot::default();
        let first = store.import_legacy(&mut snapshot, &legacy).unwrap();
        let second = store.import_legacy(&mut snapshot, &legacy).unwrap();
        assert_eq!(first.imported, 1);
        assert_eq!(second.unchanged, 1);
        assert_eq!(snapshot.projects.values().next().unwrap().revision, 1);
    }

    #[test]
    fn corrupt_snapshot_is_never_silently_replaced() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"not-json").unwrap();
        let store = AtomicSnapshotStore::new(path.clone());
        assert!(store.load().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"not-json");
    }

    #[test]
    fn legacy_removal_becomes_higher_revision_disabled_intent() {
        let dir = tempdir().unwrap();
        let store = AtomicSnapshotStore::new(dir.path().join("state.json"));
        let legacy = registry(&dir.path().join("repo"));
        let mut snapshot = SupervisorSnapshot::default();
        store.synchronize_legacy(&mut snapshot, &legacy).unwrap();
        store
            .synchronize_legacy(&mut snapshot, &MonitorRegistry::default())
            .unwrap();
        let project = snapshot.projects.values().next().unwrap();
        assert_eq!(
            project.desired_state,
            crate::supervisor::model::DesiredState::Disabled
        );
        assert_eq!(project.revision, 2);
        assert_eq!(project.source, "legacy_remove");
    }
}
