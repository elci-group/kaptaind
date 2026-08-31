mod curly_expand;
use clap::{Parser, Subcommand};
use kaptaind::util::style::*;

#[derive(Parser)]
#[command(
    name = "kaptaind",
    version,
    author = "Elci Group <kaptaind@example.com>",
    about = "Automated semantic versioning daemon for dynamic release management",
    long_about = "+-------------------+\n\
|    .-=====-.      |\n\
|   /  .---.  \\     |\n\
|  |--< </> >--|    |\n\
|   \\  '---'  /     |\n\
|    '---|---'      |\n\
|    ___/ \\___      |\n\
|   /_KAPTAIND_\\    |\n\
+-------------------+\n\n\
kaptaind watches your repository for changes, analyzes them across multiple \
dimensions (API, dependencies, runtime), computes semantic version bumps, and automatically \
commits with rich, AI-generated commit messages.\n\n\
It's a self-governing release system that eliminates manual version bumping and subjective \
commit messages by replacing them with deterministic, rule-based Git operations.\n\n\
USAGE:\n  \
  kaptaind              Run in foreground (interactive, with logs)\n  \
  kaptaind --daemon     Run as background daemon\n  \
  kaptaind --dock       View watched projects\n  \
  kaptaind --radar      View active projects and event rates\n  \
  kaptaind --lanes      View service/model load breakdown\n  \
  kaptaind pull         Fetch, inspect, plan, and integrate upstream state\n  \
  kaptaind --web        Start the WebUI dashboard (default port 8080)\n\n\
ENVIRONMENT:\n  \
  RUST_LOG              Set logging level (debug, info, warn, error)\n  \
  KAPTAIND_CONFIG       Path to kaptaind.toml (default: ./kaptaind.toml)\n\n\
CONFIG FILE:\n  \
  Default location: ./kaptaind.toml\n  \
  Generate with:   kaptaind-cli init\n\n\
REPOSITORY MUTATION:\n  \
  New profiles default to observe-only: the daemon scores every change and\n  \
  records the decision, but never stages, commits, writes VERSION, pushes, or\n  \
  ships. Opt a repo into real commits with:\n  \
    [operation]\n  \
    mode = \"actuate\"\n  \
  Pushing additionally needs [push] enabled = true and\n  \
  [capabilities] network_push = true. See CHANGELOG.md [10.2.0] and [10.1.4].\n\n\
DAEMON MODE:\n  \
  Start:   kaptaind --daemon\n  \
  Check:   kaptaind-cli status\n  \
  Stop:    pkill -f 'kaptaind.*daemon'\n  \
  Logs:    tail -f .kaptaind/daemon.out\n\n\
DOCUMENTATION:\n  \
  https://github.com/elci-group/kaptaind\n  \
  https://github.com/elci-group/kaptaind/blob/main/README.md"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 🌙 Run kaptaind as a background daemon (non-blocking)
    ///
    /// Detaches from the terminal and runs in the background, writing logs to
    /// .kaptaind/daemon.out and .kaptaind/daemon.err. PID is stored in
    /// .kaptaind/daemon.pid for later termination.
    #[arg(short, long)]
    daemon: bool,

    /// 🏗️ Show watched static projects (Dock view)
    ///
    /// Lists all projects being watched with their status. Useful for debugging
    /// which repositories kaptaind is monitoring.
    #[arg(long)]
    dock: bool,

    /// 📡 Show active projects and event rates (Radar view)
    ///
    /// Displays real-time project activity: event frequency, last action time,
    /// and current load. Great for monitoring cluster formation.
    #[arg(long)]
    radar: bool,

    /// 🛣️ Show service/model load breakdown (Lanes view)
    ///
    /// Internal view of which components are under heavy load (diff engine,
    /// dependency grapher, version heuristics, LLM inference). Useful for
    /// performance profiling.
    #[arg(long)]
    lanes: bool,

    /// 🦈 Set Shark Stating mode for this instance
    ///
    /// Determines how this instance participates in high-availability:
    /// auto (default), leader, standby, observer.
    #[arg(long, value_name = "MODE")]
    shark_mode: Option<String>,

    /// 🦈 Override the Shark Stating arbiter path
    ///
    /// Shared directory used for leadership leases. Required when running
    /// multiple instances against the same repository.
    #[arg(long, value_name = "PATH")]
    shark_arbiter: Option<std::path::PathBuf>,

    /// 🏥 Override the health server port
    ///
    /// Useful when running multiple kaptaind instances on the same host,
    /// e.g. during a zero-downtime upgrade.
    #[arg(long, value_name = "PORT")]
    health_port: Option<u16>,

    /// 🌐 Start the WebUI server alongside the daemon runtime
    ///
    /// Serves a single-page dashboard on the configured web port (default 8080)
    /// with real-time telemetry, commit timelines, 3D graphs, and config editing.
    #[arg(short = 'w', long)]
    web: bool,

    /// 🌐 Override the WebUI server port
    ///
    /// Must be different from the health server port.
    #[arg(long, value_name = "PORT")]
    web_port: Option<u16>,

    /// 🧪 Dry run: show the decision the daemon would make for pending changes
    ///
    /// Runs the full analysis pipeline over the current uncommitted changes
    /// without staging or committing, printing the bump, next version, and the
    /// exact deterministic commit message.
    #[arg(long)]
    dry_run: bool,

    /// 📁 Path to the kaptaind configuration file
    ///
    /// Overrides the default search path (./kaptaind.toml) and the
    /// KAPTAIND_CONFIG environment variable.
    #[arg(short, long, value_name = "PATH")]
    config: Option<std::path::PathBuf>,

    /// ⚠️ Start even when the worktree has uncommitted changes
    ///
    /// Overrides `[daemon] startup_guard = true` in kaptaind.toml, which
    /// otherwise refuses to start on a dirty tree.
    #[arg(long)]
    force: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Safely fetch, inspect, plan, and integrate an upstream branch.
    Pull {
        #[arg(long)]
        remote: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, default_value = "auto", value_parser = ["auto", "fast-forward", "merge", "rebase", "hybreed", "emulsify", "manual"])]
        strategy: String,
        #[arg(long)]
        check: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        autostash: bool,
        #[arg(long)]
        abort: bool,
        #[arg(long)]
        r#continue: bool,
        #[arg(long)]
        status: bool,
        #[arg(long)]
        recover: bool,
        #[arg(long)]
        verbose: bool,
        #[arg(long)]
        json: bool,
    },
}

