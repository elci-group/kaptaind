//! Enterprise governance readiness assessment.

use kaptaind::config::loader::Config;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Control {
    id: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Debug, Serialize)]
struct GovernanceAssessment {
    schema_version: u8,
    enterprise_enforced: bool,
    ready: bool,
    controls: Vec<Control>,
}

fn control(id: &'static str, result: anyhow::Result<()>) -> Control {
    match result {
        Ok(()) => Control {
            id,
            status: "pass",
            detail: "verified".to_string(),
        },
        Err(error) => Control {
            id,
            status: "fail",
            detail: error.to_string(),
        },
    }
}

fn assess(config: &Config) -> GovernanceAssessment {
    let mut controls = vec![control(
        "enterprise_posture",
        if config.governance.enforce_enterprise_controls {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "[governance].enforce_enterprise_controls is false"
            ))
        },
    )];
    controls.push(control("configuration", config.validate()));

    if config.governance.enforce_enterprise_controls {
        let policy = config
            .policy_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("config.policy_id is missing"))
            .and_then(|policy_id| {
                kaptaind::daemon::policy::Policy::load_with_trust(
                    &config.repo_path,
                    policy_id,
                    &config.policy_trust,
                    config.policy_keyring_path().as_deref(),
                )
            });
        match policy {
            Ok(policy) => controls.push(control(
                "signed_release_policy",
                policy.validate_enterprise_release_controls(),
            )),
            Err(error) => controls.push(Control {
                id: "signed_release_policy",
                status: "fail",
                detail: error.to_string(),
            }),
        }
        controls.push(control(
            "audit_chain",
            kaptaind::audit::verify_chain(&config.repo_path),
        ));
        controls.push(control(
            "audit_export",
            config
                .audit
                .export
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("[audit.export] is missing"))
                .and_then(|export| kaptaind::audit::verify_export(&config.repo_path, export)),
        ));
    }
    let ready = controls.iter().all(|control| control.status == "pass");
    GovernanceAssessment {
        schema_version: 1,
        enterprise_enforced: config.governance.enforce_enterprise_controls,
        ready,
        controls,
    }
}

pub fn handle_governance_assess(config: &Config, format: &str) -> anyhow::Result<()> {
    let assessment = assess(config);
    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&assessment)?);
    } else {
        for control in &assessment.controls {
            println!("{}: {} — {}", control.id, control.status, control.detail);
        }
        println!("enterprise_ready={}", assessment.ready);
    }
    if !assessment.ready {
        anyhow::bail!("enterprise governance assessment failed")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assessment_fails_closed_when_enterprise_mode_is_not_enabled() {
        let report = assess(&Config::default());
        assert!(!report.ready);
        assert!(report
            .controls
            .iter()
            .any(|control| control.id == "enterprise_posture" && control.status == "fail"));
    }
}
