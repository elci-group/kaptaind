//! `kaptaind-cli doctor` — host profile capture for qualification (Workstream E3).
//!
//! Collects a best-effort hardware/OS profile, checks inotify limits against
//! the repo-size tier table, verifies tool availability, recommends a tier,
//! and writes a machine-readable artifact under `.kaptaind/doctor/`.
//!
//! Also runs the v10.0.0 config-migration checks (safety plan D3): the loaded
//! `Config`, the raw `kaptaind.toml`, and the project's `.kaptainignore` are
//! inspected for legacy patterns the v10 breaking window retires, and every
//! finding is reported with a concrete fix.

use chrono::Utc;
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::table::print_table;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolVersion {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InotifyLimits {
    pub max_user_watches: Option<u64>,
    pub max_user_instances: Option<u64>,
    pub max_queued_events: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub schema: &'static str,
    pub generated_at: String,
    pub os: String,
    pub arch: String,
    pub kernel: Option<String>,
    pub cpu_model: Option<String>,
    pub logical_cores: usize,
    pub ram_total_bytes: Option<u64>,
    pub ram_available_bytes: Option<u64>,
    pub disk_type: String,
    pub inotify: InotifyLimits,
    pub tools: Vec<ToolVersion>,
    pub repo_path: String,
    pub repo_file_count: usize,
    pub recommended_tier: String,
    pub recommended_watches: u64,
    pub warnings: Vec<String>,
    pub git_rev: Option<String>,
    pub dirty: Option<bool>,
    /// v10.0.0 config-migration findings (safety plan D3). Added in v10.0.0;
    /// older readers ignore unknown fields per the compatibility contract.
    pub migration: Vec<MigrationFinding>,
}

/// Severity of a config-migration finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationSeverity {
    /// Informational: a default flipped and the user never chose explicitly.
    Info,
    /// The legacy pattern is risky or now actively harmful.
    Warn,
}

/// One legacy-pattern finding from the v10.0.0 migration checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFinding {
    /// Stable machine-readable check identifier.
    pub check: String,
    pub severity: MigrationSeverity,
    pub message: String,
    /// The concrete remediation for this finding.
    pub fix: String,
}

pub fn handle_doctor(config: &Config, format: &str) -> anyhow::Result<()> {
    let report = collect(config);
    write_artifact(config, &report)?;

    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }
    Ok(())
}

