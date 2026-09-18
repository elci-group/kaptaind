//! ELCI ecosystem integration (ELCI KAPTAIND-RTL-001 Phase 3, §17-§20):
//! Padagonia relationships, a Zebra/Vamos-consumable promotion event feed,
//! and Ingauge-shaped execution metrics.
//!
//! Every integration point here is capability-based and best-effort. The
//! deterministic lifecycle FSM in [`super::engine`] never depends on any of
//! this succeeding — a disk hiccup writing the local feed, a missing Vamos
//! webhook, or a disabled/unreachable Padagonia are all silently tolerated
//! (logged, never propagated) so Kaptaind remains fully functional without
//! any of these systems (§17 "SHALL remain functional without Padagonia",
//! extended here to the same standard for Zebra/Vamos/Ingauge).
//!
//! - **Zebra** (§18) and **Vamos** (§19) both consume the same artifact: a
//!   stable, versioned, append-only `.kaptaind/lifecycle-events.jsonl`.
//!   Kaptaind does not push to a specific Zebra/Vamos wire protocol because
//!   none is specified anywhere in this repository or its dependencies —
//!   inventing one would be indistinguishable from fabricating an
//!   integration that does not exist. What Kaptaind *can* honestly promise
//!   is the "stable machine-readable contract" the directive requires
//!   (§18), plus an optional generic webhook mirror of the same JSON for
//!   whatever endpoint the operator configures.
//! - **Ingauge** (§20) metrics are derived entirely from data Kaptaind
//!   already recorded (the promotion's own timestamped history and plan),
//!   never estimated or fabricated. `tokens_consumed` is always `0`: the
//!   core repository transition is deterministic and never invokes an LLM
//!   (§20, final sentence).
//! - **Padagonia** (§17) reuses the real, already-implemented
//!   [`crate::supervisor::padagonia::PadagoniaClient`] rather than a new
//!   bespoke client, since Padagonia has no edge/relationship API: lifecycle
//!   relationships are flattened into one node's properties.

use super::model::{Promotion, PromotionStatus, ValidationGate};
use super::policy::LifecyclePolicy;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const FEED_SCHEMA_VERSION: u8 = 1;

fn feed_path(repo: &Path) -> PathBuf {
    repo.join(".kaptaind").join("lifecycle-events.jsonl")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Who/what produced an event, and with which Kaptaind build (ELCI
/// KAPTAIND-RTL-001 §19 `provenance`, §29 determinism).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub actor: String,
    pub tool_version: String,
}

/// One lifecycle event, shaped exactly to ELCI KAPTAIND-RTL-001 §19's
/// minimum field list. This is the Zebra/Vamos-consumable contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionFeedEvent {
    pub schema_version: u8,
    pub event_id: String,
    pub kind: String,
    pub timestamp: DateTime<Utc>,
    pub repository: String,
    pub promotion_id: String,
    pub source_state: String,
    pub target_state: String,
    pub severity: Severity,
    pub result: String,
    pub evidence_count: usize,
    pub provenance: Provenance,
}

fn severity_for(status: PromotionStatus) -> Severity {
    match status {
        PromotionStatus::Failed | PromotionStatus::RecoveryRequired => Severity::Error,
        PromotionStatus::Blocked | PromotionStatus::AwaitingApproval => Severity::Warning,
        _ => Severity::Info,
    }
}

fn result_for(status: PromotionStatus) -> &'static str {
    match status {
        PromotionStatus::Completed => "success",
        PromotionStatus::AwaitingApproval => "pending",
        PromotionStatus::Blocked => "blocked",
        PromotionStatus::Failed | PromotionStatus::RecoveryRequired => "failure",
        PromotionStatus::Cancelled => "cancelled",
        PromotionStatus::Requested
        | PromotionStatus::Inspected
        | PromotionStatus::Planned
        | PromotionStatus::Validated
        | PromotionStatus::Approved
        | PromotionStatus::Executing
        | PromotionStatus::Verifying => "in-progress",
    }
}

