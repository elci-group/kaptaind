use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use colored::*;
use kaptaind::config::loader::{self, Config};
use kaptaind::daemon::scheduler::AnalysisArtifact;
use std::fs;
use std::path::PathBuf;
use tabled::{settings::Style, Table, Tabled};

#[derive(Parser)]
#[command(
    name = "kaptaind-cli",
    version = "0.1.0",
    author = "Elci Group <kaptaind@example.com>",
    about = "CLI companion to kaptaind daemon for inspection and management",
    long_about = "kaptaind-cli provides visibility into the daemon's state and offers one-off \
analysis and session management without starting the daemon.\n\n\
COMMANDS:\n  \
  status              View current daemon health and version\n  \
  log                 View recent automated commits and analysis decisions\n  \
  analyze             Dry-run: analyze working tree without committing\n  \
  dashboard           Live dashboard: stability, releases, recent analyses\n  \
  ci-hint             Release/hold recommendation for CI/CD pipelines\n  \
  aoc                 Manage Aim of Change sessions (multi-commit grouping)\n  \
  init                Initialize kaptaind config for a project\n\n\
EXAMPLES:\n  \
  kaptaind-cli status                     # Check daemon health\n  \
  kaptaind-cli log --limit 20             # View last 20 commits\n  \
  kaptaind-cli analyze                    # Dry-run diff analysis\n  \
  kaptaind-cli dashboard                  # Live terminal dashboard\n  \
  kaptaind-cli ci-hint --format json      # JSON output for CI\n  \
  kaptaind-cli aoc start \"feature: auth\"  # Begin a feature session\n  \
  kaptaind-cli init                       # Generate kaptaind.toml\n\n\
ENVIRONMENT:\n  \
  KAPTAIND_CONFIG     Path to kaptaind.toml (default: ./kaptaind.toml)\n\n\
CONFIG FILE:\n  \
  Location: ./kaptaind.toml or ~/.kaptaind/config.toml\n  \
  Run init to generate:\n  \
    kaptaind-cli init\n  \
  Then start daemon:\n  \
    kaptaind --daemon\n\n\
DOCUMENTATION:\n  \
  https://github.com/elci-group/kaptaind\n  \
  https://github.com/elci-group/kaptaind/blob/main/README.md\n  \
  https://github.com/elci-group/kaptaind/blob/main/INSTALL.md"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 📁 Repository path (overrides kaptaind.toml)
    ///
    /// If specified, operates on this repository instead of reading from
    /// kaptaind.toml. Useful for multi-repo workflows.
    #[arg(short, long)]
    repo: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// 🟢 View current daemon health and version
    ///
    /// Shows the daemon's current state (Idle, Clustering, Testing, Committing, Failed),
    /// the current version, installed binary locations, and recent error messages if any.
    ///
    /// Usage: kaptaind-cli status
    Status,

    /// 📜 View recent automated commits and analysis decisions
    ///
    /// Lists the last N commits made by kaptaind, including version bumps, scores,
    /// and the reasons for each bump (API, deps, runtime, bundle changes).
    ///
    /// Examples:
    ///   kaptaind-cli log                    # Last 10 commits (default)
    ///   kaptaind-cli log --limit 50         # Last 50 commits
    ///   kaptaind-cli log -l 5               # Last 5 commits
    Log {
        /// Number of commits to display (default: 10)
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },

    /// 🔬 Analyze working tree without committing (dry-run)
    ///
    /// Performs a full semantic diff analysis on your current uncommitted changes
    /// without actually committing. Shows what version bump would be made and why.
    /// Great for testing bump logic before the daemon sees the changes.
    ///
    /// Output includes: score breakdown, API changes, dependency changes, runtime changes,
    /// projected version bump.
    ///
    /// Usage: kaptaind-cli analyze
    Analyze,

    /// 🎯 Manage Aim of Change sessions
    ///
    /// Group related changes into named intent-driven sessions with full traceability.
    /// Useful for features, refactors, or coordinated multi-file changes.
    ///
    /// Examples:
    ///   kaptaind-cli aoc start "feature: auth flow"
    ///   kaptaind-cli aoc status
    ///   kaptaind-cli aoc ship
    ///   kaptaind-cli aoc intercept --intent "refactor" -- npm test
    #[command(subcommand)]
    Aoc(AocCommand),

    /// ⚙️ Initialize kaptaind config for the current project
    ///
    /// Auto-generates kaptaind.toml based on project type detection:
    /// - Rust (Cargo.toml) → cargo test, cargo build
    /// - Node (package.json) → npm test, npm run build
    /// - Python (pyproject.toml) → pytest, python -m build
    /// - Go (go.mod) → go test, go build
    /// - And more...
    ///
    /// Also creates .kaptainignore with sensible defaults.
    ///
    /// Usage: kaptaind-cli init
    Init,

    /// 📊 Live terminal dashboard
    ///
    /// Real-time view of kaptaind's state: version, daemon status, stability score,
    /// LLM costs, release history, and recent analysis artifacts.
    ///
    /// Perfect for monitoring your automation at a glance. Updates intelligently
    /// by reading the latest .kaptaind/ state files.
    ///
    /// Usage: kaptaind-cli dashboard
    Dashboard,

    /// 🚀 Emit release/hold recommendation for CI/CD pipelines
    ///
    /// Determines if the current state qualifies for release based on:
    /// - Stability score vs threshold
    /// - Pass streak (trailing passing commits)
    /// - Diff-spike guard (prevents releasing during volatile periods)
    /// - Cooldown (minimum time between releases)
    ///
    /// Output formats:
    ///   text   → Human-readable summary (default)
    ///   json   → Machine-readable JSON for tooling
    ///   github → GitHub Actions annotations + set-output
    ///
    /// Examples:
    ///   kaptaind-cli ci-hint                    # Text output
    ///   kaptaind-cli ci-hint --format json      # JSON for scripting
    ///   kaptaind-cli ci-hint --format github    # GitHub Actions
    ///
    /// Usage in GitHub Actions:
    ///   - name: Check release qualification
    ///     id: qualify
    ///     run: kaptaind-cli ci-hint --format github
    ///   - name: Release
    ///     if: steps.qualify.outputs.qualified == 'true'
    ///     run: ./scripts/release.sh
    CiHint {
        /// Output format: text (default), json, or github (GitHub Actions format)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// ✅ Enable auto-start for kaptaind daemon
    ///
    /// Configures the system to automatically start kaptaind on boot or shell login.
    ///
    /// Linux/systemd: Installs a user systemd service that starts with the user session
    /// macOS: Adds a launchd plist to ~/.Library/LaunchAgents/
    /// Cross-platform fallback: Adds shell initialization to ~/.bashrc and ~/.zshrc
    ///
    /// After enabling, the daemon will start automatically on next login/reboot.
    ///
    /// Usage: kaptaind-cli enable-autostart
    EnableAutostart,

    /// ❌ Disable auto-start for kaptaind daemon
    ///
    /// Removes auto-start configuration so kaptaind no longer starts automatically.
    ///
    /// For systemd: Disables the user service
    /// For launchd: Removes the plist
    /// For shell: Removes the init lines from ~/.bashrc and ~/.zshrc
    ///
    /// Usage: kaptaind-cli disable-autostart
    DisableAutostart,

    /// 🚀 Start all registered kaptaind daemons
    ///
    /// Reads ~/.kaptaind/projects.txt and launches a kaptaind daemon for each initialized project.
    /// Used internally by the auto-start system.
    Autostart,
}

