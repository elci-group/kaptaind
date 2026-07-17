//! `kaptaind-cli audit` — compliance audit-trail inspection (Workstream D2).
//!
//! Reads `.kaptaind/audit.jsonl` (one `AuditEntry` per line) and offers tail,
//! stats, and an append-only/optional-hash-chain integrity check.

use chrono::{DateTime, Utc};
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::table::print_table;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditRow {
    index: usize,
    timestamp: Option<String>,
    event_type: Option<String>,
    actor: Option<String>,
    result: Option<String>,
    raw: String,
}

pub enum AuditAction {
    Tail { n: usize },
    Stats,
    Verify,
    ExportVerify,
}

pub fn handle_audit(config: &Config, action: &AuditAction, format: &str) -> anyhow::Result<()> {
    let path = config.repo_path.join(".kaptaind").join("audit.jsonl");
    let rows = read_rows(&path);

    match action {
        AuditAction::Tail { n } => {
            let slice = tail(&rows, *n);
            if format.eq_ignore_ascii_case("json") {
                println!("{}", serde_json::to_string_pretty(&slice)?);
            } else {
                print_tail(&slice);
            }
        }
        AuditAction::Stats => {
            let stats = compute_stats(&rows);
            if format.eq_ignore_ascii_case("json") {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                print_stats(&stats);
            }
        }
        AuditAction::Verify => {
            let report = verify(&rows, &config.repo_path);
            if format.eq_ignore_ascii_case("json") {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_verify(&report);
            }
            if !report.ok {
                anyhow::bail!("audit integrity check failed (see report)");
            }
        }
        AuditAction::ExportVerify => {
            let report = verify_export(config);
            if format.eq_ignore_ascii_case("json") {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_export_verify(&report);
            }
            if !report.ok {
                anyhow::bail!("audit export integrity check failed (see report)");
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct ExportVerifyReport {
    configured: bool,
    ok: bool,
    issue: Option<String>,
}

fn verify_export(config: &Config) -> ExportVerifyReport {
    let Some(export) = config.audit.export.as_ref() else {
        return ExportVerifyReport {
            configured: false,
            ok: false,
            issue: Some("[audit].export.jsonl_path is not configured".to_string()),
        };
    };
    match kaptaind::audit::verify_export(&config.repo_path, export) {
        Ok(()) => ExportVerifyReport {
            configured: true,
            ok: true,
            issue: None,
        },
        Err(error) => ExportVerifyReport {
            configured: true,
            ok: false,
            issue: Some(error.to_string()),
        },
    }
}

fn read_rows(path: &Path) -> Vec<AuditRow> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, raw)| {
            let v: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);
            AuditRow {
                index: i,
                timestamp: v
                    .get("timestamp")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                event_type: v
                    .get("event_type")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                actor: v.get("actor").and_then(|x| x.as_str()).map(str::to_string),
                result: v.get("result").and_then(|x| x.as_str()).map(str::to_string),
                raw: raw.to_string(),
            }
        })
        .collect()
}

fn tail(rows: &[AuditRow], n: usize) -> Vec<AuditRow> {
    rows.iter()
        .skip(rows.len().saturating_sub(n))
        .cloned()
        .collect()
}

#[derive(Debug, Serialize)]
struct Stats {
    total: usize,
    by_event_type: BTreeMap<String, usize>,
    by_result: BTreeMap<String, usize>,
    failure_rate: f64,
}

fn compute_stats(rows: &[AuditRow]) -> Stats {
    let mut by_event_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_result: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = 0usize;
    for r in rows {
        *by_event_type
            .entry(r.event_type.clone().unwrap_or_else(|| "unknown".into()))
            .or_insert(0) += 1;
        let result = r.result.clone().unwrap_or_else(|| "unknown".into());
        if result == "failure" || result == "blocked" {
            failures += 1;
        }
        *by_result.entry(result).or_insert(0) += 1;
    }
    let total = rows.len();
    let failure_rate = if total == 0 {
        0.0
    } else {
        failures as f64 / total as f64
    };
    Stats {
        total,
        by_event_type,
        by_result,
        failure_rate,
    }
}

#[derive(Debug, Serialize)]
struct VerifyReport {
    total: usize,
    ordered: bool,
    chain: String,
    ok: bool,
    issues: Vec<String>,
}

