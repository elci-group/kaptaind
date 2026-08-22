//! `kaptaind-cli logs` — daemon log inspection (Workstream D1).
//!
//! Reads `.kaptaind/daemon.out` and `.kaptaind/daemon.err` (plain tracing
//! text) and offers tail / errors / grep views.

use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogLine {
    source: String,
    line: String,
}

pub enum LogsAction {
    Tail { n: usize },
    Errors,
    Grep { pattern: String },
}

pub fn handle_logs(config: &Config, action: &LogsAction, format: &str) -> anyhow::Result<()> {
    let kd = config.repo_path.join(".kaptaind");
    let mut lines = read_source(&kd.join("daemon.out"), "out");
    lines.extend(read_source(&kd.join("daemon.err"), "err"));

    let filtered = match action {
        LogsAction::Tail { n } => tail(lines, *n),
        LogsAction::Errors => lines
            .into_iter()
            .filter(|l| {
                let upper = l.line.to_ascii_uppercase();
                upper.contains("ERROR") || upper.contains("WARN")
            })
            .collect(),
        LogsAction::Grep { pattern } => {
            let re = Regex::new(pattern)?;
            lines.into_iter().filter(|l| re.is_match(&l.line)).collect()
        }
    };

    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    } else {
        print_human(&filtered);
    }
    Ok(())
}

fn read_source(path: &Path, source: &str) -> Vec<LogLine> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "read_source",
                source_line = line!(),
                "read source returned an error"
            );
            return Vec::new();
        }
    };
    content
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| LogLine {
            source: source.to_string(),
            line: l.to_string(),
        })
        .collect()
}

fn tail(lines: Vec<LogLine>, n: usize) -> Vec<LogLine> {
    let len = lines.len();
    lines.into_iter().skip(len.saturating_sub(n)).collect()
}

fn print_human(lines: &[LogLine]) {
    if lines.is_empty() {
        println!("{} {}", "📭".yellow(), "no log lines".dimmed());
        return;
    }
    for l in lines {
        let tag = if l.source == "err" {
            "err".red().to_string()
        } else {
            "out".bright_black().to_string()
        };
        println!("{} {}", format!("[{tag}]").dimmed(), l.line);
    }
}
