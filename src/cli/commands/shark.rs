use anyhow::Context;
use kaptaind::config::loader::Config;
use kaptaind::daemon::shark::{Arbiter, FileArbiter};
use kaptaind::util::style::*;
use std::time::Duration;
use tokio::time::{sleep, timeout};

use crate::SharkCommand;

// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn handle_shark(config: &Config, cmd: &SharkCommand) -> anyhow::Result<()> {
    let arbiter_path = config.shark_arbiter_path();
    let arbiter = FileArbiter::new(&arbiter_path)?;
    let instance_id = config.shark_instance_id();

    match cmd {
        SharkCommand::Status { json } => {
            let lease = arbiter.current_lease()?;
            if *json {
                let output = serde_json::json!({
                    "instance_id": instance_id,
                    "role": if lease.as_ref().map(|l| l.instance_id == instance_id).unwrap_or(false) {
                        "leader"
                    } else {
                        "standby"
                    },
                    "leader_id": lease.as_ref().map(|l| l.instance_id.clone()),
                    "lease_acquired_at": lease.as_ref().map(|l| l.acquired_at.to_rfc3339()),
                    "lease_renewed_at": lease.as_ref().map(|l| l.renewed_at.to_rfc3339()),
                    "lease_ttl_ms": lease.as_ref().map(|l| l.ttl_ms),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("{} {}", "🦈".cyan(), "Shark Stating".bold().cyan());
                println!("{} {}", "Instance:".bold(), instance_id.clone().yellow());
                let role = if lease
                    .as_ref()
                    .map(|l| l.instance_id == instance_id)
                    .unwrap_or(false)
                {
                    "leader".green()
                } else {
                    "standby".blue()
                };
                println!("{} {}", "Role:".bold(), role);
                if let Some(lease) = lease {
                    println!("{} {}", "Leader:".bold(), lease.instance_id.magenta());
                    println!(
                        "{} {}",
                        "Renewed:".bold(),
                        lease.renewed_at.to_rfc3339().dimmed()
                    );
                    println!("{} {}ms", "TTL:".bold(), lease.ttl_ms.to_string().dimmed());
                } else {
                    println!("{}", "No active lease".dimmed());
                }
            }
        }
        SharkCommand::Observe { interval_ms } => {
            println!(
                "{} {}",
                "🦈".cyan(),
                "Observing Shark Stating (Ctrl-C to stop)".bold().cyan()
            );
            let interval = Duration::from_millis(*interval_ms);
            let mut last_leader: Option<String> = None;
            loop {
                let lease = arbiter.current_lease()?;
                let leader_id = lease.as_ref().map(|l| l.instance_id.clone());
                let role = if leader_id.as_ref() == Some(&instance_id) {
                    "leader".green()
                } else if leader_id.is_some() {
                    "standby".blue()
                } else {
                    "no leader".dimmed()
                };
                if leader_id != last_leader {
                    println!(
                        "{} role={} leader={} renewed={}",
                        chrono::Utc::now().to_rfc3339(),
                        role,
                        leader_id.as_deref().unwrap_or("none").magenta(),
                        lease
                            .as_ref()
                            .map(|l| l.renewed_at.to_rfc3339())
                            .unwrap_or_default()
                            .dimmed()
                    );
                    last_leader = leader_id;
                }
                sleep(interval).await;
            }
        }
        SharkCommand::Release => {
            arbiter.release(&instance_id)?;
            println!(
                "{} {}",
                "🦈".cyan(),
                "Leadership released (if held by this instance)".green()
            );
        }
        SharkCommand::Upgrade {
            binary,
            standby_health_port,
            ready_timeout_ms,
        } => {
            println!(
                "{} {} {}",
                "🦈".cyan(),
                "Shark upgrade:".bold().cyan(),
                binary.display().to_string().yellow()
            );

            let current_lease = arbiter.current_lease()?;
            let leader_id = current_lease
                .as_ref()
                .map(|l| l.instance_id.clone())
                .unwrap_or_else(|| instance_id.clone());

            if current_lease
                .as_ref()
                .map(|l| l.instance_id != instance_id)
                .unwrap_or(false)
            {
                println!(
                    "{} current leader is {}; this instance is standby. Upgrade must be run from the leader.",
                    "ℹ️".blue(),
                    leader_id.blue()
                );
                return Ok(());
            }

            // Pick a health port for the standby. If the user did not supply one,
            // choose an ephemeral port by binding to 127.0.0.1:0 and reading it back.
            let standby_port = match *standby_health_port {
                Some(port) => port,
                None => {
                    let listener = std::net::TcpListener::bind("127.0.0.1:0")
                        .context("failed to bind ephemeral health port")?;
                    listener.local_addr()?.port()
                }
            };

            // Spawn standby instance.
            let mut child = kaptaind::daemon::shark::spawn_standby(
                &config.repo_path,
                binary,
                &arbiter_path,
                Some(standby_port),
            )
            .await?;
            println!(
                "{} standby spawned with pid {} (health port {})",
                "✅".green(),
                child.id(),
                standby_port
            );

            // Wait for the standby to report healthy before asking the leader to retire.
            let ready_timeout = Duration::from_millis(*ready_timeout_ms);
            // traci: allow -- this branch emits a structured readiness error before cleanup.
            if let Err(err) =
                kaptaind::daemon::shark::wait_for_standby_ready(standby_port, ready_timeout).await
            {
                tracing::error!(
                    ?err,
                    operation = "handle_shark",
                    source_line = line!(),
                    "handle shark returned an error"
                );
                if let Err(error) = child.kill() {
                    tracing::warn!(
                        ?error,
                        operation = "handle_shark",
                        source_line = line!(),
                        "best-effort operation failed"
                    );
                }
                anyhow::bail!("standby failed to become ready: {}", err);
            }
            println!("{} standby is healthy", "✅".green());

            // Request the current leader (us) to retire.
            kaptaind::daemon::shark::request_retire(
                &arbiter_path,
                &instance_id,
                Some(standby_port),
            )?;
            println!(
                "{} retire marker written for {} (standby health port {})",
                "✅".green(),
                instance_id.clone().yellow(),
                standby_port.to_string().dimmed()
            );

            // Wait for the standby to acquire leadership.
            let handoff_timeout = Duration::from_millis(config.shark.upgrade_handoff_timeout_ms);
            let acquired = timeout(handoff_timeout, async {
                loop {
                    match arbiter.current_lease() {
                        Ok(Some(lease)) if lease.instance_id != instance_id => {
                            return Ok::<_, anyhow::Error>(lease)
                        }
                        _ => {}
                    }
                    sleep(Duration::from_millis(250)).await;
                }
            })
            .await;

            match acquired {
                Ok(Ok(lease)) => {
                    println!(
                        "{} upgrade complete; new leader is {}",
                        "🚀".green(),
                        lease.instance_id.clone().green()
                    );
                    kaptaind::audit::log_event(
                        &config.repo_path,
                        &instance_id,
                        "shark.upgrade",
                        true,
                        serde_json::json!({
                            "new_leader": lease.instance_id,
                            "standby_health_port": standby_port,
                            "binary": binary.display().to_string(),
                        }),
                    );
                }
                _ => {
                    // Attempt to clean up the child and cancel retirement.
                    if let Err(error) = child.kill() {
                        tracing::warn!(
                            ?error,
                            operation = "handle_shark",
                            source_line = line!(),
                            "best-effort operation failed"
                        );
                    }
                    kaptaind::daemon::shark::cancel_upgrade(&arbiter_path, &instance_id);
                    eprintln!(
                        "{} upgrade handoff timed out; old leader retains control",
                        "❌".red()
                    );
                    kaptaind::audit::log_event(
                        &config.repo_path,
                        &instance_id,
                        "shark.upgrade",
                        false,
                        serde_json::json!({
                            "standby_health_port": standby_port,
                            "binary": binary.display().to_string(),
                            "error": "upgrade handoff timed out",
                        }),
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
