//! Lifecycle inspection, planning, validation, execution, and recovery
//! (ELCI KAPTAIND-RTL-001). Eligibility is always established before
//! preference, and no mutation runs without first re-checking that the plan
//! still matches reality (§8, §26).

use super::feed;
use super::git;
use super::model::{
    BranchSnapshot, Eligibility, Evidence, EvidenceKind, Operation, Promotion, PromotionEvent,
    PromotionPlan, PromotionStatus, ValidationGate,
};
use super::policy::{LifecyclePolicy, TransitionPolicy};
use super::store;
use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct InspectionReport {
    pub repository: String,
    pub branches: Vec<BranchSnapshot>,
    pub transitions: Vec<Eligibility>,
}

/// Non-mutating survey of configured branches and every configured
/// transition's eligibility (ELCI KAPTAIND-RTL-001 §21 `inspect`).
pub fn inspect(repo: &Path, policy: &LifecyclePolicy) -> Result<InspectionReport> {
    let mut branches = Vec::new();
    for branch_policy in &policy.branches {
        let Some(name) = &branch_policy.branch else {
            continue;
        };
        branches.push(BranchSnapshot {
            branch: name.clone(),
            role: branch_policy.role.clone(),
            protected: branch_policy.protected,
            commit: git::branch_commit(repo, name)?,
        });
    }
    let outstanding = outstanding_promotions(repo)?;
    let mut transitions = Vec::new();
    for transition in &policy.transitions {
        let (Some(source), Some(target)) = (
            policy.branch_for_role(&transition.from),
            policy.branch_for_role(&transition.to),
        ) else {
            continue;
        };
        transitions.push(eligibility(
            repo,
            policy,
            transition,
            source,
            target,
            &outstanding,
        )?);
    }
    Ok(InspectionReport {
        repository: repo.to_string_lossy().into_owned(),
        branches,
        transitions,
    })
}

/// Promotions that still hold an exclusive claim on their transition (ELCI
/// KAPTAIND-RTL-001 §8 "outstanding promotions").
fn outstanding_promotions(repo: &Path) -> Result<Vec<Promotion>> {
    Ok(store::latest(repo)?
        .into_iter()
        .filter(|promotion| promotion.status.holds_transition_lock())
        .collect())
}

/// True if `source` -> `target` already has a non-terminal promotion. Used
/// by [`super::queue::drain`] to decide whether a queued request can be
/// planned yet without duplicating [`plan`]'s own refusal logic.
pub fn has_outstanding(repo: &Path, source: &str, target: &str) -> Result<bool> {
    Ok(outstanding_promotions(repo)?.into_iter().any(|promotion| {
        promotion.plan.source_branch == source && promotion.plan.target_branch == target
    }))
}