fn build_event(promotion: &Promotion, kind: &str) -> PromotionFeedEvent {
    PromotionFeedEvent {
        schema_version: FEED_SCHEMA_VERSION,
        event_id: format!("event-{}", uuid::Uuid::new_v4()),
        kind: kind.to_string(),
        timestamp: Utc::now(),
        repository: promotion.repository.clone(),
        promotion_id: promotion.id.clone(),
        source_state: promotion.plan.source_role.clone(),
        target_state: promotion.plan.target_role.clone(),
        severity: severity_for(promotion.status),
        result: result_for(promotion.status).to_string(),
        evidence_count: promotion.gates.len(),
        provenance: Provenance {
            actor: "kaptaind-lifecycle".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
        },
    }
}

fn append_event(repo: &Path, event: &PromotionFeedEvent) -> Result<()> {
    let path = feed_path(repo);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(path)?;
    file.write_all(serde_json::to_string(event)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

/// Record one lifecycle transition to the local feed and, if configured,
/// fan it out to an optional Vamos/Zebra webhook and Padagonia. Never fails
/// the caller: every failure here is logged and swallowed, matching
/// [`crate::audit::log_event`]'s own non-fatal contract, so an integration
/// outage can never block a promotion (ELCI KAPTAIND-RTL-001 §17).
pub fn record(repo: &Path, promotion: &Promotion, kind: &str) {
    let event = build_event(promotion, kind);
    if let Err(error) = append_event(repo, &event) {
        tracing::warn!(error = %error, kind, "failed to write lifecycle feed event");
    }
    dispatch_optional_integrations(repo, promotion, &event);
}

/// Best-effort fan-out to a configured Vamos/Zebra webhook and to Padagonia.
/// Both legs require an active Tokio runtime (the CLI always runs inside
/// one); outside of one — e.g. a plain `#[test]` — they are silently
/// skipped rather than panicking, since this is optional observability, not
/// part of the deterministic transition itself.
fn dispatch_optional_integrations(repo: &Path, promotion: &Promotion, event: &PromotionFeedEvent) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    if let Ok(policy) = LifecyclePolicy::load(repo) {
        if let Some(webhook_url) = policy.feed_webhook_url {
            let event = event.clone();
            handle.spawn(async move {
                if let Err(error) = crate::util::http::validate_outbound_url(&webhook_url) {
                    tracing::warn!(error = %error, "refusing unsafe lifecycle feed webhook URL");
                    return;
                }
                if let Err(error) = crate::compliance::enforce_egress_url(
                    crate::config::loader::EgressChannel::Webhooks,
                    &webhook_url,
                ) {
                    tracing::warn!(error = %error, "regional policy blocked lifecycle feed webhook");
                    return;
                }
                let client = crate::util::http::hardened_client(std::time::Duration::from_secs(10));
                if let Err(error) = client.post(&webhook_url).json(&event).send().await {
                    tracing::warn!(error = %error, "failed to deliver lifecycle feed webhook");
                }
            });
        }
    }
    if let Ok(supervisor_config) = crate::supervisor::config::SupervisorConfig::load(None) {
        if let Ok(Some(client)) =
            crate::supervisor::padagonia::PadagoniaClient::from_config(&supervisor_config.padagonia)
        {
            let properties = padagonia_properties(promotion);
            let promotion_id = promotion.id.clone();
            handle.spawn(async move {
                if let Err(error) = client
                    .record_lifecycle_promotion(&promotion_id, properties)
                    .await
                {
                    tracing::warn!(error = %error, "failed to publish lifecycle promotion to Padagonia");
                }
            });
        }
    }
}

/// Flatten this promotion's branch/state relationships into Padagonia node
/// properties (ELCI KAPTAIND-RTL-001 §17). Padagonia's real API is a
/// node/property store with no edge concept, so `REPRESENTS`, `PROMOTES_TO`,
/// `CONTAINS`, and `EVIDENCED_BY` are all encoded here as plain fields
/// rather than as graph edges this service cannot accept.
fn padagonia_properties(promotion: &Promotion) -> Value {
    json!({
        "schema_version": FEED_SCHEMA_VERSION,
        "repository": promotion.repository,
        "promotion_id": promotion.id,
        "status": promotion.status,
        "transition": promotion.plan.transition,
        "source_branch": promotion.plan.source_branch,
        "target_branch": promotion.plan.target_branch,
        "represents_state": promotion.plan.target_role,
        "promotes_to": promotion.plan.target_branch,
        "operation": promotion.plan.operation,
        "commits": promotion.plan.commits,
        "files_changed": promotion.plan.files_changed,
        "insertions": promotion.plan.insertions,
        "deletions": promotion.plan.deletions,
        "conflicts": promotion.plan.conflicts,
        "requires": promotion.plan.requires,
        "gates": promotion.gates,
        "approved_by": promotion.approved_by,
        "result_commit": promotion.result_commit,
        "failure_reason": promotion.failure_reason,
        "created_at": promotion.created_at,
        "updated_at": promotion.updated_at,
    })
}

/// Every recorded feed event, oldest first. `since_event_id` returns only
/// events recorded after (not including) that event, for a consumer that
/// checkpoints its own read position (Zebra/Vamos poll this the way any
/// append-only log is tailed).
pub fn read_events(repo: &Path, since_event_id: Option<&str>) -> Result<Vec<PromotionFeedEvent>> {
    let path = feed_path(repo);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let events: Vec<PromotionFeedEvent> = std::fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(anyhow::Error::from))
        .collect::<Result<_>>()?;
    Ok(match since_event_id {
        None => events,
        Some(id) => match events.iter().position(|event| event.event_id == id) {
            Some(index) => events[index + 1..].to_vec(),
            None => events,
        },
    })
}

