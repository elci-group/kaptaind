//! `kaptaind-cli migrate` and `kaptaind-cli schema`: manage the `.kaptaind`
//! semantic-state document and its schema.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use kaptaind::util::style::*;
use serde_json::json;
use std::path::Path;

use crate::commands::MigrateArgs;
use kaptaind::schema::{
    self, latest_version, migrate_document, plan, schemas, SchemaVersion, SemanticDocument,
    STATE_FILE,
};

pub fn handle_migrate(repo: &Path, args: &MigrateArgs) -> Result<()> {
    let state_path = repo.join(STATE_FILE);
    let text = match std::fs::read_to_string(&state_path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %state_path.display(), "no existing state document; starting from legacy detection");
            None
        }
        Err(error) => {
            tracing::warn!(path = %state_path.display(), error = %error, "failed to read state document");
            return Err(error).context(format!("failed to read {}", state_path.display()));
        }
    };
    let document = schema::detect(text.as_deref())?;
    let current = document.format();
    let target = match &args.to {
        Some(raw) => raw
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid target schema `{raw}`: {e}"))?,
        None => latest_version(),
    };

    if current > latest_version() {
        bail!(
            ".kaptaind format {current} is newer than this kaptaind supports ({}); \
             read-only mode, migration unavailable",
            latest_version()
        );
    }

    let steps = plan(current, target)?;
    let outdated = !steps.is_empty();

    if args.check {
        let status = if outdated { "OUTDATED" } else { "UP-TO-DATE" };
        let path: Vec<String> = steps
            .iter()
            .map(|s| format!("{} -> {}", s.from, s.to))
            .collect();
        if args.format == "json" {
            println!(
                "{}",
                json!({
                    "state_file": state_path.display().to_string(),
                    "format": current.to_string(),
                    "installed_schema": latest_version().to_string(),
                    "target": target.to_string(),
                    "status": status,
                    "required_migrations": path,
                })
            );
        } else {
            println!(
                "{} .kaptaind: v{} (installed schema: v{})",
                if outdated { "⚠️ " } else { "✅" },
                current,
                latest_version()
            );
            println!("status: {status}");
            for step in &path {
                println!("  {step}");
            }
        }
        if outdated && args.strict {
            std::process::exit(1);
        }
        return Ok(());
    }

    // Bootstrap the baseline version from the repo's VERSION file when
    // migrating out of legacy state.
    let mut document = document;
    if current == SchemaVersion::new(1, 0) {
        let version_path = repo.join("VERSION");
        document.baseline.version = match std::fs::read_to_string(&version_path) {
            Ok(v) => Some(v.trim().to_string()),
            Err(error) => {
                tracing::debug!(path = %version_path.display(), error = %error, "no VERSION file to bootstrap baseline from");
                None
            }
        };
    }

    let (migrated, applied) = migrate_document(&document, target, args.allow_lossy)?;
    let digest_before = document.digest();
    let digest_after = migrated.digest();

    let state_parent = state_path
        .parent()
        .context("schema state path has no parent directory")?;
    std::fs::create_dir_all(state_parent)
        .with_context(|| format!("failed to create {}", state_path.display()))?;
    std::fs::write(&state_path, migrated.to_canonical_toml())
        .with_context(|| format!("failed to write {}", state_path.display()))?;

    let lossy = applied.iter().any(|s| s.lossy);
    schema::append_ledger(
        &repo.join(schema::MIGRATIONS_DIR),
        &schema::LedgerEntry {
            from: current,
            to: target,
            tool: format!("kaptaind-cli {}", env!("CARGO_PKG_VERSION")),
            timestamp: Utc::now(),
            canonicalization: migrated.canonicalization().to_string(),
            digest_before: digest_before.clone(),
            digest_after: digest_after.clone(),
            lossy,
        },
    )?;

    if args.format == "json" {
        println!(
            "{}",
            json!({
                "from": current.to_string(),
                "to": target.to_string(),
                "steps": applied.iter().map(|s| format!("{}->{}", s.from, s.to)).collect::<Vec<_>>(),
                "lossy": lossy,
                "digest_before": digest_before,
                "digest_after": digest_after,
            })
        );
    } else {
        println!(
            "{} migrated .kaptaind v{} -> v{}",
            "✅".green(),
            current,
            target
        );
        for step in &applied {
            println!(
                "  {} -> {}{}",
                step.from,
                step.to,
                if step.lossy { " (lossy)" } else { "" }
            );
        }
        println!("digest: {digest_after}");
    }
    Ok(())
}

pub fn handle_schema_list() -> Result<()> {
    println!("Installed schemas");
    println!("-----------------------------------------");
    for info in schemas() {
        let marker = if info.version == latest_version() {
            "   <- current"
        } else {
            ""
        };
        println!("{:<6} {:<16}{}", info.version, info.family, marker);
    }
    Ok(())
}

pub fn handle_schema_explain(version: &str) -> Result<()> {
    let parsed: SchemaVersion = version
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid schema version `{version}`: {e}"))?;
    let Some(info) = kaptaind::schema::find(parsed) else {
        bail!("unknown schema version {parsed}");
    };
    println!("{} ({})", info.version, info.family);
    println!("{}", info.description);
    if info.family == "semantic-state" {
        println!("example document:");
        println!(
            "{}",
            SemanticDocument::empty(info.version).to_canonical_toml()
        );
    }
    Ok(())
}
