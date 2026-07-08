use colored::*;
use kaptaind::monitor::{load_registry, save_registry};
use std::path::Path;

pub use kaptaind::monitor::{add, remove, set_enabled};

/// Print a table of all registered monitored projects.
pub fn list() -> anyhow::Result<()> {
    let registry = load_registry()?;

    if registry.projects.is_empty() {
        println!(
            "{} {}",
            "ℹ️".blue(),
            "No monitored projects registered.".blue()
        );
        return Ok(());
    }

    let rows: Vec<Vec<String>> = registry
        .projects
        .iter()
        .map(|e| {
            vec![
                e.path.display().to_string().blue().to_string(),
                e.config.display().to_string().cyan().to_string(),
                if e.enabled {
                    "✅ enabled".green().to_string()
                } else {
                    "⏸️ disabled".yellow().to_string()
                },
                e.health_port.to_string().yellow().to_string(),
                e.last_active
                    .map(|dt| dt.to_rfc3339().dimmed().to_string())
                    .unwrap_or_else(|| "never".bright_black().to_string()),
            ]
        })
        .collect();

    crate::table::print_table(
        &[
            "📂 Path",
            "⚙️ Config",
            "🚦 Status",
            "🏥 Port",
            "🕒 Last Active",
        ],
        &rows,
    );

    Ok(())
}

/// Start a daemon for every enabled project that is not already running.
///
/// A project is considered already running when its `.kaptaind/daemon.pid`
/// file points to a live process.
pub fn resume() -> anyhow::Result<()> {
    let mut registry = load_registry()?;
    let mut started = 0usize;
    let mut skipped = 0usize;
    let now = chrono::Utc::now();

    for entry in registry.projects.iter_mut().filter(|e| e.enabled) {
        let pid_file = entry.path.join(".kaptaind").join("daemon.pid");
        if let Some(pid) = read_live_pid(&pid_file) {
            println!(
                "{} {} {} (PID {})",
                "⏭️".yellow(),
                "Already running:".yellow(),
                entry.path.display().to_string().blue(),
                pid.to_string().cyan()
            );
            skipped += 1;
            continue;
        }

        println!(
            "{} {} {}",
            "🚀".cyan(),
            "Starting daemon for".bold(),
            entry.path.display().to_string().blue()
        );

        let mut cmd = std::process::Command::new("kaptaind");
        cmd.arg("--daemon")
            .arg("--config")
            .arg(&entry.config)
            .arg("--health-port")
            .arg(entry.health_port.to_string())
            .current_dir(&entry.path);

        match cmd.spawn() {
            Ok(_) => {
                started += 1;
                entry.last_active = Some(now);
            }
            Err(err) => {
                eprintln!(
                    "{} Failed to start daemon for {}: {}",
                    "❌".red(),
                    entry.path.display(),
                    err
                );
            }
        }
    }

    save_registry(&registry)?;

    println!(
        "{} {} {}, {} {}",
        "✅".green(),
        "Done.".green().bold(),
        format!("{} started", started).green(),
        format!("{} skipped", skipped).yellow(),
        "(already running)".bright_black()
    );

    Ok(())
}

fn read_live_pid(pid_file: &Path) -> Option<i32> {
    let pid_str = std::fs::read_to_string(pid_file).ok()?;
    let pid = pid_str.trim().parse::<i32>().ok()?;
    if unsafe { libc::kill(pid, 0) } == 0 {
        Some(pid)
    } else {
        None
    }
}