fn eligibility(
    repo: &Path,
    policy: &LifecyclePolicy,
    transition: &TransitionPolicy,
    source: &str,
    target: &str,
    outstanding: &[Promotion],
) -> Result<Eligibility> {
    let mut blocking = Vec::new();
    let mut warnings = Vec::new();
    let mut evidence = Vec::new();

    let source_commit = git::branch_commit(repo, source)?;
    evidence.push(Evidence::new(
        EvidenceKind::Fact,
        "source-exists",
        source_commit.is_some(),
        "git rev-parse",
    ));
    if source_commit.is_none() {
        blocking.push(format!("source branch `{source}` does not exist"));
    }
    let target_commit = git::branch_commit(repo, target)?;
    evidence.push(Evidence::new(
        EvidenceKind::Fact,
        "target-exists",
        target_commit.is_some(),
        "git rev-parse",
    ));
    if target_commit.is_none() {
        blocking.push(format!("target branch `{target}` does not exist"));
    }
    if let (Some(source_commit), Some(target_commit)) = (&source_commit, &target_commit) {
        let ancestor = git::is_ancestor(repo, target_commit, source_commit)?;
        evidence.push(Evidence::new(
            EvidenceKind::Observation,
            "target-is-ancestor-of-source",
            ancestor,
            "git merge-base --is-ancestor",
        ));
        if !ancestor && matches!(transition.operation, Operation::FastForward) {
            blocking.push(format!(
                "`{source}` and `{target}` have diverged; fast-forward is not possible"
            ));
        }
        let conflicts = git::conflict_count(repo, source_commit, target_commit).unwrap_or(0);
        evidence.push(
            Evidence::new(
                EvidenceKind::Observation,
                "conflict-free-merge",
                conflicts == 0,
                "git merge-tree",
            )
            .detail(conflicts.to_string()),
        );
        if conflicts > 0 {
            blocking.push(format!("{conflicts} path(s) would conflict"));
        }
    }
    let clean = git::is_clean(repo)?;
    evidence.push(Evidence::new(
        EvidenceKind::Observation,
        "working-tree-clean",
        clean,
        "git status --porcelain",
    ));
    if !clean
        && transition
            .requires
            .iter()
            .any(|requirement| requirement == "clean_source" || requirement == "clean_target")
    {
        blocking.push("working tree has uncommitted changes".to_string());
    }
    if policy.is_protected(target) {
        warnings.push(format!(
            "target `{target}` is protected; promotion will require explicit approval"
        ));
    }
    let conflicting_promotion = outstanding.iter().find(|promotion| {
        promotion.plan.source_branch == source && promotion.plan.target_branch == target
    });
    evidence.push(Evidence::new(
        EvidenceKind::Observation,
        "no-outstanding-promotion",
        conflicting_promotion.is_none(),
        "promotion store",
    ));
    if let Some(existing) = conflicting_promotion {
        blocking.push(format!(
            "promotion `{}` is already outstanding for this transition ({:?})",
            existing.id, existing.status
        ));
    }
    Ok(Eligibility {
        transition: format!("{}->{}", transition.from, transition.to),
        source_branch: source.to_string(),
        target_branch: target.to_string(),
        eligible: blocking.is_empty(),
        blocking_reasons: blocking,
        warnings,
        evidence,
    })
}

/// Compute a promotion plan without mutating the repository (ELCI
/// KAPTAIND-RTL-001 §9 `plan`). The plan is persisted immediately so its id
/// is stable across the later `validate`/`promote` calls.
pub fn plan(repo: &Path, policy: &LifecyclePolicy, from: &str, to: &str) -> Result<Promotion> {
    let source = policy.resolve_branch(from);
    let target = policy.resolve_branch(to);
    let source_role = policy.role_for_branch(&source);
    let target_role = policy.role_for_branch(&target);
    let transition = policy
        .transition_for(&source_role, &target_role)
        .ok_or_else(|| {
            anyhow!(
                "no policy permits `{source_role}` -> `{target_role}` (source `{source}`, target `{target}`)"
            )
        })?
        .clone();
    if let Some(existing) = outstanding_promotions(repo)?.into_iter().find(|promotion| {
        promotion.plan.source_branch == source && promotion.plan.target_branch == target
    }) {
        bail!(
            "promotion `{}` is already outstanding for `{source}` -> `{target}` (status `{:?}`); \
             validate/promote/cancel it before planning another",
            existing.id,
            existing.status
        );
    }
    let source_commit = git::branch_commit(repo, &source)?
        .ok_or_else(|| anyhow!("source branch `{source}` does not exist"))?;
    let target_commit = git::branch_commit(repo, &target)?
        .ok_or_else(|| anyhow!("target branch `{target}` does not exist"))?;
    let commits = git::commit_count(repo, &source, &target)?;
    let stat = git::diff_stat(repo, &source, &target)?;
    let conflicts = git::conflict_count(repo, &source_commit, &target_commit)?;
    // Resolve a configured `auto` strategy against real ancestry now, so the
    // persisted plan always records a concrete decision rather than a
    // deferred one (ELCI KAPTAIND-RTL-001 §11).
    let operation = if transition.operation == Operation::Auto {
        if git::is_ancestor(repo, &target_commit, &source_commit)? {
            Operation::FastForward
        } else {
            Operation::Merge
        }
    } else {
        transition.operation
    };
    // A protected target always requires explicit approval, independent of
    // whatever the transition's own `requires` list says. `protected` would
    // otherwise be a warning-only label a misconfigured transition could
    // silently bypass (ELCI KAPTAIND-RTL-001 §12).
    let mut requires = transition.requires;
    if policy.is_protected(&target) && !requires.iter().any(|requirement| requirement == "approval")
    {
        requires.push("approval".to_string());
    }
    let plan = PromotionPlan {
        repository: repo.to_string_lossy().into_owned(),
        transition: format!("{source_role}->{target_role}"),
        source_branch: source,
        target_branch: target,
        source_role,
        target_role,
        source_revision: source_commit,
        target_revision: target_commit,
        commits,
        files_changed: stat.files,
        insertions: stat.insertions,
        deletions: stat.deletions,
        conflicts,
        operation,
        requires,
    };
    let now = Utc::now();
    let mut promotion = Promotion {
        id: format!("promotion-{}", uuid::Uuid::new_v4()),
        repository: plan.repository.clone(),
        plan,
        status: PromotionStatus::Requested,
        gates: Vec::new(),
        approved_by: None,
        executed_operation: None,
        result_commit: None,
        failure_reason: None,
        recovery_action: None,
        created_at: now,
        updated_at: now,
        history: vec![PromotionEvent {
            at: now,
            status: PromotionStatus::Requested,
            note: None,
        }],
    };
    feed::record(repo, &promotion, "promotion.requested");
    promotion.transition_to(PromotionStatus::Inspected, None);
    if promotion.plan.conflicts > 0 {
        promotion.transition_to(
            PromotionStatus::Blocked,
            Some(format!("{} conflicting path(s)", promotion.plan.conflicts)),
        );
    } else {
        promotion.transition_to(PromotionStatus::Planned, None);
    }
    store::append(repo, &promotion)?;
    crate::audit::log_event(
        repo,
        "lifecycle",
        "promotion.planned",
        promotion.status == PromotionStatus::Planned,
        serde_json::to_value(&promotion)?,
    );
    feed::record(repo, &promotion, "promotion.planned");
    Ok(promotion)
}

