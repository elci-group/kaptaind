//! `kaptaind commit`: one-shot transaction through the daemon pipeline (AGX-1).

use kaptaind::config::loader::Config;
use kaptaind::daemon::oneshot::{self, OneshotOptions, OneshotReport, PushStatus};
use kaptaind::util::style::*;
use std::path::{Path, PathBuf};

/// Resolve a `--paths` argument to an absolute path: relative arguments are
/// taken from the current directory, existing paths are canonicalized, and a
/// deleted path is anchored on its canonicalized parent.
fn resolve_path(arg: &Path) -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let joined = if arg.is_absolute() {
        arg.to_path_buf()
    } else {
        base.join(arg)
    };
    if let Ok(canonical) = joined.canonicalize() {
        return canonical;
    }
    match (joined.parent(), joined.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|p| p.join(name))
            .unwrap_or(joined),
        _ => joined,
    }
}

/// Run the transaction and return the process exit code.
pub async fn handle_commit(
    config: &Config,
    paths: &[PathBuf],
    push: bool,
    force: bool,
    skip_tests: Option<&str>,
    json: bool,
) -> i32 {
    let skip_tests_reason = match skip_tests.map(oneshot::normalize_skip_reason).transpose() {
        Ok(reason) => reason,
        Err(error) => return report_error(&error, json),
    };
    let opts = OneshotOptions {
        paths: paths.iter().map(|p| resolve_path(p)).collect(),
        push,
        force,
        skip_tests_reason,
    };
    match oneshot::run(config, &opts).await {
        Ok(report) => {
            if json {
                match serde_json::to_string(&report) {
                    Ok(line) => println!("{line}"),
                    Err(error) => return report_error(&error.into(), json),
                }
            } else {
                print_human(&report);
            }
            report.exit_code()
        }
        Err(error) => report_error(&error, json),
    }
}

fn report_error(error: &anyhow::Error, json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "outcome": "error",
                "version_before": null,
                "version_after": null,
                "commit": null,
                "push": {"status": "skipped", "detail": "no commit created"},
                "reason": format!("{error:#}"),
                "cluster_id": null,
                "paths": [],
                "skipped_tests": null,
            })
        );
    } else {
        eprintln!(
            "{} {}",
            "❌".red(),
            format!("kaptaind commit: {error:#}").red()
        );
    }
    oneshot::EXIT_ERROR
}

fn print_human(report: &OneshotReport) {
    let code = report.exit_code();
    let version = |v: &Option<String>| v.as_deref().map(|v| format!("v{v}"));
    if let Some(sha) = report.commit.as_deref() {
        let short = &sha[..sha.len().min(12)];
        let versions = match (
            version(&report.version_before),
            version(&report.version_after),
        ) {
            (Some(before), Some(after)) if before != after => format!(" {before} -> {after}"),
            (_, Some(after)) => format!(" at {after} (no bump)"),
            _ => String::new(),
        };
        println!(
            "{} {}{}",
            "⚓".green(),
            format!("Committed {short}").bold().green(),
            versions.green()
        );
        println!("   {} {}", "outcome:".bold(), report.outcome);
        println!("   {} {}", "paths:".bold(), report.paths.len());
    } else {
        let (icon, colour_reason) = if code == oneshot::EXIT_NOTHING {
            ("💤", report.reason.as_str().yellow())
        } else if code == oneshot::EXIT_REFUSED {
            ("🛑", report.reason.as_str().red())
        } else {
            ("❌", report.reason.as_str().red())
        };
        println!(
            "{} {} {}",
            icon,
            format!("No commit ({})", report.outcome).bold(),
            colour_reason
        );
        return;
    }
    if let Some(reason) = report.skipped_tests.as_deref() {
        println!(
            "   {} {}",
            "⚠️ tests skipped:".yellow().bold(),
            reason.yellow()
        );
    }
    let push_line = match report.push.status {
        PushStatus::Ok => format!("🚀 {}", report.push.detail).green(),
        PushStatus::Disabled if !report.push_requested => {
            format!("⏸️  push disabled — {}", report.push.detail).dimmed()
        }
        PushStatus::Disabled => format!("🛑 push disabled — {}", report.push.detail).red(),
        PushStatus::Refused => format!("🛑 push refused — {}", report.push.detail).red(),
        PushStatus::Failed => format!("❌ push failed — {}", report.push.detail).red(),
        PushStatus::Skipped => format!("push skipped — {}", report.push.detail).dimmed(),
    };
    println!("   {push_line}");
}