fn collect(config: &Config) -> DoctorReport {
    let repo = &config.repo_path;
    let inotify = read_inotify();
    let file_count = count_files(repo);
    let tier = tier_for(file_count);
    let recommended_watches = recommended_watches_for(tier);

    let mut warnings = Vec::new();
    if let Some(watches) = inotify.max_user_watches {
        if watches < recommended_watches {
            warnings.push(format!(
                "fs.inotify.max_user_watches={watches} is below the recommended {recommended_watches} for tier {tier}; \
                 raise it (e.g. `sudo sysctl fs.inotify.max_user_watches={recommended_watches}`) to avoid missed events."
            ));
        }
    } else {
        warnings.push(
            "could not read fs.inotify.max_user_watches (non-Linux host?) — watcher limits unverified."
                .to_string(),
        );
    }

    let tools = vec![
        tool_version("git", &["--version"]),
        tool_version("rustc", &["--version"]),
        tool_version("cargo", &["--version"]),
        tool_version("docker", &["--version"]),
    ];
    if !tools[0].available {
        warnings.push("git not found on PATH; kaptaind requires git.".to_string());
    }

    let (git_rev, dirty) = git_state(repo);

    DoctorReport {
        schema: "kaptaind.doctor.v1",
        generated_at: Utc::now().to_rfc3339(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        kernel: read_kernel(),
        cpu_model: read_cpu_model(),
        logical_cores: read_cores(),
        ram_total_bytes: read_meminfo("MemTotal:").map(|kb| kb * 1024),
        ram_available_bytes: read_meminfo("MemAvailable:").map(|kb| kb * 1024),
        disk_type: read_disk_type(repo),
        inotify,
        tools,
        repo_path: repo.display().to_string(),
        repo_file_count: file_count,
        recommended_tier: tier.to_string(),
        recommended_watches,
        warnings,
        git_rev,
        dirty,
        migration: collect_migration_findings(config),
    }
}

/// `.kaptainignore` entries that were pre-v9.7.17 workarounds for the daemon's
/// own writeback churn. Obsolete since the self-write guard (v9.7.17) and now
/// harmful: a lone dependency edit to `Cargo.toml`/`Cargo.lock` (e.g.
/// `cargo update`) never clusters and therefore never commits.
const OBSOLETE_IGNORE_ENTRIES: &[&str] = &["VERSION", "Cargo.toml", "Cargo.lock"];

/// Read the raw `kaptaind.toml` and the ignore file and run every migration
/// check. File reads are best-effort: a missing file means there is nothing
/// to migrate for that check.
fn collect_migration_findings(config: &Config) -> Vec<MigrationFinding> {
    let toml_path = config.repo_path.join("kaptaind.toml");
    let toml_text = std::fs::read_to_string(toml_path).ok();

    let ignore_path = if config.watch.ignore_file.is_absolute() {
        config.watch.ignore_file.clone()
    } else {
        config.repo_path.join(&config.watch.ignore_file)
    };
    let ignore_text = std::fs::read_to_string(ignore_path).ok();

    detect_migration_findings(config, toml_text.as_deref(), ignore_text.as_deref())
}

/// Pure detection over the raw config TOML, the ignore-file text, and the
/// loaded config. Split out from the file I/O so it is unit-testable.
pub fn detect_migration_findings(
    _config: &Config,
    toml_text: Option<&str>,
    ignore_text: Option<&str>,
) -> Vec<MigrationFinding> {
    let mut findings = Vec::new();
    findings.extend(detect_staging_mode(toml_text));
    findings.extend(detect_obsolete_ignore_entries(ignore_text));
    findings.extend(detect_require_bump(toml_text));
    findings
}

/// Parse the raw TOML once; a file that does not parse fails `kaptaind
/// validate` elsewhere, so migration checks simply skip it.
fn parse_toml(toml_text: Option<&str>) -> Option<toml::Table> {
    toml_text?.parse::<toml::Table>().ok()
}

/// D3: `staging.mode` — default flipped `all` → `cluster` in v9.7.17.
fn detect_staging_mode(toml_text: Option<&str>) -> Vec<MigrationFinding> {
    let Some(table) = parse_toml(toml_text) else {
        return Vec::new();
    };
    let mode = table
        .get("staging")
        .and_then(|s| s.get("mode"))
        .and_then(|m| m.as_str());
    match mode {
        Some("all") => vec![MigrationFinding {
            check: "staging_mode_all".to_string(),
            severity: MigrationSeverity::Warn,
            message: "`staging.mode = \"all\"` is set explicitly. It runs `git add -A` \
                      across the whole worktree, sweeping in untracked files — including \
                      secrets. The default flipped to \"cluster\" in v9.7.17."
                .to_string(),
            fix: "Remove the key or set `staging.mode = \"cluster\"` so only clustered \
                  paths (plus version metadata) are staged."
                .to_string(),
        }],
        // Explicit "cluster"/"pattern": the user chose deliberately.
        Some(_) => Vec::new(),
        None => vec![MigrationFinding {
            check: "staging_mode_unset".to_string(),
            severity: MigrationSeverity::Info,
            message: "`staging.mode` is not set in kaptaind.toml, so the v9.7.17+ default \
                      \"cluster\" applies (previously \"all\"). This is safe; you just \
                      never pinned the choice."
                .to_string(),
            fix: "Optional: add `[staging] mode = \"cluster\"` to make the choice explicit."
                .to_string(),
        }],
    }
}

/// D3: obsolete `.kaptainignore` workarounds for version metadata files.
fn detect_obsolete_ignore_entries(ignore_text: Option<&str>) -> Vec<MigrationFinding> {
    let Some(text) = ignore_text else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .filter(|l| {
            let entry = l.trim_start_matches('/');
            let basename = entry.rsplit('/').next().unwrap_or(entry);
            OBSOLETE_IGNORE_ENTRIES.contains(&basename)
        })
        .map(|entry| MigrationFinding {
            check: "obsolete_kaptainignore_entry".to_string(),
            severity: MigrationSeverity::Warn,
            message: format!(
                "`.kaptainignore` entry `{entry}` is an obsolete workaround. Since \
                 v9.7.17 the daemon suppresses its own version writebacks (self-write \
                 guard), so ignoring version metadata is no longer needed — and now \
                 harms you: a lone edit to that file (e.g. `cargo update` touching \
                 Cargo.toml/Cargo.lock) never clusters and never commits."
            ),
            fix: format!("Delete the `{entry}` line from .kaptainignore."),
        })
        .collect()
}

/// D3: `[commit] require_bump` — default flipped `true` → `false` in v10.0.0.
fn detect_require_bump(toml_text: Option<&str>) -> Vec<MigrationFinding> {
    let Some(table) = parse_toml(toml_text) else {
        return Vec::new();
    };
    let require_bump = table
        .get("commit")
        .and_then(|c| c.get("require_bump"))
        .and_then(|v| v.as_bool());
    match require_bump {
        None => vec![MigrationFinding {
            check: "require_bump_unset".to_string(),
            severity: MigrationSeverity::Info,
            message: "`commit.require_bump` is not set, so the v10.0.0 default `false` \
                      applies: below-threshold clusters are now captured as non-bumping \
                      `chore:` commits instead of being skipped (pre-v10 default was \
                      `true`)."
                .to_string(),
            fix: "Leave unset to adopt the new capture behavior, or set \
                  `[commit] require_bump = true` to keep the pre-v10 skip behavior."
                .to_string(),
        }],
        Some(true) => vec![MigrationFinding {
            check: "require_bump_true".to_string(),
            severity: MigrationSeverity::Info,
            message: "`commit.require_bump = true` keeps the pre-v10 behavior \
                      intentionally: below-threshold clusters are logged as `no_bump` \
                      and left uncommitted."
                .to_string(),
            fix: "No action needed. Remove the key (or set `false`) to adopt the v10 \
                  `chore:`-commit capture behavior."
                .to_string(),
        }],
        // Explicit false: already on the v10 behavior.
        Some(false) => Vec::new(),
    }
}

pub fn tier_for(files: usize) -> &'static str {
    if files <= 500 {
        "T0"
    } else if files <= 5_000 {
        "T1"
    } else if files <= 50_000 {
        "T2"
    } else if files <= 250_000 {
        "T3"
    } else {
        "T4"
    }
}