fn command_gate(repo: &Path, name: &str, command: &str) -> ValidationGate {
    match std::process::Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(repo)
        .output()
    {
        Ok(output) => ValidationGate {
            name: name.into(),
            passed: output.status.success(),
            detail: if output.status.success() {
                "passed".into()
            } else {
                String::from_utf8_lossy(&output.stderr).trim().to_owned()
            },
        },
        Err(error) => ValidationGate {
            name: name.into(),
            passed: false,
            detail: error.to_string(),
        },
    }
}

fn concurrency_check(repo: &Path, promotion: &Promotion) -> Result<bool> {
    let source_now = git::branch_commit(repo, &promotion.plan.source_branch)?;
    let target_now = git::branch_commit(repo, &promotion.plan.target_branch)?;
    Ok(
        source_now.as_deref() == Some(promotion.plan.source_revision.as_str())
            && target_now.as_deref() == Some(promotion.plan.target_revision.as_str()),
    )
}

/// Run configured validation gates against a planned promotion (ELCI
/// KAPTAIND-RTL-001 §10 `validate`). Re-verifies the plan's revisions first;
/// a moved source or target invalidates the plan rather than silently
/// validating stale assumptions (§26).
pub fn validate(
    repo: &Path,
    id: &str,
    test_command: Option<&str>,
    build_command: Option<&str>,
) -> Result<Promotion> {
    let mut promotion = store::get(repo, id)?.ok_or_else(|| anyhow!("unknown promotion `{id}`"))?;
    if !matches!(
        promotion.status,
        PromotionStatus::Planned | PromotionStatus::Blocked
    ) {
        bail!(
            "promotion `{id}` is `{:?}` and cannot be (re)validated",
            promotion.status
        );
    }
    if !concurrency_check(repo, &promotion)? {
        promotion.transition_to(
            PromotionStatus::Blocked,
            Some("source or target moved since planning; re-plan required".into()),
        );
        store::append(repo, &promotion)?;
        bail!(
            "plan invalidated: `{}`/`{}` moved since planning; run `lifecycle plan` again",
            promotion.plan.source_branch,
            promotion.plan.target_branch
        );
    }
    let mut gates = vec![
        ValidationGate {
            name: "working-tree-clean".into(),
            passed: git::is_clean(repo)?,
            detail: "git status --porcelain".into(),
        },
        ValidationGate {
            name: "conflict-free".into(),
            passed: promotion.plan.conflicts == 0,
            detail: format!("{} conflicting path(s)", promotion.plan.conflicts),
        },
    ];
    if promotion
        .plan
        .requires
        .iter()
        .any(|requirement| requirement == "validation")
    {
        if let Some(command) = test_command {
            gates.push(command_gate(repo, "tests", command));
        }
        if let Some(command) = build_command {
            gates.push(command_gate(repo, "build", command));
        }
    }
    let passed = gates.iter().all(|gate| gate.passed);
    promotion.gates = gates;
    if passed {
        promotion.transition_to(PromotionStatus::Validated, None);
    } else {
        promotion.transition_to(
            PromotionStatus::Blocked,
            Some("validation gate failed".into()),
        );
    }
    store::append(repo, &promotion)?;
    crate::audit::log_event(
        repo,
        "lifecycle",
        "promotion.validated",
        passed,
        serde_json::to_value(&promotion)?,
    );
    feed::record(repo, &promotion, "promotion.validated");
    if !passed {
        bail!("promotion `{id}` failed validation");
    }
    Ok(promotion)
}

