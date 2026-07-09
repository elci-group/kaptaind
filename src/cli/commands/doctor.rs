//! `kaptaind-cli doctor` — host profile capture for qualification (Workstream E3).
//!
//! Collects a best-effort hardware/OS profile, checks inotify limits against
//! the repo-size tier table, verifies tool availability, recommends a tier,
//! and writes a machine-readable artifact under `.kaptaind/doctor/`.

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