/// Install the systemd/launchd service that runs `monitor resume` on login.
pub fn install_service(user: bool, system: bool) -> anyhow::Result<()> {
    if user && system {
        anyhow::bail!("Specify either --user or --system, not both.");
    }
    if !user && !system {
        anyhow::bail!("Specify --user or --system.");
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        if user {
            let home = std::env::var("HOME")?;
            let systemd_dir = format!("{}/.config/systemd/user", home);
            std::fs::create_dir_all(&systemd_dir)?;

            let service_content = r#"[Unit]
Description=Kaptaind - Automated Semantic Versioning Daemon
Documentation=https://github.com/elci-group/kaptaind
After=network.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=%h/.local/bin/kaptaind-cli monitor resume
StandardOutput=journal
StandardError=journal
SyslogIdentifier=kaptaind
Environment="RUST_LOG=info"

[Install]
WantedBy=default.target
"#;

            let service_path = format!("{}/kaptaind.service", systemd_dir);
            std::fs::write(&service_path, service_content)?;

            Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .output()?;
            Command::new("systemctl")
                .args(["--user", "enable", "kaptaind.service"])
                .output()?;

            println!(
                "{} {}",
                "✓".green(),
                "User service installed. kaptaind will resume monitored projects on login.".green()
            );
            println!("  Service file: {}", service_path);
            println!("  Start now: systemctl --user start kaptaind");
        } else {
            let service_path = "/etc/systemd/system/kaptaind.service";
            let binary = resolve_system_binary();

            let service_content = format!(
                r#"[Unit]
Description=Kaptaind - Automated Semantic Versioning Daemon
Documentation=https://github.com/elci-group/kaptaind
After=network.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart={} monitor resume
StandardOutput=journal
StandardError=journal
SyslogIdentifier=kaptaind
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
"#,
                binary.display()
            );

            match std::fs::write(service_path, service_content) {
                Ok(()) => {
                    Command::new("systemctl").args(["daemon-reload"]).output()?;
                    Command::new("systemctl")
                        .args(["enable", "kaptaind.service"])
                        .output()?;

                    println!(
                        "{} {}",
                        "✓".green(),
                        "System service installed. kaptaind will resume monitored projects at boot."
                            .green()
                    );
                    println!("  Service file: {}", service_path);
                    println!("  Start now: sudo systemctl start kaptaind");
                }
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    eprintln!(
                        "{} {}",
                        "❌".red(),
                        "Permission denied writing system service file.".red()
                    );
                    eprintln!("   Run: sudo kaptaind-cli service install --system");
                    anyhow::bail!("Permission denied");
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME")?;
        if user {
            let launchd_dir = format!("{}/Library/LaunchAgents", home);
            std::fs::create_dir_all(&launchd_dir)?;

            let plist_content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.elcigroup.kaptaind</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}/.local/bin/kaptaind-cli</string>
    <string>monitor</string>
    <string>resume</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{}/.kaptaind/daemon.out</string>
  <key>StandardErrorPath</key>
  <string>{}/.kaptaind/daemon.err</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>RUST_LOG</key>
    <string>info</string>
  </dict>
</dict>
</plist>
"#,
                home, home, home
            );

            let plist_path = format!("{}/com.elcigroup.kaptaind.plist", launchd_dir);
            std::fs::write(&plist_path, plist_content)?;

            println!(
                "{} {}",
                "✓".green(),
                "User LaunchAgent installed. kaptaind will resume monitored projects on login."
                    .green()
            );
            println!("  Plist file: {}", plist_path);
        } else {
            let binary = resolve_system_binary();
            let plist_path = "/Library/LaunchDaemons/com.elcigroup.kaptaind.plist";
            let plist_content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.elcigroup.kaptaind</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>monitor</string>
    <string>resume</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/var/log/kaptaind.out</string>
  <key>StandardErrorPath</key>
  <string>/var/log/kaptaind.err</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>RUST_LOG</key>
    <string>info</string>
  </dict>
</dict>
</plist>
"#,
                binary.display()
            );

            match std::fs::write(plist_path, plist_content) {
                Ok(()) => {
                    println!(
                        "{} {}",
                        "✓".green(),
                        "System LaunchDaemon installed. kaptaind will resume monitored projects at boot."
                            .green()
                    );
                    println!("  Plist file: {}", plist_path);
                }
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    eprintln!(
                        "{} {}",
                        "❌".red(),
                        "Permission denied writing system LaunchDaemon.".red()
                    );
                    eprintln!("   Run: sudo kaptaind-cli service install --system");
                    anyhow::bail!("Permission denied");
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        if system {
            anyhow::bail!("System-wide service installation is only supported on Linux and macOS.");
        }
        install_shell_autostart()?;
    }

    Ok(())
}

/// Remove the installed systemd/launchd service.
pub fn uninstall_service(user: bool, system: bool) -> anyhow::Result<()> {
    if user && system {
        anyhow::bail!("Specify either --user or --system, not both.");
    }
    if !user && !system {
        anyhow::bail!("Specify --user or --system.");
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        if user {
            Command::new("systemctl")
                .args(["--user", "disable", "kaptaind.service"])
                .output()
                .ok();

            let home = std::env::var("HOME")?;
            let service_path = format!("{}/.config/systemd/user/kaptaind.service", home);
            if Path::new(&service_path).exists() {
                std::fs::remove_file(&service_path)?;
            }
        } else {
            Command::new("systemctl")
                .args(["disable", "kaptaind.service"])
                .output()
                .ok();

            let service_path = "/etc/systemd/system/kaptaind.service";
            if Path::new(service_path).exists() {
                match std::fs::remove_file(service_path) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                        eprintln!(
                            "{} {}",
                            "❌".red(),
                            "Permission denied removing system service file.".red()
                        );
                        eprintln!("   Run: sudo kaptaind-cli service uninstall --system");
                        anyhow::bail!("Permission denied");
                    }
                    Err(err) => return Err(err.into()),
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        if user {
            let home = std::env::var("HOME")?;
            let plist_path = format!("{}/Library/LaunchAgents/com.elcigroup.kaptaind.plist", home);
            Command::new("launchctl")
                .args(["unload", &plist_path])
                .output()
                .ok();
            if Path::new(&plist_path).exists() {
                std::fs::remove_file(&plist_path)?;
            }
        } else {
            let plist_path = "/Library/LaunchDaemons/com.elcigroup.kaptaind.plist";
            Command::new("launchctl")
                .args(["unload", plist_path])
                .output()
                .ok();
            if Path::new(plist_path).exists() {
                match std::fs::remove_file(plist_path) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                        eprintln!(
                            "{} {}",
                            "❌".red(),
                            "Permission denied removing system LaunchDaemon.".red()
                        );
                        eprintln!("   Run: sudo kaptaind-cli service uninstall --system");
                        anyhow::bail!("Permission denied");
                    }
                    Err(err) => return Err(err.into()),
                }
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        if system {
            anyhow::bail!("System-wide service uninstall is only supported on Linux and macOS.");
        }
        remove_shell_autostart()?;
    }

    println!("{} {}", "✓".green(), "Service uninstalled.".green());

    Ok(())
}

/// Print whether the user/system service is installed and enabled.
pub fn service_status(user: bool, system: bool) -> anyhow::Result<()> {
    if user && system {
        anyhow::bail!("Specify either --user or --system, not both.");
    }
    if !user && !system {
        anyhow::bail!("Specify --user or --system.");
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let (scope, service_path) = if user {
            let home = std::env::var("HOME")?;
            (
                "user",
                format!("{}/.config/systemd/user/kaptaind.service", home),
            )
        } else {
            ("system", "/etc/systemd/system/kaptaind.service".to_string())
        };

        let installed = Path::new(&service_path).exists();
        let enabled = if installed {
            let flag = if user { "--user" } else { "" };
            let output = Command::new("systemctl")
                .args([flag, "is-enabled", "kaptaind.service"])
                .output()
                .ok();
            output.map(|o| o.status.success()).unwrap_or(false)
        } else {
            false
        };

        println!("{} Service status ({scope})", "⚙️".cyan());
        println!(
            "  Installed: {}",
            if installed { "yes".green() } else { "no".red() }
        );
        println!(
            "  Enabled:   {}",
            if enabled {
                "yes".green()
            } else {
                "no".yellow()
            }
        );
        println!("  Path:      {}", service_path);
    }

    #[cfg(target_os = "macos")]
    {
        let (label, path) = if user {
            let home = std::env::var("HOME")?;
            (
                "com.elcigroup.kaptaind",
                format!("{}/Library/LaunchAgents/com.elcigroup.kaptaind.plist", home),
            )
        } else {
            (
                "com.elcigroup.kaptaind",
                "/Library/LaunchDaemons/com.elcigroup.kaptaind.plist".to_string(),
            )
        };

        let installed = Path::new(&path).exists();
        println!("{} LaunchAgent status", "⚙️".cyan());
        println!(
            "  Installed: {}",
            if installed { "yes".green() } else { "no".red() }
        );
        println!("  Label:     {}", label);
        println!("  Path:      {}", path);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        if system {
            anyhow::bail!("System-wide service status is only supported on Linux and macOS.");
        }
        let home = std::env::var("HOME")?;
        let rc_files = [".bashrc", ".zshrc"];
        let mut installed = false;
        for rc in &rc_files {
            let path = format!("{}/{}", home, rc);
            if Path::new(&path).exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("kaptaind-cli monitor resume") {
                        installed = true;
                        println!("  Found in: {}", path);
                    }
                }
            }
        }
        println!("{} Shell autostart status", "⚙️".cyan());
        println!(
            "  Installed: {}",
            if installed { "yes".green() } else { "no".red() }
        );
    }

    Ok(())
}

