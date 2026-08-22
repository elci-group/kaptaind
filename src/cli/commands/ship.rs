use kaptaind::config::loader::Config;

use crate::ShipCommand;

fn parse_ship_format(format: &str) -> kaptaind::release::ship::OutputFormat {
    if format.eq_ignore_ascii_case("json") {
        kaptaind::release::ship::OutputFormat::Json
    } else {
        kaptaind::release::ship::OutputFormat::Text
    }
}

fn authorize_release_actor(
    config: &Config,
    actor: &kaptaind::rbac::AuthenticatedActor,
    permission: &str,
) -> anyhow::Result<()> {
    if config.governance.enforce_enterprise_controls {
        kaptaind::rbac::check_permission_for_subject(&config.rbac, permission, &actor.subject)?;
    }
    Ok(())
}

// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn handle_ship(config: &Config, cmd: &ShipCommand) -> anyhow::Result<()> {
    match cmd {
        ShipCommand::RequestApproval { ticket } => {
            let policy_id = config
                .policy_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("approval requests require config.policy_id"))?;
            let policy = kaptaind::daemon::policy::Policy::load_with_trust(
                &config.repo_path,
                policy_id,
                &config.policy_trust,
                config.policy_keyring_path().as_deref(),
            )?;
            if config.governance.enforce_enterprise_controls {
                policy.validate_enterprise_release_controls()?;
            }
            if policy.required_release_approvals == 0 {
                anyhow::bail!("policy does not require release approvals");
            }
            let version = std::fs::read_to_string(config.repo_path.join("VERSION"))?
                .trim()
                .to_string();
            let actor = kaptaind::rbac::AuthenticatedActor::from_identity_config(&config.identity)?;
            authorize_release_actor(config, &actor, "ship.run")?;
            let approval = kaptaind::daemon::policy::request_release_approval(
                &config.repo_path,
                policy_id,
                &version,
                &actor.subject,
                kaptaind::daemon::policy::ApprovalRequestOptions {
                    change_ticket: ticket.clone(),
                    require_hmac: policy.require_approval_hmac,
                    require_commit_binding: policy.require_approval_commit_binding,
                    approval_validity_hours: policy.approval_validity_hours,
                },
            )?;
            kaptaind::audit::log_event(
                &config.repo_path,
                &actor.subject,
                "release_approval_requested",
                true,
                serde_json::json!({
                    "version": version,
                    "policy_id": policy_id,
                    "change_ticket": approval.change_ticket,
                    "identity_source": actor.source.as_str(),
                }),
            );
            println!("approval requested for v{version}");
            return Ok(());
        }
        ShipCommand::Approve { version } => {
            let policy_id = config
                .policy_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("release approval requires config.policy_id"))?;
            let policy = kaptaind::daemon::policy::Policy::load_with_trust(
                &config.repo_path,
                policy_id,
                &config.policy_trust,
                config.policy_keyring_path().as_deref(),
            )?;
            if config.governance.enforce_enterprise_controls {
                policy.validate_enterprise_release_controls()?;
            }
            let version = match version {
                Some(version) => version.clone(),
                None => std::fs::read_to_string(config.repo_path.join("VERSION"))?
                    .trim()
                    .to_string(),
            };
            let actor = kaptaind::rbac::AuthenticatedActor::from_identity_config(&config.identity)?;
            authorize_release_actor(config, &actor, "ship.approve")?;
            let approval = kaptaind::daemon::policy::approve_release(
                &config.repo_path,
                &version,
                &actor.subject,
                policy.require_requester_approver_separation,
                policy.require_approval_hmac,
                policy.require_approval_commit_binding,
            )?;
            kaptaind::audit::log_event(
                &config.repo_path,
                &actor.subject,
                "release_approval_granted",
                true,
                serde_json::json!({
                    "version": version,
                    "policy_id": policy_id,
                    "approver_count": approval.approvers.len(),
                    "identity_source": actor.source.as_str(),
                }),
            );
            println!("approval recorded for v{version}");
            return Ok(());
        }
        _ => {}
    }
    let empty_targets = Vec::new();
    let empty_channels = Vec::new();
    let (targets, channels, format) = match cmd {
        ShipCommand::Plan {
            targets,
            channels,
            format,
            ..
        }
        | ShipCommand::Run {
            targets,
            channels,
            format,
            ..
        }
        | ShipCommand::Stable {
            targets,
            channels,
            format,
            ..
        }
        | ShipCommand::Nightly {
            targets,
            channels,
            format,
            ..
        } => (targets, channels, parse_ship_format(format)),
        ShipCommand::Status { format, .. } => {
            (&empty_targets, &empty_channels, parse_ship_format(format))
        }
        ShipCommand::RequestApproval { .. } | ShipCommand::Approve { .. } => (
            &empty_targets,
            &empty_channels,
            kaptaind::release::ship::OutputFormat::Text,
        ),
    };
    let targets = if targets.is_empty() {
        None
    } else {
        Some(targets.clone())
    };
    let channels = if channels.is_empty() {
        None
    } else {
        Some(channels.clone())
    };

    match cmd {
        ShipCommand::Plan { .. } => {
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: true,
                targets,
                channels,
                force: false,
                kind: kaptaind::release::ship::ShipKind::Manual,
                version_override: None,
                require_qualification: config.ship.require_qualification,
                format,
            };
            kaptaind::release::ship::run_ship(config, opts).await?;
        }
        ShipCommand::Run { force, .. } => {
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: false,
                targets,
                channels,
                force: *force,
                kind: kaptaind::release::ship::ShipKind::Manual,
                version_override: None,
                require_qualification: config.ship.require_qualification,
                format,
            };
            kaptaind::release::ship::run_ship(config, opts).await?;
        }
        ShipCommand::Stable { dry_run, force, .. } => {
            let require_qualification = config
                .ship
                .stable
                .require_qualification
                .unwrap_or(config.ship.require_qualification);
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: *dry_run,
                targets,
                channels,
                force: *force,
                kind: kaptaind::release::ship::ShipKind::Stable,
                version_override: None,
                require_qualification: if *force { false } else { require_qualification },
                format,
            };
            kaptaind::release::ship::run_stable(config, opts).await?;
        }
        ShipCommand::Nightly {
            dry_run, no_force, ..
        } => {
            let require_qualification = config.ship.nightly.require_qualification.unwrap_or(false);
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: *dry_run,
                targets,
                channels,
                force: false,
                kind: kaptaind::release::ship::ShipKind::Nightly,
                version_override: None,
                require_qualification: if *no_force {
                    true
                } else {
                    require_qualification
                },
                format,
            };
            kaptaind::release::ship::run_nightly(config, opts).await?;
        }
        ShipCommand::Status { auto, .. } => {
            if *auto {
                kaptaind::release::ship::print_auto_ship_status(config, format)?;
            }
            kaptaind::release::ship::print_ship_status(&config.repo_path, format)?;
        }
        ShipCommand::RequestApproval { .. } | ShipCommand::Approve { .. } => {
            unreachable!("handled above")
        }
    }

    Ok(())
}
