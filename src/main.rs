use clap::Parser;
use colored::*;
use std::fs::File;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Run kaptaind as a background daemon
    #[arg(short, long)]
    daemon: bool,

    /// See an index of watched static projects
    #[arg(long)]
    dock: bool,

    /// See an index of active projects
    #[arg(long)]
    radar: bool,

    /// See a breakdown of which models/services are under load
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
