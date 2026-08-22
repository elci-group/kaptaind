use kaptaind::config::loader::Config;
use kaptaind::util::style::*;

pub fn handle_resume(config: &Config) -> anyhow::Result<()> {
    kaptaind::daemon::suspend::remove(&config.repo_path)?;

    // Update status.json so the resumed state is visible immediately.
    let mut status = kaptaind::daemon::status::StatusReport {
        status: kaptaind::daemon::scheduler::State::Idle,
        last_version: None,
        last_action_time: chrono::Utc::now(),
        last_error: None,
        current_task: None,
        progress_percent: None,
    };
    status.set_idle();
    kaptaind::daemon::status::write_status(&config.repo_path, &status);

    println!("{} {}", "▶️".green(), "Daemon resumed.".bold().green());

    Ok(())
}
