use clap::{Parser, Subcommand};
use kaptaind::config::loader;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Status,
    Log {
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
    Analyze,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _config = loader::load()?;

    match &cli.command {
        Commands::Status => {
            println!("Status not implemented yet");
        }
        Commands::Log { limit } => {
            println!("Log not implemented yet (limit={limit})");
        }
        Commands::Analyze => {
            println!("Analyze not implemented yet");
        }
    }

    Ok(())
}