/// Execution metrics for one promotion (ELCI KAPTAIND-RTL-001 §20). Every
/// field is derived from data Kaptaind already recorded — the promotion's
/// own timestamped status history and plan — never estimated.
#[derive(Debug, Clone, Serialize)]
pub struct PromotionMetrics {
    pub promotion_id: String,
    pub planning_duration_ms: i64,
    pub validation_duration_ms: Option<i64>,
    pub execution_duration_ms: Option<i64>,
    pub commits: usize,
    pub files_changed: usize,
    pub conflicts_detected: usize,
    /// Always `0`: Kaptaind does not auto-resolve merge conflicts (a
    /// conflicting plan is `Blocked` before it ever reaches validation or
    /// execution, ELCI KAPTAIND-RTL-001 §24) until a resolver strategy
    /// exists.
    pub conflicts_resolved: usize,
    pub failed_validation_gates: usize,
    /// How many earlier promotions were created for this exact
    /// source/target branch pair before this one.
    pub retry_count: usize,
    pub success: bool,
    /// Always `0`: the core transition is deterministic and never invokes
    /// an LLM (§20).
    pub tokens_consumed: u64,
}

fn first_at(promotion: &Promotion, status: PromotionStatus) -> Option<DateTime<Utc>> {
    promotion
        .history
        .iter()
        .find(|event| event.status == status)
        .map(|event| event.at)
}

fn last_at(promotion: &Promotion, status: PromotionStatus) -> Option<DateTime<Utc>> {
    promotion
        .history
        .iter()
        .rev()
        .find(|event| event.status == status)
        .map(|event| event.at)
}

