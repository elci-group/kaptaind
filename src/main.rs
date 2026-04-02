#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let config = kaptaind::config::loader::load()?;
    kaptaind::daemon::runtime::start(config).await?;
    Ok(())
}
