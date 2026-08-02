use serde::{Deserialize, Serialize};
use std::path::Path;

/// Atomic write helper used for release metadata files.
pub fn write_atomic(path: &Path, content: &str) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Index entry for a completed release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseIndexEntry {
    pub version: String,
    pub commit: String,
    pub released_at: i64,
    pub stability: f64,
    pub intent: String,
    pub tarball: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleaseIndex {
    pub releases: Vec<ReleaseIndexEntry>,
}

/// Index entry for a manual `ship` run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipIndexEntry {
    #[serde(default = "default_ship_kind")]
    pub kind: String,
    pub version: String,
    pub shipped_at: i64,
    pub targets: Vec<String>,
    pub channels: Vec<String>,
    pub artifacts: Vec<String>,
}

fn default_ship_kind() -> String {
    "manual".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShipIndex {
    pub ships: Vec<ShipIndexEntry>,
}

pub fn load_index(repo_path: &Path) -> ReleaseIndex {
    let path = repo_path
        .join(".kaptaind")
        .join("releases")
        .join("index.json");
    if !path.exists() {
        return ReleaseIndex::default();
    }
    std::fs::read_to_string(&path)
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn append_index(
    repo_path: &Path,
    version: &str,
    commit: &str,
    stability: f64,
    intent: &str,
    tarball: Option<String>,
) {
    let mut index = load_index(repo_path);
    index.releases.push(ReleaseIndexEntry {
        version: version.to_string(),
        commit: commit.to_string(),
        released_at: chrono::Utc::now().timestamp(),
        stability,
        intent: intent.to_string(),
        tarball,
    });
    let releases_dir = repo_path.join(".kaptaind").join("releases");
    if let Err(error) = std::fs::create_dir_all(&releases_dir) {
        tracing::warn!(
            ?error,
            operation = "append_index",
            source_line = line!(),
            "best-effort operation failed"
        );
    }
    if let Ok(content) = serde_json::to_string_pretty(&index) {
        if let Err(error) = write_atomic(&releases_dir.join("index.json"), &content) {
            tracing::warn!(
                ?error,
                operation = "append_index",
                source_line = line!(),
                "best-effort operation failed"
            );
        }
    }
}

pub fn load_ship_index(repo_path: &Path) -> ShipIndex {
    let path = repo_path.join(".kaptaind").join("ship").join("index.json");
    if !path.exists() {
        return ShipIndex::default();
    }
    std::fs::read_to_string(&path)
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn append_ship_index(
    repo_path: &Path,
    kind: &str,
    version: &str,
    targets: &[String],
    channels: &[String],
    artifacts: &[String],
) {
    let mut index = load_ship_index(repo_path);
    index.ships.push(ShipIndexEntry {
        kind: kind.to_string(),
        version: version.to_string(),
        shipped_at: chrono::Utc::now().timestamp(),
        targets: targets.to_vec(),
        channels: channels.to_vec(),
        artifacts: artifacts.to_vec(),
    });
    let ship_dir = repo_path.join(".kaptaind").join("ship");
    if let Err(error) = std::fs::create_dir_all(&ship_dir) {
        tracing::warn!(
            ?error,
            operation = "append_ship_index",
            source_line = line!(),
            "best-effort operation failed"
        );
    }
    if let Ok(content) = serde_json::to_string_pretty(&index) {
        if let Err(error) = write_atomic(&ship_dir.join("index.json"), &content) {
            tracing::warn!(
                ?error,
                operation = "append_ship_index",
                source_line = line!(),
                "best-effort operation failed"
            );
        }
    }
}
