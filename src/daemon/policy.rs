use anyhow::Context;
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Policy {
    #[serde(default)]
    pub min_test_coverage: bool,
    #[serde(default)]
    pub required_signoff: bool,
    #[serde(default)]
    pub branch_protection: Vec<String>,
    #[serde(default)]
    pub file_pattern_allowlist: Vec<String>,
}

impl Policy {
    pub fn load_or_default(repo_path: &Path, policy_id: &str) -> anyhow::Result<Self> {
        let path = repo_path
            .join(".kaptaind")
            .join("policies")
            .join(format!("{}.json", policy_id));
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read policy file {}", path.display()))?;
            let policy: Policy = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse policy file {}", path.display()))?;
            Ok(policy)
        } else {
            tracing::info!(policy_id = %policy_id, "policy file not found, using default policy");
            Ok(Policy::default())
        }
    }
}

pub fn current_branch(repo_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn is_branch_protected(repo_path: &Path, protected_branches: &[String]) -> bool {
    if protected_branches.is_empty() {
        return false;
    }
    let Some(current) = current_branch(repo_path) else {
        return false;
    };
    protected_branches.iter().any(|b| b == &current)
}

pub fn cluster_matches_allowlist(paths: &[PathBuf], allowlist: &[String]) -> bool {
    if allowlist.is_empty() {
        return true;
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in allowlist {
        if let Ok(glob) = Glob::new(pattern) {
            builder.add(glob);
        }
    }
    let Ok(globset) = builder.build() else {
        return true;
    };
    paths.iter().any(|path| globset.is_match(path))
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub resource: String,
    pub details: serde_json::Value,
}

pub fn append_audit_log(repo_path: &Path, entry: &AuditEntry) -> anyhow::Result<()> {
    let dir = repo_path.join(".kaptaind");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("audit.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(entry)? + "\n";
    file.write_all(line.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn policy_loads_from_disk() {
        let dir = tempdir().unwrap();
        let policies_dir = dir.path().join(".kaptaind").join("policies");
        std::fs::create_dir_all(&policies_dir).unwrap();
        std::fs::write(
            policies_dir.join("prod.json"),
            r#"{"min_test_coverage":true,"required_signoff":true,"branch_protection":["main"],"file_pattern_allowlist":["**/*.rs"]}"#,
        )
        .unwrap();

        let policy = Policy::load_or_default(dir.path(), "prod").unwrap();
        assert!(policy.min_test_coverage);
        assert!(policy.required_signoff);
        assert_eq!(policy.branch_protection, vec!["main"]);
        assert_eq!(policy.file_pattern_allowlist, vec!["**/*.rs"]);
    }

    #[test]
    fn policy_defaults_when_file_missing() {
        let dir = tempdir().unwrap();
        let policy = Policy::load_or_default(dir.path(), "missing").unwrap();
        assert!(!policy.min_test_coverage);
        assert!(!policy.required_signoff);
        assert!(policy.branch_protection.is_empty());
        assert!(policy.file_pattern_allowlist.is_empty());
    }

    #[test]
    fn branch_protection_detects_protected_branch() {
        let dir = tempdir().unwrap();
        // init git repo with an initial commit so HEAD resolves to a real branch
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["init"])
            .output();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["config", "user.email", "test@test.com"])
            .output();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["config", "user.name", "Test"])
            .output();
        std::fs::write(dir.path().join("file.txt"), "x").unwrap();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["commit", "-m", "init"])
            .output();

        assert!(is_branch_protected(dir.path(), &["main".to_string(), "master".to_string()]));
        assert!(!is_branch_protected(dir.path(), &["develop".to_string()]));
    }

    #[test]
    fn allowlist_matches_paths() {
        let paths = vec![PathBuf::from("src/main.rs"), PathBuf::from("README.md")];
        assert!(cluster_matches_allowlist(&paths, &["**/*.rs".to_string()]));
        assert!(!cluster_matches_allowlist(&paths, &["**/*.go".to_string()]));
        assert!(cluster_matches_allowlist(&paths, &[]));
    }

    #[test]
    fn audit_log_appends_jsonl() {
        let dir = tempdir().unwrap();
        let entry = AuditEntry {
            timestamp: Utc::now(),
            action: "commit_blocked".to_string(),
            resource: "test_failure".to_string(),
            details: serde_json::json!({"reason": "coverage"}),
        };
        append_audit_log(dir.path(), &entry).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".kaptaind/audit.jsonl")).unwrap();
        assert!(content.contains("commit_blocked"));
        assert!(content.contains("coverage"));
    }
}
