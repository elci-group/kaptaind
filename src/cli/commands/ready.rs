//! `kaptaind ready`: evaluate every mutation gate at once.

use kaptaind::config::loader::Config;
use kaptaind::readiness::{self, Gate, GateState, Purpose};
use kaptaind::util::style::*;

/// Exit code when a gate required by `--for` blocks.
pub const EXIT_NOT_READY: i32 = 4;

/// Render the readiness report as the stable JSON document
/// `{"for", "ready", "gates": [{"id","state","detail","fix"}]}`.
pub fn to_json(purpose: Purpose, gates: &[Gate]) -> serde_json::Value {
    serde_json::json!({
        "for": purpose.as_str(),
        "ready": readiness::is_ready(gates),
        "gates": gates,
    })
}

fn state_label(state: GateState) -> String {
    let label = format!("{:<5}", state.as_str());
    match state {
        GateState::Pass => label.green().to_string(),
        GateState::Block => label.red().bold().to_string(),
        GateState::Warn => label.yellow().to_string(),
        GateState::Info => label.cyan().to_string(),
        GateState::NotApplicable => label.dimmed().to_string(),
    }
}

/// Human-readable report.
pub fn render_text(purpose: Purpose, gates: &[Gate]) -> String {
    let mut out = String::new();
    let width = gates.iter().map(|g| g.id.len()).max().unwrap_or(0);
    out.push_str(&format!("kaptaind ready --for {purpose}\n\n"));
    for gate in gates {
        out.push_str(&format!(
            "  {} {:<width$}  {}\n",
            state_label(gate.state),
            gate.id,
            gate.detail
        ));
        if let Some(fix) = &gate.fix {
            if gate.state != GateState::Pass {
                out.push_str(&format!("  {:<5} {:<width$}  fix: {}\n", "", "", fix));
            }
        }
    }
    let blockers = gates.iter().filter(|g| g.state == GateState::Block).count();
    out.push('\n');
    if blockers == 0 {
        out.push_str(&format!("{} ready for {purpose}\n", "✅".green()));
    } else {
        out.push_str(&format!(
            "{} not ready for {purpose}: {blockers} blocking gate(s)\n",
            "❌".red()
        ));
    }
    out
}

pub fn handle_ready(config: &Config, purpose: &str, json: bool) -> anyhow::Result<()> {
    let purpose: Purpose = purpose.parse()?;
    let gates = readiness::evaluate(config, purpose);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&to_json(purpose, &gates))?
        );
    } else {
        print!("{}", render_text(purpose, &gates));
    }
    if !readiness::is_ready(&gates) {
        std::process::exit(EXIT_NOT_READY);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_document_shape() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        let gates = readiness::evaluate(&config, Purpose::Commit);
        let doc = to_json(Purpose::Commit, &gates);
        assert_eq!(doc["for"], "commit");
        assert!(doc["ready"].is_boolean());
        let arr = doc["gates"].as_array().unwrap();
        assert_eq!(arr.len(), readiness::GATE_IDS.len());
        for g in arr {
            for key in ["id", "state", "detail", "fix"] {
                assert!(g.get(key).is_some(), "missing {key}");
            }
        }
    }

    #[test]
    fn text_lists_fix_for_blockers() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            repo_path: dir.path().to_path_buf(),
            ..Config::default()
        };
        let gates = readiness::evaluate(&config, Purpose::Commit);
        let text = render_text(Purpose::Commit, &gates);
        assert!(text.contains("operation_mode"));
        assert!(text.contains("fix: set [operation] mode = \"actuate\""));
        assert!(text.contains("not ready for commit"));
    }
}
