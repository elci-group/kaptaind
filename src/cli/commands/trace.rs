use kaptaind::config::loader::Config;
use kaptaind::util::style::*;

use crate::table::print_table;
use crate::TraceCommand;

pub fn handle_trace(config: &Config, cmd: &TraceCommand) -> anyhow::Result<()> {
    match cmd {
        TraceCommand::Log { aoc_id, limit } => {
            handle_trace_log(config, aoc_id.as_deref(), *limit, "text")?;
        }
        TraceCommand::List { format, limit } => {
            handle_trace_log(config, None, *limit, format)?;
        }
        TraceCommand::Show { cluster_id } => {
            handle_trace_show(config, cluster_id)?;
        }
        TraceCommand::Prune { days } => {
            handle_trace_prune(config, *days)?;
        }
    }
    Ok(())
}

fn handle_trace_log(
    config: &Config,
    aoc_id: Option<&str>,
    limit: usize,
    format: &str,
) -> anyhow::Result<()> {
    let target_aoc_id = match aoc_id {
        Some(id) => id.to_string(),
        None => {
            let session =
                kaptaind::aoc::session::load_active(&config.repo_path)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "No active AoC session found. Provide --aoc-id or start a session."
                    )
                })?;
            session.id
        }
    };

    let traces = kaptaind::aoc::db::get_traces_for_aoc(&config.repo_path, &target_aoc_id)?;

    if format.eq_ignore_ascii_case("json") {
        let slice: Vec<&kaptaind::aoc::TraceRecord> = traces.iter().rev().take(limit).collect();
        println!("{}", serde_json::to_string_pretty(&slice)?);
        return Ok(());
    }

    println!(
        "{} {} {}",
        "📜".cyan(),
        "Traces for AoC:".bold(),
        target_aoc_id.magenta()
    );
    println!("{}", "-".repeat(80).cyan());

    let rows: Vec<Vec<String>> = traces
        .iter()
        .rev()
        .take(limit)
        .map(|t| {
            let result = match &t.result {
                kaptaind::aoc::TraceResult::Committed { bump, version } => {
                    format!("✅ {} ({})", bump, version).green().to_string()
                }
                kaptaind::aoc::TraceResult::Skipped { reason } => {
                    format!("⏭️  Skipped ({})", reason).yellow().to_string()
                }
            };

            vec![
                t.cluster_id[..8].to_string(),
                t.started_at.format("%H:%M:%S").to_string(),
                format!("{}ms", t.duration_ms),
                result,
            ]
        })
        .collect();

    if rows.is_empty() {
        println!("No traces found for this session.");
    } else {
        print_table(&["ID", "Time", "Duration", "Result"], &rows);
    }

    Ok(())
}

fn handle_trace_show(config: &Config, cluster_id: &str) -> anyhow::Result<()> {
    let db_path = config.repo_path.join(".kaptaind").join("traces.db");
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt =
        conn.prepare("SELECT data FROM traces WHERE cluster_id = ?1 OR cluster_id LIKE ?2")?;

    let pattern = format!("{}%", cluster_id);
    let mut rows = stmt.query([cluster_id, &pattern])?;

    if let Some(row) = rows.next()? {
        let data: String = row.get(0)?;
        let trace: kaptaind::aoc::TraceRecord = serde_json::from_str(&data)?;

        println!(
            "{} {} {}",
            "🔬".cyan(),
            "Trace:".bold(),
            trace.cluster_id.magenta()
        );
        println!("{} {}", "AoC ID:".bold(), trace.aoc_id);
        println!("{} {}", "Started:".bold(), trace.started_at);
        println!("{} {}ms", "Duration:".bold(), trace.duration_ms);
        println!("{} {}", "Test:".bold(), trace.test.outcome);

        match &trace.result {
            kaptaind::aoc::TraceResult::Committed { bump, version } => {
                println!("{} {} ({})", "Result:".bold(), bump.green(), version.blue());
            }
            kaptaind::aoc::TraceResult::Skipped { reason } => {
                println!(
                    "{} {}",
                    "Result:".bold(),
                    format!("Skipped ({})", reason).yellow()
                );
            }
        }

        println!("\n{}", "📂 Touched Paths:".bold());
        for event in &trace.events {
            for path in &event.paths {
                println!(
                    "  {} {}",
                    match event.kind.as_str() {
                        "create" => "+".green(),
                        "modify" => "M".yellow(),
                        "remove" => "-".red(),
                        _ => "?".blue(),
                    },
                    path
                );
            }
        }

        if let Some(agent) = &trace.agent_event {
            println!("\n{}", "🤖 Agent Event:".bold());
            println!("  Model:   {}", agent.model.as_deref().unwrap_or("unknown"));
            println!("  Latency: {}ms", agent.latency_ms);
            println!("  Tools:   {}", agent.tools.join(", "));
        }
    } else {
        anyhow::bail!("Trace not found: {}", cluster_id);
    }

    Ok(())
}

fn handle_trace_prune(config: &Config, days: i64) -> anyhow::Result<()> {
    let deleted = kaptaind::aoc::db::prune_old_traces(&config.repo_path, days)?;
    println!(
        "{} {} traces older than {} days.",
        "🧹".green(),
        deleted,
        days
    );
    Ok(())
}
