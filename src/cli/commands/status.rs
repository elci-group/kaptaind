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
    if let Some(pid) = pid_running {
        let status_json = config.repo_path.join(".kaptaind").join("status.json");
        let mut state_display = "[🟢 Running]".green().to_string();

        if let Ok(content) = fs::read_to_string(&status_json) {
            if let Ok(report) =
                serde_json::from_str::<kaptaind::daemon::scheduler::StatusReport>(&content)
            {
                state_display = match report.status {
                    kaptaind::daemon::scheduler::State::Idle => "[💤 Idle]".blue().to_string(),
                    kaptaind::daemon::scheduler::State::Clustering => {
                        "[🔍 Clustering]".cyan().to_string()
                    }
                    kaptaind::daemon::scheduler::State::Testing => {
                        "[🧪 Testing]".yellow().to_string()
                    }
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
                };
            }
        }

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
            // Signal 0 checks if the process is running and we have permissions to signal it.
            if unsafe { libc::kill(pid, 0) } == 0 {
                return Some(pid);
            }
        }
    }
    None
}
