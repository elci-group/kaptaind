use kaptaind::config::loader::Config;
use kaptaind::daemon::decisions::{render_decisions, tail_decisions};
use kaptaind::util::style::*;

pub fn handle_explain(config: &Config, last: usize) -> anyhow::Result<()> {
    let records = tail_decisions(&config.repo_path, last)?;
    println!(
        "{} {}",
        "🧭".cyan(),
        format!("Last {} cluster decision(s)", records.len())
            .bold()
            .cyan()
    );
    println!("{}", "-".repeat(60).cyan());
    print!("{}", render_decisions(&records));
    Ok(())
}
