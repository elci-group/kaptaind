use kaptaind::config::loader::Config;

use crate::VacsCommand;

pub fn handle_vacs(config: &Config, cmd: &VacsCommand) -> anyhow::Result<()> {
    match cmd {
        VacsCommand::Show { commit } => {
            let manager = kaptaind::vacs::asset::AssetManager::new(&config.repo_path);
            let assets = manager.get_all()?;

            let filtered: Vec<_> = if let Some(c) = commit {
                assets
                    .into_iter()
                    .filter(|a| a.source_commit == *c || a.concept_id == *c)
                    .collect()
            } else {
                assets
            };

            if filtered.is_empty() {
                println!("No VACS assets found.");
            } else {
                for a in filtered {
                    println!(
                        "Asset ID: {}\nType: {}\nCommit: {}\nConcept: {}\n",
                        a.asset_id, a.asset_type, a.source_commit, a.concept_id
                    );
                }
            }
        }
        VacsCommand::Generate { asset_type } => {
            println!(
                "Manually triggering generation for type: {} is not yet supported in MVP.",
                asset_type
            );
        }
    }
    Ok(())
}
