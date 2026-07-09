use crate::config::loader::{BuildConfig, Config, ShipConfig, ShipKindConfig};
use crate::qualification::engine::{evaluate, QualificationResult};
use crate::release::index::{append_ship_index, load_index, load_ship_index};
use crate::release::packager;
use crate::schedule::next_fire_after;
use crate::util::style::*;
use anyhow::{anyhow, Context};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Kind of release being produced by `ship`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipKind {
    /// Manual `ship run` / `ship plan`.
    Manual,
    /// Production stable release (`ship stable`).
    Stable,
    /// Bleeding-edge nightly release (`ship nightly`).
    Nightly,
}

impl ShipKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShipKind::Manual => "manual",
            ShipKind::Stable => "stable",
            ShipKind::Nightly => "nightly",
        }
    }
}

/// Output format for `ship` commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Human-readable terminal output.
    #[default]
    Text,
    /// Machine-readable JSON.
    Json,
}

/// Options controlling a single `ship` invocation.
pub struct ShipOptions {
    pub dry_run: bool,
    pub targets: Option<Vec<String>>,
    pub channels: Option<Vec<String>>,
    pub force: bool,
    pub kind: ShipKind,
    /// Explicit version to use instead of reading `VERSION`.
    pub version_override: Option<String>,
    /// Effective qualification-gate requirement for this invocation.
    pub require_qualification: bool,
    /// Output format for plan/summary/status.
    pub format: OutputFormat,
}

impl Default for ShipOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            targets: None,
            channels: None,
            force: false,
            kind: ShipKind::Manual,
            version_override: None,
            require_qualification: true,
            format: OutputFormat::Text,
        }
    }
}

/// Result of a completed `ship` run.
pub struct ShipResult {
    pub version: String,
    pub artifacts: Vec<PathBuf>,
    pub distributed: Vec<String>,
}

/// Output from one target build.
struct BuiltTarget {
    triple: String,
    kaptaind: PathBuf,
    kaptaind_cli: PathBuf,
}

