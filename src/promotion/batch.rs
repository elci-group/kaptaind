//! Multi-repository lifecycle orchestration and cross-repository provenance
//! (ELCI KAPTAIND-RTL-001 §34 Phase 4).
//!
//! A batch coordinates the *same* transition across several repositories,
//! each independently governed by its own policy, eligibility, and
//! promotion record. Git has no cross-repository atomic-commit primitive,
//! so Kaptaind does not invent one: a batch cannot promise all-or-nothing
//! across repositories. What it does provide is the manifest the directive
//! actually asks for — a single, auditable record tying each repository's
//! own `promotion-<uuid>` (in *its own* `.kaptaind/promotions.jsonl`) to one
//! coordinated request, which is what "cross-repository provenance" means
//! in a world without distributed transactions: you can always trace from
//! the batch back to exactly what happened, and did not happen, in each
//! member repository.

use super::engine;
use super::model::PromotionStatus;
use super::policy::LifecyclePolicy;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn batches_dir(coordinator_repo: &Path) -> PathBuf {
    coordinator_repo.join(".kaptaind").join("batches")
}

fn batch_path(coordinator_repo: &Path, id: &str) -> PathBuf {
    batches_dir(coordinator_repo).join(format!("{id}.json"))
}

/// One repository's part of a batch. `repository` is an absolute path: the
/// cross-repository provenance anchor a reader follows to that repo's own
/// `.kaptaind/promotions.jsonl` for the full evidence trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMember {
    pub repository: String,
    pub promotion_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionBatch {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub from: String,
    pub to: String,
    pub members: Vec<BatchMember>,
}

impl PromotionBatch {
    pub fn all_completed(&self) -> bool {
        !self.members.is_empty()
            && self
                .members
                .iter()
                .all(|member| member.status == "completed")
    }

    pub fn any_failed(&self) -> bool {
        self.members.iter().any(|member| {
            member.error.is_some()
                || matches!(
                    member.status.as_str(),
                    "failed" | "blocked" | "recovery-required" | "error"
                )
        })
    }
}

fn status_label(status: PromotionStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{status:?}"))
}

fn save(coordinator_repo: &Path, batch: &PromotionBatch) -> Result<()> {
    let dir = batches_dir(coordinator_repo);
    std::fs::create_dir_all(&dir)?;
    let path = batch_path(coordinator_repo, &batch.id);
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(batch)?)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

fn load(coordinator_repo: &Path, id: &str) -> Result<PromotionBatch> {
    let path = batch_path(coordinator_repo, id);
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("unknown batch `{id}`"))?;
    serde_json::from_str(&content).with_context(|| format!("malformed batch record `{id}`"))
}

/// Plan the same transition independently in each member repository. A
/// per-repository failure (missing branch, no policy match, ...) does not
/// abort the rest of the batch — it is recorded as that member's `error` so
/// the operator sees exactly which repositories are, and are not, ready.
pub fn plan_batch(
    coordinator_repo: &Path,
    repositories: &[PathBuf],
    from: &str,
    to: &str,
) -> Result<PromotionBatch> {
    anyhow::ensure!(
        !repositories.is_empty(),
        "a batch requires at least one repository"
    );
    let now = Utc::now();
    let mut members = Vec::with_capacity(repositories.len());
    for repo in repositories {
        let repository = repo.to_string_lossy().into_owned();
        let member = match LifecyclePolicy::load(repo)
            .and_then(|policy| engine::plan(repo, &policy, from, to))
        {
            Ok(promotion) => BatchMember {
                repository,
                promotion_id: Some(promotion.id),
                status: status_label(promotion.status),
                error: None,
            },
            Err(error) => BatchMember {
                repository,
                promotion_id: None,
                status: "error".to_string(),
                error: Some(error.to_string()),
            },
        };
        members.push(member);
    }
    let batch = PromotionBatch {
        id: format!("batch-{}", uuid::Uuid::new_v4()),
        created_at: now,
        updated_at: now,
        from: from.to_string(),
        to: to.to_string(),
        members,
    };
    save(coordinator_repo, &batch)?;
    crate::audit::log_event(
        coordinator_repo,
        "lifecycle",
        "batch.planned",
        !batch.any_failed(),
        serde_json::to_value(&batch)?,
    );
    Ok(batch)
}

/// Each member repository is independently configured, so its own
/// `kaptaind.toml` — not the coordinator's — decides its test/build gates.
/// A missing `kaptaind.toml` falls back to the same defaults `kaptaind`
/// itself uses for a repository with none.
fn member_gate_commands(repo: &Path) -> (Option<String>, Option<String>) {
    let path = repo.join("kaptaind.toml");
    let config = if path.exists() {
        crate::config::loader::load_from_path(&path).unwrap_or_default()
    } else {
        crate::config::loader::Config::default()
    };
    (config.test.command, config.build.command)
}

/// Validate every member that has a live promotion id. A member already in
/// error from planning is left untouched — a batch is never partially
/// re-planned on your behalf.
pub fn validate_batch(coordinator_repo: &Path, id: &str) -> Result<PromotionBatch> {
    let mut batch = load(coordinator_repo, id)?;
    for member in &mut batch.members {
        let Some(promotion_id) = member.promotion_id.clone() else {
            continue;
        };
        let repo = PathBuf::from(&member.repository);
        let (test_command, build_command) = member_gate_commands(&repo);
        match engine::validate(
            &repo,
            &promotion_id,
            test_command.as_deref(),
            build_command.as_deref(),
        ) {
            Ok(promotion) => {
                member.status = status_label(promotion.status);
                member.error = None;
            }
            Err(error) => {
                member.error = Some(error.to_string());
            }
        }
    }
    batch.updated_at = Utc::now();
    save(coordinator_repo, &batch)?;
    crate::audit::log_event(
        coordinator_repo,
        "lifecycle",
        "batch.validated",
        !batch.any_failed(),
        serde_json::to_value(&batch)?,
    );
    Ok(batch)
}

