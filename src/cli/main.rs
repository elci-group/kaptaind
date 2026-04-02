use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use kaptaind::config::loader::{self, Config};
use kaptaind::daemon::scheduler::AnalysisArtifact;
use std::fs;
use std::path::PathBuf;
use tabled::{settings::Style, Table, Tabled};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show current daemon status
    Status,
    /// View analysis history
    Log {
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
    /// Perform a one-off analysis of the working tree without committing
    Analyze,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = loader::load()?;

    match &cli.command {
        Commands::Status => {
            handle_status(&config)?;
        }
        Commands::Log { limit } => {
            handle_log(&config, *limit)?;
        }
        Commands::Analyze => {
            println!("Analyze not implemented yet");
        }
    }

    Ok(())
}

fn handle_status(config: &Config) -> anyhow::Result<()> {
    let version_path = config.repo_path.join("VERSION");
    let version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "None (no VERSION file)".to_string()
    };

    println!("Kaptaind Status");
    println!("===============");
    println!("Repository: {}", config.repo_path.display());
    println!("Version:    {version}");

    let pid_running = check_daemon_pid();
    if pid_running {
        println!("Daemon:     Running");
    } else {
        println!("Daemon:     Stopped");
    }

    Ok(())
}

fn check_daemon_pid() -> bool {
    // MVP stub: checking daemon health would normally involve a PID file or socket
    // Since phase 1 didn't write a PID file, we'll just return unknown/false.
    false
}

#[derive(Tabled)]
struct LogRow {
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Bump")]
    bump: String,
    #[tabled(rename = "Score")]
    score: String,
    #[tabled(rename = "Events")]
    events: usize,
    #[tabled(rename = "Date")]
    date: String,
    #[tabled(rename = "ID")]
    id: String,
}

fn handle_log(config: &Config, limit: usize) -> anyhow::Result<()> {
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
        .map(|a| LogRow {
            version: a.version,
            bump: a.bump,
            score: format!("{:.3}", a.weight.score),
            events: a.event_count,
            date: format_datetime(a.ended_at),
            id: a.cluster_id.chars().take(8).collect(),
        })
        .collect();

    let mut table = Table::new(rows);
    table.with(Style::modern());
    println!("{table}");

    Ok(())
}

fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}
