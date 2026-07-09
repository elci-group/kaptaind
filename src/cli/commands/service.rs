use crate::ServiceCommand;
use kaptaind::util::style::*;

pub fn handle_service(cmd: &ServiceCommand) -> anyhow::Result<()> {
    match cmd {
        ServiceCommand::Install { user, system } => {
            crate::monitor::install_service(*user, *system)?;
        }
        ServiceCommand::Uninstall { user, system } => {
            crate::monitor::uninstall_service(*user, *system)?;
        }
        ServiceCommand::InstallIcon { user, system } => {
            let target = kaptaind::icon::install_icon(*user, *system)?;
            println!(
                "{} {} {}",
                "✓".green(),
                "Logo installed to".green(),
                target.display().to_string().blue()
            );
            println!("  Notifications and launchers can now reference the icon by name: kaptaind");
        }
        ServiceCommand::Status { user, system } => {
            crate::monitor::service_status(*user, *system)?;
        }
    }
    Ok(())
}