/// Execute every member that is ready. Each repository's promotion executes
/// independently and can fail independently — see the module docs on why a
/// batch cannot be all-or-nothing across repositories.
pub fn promote_batch(
    coordinator_repo: &Path,
    id: &str,
    approve: bool,
    dry_run: bool,
) -> Result<PromotionBatch> {
    let mut batch = load(coordinator_repo, id)?;
    for member in &mut batch.members {
        let Some(promotion_id) = member.promotion_id.clone() else {
            continue;
        };
        let repo = PathBuf::from(&member.repository);
        match engine::promote(&repo, &promotion_id, approve, dry_run) {
            Ok(promotion) => {
                member.status = status_label(promotion.status);
                member.error = None;
            }
            Err(error) => {
                member.error = Some(error.to_string());
            }
        }
    }
    batch.updated_at = Utc::now();
    save(coordinator_repo, &batch)?;
    crate::audit::log_event(
        coordinator_repo,
        "lifecycle",
        "batch.promoted",
        batch.all_completed(),
        serde_json::to_value(&batch)?,
    );
    Ok(batch)
}

pub fn status(coordinator_repo: &Path, id: &str) -> Result<PromotionBatch> {
    load(coordinator_repo, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn run_git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {:?} failed", args);
    }

    fn member_repo(feature: bool) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "-q"]);
        run_git(
            dir.path(),
            &["symbolic-ref", "HEAD", "refs/heads/development"],
        );
        run_git(dir.path(), &["config", "user.name", "Kaptaind Test"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        std::fs::write(dir.path().join("file.txt"), "base\n").unwrap();
        run_git(dir.path(), &["add", "."]);
        run_git(dir.path(), &["commit", "-q", "-m", "base"]);
        run_git(dir.path(), &["branch", "staging"]);
        run_git(dir.path(), &["branch", "main"]);
        std::fs::write(
            dir.path().join("kaptaind.toml"),
            "[test]\ncommand = \"true\"\n",
        )
        .unwrap();
        run_git(dir.path(), &["add", "kaptaind.toml"]);
        run_git(dir.path(), &["commit", "-q", "-m", "trivial test command"]);
        if feature {
            std::fs::write(dir.path().join("file.txt"), "base\nfeature\n").unwrap();
            run_git(dir.path(), &["commit", "-q", "-am", "feature"]);
        }
        dir
    }

    #[test]
    fn plan_validate_promote_a_batch_across_two_repositories() {
        let coordinator = tempfile::tempdir().unwrap();
        let repo_a = member_repo(true);
        let repo_b = member_repo(true);
        let repos = vec![repo_a.path().to_path_buf(), repo_b.path().to_path_buf()];

        let planned = plan_batch(coordinator.path(), &repos, "development", "staging").unwrap();
        assert_eq!(planned.members.len(), 2);
        assert!(planned.members.iter().all(|m| m.error.is_none()));
        assert!(!planned.any_failed());

        let validated = validate_batch(coordinator.path(), &planned.id).unwrap();
        assert!(validated.members.iter().all(|m| m.status == "validated"));

        let promoted = promote_batch(coordinator.path(), &planned.id, false, false).unwrap();
        assert!(promoted.all_completed());

        let reloaded = status(coordinator.path(), &planned.id).unwrap();
        assert_eq!(reloaded.id, planned.id);
        assert!(reloaded.all_completed());

        // Cross-repository provenance: each member's promotion id resolves
        // in *that* repository's own store, independent of the batch file.
        for member in &reloaded.members {
            let repo = PathBuf::from(&member.repository);
            let promotion_id = member.promotion_id.as_deref().unwrap();
            let promotion = engine::history(&repo, None)
                .unwrap()
                .into_iter()
                .find(|p| p.id == promotion_id)
                .unwrap();
            assert_eq!(promotion.status, PromotionStatus::Completed);
        }
    }

    #[test]
    fn one_repository_missing_the_transition_does_not_abort_the_others() {
        let coordinator = tempfile::tempdir().unwrap();
        let repo_ok = member_repo(true);
        let repo_missing_branch = tempfile::tempdir().unwrap();
        run_git(repo_missing_branch.path(), &["init", "-q"]);
        run_git(
            repo_missing_branch.path(),
            &["symbolic-ref", "HEAD", "refs/heads/development"],
        );
        run_git(
            repo_missing_branch.path(),
            &["config", "user.name", "Kaptaind Test"],
        );
        run_git(
            repo_missing_branch.path(),
            &["config", "user.email", "test@example.com"],
        );
        std::fs::write(repo_missing_branch.path().join("file.txt"), "base\n").unwrap();
        run_git(repo_missing_branch.path(), &["add", "."]);
        run_git(repo_missing_branch.path(), &["commit", "-q", "-m", "base"]);
        // No `staging` branch created here — planning this member must fail.

        let repos = vec![
            repo_ok.path().to_path_buf(),
            repo_missing_branch.path().to_path_buf(),
        ];
        let planned = plan_batch(coordinator.path(), &repos, "development", "staging").unwrap();
        assert!(planned.any_failed());
        assert!(planned.members[0].error.is_none());
        assert!(planned.members[1].error.is_some());
        assert_eq!(planned.members[1].status, "error");
    }
}
