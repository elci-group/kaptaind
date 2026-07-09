use kaptaind::config::loader::Config;
use kaptaind::daemon::scheduler::AnalysisArtifact;
use kaptaind::util::style::*;
use std::fs;

use crate::format_datetime;
use crate::table::print_table;

struct LogRow {
    version: String,
    bump: String,
    score: String,
    paths: usize,
    api_touches: usize,
    api_added: String,
    api_break: String,
    events: usize,
    date: String,
    id: String,
}

pub fn handle_log(config: &Config, limit: usize) -> anyhow::Result<()> {
    let analysis_dir = config.repo_path.join(".kaptaind").join("analysis");
    if !analysis_dir.exists() {
        println!("No analysis history found in {}", analysis_dir.display());
        return Ok(());
    }

    let mut artifacts = Vec::new();
    for entry in fs::read_dir(analysis_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(artifact) = serde_json::from_str::<AnalysisArtifact>(&content) {
                    artifacts.push(artifact);
                }
            }
        }
    }

    artifacts.sort_by_key(|a| std::cmp::Reverse(a.ended_at));
    artifacts.truncate(limit);

    if artifacts.is_empty() {
        println!("History is empty.");
        return Ok(());
    }

    let rows: Vec<LogRow> = artifacts
        .into_iter()
        .map(|a| {
            let bump_display = match a.bump.as_str() {
                "Major" => "🚀 Major".red().bold().to_string(),
                "Minor" => "✨ Minor".cyan().bold().to_string(),
                "Patch" => "🩹 Patch".green().to_string(),
                _ => "📌 Stable".blue().to_string(),
            };

            LogRow {
                version: a.version.magenta().to_string(),
                bump: bump_display,
                score: format!("{:.3}", a.weight.score).yellow().to_string(),
                paths: a.diff.touched_paths,
                api_touches: a.diff.api_touches,
                api_added: if a.diff.api_added {
                    "Yes".green().to_string()
                } else {
                    "No".to_string()
                },
                api_break: if a.diff.api_breaking {
                    "Yes".red().bold().to_string()
                } else {
                    "No".to_string()
                },
                events: a.event_count,
                date: format_datetime(a.ended_at),
                id: a.cluster_id.chars().take(8).collect(),
            }
        })
        .collect();

    let table_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|row| {
            vec![
                row.version,
                row.bump,
                row.score,
                row.paths.to_string(),
                row.api_touches.to_string(),
                row.api_added,
                row.api_break,
                row.events.to_string(),
                row.date,
                row.id,
            ]
        })
        .collect();

    print_table(
        &[
            "🏷️ Version",
            "📈 Bump",
            "🎯 Score",
            "🗂️ Paths",
            "🔌 API Touches",
            "➕ API Added",
            "💥 API Break",
            "⚡ Events",
            "🕒 Date",
            "🆔 ID",
        ],
        &table_rows,
    );

    Ok(())
}