fn verify(rows: &[AuditRow], repo_path: &Path) -> VerifyReport {
    let mut issues = Vec::new();

    // 1. Append-only ordering by timestamp.
    let mut ordered = true;
    let parsed: Vec<Option<DateTime<Utc>>> = rows
        .iter()
        .map(|r| {
            r.timestamp
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        })
        .collect();
    for w in parsed.windows(2) {
        if let (Some(a), Some(b)) = (&w[0], &w[1]) {
            if b < a {
                ordered = false;
                issues.push("timestamps are not monotonically increasing".into());
                break;
            }
        }
    }

    // 2. The production writer emits a companion hash chain. A legacy audit
    // log without its companion chain is explicitly unverified, not accepted.
    let chain = match kaptaind::audit::verify_chain(repo_path) {
        Ok(()) => "ok".to_string(),
        Err(error) => {
            issues.push(format!("audit hash chain: {error}"));
            "broken".to_string()
        }
    };

    let ok = ordered && chain != "broken";
    VerifyReport {
        total: rows.len(),
        ordered,
        chain,
        ok,
        issues,
    }
}

fn print_tail(rows: &[AuditRow]) {
    if rows.is_empty() {
        println!("{} {}", "📭".yellow(), "no audit entries".dimmed());
        return;
    }
    let out: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.timestamp.clone().unwrap_or_else(|| "?".into()),
                r.event_type.clone().unwrap_or_else(|| "?".into()),
                r.actor.clone().unwrap_or_else(|| "?".into()),
                r.result.clone().unwrap_or_else(|| "?".into()),
            ]
        })
        .collect();
    print_table(&["Timestamp", "Event", "Actor", "Result"], &out);
}

fn print_stats(s: &Stats) {
    println!("{} {} entries", "📊".cyan(), s.total.to_string().blue());
    println!(
        "{} {:.1}%",
        "Failure rate:".bold().cyan(),
        (s.failure_rate * 100.0).to_string().blue()
    );
    let rows: Vec<Vec<String>> = s
        .by_event_type
        .iter()
        .map(|(k, v)| vec![k.clone(), v.to_string()])
        .collect();
    if !rows.is_empty() {
        println!("\n{}", "by event_type:".bold().cyan());
        print_table(&["Event", "Count"], &rows);
    }
    let rows: Vec<Vec<String>> = s
        .by_result
        .iter()
        .map(|(k, v)| vec![k.clone(), v.to_string()])
        .collect();
    if !rows.is_empty() {
        println!("\n{}", "by result:".bold().cyan());
        print_table(&["Result", "Count"], &rows);
    }
}

fn print_verify(r: &VerifyReport) {
    let verdict = if r.ok {
        "OK".green().to_string()
    } else {
        "FAIL".red().to_string()
    };
    println!(
        "{} {} entries, ordered={}, chain={}, verdict={}",
        "🔐".cyan(),
        r.total.to_string().blue(),
        r.ordered,
        r.chain.as_str().blue(),
        verdict
    );
    for i in &r.issues {
        println!("  {} {}", "•".yellow(), i);
    }
}

fn print_export_verify(report: &ExportVerifyReport) {
    let verdict = if report.ok {
        "OK".green().to_string()
    } else {
        "FAIL".red().to_string()
    };
    println!(
        "{} configured={}, verdict={}",
        "🔐".cyan(),
        report.configured,
        verdict
    );
    if let Some(issue) = &report.issue {
        println!("  {} {}", "•".yellow(), issue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_uses_the_production_sidecar_chain() {
        let dir = tempfile::tempdir().unwrap();
        kaptaind::audit::append(
            dir.path(),
            &kaptaind::audit::AuditEntry::new("release", "ci", "success"),
        )
        .unwrap();
        let rows = read_rows(&dir.path().join(".kaptaind/audit.jsonl"));
        let report = verify(&rows, dir.path());
        assert!(report.ok);
        assert_eq!(report.chain, "ok");
    }

    #[test]
    fn verify_rejects_a_log_without_its_required_chain() {
        let dir = tempfile::tempdir().unwrap();
        let audit_dir = dir.path().join(".kaptaind");
        std::fs::create_dir_all(&audit_dir).unwrap();
        std::fs::write(
            audit_dir.join("audit.jsonl"),
            r#"{"timestamp":"2026-01-01T00:00:00Z","event_type":"release","actor":"ci","result":"success","details":null}\n"#,
        )
        .unwrap();
        let rows = read_rows(&audit_dir.join("audit.jsonl"));
        assert!(!verify(&rows, dir.path()).ok);
    }

    #[test]
    fn export_verify_reports_missing_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        let report = verify_export(&config);
        assert!(!report.configured);
        assert!(!report.ok);
    }
}
