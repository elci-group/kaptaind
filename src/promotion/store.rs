//! Append-only JSONL store for promotions (`.kaptaind/promotions.jsonl`).
//!
//! Every status change appends a new full snapshot rather than mutating a
//! record in place, so the file is itself a promotion audit trail (ELCI
//! KAPTAIND-RTL-001 §28). Callers collapse to the latest snapshot per id when
//! they need "current" state.

use super::model::Promotion;
use anyhow::Result;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

fn path(repo: &Path) -> PathBuf {
    repo.join(".kaptaind").join("promotions.jsonl")
}

pub fn append(repo: &Path, promotion: &Promotion) -> Result<()> {
    let file_path = path(repo);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(file_path)?;
    file.write_all(serde_json::to_string(promotion)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

fn load_all(repo: &Path) -> Result<Vec<Promotion>> {
    let file_path = path(repo);
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    std::fs::read_to_string(file_path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

/// The latest recorded snapshot of every promotion, in first-seen order.
pub fn latest(repo: &Path) -> Result<Vec<Promotion>> {
    let mut order = Vec::new();
    let mut by_id: BTreeMap<String, Promotion> = BTreeMap::new();
    for promotion in load_all(repo)? {
        if !by_id.contains_key(&promotion.id) {
            order.push(promotion.id.clone());
        }
        by_id.insert(promotion.id.clone(), promotion);
    }
    Ok(order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect())
}

pub fn get(repo: &Path, id: &str) -> Result<Option<Promotion>> {
    Ok(latest(repo)?
        .into_iter()
        .find(|promotion| promotion.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::promotion::model::{Operation, PromotionPlan, PromotionStatus};
    use chrono::Utc;

    fn sample(id: &str, status: PromotionStatus) -> Promotion {
        let now = Utc::now();
        Promotion {
            id: id.to_string(),
            repository: "repo".into(),
            plan: PromotionPlan {
                repository: "repo".into(),
                transition: "development->staging".into(),
                source_branch: "development".into(),
                target_branch: "staging".into(),
                source_role: "development".into(),
                target_role: "staging".into(),
                source_revision: "abc".into(),
                target_revision: "def".into(),
                commits: 1,
                files_changed: 1,
                insertions: 1,
                deletions: 0,
                conflicts: 0,
                operation: Operation::Merge,
                requires: vec![],
            },
            status,
            gates: vec![],
            approved_by: None,
            executed_operation: None,
            result_commit: None,
            failure_reason: None,
            recovery_action: None,
            created_at: now,
            updated_at: now,
            history: vec![],
        }
    }

    #[test]
    fn latest_collapses_repeated_snapshots_to_the_newest_status() {
        let dir = tempfile::tempdir().unwrap();
        append(
            dir.path(),
            &sample("promotion-1", PromotionStatus::Requested),
        )
        .unwrap();
        append(dir.path(), &sample("promotion-1", PromotionStatus::Planned)).unwrap();
        append(
            dir.path(),
            &sample("promotion-2", PromotionStatus::Requested),
        )
        .unwrap();
        let latest = latest(dir.path()).unwrap();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].status, PromotionStatus::Planned);
        assert_eq!(
            get(dir.path(), "promotion-2").unwrap().unwrap().status,
            PromotionStatus::Requested
        );
    }
}