#[derive(Subcommand)]
enum AocCommand {
    /// 🎯 Start a new Aim of Change session
    ///
    /// Begins a named session to group related commits under a single intent.
    /// All commits while the session is active will be tagged with this label
    /// and linked in the manifest.
    ///
    /// Session state is stored in .kaptaind/aoc/active.json
    /// When shipped, it's archived to .kaptaind/aoc/manifests/<id>.json
    ///
    /// Examples:
    ///   kaptaind-cli aoc start "feature: authentication flow"
    ///   kaptaind-cli aoc start "refactor: database layer"
    ///   kaptaind-cli aoc start "fix: memory leaks"
    Start {
        /// User-friendly name for this session (required)
        label: String,
    },

    /// 🚢 End and ship the current session
    ///
    /// Finalizes the active Aim of Change session, creates a manifest with:
    /// - Session name and ID
    /// - Start and end timestamps
    /// - All commits included
    /// - Version progression (start -> end)
    /// - Test pass/fail summary
    ///
    /// Useful for linking to deploys, release notes, or audit logs.
    ///
    /// Usage: kaptaind-cli aoc ship
    Ship,

    /// 📋 Show status of the current session
    ///
    /// Displays active session name, number of commits so far, and timeline.
    /// Returns error if no session is active.
    ///
    /// Usage: kaptaind-cli aoc status
    Status,

