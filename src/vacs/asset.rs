use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMetrics {
    pub views: u32,
    pub reuse: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub asset_id: String,
    pub concept_id: String,
    #[serde(rename = "type")]
    pub asset_type: String,
    pub created_at: DateTime<Utc>,
    pub source_commit: String,
    pub hash: String,
    pub status: String, // "active" | "stale"
    pub metrics: AssetMetrics,
    pub content: String, // MVP stores SVG/content directly
}

pub struct AssetManager {
    store_dir: PathBuf,
}

impl AssetManager {
    pub fn new(repo_path: &Path) -> Self {
        let store_dir = repo_path.join(".kaptaind").join("vacs").join("assets");
        std::fs::create_dir_all(&store_dir).unwrap_or_default();
        Self { store_dir }
    }

    pub fn save(&self, asset: &Asset) -> anyhow::Result<()> {
        let path = self.store_dir.join(format!("{}.json", asset.asset_id));
        let data = serde_json::to_string_pretty(asset)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn get_all(&self) -> anyhow::Result<Vec<Asset>> {
        let mut assets = Vec::new();
        if !self.store_dir.exists() {
            return Ok(assets);
        }

        for entry in std::fs::read_dir(&self.store_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(asset) = serde_json::from_str::<Asset>(&content) {
                        assets.push(asset);
                    }
                }
            }
        }
        
        assets.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        Ok(assets)
    }
}