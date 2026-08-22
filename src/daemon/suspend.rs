use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Persistent daemon suspension state stored in `.kaptaind/suspend.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspendState {
    pub suspended: bool,
    pub since: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "manual".to_string()
}

impl SuspendState {
    /// Create a new suspended state.
    pub fn suspended(source: impl Into<String>, reason: Option<String>) -> Self {
        Self {
            suspended: true,
            since: Utc::now(),
            reason,
            source: source.into(),
        }
    }
}

/// Return `true` if a suspend file exists and marks the daemon as suspended.
pub fn is_suspended(repo_path: &Path) -> bool {
    load(repo_path)
        .map(|s| s.map(|s| s.suspended).unwrap_or(false))
        .unwrap_or(false)
}

/// Load the current suspend state, if any.
pub fn load(repo_path: &Path) -> anyhow::Result<Option<SuspendState>> {
    let path = repo_path.join(".kaptaind").join("suspend.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let state = serde_json::from_str(&content)?;
    Ok(Some(state))
}

/// Persist a suspend state atomically.
pub fn save(repo_path: &Path, state: &SuspendState) -> anyhow::Result<()> {
    let dir = repo_path.join(".kaptaind");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("suspend.json");
    let content = serde_json::to_string_pretty(state)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove the suspend file, if present.
pub fn remove(repo_path: &Path) -> anyhow::Result<()> {
    let path = repo_path.join(".kaptaind").join("suspend.json");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn no_suspend_file_means_not_suspended() {
        let dir = tempdir().expect("temp dir");
        assert!(!is_suspended(dir.path()));
        assert!(load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn suspend_save_load_remove_roundtrip() {
        let dir = tempdir().expect("temp dir");
        let state = SuspendState::suspended("aoc", Some("AoC session started".to_string()));
        save(dir.path(), &state).unwrap();

        assert!(is_suspended(dir.path()));
        let loaded = load(dir.path()).unwrap().expect("state exists");
        assert!(loaded.suspended);
        assert_eq!(loaded.source, "aoc");
        assert_eq!(loaded.reason.as_deref(), Some("AoC session started"));

        remove(dir.path()).unwrap();
        assert!(!is_suspended(dir.path()));
        assert!(load(dir.path()).unwrap().is_none());
        assert!(!dir.path().join(".kaptaind").join("suspend.tmp").exists());
    }
}
