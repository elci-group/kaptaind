use crate::diff_version::detector::LanguageVersion;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const CACHE_FILE: &str = ".kaptaind/version_cache.json";
/// Redetect after 1 hour.
const TTL_SECS: i64 = 3600;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionCache {
    entries: HashMap<String, CacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    version: String,
    detected_from: String,
    cached_at: i64,
}

impl VersionCache {
    /// Load from `.kaptaind/version_cache.json`, or return empty cache.
    pub fn load(repo_path: &Path) -> Self {
        let path = repo_path.join(CACHE_FILE);
        std::fs::read_to_string(&path)
            // traci: allow -- optional failure is represented by None and handled by the caller.
            .ok()
            // traci: allow -- optional failure is represented by None and handled by the caller.
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    }

    /// Persist to disk.
    pub fn save(&self, repo_path: &Path) {
        let path = repo_path.join(CACHE_FILE);
        if let Some(parent) = path.parent() {
            // traci: allow -- this dependency-light library treats cache persistence as optional.
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            // traci: allow -- this dependency-light library treats cache persistence as optional.
            let _ = std::fs::write(&path, json);
        }
    }

    /// Retrieve a cached version for `lang_name` if it's still within TTL.
    pub fn get(&self, lang_name: &str) -> Option<LanguageVersion> {
        let entry = self.entries.get(lang_name)?;
        let now = chrono::Utc::now().timestamp();
        if now - entry.cached_at > TTL_SECS {
            return None; // stale
        }
        Some(LanguageVersion {
            version: entry.version.clone(),
            detected_from: entry.detected_from.clone(),
            source: Default::default(),
        })
    }

    /// Store a detected version.
    pub fn put(&mut self, lang_name: &str, lv: &LanguageVersion) {
        self.entries.insert(
            lang_name.to_string(),
            CacheEntry {
                version: lv.version.clone(),
                detected_from: lv.detected_from.clone(),
                cached_at: chrono::Utc::now().timestamp(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrip_through_disk() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".kaptaind")).unwrap();
        let mut cache = VersionCache::default();
        cache.put(
            "rust",
            &LanguageVersion {
                version: "2021".into(),
                detected_from: "Cargo.toml".into(),
                source: Default::default(),
            },
        );
        cache.save(dir.path());
        let loaded = VersionCache::load(dir.path());
        let got = loaded.get("rust").unwrap();
        assert_eq!(got.version, "2021");
        assert_eq!(got.detected_from, "Cargo.toml");
    }

    #[test]
    fn missing_entry_returns_none() {
        let cache = VersionCache::default();
        assert!(cache.get("python").is_none());
    }
}
