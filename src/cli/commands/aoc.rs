use chrono::Utc;
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use std::fs;

use crate::format_datetime;
use crate::table::print_table;
use crate::AocCommand;

pub fn handle_aoc(config: &Config, cmd: &AocCommand) -> anyhow::Result<()> {
    match cmd {
        AocCommand::Start { label } => {
            handle_aoc_start(config, label)?;
        }
        AocCommand::Ship => {
            handle_aoc_ship(config)?;
        }
        AocCommand::Status => {
            handle_aoc_status(config)?;
        }
        AocCommand::Intercept {
            model,
            intent,
            command,
            args,
        } => {
            handle_aoc_intercept(config, model.clone(), intent.clone(), command, args)?;
        }
        AocCommand::Log { limit } => {
            handle_aoc_log(config, *limit)?;
        }
    }
    Ok(())
}

fn handle_aoc_start(config: &Config, label: &str) -> anyhow::Result<()> {
    // Check if an active session already exists
    if let Ok(Some(_)) = kaptaind::aoc::session::load_active(&config.repo_path) {
        anyhow::bail!("An AoC session is already active. Run 'aoc ship' to end it.");
    }

    // Read current version
    let version_path = config.repo_path.join("VERSION");
    let initial_version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "0.1.0".to_string()
    };

    // Create new session
    let session = kaptaind::aoc::AocSession {
        id: uuid::Uuid::new_v4().to_string(),
        label: label.to_string(),
        created_at: Utc::now(),
        initial_version: initial_version.clone(),
        intent: None,
        target_stability: None,
    };

    // Save session
    kaptaind::aoc::session::save_active(&config.repo_path, &session)?;

    println!(
        "{} {} {} {}",
        "🎯".cyan(),
        "AoC started:".bold().cyan(),
        label.magenta(),
        format!("@ v{}", initial_version).blue()
    );

    Ok(())
}

fn handle_aoc_ship(config: &Config) -> anyhow::Result<()> {
    // Load active session
    let session = kaptaind::aoc::session::load_active(&config.repo_path)?
        .ok_or_else(|| anyhow::anyhow!("No active AoC session found"))?;

    // Read final version
    let version_path = config.repo_path.join("VERSION");
    let final_version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "0.1.0".to_string()
    };

    // Read traces
    let traces = kaptaind::aoc::tracer::read_traces_for_aoc(&config.repo_path, &session.id)?;

    // Count commits and test failures
    let commit_count = traces
        .iter()
        .filter(|t| matches!(t.result, kaptaind::aoc::TraceResult::Committed { .. }))
        .count();
    let test_failures = traces.iter().filter(|t| t.test.outcome == "failed").count();

    // Create manifest
    let manifest = kaptaind::aoc::AocManifest {
        id: session.id.clone(),
        label: session.label.clone(),
        created_at: session.created_at,
        shipped_at: Utc::now(),
        initial_version: session.initial_version.clone(),
        final_version: final_version.clone(),
        cluster_count: traces.len(),
        commit_count,
        test_failures,
        trace_ids: traces.iter().map(|t| t.cluster_id.clone()).collect(),
    };

    // Save manifest
    kaptaind::aoc::session::save_manifest(&config.repo_path, &manifest)?;

    // Remove active session
    kaptaind::aoc::session::remove_active(&config.repo_path)?;

    // Print summary
    println!(
        "{} {} {} {}",
        "🚢".green(),
        "AoC shipped:".bold().green(),
        session.label.magenta(),
        "✓".green()
    );
    println!("{}", "---".green());
    println!(
        "{} {} {}",
        "Version:".cyan(),
        format!("{} → {}", session.initial_version, final_version).magenta(),
        if session.initial_version != final_version {
            "✨"
        } else {
            ""
        }
        .yellow()
    );
    println!(
        "{} {}",
        "Clusters:".cyan(),
        traces.len().to_string().yellow()
    );
    println!(
        "{} {}",
        "Commits:".cyan(),
        commit_count.to_string().yellow()
    );
    println!(
        "{} {}",
        "Test Failures:".cyan(),
        format!("{}", test_failures).yellow()
    );

    Ok(())
}

