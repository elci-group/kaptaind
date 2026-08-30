use anyhow::Result;
use clap::{Parser, Subcommand};
use kaptaind::supervisor::config::SupervisorConfig;
use kaptaind::supervisor::reconcile::{OsWorkerControl, Supervisor};
use kaptaind::supervisor::store::AtomicSnapshotStore;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(
    name = "kaptaind-supervisor",
    version,
    about = "Padagonia-backed supervisor for isolated kaptaind project workers"
)]
struct Cli {
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the resident supervisor and loopback control API.
    Run,
    /// Reconcile once and exit.
    Once {
        /// Calculate actions without starting workers or persisting observations.
        #[arg(long)]
        dry_run: bool,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print the deterministic reconciliation plan without side effects.
    Plan {
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print aggregate supervisor status from the local continuity snapshot.
    Status {
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Import the legacy monitored.json registry into the versioned snapshot.
    Import {
        /// Override the legacy registry path.
        #[arg(long, value_name = "PATH")]
        legacy: Option<PathBuf>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    let mut config = SupervisorConfig::load(cli.config.as_deref())?;
    match cli.command {
        Command::Run => kaptaind::supervisor::runtime::run(config).await,
        Command::Once { dry_run, json } => {
            let worker = Arc::new(OsWorkerControl::new(config.worker_binary.clone()));
            let mut supervisor = Supervisor::bootstrap(config, worker).await?;
            let report = supervisor.reconcile(dry_run).await?;
            print_value(&report, json)
        }
        Command::Plan { json } => {
            let worker = Arc::new(OsWorkerControl::new(config.worker_binary.clone()));
            let supervisor = Supervisor::bootstrap(config, worker).await?;
            print_value(&supervisor.plan(), json)
        }
        Command::Status { json } => {
            let store = AtomicSnapshotStore::new(config.state_path);
            print_value(
                &kaptaind::supervisor::model::FleetStatus::from(&store.load()?),
                json,
            )
        }
        Command::Import { legacy, json } => {
            if let Some(path) = legacy {
                config.legacy_registry_path = path;
            }
            let registry = kaptaind::monitor::load_registry_at(&config.legacy_registry_path)?;
            let store = AtomicSnapshotStore::new(config.state_path);
            let mut snapshot = store.load()?;
            let summary = store.import_legacy(&mut snapshot, &registry)?;
            print_value(&summary, json)
        }
    }
}

fn print_value(value: &impl serde::Serialize, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{}", serde_json::to_string(value)?);
    }
    Ok(())
}