fn recommended_watches_for(tier: &str) -> u64 {
    match tier {
        "T0" => 8_192,
        "T1" => 65_536,
        "T2" => 524_288,
        "T3" => 524_288,
        _ => 1_048_576,
    }
}

fn read_inotify() -> InotifyLimits {
    InotifyLimits {
        max_user_watches: read_proc_u64("/proc/sys/fs/inotify/max_user_watches"),
        max_user_instances: read_proc_u64("/proc/sys/fs/inotify/max_user_instances"),
        max_queued_events: read_proc_u64("/proc/sys/fs/inotify/max_queued_events"),
    }
}

fn read_proc_u64(path: &str) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_kernel() -> Option<String> {
    if let Ok(v) = std::fs::read_to_string("/proc/version") {
        return Some(v.trim().to_string());
    }
    Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_cpu_model() -> Option<String> {
    let content = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("model name") {
            if let Some((_, value)) = rest.split_once(':') {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn read_cores() -> usize {
    if let Ok(out) = Command::new("nproc").output() {
        if let Ok(n) = String::from_utf8_lossy(&out.stdout).trim().parse::<usize>() {
            return n;
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Read a `/proc/meminfo` entry (value in kB).
fn read_meminfo(key: &str) -> Option<u64> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let kb: u64 = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())?;
            return Some(kb);
        }
    }
    None
}

/// Best-effort disk type: locate the block device backing `repo` and read its
/// rotational flag. Returns "ssd", "hdd", "nvme", or "unknown".
fn read_disk_type(repo: &Path) -> String {
    let dev = match mount_source(repo) {
        Some(d) => d,
        None => return "unknown".to_string(),
    };
    let name = dev.rsplit('/').next().unwrap_or(&dev).to_string();
    // NVMe devices expose namespaces like `nvme0n1`; treat the prefix as nvme.
    if name.starts_with("nvme") {
        return "nvme".to_string();
    }
    // Strip a trailing partition number (e.g. sda1 -> sda, vda1 -> vda).
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let rot = format!("/sys/block/{base}/queue/rotational");
    match std::fs::read_to_string(&rot).ok().as_deref().map(str::trim) {
        Some("0") => "ssd".to_string(),
        Some("1") => "hdd".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Find the mount source device for `repo` from `/proc/mounts` (longest match).
fn mount_source(repo: &Path) -> Option<String> {
    let content = std::fs::read_to_string("/proc/mounts").ok()?;
    let canonical = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let mut best: Option<(usize, String)> = None;
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let (Some(src), Some(mount)) = (parts.next(), parts.next()) else {
            continue;
        };
        let mount = mount.replace("\\040", " ");
        if canonical.starts_with(&mount) {
            let len = mount.len();
            if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
                best = Some((len, src.to_string()));
            }
        }
    }
    best.map(|(_, s)| s)
}

fn tool_version(name: &str, args: &[&str]) -> ToolVersion {
    match Command::new(name).args(args).output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let line = text.lines().next().unwrap_or("").trim().to_string();
            ToolVersion {
                name: name.to_string(),
                available: true,
                version: Some(line).filter(|s| !s.is_empty()),
            }
        }
        _ => ToolVersion {
            name: name.to_string(),
            available: false,
            version: None,
        },
    }
}

fn git_state(repo: &Path) -> (Option<String>, Option<bool>) {
    let rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty());

    (rev, dirty)
}