/// Manual release pipeline: build binaries for configured targets, produce
/// installers, and distribute to package managers / app stores.
pub async fn run_ship(config: &Config, opts: ShipOptions) -> anyhow::Result<ShipResult> {
    if !config.ship.enabled {
        anyhow::bail!("ship is not enabled. Add [ship] enabled = true to kaptaind.toml.");
    }

    let version = match opts.version_override {
        Some(v) => v,
        None => read_version(&config.repo_path)?,
    };
    let kind_cfg = kind_config(&config.ship, opts.kind);
    let targets = resolve_targets(
        &config.ship.targets,
        opts.targets.or_else(|| kind_cfg.targets.clone()),
    );
    let channels = resolve_kind_channels(&config.ship, opts.channels, opts.kind);

    if targets.is_empty() {
        anyhow::bail!("no build targets selected");
    }
    if channels.is_empty() {
        anyhow::bail!("no distribution channels selected");
    }

    println!(
        "{} {} {}",
        "🚢".cyan(),
        "Kaptaind Ship".bold().cyan(),
        format!("v{}", version).magenta()
    );
    println!("   Kind:     {}", opts.kind.as_str().yellow());
    println!("   Targets:  {}", targets.join(", ").yellow());
    println!("   Channels: {}", channels.join(", ").yellow());

    if opts.dry_run {
        if opts.format == OutputFormat::Json {
            let plan = serde_json::json!({
                "version": version,
                "kind": opts.kind.as_str(),
                "targets": targets,
                "channels": channels,
                "require_qualification": opts.require_qualification,
                "dry_run": true,
            });
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            println!(
                "\n{} {}",
                "✅".green(),
                "Dry-run plan complete. No changes made.".green()
            );
        }
        return Ok(ShipResult {
            version,
            artifacts: Vec::new(),
            distributed: Vec::new(),
        });
    }

    // Qualification gate
    if opts.require_qualification && config.qualification.enabled && !opts.force {
        check_qualification(config)?;
    }

    let ship_dir = config
        .repo_path
        .join(".kaptaind")
        .join("ship")
        .join(&version);
    std::fs::create_dir_all(&ship_dir)?;

    let mut all_artifacts: Vec<PathBuf> = Vec::new();
    let mut distributed: Vec<String> = Vec::new();

    // ------------------------------------------------------------------
    // SBOM generation
    // ------------------------------------------------------------------
    if config.ship.sbom.enabled {
        match crate::release::sbom::generate_sbom(&config.repo_path, &config.ship.sbom.format) {
            Ok(sbom_path) => {
                all_artifacts.push(sbom_path);
                distributed.push("sbom".to_string());
            }
            Err(err) => {
                eprintln!(
                    "{} {}: {}",
                    "⚠️".yellow(),
                    "SBOM generation failed".yellow(),
                    err
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // 1. Build binaries for every target
    // ------------------------------------------------------------------
    let mut built_targets: Vec<BuiltTarget> = Vec::new();
    for triple in &targets {
        match build_target(&config.repo_path, triple, &config.build).await {
            Ok(bt) => {
                println!(
                    "{} {} {}",
                    "✅".green(),
                    "Built target".green(),
                    triple.yellow()
                );
                built_targets.push(bt);
            }
            Err(err) => {
                eprintln!(
                    "{} {} {}: {}",
                    "⚠️".yellow(),
                    "Skipping target".yellow(),
                    triple.yellow(),
                    err
                );
                if targets.len() == 1 {
                    return Err(err);
                }
            }
        }
    }

    if built_targets.is_empty() {
        anyhow::bail!("no targets built successfully");
    }

    // ------------------------------------------------------------------
    // 2. Package binaries
    // ------------------------------------------------------------------
    if channels.contains(&"binaries".to_string()) {
        for bt in &built_targets {
            let target_dir = ship_dir.join(&bt.triple);
            std::fs::create_dir_all(&target_dir)?;

            for (name, path) in [
                ("kaptaind", &bt.kaptaind),
                ("kaptaind-cli", &bt.kaptaind_cli),
            ] {
                if path.exists() {
                    let pkg = packager::create_from_artifact(
                        &version,
                        "ship",
                        0.0,
                        path,
                        name,
                        &target_dir,
                    )?;
                    all_artifacts.push(pkg.tarball);
                }
            }
        }
        distributed.push("binaries".to_string());
    }

    // ------------------------------------------------------------------
    // 3. Shell installer bundle
    // ------------------------------------------------------------------
    if channels.contains(&"shell-installer".to_string()) {
        let installer_dir = ship_dir.join("installer");
        std::fs::create_dir_all(&installer_dir)?;
        let bundle_path = installer_dir.join(format!("kaptaind-{}-installer.tar.gz", version));
        build_shell_installer_bundle(
            &config.repo_path,
            &installer_dir,
            &bundle_path,
            &version,
            &built_targets,
        )
        .await?;
        all_artifacts.push(bundle_path);
        distributed.push("shell-installer".to_string());
    }

    // ------------------------------------------------------------------
    // 4. Tauri desktop bundles
    // ------------------------------------------------------------------
    if channels.contains(&"tauri".to_string()) {
        let tauri_dir = config.repo_path.join("apps").join("desktop");
        if tauri_dir.join("src-tauri").join("tauri.conf.json").exists() {
            match build_tauri_bundles(&tauri_dir, &ship_dir).await {
                Ok(mut artifacts) => {
                    all_artifacts.append(&mut artifacts);
                    distributed.push("tauri".to_string());
                }
                Err(err) => {
                    eprintln!("{} Tauri bundles failed: {}", "⚠️".yellow(), err);
                }
            }
        } else {
            eprintln!(
                "{} {}",
                "⚠️".yellow(),
                "Tauri config not found at apps/desktop/src-tauri/tauri.conf.json; skipping"
                    .yellow()
            );
        }
    }

    // ------------------------------------------------------------------
    // 5. Package managers (Homebrew)
    // ------------------------------------------------------------------
    for pm in &config.ship.channels.package_managers {
        if !channels.contains(&format!("package-manager:{}", pm.kind)) {
            continue;
        }
        match pm.kind.as_str() {
            "homebrew" => {
                let formula_path = ship_dir.join(format!("{}.rb", pm.formula_name));
                generate_homebrew_formula(
                    &config.repo_path,
                    &formula_path,
                    &version,
                    &built_targets,
                )
                .await?;
                all_artifacts.push(formula_path);
                distributed.push(format!("homebrew:{}", pm.formula_name));
            }
            _ => {
                eprintln!(
                    "{} Unknown package manager kind: {}",
                    "⚠️".yellow(),
                    pm.kind
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // 6. Generate checksums and detached GPG signatures
    // ------------------------------------------------------------------
    let sign_artifacts = signing_enabled(&config.ship, opts.kind);
    let gpg_key_id = config.ship.gpg_key_id.as_deref();
    let artifacts_to_checksum: Vec<PathBuf> = all_artifacts.clone();
    for artifact in &artifacts_to_checksum {
        if !artifact.exists() {
            continue;
        }
        match generate_checksum(artifact) {
            Ok(checksum_path) => {
                all_artifacts.push(checksum_path.clone());
                if sign_artifacts {
                    match gpg_sign_checksum(&checksum_path, gpg_key_id).await {
                        Ok(sig_path) => all_artifacts.push(sig_path),
                        Err(err) => {
                            eprintln!(
                                "{} {}: {}",
                                "⚠️".yellow(),
                                format!("Failed to sign {}", checksum_path.display()).yellow(),
                                err
                            );
                        }
                    }
                }
            }
            Err(err) => {
                eprintln!(
                    "{} {}: {}",
                    "⚠️".yellow(),
                    format!("Failed to checksum {}", artifact.display()).yellow(),
                    err
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // 7. SLSA provenance attestation
    // ------------------------------------------------------------------
    if config.ship.provenance.enabled {
        match crate::release::provenance::generate_provenance(
            &config.repo_path,
            &version,
            opts.kind,
            &targets,
            &artifacts_to_checksum,
            &config.ship.provenance,
        ) {
            Ok(prov_path) => {
                all_artifacts.push(prov_path.clone());
                if sign_artifacts {
                    match crate::release::provenance::sign_provenance(&prov_path, gpg_key_id).await
                    {
                        Ok(sig_path) => all_artifacts.push(sig_path),
                        Err(err) => {
                            eprintln!(
                                "{} {}: {}",
                                "⚠️".yellow(),
                                format!("Failed to sign provenance {}", prov_path.display())
                                    .yellow(),
                                err
                            );
                        }
                    }
                }
                distributed.push("provenance".to_string());
            }
            Err(err) => {
                eprintln!(
                    "{} {}: {}",
                    "⚠️".yellow(),
                    "Provenance generation failed".yellow(),
                    err
                );
            }
        }
    }

    let short_commit =
        git_short_commit(&config.repo_path).unwrap_or_else(|_| "unknown".to_string());

    // ------------------------------------------------------------------
    // 8. App stores / GitHub Releases
    // ------------------------------------------------------------------
    for store in &config.ship.channels.app_stores {
        if !channels.contains(&format!("app-store:{}", store.kind)) {
            continue;
        }
        match store.kind.as_str() {
            "github-releases" => {
                let (draft, prerelease) = match opts.kind {
                    ShipKind::Stable => (kind_cfg.draft, kind_cfg.prerelease),
                    ShipKind::Nightly => (kind_cfg.draft, true),
                    ShipKind::Manual => (store.draft, store.prerelease),
                };
                let notes = if kind_cfg.release_notes {
                    generate_release_notes(&config.repo_path, &version, opts.kind, &short_commit)
                } else {
                    None
                };
                publish_github_release(
                    &version,
                    &all_artifacts,
                    draft,
                    prerelease,
                    notes.as_deref(),
                )
                .await?;
                distributed.push("github-releases".to_string());
            }
            _ => {
                eprintln!("{} Unknown app store kind: {}", "⚠️".yellow(), store.kind);
            }
        }
    }

    // ------------------------------------------------------------------
    // 9. Persist ship index
    // ------------------------------------------------------------------
    append_ship_index(
        &config.repo_path,
        opts.kind.as_str(),
        &version,
        &built_targets
            .iter()
            .map(|bt| bt.triple.clone())
            .collect::<Vec<_>>(),
        &distributed,
        &all_artifacts
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>(),
    );

    // Write a human-readable "last ship" summary for `ship status`.
    write_last_ship(
        &config.repo_path,
        opts.kind.as_str(),
        &version,
        &distributed,
        &all_artifacts,
    )?;

    // Create and optionally push a signed git tag for stable/nightly releases.
    if matches!(opts.kind, ShipKind::Stable | ShipKind::Nightly) {
        let sign_tag = signing_enabled(&config.ship, opts.kind);
        if let Err(err) = create_git_tag(
            &config.repo_path,
            &version,
            opts.kind,
            sign_tag,
            config.ship.gpg_key_id.as_deref(),
        )
        .await
        {
            eprintln!(
                "{} {}: {}",
                "⚠️".yellow(),
                "Failed to create git tag".yellow(),
                err
            );
        } else if kind_cfg.push_tag && config.push.enabled {
            if let Err(err) = push_git_tag(&config.repo_path, &config.push.remote, &version).await {
                eprintln!(
                    "{} {}: {}",
                    "⚠️".yellow(),
                    "Failed to push git tag".yellow(),
                    err
                );
            }
        }
    }

    // Retain only the configured number of nightly releases.
    if opts.kind == ShipKind::Nightly {
        if let Err(err) = prune_nightlies(&config.repo_path, config.ship.nightly.retain_count) {
            eprintln!(
                "{} {}: {}",
                "⚠️".yellow(),
                "Failed to prune old nightly releases".yellow(),
                err
            );
        }
    }

    crate::audit::log_release(
        &config.repo_path,
        "kaptaind-cli",
        &version,
        opts.kind.as_str(),
        &distributed,
        true,
    );

    if opts.format == OutputFormat::Json {
        let summary = serde_json::json!({
            "version": version,
            "kind": opts.kind.as_str(),
            "targets": built_targets.iter().map(|bt| bt.triple.clone()).collect::<Vec<_>>(),
            "channels": distributed,
            "artifacts": all_artifacts.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "\n{} {} {}",
            "✅".green(),
            "Ship complete.".green().bold(),
            format!(
                "{} artifact(s), {} channel(s)",
                all_artifacts.len(),
                distributed.len()
            )
            .yellow()
        );
    }

    Ok(ShipResult {
        version,
        artifacts: all_artifacts,
        distributed,
    })
}

/// Run a stable release from the current `VERSION`.
pub async fn run_stable(config: &Config, opts: ShipOptions) -> anyhow::Result<ShipResult> {
    let version = read_version(&config.repo_path)?;
    let require_qualification = config
        .ship
        .stable
        .require_qualification
        .unwrap_or(opts.require_qualification);
    run_ship(
        config,
        ShipOptions {
            kind: ShipKind::Stable,
            version_override: Some(version),
            require_qualification,
            ..opts
        },
    )
    .await
}

/// Run a nightly release with a computed prerelease version.
pub async fn run_nightly(config: &Config, opts: ShipOptions) -> anyhow::Result<ShipResult> {
    let base = read_version(&config.repo_path)?;
    let commit = git_short_commit(&config.repo_path).unwrap_or_else(|_| "unknown".to_string());
    if let Some(existing) = find_existing_nightly(&config.repo_path, &base, &commit) {
        if !opts.force {
            anyhow::bail!(
                "nightly release {} already shipped for this commit (pass --force to re-ship)",
                existing
            );
        }
    }
    let version = nightly_version(&config.repo_path, &base)?;
    let require_qualification = config
        .ship
        .nightly
        .require_qualification
        .unwrap_or(opts.require_qualification);
    run_ship(
        config,
        ShipOptions {
            kind: ShipKind::Nightly,
            version_override: Some(version),
            require_qualification,
            ..opts
        },
    )
    .await
}

/// Print the last ship run, if any.
pub fn print_ship_status(repo_path: &Path, format: OutputFormat) -> anyhow::Result<()> {
    let index = load_ship_index(repo_path);
    if let Some(last) = index.ships.last() {
        if format == OutputFormat::Json {
            println!("{}", serde_json::to_string_pretty(last)?);
        } else {
            let dt = chrono::DateTime::from_timestamp(last.shipped_at, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("{} {}", "🚢".cyan(), "Last ship".bold().cyan());
            println!("   Kind:     {}", last.kind.clone().yellow());
            println!("   Version:  {}", last.version.clone().magenta());
            println!("   When:     {}", dt.bright_black());
            println!("   Targets:  {}", last.targets.join(", ").yellow());
            println!("   Channels: {}", last.channels.join(", ").yellow());
            println!(
                "   Artifacts: {}",
                last.artifacts.len().to_string().yellow()
            );
        }
    } else if format == OutputFormat::Json {
        println!("null");
    } else {
        println!("{} {}", "ℹ️".blue(), "No ship history found.".blue());
    }
    Ok(())
}

/// Print scheduled auto-release status.
pub fn print_auto_ship_status(config: &Config, format: OutputFormat) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    let nightly_next = config
        .ship
        .auto_nightly
        .enabled
        .then(|| {
            next_fire_after(
                now,
                &config.ship.auto_nightly.schedule,
                &config.ship.auto_nightly.cron_timezone,
            )
        })
        .flatten();
    let stable_next = config
        .ship
        .auto_stable
        .enabled
        .then(|| {
            next_fire_after(
                now,
                &config.ship.auto_stable.schedule,
                &config.ship.auto_stable.cron_timezone,
            )
        })
        .flatten();

    if format == OutputFormat::Json {
        let json = serde_json::json!({
            "auto_nightly": {
                "enabled": config.ship.auto_nightly.enabled,
                "schedule": config.ship.auto_nightly.schedule,
                "cron_timezone": config.ship.auto_nightly.cron_timezone,
                "next_fire": nightly_next.map(|dt| dt.to_rfc3339()),
                "require_qualification": config.ship.auto_nightly.require_qualification,
            },
            "auto_stable": {
                "enabled": config.ship.auto_stable.enabled,
                "schedule": config.ship.auto_stable.schedule,
                "cron_timezone": config.ship.auto_stable.cron_timezone,
                "next_fire": stable_next.map(|dt| dt.to_rfc3339()),
                "require_qualification": config.ship.auto_stable.require_qualification,
            }
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{} {}", "🗓️".cyan(), "Auto-ship schedule".bold().cyan());
        print_auto_kind_status("Nightly", &config.ship.auto_nightly, nightly_next);
        print_auto_kind_status("Stable", &config.ship.auto_stable, stable_next);
    }
    Ok(())
}

fn print_auto_kind_status(
    label: &str,
    cfg: &crate::config::loader::ShipAutoConfig,
    next_fire: Option<chrono::DateTime<chrono::Utc>>,
) {
    let status = if cfg.enabled {
        "enabled".green()
    } else {
        "disabled".bright_black()
    };
    println!("   {}: {}", label.yellow(), status);
    if cfg.enabled {
        println!(
            "      Schedule:  {} ({})",
            cfg.schedule.clone().yellow(),
            cfg.cron_timezone.clone().yellow()
        );
        let next_str = next_fire
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unable to compute".to_string());
        println!("      Next fire: {}", next_str.yellow());
        println!(
            "      Qualification gate: {}",
            if cfg.require_qualification {
                "required".green()
            } else {
                "skipped".yellow()
            }
        );
    }
}

fn read_version(repo_path: &Path) -> anyhow::Result<String> {
    let version_path = repo_path.join("VERSION");
    if !version_path.exists() {
        anyhow::bail!("VERSION file not found at {}", version_path.display());
    }
    let version = std::fs::read_to_string(&version_path)?.trim().to_string();
    if version.is_empty() {
        anyhow::bail!("VERSION file is empty");
    }
    Ok(version)
}

fn resolve_targets(defaults: &[String], override_targets: Option<Vec<String>>) -> Vec<String> {
    override_targets
        .map(|v| if v.is_empty() { defaults.to_vec() } else { v })
        .unwrap_or_else(|| defaults.to_vec())
}

fn resolve_channels(
    channels: &crate::config::loader::ShipChannelsConfig,
    override_channels: Option<Vec<String>>,
) -> Vec<String> {
    if let Some(overrides) = override_channels {
        if !overrides.is_empty() {
            return overrides;
        }
    }

    let mut out: Vec<String> = Vec::new();
    if channels.binaries {
        out.push("binaries".to_string());
    }
    if channels.installers.shell {
        out.push("shell-installer".to_string());
    }
    if channels.installers.tauri {
        out.push("tauri".to_string());
    }
    for pm in &channels.package_managers {
        out.push(format!("package-manager:{}", pm.kind));
    }
    for store in &channels.app_stores {
        out.push(format!("app-store:{}", store.kind));
    }
    out
}

fn check_qualification(config: &Config) -> anyhow::Result<()> {
    let stability = crate::stability::engine::load(&config.repo_path).unwrap_or_default();
    let index = load_index(&config.repo_path);
    let last_release_ts = index.releases.last().map(|e| e.released_at);

    let result = evaluate(
        &stability,
        &config.qualification,
        true,
        true,
        0.0,
        last_release_ts,
    );

    match result {
        QualificationResult::Qualified => Ok(()),
        QualificationResult::Rejected(reason) => {
            anyhow::bail!(
                "qualification check failed: {}. Pass --force to ship anyway.",
                reason
            )
        }
    }
}

async fn build_target(
    repo_path: &Path,
    triple: &str,
    build_config: &BuildConfig,
) -> anyhow::Result<BuiltTarget> {
    let host_triple = host_target_triple().await?;
    let target_dir = repo_path.join("target").join(triple).join("release");

    if triple == host_triple {
        // Use the existing build orchestration for the host target.
        let status = crate::release::builder::run(build_config, repo_path).await;
        if !status.passed() {
            let msg = match status {
                crate::release::builder::BuildStatus::Failed { stderr, .. } => stderr,
                crate::release::builder::BuildStatus::Skipped => {
                    "build skipped; no [build] command configured".to_string()
                }
                _ => "build failed".to_string(),
            };
            anyhow::bail!("host build failed: {}", msg);
        }

        let artifact_base = Path::new(&build_config.artifact_path)
            .parent()
            .unwrap_or(Path::new("target/release"));
        return Ok(BuiltTarget {
            triple: triple.to_string(),
            kaptaind: artifact_base.join("kaptaind"),
            kaptaind_cli: artifact_base.join("kaptaind-cli"),
        });
    }

    // Cross-compilation: make sure the target is installed.
    let installed = list_installed_targets().await?;
    if !installed.contains(&triple.to_string()) {
        let _ = Command::new("rustup")
            .args(["target", "add", triple])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--release")
        .arg("--target")
        .arg(triple)
        .current_dir(repo_path)
        .env("CARGO_TARGET_DIR", repo_path.join("target"));

    let output = cmd
        .output()
        .await
        .context(format!("failed to spawn cargo build for {}", triple))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo build for {} failed: {}", triple, stderr);
    }

    Ok(BuiltTarget {
        triple: triple.to_string(),
        kaptaind: target_dir.join(if triple.contains("windows") {
            "kaptaind.exe"
        } else {
            "kaptaind"
        }),
        kaptaind_cli: target_dir.join(if triple.contains("windows") {
            "kaptaind-cli.exe"
        } else {
            "kaptaind-cli"
        }),
    })
}

async fn host_target_triple() -> anyhow::Result<String> {
    let output = Command::new("rustc")
        .args(["-vV"])
        .output()
        .await
        .context("failed to run rustc -vV")?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("host: ") {
            return Ok(value.trim().to_string());
        }
    }
    anyhow::bail!("could not determine host target triple")
}

async fn list_installed_targets() -> anyhow::Result<Vec<String>> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .await
        .context("failed to run rustup target list")?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

async fn build_shell_installer_bundle(
    repo_path: &Path,
    installer_dir: &Path,
    bundle_path: &Path,
    version: &str,
    targets: &[BuiltTarget],
) -> anyhow::Result<()> {
    let install_sh_src = repo_path.join("install.sh");
    if !install_sh_src.exists() {
        anyhow::bail!("install.sh not found at repo root");
    }

    // Stage files into a temp directory so the tarball has a clean prefix.
    let stage = installer_dir.join("stage");
    std::fs::create_dir_all(&stage)?;
    std::fs::copy(&install_sh_src, stage.join("install.sh"))?;

    let bin_dir = stage.join("binaries").join(version);
    std::fs::create_dir_all(&bin_dir)?;

    for bt in targets {
        let target_bin = bin_dir.join(&bt.triple);
        std::fs::create_dir_all(&target_bin)?;
        if bt.kaptaind.exists() {
            std::fs::copy(
                &bt.kaptaind,
                target_bin.join(bt.kaptaind.file_name().unwrap()),
            )?;
        }
        if bt.kaptaind_cli.exists() {
            std::fs::copy(
                &bt.kaptaind_cli,
                target_bin.join(bt.kaptaind_cli.file_name().unwrap()),
            )?;
        }
    }

    // Create tarball
    let tarball_file = std::fs::File::create(bundle_path)?;
    let enc = flate2::write::GzEncoder::new(tarball_file, flate2::Compression::best());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(format!("kaptaind-{}-installer", version), &stage)?;
    tar.finish()?;

    Ok(())
}

async fn build_tauri_bundles(tauri_dir: &Path, ship_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !tauri_dir.join("package.json").exists() {
        anyhow::bail!("package.json not found in {}", tauri_dir.display());
    }

    // npm ci is best-effort; fall back to npm install.
    let mut npm_cmd = Command::new("npm");
    npm_cmd.arg("ci").current_dir(tauri_dir);
    let npm_out = npm_cmd.output().await.context("failed to run npm ci")?;
    if !npm_out.status.success() {
        let mut fallback = Command::new("npm");
        fallback.arg("install").current_dir(tauri_dir);
        let fallback_out = fallback
            .output()
            .await
            .context("failed to run npm install")?;
        if !fallback_out.status.success() {
            anyhow::bail!(
                "npm install failed: {}",
                String::from_utf8_lossy(&fallback_out.stderr)
            );
        }
    }

    let mut build_cmd = Command::new("npm");
    build_cmd
        .args(["run", "tauri", "build"])
        .current_dir(tauri_dir)
        .env("CI", "true");
    let output = build_cmd
        .output()
        .await
        .context("failed to run npm run tauri build")?;
    if !output.status.success() {
        anyhow::bail!(
            "tauri build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let bundle_dir = tauri_dir
        .join("src-tauri")
        .join("target")
        .join("release")
        .join("bundle");
    if !bundle_dir.exists() {
        anyhow::bail!("tauri bundle directory not found");
    }

    let mut artifacts: Vec<PathBuf> = Vec::new();
    collect_bundle_artifacts(&bundle_dir, ship_dir, &mut artifacts)?;

    Ok(artifacts)
}

fn collect_bundle_artifacts(
    bundle_dir: &Path,
    ship_dir: &Path,
    artifacts: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(bundle_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_bundle_artifacts(&path, ship_dir, artifacts)?;
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(ext, "deb" | "rpm" | "dmg" | "msi" | "AppImage") {
                let dest = ship_dir.join(path.file_name().unwrap());
                std::fs::copy(&path, &dest)?;
                artifacts.push(dest);
            }
        }
    }
    Ok(())
}

async fn generate_homebrew_formula(
    _repo_path: &Path,
    formula_path: &Path,
    version: &str,
    targets: &[BuiltTarget],
) -> anyhow::Result<()> {
    // Prefer the host target tarball for the formula URL.
    let host = host_target_triple().await?;
    let host_target = targets
        .iter()
        .find(|bt| bt.triple == host)
        .or_else(|| targets.first())
        .ok_or_else(|| anyhow!("no targets available for Homebrew formula"))?;

    let tarball = host_target
        .kaptaind
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("kaptaind-{}.tar.gz", version));

    let sha256 = if tarball.exists() {
        let bytes = std::fs::read(&tarball)?;
        crate::util::hex::encode(Sha256::digest(&bytes))
    } else {
        "REPLACE_WITH_ACTUAL_SHA256".to_string()
    };

    // URL is a placeholder; users should replace it with their real download URL.
    let url = format!(
        "https://github.com/elci-group/kaptaind/releases/download/v{}/kaptaind-{}-{}.tar.gz",
        version, host_target.triple, version
    );

    let formula = format!(
        r#"class Kaptaind < Formula
  desc "Automated semantic-versioning daemon"
  homepage "https://github.com/elci-group/kaptaind"
  url "{url}"
  sha256 "{sha256}"
  version "{version}"
  license "MIT"

  def install
    bin.install "kaptaind"
    bin.install "kaptaind-cli"
  end

  service do
    run [opt_bin/"kaptaind", "--daemon"]
    keep_alive true
    log_path var/"log/kaptaind.log"
    error_log_path var/"log/kaptaind.error.log"
  end

  test do
    system "{{}}", "--version"
  end
end
"#,
        url = url,
        sha256 = sha256,
        version = version
    );

    std::fs::write(formula_path, formula)?;
    Ok(())
}

async fn publish_github_release(
    version: &str,
    artifacts: &[PathBuf],
    draft: bool,
    prerelease: bool,
    notes: Option<&str>,
) -> anyhow::Result<()> {
    let tag = format!("v{}", version);

    let mut args = vec!["release", "create", &tag];
    if draft {
        args.push("--draft");
    }
    if prerelease {
        args.push("--prerelease");
    }
    args.push("--title");
    args.push(&tag);
    if let Some(n) = notes {
        args.push("--notes");
        args.push(n);
    } else {
        args.push("--generate-notes");
    }
    for art in artifacts {
        args.push(
            art.to_str()
                .ok_or_else(|| anyhow!("invalid artifact path"))?,
        );
    }

    let output = Command::new("gh")
        .args(&args)
        .output()
        .await
        .context("failed to run gh release create. Is gh installed and authenticated?")?;

    if !output.status.success() {
        anyhow::bail!(
            "gh release create failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn write_last_ship(
    repo_path: &Path,
    kind: &str,
    version: &str,
    channels: &[String],
    artifacts: &[PathBuf],
) -> anyhow::Result<()> {
    let ship_dir = repo_path.join(".kaptaind").join("ship");
    std::fs::create_dir_all(&ship_dir)?;
    let summary = serde_json::json!({
        "kind": kind,
        "version": version,
        "shipped_at": chrono::Utc::now().timestamp(),
        "channels": channels,
        "artifacts": artifacts.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
    });
    std::fs::write(
        ship_dir.join("last.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    Ok(())
}

fn kind_config(ship: &ShipConfig, kind: ShipKind) -> &ShipKindConfig {
    match kind {
        ShipKind::Stable => &ship.stable,
        ShipKind::Nightly => &ship.nightly,
        ShipKind::Manual => {
            // Manual runs do not use kind-specific overrides.
            static DEFAULT: std::sync::OnceLock<ShipKindConfig> = std::sync::OnceLock::new();
            DEFAULT.get_or_init(ShipKindConfig::default)
        }
    }
}

fn signing_enabled(ship: &ShipConfig, kind: ShipKind) -> bool {
    kind_config(ship, kind).sign.unwrap_or(ship.sign)
}

fn generate_checksum(artifact: &Path) -> anyhow::Result<PathBuf> {
    let bytes = std::fs::read(artifact)?;
    let hash = crate::util::hex::encode(Sha256::digest(&bytes));
    let filename = artifact
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("artifact");
    let mut checksum_path = artifact.as_os_str().to_owned();
    checksum_path.push(".sha256");
    let checksum = format!("{}  {}\n", hash, filename);
    std::fs::write(&checksum_path, checksum)?;
    Ok(PathBuf::from(checksum_path))
}

async fn gpg_sign_checksum(checksum_path: &Path, key_id: Option<&str>) -> anyhow::Result<PathBuf> {
    let mut args = vec![
        "--batch".to_string(),
        "--yes".to_string(),
        "--detach-sign".to_string(),
        "--armor".to_string(),
    ];
    if let Some(k) = key_id {
        args.push("--local-user".to_string());
        args.push(k.to_string());
    }
    args.push(checksum_path.to_str().unwrap_or("").to_string());

    let output = Command::new("gpg")
        .args(&args)
        .output()
        .await
        .context("failed to run gpg --detach-sign")?;
    if !output.status.success() {
        anyhow::bail!(
            "gpg --detach-sign failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut sig_path = checksum_path.as_os_str().to_owned();
    sig_path.push(".asc");
    Ok(PathBuf::from(sig_path))
}

fn resolve_kind_channels(
    ship: &ShipConfig,
    override_channels: Option<Vec<String>>,
    kind: ShipKind,
) -> Vec<String> {
    if let Some(overrides) = override_channels {
        if !overrides.is_empty() {
            return overrides;
        }
    }
    let kind_cfg = kind_config(ship, kind);
    if let Some(ref overrides) = kind_cfg.channels {
        if !overrides.is_empty() {
            return overrides.clone();
        }
    }
    resolve_channels(&ship.channels, None)
}

/// Build a nightly prerelease version from a base version and the current
/// git short commit, e.g. `0.1.2-nightly.20260707.abc1234`.
fn nightly_version(repo_path: &Path, base: &str) -> anyhow::Result<String> {
    let parsed = semver::Version::parse(base.trim())
        .with_context(|| format!("failed to parse base version '{}'", base))?;
    let core = format!("{}.{}.{}", parsed.major, parsed.minor, parsed.patch);
    let date = chrono::Utc::now().format("%Y%m%d").to_string();
    let short_commit = git_short_commit(repo_path).unwrap_or_else(|_| "unknown".to_string());
    Ok(format!("{}-nightly.{}.{}", core, date, short_commit))
}

fn git_short_commit(repo_path: &Path) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap_or("."),
            "rev-parse",
            "--short",
            "HEAD",
        ])
        .output()
        .context("failed to run git rev-parse")?;
    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn create_git_tag(
    repo_path: &Path,
    version: &str,
    kind: ShipKind,
    sign: bool,
    key_id: Option<&str>,
) -> anyhow::Result<()> {
    let tag = format!("v{}", version);
    let message = format!("kaptaind {} release {}", kind.as_str(), version);
    let mut args: Vec<String> = vec![
        "-C".into(),
        repo_path.to_str().unwrap_or(".").into(),
        "tag".into(),
    ];
    if sign {
        args.push("-s".into());
    } else {
        args.push("-a".into());
    }
    if let Some(k) = key_id {
        args.push("-u".into());
        args.push(k.into());
    }
    args.push(tag);
    args.push("-m".into());
    args.push(message);
    let output = Command::new("git")
        .args(&args)
        .output()
        .await
        .context("failed to run git tag")?;
    if !output.status.success() {
        anyhow::bail!(
            "git tag failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

async fn push_git_tag(repo_path: &Path, remote: &str, version: &str) -> anyhow::Result<()> {
    let tag = format!("v{}", version);
    let output = Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap_or("."),
            "push",
            remote,
            &tag,
        ])
        .output()
        .await
        .context("failed to run git push tag")?;
    if !output.status.success() {
        anyhow::bail!(
            "git push tag failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Look for an existing nightly release that matches the current base version,
/// today's date, and the current commit. Returns the existing version string if
/// found.
fn find_existing_nightly(repo_path: &Path, base: &str, commit: &str) -> Option<String> {
    let parsed = semver::Version::parse(base.trim()).ok()?;
    let core = format!("{}.{}.{}", parsed.major, parsed.minor, parsed.patch);
    let date = chrono::Utc::now().format("%Y%m%d").to_string();
    let prefix = format!("{}-nightly.{}", core, date);
    let index = load_ship_index(repo_path);
    index
        .ships
        .iter()
        .filter(|e| e.kind == "nightly")
        .find(|e| e.version.starts_with(&prefix) && e.version.ends_with(&format!(".{}", commit)))
        .map(|e| e.version.clone())
}

/// Prune old nightly releases, keeping only the most recent `retain_count`.
fn prune_nightlies(repo_path: &Path, retain_count: Option<usize>) -> anyhow::Result<()> {
    let Some(limit) = retain_count else {
        return Ok(());
    };
    if limit == 0 {
        return Ok(());
    }

    let mut index = load_ship_index(repo_path);
    let mut nightlies: Vec<_> = index
        .ships
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "nightly")
        .collect();
    nightlies.sort_by_key(|(_, e)| std::cmp::Reverse(e.shipped_at));

    if nightlies.len() <= limit {
        return Ok(());
    }

    let to_remove: std::collections::HashSet<_> = nightlies
        .into_iter()
        .skip(limit)
        .map(|(idx, _)| idx)
        .collect();

    for (idx, entry) in index.ships.iter().enumerate() {
        if to_remove.contains(&idx) {
            let ship_dir = repo_path
                .join(".kaptaind")
                .join("ship")
                .join(&entry.version);
            if ship_dir.exists() {
                let _ = std::fs::remove_dir_all(&ship_dir);
            }
            let tag = format!("v{}", entry.version);
            let _ = std::process::Command::new("git")
                .args(["-C", repo_path.to_str().unwrap_or("."), "tag", "-d", &tag])
                .output();
        }
    }

    index.ships = index
        .ships
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| !to_remove.contains(idx))
        .map(|(_, entry)| entry)
        .collect();

    let ship_dir = repo_path.join(".kaptaind").join("ship");
    let _ = std::fs::create_dir_all(&ship_dir);
    if let Ok(content) = serde_json::to_string_pretty(&index) {
        let _ = crate::release::index::write_atomic(&ship_dir.join("index.json"), &content);
    }

    Ok(())
}

/// Generate release notes for a stable or nightly release.
fn generate_release_notes(
    repo_path: &Path,
    version: &str,
    kind: ShipKind,
    commit: &str,
) -> Option<String> {
    match kind {
        ShipKind::Nightly => {
            let date = chrono::Utc::now()
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string();
            Some(format!(
                "Automated nightly build.\n\nVersion: {}\nCommit: {}\nBuilt: {}",
                version, commit, date
            ))
        }
        ShipKind::Stable => Some(generate_stable_release_notes(repo_path, version)),
        ShipKind::Manual => None,
    }
}

fn generate_stable_release_notes(repo_path: &Path, version: &str) -> String {
    let index = load_ship_index(repo_path);
    let previous = index
        .ships
        .iter()
        .filter(|e| e.kind == "stable")
        .map(|e| e.version.as_str())
        .next_back();

    let range = previous.map(|prev| format!("v{}..HEAD", prev));
    let mut cmd = std::process::Command::new("git");
    cmd.args([
        "-C",
        repo_path.to_str().unwrap_or("."),
        "log",
        "--pretty=format:- %s",
    ]);
    if let Some(ref r) = range {
        cmd.arg(r);
    } else {
        cmd.arg("-n").arg("20");
    }

    let output = cmd.output();
    let commits = output
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let header = format!("Release v{}\n", version);
    let body = if commits.is_empty() {
        "No commit history available.".to_string()
    } else {
        format!("## Changes\n{}", commits)
    };
    format!("{}{}", header, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::{
        ShipAppStoreConfig, ShipChannelsConfig, ShipConfig, ShipInstallersConfig,
        ShipPackageManagerConfig,
    };
    use crate::release::index::{ShipIndex, ShipIndexEntry};

    #[test]
    fn resolve_targets_uses_defaults_when_empty() {
        let defaults = vec!["x86_64-unknown-linux-gnu".to_string()];
        assert_eq!(resolve_targets(&defaults, None), defaults);
        assert_eq!(resolve_targets(&defaults, Some(vec![])), defaults);
    }

    #[test]
    fn resolve_targets_uses_override() {
        let defaults = vec!["x86_64-unknown-linux-gnu".to_string()];
        let overrides = vec!["aarch64-apple-darwin".to_string()];
        assert_eq!(
            resolve_targets(&defaults, Some(overrides.clone())),
            overrides
        );
    }

    #[test]
    fn resolve_channels_collects_enabled_channels() {
        let channels = ShipChannelsConfig {
            binaries: true,
            installers: ShipInstallersConfig {
                shell: true,
                tauri: false,
            },
            package_managers: vec![ShipPackageManagerConfig {
                kind: "homebrew".to_string(),
                tap: None,
                formula_name: "kaptaind".to_string(),
                token_env: None,
            }],
            app_stores: vec![ShipAppStoreConfig {
                kind: "github-releases".to_string(),
                draft: false,
                prerelease: false,
                token_env: None,
            }],
        };
        let out = resolve_channels(&channels, None);
        assert!(out.contains(&"binaries".to_string()));
        assert!(out.contains(&"shell-installer".to_string()));
        assert!(out.contains(&"package-manager:homebrew".to_string()));
        assert!(out.contains(&"app-store:github-releases".to_string()));
        assert!(!out.contains(&"tauri".to_string()));
    }

    #[test]
    fn resolve_kind_channels_uses_kind_specific_overrides() {
        let mut ship = ShipConfig::default();
        ship.channels.binaries = false;
        ship.nightly.channels = Some(vec!["github-releases".to_string()]);
        let out = resolve_kind_channels(&ship, None, ShipKind::Nightly);
        assert_eq!(out, vec!["github-releases".to_string()]);
    }

    #[test]
    fn resolve_kind_channels_falls_back_to_top_level() {
        let mut ship = ShipConfig::default();
        ship.channels.binaries = true;
        let out = resolve_kind_channels(&ship, None, ShipKind::Stable);
        assert!(out.contains(&"binaries".to_string()));
    }

    #[test]
    fn kind_config_returns_appropriate_config() {
        let mut ship = ShipConfig::default();
        ship.stable.push_tag = true;
        ship.nightly.prerelease = true;
        assert!(kind_config(&ship, ShipKind::Stable).push_tag);
        assert!(kind_config(&ship, ShipKind::Nightly).prerelease);
        assert!(!kind_config(&ship, ShipKind::Manual).push_tag);
    }

    fn temp_git_repo() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("file.txt"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();
        let commit = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .current_dir(path)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        (dir, commit)
    }

    #[test]
    fn nightly_version_uses_core_version() {
        let (dir, commit) = temp_git_repo();
        let version = nightly_version(dir.path(), "1.2.3").unwrap();
        assert!(version.starts_with("1.2.3-nightly."));
        assert!(version.ends_with(&format!(".{}", commit)));
    }

    #[test]
    fn nightly_version_strips_existing_prerelease() {
        let (dir, _) = temp_git_repo();
        let version = nightly_version(dir.path(), "1.2.3-alpha.1").unwrap();
        assert!(version.starts_with("1.2.3-nightly."));
        assert!(!version.contains("alpha"));
    }

    #[test]
    fn nightly_version_falls_back_on_missing_git() {
        let dir = tempfile::tempdir().unwrap();
        let version = nightly_version(dir.path(), "0.1.0").unwrap();
        assert!(version.starts_with("0.1.0-nightly."));
        assert!(version.ends_with(".unknown"));
    }

    fn write_ship_index(path: &std::path::Path, ships: Vec<ShipIndexEntry>) {
        let index = ShipIndex { ships };
        let dir = path.join(".kaptaind").join("ship");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("index.json"),
            serde_json::to_string(&index).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn find_existing_nightly_detects_same_date_and_commit() {
        let (dir, commit) = temp_git_repo();
        let base = "1.2.3";
        let date = chrono::Utc::now().format("%Y%m%d").to_string();
        let version = format!("{}-nightly.{}.{}", base, date, commit);
        write_ship_index(
            dir.path(),
            vec![ShipIndexEntry {
                kind: "nightly".to_string(),
                version: version.clone(),
                shipped_at: 0,
                targets: vec![],
                channels: vec![],
                artifacts: vec![],
            }],
        );
        assert_eq!(
            find_existing_nightly(dir.path(), base, &commit),
            Some(version)
        );
    }

    #[test]
    fn prune_nightlies_retains_most_recent() {
        let (dir, _) = temp_git_repo();
        let ships: Vec<_> = (0..5)
            .map(|i| ShipIndexEntry {
                kind: "nightly".to_string(),
                version: format!("1.2.3-nightly.2026070{}.abc{}", i, i),
                shipped_at: i as i64,
                targets: vec![],
                channels: vec![],
                artifacts: vec![],
            })
            .collect();
        write_ship_index(dir.path(), ships);
        prune_nightlies(dir.path(), Some(2)).unwrap();
        let remaining = load_ship_index(dir.path()).ships;
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().any(|e| e.version.contains("abc3")));
        assert!(remaining.iter().any(|e| e.version.contains("abc4")));
    }

    #[test]
    fn prune_nightlies_ignores_non_nightly_entries() {
        let (dir, _) = temp_git_repo();
        let ships = vec![
            ShipIndexEntry {
                kind: "stable".to_string(),
                version: "1.0.0".to_string(),
                shipped_at: 0,
                targets: vec![],
                channels: vec![],
                artifacts: vec![],
            },
            ShipIndexEntry {
                kind: "nightly".to_string(),
                version: "1.2.3-nightly.20260707.abc".to_string(),
                shipped_at: 1,
                targets: vec![],
                channels: vec![],
                artifacts: vec![],
            },
        ];
        write_ship_index(dir.path(), ships);
        prune_nightlies(dir.path(), Some(1)).unwrap();
        let remaining = load_ship_index(dir.path()).ships;
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn generate_release_notes_for_nightly_contains_version_and_commit() {
        let notes = generate_release_notes(
            std::path::Path::new("."),
            "1.2.3-nightly.20260707.abc",
            ShipKind::Nightly,
            "abc",
        )
        .unwrap();
        assert!(notes.contains("1.2.3-nightly.20260707.abc"));
        assert!(notes.contains("abc"));
    }

    #[test]
    fn generate_release_notes_for_stable_includes_header() {
        let (dir, _) = temp_git_repo();
        let notes = generate_release_notes(dir.path(), "1.2.3", ShipKind::Stable, "abc").unwrap();
        assert!(notes.starts_with("Release v1.2.3"));
    }

    #[test]
    fn generate_checksum_produces_valid_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = dir.path().join("artifact.tar.gz");
        std::fs::write(&artifact, b"hello ship").unwrap();

        let checksum_path = generate_checksum(&artifact).unwrap();

        let mut expected_path = artifact.as_os_str().to_owned();
        expected_path.push(".sha256");
        assert_eq!(checksum_path.as_os_str(), expected_path);

        let content = std::fs::read_to_string(&checksum_path).unwrap();
        let (hash, filename) = content.trim().split_once("  ").unwrap();
        assert_eq!(filename, "artifact.tar.gz");
        let expected = crate::util::hex::encode(Sha256::digest(b"hello ship"));
        assert_eq!(hash, expected);
    }

    #[test]
    fn signing_enabled_respects_kind_overrides() {
        let mut ship = ShipConfig {
            sign: true,
            ..ShipConfig::default()
        };
        ship.stable.sign = Some(false);
        ship.nightly.sign = None;
        assert!(!signing_enabled(&ship, ShipKind::Stable));
        assert!(signing_enabled(&ship, ShipKind::Nightly));
        assert!(signing_enabled(&ship, ShipKind::Manual));
    }
}
