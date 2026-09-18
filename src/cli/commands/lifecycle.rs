use kaptaind::config::Config;
use kaptaind::promotion::{self, LifecyclePolicy};
use std::path::Path;

fn print(
    value: &impl serde::Serialize,
    json: bool,
    text: impl FnOnce() -> String,
) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{}", text());
    }
    Ok(())
}

pub fn inspect(config: &Config, json: bool) -> anyhow::Result<()> {
    let policy = LifecyclePolicy::load(&config.repo_path)?;
    let report = promotion::inspect(&config.repo_path, &policy)?;
    print(&report, json, || {
        let mut out = String::from("Configured branches:\n");
        for branch in &report.branches {
            out.push_str(&format!(
                "  {} [{}]{} -> {}\n",
                branch.branch,
                branch.role,
                if branch.protected { " (protected)" } else { "" },
                branch.commit.as_deref().unwrap_or("missing")
            ));
        }
        out.push_str("\nConfigured transitions:\n");
        for transition in &report.transitions {
            out.push_str(&format!(
                "  {} ({} -> {}): {}\n",
                transition.transition,
                transition.source_branch,
                transition.target_branch,
                if transition.eligible {
                    "ELIGIBLE"
                } else {
                    "BLOCKED"
                }
            ));
            for reason in &transition.blocking_reasons {
                out.push_str(&format!("    - {reason}\n"));
            }
            for warning in &transition.warnings {
                out.push_str(&format!("    ! {warning}\n"));
            }
        }
        out
    })
}

pub fn plan(config: &Config, from: &str, to: &str, json: bool) -> anyhow::Result<()> {
    let policy = LifecyclePolicy::load(&config.repo_path)?;
    let promotion = promotion::plan(&config.repo_path, &policy, from, to)?;
    print(&promotion, json, || render_plan(&promotion))
}

pub fn validate(config: &Config, id: &str, json: bool) -> anyhow::Result<()> {
    let promotion = promotion::validate(
        &config.repo_path,
        id,
        config.test.command.as_deref(),
        config.build.command.as_deref(),
    )?;
    print(&promotion, json, || render_status(&promotion))
}

pub fn promote(
    config: &Config,
    id: &str,
    approve: bool,
    dry_run: bool,
    json: bool,
) -> anyhow::Result<()> {
    let promotion = promotion::promote(&config.repo_path, id, approve, dry_run)?;
    print(&promotion, json, || render_status(&promotion))
}

