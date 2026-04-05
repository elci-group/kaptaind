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
    /// Manage Aim of Change sessions
    #[command(subcommand)]
    Aoc(AocCommand),
    /// Initialize kaptaind config for the current project
    Init,
}

#[derive(Subcommand)]
enum AocCommand {
    /// Start a new Aim of Change session
    Start {
        /// User-friendly name for this AoC
        label: String,
    },
    /// End and ship the current AoC session
    Ship,
    /// Show status of the current AoC session
    Status,
    /// Intercept agent operations for contextual tracing
    Intercept {
        /// Agent model used (e.g. "claude-3-5-sonnet")
        #[arg(short, long)]
        model: Option<String>,
        /// Intent or task description
        #[arg(short, long)]
        intent: Option<String>,
        /// Command to wrap and execute
        command: String,
        /// Arguments for the command
        args: Vec<String>,
    },
    /// View completed AoC sessions
    Log {
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Init command works without a valid config
    if matches!(&cli.command, Commands::Init) {
        let repo_path = cli
            .repo
            .map(|p| p.canonicalize().unwrap_or(p))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let config = Config {
            repo_path,
            ..Config::default()
        };
        handle_init(&config)?;
        return Ok(());
    }

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
        Commands::Aoc(aoc_cmd) => {
            handle_aoc(&config, aoc_cmd)?;
        }
        Commands::Init => {
            handle_init(&config)?;
        }
    }

    Ok(())
}

fn handle_aoc(config: &Config, cmd: &AocCommand) -> anyhow::Result<()> {
    match cmd {
        AocCommand::Start { label } => {
            handle_aoc_start(config, label)?;
        }
        AocCommand::Ship => {
            handle_aoc_ship(config)?;
        }
        AocCommand::Status => {
            handle_aoc_status(config)?;
        }
        AocCommand::Intercept { model, intent, command, args } => {
            handle_aoc_intercept(config, model.clone(), intent.clone(), command, args)?;
        }
        AocCommand::Log { limit } => {
            handle_aoc_log(config, *limit)?;
        }
    }
    Ok(())
}

fn handle_aoc_start(config: &Config, label: &str) -> anyhow::Result<()> {
    // Check if an active session already exists
    if let Ok(Some(_)) = kaptaind::aoc::session::load_active(&config.repo_path) {
        anyhow::bail!("An AoC session is already active. Run 'aoc ship' to end it.");
    }

    // Read current version
    let version_path = config.repo_path.join("VERSION");
    let initial_version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "0.1.0".to_string()
    };

    // Create new session
    let session = kaptaind::aoc::AocSession {
        id: uuid::Uuid::new_v4().to_string(),
        label: label.to_string(),
        created_at: Utc::now(),
        initial_version: initial_version.clone(),
        intent: None,
        target_stability: None,
    };

    // Save session
    kaptaind::aoc::session::save_active(&config.repo_path, &session)?;

    println!(
        "{} {} {} {}",
        "🎯".cyan(),
        "AoC started:".bold().cyan(),
        label.magenta(),
        format!("@ v{}", initial_version).blue()
    );

    Ok(())
}

fn handle_aoc_ship(config: &Config) -> anyhow::Result<()> {
    // Load active session
    let session = kaptaind::aoc::session::load_active(&config.repo_path)?
        .ok_or_else(|| anyhow::anyhow!("No active AoC session found"))?;

    // Read final version
    let version_path = config.repo_path.join("VERSION");
    let final_version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "0.1.0".to_string()
    };

    // Read traces
    let traces = kaptaind::aoc::tracer::read_traces_for_aoc(&config.repo_path, &session.id)?;

    // Count commits and test failures
    let commit_count = traces
        .iter()
        .filter(|t| matches!(t.result, kaptaind::aoc::TraceResult::Committed { .. }))
        .count();
    let test_failures = traces
        .iter()
        .filter(|t| t.test.outcome == "failed")
        .count();

    // Create manifest
    let manifest = kaptaind::aoc::AocManifest {
        id: session.id.clone(),
        label: session.label.clone(),
        created_at: session.created_at,
        shipped_at: Utc::now(),
        initial_version: session.initial_version.clone(),
        final_version: final_version.clone(),
        cluster_count: traces.len(),
        commit_count,
        test_failures,
        trace_ids: traces.iter().map(|t| t.cluster_id.clone()).collect(),
    };

    // Save manifest
    kaptaind::aoc::session::save_manifest(&config.repo_path, &manifest)?;

    // Remove active session
    kaptaind::aoc::session::remove_active(&config.repo_path)?;

    // Print summary
    println!("{} {} {} {}", "🚢".green(), "AoC shipped:".bold().green(), session.label.magenta(), "✓".green());
    println!("{}", "---".green());
    println!(
        "{} {} {}",
        "Version:".cyan(),
        format!("{} → {}", session.initial_version, final_version).magenta(),
        if session.initial_version != final_version { "✨" } else { "" }.yellow()
    );
    println!("{} {}", "Clusters:".cyan(), traces.len().to_string().yellow());
    println!("{} {}", "Commits:".cyan(), commit_count.to_string().yellow());
    println!("{} {}", "Test Failures:".cyan(), format!("{}", test_failures).yellow());

    Ok(())
}