fn execute(repo: &Path, plan: &PromotionPlan) -> Result<String> {
    match plan.operation {
        Operation::FastForward => {
            if !git::is_ancestor(repo, &plan.target_revision, &plan.source_revision)? {
                bail!(
                    "fast-forward requires `{}` to be an ancestor of `{}`",
                    plan.target_branch,
                    plan.source_branch
                );
            }
            git::compare_and_swap_branch(
                repo,
                &plan.target_branch,
                &plan.target_revision,
                &plan.source_revision,
            )?;
            Ok(plan.source_revision.clone())
        }
        Operation::Merge => {
            let message = format!(
                "kaptaind lifecycle: merge {} into {}",
                plan.source_branch, plan.target_branch
            );
            let commit = git::merge_commit(
                repo,
                &plan.source_branch,
                &plan.target_branch,
                &plan.source_revision,
                &plan.target_revision,
                &message,
            )?;
            git::compare_and_swap_branch(
                repo,
                &plan.target_branch,
                &plan.target_revision,
                &commit,
            )?;
            Ok(commit)
        }
        Operation::Rebase | Operation::CherryPick => {
            bail!(
                "`{}` promotion operation is not yet implemented",
                plan.operation
            )
        }
        Operation::Auto => {
            // `plan` always resolves `auto` to a concrete operation before
            // persisting; reaching this means a plan was hand-edited or a
            // future planning path forgot to resolve it — either way it is
            // a bug, not a policy or repository-state failure.
            bail!("promotion plan reached execution with an unresolved `auto` operation")
        }
    }
}

