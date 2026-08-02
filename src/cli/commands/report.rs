//! `kaptaind-cli report` — aggregate qualification evidence (Workstream G).
//!
//! Discovers the latest doctor/bench/stress artifacts, folds in optional
//! external logs (cargo-test/clippy/deny/container), computes git state and
//! the config hash, and emits the `kaptaind.qualification.v1` JSON plus a
//! human markdown report under `.kaptaind/report/`.

use chrono::Utc;
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::table::print_table;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum Verdict {
    Pass,
    PassWithNotes,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub verdict: Verdict,
    pub evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub rev: Option<String>,
    pub dirty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub cpu: Option<String>,
    pub cores: Option<usize>,
    pub ram_gb: Option<f64>,
    pub disk: Option<String>,
    pub os: Option<String>,
    pub container: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainInfo {
    pub rustc: Option<String>,
    pub cargo_deny: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignOff {
    pub prepared_by: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualificationReport {
    pub schema: &'static str,
    pub version: String,
    pub git: GitInfo,
    pub generated_at: String,
    pub config_hash: Option<String>,
    pub host: HostInfo,
    pub toolchain: ToolchainInfo,
    pub sections: BTreeMap<String, Section>,
    pub overall: Verdict,
    pub sign_off: SignOff,
}

pub struct ReportOptions<'a> {
    pub version: Option<&'a str>,
    pub out: Option<&'a Path>,
    pub cargo_test: Option<&'a Path>,
    pub clippy: Option<&'a Path>,
    pub deny: Option<&'a Path>,
    pub container: Option<&'a Path>,
}

pub fn handle_report(
    config: &Config,
    opts: &ReportOptions<'_>,
    format: &str,
) -> anyhow::Result<()> {
    let version = match opts.version {
        Some(v) => v.to_string(),
        None => read_version(&config.repo_path).unwrap_or_else(|| "unknown".to_string()),
    };

    let report = build(config, &version, opts);
    let (md_path, json_path) = write_artifacts(config, opts.out, &report)?;

    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report, &md_path, &json_path);
    }
    Ok(())
}

fn build(config: &Config, version: &str, opts: &ReportOptions<'_>) -> QualificationReport {
    let repo = &config.repo_path;
    let kd = repo.join(".kaptaind");

    let doctor = latest_json(&kd.join("doctor"));
    let bench = latest_json(&kd.join("bench"));
    let stress = latest_json(&kd.join("stress"));
    let soak = latest_json(&kd.join("soak"));

    let mut sections: BTreeMap<String, Section> = BTreeMap::new();

    // correctness: cargo-test (+ clippy folded in as a build-correctness gate).
    sections.insert(
        "correctness".into(),
        section_from_logs(&[
            ("cargo-test", opts.cargo_test, "TEST_EXIT"),
            ("clippy", opts.clippy, "CLIPPY_EXIT"),
        ]),
    );

    // benchmarks: latest bench artifact.
    sections.insert(
        "benchmarks".into(),
        artifact_section(&bench, None, "no bench artifact; not run in-session"),
    );

    // stress: latest stress artifact; verdict reflects its own pass/fail.
    let stress_section = match &stress {
        Some(path) => {
            let pass = read_json_bool(path, "pass").unwrap_or(false);
            let verdict = if pass { Verdict::Pass } else { Verdict::Fail };
            Section {
                verdict,
                evidence: vec![display(path)],
                notes: if pass {
                    None
                } else {
                    Some("stress artifact reports a failing invariant".into())
                },
            }
        }
        None => Section {
            verdict: Verdict::PassWithNotes,
            evidence: vec![],
            notes: Some("no stress artifact; not run in-session".into()),
        },
    };
    sections.insert("stress".into(), stress_section);

    // soak: always PASS-WITH-NOTES unless an artifact exists.
    sections.insert(
        "soak".into(),
        artifact_section(
            &soak,
            Some("24h soak is a CI/scheduled harness; evidence present"),
            "24h soak is a CI/scheduled harness; not run in-session",
        ),
    );

    // inspection: clippy (lint) + note that trace/audit verify are CLI-only.
    let mut inspection = section_from_logs(&[("clippy", opts.clippy, "CLIPPY_EXIT")]);
    if inspection.evidence.is_empty() && inspection.verdict == Verdict::PassWithNotes {
        inspection.notes =
            Some("trace/audit verify run via CLI on demand; no inspection log in-session".into());
    }
    sections.insert("inspection".into(), inspection);

    // containers.
    sections.insert(
        "containers".into(),
        section_from_logs(&[("container", opts.container, "CONTAINER_EXIT")]),
    );

    // security.
    sections.insert(
        "security".into(),
        section_from_logs(&[("deny", opts.deny, "DENY_EXIT")]),
    );

    // hardware.
    sections.insert(
        "hardware".into(),
        artifact_section(&doctor, None, "no doctor artifact; not run in-session"),
    );

    let overall = overall_verdict(&sections);

    QualificationReport {
        schema: "kaptaind.qualification.v1",
        version: version.to_string(),
        git: GitInfo {
            rev: git_rev(repo),
            dirty: git_dirty(repo),
        },
        generated_at: Utc::now().to_rfc3339(),
        config_hash: hash_file(&repo.join("kaptaind.toml")),
        host: host_from_doctor(doctor.as_deref()),
        toolchain: ToolchainInfo {
            rustc: run_version("rustc", &["--version"]),
            cargo_deny: opts.deny.map(|_| "see security evidence".to_string()),
        },
        sections,
        overall,
        sign_off: SignOff {
            prepared_by: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "unknown".to_string()),
            approved_by: None,
            approved_at: None,
        },
    }
}

