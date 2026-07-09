use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use std::fs;

pub fn handle_dashboard(config: &Config) -> anyhow::Result<()> {
    let kd = config.repo_path.join(".kaptaind");

    // --- Version ---
    let version = fs::read_to_string(config.repo_path.join("VERSION"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // --- Daemon status ---
    let daemon_state = fs::read_to_string(kd.join("status.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::daemon::scheduler::StatusReport>(&s).ok());

    // --- Telemetry ---
    let telemetry = fs::read_to_string(kd.join("telemetry.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::daemon::telemetry::TokenMetrics>(&s).ok());

    // --- Stability ---
    let stability = fs::read_to_string(kd.join("stability.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::stability::model::StabilityRecord>(&s).ok());

    // --- Releases index ---
    let release_index = fs::read_to_string(kd.join("releases").join("index.json"))
        .ok()
        .and_then(|s| {
            serde_json::from_str::<kaptaind::release::orchestrator::ReleaseIndex>(&s).ok()
        });

    // --- Recent analyses ---
    let analysis_dir = kd.join("analysis");
    let mut recent_analyses: Vec<kaptaind::daemon::scheduler::AnalysisArtifact> = Vec::new();
    if analysis_dir.exists() {
        let mut entries: Vec<_> = fs::read_dir(&analysis_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for entry in entries.iter().take(5) {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(a) =
                    serde_json::from_str::<kaptaind::daemon::scheduler::AnalysisArtifact>(&content)
                {
                    recent_analyses.push(a);
                }
            }
        }
    }

    // ======= Render =======
    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════╗".cyan()
    );
    println!(
        "{}",
        "║          kaptaind  ·  Live Dashboard         ║"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════╝".cyan()
    );
    println!();

    // Version + daemon
    println!(
        "{}",
        "── Project ─────────────────────────────────────".bright_black()
    );
    println!("  {}  {}", "Version:".bold(), version.magenta().bold());
    println!(
        "  {}  {}",
        "Repo:   ".bold(),
        config.repo_path.display().to_string().blue()
    );
    if let Some(ref st) = daemon_state {
        let state_str = match st.status {
            kaptaind::daemon::scheduler::State::Idle => "Idle".green().to_string(),
            kaptaind::daemon::scheduler::State::Clustering => "Clustering".cyan().to_string(),
            kaptaind::daemon::scheduler::State::Testing => "Testing".yellow().to_string(),
            kaptaind::daemon::scheduler::State::Committing => "Committing".magenta().to_string(),
            kaptaind::daemon::scheduler::State::Failed => "Failed".red().bold().to_string(),
            kaptaind::daemon::scheduler::State::Stopping => "Stopping".yellow().to_string(),
            kaptaind::daemon::scheduler::State::Stopped => "Stopped".bright_black().to_string(),
        };
        println!("  {}  {}", "Daemon: ".bold(), state_str);
        if let Some(ref err) = st.last_error {
            println!("  {}  {}", "Error:  ".bold(), err.red());
        }
    } else {
        println!(
            "  {}  {}",
            "Daemon: ".bold(),
            "Not running / no status file".bright_black()
        );
    }
    println!();

    // Stability
    println!(
        "{}",
        "── Stability ───────────────────────────────────".bright_black()
    );
    if let Some(ref s) = stability {
        let bar = stability_bar(s.score);
        let score_colored = if s.score >= 0.85 {
            format!("{:.3}", s.score).green().bold().to_string()
        } else if s.score >= 0.6 {
            format!("{:.3}", s.score).yellow().to_string()
        } else {
            format!("{:.3}", s.score).red().to_string()
        };
        println!(
            "  Score:  {} {}  {}",
            bar,
            score_colored,
            format!("({} commits tracked)", s.history.len()).bright_black()
        );
        if let Some(reg_ts) = s.last_regression {
            let reg_dt = chrono::DateTime::<chrono::Utc>::from_timestamp(reg_ts, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("  Last regression: {}", reg_dt.yellow());
        }
    } else {
        println!("  {}", "No stability data yet.".bright_black());
    }
    println!();

    // Telemetry
    println!(
        "{}",
        "── Telemetry ───────────────────────────────────".bright_black()
    );
    if let Some(ref t) = telemetry {
        println!(
            "  {}  ${:.4}  (${:.6} this session)",
            "LLM cost:".bold(),
            t.aggregate_cost,
            t.marginal_cost
        );
        println!(
            "  {}  {}  failed: {}",
            "Releases:".bold(),
            t.releases.to_string().green(),
            t.failed_releases.to_string().red()
        );
    } else {
        println!("  {}", "No telemetry data.".bright_black());
    }
    println!();

    // Recent releases
    println!(
        "{}",
        "── Releases ────────────────────────────────────".bright_black()
    );
    if let Some(ref idx) = release_index {
        if idx.releases.is_empty() {
            println!("  {}", "No releases yet.".bright_black());
        } else {
            for entry in idx.releases.iter().rev().take(5) {
                let ts = chrono::DateTime::<chrono::Utc>::from_timestamp(entry.released_at, 0)
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!(
                    "  {} {}  {}  S={:.3}",
                    "▸".green(),
                    format!("v{}", entry.version).magenta().bold(),
                    ts.bright_black(),
                    entry.stability
                );
            }
        }
    } else {
        println!("  {}", "No release index found.".bright_black());
    }
    println!();

    // Recent analyses
    println!(
        "{}",
        "── Recent Analyses ─────────────────────────────".bright_black()
    );
    if recent_analyses.is_empty() {
        println!("  {}", "No analyses yet.".bright_black());
    } else {
        for a in &recent_analyses {
            let bump_sym = match a.bump.as_str() {
                "Major" => "🚀".to_string(),
                "Minor" => "✨".to_string(),
                "Patch" => "🩹".to_string(),
                _ => "─".to_string(),
            };
            println!(
                "  {} {}  score={:.3}  bump={}{}  paths={}",
                bump_sym,
                a.version.clone().magenta(),
                a.weight.score,
                a.bump.clone().cyan(),
                if a.weight.api_breaking {
                    " [BREAKING]".red().to_string()
                } else {
                    String::new()
                },
                a.diff.touched_paths
            );
        }
    }
    println!();

    Ok(())
}

fn stability_bar(score: f64) -> String {
    let filled = (score * 20.0).round() as usize;
    let empty = 20usize.saturating_sub(filled);
    let bar: String = std::iter::repeat_n('█', filled)
        .chain(std::iter::repeat_n('░', empty))
        .collect();
    format!("[{}]", bar)
}
