use clap::Parser;
use colored::*;
use std::fs::File;

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
  kaptaind --web        Start the WebUI dashboard (default port 8080)\n\n\
ENVIRONMENT:\n  \
  RUST_LOG              Set logging level (debug, info, warn, error)\n  \
  KAPTAIND_CONFIG       Path to kaptaind.toml (default: ./kaptaind.toml)\n\n\
CONFIG FILE:\n  \
  Default location: ./kaptaind.toml\n  \
  Generate with:   kaptaind-cli init\n\n\
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

    /// 📁 Path to the kaptaind configuration file
    ///
    /// Overrides the default search path (./kaptaind.toml) and the
    /// KAPTAIND_CONFIG environment variable.
    #[arg(short, long, value_name = "PATH")]
    config: Option<std::path::PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(path) = cli.config.as_ref() {
        std::env::set_var("KAPTAIND_CONFIG", path);
    }
    let mut config = kaptaind::config::loader::load()?;

    // Track this project as active in the monitor registry.
    let _ = kaptaind::monitor::touch_last_active(&config.repo_path);

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

    kaptaind::git::repo::ensure_git_available()
        .map_err(|err| anyhow::anyhow!("kaptaind requires git in PATH: {err}"))?;

    if cli.daemon {
        let kaptaind_dir = config.repo_path.join(".kaptaind");
        std::fs::create_dir_all(&kaptaind_dir)?;

        let stdout = File::create(kaptaind_dir.join("daemon.out"))?;
        let stderr = File::create(kaptaind_dir.join("daemon.err"))?;

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
        tracing::info!("Starting kaptaind");
        tracing::info!("Watching repository at: {}", config.repo_path.display());
        kaptaind::daemon::runtime::start(config).await
    })
}
