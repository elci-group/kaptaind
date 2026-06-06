use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// An active or completed Aim of Change session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AocSession {
    pub id: String,    // UUID as string for stable serialization
    pub label: String, // User-friendly name (e.g. "refactor-engine")
    pub created_at: DateTime<Utc>,
    pub initial_version: String,
    /// Optional release intent (directive §8.3): "none" | "preview" | "internal" | "public".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Minimum stability score required before this session triggers a release on close.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_stability: Option<f64>,
}

/// A completed AoC manifest that links all traces from a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AocManifest {
    pub id: String, // UUID
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub shipped_at: DateTime<Utc>,
    pub initial_version: String,
    pub final_version: String,
    pub cluster_count: usize,
    pub commit_count: usize,
    pub test_failures: usize,
    pub trace_ids: Vec<String>, // cluster UUIDs in order
}

/// Load the currently active AoC session, if any.
pub fn load_active(repo_path: &Path) -> anyhow::Result<Option<AocSession>> {
    let active_path = repo_path.join(".kaptaind").join("aoc").join("active.json");
    if !active_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&active_path)?;
    let session = serde_json::from_str(&content)?;
    Ok(Some(session))
}

/// Write the active AoC session.
pub fn save_active(repo_path: &Path, session: &AocSession) -> anyhow::Result<()> {
    let aoc_dir = repo_path.join(".kaptaind").join("aoc");
    fs::create_dir_all(&aoc_dir)?;
    let active_path = aoc_dir.join("active.json");
    let content = serde_json::to_string_pretty(session)?;
    fs::write(&active_path, content)?;
    Ok(())
}

/// Remove the active AoC session.
pub fn remove_active(repo_path: &Path) -> anyhow::Result<()> {
    let active_path = repo_path.join(".kaptaind").join("aoc").join("active.json");
    if active_path.exists() {
        fs::remove_file(&active_path)?;
    }
    Ok(())
}

/// Save a completed AoC manifest.
pub fn save_manifest(repo_path: &Path, manifest: &AocManifest) -> anyhow::Result<()> {
    let aoc_dir = repo_path.join(".kaptaind").join("aoc");
    fs::create_dir_all(&aoc_dir)?;
    let manifest_path = aoc_dir.join(format!("{}.json", manifest.id));
    let content = serde_json::to_string_pretty(manifest)?;
    fs::write(&manifest_path, content)?;
    Ok(())
}

/// List all completed AoC manifests (excluding active.json).
pub fn list_manifests(repo_path: &Path) -> anyhow::Result<Vec<AocManifest>> {
    let aoc_dir = repo_path.join(".kaptaind").join("aoc");
    if !aoc_dir.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    for entry in fs::read_dir(&aoc_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|ext| ext == "json").unwrap_or(false)
            && path
                .file_name()
                .map(|n| n != "active.json")
                .unwrap_or(false)
        {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(manifest) = serde_json::from_str::<AocManifest>(&content) {
                    manifests.push(manifest);
                }
            }
        }
    }

    // Sort by created_at descending (newest first)
    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn test_save_and_load_active() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path();

        let session = AocSession {
            id: Uuid::new_v4().to_string(),
            label: "test-feature".to_string(),
            created_at: Utc::now(),
            initial_version: "0.1.0".to_string(),
            intent: None,
            target_stability: None,
        };

        save_active(repo_path, &session).unwrap();
        let loaded = load_active(repo_path).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.label, "test-feature");
    }

    #[test]
    fn test_remove_active() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path();

        let session = AocSession {
            id: Uuid::new_v4().to_string(),
            label: "test-feature".to_string(),
            created_at: Utc::now(),
            initial_version: "0.1.0".to_string(),
            intent: None,
            target_stability: None,
        };

        save_active(repo_path, &session).unwrap();
        assert!(load_active(repo_path).unwrap().is_some());

        remove_active(repo_path).unwrap();
        assert!(load_active(repo_path).unwrap().is_none());
    }

    #[test]
    fn test_save_and_list_manifests() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path();

        let manifest = AocManifest {
            id: Uuid::new_v4().to_string(),
            label: "test-feature".to_string(),
            created_at: Utc::now(),
            shipped_at: Utc::now(),
            initial_version: "0.1.0".to_string(),
            final_version: "0.2.0".to_string(),
            cluster_count: 5,
            commit_count: 3,
            test_failures: 1,
            trace_ids: vec![],
        };

        save_manifest(repo_path, &manifest).unwrap();
        let manifests = list_manifests(repo_path).unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].label, "test-feature");
    }
}
