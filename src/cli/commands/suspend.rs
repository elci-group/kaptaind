use kaptaind::config::loader::Config;
use kaptaind::util::style::*;

pub fn handle_suspend(config: &Config, reason: Option<&str>) -> anyhow::Result<()> {
    let state =
        kaptaind::daemon::suspend::SuspendState::suspended("manual", reason.map(|r| r.to_string()));
    kaptaind::daemon::suspend::save(&config.repo_path, &state)?;

    // Update status.json so the suspended state is visible even if the daemon
    // is not currently running.
    let mut status = kaptaind::daemon::status::StatusReport {
        status: kaptaind::daemon::scheduler::State::Idle,
        last_version: None,
        last_action_time: chrono::Utc::now(),
        last_error: None,
        current_task: None,
        progress_percent: None,
    };
    status.set_suspended(reason);
    kaptaind::daemon::status::write_status(&config.repo_path, &status);

    println!("{} {}", "⏸️".yellow(), "Daemon suspended.".bold().yellow());
    if let Some(r) = reason {
        println!("{} {}", "Reason:".cyan(), r.magenta());
    }

    Ok(())
}
