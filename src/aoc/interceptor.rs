use crate::aoc::tracer::AgentEvent;
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// Logs an agent event to the interceptor log file.
pub fn log_event(repo_path: &Path, event: &AgentEvent) -> anyhow::Result<()> {
    let dir = repo_path.join(".kaptaind").join("aoc");
    fs::create_dir_all(&dir)?;
    let log_path = dir.join("interceptor.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(log_path)?;
    let content = serde_json::to_string(event)?;
    writeln!(file, "{}", content)?;
    Ok(())
}

/// Consume events from the interceptor log file that fall within a time window.
/// This reads the file, parses events, keeps the ones outside the window (or unconsumed),
/// and returns the matched ones.
pub fn consume_events_in_window(
    repo_path: &Path,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> anyhow::Result<Vec<AgentEvent>> {
    let log_path = repo_path.join(".kaptaind").join("aoc").join("interceptor.jsonl");
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&log_path)?;
    let reader = BufReader::new(file);

    let mut matched = Vec::new();
    let mut remaining = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<AgentEvent>(&line) {
            // Buffer the time slightly to catch events associated with the cluster
            if event.timestamp >= start && event.timestamp <= end {
                matched.push(event);
            } else {
                remaining.push(line);
            }
        } else {
            remaining.push(line);
        }
    }

    // Rewrite the file with remaining events
    let mut out_file = fs::File::create(&log_path)?;
    for line in remaining {
        writeln!(out_file, "{}", line)?;
    }

    Ok(matched)
}