fn handle_aoc_status(config: &Config) -> anyhow::Result<()> {
    match kaptaind::aoc::session::load_active(&config.repo_path)? {
        Some(session) => {
            // Count traces
            let traces = kaptaind::aoc::tracer::read_traces_for_aoc(&config.repo_path, &session.id)?;

            println!("{} {}", "🎯".cyan(), "Active AoC:".bold().cyan());
            println!("{}", "---".cyan());
            println!("{} {}", "Label:".cyan(), session.label.magenta());
            println!("{} {}", "Started:".cyan(), format_datetime(session.created_at).blue());
            println!("{} {}", "Initial Version:".cyan(), session.initial_version.yellow());
            println!("{} {}", "Traces:".cyan(), traces.len().to_string().yellow());
        }
        None => {
            println!("{} {}", "ℹ️".blue(), "No active AoC session.".blue());
        }
    }

    Ok(())
}

fn handle_aoc_intercept(
    config: &Config,
    model: Option<String>,
    intent: Option<String>,
    command: &str,
    args: &[String],
) -> anyhow::Result<()> {
    // Check if an active session already exists, start one if not
    let mut tmp_aoc = false;
    if kaptaind::aoc::session::load_active(&config.repo_path)?.is_none() {
        tmp_aoc = true;
        let label = intent.clone().unwrap_or_else(|| "agent-intercept".to_string());
        handle_aoc_start(config, &label)?;
    }

    let start_time = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();

    println!("{} {}", "🤖".cyan(), "Intercepting Agent execution...".bold().cyan());

    // Spawn command
    let mut child = std::process::Command::new(command)
        .args(args)
        .spawn()?;
    
    let status = child.wait()?;

    let end_time = Utc::now();
    let duration = (end_time - start_time).num_milliseconds().max(0) as u64;

    // Build AgentEvent
    let agent_event = kaptaind::aoc::AgentEvent {
        id,
        timestamp: start_time,
        model: model.clone(),
        input: intent.map(|s| serde_json::Value::String(s)),
        output: Some(serde_json::Value::String(format!("exit code: {:?}", status.code()))),
        tools: vec![command.to_string()], // simple tool recording
        latency_ms: duration,
    };

    kaptaind::aoc::interceptor::log_event(&config.repo_path, &agent_event)?;

    println!("{} {}", "✅".green(), "Agent event logged for context mapping.".bold().green());

    if tmp_aoc {
        println!("{} {}", "ℹ️".blue(), "AoC session remains active for daemon to process clusters. Run 'kaptaind-cli aoc ship' later.".blue());
    }

    Ok(())
}

