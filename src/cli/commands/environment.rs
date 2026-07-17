use kaptaind::environment::{self, EnvironmentEvent, LifecycleAction};
use serde::Serialize;
use std::path::Path;

pub fn status(repo: &Path, format: &str) -> anyhow::Result<()> {
    let events = environment::history(repo, None)?;
    let mut latest = std::collections::BTreeMap::new();
    for event in events {
        latest.insert(event.environment.clone(), event);
    }
    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&latest)?);
    } else {
        for name in environment::STANDARD_ENVIRONMENTS {
            match latest.remove(*name) {
                Some(event) => render(vec![event], format)?,
                None => println!("{name} unknown (no lifecycle evidence)"),
            }
        }
        render(latest.into_values().collect(), format)?;
    }
    Ok(())
}

pub fn risk(repo: &Path, format: &str) -> anyhow::Result<()> {
    let report = environment::risk(repo)?;
    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("environment deployment risk: {}", report.level);
        for signal in report.signals {
            println!("- {signal}");
        }
    }
    Ok(())
}

pub fn history(repo: &Path, environment: &str, format: &str) -> anyhow::Result<()> {
    render(environment::history(repo, Some(environment))?, format)
}

pub fn diff(repo: &Path, from: &str, to: &str, format: &str) -> anyhow::Result<()> {
    #[derive(Serialize)]
    struct Diff {
        from: Option<EnvironmentEvent>,
        to: Option<EnvironmentEvent>,
        version_changed: Option<bool>,
        configuration_changed: Option<bool>,
    }
    let from_event = environment::latest(repo, from)?;
    let to_event = environment::latest(repo, to)?;
    let result = Diff {
        version_changed: from_event
            .as_ref()
            .zip(to_event.as_ref())
            .map(|(left, right)| left.version != right.version),
        configuration_changed: from_event
            .as_ref()
            .zip(to_event.as_ref())
            .and_then(|(left, right)| {
                left.config_sha256
                    .as_ref()
                    .zip(right.config_sha256.as_ref())
            })
            .map(|(left, right)| left != right),
        from: from_event,
        to: to_event,
    };
    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{} -> {}", from, to);
        println!("version changed: {:?}", result.version_changed);
        println!("configuration changed: {:?}", result.configuration_changed);
    }
    Ok(())
}

pub fn promote(
    repo: &Path,
    from: &str,
    to: &str,
    version: &str,
    adr: Option<String>,
) -> anyhow::Result<()> {
    let mut event = EnvironmentEvent::new(LifecycleAction::PromotionRequested, to, version)?;
    event.source_environment = Some(from.to_string());
    event.adr = adr;
    event.note = Some("Recorded promotion request only; Kaptaind did not deploy.".to_string());
    environment::append(repo, &event)?;
    println!("Recorded promotion request: {from} -> {to} ({version})");
    Ok(())
}

pub fn record(
    repo: &Path,
    environment: &str,
    version: &str,
    health: Option<String>,
    rollout_percent: Option<u8>,
    config_sha256: Option<String>,
    note: Option<String>,
) -> anyhow::Result<()> {
    let mut event = EnvironmentEvent::new(LifecycleAction::Observed, environment, version)?;
    event.health = health;
    event.rollout_percent = rollout_percent;
    event.config_sha256 = config_sha256;
    event.note = note;
    environment::append(repo, &event)?;
    println!("Recorded observed deployment: {environment} ({version})");
    Ok(())
}

pub fn rollback(
    repo: &Path,
    environment: &str,
    version: &str,
    adr: Option<String>,
) -> anyhow::Result<()> {
    let mut event = EnvironmentEvent::new(LifecycleAction::RollbackRecorded, environment, version)?;
    event.adr = adr;
    event.note = Some("Recorded rollback decision only; Kaptaind did not deploy.".to_string());
    environment::append(repo, &event)?;
    println!("Recorded rollback for {environment} to {version}");
    Ok(())
}

fn render(events: Vec<EnvironmentEvent>, format: &str) -> anyhow::Result<()> {
    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&events)?);
    } else if events.is_empty() {
        println!("No environment lifecycle records found.");
    } else {
        for event in events {
            println!(
                "{} {} {} health={} rollout={} adr={}",
                event.environment,
                event.version,
                serde_json::to_string(&event.action)?.trim_matches('"'),
                event.health.as_deref().unwrap_or("unknown"),
                event
                    .rollout_percent
                    .map_or("unknown".to_string(), |value| format!("{value}%")),
                event.adr.as_deref().unwrap_or("none")
            );
        }
    }
    Ok(())
}
