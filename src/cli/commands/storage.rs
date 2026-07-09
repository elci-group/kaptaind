use kaptaind::config::loader::Config;
use kaptaind::util::style::*;

use crate::StorageCommand;

pub fn handle_storage(config: &Config, cmd: &StorageCommand) -> anyhow::Result<()> {
    let dh_cfg = deckhand_config_from_kaptaind(config);

    match cmd {
        StorageCommand::Clean {
            profile,
            dry_run,
            older_than,
        } => {
            println!(
                "{} {} {}",
                "🧹".cyan(),
                "Storage clean:".bold().cyan(),
                profile.yellow()
            );
            deckhand::clean::run(&dh_cfg, profile, *dry_run, *older_than, None)?;
        }
        StorageCommand::Sweep { keep_days, dry_run } => {
            println!(
                "{} {} (keep {} days)",
                "🧹".cyan(),
                "Storage sweep".bold().cyan(),
                keep_days.to_string().yellow()
            );
            deckhand::sweep::run(&dh_cfg, &config.repo_path, *dry_run, *keep_days)?;
        }
        StorageCommand::Status { json, limit } => {
            deckhand::status::run(&dh_cfg, *json, *limit)?;
        }
    }

    Ok(())
}

fn deckhand_config_from_kaptaind(config: &Config) -> deckhand::config::Config {
    use deckhand::config::{
        AutoCleanConfig, CleanConfig, StatusConfig, SweepConfig, WorkspaceConfig,
    };

    deckhand::config::Config {
        workspace: WorkspaceConfig {
            path: config.repo_path.clone(),
            members: deckhand::config::MemberSpec::Auto,
        },
        clean: CleanConfig {
            profiles: config.deckhand.clean_profiles.clone(),
            keep_incremental: false,
            keep_days: config.deckhand.clean_older_than_days.unwrap_or(0),
            languages: vec!["cargo".to_string()],
            allow_native_commands: false,
            remove_node_modules: false,
            remove_venvs: false,
        },
        sweep: SweepConfig {
            registry_cache: true,
            git_checkouts: true,
            keep_registry_days: config.deckhand.sweep_keep_days,
            node_modules: false,
            python_bytecode: false,
            go_build_cache: false,
            swift_derived_data: false,
        },
        status: StatusConfig {
            warn_free_percent: config.deckhand.min_free_percent,
        },
        auto_clean: AutoCleanConfig::default(),
    }
}
