use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use std::fs;

pub fn handle_ci_hint(config: &Config, format: &str) -> anyhow::Result<()> {
    let kd = config.repo_path.join(".kaptaind");

    let stability = fs::read_to_string(kd.join("stability.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::stability::model::StabilityRecord>(&s).ok());

    let release_index = fs::read_to_string(kd.join("releases").join("index.json"))
        .ok()
        .and_then(|s| {
            serde_json::from_str::<kaptaind::release::orchestrator::ReleaseIndex>(&s).ok()
        });

    let current_score = stability.as_ref().map(|s| s.score).unwrap_or(0.0);
    let pass_streak = stability
        .as_ref()
        .map(kaptaind::stability::engine::pass_streak)
        .unwrap_or(0);
    let threshold = config.qualification.stability_threshold;
    let min_streak = config.qualification.min_pass_streak;

    let qualified = current_score >= threshold && pass_streak >= min_streak;
    let last_version = release_index
        .as_ref()
        .and_then(|idx| idx.releases.last())
        .map(|e| e.version.clone())
        .unwrap_or_else(|| "none".to_string());
    let current_version = fs::read_to_string(config.repo_path.join("VERSION"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    match format {
        "json" => {
            let out = serde_json::json!({
                "qualified": qualified,
                "stability_score": current_score,
                "pass_streak": pass_streak,
                "threshold": threshold,
                "min_streak": min_streak,
                "current_version": current_version,
                "last_released_version": last_version,
                "recommendation": if qualified { "release" } else { "hold" }
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        "github" => {
            // GitHub Actions workflow command format
            if qualified {
                println!("::notice title=kaptaind::Release qualified — v{current_version} (stability={current_score:.3}, streak={pass_streak})");
                println!("::set-output name=qualified::true");
                println!("::set-output name=version::{current_version}");
            } else {
                println!("::warning title=kaptaind::Hold — stability={current_score:.3} (need {threshold:.3}), streak={pass_streak} (need {min_streak})");
                println!("::set-output name=qualified::false");
                println!("::set-output name=version::{current_version}");
            }
        }
        _ => {
            // Plain text
            let status_str = if qualified {
                "RELEASE".green().bold().to_string()
            } else {
                "HOLD".yellow().bold().to_string()
            };
            println!("{} {}", "CI Hint:".bold(), status_str);
            println!(
                "  Stability score : {:.3}  (threshold: {:.3})",
                current_score, threshold
            );
            println!(
                "  Pass streak     : {}  (required: {})",
                pass_streak, min_streak
            );
            println!("  Current version : {}", current_version.clone().magenta());
            println!("  Last release    : {}", last_version.blue());
            if qualified {
                println!(
                    "  → Recommendation: {}",
                    "ship v".green().to_string() + &current_version
                );
            } else {
                let missing_score = (threshold - current_score).max(0.0);
                let missing_streak = min_streak.saturating_sub(pass_streak);
                if missing_score > 0.001 {
                    println!("  → Need +{:.3} stability score to qualify", missing_score);
                }
                if missing_streak > 0 {
                    println!(
                        "  → Need {} more passing commit(s) in streak",
                        missing_streak
                    );
                }
            }
        }
    }

    Ok(())
}
