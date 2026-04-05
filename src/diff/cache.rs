use crate::diff::lang::adapter::AstRepresentation;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CACHE_FILE: &str = ".kaptaind/ast_cache.json";

/// Cached AST entry: hash + serializable snapshot of parsed symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub hash: String,
    pub symbols: Vec<CachedSymbol>,
    pub structure_hash: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSymbol {
    pub name: String,
    pub kind: String,
}

/// Persistent file-hash cache for AST analysis results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AstCache {
    /// Map from relative file path to cached entry.
    entries: HashMap<String, CacheEntry>,
    #[serde(skip)]
    dirty: bool,
}

impl AstCache {
    /// Load cache from disk, or return empty cache if missing/corrupt.
    pub fn load(repo_root: &Path) -> Self {
        let path = repo_root.join(CACHE_FILE);
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    /// Persist cache to disk (only if dirty).
    pub fn save(&self, repo_root: &Path) {
        if !self.dirty {
            return;
        }
        let path = repo_root.join(CACHE_FILE);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Look up a cached AST result by file path and current hash.
    /// Returns `Some(AstRepresentation)` if the file hasn't changed since last parse.
    pub fn get(&self, relative_path: &str, current_hash: &str) -> Option<AstRepresentation> {
        let entry = self.entries.get(relative_path)?;
        if entry.hash != current_hash {
            return None;
        }
        let symbols = entry
            .symbols
            .iter()
            .map(|s| crate::diff::lang::adapter::Symbol {
                name: s.name.clone(),
                kind: s.kind.clone(),
            })
            .collect();
        Some(AstRepresentation {
            symbols,
            structure_hash: entry.structure_hash,
            ..Default::default()
        })
    }

    /// Store a parsed AST result for a file.
    pub fn put(&mut self, relative_path: &str, hash: &str, ast: &AstRepresentation) {
        let symbols = ast
            .symbols
            .iter()
            .map(|s| CachedSymbol {
                name: s.name.clone(),
                kind: s.kind.clone(),
            })
            .collect();
        self.entries.insert(
            relative_path.to_string(),
            CacheEntry {
                hash: hash.to_string(),
                symbols,
                structure_hash: ast.structure_hash,
            },
        );
        self.dirty = true;
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Evict entries for files that no longer exist relative to repo_root.
    pub fn prune(&mut self, repo_root: &Path) {
        let before = self.entries.len();
        self.entries
            .retain(|path, _| repo_root.join(path).exists());
        if self.entries.len() != before {
            self.dirty = true;
        }
    }
}

/// Compute SHA256 hash of a file's contents. Returns hex string.
pub fn hash_file(path: &Path) -> Option<String> {
    let content = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Some(hex::encode(hasher.finalize()))
}

/// Compute SHA256 of a byte slice (for testing).
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::lang::adapter::{AstRepresentation, Symbol};
    use tempfile::tempdir;

    #[test]
    fn cache_hit_returns_ast() {
        let mut cache = AstCache::default();
        let ast = AstRepresentation {
            symbols: vec![Symbol {
                name: "greet".to_string(),
                kind: "function".to_string(),
            }],
            structure_hash: 42,
            ..Default::default()
        };
        cache.put("src/lib.rs", "abc123", &ast);

        let result = cache.get("src/lib.rs", "abc123");
        assert!(result.is_some());
        let cached = result.unwrap();
        assert_eq!(cached.symbols.len(), 1);
        assert_eq!(cached.symbols[0].name, "greet");
        assert_eq!(cached.structure_hash, 42);
    }

    #[test]
    fn cache_miss_on_different_hash() {
        let mut cache = AstCache::default();
        let ast = AstRepresentation {
            symbols: vec![],
            structure_hash: 0,
            ..Default::default()
        };
        cache.put("src/lib.rs", "abc123", &ast);
        assert!(cache.get("src/lib.rs", "def456").is_none());
    }

    #[test]
    fn cache_miss_on_unknown_path() {
        let cache = AstCache::default();
        assert!(cache.get("src/main.rs", "abc123").is_none());
    }

    #[test]
    fn cache_save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".kaptaind")).unwrap();

        let mut cache = AstCache::default();
        let ast = AstRepresentation {
            symbols: vec![Symbol {
                name: "hello".to_string(),
                kind: "function".to_string(),
            }],
            structure_hash: 99,
            ..Default::default()
        };
        cache.put("src/lib.rs", "hash1", &ast);
        cache.save(dir.path());

        let loaded = AstCache::load(dir.path());
        let result = loaded.get("src/lib.rs", "hash1");
        assert!(result.is_some());
        assert_eq!(result.unwrap().symbols[0].name, "hello");
    }

    #[test]
    fn hash_file_is_deterministic() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "pub fn hello() {}").unwrap();

        let h1 = hash_file(&file).unwrap();
        let h2 = hash_file(&file).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA256 hex = 64 chars
    }

    #[test]
    fn hash_file_changes_on_content_change() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.rs");

        std::fs::write(&file, "version 1").unwrap();
        let h1 = hash_file(&file).unwrap();

        std::fs::write(&file, "version 2").unwrap();
        let h2 = hash_file(&file).unwrap();

        assert_ne!(h1, h2);
    }

    #[test]
    fn prune_removes_stale_entries() {
        let dir = tempdir().unwrap();
        let existing = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(existing.parent().unwrap()).unwrap();
        std::fs::write(&existing, "").unwrap();

        let mut cache = AstCache::default();
        let ast = AstRepresentation::default();
        cache.put("src/lib.rs", "h1", &ast);
        cache.put("src/gone.rs", "h2", &ast);

        cache.prune(dir.path());
        assert_eq!(cache.len(), 1);
        assert!(cache.get("src/lib.rs", "h1").is_some());
        assert!(cache.get("src/gone.rs", "h2").is_none());
    }
}
