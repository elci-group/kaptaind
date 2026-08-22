use crate::format_datetime;
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use std::fs;

pub fn handle_status(config: &Config) -> anyhow::Result<()> {
    let version_path = config.repo_path.join("VERSION");
    let version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "None (no VERSION file)".to_string()
    };

    println!("{} {}", "🚢".blue(), "Kaptaind Status".bold().blue());
    println!("{}", "=================".blue());
    println!(
        "{} {}",
        "📂 Repository: ".bold().cyan(),
        config.repo_path.display().to_string().blue()
    );
    println!("{} {}", "🏷️  Version:    ".bold().cyan(), version.magenta());

    let pid_running = get_daemon_pid(config);
    let status_json = config.repo_path.join(".kaptaind").join("status.json");
    let report = match fs::read_to_string(&status_json) {
        Ok(content) => {
            match serde_json::from_str::<kaptaind::daemon::scheduler::StatusReport>(&content) {
                Ok(report) => Some(report),
                Err(error) => {
                    tracing::warn!(path = %status_json.display(), error = %error, "status.json is malformed; showing status without daemon report");
                    None
                }
            }
        }
        Err(error) => {
            tracing::debug!(path = %status_json.display(), error = %error, "no status.json yet; daemon may never have run");
            None
        }
    };

    let is_suspended = report
        .as_ref()
        .map(|r| matches!(r.status, kaptaind::daemon::scheduler::State::Suspended))
        .unwrap_or(false);

    if is_suspended {
        // Suspended state is shown even when the daemon is not running,
        // because CLI suspend/resume write status.json directly.
        println!(
            "{} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            "[⏸️  Suspended]".yellow()
        );
        if let Some(report) = report {
            if let Some(ref err) = report.last_error {
                println!("{} {}", "   Reason:    ".cyan(), err.magenta());
            }
        }
        if let Ok(Some(suspend)) = kaptaind::daemon::suspend::load(&config.repo_path) {
            println!("{} {}", "   Source:    ".cyan(), suspend.source.yellow());
            println!(
                "{} {}",
                "   Since:     ".cyan(),
                format_datetime(suspend.since).blue()
            );
        }
        if let Some(pid) = pid_running {
            println!("{}", format!("   PID:       {pid}").blue());
        }
    } else if let Some(pid) = pid_running {
        let state_display = report
            .map(|r| match r.status {
                kaptaind::daemon::scheduler::State::Idle => "[💤 Idle]".blue().to_string(),
                kaptaind::daemon::scheduler::State::Clustering => {
                    "[🔍 Clustering]".cyan().to_string()
                }
                kaptaind::daemon::scheduler::State::Testing => "[🧪 Testing]".yellow().to_string(),
                kaptaind::daemon::scheduler::State::Committing => {
                    "[🚢 Committing]".magenta().to_string()
                }
                kaptaind::daemon::scheduler::State::Failed => "[🛑 Failed]".red().to_string(),
                kaptaind::daemon::scheduler::State::Stopping => {
                    "[⏹️  Stopping]".yellow().to_string()
                }
                kaptaind::daemon::scheduler::State::Stopped => {
                    "[⏹️  Stopped]".bright_black().to_string()
                }
                kaptaind::daemon::scheduler::State::Suspended => {
                    "[⏸️  Suspended]".yellow().to_string()
                }
            })
            .unwrap_or_else(|| "[🟢 Running]".green().to_string());

        println!(
            "{} {} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            state_display,
            format!("(PID: {})", pid).blue()
        );
    } else {
        println!(
            "{} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            "🛑 Stopped".red()
        );
    }

    Ok(())
}

fn get_daemon_pid(config: &Config) -> Option<i32> {
    let pid_file = config.repo_path.join(".kaptaind").join("daemon.pid");
    if let Ok(pid_str) = fs::read_to_string(pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            #[cfg(target_os = "linux")]
            {
                if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                    return Some(pid);
                }
            }
            #[cfg(all(unix, not(target_os = "linux")))]
            {
                // Signal 0 checks if the process is running and we have permissions to signal it.
                if unsafe { libc::kill(pid, 0) } == 0 {
                    return Some(pid);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = pid;
            }
        }
    }
    None
}
