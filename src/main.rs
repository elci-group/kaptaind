use clap::Parser;
use std::fs::File;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Run kaptaind as a background daemon
    #[arg(short, long)]
    daemon: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = kaptaind::config::loader::load()?;

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