pub fn status(config: &Config, json: bool) -> anyhow::Result<()> {
    let promotions = promotion::status(&config.repo_path)?;
    print(&promotions, json, || {
        if promotions.is_empty() {
            "No active promotions.".to_string()
        } else {
            promotions
                .iter()
                .map(render_status)
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

pub fn history(config: &Config, limit: Option<usize>, json: bool) -> anyhow::Result<()> {
    let promotions = promotion::history(&config.repo_path, limit)?;
    print(&promotions, json, || {
        if promotions.is_empty() {
            "No recorded promotions.".to_string()
        } else {
            promotions
                .iter()
                .map(render_status)
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

pub fn cancel(config: &Config, id: &str, reason: Option<&str>, json: bool) -> anyhow::Result<()> {
    let promotion = promotion::cancel(&config.repo_path, id, reason)?;
    print(&promotion, json, || render_status(&promotion))
}

pub fn recover(config: &Config, id: &str, json: bool) -> anyhow::Result<()> {
    let promotion = promotion::recover(&config.repo_path, id)?;
    print(&promotion, json, || {
        format!(
            "{}\n{}",
            render_status(&promotion),
            promotion
                .recovery_action
                .as_deref()
                .unwrap_or("no recovery action recorded")
        )
    })
}

pub fn feed(config: &Config, since: Option<&str>, json: bool) -> anyhow::Result<()> {
    let events = promotion::read_events(&config.repo_path, since)?;
    print(&events, json, || {
        if events.is_empty() {
            "No recorded lifecycle events.".to_string()
        } else {
            events
                .iter()
                .map(|event| {
                    format!(
                        "{} {} {} [{:?}] {} -> {} ({})",
                        event.timestamp.to_rfc3339(),
                        event.event_id,
                        event.kind,
                        event.severity,
                        event.source_state,
                        event.target_state,
                        event.result
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

pub fn metrics(config: &Config, id: &str, json: bool) -> anyhow::Result<()> {
    let promotion = promotion::history(&config.repo_path, None)?
        .into_iter()
        .find(|promotion| promotion.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown promotion `{id}`"))?;
    let metrics = promotion::compute_metrics(&config.repo_path, &promotion)?;
    print(&metrics, json, || {
        format!(
            "{}\nplanning: {}ms  validation: {:?}ms  execution: {:?}ms\ncommits: {}  files changed: {}  conflicts detected: {}  conflicts resolved: {}\nfailed gates: {}  retries: {}  success: {}  tokens consumed: {}",
            metrics.promotion_id,
            metrics.planning_duration_ms,
            metrics.validation_duration_ms,
            metrics.execution_duration_ms,
            metrics.commits,
            metrics.files_changed,
            metrics.conflicts_detected,
            metrics.conflicts_resolved,
            metrics.failed_validation_gates,
            metrics.retry_count,
            metrics.success,
            metrics.tokens_consumed,
        )
    })
}

/// Convenience alias: plan, validate, and promote in one call, resolving
/// internally to the same canonical model (ELCI KAPTAIND-RTL-001 §21).
pub fn promote_alias(
    config: &Config,
    from: &str,
    to: &str,
    approve: bool,
    dry_run: bool,
    json: bool,
) -> anyhow::Result<()> {
    let repo: &Path = &config.repo_path;
    let policy = LifecyclePolicy::load(repo)?;
    let planned = promotion::plan(repo, &policy, from, to)?;
    let validated = promotion::validate(
        repo,
        &planned.id,
        config.test.command.as_deref(),
        config.build.command.as_deref(),
    )?;
    let _ = validated;
    let promoted = promotion::promote(repo, &planned.id, approve, dry_run)?;
    print(&promoted, json, || render_status(&promoted))
}

fn render_plan(promotion: &kaptaind::promotion::Promotion) -> String {
    let plan = &promotion.plan;
    format!(
        "PROMOTION PLAN {}\n\nSource: {} ({})\nTarget: {} ({})\nOperation: {}\n\nCommits: {}\nFiles changed: {}\nInsertions: {}\nDeletions: {}\nConflicts: {}\nRequires: {}\n\nStatus: {:?}",
        promotion.id,
        plan.source_branch,
        plan.source_role,
        plan.target_branch,
        plan.target_role,
        plan.operation,
        plan.commits,
        plan.files_changed,
        plan.insertions,
        plan.deletions,
        plan.conflicts,
        if plan.requires.is_empty() {
            "none".to_string()
        } else {
            plan.requires.join(", ")
        },
        promotion.status
    )
}

fn render_status(promotion: &kaptaind::promotion::Promotion) -> String {
    format!(
        "{} [{:?}] {} -> {}{}",
        promotion.id,
        promotion.status,
        promotion.plan.source_branch,
        promotion.plan.target_branch,
        promotion
            .failure_reason
            .as_deref()
            .map(|reason| format!(" ({reason})"))
            .unwrap_or_default()
    )
}

pub fn queue_add(
    config: &Config,
    from: &str,
    to: &str,
    note: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let policy = LifecyclePolicy::load(&config.repo_path)?;
    let request = promotion::enqueue(&config.repo_path, &policy, from, to, note)?;
    print(&request, json, || {
        format!(
            "{} queued: {} -> {}",
            request.id, request.source, request.target
        )
    })
}

pub fn queue_list(config: &Config, json: bool) -> anyhow::Result<()> {
    let requests = promotion::list_queue(&config.repo_path)?;
    print(&requests, json, || {
        if requests.is_empty() {
            "No queued requests.".to_string()
        } else {
            requests
                .iter()
                .map(|request| {
                    format!(
                        "{} {} -> {} (requested {} by {})",
                        request.id,
                        request.source,
                        request.target,
                        request.requested_at.to_rfc3339(),
                        request.requested_by
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

pub fn queue_remove(config: &Config, id: &str, json: bool) -> anyhow::Result<()> {
    let removed = promotion::remove_from_queue(&config.repo_path, id)?;
    print(&removed, json, || format!("removed {}", removed.id))
}

pub fn queue_drain(config: &Config, json: bool) -> anyhow::Result<()> {
    let policy = LifecyclePolicy::load(&config.repo_path)?;
    let outcome = promotion::drain(&config.repo_path, &policy)?;
    print(&outcome, json, || {
        let mut out = format!("Planned: {}\n", outcome.planned.len());
        for promotion in &outcome.planned {
            out.push_str(&format!("  {}\n", render_status(promotion)));
        }
        out.push_str(&format!("Skipped: {}\n", outcome.skipped.len()));
        for skipped in &outcome.skipped {
            out.push_str(&format!(
                "  {} {} -> {} ({})\n",
                skipped.request.id, skipped.request.source, skipped.request.target, skipped.reason
            ));
        }
        out
    })
}

fn render_batch(batch: &kaptaind::promotion::PromotionBatch) -> String {
    let mut out = format!("BATCH {} ({} -> {})\n", batch.id, batch.from, batch.to);
    for member in &batch.members {
        out.push_str(&format!(
            "  {} [{}] {}{}\n",
            member.repository,
            member.status,
            member.promotion_id.as_deref().unwrap_or("-"),
            member
                .error
                .as_deref()
                .map(|error| format!(" error: {error}"))
                .unwrap_or_default()
        ));
    }
    out
}

pub fn batch_plan(
    config: &Config,
    repos: &[std::path::PathBuf],
    from: &str,
    to: &str,
    json: bool,
) -> anyhow::Result<()> {
    let batch = promotion::plan_batch(&config.repo_path, repos, from, to)?;
    print(&batch, json, || render_batch(&batch))
}

pub fn batch_validate(config: &Config, id: &str, json: bool) -> anyhow::Result<()> {
    let batch = promotion::validate_batch(&config.repo_path, id)?;
    print(&batch, json, || render_batch(&batch))
}

pub fn batch_promote(
    config: &Config,
    id: &str,
    approve: bool,
    dry_run: bool,
    json: bool,
) -> anyhow::Result<()> {
    let batch = promotion::promote_batch(&config.repo_path, id, approve, dry_run)?;
    print(&batch, json, || render_batch(&batch))
}

pub fn batch_status(config: &Config, id: &str, json: bool) -> anyhow::Result<()> {
    let batch = promotion::batch_status(&config.repo_path, id)?;
    print(&batch, json, || render_batch(&batch))
}