/// Execute (or, with `dry_run`, simulate) a validated promotion (ELCI
/// KAPTAIND-RTL-001 §11 `promote`). Approval is never inferred from
/// validation passing: a transition whose policy `requires` "approval" stops
/// at `AwaitingApproval` until `--approve` is passed explicitly (§10, §12).
pub fn promote(repo: &Path, id: &str, approve: bool, dry_run: bool) -> Result<Promotion> {
    let mut promotion = store::get(repo, id)?.ok_or_else(|| anyhow!("unknown promotion `{id}`"))?;
    if !matches!(
        promotion.status,
        PromotionStatus::Validated | PromotionStatus::AwaitingApproval
    ) {
        bail!(
            "promotion `{id}` is `{:?}`; run `lifecycle validate` first",
            promotion.status
        );
    }
    let needs_approval = promotion
        .plan
        .requires
        .iter()
        .any(|requirement| requirement == "approval");
    if needs_approval && !approve {
        promotion.transition_to(
            PromotionStatus::AwaitingApproval,
            Some("policy requires --approve".into()),
        );
        store::append(repo, &promotion)?;
        return Ok(promotion);
    }
    if needs_approval {
        promotion.approved_by = Some(actor());
        promotion.transition_to(PromotionStatus::Approved, None);
        store::append(repo, &promotion)?;
        crate::audit::log_event(
            repo,
            "lifecycle",
            "promotion.approved",
            true,
            serde_json::to_value(&promotion)?,
        );
        feed::record(repo, &promotion, "promotion.approved");
    }
    if !concurrency_check(repo, &promotion)? {
        promotion.transition_to(
            PromotionStatus::Blocked,
            Some("source or target moved since validation; re-plan required".into()),
        );
        store::append(repo, &promotion)?;
        bail!("plan invalidated: `{id}` must be re-planned");
    }
    if dry_run {
        return Ok(promotion);
    }
    if git::current_branch(repo)? == promotion.plan.target_branch {
        bail!(
            "refusing to promote onto the checked-out branch `{}`; switch away first",
            promotion.plan.target_branch
        );
    }
    promotion.transition_to(PromotionStatus::Executing, None);
    store::append(repo, &promotion)?;
    crate::audit::log_event(
        repo,
        "lifecycle",
        "promotion.started",
        true,
        serde_json::to_value(&promotion)?,
    );
    feed::record(repo, &promotion, "promotion.started");

    match execute(repo, &promotion.plan) {
        Ok(new_commit) => {
            promotion.executed_operation = Some(promotion.plan.operation);
            promotion.result_commit = Some(new_commit.clone());
            promotion.transition_to(PromotionStatus::Verifying, None);
            let verified = git::branch_commit(repo, &promotion.plan.target_branch)?.as_deref()
                == Some(new_commit.as_str());
            if verified {
                promotion.transition_to(PromotionStatus::Completed, None);
                store::append(repo, &promotion)?;
                crate::audit::log_event(
                    repo,
                    "lifecycle",
                    "promotion.completed",
                    true,
                    serde_json::to_value(&promotion)?,
                );
                feed::record(repo, &promotion, "promotion.completed");
            } else {
                promotion.failure_reason = Some("target ref did not verify after update".into());
                promotion.transition_to(PromotionStatus::RecoveryRequired, None);
                store::append(repo, &promotion)?;
                crate::audit::log_event(
                    repo,
                    "lifecycle",
                    "promotion.failed",
                    false,
                    serde_json::to_value(&promotion)?,
                );
                feed::record(repo, &promotion, "promotion.failed");
                bail!(
                    "promotion `{id}` executed but did not verify; status is now recovery-required"
                );
            }
        }
        Err(error) => {
            promotion.failure_reason = Some(error.to_string());
            promotion.transition_to(PromotionStatus::Failed, None);
            store::append(repo, &promotion)?;
            crate::audit::log_event(
                repo,
                "lifecycle",
                "promotion.failed",
                false,
                serde_json::to_value(&promotion)?,
            );
            feed::record(repo, &promotion, "promotion.failed");
            return Err(error);
        }
    }
    Ok(promotion)
}

pub(super) fn actor() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

/// Active (non-terminal) promotions.
pub fn status(repo: &Path) -> Result<Vec<Promotion>> {
    Ok(store::latest(repo)?
        .into_iter()
        .filter(|promotion| !promotion.status.is_terminal())
        .collect())
}

/// Every promotion's latest snapshot, most recently created first.
pub fn history(repo: &Path, limit: Option<usize>) -> Result<Vec<Promotion>> {
    let mut promotions = store::latest(repo)?;
    promotions.sort_by_key(|promotion| std::cmp::Reverse(promotion.created_at));
    if let Some(limit) = limit {
        promotions.truncate(limit);
    }
    Ok(promotions)
}

/// Explicitly abandon a promotion that no longer needs to complete, releasing
/// its hold on the transition (ELCI KAPTAIND-RTL-001 §15 `CANCELLED`) so a
/// fresh `plan` for the same source/target is no longer refused as
/// outstanding. Unlike [`recover`], this never inspects or touches the
/// repository — it only records the decision.
pub fn cancel(repo: &Path, id: &str, reason: Option<&str>) -> Result<Promotion> {
    let mut promotion = store::get(repo, id)?.ok_or_else(|| anyhow!("unknown promotion `{id}`"))?;
    if promotion.status.is_terminal() {
        bail!("promotion `{id}` is already `{:?}`", promotion.status);
    }
    promotion.transition_to(PromotionStatus::Cancelled, reason.map(str::to_owned));
    store::append(repo, &promotion)?;
    crate::audit::log_event(
        repo,
        "lifecycle",
        "promotion.cancelled",
        true,
        serde_json::to_value(&promotion)?,
    );
    feed::record(repo, &promotion, "promotion.cancelled");
    Ok(promotion)
}