/// Build a section from one or more optional exit-marker logs.
fn section_from_logs(sources: &[(&str, Option<&Path>, &str)]) -> Section {
    let mut evidence = Vec::new();
    let mut verdict: Option<Verdict> = None;
    let mut notes = Vec::new();

    for (label, path, marker) in sources {
        match path {
            None => {} // not run — handled below if no evidence at all
            Some(p) => match read_exit_marker(p, marker) {
                Some(0) => {
                    evidence.push(display(p));
                    verdict = Some(merge_verdict(verdict, Verdict::Pass));
                }
                Some(code) => {
                    evidence.push(display(p));
                    notes.push(format!("{label} exited {code}"));
                    verdict = Some(Verdict::Fail);
                }
                None => {
                    evidence.push(display(p));
                    notes.push(format!(
                        "{label}: log present but no `{marker}` exit marker"
                    ));
                    verdict = Some(merge_verdict(verdict, Verdict::PassWithNotes));
                }
            },
        }
    }

    if evidence.is_empty() {
        return Section {
            verdict: Verdict::PassWithNotes,
            evidence,
            notes: Some("not run in-session".into()),
        };
    }

    Section {
        verdict: verdict.unwrap_or(Verdict::PassWithNotes),
        evidence,
        notes: if notes.is_empty() {
            None
        } else {
            Some(notes.join("; "))
        },
    }
}

fn artifact_section(
    path: &Option<PathBuf>,
    present_note: Option<&str>,
    absent_note: &str,
) -> Section {
    match path {
        Some(p) => Section {
            verdict: Verdict::Pass,
            evidence: vec![display(p)],
            notes: present_note.map(|s| s.to_string()),
        },
        None => Section {
            verdict: Verdict::PassWithNotes,
            evidence: vec![],
            notes: Some(absent_note.to_string()),
        },
    }
}

fn merge_verdict(current: Option<Verdict>, new: Verdict) -> Verdict {
    match (current, new) {
        (Some(Verdict::Fail), _) | (_, Verdict::Fail) => Verdict::Fail,
        (Some(Verdict::PassWithNotes), _) | (_, Verdict::PassWithNotes) => Verdict::PassWithNotes,
        _ => Verdict::Pass,
    }
}