fn handle_aoc_status(config: &Config) -> anyhow::Result<()> {
    match kaptaind::aoc::session::load_active(&config.repo_path)? {
        Some(session) => {
            // Count traces
            let traces =
                kaptaind::aoc::tracer::read_traces_for_aoc(&config.repo_path, &session.id)?;

            println!("{} {}", "🎯".cyan(), "Active AoC:".bold().cyan());
            println!("{}", "---".cyan());
            println!("{} {}", "Label:".cyan(), session.label.magenta());
            println!(
                "{} {}",
                "Started:".cyan(),
                format_datetime(session.created_at).blue()
            );
            println!(
                "{} {}",
                "Initial Version:".cyan(),
                session.initial_version.yellow()
            );
            println!("{} {}", "Traces:".cyan(), traces.len().to_string().yellow());
        }
        None => {
            println!("{} {}", "ℹ️".blue(), "No active AoC session.".blue());
        }
    }

    Ok(())
}

fn handle_aoc_intercept(
    config: &Config,
    model: Option<String>,
    intent: Option<String>,
    command: &str,
    args: &[String],
) -> anyhow::Result<()> {
    // Check if an active session already exists, start one if not
    let mut tmp_aoc = false;
    if kaptaind::aoc::session::load_active(&config.repo_path)?.is_none() {
        tmp_aoc = true;
        let label = intent
            .clone()
            .unwrap_or_else(|| "agent-intercept".to_string());
        handle_aoc_start(config, &label)?;
    }

    let start_time = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();

    println!(
        "{} {}",
        "🤖".cyan(),
        "Intercepting Agent execution...".bold().cyan()
    );

    // Spawn command
    let mut child = std::process::Command::new(command).args(args).spawn()?;

    let status = child.wait()?;

    let end_time = Utc::now();
    let duration = (end_time - start_time).num_milliseconds().max(0) as u64;

    // Build AgentEvent
    let agent_event = kaptaind::aoc::AgentEvent {
        id,
        timestamp: start_time,
        model: model.clone(),
        input: intent.map(serde_json::Value::String),
        output: Some(serde_json::Value::String(format!(
            "exit code: {:?}",
            status.code()
        ))),
        tools: vec![command.to_string()], // simple tool recording
        latency_ms: duration,
    };

    kaptaind::aoc::interceptor::log_event(&config.repo_path, &agent_event)?;

    println!(
        "{} {}",
        "✅".green(),
        "Agent event logged for context mapping.".bold().green()
    );

    if tmp_aoc {
        println!("{} {}", "ℹ️".blue(), "AoC session remains active for daemon to process clusters. Run 'kaptaind-cli aoc ship' later.".blue());
    }

    Ok(())
}

fn handle_aoc_log(config: &Config, limit: usize) -> anyhow::Result<()> {
    let manifests = kaptaind::aoc::session::list_manifests(&config.repo_path)?;

    if manifests.is_empty() {
        println!("No completed AoC sessions found.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = manifests
        .into_iter()
        .take(limit)
        .map(|m| {
            vec![
                m.label.magenta().to_string(),
                format!("{} → {}", m.initial_version, m.final_version)
                    .cyan()
                    .to_string(),
                m.cluster_count.to_string(),
                m.commit_count.to_string(),
                m.test_failures.to_string(),
                format_datetime(m.shipped_at).blue().to_string(),
            ]
        })
        .collect();

    print_table(
        &[
            "🏷️ Label",
            "📈 Version",
            "🗂️ Clusters",
            "🚀 Commits",
            "🧪 Failures",
            "🕒 Shipped",
        ],
        &rows,
    );

    Ok(())
}
