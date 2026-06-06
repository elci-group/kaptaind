use chrono::Utc;
use colored::*;
use kaptaind::config::loader::Config;

pub fn handle_analyze(config: &Config) -> anyhow::Result<()> {
    let repo = match kaptaind::git::repo::Repo::open(&config.repo_path) {
        Ok(repo) => repo,
        Err(err) => {
            anyhow::bail!(
                "Could not open Git repository at {}: {}",
                config.repo_path.display(),
                err
            );
        }
    };
    let paths = repo.changed_paths()?;

    if paths.is_empty() {
        println!("Working tree is clean. No analysis generated.");
        return Ok(());
    }

    let timestamp = Utc::now();
    let cluster = kaptaind::cluster::engine::Cluster {
        id: uuid::Uuid::new_v4(),
        started_at: timestamp,
        ended_at: timestamp,
        events: vec![kaptaind::watcher::FsEvent {
            paths,
            kind: kaptaind::watcher::FsEventKind::Modify,
            timestamp,
        }],
    };

    let mut diff_analysis = kaptaind::diff::analyze(&cluster, &config.repo_path);
    if config.bundle.command.is_some() {
        diff_analysis.bundle =
            kaptaind::diff::bundle::bundle_score(&config.bundle, &config.repo_path).score;
    }
    let weight = kaptaind::weight::compute(&diff_analysis, &config.weights);
    let bump = kaptaind::version::decide(&weight, &config.version_thresholds);

    println!("{}", "🧪 Dry-run Analysis Result:".bold().magenta());
    println!("{}", "-----------------------------------".magenta());
    println!(
        "{} {}",
        "🗂️ Touched Paths:".cyan(),
        diff_analysis.touched_paths
    );
    println!(
        "{} {}",
        "💥 API Break:    ".cyan(),
        if diff_analysis.api_breaking {
            "Yes".red().bold()
        } else {
            "No".green()
        }
    );
    println!(
        "{} {}",
        "➕ API Added:    ".cyan(),
        if diff_analysis.api_added {
            "Yes".green()
        } else {
            "No".yellow()
        }
    );
    println!(
        "{} {}",
        "🔌 API Score:    ".cyan(),
        format!("{:.3}", diff_analysis.api).yellow()
    );
    println!(
        "{} {}",
        "📦 Deps Score:   ".cyan(),
        format!("{:.3}", diff_analysis.deps).yellow()
    );
    println!(
        "{} {}",
        "⚙️ Runtime Score:".cyan(),
        format!("{:.3}", diff_analysis.runtime).yellow()
    );
    println!("{}", "-----------------------------------".magenta());
    println!(
        "{} {}",
        "🎯 Total Score:  ".bold().cyan(),
        format!("{:.3}", weight.score).bold().yellow()
    );

    let current_version = read_current_version(config);
    let next_version = kaptaind::version::apply(current_version, bump);

    let bump_str = match bump {
        kaptaind::version::Bump::Major => "🚀 Major".red().bold(),
        kaptaind::version::Bump::Minor => "✨ Minor".cyan().bold(),
        kaptaind::version::Bump::Patch => "🩹 Patch".green().bold(),
        kaptaind::version::Bump::None => "📌 Stable".blue(),
    };

    if bump == kaptaind::version::Bump::None {
        println!("{} {}", "📈 Projected Bump:".bold().cyan(), bump_str);
    } else {
        let bump_display = format!("{} -> v{}", bump_str, next_version);
        println!("{} {}", "📈 Projected Bump:".bold().cyan(), bump_display);
    }

    Ok(())
}

fn read_current_version(config: &Config) -> semver::Version {
    let version_path = config.repo_path.join("VERSION");
    let current_version_str = if version_path.exists() {
        std::fs::read_to_string(&version_path)
            .unwrap_or_else(|_| "0.1.0".to_string())
            .trim()
            .to_string()
    } else {
        "0.1.0".to_string()
    };
    semver::Version::parse(&current_version_str).unwrap_or_else(|_| semver::Version::new(0, 1, 0))
}
