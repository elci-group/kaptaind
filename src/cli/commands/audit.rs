//! `kaptaind-cli audit` — compliance audit-trail inspection (Workstream D2).
//!
//! Reads `.kaptaind/audit.jsonl` (one `AuditEntry` per line) and offers tail,
//! stats, and an append-only/optional-hash-chain integrity check.

use chrono::{DateTime, Utc};
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
            let report = verify(&rows);
            if format.eq_ignore_ascii_case("json") {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_verify(&report);
            }
            if !report.ok {
                anyhow::bail!("audit integrity check failed (see report)");
            }
        }
    }
    Ok(())
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

fn verify(rows: &[AuditRow]) -> VerifyReport {
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

    // 2. Optional hash chain. AuditEntry does not (yet) carry prev_hash, so a
    //    fully-legacy log is reported as "pre-chain" and not treated as failure.
    let chain = verify_chain(rows, &mut issues);

    let ok = ordered && chain != "broken";
    VerifyReport {
        total: rows.len(),
        ordered,
        chain,
        ok,
        issues,
    }
}

fn verify_chain(rows: &[AuditRow], issues: &mut Vec<String>) -> String {
    // Detect whether entries carry a prev_hash field.
    let any_chain = rows.iter().any(|r| {
        serde_json::from_str::<serde_json::Value>(&r.raw)
            .ok()
            .and_then(|v| v.get("prev_hash").cloned())
            .map(|p| !p.is_null())
            .unwrap_or(false)
    });

    if !any_chain {
        return "pre-chain (legacy)".to_string();
    }

    // Best-effort chain check: each entry's prev_hash must equal the sha256 of
    // the previous raw line; the first entry must have a null/absent prev_hash.
    let mut prev_hash: Option<String> = None;
    for r in rows {
        let v: serde_json::Value = serde_json::from_str(&r.raw).unwrap_or(serde_json::Value::Null);
        let claimed = v
            .get("prev_hash")
            .and_then(|p| p.as_str())
            .map(str::to_string);
        match (&prev_hash, &claimed) {
            (None, None) | (None, Some(_)) => {}
            (Some(expected), Some(got)) if expected == got => {}
            (Some(expected), Some(got)) => {
                issues.push(format!(
                    "line {}: prev_hash mismatch (expected {}, got {})",
                    r.index, expected, got
                ));
                return "broken".to_string();
            }
            (Some(_), None) => {
                issues.push(format!(
                    "line {}: missing prev_hash in chained log",
                    r.index
                ));
                return "broken".to_string();
            }
        }
        let mut hasher = Sha256::new();
        hasher.update(r.raw.as_bytes());
        prev_hash = Some(kaptaind::util::hex::encode(hasher.finalize()));
    }
    if issues.is_empty() {
        "ok".to_string()
    } else {
        "broken".to_string()
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
