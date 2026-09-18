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
    /// Commit SHAs (oldest first) whose daemon commit body references one of
    /// `trace_ids` via `cluster=<uuid>`. Recorded at ship time so downstream
    /// tools (e.g. scrawny's `--aoc` scope) can derive the session's
    /// realised diff from the manifest alone, without re-grepping history.
    /// Absent in manifests written before this linkage existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<String>,
}

/// Commit SHAs (oldest first) realised by an AoC session. Daemon commit
/// bodies embed `cluster=<uuid>` for the cluster they realised, so the
/// session's commits are exactly those still reachable whose message
/// mentions one of its cluster ids. Commits amended or rebased away since
/// simply aren't recorded — the manifest links what history still holds.
/// Shared by the `aoc ship` CLI path and the daemon's auto-reap ship so
/// both produce the same linkage.
pub fn session_commit_hashes(repo_path: &Path, trace_ids: &[String]) -> anyhow::Result<Vec<String>> {
    use std::collections::HashSet;

    if trace_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut args: Vec<String> = vec!["log".to_string(), "--format=%H".to_string()];
    for id in trace_ids {
        args.push("--grep".to_string());
        args.push(format!("cluster={id}"));
    }

    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(&args)
        .output()
        .map_err(|e| anyhow::anyhow!("running git log for AoC commit linkage: {e}"))?;

    // A failing log (empty repo, not a repository at all) means no linkage,
    // not a failed ship — the manifest just records no commits.
    if !output.status.success() {
        return Ok(Vec::new());
    }

    // git log walks the history newest-first; a session's manifest reads
    // oldest-first. Reverse git's own DAG order rather than sorting by
    // date — commits within the same second would otherwise be ordered
    // arbitrarily.
    let text = String::from_utf8_lossy(&output.stdout);
    let mut seen = HashSet::new();
    let mut hashes: Vec<String> = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    hashes.reverse();
    Ok(hashes
        .into_iter()
        .filter(|hash| seen.insert(hash.clone()))
        .collect())
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
    manifests.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
            commits: vec![],
        };

        save_manifest(repo_path, &manifest).unwrap();
        let manifests = list_manifests(repo_path).unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].label, "test-feature");
    }

    #[test]
    fn test_manifest_without_commits_field_still_deserializes() {
        // Manifests written before commit linkage existed must keep loading.
        let json = r#"{
            "id": "22222222-2222-2222-2222-222222222222",
            "label": "legacy",
            "created_at": "2026-09-01T10:00:00Z",
            "shipped_at": "2026-09-02T10:00:00Z",
            "initial_version": "0.1.0",
            "final_version": "0.1.1",
            "cluster_count": 1,
            "commit_count": 1,
            "test_failures": 0,
            "trace_ids": ["aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"]
        }"#;
        let manifest: AocManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.commits.is_empty());
    }

    #[test]
    fn test_manifest_with_commits_round_trips() {
        let json = r#"{
            "id": "22222222-2222-2222-2222-222222222222",
            "label": "linked",
            "created_at": "2026-09-01T10:00:00Z",
            "shipped_at": "2026-09-02T10:00:00Z",
            "initial_version": "0.1.0",
            "final_version": "0.1.1",
            "cluster_count": 1,
            "commit_count": 1,
            "test_failures": 0,
            "trace_ids": ["aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"],
            "commits": ["1111111111111111111111111111111111111111"]
        }"#;
        let manifest: AocManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.commits.len(), 1);
        let serialised = serde_json::to_string(&manifest).unwrap();
        assert!(serialised.contains("\"commits\""));
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .expect("git command runnable");
        assert!(status.success(), "git {args:?} failed");
    }

    fn git_output(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git runnable");
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn session_commit_hashes_collects_only_session_commits_oldest_first() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        git(repo, &["init", "-q"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Kaptaind Test"]);

        std::fs::write(repo.join("f.txt"), "1\n").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", "unrelated initial commit"]);

        std::fs::write(repo.join("f.txt"), "2\n").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", "kaptaind: Patch -> v0.1.1 [cluster=aaaaaaaa]"]);

        std::fs::write(repo.join("f.txt"), "3\n").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", "kaptaind: Patch -> v0.1.2 [cluster=bbbbbbbb]"]);

        let hashes =
            session_commit_hashes(repo, &["aaaaaaaa".to_string(), "bbbbbbbb".to_string()])
                .unwrap();
        assert_eq!(hashes.len(), 2, "the unrelated initial commit must be excluded");
        assert_eq!(hashes[0], git_output(repo, &["rev-parse", "HEAD~1"]), "oldest first");
        assert_eq!(hashes[1], git_output(repo, &["rev-parse", "HEAD"]));
    }

    #[test]
    fn session_commit_hashes_without_traces_is_empty() {
        let dir = TempDir::new().unwrap();
        assert!(session_commit_hashes(dir.path(), &[]).unwrap().is_empty());
    }

    #[test]
    fn session_commit_hashes_outside_a_repository_is_empty() {
        let dir = TempDir::new().unwrap();
        let result = session_commit_hashes(dir.path(), &["aaaaaaaa".to_string()]).unwrap();
        assert!(result.is_empty(), "not a repository → no linkage, not an error");
    }
}