fn handle_aoc_log(config: &Config, limit: usize) -> anyhow::Result<()> {
    let manifests = kaptaind::aoc::session::list_manifests(&config.repo_path)?;

    if manifests.is_empty() {
        println!("No completed AoC sessions found.");
        return Ok(());
    }

    #[derive(Tabled)]
    struct AocRow {
        #[tabled(rename = "🏷️ Label")]
        label: String,
        #[tabled(rename = "📈 Version")]
        version: String,
        #[tabled(rename = "🗂️ Clusters")]
        clusters: usize,
        #[tabled(rename = "🚀 Commits")]
        commits: usize,
        #[tabled(rename = "🧪 Failures")]
        failures: usize,
        #[tabled(rename = "🕒 Shipped")]
        shipped: String,
    }

    let rows: Vec<AocRow> = manifests
        .into_iter()
        .take(limit)
        .map(|m| AocRow {
            label: m.label.magenta().to_string(),
            version: format!("{} → {}", m.initial_version, m.final_version).cyan().to_string(),
            clusters: m.cluster_count,
            commits: m.commit_count,
            failures: m.test_failures,
            shipped: format_datetime(m.shipped_at).blue().to_string(),
        })
        .collect();

    let mut table = Table::new(rows);
    table.with(Style::modern());
    println!("{table}");

    Ok(())
}

fn handle_init(config: &Config) -> anyhow::Result<()> {
    let root = &config.repo_path;

    // Don't overwrite existing config
    let toml_path = root.join("kaptaind.toml");
    if toml_path.exists() {
        println!(
            "{} {}",
            "⚠️".yellow(),
            "kaptaind.toml already exists. Skipping.".yellow()
        );
        return Ok(());
    }

    let project = detect_project_type(root);

    // Generate kaptaind.toml
    let toml_content = generate_toml(&project);
    fs::write(&toml_path, &toml_content)?;
    println!(
        "{} {} {}",
        "✅".green(),
        "Created".green(),
        "kaptaind.toml".bold()
    );

    // Generate .kaptainignore
    let ignore_path = root.join(".kaptainignore");
    if !ignore_path.exists() {
        let ignore_content = generate_ignore(&project);
        fs::write(&ignore_path, &ignore_content)?;
        println!(
            "{} {} {}",
            "✅".green(),
            "Created".green(),
            ".kaptainignore".bold()
        );
    } else {
        println!(
            "{} {}",
            "⚠️".yellow(),
            ".kaptainignore already exists. Skipping.".yellow()
        );
    }

    println!(
        "\n{} {} {}",
        "🎯".cyan(),
        "Detected project type:".cyan(),
        format!("{:?}", project).bold().magenta()
    );

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Swift,
    Kotlin,
    Unknown,
}

fn detect_project_type(root: &std::path::Path) -> ProjectType {
    if root.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if root.join("Package.swift").exists() {
        ProjectType::Swift
    } else if root.join("build.gradle.kts").exists() || root.join("build.gradle").exists() {
        ProjectType::Kotlin
    } else if root.join("package.json").exists() {
        ProjectType::Node
    } else if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        ProjectType::Python
    } else if root.join("go.mod").exists() {
        ProjectType::Go
    } else {
        ProjectType::Unknown
    }
}

