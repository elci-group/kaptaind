mod cluster;
mod commit;
mod config;
mod daemon;
mod diff;
mod git;
mod push;
mod version;
mod watcher;
mod weight;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let config = config::loader::load()?;
    daemon::runtime::start(config).await?;
    Ok(())
}
