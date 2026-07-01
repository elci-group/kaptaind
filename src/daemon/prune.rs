use chrono::Utc;
use std::path::Path;

pub struct PruneResult {
    pub deleted: usize,
    pub errors: usize,
}

/// Prune analysis artifacts older than the retention threshold.
/// Ported from web/lib/retention.ts.
pub async fn prune_analysis_artifacts(repo_path: &Path, retention_days: u32) -> PruneResult {
    let dir = repo_path.join(".kaptaind").join("analysis");
    if !dir.exists() || !dir.is_dir() {
        return PruneResult {
            deleted: 0,
            errors: 0,
        };
    }

    let cutoff = if retention_days == 0 {
        Utc::now()
    } else {
        Utc::now() - chrono::Duration::days(retention_days as i64)
    };

    let mut deleted = 0usize;
    let mut errors = 0usize;

    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return PruneResult {
            deleted: 0,
            errors: 0,
        };
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let should_delete =
                    if let Ok(artifact) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(ended_at) = artifact.get("ended_at").and_then(|v| v.as_str()) {
                            chrono::DateTime::parse_from_rfc3339(ended_at)
                                .map(|dt| dt.with_timezone(&chrono::Utc) <= cutoff)
                                .unwrap_or(true)
                        } else {
                            true // missing ended_at → delete
                        }
                    } else {
                        true // unparseable → delete
                    };

                if should_delete {
                    if tokio::fs::remove_file(&path).await.is_ok() {
                        deleted += 1;
                    } else {
                        errors += 1;
                    }
                }
            }
            Err(_) => {
                errors += 1;
            }
        }
    }

    PruneResult { deleted, errors }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn prunes_old_artifacts() {
        let dir = tempdir().unwrap();
        let analysis_dir = dir.path().join(".kaptaind").join("analysis");
        std::fs::create_dir_all(&analysis_dir).unwrap();

        let old = Utc::now() - chrono::Duration::days(10);
        std::fs::write(
            analysis_dir.join("old.json"),
            format!(r#"{{"ended_at":"{}"}}"#, old.to_rfc3339()),
        )
        .unwrap();

        let recent = Utc::now() - chrono::Duration::days(1);
        std::fs::write(
            analysis_dir.join("recent.json"),
            format!(r#"{{"ended_at":"{}"}}"#, recent.to_rfc3339()),
        )
        .unwrap();

        let result = prune_analysis_artifacts(dir.path(), 5).await;
        assert_eq!(result.deleted, 1);
        assert_eq!(result.errors, 0);
        assert!(!analysis_dir.join("old.json").exists());
        assert!(analysis_dir.join("recent.json").exists());
    }

    #[tokio::test]
    async fn retention_zero_deletes_everything() {
        let dir = tempdir().unwrap();
        let analysis_dir = dir.path().join(".kaptaind").join("analysis");
        std::fs::create_dir_all(&analysis_dir).unwrap();

        let recent = Utc::now() - chrono::Duration::hours(1);
        std::fs::write(
            analysis_dir.join("recent.json"),
            format!(r#"{{"ended_at":"{}"}}"#, recent.to_rfc3339()),
        )
        .unwrap();

        let result = prune_analysis_artifacts(dir.path(), 0).await;
        assert_eq!(result.deleted, 1);
        assert_eq!(result.errors, 0);
    }

    #[tokio::test]
    async fn missing_dir_returns_zero() {
        let dir = tempdir().unwrap();
        let result = prune_analysis_artifacts(dir.path(), 30).await;
        assert_eq!(result.deleted, 0);
        assert_eq!(result.errors, 0);
    }
}
