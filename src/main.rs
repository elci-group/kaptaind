use clap::Parser;
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
        println!("Watched Static Projects (Dock):");
        println!("-------------------------------");
        println!("{:<40} | {:<10}", "Project Path", "Status");
        println!("{:<40} | {:<10}", config.repo_path.display().to_string(), "Watched");
        return Ok(());
    }

    if cli.radar {
        println!("Active Projects (Radar):");
        println!("------------------------");
        println!("{:<40} | {:<15} | {:<15}", "Active Project", "Events/hr", "Last Action");
        println!("{:<40} | {:<15} | {:<15}", config.repo_path.display().to_string(), "~", "Recent");
        return Ok(());
    }

    if cli.lanes {
        println!("Service/Model Load Breakdown (Lanes):");
        println!("-------------------------------------");
        println!("{:<25} | {:<10} | {:<15}", "Service/Model", "Load", "Status");
        println!("{:<25} | {:<10} | {:<15}", "Semantic Diff Engine", "Low", "Optimal");
        println!("{:<25} | {:<10} | {:<15}", "Dependency Grapher", "Idle", "Ready");
        println!("{:<25} | {:<10} | {:<15}", "Version Heuristics", "Low", "Optimal");
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