fn overall_verdict(sections: &BTreeMap<String, Section>) -> Verdict {
    let mut saw_notes = false;
    for s in sections.values() {
        match s.verdict {
            Verdict::Fail => return Verdict::Fail,
            Verdict::PassWithNotes => saw_notes = true,
            Verdict::Pass => {}
        }
    }
    if saw_notes {
        Verdict::PassWithNotes
    } else {
        Verdict::Pass
    }
}

/// Read the last non-empty line of `path` and extract `<MARKER>=<int>`.
fn read_exit_marker(path: &Path, marker: &str) -> Option<i32> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let content = std::fs::read_to_string(path).ok()?;
    let last = content.lines().rev().find(|l| !l.trim().is_empty())?;
    extract_marker(last, marker).or_else(|| extract_marker(last, "EXIT"))
}

fn extract_marker(line: &str, marker: &str) -> Option<i32> {
    let prefix = format!("{marker}=");
    let idx = line.find(&prefix)?;
    let rest = &line[idx + prefix.len()..];
    let digits: String = rest
        .chars()
        .take_while(|c| *c == '-' || c.is_ascii_digit())
        .collect();
    // traci: allow -- optional failure is represented by None and handled by the caller.
    digits.parse().ok()
}

fn read_json_bool(path: &Path, key: &str) -> Option<bool> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let content = std::fs::read_to_string(path).ok()?;
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    value.get(key)?.as_bool()
}

fn host_from_doctor(path: Option<&Path>) -> HostInfo {
    let read = || -> Option<serde_json::Value> {
        let p = path?;
        // traci: allow -- optional failure is represented by None and handled by the caller.
        let content = std::fs::read_to_string(p).ok()?;
        // traci: allow -- optional failure is represented by None and handled by the caller.
        serde_json::from_str(&content).ok()
    };
    let doc = read();
    let get_s = |k: &str| {
        doc.as_ref()
            .and_then(|d| d.get(k)?.as_str())
            .map(str::to_string)
    };
    let ram_gb = doc
        .as_ref()
        .and_then(|d| d.get("ram_total_bytes")?.as_u64())
        .map(|b| (b as f64 / (1024.0 * 1024.0 * 1024.0) * 10.0).round() / 10.0);
    HostInfo {
        cpu: get_s("cpu_model"),
        cores: doc
            .as_ref()
            .and_then(|d| d.get("logical_cores")?.as_u64())
            .map(|n| n as usize),
        ram_gb,
        disk: get_s("disk_type"),
        os: get_s("os"),
        container: None,
    }
}

/// Latest `*.json` in `dir` by lexicographic name (timestamp-named), skipping
/// the `latest.json` pointer.
fn latest_json(dir: &Path) -> Option<PathBuf> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|e| e == "json").unwrap_or(false)
                && p.file_name().map(|n| n != "latest.json").unwrap_or(false)
        })
        .collect();
    names.sort();
    names.pop()
}

fn read_version(repo: &Path) -> Option<String> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let v = std::fs::read_to_string(repo.join("VERSION")).ok()?;
    let v = v.trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn git_rev(repo: &Path) -> Option<String> {
    run_git(repo, &["rev-parse", "HEAD"])
}

fn git_dirty(repo: &Path) -> Option<bool> {
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(!out.stdout.is_empty())
}

fn run_git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn run_version(bin: &str, args: &[&str]) -> Option<String> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let out = std::process::Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn hash_file(path: &Path) -> Option<String> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(kaptaind::util::hex::encode(hasher.finalize()))
}