    /// 🔍 Intercept agent operations for contextual tracing
    ///
    /// Wraps a command (test, build, script) and captures:
    /// - Command output
    /// - Exit code
    /// - Execution time
    /// - Optionally: agent model name and intent description
    ///
    /// Logs are attached to the current Aim of Change session for full traceability.
    /// Great for audit trails in regulated environments.
    ///
    /// Examples:
    ///   kaptaind-cli aoc intercept -- npm test
    ///   kaptaind-cli aoc intercept --model claude-3-5-sonnet -- cargo test
    ///   kaptaind-cli aoc intercept --intent \"refactor auth\" -- npm test
    ///
    /// Usage in scripts:
    ///   if kaptaind-cli aoc intercept --model my-model -- ./my_test.sh; then
    ///     echo "Tests passed, changes are safe"
    ///   fi
    Intercept {
        /// 🤖 Agent/LLM model name (e.g., claude-3-5-sonnet, gpt-4, local-llama)
        ///
        /// Optional label for which AI model made the change. Useful for tracking
        /// which agent generated commits.
        #[arg(short, long)]
        model: Option<String>,

        /// 💡 Intent or task description
        ///
        /// High-level description of what the agent is trying to do. Stored in
        /// the trace for context and auditing.
        #[arg(short, long)]
        intent: Option<String>,

        /// Command to wrap and execute (everything after --)
        command: String,

        /// Arguments for the command
        args: Vec<String>,
    },