fn resolve_system_binary() -> std::path::PathBuf {
    let candidates = ["/usr/local/bin/kaptaind-cli", "/bin/kaptaind-cli"];
    for c in &candidates {
        if Path::new(c).exists() {
            return Path::new(c).to_path_buf();
        }
    }
    eprintln!(
        "{} {}",
        "⚠️".yellow(),
        "kaptaind-cli not found at /usr/local/bin/kaptaind-cli or /bin/kaptaind-cli.".yellow()
    );
    eprintln!(
        "   Symlink your binary first, e.g.: sudo ln -s $(which kaptaind-cli) /usr/local/bin/kaptaind-cli"
    );
    // Return the default path anyway; the service will fail clearly if missing.
    std::path::PathBuf::from("/usr/local/bin/kaptaind-cli")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn install_shell_autostart() -> anyhow::Result<()> {
    let home = std::env::var("HOME")?;
    let autostart_line =
        "# Auto-start kaptaind\nexport PATH=\"$HOME/.local/bin:$PATH\"\nkaptaind-cli monitor resume > /dev/null 2>&1\n";

    for rc_file in [".bashrc", ".zshrc"] {
        let rc_path = format!("{}/{}", home, rc_file);
        if !Path::new(&rc_path).exists() {
            continue;
        }
        let content = std::fs::read_to_string(&rc_path)?;
        if !content.contains("kaptaind-cli monitor resume") {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(&rc_path)?;
            writeln!(file, "\n{}", autostart_line)?;
        }
    }

    println!(
        "{} {}",
        "✓".green(),
        "Shell autostart installed. kaptaind will resume monitored projects in new shells.".green()
    );
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn remove_shell_autostart() -> anyhow::Result<()> {
    let home = std::env::var("HOME")?;
    for rc_file in [".bashrc", ".zshrc"] {
        let rc_path = format!("{}/{}", home, rc_file);
        if !Path::new(&rc_path).exists() {
            continue;
        }
        let content = std::fs::read_to_string(&rc_path)?;
        if content.contains("kaptaind-cli monitor resume") {
            let filtered: String = content
                .lines()
                .filter(|line| {
                    !line.contains("kaptaind-cli monitor resume")
                        && !line.contains("# Auto-start kaptaind")
                })
                .map(|line| format!("{}\n", line))
                .collect();
            std::fs::write(&rc_path, filtered)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaptaind::monitor::{load_registry_at, save_registry_at, MonitorEntry, MonitorRegistry};
    use std::io::Write;
    use std::path::PathBuf;

    static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_home() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let registry = tmp.path().join(".config/kaptaind/monitored.json");
        (tmp, registry)
    }

    fn with_home<F>(f: F)
    where
        F: FnOnce(&tempfile::TempDir, &Path),
    {
        let _guard = HOME_LOCK.lock().unwrap();
        let (tmp, registry) = temp_home();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());
        f(&tmp, &registry);
        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let registry = MonitorRegistry {
            projects: vec![MonitorEntry {
                path: PathBuf::from("/tmp/repo"),
                config: PathBuf::from("/tmp/repo/kaptaind.toml"),
                enabled: true,
                health_port: 3000,
                last_active: Some(chrono::Utc::now()),
            }],
        };

        let json = serde_json::to_string(&registry).unwrap();
        let parsed: MonitorRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(registry, parsed);
    }

    #[test]
    fn test_add_and_list() {
        with_home(|tmp, registry| {
            let project = tmp.path().join("repo");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(project.join("kaptaind.toml"), "[watch]\npath = \".\"\n").unwrap();

            std::env::set_current_dir(&project).unwrap();
            add(Path::new("."), None, None, None).unwrap();

            let loaded = load_registry_at(registry).unwrap();
            assert_eq!(loaded.projects.len(), 1);
            assert_eq!(loaded.projects[0].path, project);
            assert_eq!(loaded.projects[0].health_port, 3000);
            assert!(loaded.projects[0].enabled);
        });
    }

    #[test]
    fn test_port_auto_assignment_avoids_collisions() {
        with_home(|tmp, registry| {
            let mut expected_ports = vec![];
            for i in 0..5 {
                let project = tmp.path().join(format!("repo{}", i));
                std::fs::create_dir_all(&project).unwrap();
                std::fs::write(project.join("kaptaind.toml"), "").unwrap();
                std::env::set_current_dir(&project).unwrap();
                add(Path::new("."), None, None, None).unwrap();
                expected_ports.push(3000 + i);
            }

            let loaded = load_registry_at(registry).unwrap();
            let ports: Vec<u16> = loaded.projects.iter().map(|e| e.health_port).collect();
            assert_eq!(ports, expected_ports);
        });
    }

    #[test]
    fn test_remove_project() {
        with_home(|tmp, registry| {
            let project = tmp.path().join("repo");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(project.join("kaptaind.toml"), "").unwrap();

            std::env::set_current_dir(&project).unwrap();
            add(Path::new("."), None, None, None).unwrap();
            assert_eq!(load_registry_at(registry).unwrap().projects.len(), 1);

            remove(Path::new(".")).unwrap();
            assert!(load_registry_at(registry).unwrap().projects.is_empty());
        });
    }

    #[test]
    fn test_set_enabled() {
        with_home(|tmp, _registry| {
            let project = tmp.path().join("repo");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(project.join("kaptaind.toml"), "").unwrap();

            std::env::set_current_dir(&project).unwrap();
            add(Path::new("."), None, None, Some(true)).unwrap();
            set_enabled(Path::new("."), false).unwrap();

            let loaded = load_registry().unwrap();
            assert_eq!(loaded.projects.len(), 1);
            assert!(!loaded.projects[0].enabled);
        });
    }

    #[test]
    fn test_resume_skips_already_running() {
        with_home(|tmp, registry| {
            let project = tmp.path().join("repo");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(project.join("kaptaind.toml"), "").unwrap();

            let kaptaind_dir = project.join(".kaptaind");
            std::fs::create_dir_all(&kaptaind_dir).unwrap();
            let mut pid_file = std::fs::File::create(kaptaind_dir.join("daemon.pid")).unwrap();
            write!(pid_file, "{}", std::process::id()).unwrap();

            // Seed registry directly so we don't depend on `add`.
            save_registry_at(
                registry,
                &MonitorRegistry {
                    projects: vec![MonitorEntry {
                        path: project.clone(),
                        config: project.join("kaptaind.toml"),
                        enabled: true,
                        health_port: 3000,
                        last_active: None,
                    }],
                },
            )
            .unwrap();

            resume().unwrap();

            let loaded = load_registry_at(registry).unwrap();
            assert!(
                loaded.projects[0].last_active.is_none(),
                "resume should skip a live PID"
            );
        });
    }
}
