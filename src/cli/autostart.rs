use colored::*;

pub fn handle_enable_autostart() -> anyhow::Result<()> {
    let home = std::env::var("HOME")?;
    let kaptaind_path = format!("{}/.local/bin/kaptaind", home);

    if !std::path::Path::new(&kaptaind_path).exists() {
        anyhow::bail!(
            "kaptaind not found at {}. Run install.sh first.",
            kaptaind_path
        );
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let systemd_dir = format!("{}/.config/systemd/user", home);
        std::fs::create_dir_all(&systemd_dir)?;

        let service_content = format!(
            r#"[Unit]
Description=Kaptaind - Automated Semantic Versioning Daemon
Documentation=https://github.com/elci-group/kaptaind
After=network.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart={}-cli autostart
StandardOutput=journal
StandardError=journal
SyslogIdentifier=kaptaind
Environment="RUST_LOG=info"

[Install]
WantedBy=default.target
"#,
            kaptaind_path
        );

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
            "Auto-start enabled via systemd user service".green()
        );
        println!("  Service file: {}/kaptaind.service", systemd_dir);
        println!("  Auto-start on next login with: systemctl --user start kaptaind");
    }

    #[cfg(target_os = "macos")]
    {
        let launchd_dir = format!("{}/.Library/LaunchAgents", home);
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
    <string>{}-cli</string>
    <string>autostart</string>
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
            kaptaind_path, home, home
        );

        let plist_path = format!("{}/com.elcigroup.kaptaind.plist", launchd_dir);
        std::fs::write(&plist_path, plist_content)?;

        println!(
            "{} {}",
            "✓".green(),
            "Auto-start enabled via launchd plist".green()
        );
        println!("  Plist file: {}", plist_path);
        println!("  Auto-start on next login");
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        setup_shell_autostart(&home, &kaptaind_path)?;
    }

    Ok(())
}

pub fn handle_disable_autostart() -> anyhow::Result<()> {
    let home = std::env::var("HOME")?;

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        Command::new("systemctl")
            .args(["--user", "disable", "kaptaind.service"])
            .output()?;

        let service_path = format!("{}/.config/systemd/user/kaptaind.service", home);
        if std::path::Path::new(&service_path).exists() {
            std::fs::remove_file(&service_path)?;
        }

        println!(
            "{} {}",
            "✓".green(),
            "Auto-start disabled (systemd service removed)".green()
        );
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let plist_path = format!(
            "{}/.Library/LaunchAgents/com.elcigroup.kaptaind.plist",
            home
        );

        Command::new("launchctl")
            .args(["unload", &plist_path])
            .output()
            .ok();

        if std::path::Path::new(&plist_path).exists() {
            std::fs::remove_file(&plist_path)?;
        }

        println!(
            "{} {}",
            "✓".green(),
            "Auto-start disabled (launchd plist removed)".green()
        );
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        remove_shell_autostart(&home)?;
    }

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn setup_shell_autostart(home: &str, kaptaind_path: &str) -> anyhow::Result<()> {
    let autostart_line = format!(
        "# Auto-start kaptaind\nexport PATH=\"$HOME/.local/bin:$PATH\"\n{}-cli autostart > /dev/null 2>&1\n",
        kaptaind_path
    );

    for rc_file in [".bashrc", ".zshrc"] {
        let rc_path = format!("{}/{}", home, rc_file);
        if !std::path::Path::new(&rc_path).exists() {
            continue;
        }

        let content = std::fs::read_to_string(&rc_path)?;
        if !content.contains("Auto-start kaptaind") {
            use std::io::Write;

            let mut file = std::fs::OpenOptions::new().append(true).open(&rc_path)?;
            writeln!(file, "\n{}", autostart_line)?;
        }
    }

    println!(
        "{} {}",
        "✓".green(),
        "Auto-start enabled via shell initialization".green()
    );
    println!("  Added to ~/.bashrc and ~/.zshrc");
    println!("  Auto-start on next shell login");

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn remove_shell_autostart(home: &str) -> anyhow::Result<()> {
    for rc_file in [".bashrc", ".zshrc"] {
        let rc_path = format!("{}/{}", home, rc_file);
        if !std::path::Path::new(&rc_path).exists() {
            continue;
        }

        let content = std::fs::read_to_string(&rc_path)?;
        if content.contains("Auto-start kaptaind") {
            let filtered: String = content
                .lines()
                .filter(|line| {
                    !line.contains("Auto-start kaptaind")
                        && !line.contains("nohup")
                        && !line.contains("kaptaind.*daemon")
                })
                .map(|line| format!("{}\n", line))
                .collect();
            std::fs::write(&rc_path, filtered)?;
        }
    }

    println!(
        "{} {}",
        "✓".green(),
        "Auto-start disabled (shell initialization removed)".green()
    );

    Ok(())
}
