use kaptaind::util::style::*;

pub fn handle_enable_autostart() -> anyhow::Result<()> {
    eprintln!(
        "{} {} {}",
        "⚠️".yellow(),
        "enable-autostart is deprecated.".yellow().bold(),
        "Use: kaptaind-cli service install --user".cyan()
    );

    crate::monitor::install_service(true, false)
}

pub fn handle_disable_autostart() -> anyhow::Result<()> {
    eprintln!(
        "{} {} {}",
        "⚠️".yellow(),
        "disable-autostart is deprecated.".yellow().bold(),
        "Use: kaptaind-cli service uninstall --user".cyan()
    );

    crate::monitor::uninstall_service(true, false)
}