/// Reconcile a failed or recovery-required promotion (ELCI KAPTAIND-RTL-001
/// §27). This is recovery, not rollback: it never rewrites history or forces
/// a ref. It only checks whether reality now matches what execution expected
/// and reports what remains true if not.
pub fn recover(repo: &Path, id: &str) -> Result<Promotion> {
    let mut promotion = store::get(repo, id)?.ok_or_else(|| anyhow!("unknown promotion `{id}`"))?;
    if !matches!(
        promotion.status,
        PromotionStatus::RecoveryRequired | PromotionStatus::Failed
    ) {
        bail!(
            "promotion `{id}` is `{:?}`; nothing to recover",
            promotion.status
        );
    }
    let actual = git::branch_commit(repo, &promotion.plan.target_branch)?;
    let expected = promotion.result_commit.clone();
    let exact_match = expected.is_some() && actual == expected;
    // Sophisticated reconciliation (ELCI KAPTAIND-RTL-001 §24, §27): an
    // exact commit match is not the only way the promotion's actual goal —
    // getting `source_revision` into the target's history — can already be
    // satisfied. If some other process (a manual `git merge`, a retried
    // promotion, a hand-applied fix) has since made the target a descendant
    // of what we planned to promote, the intent is met even though the
    // resulting commit hash differs from what this promotion executed.
    // Every branch here is a deterministic ancestry check, never a guess.
    let goal_achieved_by_other_means = !exact_match
        && actual.as_deref().is_some_and(|actual_commit| {
            git::is_ancestor(repo, &promotion.plan.source_revision, actual_commit).unwrap_or(false)
        });
    let reconciled = exact_match || goal_achieved_by_other_means;
    if exact_match {
        promotion.recovery_action = Some(format!(
            "target `{}` is already at the expected commit; marking recovered \
             (strategy: deterministic-exact-match)",
            promotion.plan.target_branch
        ));
        let note = promotion.recovery_action.clone();
        promotion.transition_to(PromotionStatus::Completed, note);
    } else if goal_achieved_by_other_means {
        promotion.recovery_action = Some(format!(
            "target `{}` now contains the planned source revision via a different commit \
             than this promotion executed; marking recovered \
             (strategy: deterministic-ancestry-equivalence)",
            promotion.plan.target_branch
        ));
        let note = promotion.recovery_action.clone();
        promotion.transition_to(PromotionStatus::Completed, note);
    } else {
        promotion.recovery_action = Some(format!(
            "target `{}` is at {actual:?}, expected {expected:?}; no destructive action was taken — resolve manually and re-plan",
            promotion.plan.target_branch
        ));
    }
    store::append(repo, &promotion)?;
    crate::audit::log_event(
        repo,
        "lifecycle",
        "promotion.recovered",
        reconciled,
        serde_json::to_value(&promotion)?,
    );
    feed::record(repo, &promotion, "promotion.recovered");
    Ok(promotion)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::promotion::policy::LifecyclePolicy;
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

    /// A repo with `development`, `staging`, and `main` all pointing at the
    /// same base commit, matching the zero-config default policy's roles.
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
    fn plan_validate_promote_completes_a_merge_without_approval() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert_eq!(planned.status, PromotionStatus::Planned);
        assert_eq!(planned.plan.operation, Operation::Merge);
        assert_eq!(planned.plan.commits, 1);

        let validated = validate(dir.path(), &planned.id, None, None).unwrap();
        assert_eq!(validated.status, PromotionStatus::Validated);

        let promoted = promote(dir.path(), &planned.id, false, false).unwrap();
        assert_eq!(promoted.status, PromotionStatus::Completed);
        assert_eq!(
            git::branch_commit(dir.path(), "staging").unwrap(),
            promoted.result_commit
        );
    }

    #[test]
    fn promotion_requiring_approval_stops_until_explicitly_authorised() {
        let dir = repo();
        write_commit(dir.path(), "staging", "base\nstaged\n", "staged change");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "staging", "production").unwrap();
        assert!(planned.plan.requires.iter().any(|r| r == "approval"));
        validate(dir.path(), &planned.id, None, None).unwrap();

        let awaiting = promote(dir.path(), &planned.id, false, false).unwrap();
        assert_eq!(awaiting.status, PromotionStatus::AwaitingApproval);

        let completed = promote(dir.path(), &planned.id, true, false).unwrap();
        assert_eq!(completed.status, PromotionStatus::Completed);
        assert!(completed.approved_by.is_some());
    }

    #[test]
    fn validate_blocks_a_plan_invalidated_by_concurrent_mutation() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();

        write_commit(
            dir.path(),
            "staging",
            "base\nsurprise\n",
            "concurrent change",
        );

        let error = validate(dir.path(), &planned.id, None, None).unwrap_err();
        assert!(error.to_string().contains("plan invalidated"));
        let stored = store::get(dir.path(), &planned.id).unwrap().unwrap();
        assert_eq!(stored.status, PromotionStatus::Blocked);
    }

    #[test]
    fn conflicting_changes_block_planning_instead_of_executing() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\ndev-line\n", "dev change");
        write_commit(
            dir.path(),
            "staging",
            "base\nstaging-line\n",
            "staging change",
        );
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert!(planned.plan.conflicts > 0);
        assert_eq!(planned.status, PromotionStatus::Blocked);
    }

    #[test]
    fn a_failed_execution_never_reports_success_and_recovery_takes_no_destructive_action() {
        let dir = repo();
        // Diverge development and staging with non-conflicting changes to
        // distinct files, so planning succeeds but a fast-forward cannot.
        write_commit(dir.path(), "development", "base\ndev-only\n", "dev change");
        run_git(dir.path(), &["checkout", "-q", "staging"]);
        std::fs::write(dir.path().join("other.txt"), "staging-only\n").unwrap();
        run_git(dir.path(), &["add", "other.txt"]);
        run_git(dir.path(), &["commit", "-q", "-m", "staging change"]);
        run_git(dir.path(), &["checkout", "-q", "main"]);

        // Written after the branch setup: an untracked `kaptaind.toml` would
        // otherwise be swept up by a later `git add .` on another branch and
        // then deleted on checkout back to a branch that never tracked it.
        std::fs::write(
            dir.path().join("kaptaind.toml"),
            r#"
[lifecycle.branch.development]
role = "development"

[lifecycle.branch.staging]
role = "staging"

[lifecycle.transitions.development_to_staging]
from = "development"
to = "staging"
operation = "fast-forward"
requires = []
"#,
        )
        .unwrap();
        // Commit it (on the currently checked-out `main`, never touched by
        // the checkout-free engine below) so an untracked policy file does
        // not itself fail the "working tree clean" validation gate.
        run_git(dir.path(), &["add", "kaptaind.toml"]);
        run_git(dir.path(), &["commit", "-q", "-m", "policy"]);

        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert_eq!(planned.plan.operation, Operation::FastForward);
        assert_eq!(planned.plan.conflicts, 0);
        assert_eq!(planned.status, PromotionStatus::Planned);
        validate(dir.path(), &planned.id, None, None).unwrap();

        let error = promote(dir.path(), &planned.id, false, false).unwrap_err();
        assert!(error.to_string().contains("fast-forward"));
        let failed = store::get(dir.path(), &planned.id).unwrap().unwrap();
        assert_eq!(failed.status, PromotionStatus::Failed);

        let recovered = recover(dir.path(), &planned.id).unwrap();
        assert_eq!(recovered.status, PromotionStatus::Failed);
        assert!(recovered
            .recovery_action
            .as_deref()
            .unwrap()
            .contains("no destructive action"));
    }

    #[test]
    fn inspect_reports_branches_and_transition_eligibility() {
        let dir = repo();
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let report = inspect(dir.path(), &policy).unwrap();
        assert_eq!(report.branches.len(), 3);
        assert!(report
            .transitions
            .iter()
            .any(|t| t.transition == "development->staging" && t.eligible));
    }

    #[test]
    fn a_protected_target_requires_approval_even_if_policy_omits_it() {
        let dir = repo();
        std::fs::write(
            dir.path().join("kaptaind.toml"),
            r#"
[lifecycle.branch.development]
role = "development"

[lifecycle.branch.staging]
role = "staging"
protected = true

[lifecycle.transitions.development_to_staging]
from = "development"
to = "staging"
operation = "merge"
requires = []
"#,
        )
        .unwrap();
        run_git(dir.path(), &["add", "kaptaind.toml"]);
        run_git(dir.path(), &["commit", "-q", "-m", "policy"]);
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");

        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert!(planned.plan.requires.iter().any(|r| r == "approval"));

        validate(dir.path(), &planned.id, None, None).unwrap();
        let awaiting = promote(dir.path(), &planned.id, false, false).unwrap();
        assert_eq!(awaiting.status, PromotionStatus::AwaitingApproval);
    }

    #[test]
    fn a_second_plan_for_the_same_outstanding_transition_is_refused() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let first = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert_eq!(first.status, PromotionStatus::Planned);

        let report = inspect(dir.path(), &policy).unwrap();
        let eligibility = report
            .transitions
            .iter()
            .find(|t| t.transition == "development->staging")
            .unwrap();
        assert!(!eligibility.eligible);

        let error = plan(dir.path(), &policy, "development", "staging").unwrap_err();
        assert!(error.to_string().contains("already outstanding"));

        // Cancelling releases the lock so a fresh plan is accepted again.
        let cancelled = cancel(dir.path(), &first.id, Some("superseded")).unwrap();
        assert_eq!(cancelled.status, PromotionStatus::Cancelled);
        let second = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert_eq!(second.status, PromotionStatus::Planned);
    }

    #[test]
    fn cancel_rejects_an_already_terminal_promotion() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        validate(dir.path(), &planned.id, None, None).unwrap();
        let completed = promote(dir.path(), &planned.id, false, false).unwrap();
        assert_eq!(completed.status, PromotionStatus::Completed);
        assert!(cancel(dir.path(), &planned.id, None).is_err());
    }

    #[test]
    fn auto_strategy_resolves_to_fast_forward_when_possible_and_merge_when_not() {
        let dir = repo();
        std::fs::write(
            dir.path().join("kaptaind.toml"),
            r#"
[lifecycle.branch.development]
role = "development"

[lifecycle.branch.staging]
role = "staging"

[lifecycle.transitions.development_to_staging]
from = "development"
to = "staging"
operation = "auto"
requires = []
"#,
        )
        .unwrap();
        run_git(dir.path(), &["add", "kaptaind.toml"]);
        run_git(dir.path(), &["commit", "-q", "-m", "policy"]);

        // Trivially fast-forwardable: staging is an ancestor of development.
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let ff_plan = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert_eq!(ff_plan.plan.operation, Operation::FastForward);
        validate(dir.path(), &ff_plan.id, None, None).unwrap();
        let completed = promote(dir.path(), &ff_plan.id, false, false).unwrap();
        assert_eq!(completed.status, PromotionStatus::Completed);
        assert_eq!(
            completed.result_commit.as_deref(),
            Some(ff_plan.plan.source_revision.as_str())
        );

        // Diverged: staging has its own commit development never saw.
        run_git(dir.path(), &["checkout", "-q", "development"]);
        std::fs::write(dir.path().join("dev-only.txt"), "dev\n").unwrap();
        run_git(dir.path(), &["add", "dev-only.txt"]);
        run_git(dir.path(), &["commit", "-q", "-m", "dev-only change"]);
        run_git(dir.path(), &["checkout", "-q", "staging"]);
        std::fs::write(dir.path().join("staging-only.txt"), "staging\n").unwrap();
        run_git(dir.path(), &["add", "staging-only.txt"]);
        run_git(dir.path(), &["commit", "-q", "-m", "staging-only change"]);
        run_git(dir.path(), &["checkout", "-q", "main"]);

        let merge_plan = plan(dir.path(), &policy, "development", "staging").unwrap();
        assert_eq!(merge_plan.plan.operation, Operation::Merge);
    }

    #[test]
    fn recover_reconciles_when_target_reached_the_goal_via_a_different_commit() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        validate(dir.path(), &planned.id, None, None).unwrap();
        let completed = promote(dir.path(), &planned.id, false, false).unwrap();
        assert_eq!(completed.status, PromotionStatus::Completed);

        // Simulate a promotion whose recorded expectation no longer matches
        // the current tip, but whose actual goal (source_revision reachable
        // from target) was independently satisfied by the real promotion
        // above: hand-craft a Failed record pointing at a stale expectation.
        let mut stale = completed.clone();
        stale.status = PromotionStatus::Failed;
        stale.result_commit = Some("0000000000000000000000000000000000dead".to_string());
        stale.failure_reason = Some("synthetic failure for reconciliation test".to_string());
        store::append(dir.path(), &stale).unwrap();

        let recovered = recover(dir.path(), &stale.id).unwrap();
        assert_eq!(recovered.status, PromotionStatus::Completed);
        assert!(recovered
            .recovery_action
            .as_deref()
            .unwrap()
            .contains("ancestry-equivalence"));
    }
}
