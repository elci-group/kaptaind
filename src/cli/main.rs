use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use colored::*;
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

    /// Optional path to the repository to operate on (overrides kaptaind.toml)
    #[arg(short, long)]
    repo: Option<PathBuf>,
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
    let mut config = loader::load()?;

    if let Some(repo_override) = cli.repo {
        config.repo_path = repo_override.canonicalize().unwrap_or(repo_override);
    }

    match &cli.command {
        Commands::Status => {
            handle_status(&config)?;
        }
        Commands::Log { limit } => {
            handle_log(&config, *limit)?;
        }
        Commands::Analyze => {
            handle_analyze(&config)?;
        }
    }

    Ok(())
}

fn handle_analyze(config: &Config) -> anyhow::Result<()> {
    let repo = match kaptaind::git::repo::Repo::open(&config.repo_path) {
        Ok(repo) => repo,
        Err(err) => {
            anyhow::bail!(
                "Could not open Git repository at {}: {}",
                config.repo_path.display(),
                err
            );
        }
    };
    let diff = repo.diff_workdir()?;

    let mut paths = Vec::new();
    diff.print(git2::DiffFormat::NameOnly, |delta, _, _| {
        if let Some(path) = delta.new_file().path() {
            paths.push(path.to_path_buf());
        }
        true
    })?;

    if paths.is_empty() {
        println!("Working tree is clean. No analysis generated.");
        return Ok(());
    }

    let timestamp = Utc::now();
    let cluster = kaptaind::cluster::engine::Cluster {
        id: uuid::Uuid::new_v4(),
        started_at: timestamp,
        ended_at: timestamp,
        events: vec![kaptaind::watcher::FsEvent {
            paths,
            kind: kaptaind::watcher::FsEventKind::Modify,
            timestamp,
        }],
    };

    let diff_analysis = kaptaind::diff::analyze(&cluster, &config.repo_path);
    let weight = kaptaind::weight::compute(&diff_analysis, &config.weights);
    let bump = kaptaind::version::decide(&weight);

    println!("{}", "🧪 Dry-run Analysis Result:".bold().magenta());
    println!("{}", "-----------------------------------".magenta());
    println!("{} {}", "🗂️ Touched Paths:".cyan(), diff_analysis.touched_paths);
    println!("{} {}", "💥 API Break:    ".cyan(), if diff_analysis.api_breaking { "Yes".red().bold() } else { "No".green() });
    println!("{} {}", "➕ API Added:    ".cyan(), if diff_analysis.api_added { "Yes".green() } else { "No".yellow() });
    println!("{} {}", "🔌 API Score:    ".cyan(), format!("{:.3}", diff_analysis.api).yellow());
    println!("{} {}", "📦 Deps Score:   ".cyan(), format!("{:.3}", diff_analysis.deps).yellow());
    println!("{} {}", "⚙️ Runtime Score:".cyan(), format!("{:.3}", diff_analysis.runtime).yellow());
    println!("{}", "-----------------------------------".magenta());
    println!("{} {}", "🎯 Total Score:  ".bold().cyan(), format!("{:.3}", weight.score).bold().yellow());
    
    let bump_str = match bump {
        kaptaind::version::Bump::Major => "🚀 Major".red().bold(),
        kaptaind::version::Bump::Minor => "✨ Minor".cyan().bold(),
        kaptaind::version::Bump::Patch => "🩹 Patch".green().bold(),
        kaptaind::version::Bump::None => "📌 Stable".blue(),
    };
    
    println!("{} {}", "📈 Projected Bump:".bold().cyan(), bump_str);

    Ok(())
}

fn handle_status(config: &Config) -> anyhow::Result<()> {
    let version_path = config.repo_path.join("VERSION");
    let version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "None (no VERSION file)".to_string()
    };

    println!("{} {}", "🚢".blue(), "Kaptaind Status".bold().blue());
    println!("{}", "=================".blue());
    println!("{} {}", "📂 Repository: ".bold().cyan(), config.repo_path.display().to_string().blue());
    println!("{} {}", "🏷️  Version:    ".bold().cyan(), version.magenta());

    let pid_running = check_daemon_pid(config);
    if pid_running {
        println!("{} {}", "⚙️  Daemon:     ".bold().cyan(), "🟢 Running".green());
    } else {
        println!("{} {}", "⚙️  Daemon:     ".bold().cyan(), "🛑 Stopped".red());
    }

    Ok(())
}

fn check_daemon_pid(config: &Config) -> bool {
    let pid_file = config.repo_path.join(".kaptaind").join("daemon.pid");
    if let Ok(pid_str) = fs::read_to_string(pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Signal 0 checks if the process is running and we have permissions to signal it.
            return unsafe { libc::kill(pid, 0) } == 0;
        }
    }
    false
}

#[derive(Tabled)]
struct LogRow {
    #[tabled(rename = "🏷️ Version")]
    version: String,
    #[tabled(rename = "📈 Bump")]
    bump: String,
    #[tabled(rename = "🎯 Score")]
    score: String,
    #[tabled(rename = "🗂️ Paths")]
    paths: usize,
    #[tabled(rename = "🔌 API Touches")]
    api_touches: usize,
    #[tabled(rename = "➕ API Added")]
    api_added: String,
    #[tabled(rename = "💥 API Break")]
    api_break: String,
    #[tabled(rename = "⚡ Events")]
    events: usize,
    #[tabled(rename = "🕒 Date")]
    date: String,
    #[tabled(rename = "🆔 ID")]
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
                api_added: if a.diff.api_added { "Yes".green().to_string() } else { "No".to_string() },
                api_break: if a.diff.api_breaking { "Yes".red().bold().to_string() } else { "No".to_string() },
                events: a.event_count,
                date: format_datetime(a.ended_at),
                id: a.cluster_id.chars().take(8).collect(),
            }
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