fn display(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

fn write_artifacts(
    config: &Config,
    out: Option<&Path>,
    report: &QualificationReport,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let dir = match out {
        Some(p) => p.to_path_buf(),
        None => config.repo_path.join(".kaptaind").join("report"),
    };
    std::fs::create_dir_all(&dir)?;

    let date = Utc::now().format("%Y%m%d");
    let run_id = format!("{}-{}", report.version, Utc::now().format("%Y%m%dT%H%M%SZ"));

    let json_path = dir.join(format!("{run_id}.json"));
    std::fs::write(&json_path, serde_json::to_string_pretty(report)?)?;

    let md_path = dir.join(format!("report-{}-{}.md", report.version, date));
    std::fs::write(&md_path, render_markdown(report))?;

    Ok((md_path, json_path))
}

fn render_markdown(r: &QualificationReport) -> String {
    let mut md = String::new();
    md.push_str(&format!(
        "# Kaptaind Qualification Report — v{}\n\n",
        r.version
    ));
    md.push_str(&format!("- Generated: `{}`\n", r.generated_at));
    md.push_str(&format!(
        "- Git: `{}` (dirty: {})\n",
        r.git.rev.as_deref().unwrap_or("unknown"),
        r.git
            .dirty
            .map(|d| d.to_string())
            .unwrap_or("unknown".into())
    ));
    md.push_str(&format!(
        "- Config hash: `{}`\n",
        r.config_hash.as_deref().unwrap_or("n/a")
    ));
    md.push_str(&format!("- Overall: **{:?}**\n\n", r.overall));

    md.push_str("## Sections\n\n");
    md.push_str("| Section | Verdict | Evidence | Notes |\n");
    md.push_str("|---|---|---|---|\n");
    for (name, s) in &r.sections {
        let evidence = if s.evidence.is_empty() {
            "—".to_string()
        } else {
            s.evidence.join(", ")
        };
        let notes = s.notes.clone().unwrap_or_else(|| "—".to_string());
        md.push_str(&format!(
            "| {name} | {:?} | {evidence} | {notes} |\n",
            s.verdict
        ));
    }

    md.push_str("\n## Sign-off checklist\n\n");
    md.push_str("- [ ] All sections PASS or PASS-WITH-NOTES (no FAIL).\n");
    md.push_str("- [ ] Hardware tiers replaced with measured values.\n");
    md.push_str("- [ ] No open high/critical advisories.\n");
    md.push_str("- [ ] Container matrix green.\n");
    md.push_str("- [ ] Soak ≥ 24h with bounded memory/fd and zero data loss.\n");
    md.push_str("- [ ] Trace/audit determinism checks pass on a sampled set.\n");
    md.push_str(&format!(
        "- Prepared by: {}\n- Approved by: _pending_\n",
        r.sign_off.prepared_by
    ));
    md
}

fn print_human(r: &QualificationReport, md_path: &Path, json_path: &Path) {
    println!(
        "{} {}",
        "📑".blue(),
        "Kaptaind Qualification Report".bold().blue()
    );
    println!("{}", "===============================".blue());
    println!(
        "{} {}  {} {}  {} {}",
        "Version:".bold().cyan(),
        r.version.as_str().magenta(),
        "Git:".bold().cyan(),
        r.git
            .rev
            .as_deref()
            .unwrap_or("unknown")
            .chars()
            .take(8)
            .collect::<String>()
            .blue(),
        "Overall:".bold().cyan(),
        verdict_styled(&r.overall)
    );

    let rows: Vec<Vec<String>> = r
        .sections
        .iter()
        .map(|(name, s)| {
            vec![
                name.clone(),
                verdict_styled(&s.verdict).to_string(),
                if s.evidence.is_empty() {
                    "—".to_string()
                } else {
                    s.evidence.join(", ")
                },
            ]
        })
        .collect();
    print_table(&["Section", "Verdict", "Evidence"], &rows);

    for (name, s) in &r.sections {
        if let Some(notes) = &s.notes {
            println!("  {} {}: {}", "•".yellow(), name, notes.dimmed());
        }
    }

    println!("\n{} {}", "Markdown:".dimmed(), md_path.display());
    println!("{} {}", "JSON:    ".dimmed(), json_path.display());
}

fn verdict_styled(v: &Verdict) -> kaptaind::util::style::StyledString {
    match v {
        Verdict::Pass => "PASS".green(),
        Verdict::PassWithNotes => "PASS-WITH-NOTES".yellow(),
        Verdict::Fail => "FAIL".red(),
    }
}
