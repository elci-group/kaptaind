//! Local, file-backed FIFO promotion queue (ELCI KAPTAIND-RTL-001 §34 Phase 4
//! "promotion queues").
//!
//! A queue entry is a *request* to plan a transition, not a plan itself.
//! [`super::model::PromotionPlan`] snapshots live repository revisions —
//! queuing a plan would let it go stale while it waits. Instead, a request
//! sits in the queue until [`drain`] is run, at which point it is planned
//! against whatever the repository actually looks like then. This also
//! means a queued request never becomes a second outstanding claim on the
//! same transition: it is only turned into a real (lock-holding) promotion
//! once the prior one has cleared.

use super::engine;
use super::model::Promotion;
use super::policy::LifecyclePolicy;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn queue_path(repo: &Path) -> PathBuf {
    repo.join(".kaptaind").join("lifecycle-queue.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedRequest {
    pub id: String,
    pub source: String,
    pub target: String,
    pub requested_at: DateTime<Utc>,
    pub requested_by: String,
    pub note: Option<String>,
}

fn load(repo: &Path) -> Result<Vec<QueuedRequest>> {
    let path = queue_path(repo);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("malformed lifecycle queue in {}", path.display()))
}

fn save(repo: &Path, queue: &[QueuedRequest]) -> Result<()> {
    let path = queue_path(repo);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(queue)?)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

/// Add a request for `source` -> `target`. Refused if that exact pair is
/// already queued (nothing gained by a duplicate request) or already has an
/// outstanding promotion (queuing would just have to refuse it again at
/// drain time — better to say so now).
pub fn enqueue(
    repo: &Path,
    policy: &LifecyclePolicy,
    source: &str,
    target: &str,
    note: Option<&str>,
) -> Result<QueuedRequest> {
    let resolved_source = policy.resolve_branch(source);
    let resolved_target = policy.resolve_branch(target);
    let mut queue = load(repo)?;
    if queue
        .iter()
        .any(|request| request.source == resolved_source && request.target == resolved_target)
    {
        anyhow::bail!("a request for `{resolved_source}` -> `{resolved_target}` is already queued");
    }
    if engine::has_outstanding(repo, &resolved_source, &resolved_target)? {
        anyhow::bail!(
            "`{resolved_source}` -> `{resolved_target}` already has an outstanding promotion; \
             plan/validate/promote/cancel it before queuing another request for the same transition"
        );
    }
    let request = QueuedRequest {
        id: format!("queued-{}", uuid::Uuid::new_v4()),
        source: resolved_source,
        target: resolved_target,
        requested_at: Utc::now(),
        requested_by: engine::actor(),
        note: note.map(str::to_owned),
    };
    queue.push(request.clone());
    save(repo, &queue)?;
    crate::audit::log_event(
        repo,
        "lifecycle",
        "promotion.queued",
        true,
        serde_json::to_value(&request)?,
    );
    Ok(request)
}

pub fn list(repo: &Path) -> Result<Vec<QueuedRequest>> {
    load(repo)
}

/// Remove a specific queued request without planning it.
pub fn remove(repo: &Path, id: &str) -> Result<QueuedRequest> {
    let mut queue = load(repo)?;
    let index = queue
        .iter()
        .position(|request| request.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown queued request `{id}`"))?;
    let removed = queue.remove(index);
    save(repo, &queue)?;
    crate::audit::log_event(
        repo,
        "lifecycle",
        "promotion.dequeued",
        true,
        serde_json::to_value(&removed)?,
    );
    Ok(removed)
}

/// The result of one [`drain`] pass.
#[derive(Debug, Clone, Serialize)]
pub struct DrainOutcome {
    pub planned: Vec<Promotion>,
    pub skipped: Vec<SkippedRequest>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedRequest {
    pub request: QueuedRequest,
    pub reason: String,
}

/// Plan every queued request whose transition has no outstanding promotion,
/// removing it from the queue on success (FIFO: oldest requests are tried
/// first). A request whose transition is still outstanding, or that no
/// longer plans cleanly (e.g. a branch was deleted), is left in the queue
/// and reported as skipped rather than silently dropped — the operator
/// decides whether to remove it.
pub fn drain(repo: &Path, policy: &LifecyclePolicy) -> Result<DrainOutcome> {
    let queue = load(repo)?;
    let mut remaining = Vec::new();
    let mut planned = Vec::new();
    let mut skipped = Vec::new();
    for request in queue {
        if engine::has_outstanding(repo, &request.source, &request.target)? {
            skipped.push(SkippedRequest {
                request: request.clone(),
                reason: "transition still outstanding".to_string(),
            });
            remaining.push(request);
            continue;
        }
        match engine::plan(repo, policy, &request.source, &request.target) {
            Ok(promotion) => planned.push(promotion),
            Err(error) => {
                skipped.push(SkippedRequest {
                    request: request.clone(),
                    reason: error.to_string(),
                });
                remaining.push(request);
            }
        }
    }
    save(repo, &remaining)?;
    Ok(DrainOutcome { planned, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::promotion::model::PromotionStatus;
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

    fn write_commit(dir: &Path, branch: &str, content: &str, message: &str) {
        run_git(dir, &["checkout", "-q", branch]);
        std::fs::write(dir.join("file.txt"), content).unwrap();
        run_git(dir, &["commit", "-q", "-am", message]);
    }

    fn repo() -> TempDir {
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
        dir
    }

    #[test]
    fn a_request_cannot_be_queued_twice_for_the_same_pair() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        enqueue(dir.path(), &policy, "development", "staging", Some("first")).unwrap();
        assert!(enqueue(dir.path(), &policy, "development", "staging", None).is_err());
        assert_eq!(list(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn a_request_cannot_be_queued_while_its_transition_already_has_a_live_promotion() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        engine::plan(dir.path(), &policy, "development", "staging").unwrap();
        let error = enqueue(dir.path(), &policy, "development", "staging", None).unwrap_err();
        assert!(error.to_string().contains("outstanding"));
        assert!(list(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn drain_plans_queued_requests_and_removes_them_on_success() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        enqueue(dir.path(), &policy, "development", "staging", None).unwrap();
        assert_eq!(list(dir.path()).unwrap().len(), 1);

        let outcome = drain(dir.path(), &policy).unwrap();
        assert_eq!(outcome.planned.len(), 1);
        assert!(outcome.skipped.is_empty());
        assert_eq!(outcome.planned[0].status, PromotionStatus::Planned);
        assert!(list(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn drain_leaves_a_request_queued_while_its_transition_is_outstanding() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let request = enqueue(dir.path(), &policy, "development", "staging", None).unwrap();

        // A live promotion appears for the same pair after the request was
        // queued (e.g. someone ran `lifecycle plan` directly in the
        // meantime) — drain must not try to plan on top of it.
        engine::plan(dir.path(), &policy, "development", "staging").unwrap();

        let outcome = drain(dir.path(), &policy).unwrap();
        assert!(outcome.planned.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].request.id, request.id);
        assert_eq!(list(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn remove_deletes_a_queued_request_without_planning_it() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let request = enqueue(dir.path(), &policy, "development", "staging", None).unwrap();
        remove(dir.path(), &request.id).unwrap();
        assert!(list(dir.path()).unwrap().is_empty());
        assert!(remove(dir.path(), &request.id).is_err());
    }
}
