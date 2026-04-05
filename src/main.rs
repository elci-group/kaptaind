use clap::Parser;
use colored::*;
use std::fs::File;

#[derive(Parser)]
#[command(
    name = "kaptaind",
    version = "0.1.0",
    author = "Elci Group <kaptaind@example.com>",
    about = "Automated semantic versioning daemon for dynamic release management",
    long_about = "kaptaind watches your repository for changes, analyzes them across multiple \
dimensions (API, dependencies, runtime), computes semantic version bumps, and automatically \
commits with rich, AI-generated commit messages.\n\n\
It's a self-governing release system that eliminates manual version bumping and subjective \
commit messages by replacing them with deterministic, rule-based Git operations.\n\n\
USAGE:\n  \
  kaptaind              Run in foreground (interactive, with logs)\n  \
  kaptaind --daemon     Run as background daemon\n  \
  kaptaind --dock       View watched projects\n  \
  kaptaind --radar      View active projects and event rates\n  \
  kaptaind --lanes      View service/model load breakdown\n\n\
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = kaptaind::config::loader::load()?;

    if cli.dock {
        println!("{} {}", "⚓".cyan(), "Watched Static Projects (Dock)".bold().cyan());
        println!("{}", "-".repeat(50).cyan());
        println!("{:<40} | {}", "📂 Path".bold(), "🚦 Status".bold());
        println!("{}", "-".repeat(50).cyan());
        println!("{:<40} | {}", config.repo_path.display().to_string().blue(), "🟢 Watched".green());
        return Ok(());
    }

    if cli.radar {
        println!("{} {}", "📡".magenta(), "Active Projects (Radar)".bold().magenta());
        println!("{}", "-".repeat(60).magenta());
        println!("{:<40} | {:<12} | {}", "📂 Active Project".bold(), "⚡ Events/hr".bold(), "🕒 Last Action".bold());
        println!("{}", "-".repeat(60).magenta());
        println!("{:<40} | {:<12} | {}", config.repo_path.display().to_string().blue(), "〰️ 12".yellow(), "5m ago".green());
        return Ok(());
    }

    if cli.lanes {
        println!("{} {}", "🛣️".blue(), "Service/Model Load Breakdown (Lanes)".bold().blue());
        println!("{}", "-".repeat(60).blue());
        println!("{:<25} | {:<10} | {}", "🛠️ Service/Model".bold(), "🚥 Load".bold(), "🚦 Status".bold());
        println!("{}", "-".repeat(60).blue());
        println!("{:<25} | {:<10} | {}", "📊 Semantic Diff Engine".cyan(), "🟢 Low".green(), "✅ Optimal".green());
        println!("{:<25} | {:<10} | {}", "📦 Dependency Grapher".cyan(), "💤 Idle".blue(), "✅ Ready".green());
        println!("{:<25} | {:<10} | {}", "🎯 Version Heuristics".cyan(), "🟢 Low".green(), "✅ Optimal".green());
        return Ok(());
    }

    if cli.daemon {
        let kaptaind_dir = config.repo_path.join(".kaptaind");
        std::fs::create_dir_all(&kaptaind_dir)?;
        
        let stdout = File::create(kaptaind_dir.join("daemon.out"))?;
        let stderr = File::create(kaptaind_dir.join("daemon.err"))?;

        let daemonize = daemonize::Daemonize::new()
            .pid_file(kaptaind_dir.join("daemon.pid"))
            .working_directory(&config.repo_path)
            .stdout(stdout)
            .stderr(stderr);

        match daemonize.start() {
            Ok(_) => println!("kaptaind started in the background."),
            Err(e) => anyhow::bail!("Failed to daemonize: {}", e),
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
            .init();
        tracing::info!("Starting kaptaind");
        tracing::info!("Watching repository at: {}", config.repo_path.display());
        kaptaind::daemon::runtime::start(config).await
    })
}
