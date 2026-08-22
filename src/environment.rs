//! Environment lifecycle observation and promotion evidence.
//!
//! Kaptaind never deploys or rolls back infrastructure. It records externally
//! performed deployment facts, requested promotions, and rollback decisions in
//! a durable timeline so release governance has one authoritative view.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub const STANDARD_ENVIRONMENTS: &[&str] =
    &["local", "dev", "qa", "staging", "canary", "production"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleAction {
    Observed,
    PromotionRequested,
    RollbackRecorded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentEvent {
    pub schema_version: u8,
    pub action: LifecycleAction,
    pub environment: String,
    pub version: String,
    pub occurred_at: DateTime<Utc>,
    pub source_environment: Option<String>,
    pub adr: Option<String>,
    pub health: Option<String>,
    pub rollout_percent: Option<u8>,
    pub config_sha256: Option<String>,
    pub note: Option<String>,
}

fn validate_environment(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        bail!("environment must be 1-64 ASCII letters, digits, '-' or '_'");
    }
    Ok(())
}

impl EnvironmentEvent {
    pub fn new(action: LifecycleAction, environment: &str, version: &str) -> Result<Self> {
        validate_environment(environment)?;
        if version.is_empty() || version.len() > 128 {
            bail!("environment version must be 1-128 characters");
        }
        Ok(Self {
            schema_version: 1,
            action,
            environment: environment.to_string(),
            version: version.to_string(),
            occurred_at: Utc::now(),
            source_environment: None,
            adr: None,
            health: None,
            rollout_percent: None,
            config_sha256: None,
            note: None,
        })
    }
}

fn timeline_path(repo_path: &Path) -> std::path::PathBuf {
    repo_path.join(".kaptaind/environments/timeline.jsonl")
}

/// Append a lifecycle fact and a digest-only audit event. This function records
/// evidence; it intentionally has no provider/deployment side effects.
pub fn append(repo_path: &Path, event: &EnvironmentEvent) -> Result<()> {
    validate_environment(&event.environment)?;
    if event.rollout_percent.is_some_and(|value| value > 100) {
        bail!("environment rollout percent must be at most 100");
    }
    let path = timeline_path(repo_path);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("environment timeline has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let encoded = serde_json::to_vec(event)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&encoded)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    crate::audit::log_event(
        repo_path,
        "environment",
        "environment_lifecycle_recorded",
        true,
        serde_json::json!({
            "action": event.action,
            "environment": event.environment,
            "version": event.version,
            "source_environment": event.source_environment,
            "adr": event.adr,
            "health": event.health,
            "rollout_percent": event.rollout_percent,
            "config_sha256": event.config_sha256,
        }),
    );
    Ok(())
}

pub fn history(repo_path: &Path, environment: Option<&str>) -> Result<Vec<EnvironmentEvent>> {
    let path = timeline_path(repo_path);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map(|events: Vec<EnvironmentEvent>| {
            events
                .into_iter()
                .filter(|event| environment.is_none_or(|name| event.environment == name))
                .collect()
        })
        .map_err(Into::into)
}

pub fn latest(repo_path: &Path, environment: &str) -> Result<Option<EnvironmentEvent>> {
    Ok(history(repo_path, Some(environment))?.into_iter().last())
}

/// Explain deployment risk from recorded facts. This is deliberately
/// deterministic and evidence-based: it is not a deployment authorization.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RiskReport {
    pub level: String,
    pub signals: Vec<String>,
}

pub fn risk(repo_path: &Path) -> Result<RiskReport> {
    let events = history(repo_path, None)?;
    let production = events
        .iter()
        .rev()
        .find(|event| event.environment == "production");
    let staging = events
        .iter()
        .rev()
        .find(|event| event.environment == "staging");
    let mut signals = Vec::new();
    let mut high = false;
    let mut medium = false;
    if let Some(event) = production {
        if event.action == LifecycleAction::RollbackRecorded {
            high = true;
            signals.push("latest production event is a rollback".to_string());
        }
        if event
            .health
            .as_deref()
            .is_some_and(|health| !health.eq_ignore_ascii_case("healthy"))
        {
            high = true;
            signals.push("latest production health is not healthy".to_string());
        }
        if event.rollout_percent.is_some_and(|percent| percent < 100) {
            medium = true;
            signals.push("latest production rollout is incomplete".to_string());
        }
    } else {
        medium = true;
        signals.push("no production observation recorded".to_string());
    }
    if let (Some(staging), Some(production)) = (staging, production) {
        if staging.version != production.version {
            medium = true;
            signals.push(format!(
                "staging {} differs from production {}",
                staging.version, production.version
            ));
        }
        if staging.config_sha256.is_some()
            && production.config_sha256.is_some()
            && staging.config_sha256 != production.config_sha256
        {
            medium = true;
            signals.push("staging and production configuration digests differ".to_string());
        }
    }
    if signals.is_empty() {
        signals.push("recorded rollout and health evidence has no active risk signal".to_string());
    }
    Ok(RiskReport {
        level: if high {
            "high"
        } else if medium {
            "medium"
        } else {
            "low"
        }
        .to_string(),
        signals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_is_append_only_and_queryable_by_environment() {
        let dir = tempfile::tempdir().unwrap();
        let mut staging =
            EnvironmentEvent::new(LifecycleAction::Observed, "staging", "2.4.0").unwrap();
        staging.health = Some("healthy".to_string());
        append(dir.path(), &staging).unwrap();
        let mut production =
            EnvironmentEvent::new(LifecycleAction::PromotionRequested, "production", "2.4.0")
                .unwrap();
        production.source_environment = Some("staging".to_string());
        production.adr = Some("ADR-42".to_string());
        append(dir.path(), &production).unwrap();
        assert_eq!(history(dir.path(), Some("staging")).unwrap(), vec![staging]);
        assert_eq!(latest(dir.path(), "production").unwrap(), Some(production));
        assert!(
            std::fs::read_to_string(dir.path().join(".kaptaind/audit.jsonl"))
                .unwrap()
                .contains("environment_lifecycle_recorded")
        );
    }

    #[test]
    fn risk_explains_incomplete_or_divergent_release_evidence() {
        let dir = tempfile::tempdir().unwrap();
        append(
            dir.path(),
            &EnvironmentEvent::new(LifecycleAction::Observed, "staging", "2.4.0").unwrap(),
        )
        .unwrap();
        let report = risk(dir.path()).unwrap();
        assert_eq!(report.level, "medium");
        assert!(report
            .signals
            .iter()
            .any(|signal| signal.contains("no production")));
    }
}