pub fn compute_metrics(repo: &Path, promotion: &Promotion) -> Result<PromotionMetrics> {
    let planned_or_blocked = first_at(promotion, PromotionStatus::Planned)
        .or_else(|| first_at(promotion, PromotionStatus::Blocked));
    let planning_duration_ms = planned_or_blocked
        .map(|at| (at - promotion.created_at).num_milliseconds())
        .unwrap_or(0);

    let validated_or_reblocked = last_at(promotion, PromotionStatus::Validated)
        .or_else(|| last_at(promotion, PromotionStatus::Blocked));
    let validation_duration_ms = match (planned_or_blocked, validated_or_reblocked) {
        (Some(start), Some(end)) if end >= start => Some((end - start).num_milliseconds()),
        _ => None,
    };

    let executing_at = first_at(promotion, PromotionStatus::Executing);
    let terminal_at = last_at(promotion, PromotionStatus::Completed)
        .or_else(|| last_at(promotion, PromotionStatus::Failed))
        .or_else(|| last_at(promotion, PromotionStatus::RecoveryRequired));
    let execution_duration_ms = match (executing_at, terminal_at) {
        (Some(start), Some(end)) if end >= start => Some((end - start).num_milliseconds()),
        _ => None,
    };

    let retry_count = super::store::latest(repo)?
        .into_iter()
        .filter(|other| {
            other.id != promotion.id
                && other.plan.source_branch == promotion.plan.source_branch
                && other.plan.target_branch == promotion.plan.target_branch
                && other.created_at < promotion.created_at
        })
        .count();

    Ok(PromotionMetrics {
        promotion_id: promotion.id.clone(),
        planning_duration_ms,
        validation_duration_ms,
        execution_duration_ms,
        commits: promotion.plan.commits,
        files_changed: promotion.plan.files_changed,
        conflicts_detected: promotion.plan.conflicts,
        conflicts_resolved: 0,
        failed_validation_gates: promotion
            .gates
            .iter()
            .filter(|gate: &&ValidationGate| !gate.passed)
            .count(),
        retry_count,
        success: promotion.status == PromotionStatus::Completed,
        tokens_consumed: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::promotion::policy::LifecyclePolicy;
    use crate::promotion::{cancel, plan, promote, validate};
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
    fn a_full_promotion_writes_the_expected_event_sequence() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        validate(dir.path(), &planned.id, None, None).unwrap();
        promote(dir.path(), &planned.id, false, false).unwrap();

        let events = read_events(dir.path(), None).unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec![
                "promotion.requested",
                "promotion.planned",
                "promotion.validated",
                "promotion.started",
                "promotion.completed",
            ]
        );
        assert!(events.iter().all(|event| event.promotion_id == planned.id));
        assert!(events
            .iter()
            .all(|event| event.schema_version == FEED_SCHEMA_VERSION));
        let completed = events.last().unwrap();
        assert_eq!(completed.result, "success");
        assert_eq!(completed.severity, Severity::Info);
    }

    #[test]
    fn read_events_since_returns_only_later_events() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        let all = read_events(dir.path(), None).unwrap();
        assert_eq!(all.len(), 2); // requested, planned

        let since_first = read_events(dir.path(), Some(&all[0].event_id)).unwrap();
        assert_eq!(since_first.len(), 1);
        assert_eq!(since_first[0].kind, "promotion.planned");

        cancel(dir.path(), &planned.id, None).unwrap();
        let since_last = read_events(dir.path(), Some(&all[1].event_id)).unwrap();
        assert_eq!(since_last.len(), 1);
        assert_eq!(since_last[0].kind, "promotion.cancelled");
    }

    #[test]
    fn blocked_planning_is_reported_as_a_warning_with_blocked_result() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\ndev-line\n", "dev change");
        write_commit(
            dir.path(),
            "staging",
            "base\nstaging-line\n",
            "staging change",
        );
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        plan(dir.path(), &policy, "development", "staging").unwrap();

        let events = read_events(dir.path(), None).unwrap();
        let planned_event = events
            .iter()
            .find(|event| event.kind == "promotion.planned")
            .unwrap();
        assert_eq!(planned_event.result, "blocked");
        assert_eq!(planned_event.severity, Severity::Warning);
    }

    #[test]
    fn metrics_reflect_the_plan_and_are_deterministic_about_tokens_and_conflicts() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let planned = plan(dir.path(), &policy, "development", "staging").unwrap();
        let validated = validate(dir.path(), &planned.id, None, None).unwrap();
        let completed = promote(dir.path(), &planned.id, false, false).unwrap();

        let metrics = compute_metrics(dir.path(), &completed).unwrap();
        assert_eq!(metrics.promotion_id, completed.id);
        assert_eq!(metrics.commits, 1);
        assert_eq!(metrics.files_changed, 1);
        assert_eq!(metrics.conflicts_detected, 0);
        assert_eq!(metrics.conflicts_resolved, 0);
        assert_eq!(metrics.failed_validation_gates, 0);
        assert_eq!(metrics.retry_count, 0);
        assert!(metrics.success);
        assert_eq!(metrics.tokens_consumed, 0);
        assert!(metrics.planning_duration_ms >= 0);
        assert!(metrics.validation_duration_ms.unwrap() >= 0);
        assert!(metrics.execution_duration_ms.unwrap() >= 0);
        let _ = validated;
    }

    #[test]
    fn retry_count_reflects_earlier_promotions_on_the_same_transition() {
        let dir = repo();
        write_commit(dir.path(), "development", "base\nfeature\n", "feature");
        let policy = LifecyclePolicy::load(dir.path()).unwrap();
        let first = plan(dir.path(), &policy, "development", "staging").unwrap();
        cancel(dir.path(), &first.id, None).unwrap();
        let second = plan(dir.path(), &policy, "development", "staging").unwrap();

        let metrics = compute_metrics(dir.path(), &second).unwrap();
        assert_eq!(metrics.retry_count, 1);
    }
}