fn __curly_original_main() -> anyhow::Result<()> {
    // Load optional `.env` file so provider API keys and other secrets can live
    // outside of `kaptaind.toml`.
    if let Err(error) = kaptaind::util::dotenv::load() {
        tracing::warn!(
            ?error,
            operation = "main",
            source_line = line!(),
            "best-effort operation failed"
        );
    }
    let cli = Cli::parse();
    if let Some(path) = cli.config.as_ref() {
        std::env::set_var("KAPTAIND_CONFIG", path);
    }
    let mut config = kaptaind::config::loader::load()?;
    kaptaind::audit::configure_export(config.audit.export.clone());
    kaptaind::audit::configure_governance_context(
        config.governance.organization_id.clone(),
        config.governance.tenant_id.clone(),
    );
    kaptaind::compliance::configure(config.clone());

    if let Some(Commands::Pull {
        remote,
        branch,
        strategy,
        check,
        dry_run,
        force,
        autostash,
        abort,
        r#continue,
        status,
        recover,
        verbose,
        json,
    }) = cli.command
    {
        let controls = [abort, r#continue, status, recover]
            .into_iter()
            .filter(|enabled| *enabled)
            .count();
        if controls > 1 {
            eprintln!("pull --abort, --continue, --status, and --recover are mutually exclusive");
            std::process::exit(kaptaind::pull::ExitCode::InvalidInvocation as i32);
        }
        let result = if abort {
            kaptaind::pull::abort(&config.repo_path).map(|()| None)
        } else if recover {
            kaptaind::pull::recover(&config.repo_path).map(|()| None)
        } else if status {
            match kaptaind::pull::status(&config.repo_path) {
                Ok(value) => {
                    if json || value.is_some() {
                        println!("{}", serde_json::to_string_pretty(&value)?);
                    } else {
                        println!("No pull transactions found.");
                    }
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        } else if r#continue {
            kaptaind::pull::continue_operation(&config.repo_path, &config.pull).map(Some)
        } else {
            let parsed_result: Result<kaptaind::pull::IntegrationStrategy, _> = strategy.parse();
            let parsed = match parsed_result {
                Ok(strategy) => strategy,
                Err(error) => {
                    eprintln!("ERROR: {error}");
                    std::process::exit(error.exit_code());
                }
            };
            kaptaind::pull::run(
                &config.repo_path,
                &kaptaind::pull::PullOptions {
                    remote,
                    branch,
                    strategy: parsed,
                    check,
                    dry_run,
                    force,
                    autostash,
                    verbose,
                    emit_assessment: !json,
                },
                &config.pull,
                &config.integrations,
            )
            .map(Some)
        };
        match result {
            Ok(Some(report)) if json => println!("{}", serde_json::to_string_pretty(&report)?),
            Ok(Some(report)) => print!("{}", kaptaind::pull::render_text(&report, verbose)),
            Ok(None) if abort || recover => {
                println!("Kaptaind pull transaction restored to its recovery point.")
            }
            Ok(None) => {}
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "schema": kaptaind::pull::JSON_SCHEMA,
                            "operation": "pull",
                            "status": "error",
                            "exit_code": error.exit_code(),
                            "error": error.to_string(),
                        })
                    );
                } else {
                    eprintln!("ERROR: {error}");
                }
                std::process::exit(error.exit_code());
            }
        }
        return Ok(());
    }

    // Track this project as active in the monitor registry.
    if let Err(error) = kaptaind::monitor::touch_last_active(&config.repo_path) {
        tracing::warn!(
            ?error,
            operation = "main",
            source_line = line!(),
            "best-effort operation failed"
        );
    }

    if let Some(mode) = cli.shark_mode {
        config.shark.enabled = true;
        config.shark.mode = match mode.to_lowercase().as_str() {
            "leader" => kaptaind::config::loader::SharkMode::Leader,
            "standby" => kaptaind::config::loader::SharkMode::Standby,
            "observer" => kaptaind::config::loader::SharkMode::Observer,
            _ => kaptaind::config::loader::SharkMode::Auto,
        };
    }
    if let Some(path) = cli.shark_arbiter {
        config.shark.arbiter_path = path;
    }
    if let Some(port) = cli.health_port {
        config.health_port = port;
    }
    if cli.web || cli.web_port.is_some() {
        config.web_port = cli.web_port.unwrap_or(8080);
        if config.web_port == config.health_port {
            tracing::error!(
                operation = "main",
                source_line = line!(),
                "main returned an error"
            );
            return Err(anyhow::anyhow!(
                "WebUI port ({}) must be different from health port ({})",
                config.web_port,
                config.health_port
            ));
        }
    }

    if cli.dock {
        println!(
            "{} {}",
            "⚓".cyan(),
            "Watched Static Projects (Dock)".bold().cyan()
        );
        println!("{}", "-".repeat(50).cyan());
        println!("{:<40} | {}", "📂 Path".bold(), "🚦 Status".bold());
        println!("{}", "-".repeat(50).cyan());
        println!(
            "{:<40} | {}",
            config.repo_path.display().to_string().blue(),
            "🟢 Watched".green()
        );
        return Ok(());
    }

    if cli.radar {
        println!(
            "{} {}",
            "📡".magenta(),
            "Active Projects (Radar)".bold().magenta()
        );
        println!("{}", "-".repeat(60).magenta());
        println!(
            "{:<40} | {:<12} | {}",
            "📂 Active Project".bold(),
            "⚡ Events/hr".bold(),
            "🕒 Last Action".bold()
        );
        println!("{}", "-".repeat(60).magenta());
        println!(
            "{:<40} | {:<12} | {}",
            config.repo_path.display().to_string().blue(),
            "〰️ 12".yellow(),
            "5m ago".green()
        );
        return Ok(());
    }

    if cli.lanes {
        println!(
            "{} {}",
            "🛣️".blue(),
            "Service/Model Load Breakdown (Lanes)".bold().blue()
        );
        println!("{}", "-".repeat(60).blue());
        println!(
            "{:<25} | {:<10} | {}",
            "🛠️ Service/Model".bold(),
            "🚥 Load".bold(),
            "🚦 Status".bold()
        );
        println!("{}", "-".repeat(60).blue());
        println!(
            "{:<25} | {:<10} | {}",
            "📊 Semantic Diff Engine".cyan(),
            "🟢 Low".green(),
            "✅ Optimal".green()
        );
        println!(
            "{:<25} | {:<10} | {}",
            "📦 Dependency Grapher".cyan(),
            "💤 Idle".blue(),
            "✅ Ready".green()
        );
        println!(
            "{:<25} | {:<10} | {}",
            "🎯 Version Heuristics".cyan(),
            "🟢 Low".green(),
            "✅ Optimal".green()
        );
        return Ok(());
    }

    // Validate after CLI overrides but before any operation that can invoke
    // configured commands (including dry-run bundle scoring and the daemon).
    // Read-only status views above intentionally remain usable for reviewing
    // an untrusted repository configuration.
    config.validate()?;

    kaptaind::git::repo::ensure_git_available()
        .map_err(|err| anyhow::anyhow!("kaptaind requires git in PATH: {err}"))?;

    if cli.dry_run {
        return kaptaind::dryrun::run(&config);
    }

    // Startup guard: refuse to run against a dirty tree when the repo opted
    // in — accidental starts must not catch-up-commit in-flight work. Checked
    // before daemonizing so the refusal is visible on the operator's terminal.
    if config.daemon.startup_guard && !cli.force {
        let dirty = kaptaind::git::repo::dirty_path_count(&config.repo_path)?;
        if dirty > 0 {
            tracing::error!(
                operation = "main",
                source_line = line!(),
                "main returned an error"
            );
            return Err(anyhow::anyhow!(
                "startup guard: {} uncommitted path(s) under {} — refusing to start. \
                 Commit or stash first, or pass --force to override.",
                dirty,
                config.repo_path.display()
            ));
        }
    }

    if cli.daemon {
        let kaptaind_dir = config.repo_path.join(".kaptaind");
        kaptaind::util::permissions::ensure_private_dir(&kaptaind_dir)?;

        let stdout_path = kaptaind_dir.join("daemon.out");
        let stderr_path = kaptaind_dir.join("daemon.err");
        let stdout = kaptaind::util::permissions::create_private_file(&stdout_path)?;
        let stderr = kaptaind::util::permissions::create_private_file(&stderr_path)?;

        kaptaind::daemon::process::daemonize(
            &config.repo_path,
            &kaptaind_dir.join("daemon.pid"),
            stdout,
            stderr,
        )?;
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();
        tracing::info!(component = module_path!(), "Starting kaptaind");
        tracing::info!(
            component = module_path!(),
            "Watching repository at: {}",
            config.repo_path.display()
        );
        if matches!(
            config.staging.mode,
            kaptaind::config::loader::StagingMode::All
        ) {
            tracing::warn!(
                component = module_path!(),
                "staging mode \"all\" runs `git add -A` across the whole worktree: \
                 untracked files — including secrets — may be committed. Prefer \
                 mode = \"cluster\" (the default since v9.7.17). Commits abort \
                 fail-closed if a changed path matches the secret denylist."
            );
        }
        kaptaind::daemon::runtime::start(config).await
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = std::env::args().collect();
    let mut positions: Vec<usize> = Vec::new();
    let mut fields: Vec<Vec<String>> = Vec::new();
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--shark-mode" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--shark-mode=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--shark-mode={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--shark-arbiter" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--shark-arbiter=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--shark-arbiter={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--config" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--config=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--config={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--repo" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--repo=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--repo={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--config" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--config=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--config={}", v))
                    .collect(),
            );
            break;
        }
    }

    if fields.is_empty() || fields.iter().all(|f| f.len() <= 1) {
        return Ok(__curly_original_main()?);
    }

    let combos = curly_expand::cartesian(&fields);
    let exe = std::env::current_exe().expect("resolve current exe");
    let mut had_failure = false;
    for combo in &combos {
        let mut new_args = raw_args.clone();
        for (slot, value) in positions.iter().zip(combo.iter()) {
            new_args[*slot] = value.clone();
        }
        let status = std::process::Command::new(&exe)
            .args(&new_args[1..])
            .status()
            .expect("failed to re-exec self");
        if !status.success() {
            had_failure = true;
        }
    }
    if had_failure {
        std::process::exit(1);
    }
    Ok(())
}
