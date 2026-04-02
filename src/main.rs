use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    tracing_subscriber::fmt::init();
    let config = kaptaind::config::loader::load()?;
    kaptaind::daemon::runtime::start(config).await?;
    Ok(())
}