    /// 📚 View completed Aim of Change sessions
    ///
    /// Lists shipped AoC sessions with their manifests, showing:
    /// - Session name and ID
    /// - Start and end times
    /// - Version change (e.g., v1.2.0 → v1.3.0)
    /// - Commit count
    /// - Test results
    ///
    /// Examples:
    ///   kaptaind-cli aoc log                  # Last 10 sessions (default)
    ///   kaptaind-cli aoc log --limit 50       # Last 50 sessions
    Log {
        /// Number of sessions to display (default: 10)
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
        Commands::Dashboard => {
            handle_dashboard(&config)?;
        }
        Commands::CiHint { format } => {
            handle_ci_hint(&config, format)?;
        }
        Commands::EnableAutostart => {
            handle_enable_autostart()?;
        }
        Commands::DisableAutostart => {
            handle_disable_autostart()?;
        }
        Commands::Autostart => {
            handle_autostart()?;
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

    // Register project for autostart
    if let Ok(home) = std::env::var("HOME") {
        let kaptaind_dir = std::path::Path::new(&home).join(".kaptaind");
        let _ = fs::create_dir_all(&kaptaind_dir);
        let projects_file = kaptaind_dir.join("projects.txt");
        
        let path_str = root.display().to_string();
        let mut add = true;
        
        if projects_file.exists() {
            if let Ok(content) = fs::read_to_string(&projects_file) {
                if content.lines().any(|l| l.trim() == path_str) {
                    add = false;
                }
            }
        }
        
        if add {
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&projects_file) {
                use std::io::Write;
                let _ = writeln!(file, "{}", path_str);
            }
        }
    }

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
    let bump = kaptaind::version::decide(&weight, &config.version_thresholds);

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

fn handle_dashboard(config: &Config) -> anyhow::Result<()> {
    let kd = config.repo_path.join(".kaptaind");

    // --- Version ---
    let version = fs::read_to_string(config.repo_path.join("VERSION"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // --- Daemon status ---
    let daemon_state = fs::read_to_string(kd.join("status.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::daemon::scheduler::StatusReport>(&s).ok());

    // --- Telemetry ---
    let telemetry = fs::read_to_string(kd.join("telemetry.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::daemon::telemetry::TokenMetrics>(&s).ok());

    // --- Stability ---
    let stability = fs::read_to_string(kd.join("stability.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::stability::model::StabilityRecord>(&s).ok());

    // --- Releases index ---
    let release_index = fs::read_to_string(kd.join("releases").join("index.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::release::orchestrator::ReleaseIndex>(&s).ok());

    // --- Recent analyses ---
    let analysis_dir = kd.join("analysis");
    let mut recent_analyses: Vec<kaptaind::daemon::scheduler::AnalysisArtifact> = Vec::new();
    if analysis_dir.exists() {
        let mut entries: Vec<_> = fs::read_dir(&analysis_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for entry in entries.iter().take(5) {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(a) = serde_json::from_str::<kaptaind::daemon::scheduler::AnalysisArtifact>(&content) {
                    recent_analyses.push(a);
                }
            }
        }
    }

    // ======= Render =======
    println!();
    println!("{}", "╔══════════════════════════════════════════════╗".cyan());
    println!("{}", "║          kaptaind  ·  Live Dashboard         ║".cyan().bold());
    println!("{}", "╚══════════════════════════════════════════════╝".cyan());
    println!();

    // Version + daemon
    println!("{}", "── Project ─────────────────────────────────────".bright_black());
    println!("  {}  {}", "Version:".bold(), version.magenta().bold());
    println!("  {}  {}", "Repo:   ".bold(), config.repo_path.display().to_string().blue());
    if let Some(ref st) = daemon_state {
        let state_str = match st.status {
            kaptaind::daemon::scheduler::State::Idle => "Idle".green().to_string(),
            kaptaind::daemon::scheduler::State::Clustering => "Clustering".cyan().to_string(),
            kaptaind::daemon::scheduler::State::Testing => "Testing".yellow().to_string(),
            kaptaind::daemon::scheduler::State::Committing => "Committing".magenta().to_string(),
            kaptaind::daemon::scheduler::State::Failed => "Failed".red().bold().to_string(),
        };
        println!("  {}  {}", "Daemon: ".bold(), state_str);
        if let Some(ref err) = st.last_error {
            println!("  {}  {}", "Error:  ".bold(), err.red());
        }
    } else {
        println!("  {}  {}", "Daemon: ".bold(), "Not running / no status file".bright_black());
    }
    println!();

    // Stability
    println!("{}", "── Stability ───────────────────────────────────".bright_black());
    if let Some(ref s) = stability {
        let bar = stability_bar(s.score);
        let score_colored = if s.score >= 0.85 {
            format!("{:.3}", s.score).green().bold().to_string()
        } else if s.score >= 0.6 {
            format!("{:.3}", s.score).yellow().to_string()
        } else {
            format!("{:.3}", s.score).red().to_string()
        };
        println!("  Score:  {} {}  {}", bar, score_colored, format!("({} commits tracked)", s.history.len()).bright_black());
        if let Some(reg_ts) = s.last_regression {
            let reg_dt = chrono::DateTime::<chrono::Utc>::from_timestamp(reg_ts, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("  Last regression: {}", reg_dt.yellow());
        }
    } else {
        println!("  {}", "No stability data yet.".bright_black());
    }
    println!();

    // Telemetry
    println!("{}", "── Telemetry ───────────────────────────────────".bright_black());
    if let Some(ref t) = telemetry {
        println!("  {}  ${:.4}  (${:.6} this session)", "LLM cost:".bold(), t.aggregate_cost, t.marginal_cost);
        println!("  {}  {}  failed: {}", "Releases:".bold(), t.releases.to_string().green(), t.failed_releases.to_string().red());
    } else {
        println!("  {}", "No telemetry data.".bright_black());
    }
    println!();

    // Recent releases
    println!("{}", "── Releases ────────────────────────────────────".bright_black());
    if let Some(ref idx) = release_index {
        if idx.releases.is_empty() {
            println!("  {}", "No releases yet.".bright_black());
        } else {
            for entry in idx.releases.iter().rev().take(5) {
                let ts = chrono::DateTime::<chrono::Utc>::from_timestamp(entry.released_at, 0)
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!(
                    "  {} {}  {}  S={:.3}",
                    "▸".green(),
                    format!("v{}", entry.version).magenta().bold(),
                    ts.bright_black(),
                    entry.stability
                );
            }
        }
    } else {
        println!("  {}", "No release index found.".bright_black());
    }
    println!();

    // Recent analyses
    println!("{}", "── Recent Analyses ─────────────────────────────".bright_black());
    if recent_analyses.is_empty() {
        println!("  {}", "No analyses yet.".bright_black());
    } else {
        for a in &recent_analyses {
            let bump_sym = match a.bump.as_str() {
                "Major" => "🚀".to_string(),
                "Minor" => "✨".to_string(),
                "Patch" => "🩹".to_string(),
                _ => "─".to_string(),
            };
            println!(
                "  {} {}  score={:.3}  bump={}{}  paths={}",
                bump_sym,
                a.version.magenta(),
                a.weight.score,
                a.bump.cyan(),
                if a.weight.api_breaking { " [BREAKING]".red().to_string() } else { String::new() },
                a.diff.touched_paths
            );
        }
    }
    println!();

    Ok(())
}

fn stability_bar(score: f64) -> String {
    let filled = (score * 20.0).round() as usize;
    let empty = 20usize.saturating_sub(filled);
    let bar: String = std::iter::repeat('█').take(filled)
        .chain(std::iter::repeat('░').take(empty))
        .collect();
    format!("[{}]", bar)
}

fn handle_ci_hint(config: &Config, format: &str) -> anyhow::Result<()> {
    let kd = config.repo_path.join(".kaptaind");

    let stability = fs::read_to_string(kd.join("stability.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::stability::model::StabilityRecord>(&s).ok());

    let release_index = fs::read_to_string(kd.join("releases").join("index.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::release::orchestrator::ReleaseIndex>(&s).ok());

    let current_score = stability.as_ref().map(|s| s.score).unwrap_or(0.0);
    let pass_streak = stability.as_ref().map(|s| kaptaind::stability::engine::pass_streak(s)).unwrap_or(0);
    let threshold = config.qualification.stability_threshold;
    let min_streak = config.qualification.min_pass_streak;

    let qualified = current_score >= threshold && pass_streak >= min_streak;
    let last_version = release_index
        .as_ref()
        .and_then(|idx| idx.releases.last())
        .map(|e| e.version.clone())
        .unwrap_or_else(|| "none".to_string());
    let current_version = fs::read_to_string(config.repo_path.join("VERSION"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    match format {
        "json" => {
            let out = serde_json::json!({
                "qualified": qualified,
                "stability_score": current_score,
                "pass_streak": pass_streak,
                "threshold": threshold,
                "min_streak": min_streak,
                "current_version": current_version,
                "last_released_version": last_version,
                "recommendation": if qualified { "release" } else { "hold" }
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        "github" => {
            // GitHub Actions workflow command format
            if qualified {
                println!("::notice title=kaptaind::Release qualified — v{current_version} (stability={current_score:.3}, streak={pass_streak})");
                println!("::set-output name=qualified::true");
                println!("::set-output name=version::{current_version}");
            } else {
                println!("::warning title=kaptaind::Hold — stability={current_score:.3} (need {threshold:.3}), streak={pass_streak} (need {min_streak})");
                println!("::set-output name=qualified::false");
                println!("::set-output name=version::{current_version}");
            }
        }
        _ => {
            // Plain text
            let status_str = if qualified {
                "RELEASE".green().bold().to_string()
            } else {
                "HOLD".yellow().bold().to_string()
            };
            println!("{} {}", "CI Hint:".bold(), status_str);
            println!("  Stability score : {:.3}  (threshold: {:.3})", current_score, threshold);
            println!("  Pass streak     : {}  (required: {})", pass_streak, min_streak);
            println!("  Current version : {}", current_version.magenta());
            println!("  Last release    : {}", last_version.blue());
            if qualified {
                println!("  → Recommendation: {}", "ship v".green().to_string() + &current_version);
            } else {
                let missing_score = (threshold - current_score).max(0.0);
                let missing_streak = min_streak.saturating_sub(pass_streak);
                if missing_score > 0.001 {
                    println!("  → Need +{:.3} stability score to qualify", missing_score);
                }
                if missing_streak > 0 {
                    println!("  → Need {} more passing commit(s) in streak", missing_streak);
                }
            }
        }
    }

    Ok(())
}

fn handle_enable_autostart() -> anyhow::Result<()> {
    use std::process::Command;

    let home = std::env::var("HOME")?;
    let kaptaind_path = format!("{}/.local/bin/kaptaind", home);

    // Verify kaptaind exists
    if !std::path::Path::new(&kaptaind_path).exists() {
        anyhow::bail!("kaptaind not found at {}. Run install.sh first.", kaptaind_path);
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        // Set up systemd user service
        let systemd_dir = format!("{}/.config/systemd/user", home);
        std::fs::create_dir_all(&systemd_dir)?;

        let service_content = format!(
            r#"[Unit]
Description=Kaptaind - Automated Semantic Versioning Daemon
Documentation=https://github.com/elci-group/kaptaind
After=network.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart={}-cli autostart
StandardOutput=journal
StandardError=journal
SyslogIdentifier=kaptaind
Environment="RUST_LOG=info"

[Install]
WantedBy=default.target
"#,
            kaptaind_path
        );

        let service_path = format!("{}/kaptaind.service", systemd_dir);
        std::fs::write(&service_path, service_content)?;

        // Enable the service
        Command::new("systemctl")
            .args(&["--user", "daemon-reload"])
            .output()?;

        Command::new("systemctl")
            .args(&["--user", "enable", "kaptaind.service"])
            .output()?;

        println!("{} {}", "✓".green(), "Auto-start enabled via systemd user service".green());
        println!("  Service file: {}/kaptaind.service", systemd_dir);
        println!("  Auto-start on next login with: systemctl --user start kaptaind");
    }

    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        // Set up launchd plist
        let launchd_dir = format!("{}/.Library/LaunchAgents", home);
        std::fs::create_dir_all(&launchd_dir)?;

        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.elcigroup.kaptaind</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}-cli</string>
    <string>autostart</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{}/.kaptaind/daemon.out</string>
  <key>StandardErrorPath</key>
  <string>{}/.kaptaind/daemon.err</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>RUST_LOG</key>
    <string>info</string>
  </dict>
</dict>
</plist>
"#,
            kaptaind_path, home, home
        );

        let plist_path = format!("{}/com.elcigroup.kaptaind.plist", launchd_dir);
        std::fs::write(&plist_path, plist_content)?;

        println!("{} {}", "✓".green(), "Auto-start enabled via launchd plist".green());
        println!("  Plist file: {}", plist_path);
        println!("  Auto-start on next login");
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        // Fallback: shell initialization
        setup_shell_autostart(&home, &kaptaind_path)?;
    }

    Ok(())
}

fn handle_disable_autostart() -> anyhow::Result<()> {
    use std::process::Command;

    let home = std::env::var("HOME")?;

    #[cfg(target_os = "linux")]
    {
        // Disable systemd user service
        Command::new("systemctl")
            .args(&["--user", "disable", "kaptaind.service"])
            .output()?;

        let service_path = format!("{}/.config/systemd/user/kaptaind.service", home);
        if std::path::Path::new(&service_path).exists() {
            std::fs::remove_file(&service_path)?;
        }

        println!("{} {}", "✓".green(), "Auto-start disabled (systemd service removed)".green());
    }

    #[cfg(target_os = "macos")]
    {
        // Disable launchd plist
        let plist_path = format!("{}/.Library/LaunchAgents/com.elcigroup.kaptaind.plist", home);

        Command::new("launchctl")
            .args(&["unload", &plist_path])
            .output()
            .ok();

        if std::path::Path::new(&plist_path).exists() {
            std::fs::remove_file(&plist_path)?;
        }

        println!("{} {}", "✓".green(), "Auto-start disabled (launchd plist removed)".green());
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        // Fallback: shell initialization
        remove_shell_autostart(&home)?;
    }

    Ok(())
}

fn setup_shell_autostart(home: &str, kaptaind_path: &str) -> anyhow::Result<()> {
    let autostart_line = format!("# Auto-start kaptaind\nexport PATH=\"$HOME/.local/bin:$PATH\"\n{}-cli autostart > /dev/null 2>&1\n", kaptaind_path);

    for rc_file in &[".bashrc", ".zshrc"] {
        let rc_path = format!("{}/{}", home, rc_file);
        if !std::path::Path::new(&rc_path).exists() {
            continue;
        }

        let content = std::fs::read_to_string(&rc_path)?;
        if !content.contains("Auto-start kaptaind") {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&rc_path)?;
            use std::io::Write;
            writeln!(file, "\n{}", autostart_line)?;
        }
    }

    println!("{} {}", "✓".green(), "Auto-start enabled via shell initialization".green());
    println!("  Added to ~/.bashrc and ~/.zshrc");
    println!("  Auto-start on next shell login");

    Ok(())
}

fn remove_shell_autostart(home: &str) -> anyhow::Result<()> {
    for rc_file in &[".bashrc", ".zshrc"] {
        let rc_path = format!("{}/{}", home, rc_file);
        if !std::path::Path::new(&rc_path).exists() {
            continue;
        }

        let content = std::fs::read_to_string(&rc_path)?;
        if content.contains("Auto-start kaptaind") {
            let filtered: String = content
                .lines()
                .filter(|line| !line.contains("Auto-start kaptaind") && !line.contains("nohup") && !line.contains("kaptaind.*daemon"))
                .map(|line| format!("{}\n", line))
                .collect();
            std::fs::write(&rc_path, filtered)?;
        }
    }

    println!("{} {}", "✓".green(), "Auto-start disabled (shell initialization removed)".green());

    Ok(())
}

fn handle_autostart() -> anyhow::Result<()> {
    let home = std::env::var("HOME")?;
    let projects_file = format!("{}/.kaptaind/projects.txt", home);
    
    if !std::path::Path::new(&projects_file).exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(&projects_file)?;
    for line in contents.lines() {
        let path = line.trim();
        if path.is_empty() { continue; }
        
        let repo_path = std::path::PathBuf::from(path);
        if repo_path.join("kaptaind.toml").exists() {
            println!("Starting kaptaind for {}", path);
            std::process::Command::new("kaptaind")
                .arg("--daemon")
                .current_dir(repo_path)
                .spawn()
                .ok();
        }
    }
    
    Ok(())
}