fn generate_toml(project: &ProjectType) -> String {
    let (test_cmd, weights) = match project {
        ProjectType::Rust => ("cargo test", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Node => ("npm test", "s = 0.30\na = 0.35\nd = 0.20\nr = 0.15"),
        ProjectType::Python => ("pytest", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Go => ("go test ./...", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Swift => ("swift test", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Kotlin => ("./gradlew test", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Unknown => ("echo 'no test command configured'", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
    };

    format!(
        r#"# kaptaind configuration — auto-generated by `kaptaind-cli init`

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5

[weights]
{weights}

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10

[test]
command = "{test_cmd}"
required = true

# [staging]
# mode = "all"        # "all" (default), "cluster" (only changed files), or "pattern"
# include = ["src/**"] # only used in "pattern" mode
# exclude = ["*.log", ".env*"]
"#
    )
}

fn generate_ignore(project: &ProjectType) -> String {
    let mut lines = vec![
        "# Common",
        ".git",
        ".kaptaind",
        ".DS_Store",
        "*.swp",
        "*.swo",
    ];

    match project {
        ProjectType::Rust => {
            lines.extend(["", "# Rust", "target"]);
        }
        ProjectType::Node => {
            lines.extend([
                "",
                "# Node",
                "node_modules",
                ".next",
                "dist",
                "build",
                ".turbo",
                ".vercel",
                ".output",
                "*.lock",
            ]);
        }
        ProjectType::Python => {
            lines.extend([
                "",
                "# Python",
                "__pycache__",
                ".venv",
                ".pytest_cache",
                "*.egg-info",
                "dist",
            ]);
        }
        ProjectType::Go => {
            lines.extend(["", "# Go", "vendor"]);
        }
        ProjectType::Swift => {
            lines.extend([
                "",
                "# Swift",
                ".build",
                "DerivedData",
                "*.xcodeproj/xcuserdata",
                "*.xcworkspace/xcuserdata",
                "Pods",
            ]);
        }
        ProjectType::Kotlin => {
            lines.extend([
                "",
                "# Kotlin/Gradle",
                "build",
                ".gradle",
                "*.iml",
                ".idea",
                "local.properties",
            ]);
        }
        ProjectType::Unknown => {}
    }

    lines.push(""); // trailing newline
    lines.join("\n")
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

    let mut diff_analysis = kaptaind::diff::analyze(&cluster, &config.repo_path);
    if config.bundle.command.is_some() {
        diff_analysis.bundle = kaptaind::diff::bundle::bundle_score(&config.bundle, &config.repo_path).score;
    }
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
    
    let version_path = config.repo_path.join("VERSION");
    let current_version_str = if version_path.exists() {
        fs::read_to_string(&version_path).unwrap_or_else(|_| "0.1.0".to_string()).trim().to_string()
    } else {
        "0.1.0".to_string()
    };
    let current_version = semver::Version::parse(&current_version_str).unwrap_or_else(|_| semver::Version::new(0, 1, 0));
    let next_version = kaptaind::version::apply(current_version, bump);

    let bump_str = match bump {
        kaptaind::version::Bump::Major => "🚀 Major".red().bold(),
        kaptaind::version::Bump::Minor => "✨ Minor".cyan().bold(),
        kaptaind::version::Bump::Patch => "🩹 Patch".green().bold(),
        kaptaind::version::Bump::None => "📌 Stable".blue(),
    };
    
    if bump == kaptaind::version::Bump::None {
        println!("{} {}", "📈 Projected Bump:".bold().cyan(), bump_str);
    } else {
        let bump_display = format!("{} -> v{}", bump_str, next_version);
        println!("{} {}", "📈 Projected Bump:".bold().cyan(), bump_display);
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

    println!("{} {}", "🚢".blue(), "Kaptaind Status".bold().blue());
    println!("{}", "=================".blue());
    println!("{} {}", "📂 Repository: ".bold().cyan(), config.repo_path.display().to_string().blue());
    println!("{} {}", "🏷️  Version:    ".bold().cyan(), version.magenta());

    let pid_running = get_daemon_pid(config);
    if let Some(pid) = pid_running {
        let status_json = config.repo_path.join(".kaptaind").join("status.json");
        let mut state_display = "[🟢 Running]".green().to_string();
        
        if let Ok(content) = fs::read_to_string(&status_json) {
            if let Ok(report) = serde_json::from_str::<kaptaind::daemon::scheduler::StatusReport>(&content) {
                state_display = match report.status {
                    kaptaind::daemon::scheduler::State::Idle => "[💤 Idle]".blue().to_string(),
                    kaptaind::daemon::scheduler::State::Clustering => "[🔍 Clustering]".cyan().to_string(),
                    kaptaind::daemon::scheduler::State::Testing => "[🧪 Testing]".yellow().to_string(),
                    kaptaind::daemon::scheduler::State::Committing => "[🚢 Committing]".magenta().to_string(),
                    kaptaind::daemon::scheduler::State::Failed => "[🛑 Failed]".red().to_string(),
                };
            }
        }
        
        println!("{} {} {}", "⚙️  Daemon:     ".bold().cyan(), state_display, format!("(PID: {})", pid).blue());
    } else {
        println!("{} {}", "⚙️  Daemon:     ".bold().cyan(), "🛑 Stopped".red());
    }

    Ok(())
}

fn get_daemon_pid(config: &Config) -> Option<i32> {
    let pid_file = config.repo_path.join(".kaptaind").join("daemon.pid");
    if let Ok(pid_str) = fs::read_to_string(pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Signal 0 checks if the process is running and we have permissions to signal it.
            if unsafe { libc::kill(pid, 0) } == 0 {
                return Some(pid);
            }
        }
    }
    None
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
