use kaptaind::config::loader::Config;

use crate::ShipCommand;

fn parse_ship_format(format: &str) -> kaptaind::release::ship::OutputFormat {
    if format.eq_ignore_ascii_case("json") {
        kaptaind::release::ship::OutputFormat::Json
    } else {
        kaptaind::release::ship::OutputFormat::Text
    }
}

pub async fn handle_ship(config: &Config, cmd: &ShipCommand) -> anyhow::Result<()> {
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
    }

    Ok(())
}