/// Count regular files under `root`, skipping `.git`, `target`, `node_modules`.
fn count_files(root: &Path) -> usize {
    const SKIP: &[&str] = &[".git", "target", "node_modules"];
    let mut count = 0usize;
    let mut stack: VecDeque<PathBuf> = VecDeque::new();
    stack.push_back(root.to_path_buf());
    while let Some(dir) = stack.pop_front() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !SKIP.iter().any(|s| *s == name) {
                    stack.push_back(path);
                }
            } else if path.is_file() {
                count += 1;
            }
        }
    }
    count
}

fn write_artifact(config: &Config, report: &DoctorReport) -> anyhow::Result<()> {
    let dir = config.repo_path.join(".kaptaind").join("doctor");
    std::fs::create_dir_all(&dir)?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("{stamp}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(report)?)?;
    // Keep a stable pointer to the latest run for the report aggregator.
    let latest = dir.join("latest.json");
    let _ = std::fs::write(&latest, serde_json::to_string_pretty(report)?);
    Ok(())
}

fn print_human(r: &DoctorReport) {
    println!("{} {}", "🩺".blue(), "Kaptaind Doctor".bold().blue());
    println!("{}", "=================".blue());
    println!(
        "{} {} ({})",
        "Host:".bold().cyan(),
        format!("{} {}", r.os, r.arch).blue(),
        r.kernel.as_deref().unwrap_or("kernel unknown").dimmed()
    );
    println!(
        "{} {}",
        "CPU:".bold().cyan(),
        format!(
            "{} ({} logical cores)",
            r.cpu_model.as_deref().unwrap_or("unknown"),
            r.logical_cores
        )
        .blue()
    );
    println!(
        "{} total {} / available {}",
        "RAM:".bold().cyan(),
        human_bytes(r.ram_total_bytes).blue(),
        human_bytes(r.ram_available_bytes).blue()
    );
    println!("{} {}", "Disk:".bold().cyan(), r.disk_type.as_str().blue());

    let rows: Vec<Vec<String>> = vec![
        vec![
            "max_user_watches".to_string(),
            fmt_opt(r.inotify.max_user_watches),
        ],
        vec![
            "max_user_instances".to_string(),
            fmt_opt(r.inotify.max_user_instances),
        ],
        vec![
            "max_queued_events".to_string(),
            fmt_opt(r.inotify.max_queued_events),
        ],
    ];
    println!("\n{}", "inotify limits:".bold().cyan());
    print_table(&["Limit", "Value"], &rows);

    let tool_rows: Vec<Vec<String>> = r
        .tools
        .iter()
        .map(|t| {
            vec![
                t.name.clone(),
                if t.available {
                    t.version
                        .clone()
                        .unwrap_or_else(|| "?".to_string())
                        .green()
                        .to_string()
                } else {
                    "not found".red().to_string()
                },
            ]
        })
        .collect();
    println!("\n{}", "toolchain:".bold().cyan());
    print_table(&["Tool", "Version"], &tool_rows);

    println!(
        "\n{} {} files → tier {} (recommend max_user_watches ≥ {})",
        "Repo:".bold().cyan(),
        r.repo_file_count.to_string().blue(),
        r.recommended_tier.as_str().magenta(),
        r.recommended_watches.to_string().blue()
    );

    if r.warnings.is_empty() {
        println!("\n{} {}", "✅".green(), "No warnings.".green());
    } else {
        println!("\n{}", "⚠️  Warnings:".bold().yellow());
        for w in &r.warnings {
            println!("  {} {}", "•".yellow(), w);
        }
    }

    println!("\n{}", "🧭 Config migration (v10.0.0):".bold().cyan());
    if r.migration.is_empty() {
        println!(
            "  {} {}",
            "✅".green(),
            "No legacy patterns detected.".green()
        );
    } else {
        for f in &r.migration {
            let badge = match f.severity {
                MigrationSeverity::Info => "info".cyan().to_string(),
                MigrationSeverity::Warn => "WARN".yellow().to_string(),
            };
            println!("  {} {}", badge, f.message);
            println!("       {} {}", "fix:".dimmed(), f.fix.as_str().dimmed());
        }
    }

    println!(
        "\n{} {}",
        "Artifact:".dimmed(),
        config_relative(&r.repo_path).dimmed()
    );
}

fn human_bytes(bytes: Option<u64>) -> String {
    match bytes {
        None => "unknown".to_string(),
        Some(b) => {
            let gb = b as f64 / (1024.0 * 1024.0 * 1024.0);
            if gb >= 1.0 {
                format!("{gb:.1} GiB")
            } else {
                format!("{:.0} MiB", b as f64 / (1024.0 * 1024.0))
            }
        }
    }
}

fn fmt_opt(v: Option<u64>) -> String {
    v.map(|n| n.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn config_relative(repo: &str) -> String {
    format!("{repo}/.kaptaind/doctor/<timestamp>.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(toml_text: Option<&str>, ignore_text: Option<&str>) -> Vec<MigrationFinding> {
        detect_migration_findings(&Config::default(), toml_text, ignore_text)
    }

    fn check<'a>(findings: &'a [MigrationFinding], check: &str) -> Option<&'a MigrationFinding> {
        findings.iter().find(|f| f.check == check)
    }

    #[test]
    fn staging_mode_all_explicit_warns() {
        let fs = findings(Some("[staging]\nmode = \"all\"\n"), None);
        let f = check(&fs, "staging_mode_all").expect("expected staging_mode_all finding");
        assert_eq!(f.severity, MigrationSeverity::Warn);
        assert!(f.message.contains("git add -A"));
        assert!(f.fix.contains("cluster"));
    }

    #[test]
    fn staging_mode_missing_is_info() {
        let fs = findings(Some("[watch]\npath = \".\"\n"), None);
        let f = check(&fs, "staging_mode_unset").expect("expected staging_mode_unset finding");
        assert_eq!(f.severity, MigrationSeverity::Info);
        assert!(f.message.contains("v9.7.17"));
    }

    #[test]
    fn staging_mode_cluster_explicit_no_finding() {
        let fs = findings(Some("[staging]\nmode = \"cluster\"\n"), None);
        assert!(check(&fs, "staging_mode_all").is_none());
        assert!(check(&fs, "staging_mode_unset").is_none());
    }

    #[test]
    fn no_config_file_yields_no_toml_findings() {
        let fs = findings(None, None);
        assert!(fs.is_empty());
    }

    #[test]
    fn ignore_file_obsolete_entries_warn() {
        let ignore = "# comment\n\nVERSION\n/Cargo.toml\nsub/Cargo.lock\nsrc/**\n";
        let fs = findings(None, Some(ignore));
        let obsolete: Vec<_> = fs
            .iter()
            .filter(|f| f.check == "obsolete_kaptainignore_entry")
            .collect();
        assert_eq!(obsolete.len(), 3);
        assert!(obsolete
            .iter()
            .all(|f| f.severity == MigrationSeverity::Warn));
        assert!(obsolete.iter().any(|f| f.message.contains("`VERSION`")));
        assert!(obsolete.iter().any(|f| f.message.contains("`/Cargo.toml`")));
        assert!(obsolete
            .iter()
            .any(|f| f.message.contains("`sub/Cargo.lock`")));
        assert!(obsolete.iter().all(|f| f.fix.starts_with("Delete the")));
    }

    #[test]
    fn ignore_file_clean_and_negated_no_finding() {
        let fs = findings(None, Some("target/\n*.log\n!VERSION\n"));
        assert!(check(&fs, "obsolete_kaptainignore_entry").is_none());
    }

    #[test]
    fn require_bump_absent_is_info() {
        let fs = findings(Some("[commit]\nsign = true\n"), None);
        let f = check(&fs, "require_bump_unset").expect("expected require_bump_unset finding");
        assert_eq!(f.severity, MigrationSeverity::Info);
        assert!(f.message.contains("chore:"));
        assert!(f.fix.contains("require_bump = true"));
    }

    #[test]
    fn require_bump_true_is_info() {
        let fs = findings(Some("[commit]\nrequire_bump = true\n"), None);
        let f = check(&fs, "require_bump_true").expect("expected require_bump_true finding");
        assert_eq!(f.severity, MigrationSeverity::Info);
        assert!(f.message.contains("pre-v10"));
        assert!(check(&fs, "require_bump_unset").is_none());
    }

    #[test]
    fn require_bump_false_no_finding() {
        let fs = findings(Some("[commit]\nrequire_bump = false\n"), None);
        assert!(check(&fs, "require_bump_unset").is_none());
        assert!(check(&fs, "require_bump_true").is_none());
    }

    #[test]
    fn unparseable_toml_yields_no_findings() {
        let fs = findings(Some("this is [not valid toml"), None);
        assert!(fs.is_empty());
    }

    #[test]
    fn legacy_config_reports_every_retired_pattern() {
        let toml = "[staging]\nmode = \"all\"\n";
        let ignore = "VERSION\nCargo.toml\nCargo.lock\n";
        let fs = findings(Some(toml), Some(ignore));
        assert_eq!(
            check(&fs, "staging_mode_all").unwrap().severity,
            MigrationSeverity::Warn
        );
        assert_eq!(
            fs.iter()
                .filter(|f| f.check == "obsolete_kaptainignore_entry")
                .count(),
            3
        );
        assert!(check(&fs, "require_bump_unset").is_some());
    }
}
