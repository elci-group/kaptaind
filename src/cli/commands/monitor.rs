use kaptaind::util::style::*;
use std::path::PathBuf;

use crate::MonitorCommand;

pub fn handle_monitor(cmd: &MonitorCommand) -> anyhow::Result<()> {
    match cmd {
        MonitorCommand::Add {
            path,
            config,
            port,
            enabled,
        } => {
            let project_path = path
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            crate::monitor::add(&project_path, config.as_deref(), *port, *enabled)?;
            println!(
                "{} {} {}",
                "✅".green(),
                "Registered".green(),
                project_path.display().to_string().blue()
            );
        }
        MonitorCommand::Remove { path } => {
            if crate::monitor::remove(path)? {
                println!(
                    "{} {} {}",
                    "✅".green(),
                    "Removed".green(),
                    path.display().to_string().blue()
                );
            } else {
                anyhow::bail!("Project not registered: {}", path.display());
            }
        }
        MonitorCommand::List => {
            crate::monitor::list()?;
        }
        MonitorCommand::Enable { path } => {
            crate::monitor::set_enabled(path, true)?;
            println!(
                "{} {} {}",
                "✅".green(),
                "Enabled".green(),
                path.display().to_string().blue()
            );
        }
        MonitorCommand::Disable { path } => {
            crate::monitor::set_enabled(path, false)?;
            println!(
                "{} {} {}",
                "✅".green(),
                "Disabled".green(),
                path.display().to_string().blue()
            );
        }
        MonitorCommand::Resume => {
            crate::monitor::resume()?;
        }
    }
    Ok(())
}
