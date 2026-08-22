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
//!
//! On Cargo workspaces it additionally runs the W2 checks from
//! `docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md` §3.4: member lockfile
//! drift, unsatisfiable inter-member requirements, and the `root_only`
//! deflation signature.

use chrono::Utc;
use kaptaind::config::loader::{Config, WorkspacePolicy};
use kaptaind::util::style::*;
use kaptaind::version::workspace::WorkspaceLayout;
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
    /// W2 workspace findings (plan §3.4): member lockfile drift,
    /// unsatisfiable inter-member requirements, `root_only` deflation.
    /// Additive; older readers ignore unknown fields per the compatibility
    /// contract.
    #[serde(default)]
    pub workspace: Vec<MigrationFinding>,
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
        workspace: collect_workspace_findings(config),
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
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let toml_text = std::fs::read_to_string(toml_path).ok();

    let ignore_path = if config.watch.ignore_file.is_absolute() {
        config.watch.ignore_file.clone()
    } else {
        config.repo_path.join(&config.watch.ignore_file)
    };
    // traci: allow -- optional failure is represented by None and handled by the caller.
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
    // traci: allow -- optional failure is represented by None and handled by the caller.
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

/// A workspace crate whose declared version is checked against its
/// `Cargo.lock` `[[package]]` entry.
struct DeclaredVersion {
    /// `[package].name`, matching the lockfile entry's `name`.
    name: String,
    /// The version the crate currently declares (VERSION-file and
    /// workspace-inheritance precedence already resolved by the caller).
    version: String,
}

/// A workspace member with its resolved current version, used to check
/// inter-member path-dependency requirements.
struct MemberVersion {
    name: String,
    manifest: PathBuf,
    version: semver::Version,
}

/// Read the workspace manifests, lockfile, and recent git history and run
/// the W2 workspace checks (plan §3.4). Everything is best-effort: a repo
/// without a Cargo workspace, lockfile, or git history simply yields fewer
/// findings, and discovery errors surface on the daemon's own writeback
/// path, so doctor does not re-report them.
fn collect_workspace_findings(config: &Config) -> Vec<MigrationFinding> {
    let repo = &config.repo_path;
    let Ok(layout) = WorkspaceLayout::discover(repo) else {
        return Vec::new();
    };
    if matches!(layout, WorkspaceLayout::Single) {
        return Vec::new();
    }

    let root_manifest = repo.join("Cargo.toml");
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let root_text = std::fs::read_to_string(&root_manifest).ok();
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let version_text = std::fs::read_to_string(repo.join("VERSION")).ok();
    let member_texts: Vec<Option<String>> = layout
        .members()
        .iter()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .map(|m| std::fs::read_to_string(&m.manifest).ok())
        .collect();
    let members = member_versions(&layout, root_text.as_deref(), &member_texts);

    let mut findings = Vec::new();

    // workspace_lock_drift: declared versions vs Cargo.lock entries.
    if let Ok(lock_text) = std::fs::read_to_string(repo.join("Cargo.lock")) {
        let mut crates: Vec<DeclaredVersion> = Vec::new();
        if matches!(layout, WorkspaceLayout::RootCrate { .. }) {
            crates.extend(root_declared_version(
                root_text.as_deref(),
                version_text.as_deref(),
            ));
        }
        crates.extend(members.iter().map(|m| DeclaredVersion {
            name: m.name.clone(),
            version: m.version.to_string(),
        }));
        findings.extend(detect_workspace_lock_drift(&crates, &lock_text));
    }

    // workspace_requirement_unsatisfiable: inter-member path-dep floors.
    let mut manifests: Vec<(PathBuf, String)> = Vec::new();
    if let Some(text) = root_text {
        manifests.push((root_manifest, text));
    }
    for (member, text) in layout.members().iter().zip(member_texts.iter()) {
        if let Some(text) = text {
            manifests.push((member.manifest.clone(), text.clone()));
        }
    }
    findings.extend(detect_workspace_requirement_unsatisfiable(
        &manifests, &members,
    ));

    // workspace_root_only_deflation: member-only history under root_only.
    if let Some(commits) = recent_commit_paths(repo) {
        let member_dirs: Vec<PathBuf> = layout
            .members()
            .iter()
            .filter_map(|m| {
                let dir = m.manifest.parent()?;
                Some(normalize_path(dir.strip_prefix(repo).unwrap_or(dir)))
            })
            .collect();
        findings.extend(detect_workspace_root_only_deflation(
            config.versioning.workspace,
            &member_dirs,
            &commits,
        ));
    }

    findings
}

/// Resolve the root crate's declared version: the `VERSION` file wins, else
/// the root `[package].version`. `None` when neither resolves — never guess.
fn root_declared_version(
    root_text: Option<&str>,
    version_text: Option<&str>,
) -> Option<DeclaredVersion> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let doc = root_text?.parse::<toml_edit::DocumentMut>().ok()?;
    let name = doc
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())?;
    let version = version_text
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| {
            doc.get("package")
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })?;
    Some(DeclaredVersion {
        name: name.to_string(),
        version,
    })
}

/// Resolve every member's current manifest version: inheriting members read
/// the root `[workspace.package].version`, plain members their own
/// `[package].version`. Members whose version is missing or not valid
/// semver are skipped — never guessed.
fn member_versions(
    layout: &WorkspaceLayout,
    root_text: Option<&str>,
    member_texts: &[Option<String>],
) -> Vec<MemberVersion> {
    let workspace_version = root_text
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .and_then(|t| t.parse::<toml_edit::DocumentMut>().ok())
        .and_then(|doc| {
            doc.get("workspace")
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    let mut members = Vec::new();
    for (member, text) in layout.members().iter().zip(member_texts.iter()) {
        let raw = if member.inherits_version {
            workspace_version.clone()
        } else {
            text.as_deref()
                // traci: allow -- optional failure is represented by None and handled by the caller.
                .and_then(|t| t.parse::<toml_edit::DocumentMut>().ok())
                .and_then(|doc| {
                    doc.get("package")
                        .and_then(|p| p.get("version"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
        };
        // traci: allow -- optional failure is represented by None and handled by the caller.
        let version = raw.and_then(|raw| semver::Version::parse(&raw).ok());
        if let Some(version) = version {
            members.push(MemberVersion {
                name: member.name.clone(),
                manifest: member.manifest.clone(),
                version,
            });
        }
    }
    members
}

/// W2 check `workspace_lock_drift`: a crate's declared version disagrees
/// with its `Cargo.lock` entry — the manifest moved without a lock sync (or
/// vice versa), so the N-tuple invariant the daemon maintains is broken.
/// Skipped by the caller when there is no lockfile or the layout is
/// `Single`; crates absent from the lock are skipped here.
fn detect_workspace_lock_drift(
    crates: &[DeclaredVersion],
    lock_text: &str,
) -> Vec<MigrationFinding> {
    let Ok(doc) = lock_text.parse::<toml_edit::DocumentMut>() else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for krate in crates {
        let Ok(declared) = semver::Version::parse(&krate.version) else {
            continue; // unparseable declared version — resolve_baseline reports it
        };
        let Some(locked) = lock_entry_version(&doc, &krate.name) else {
            continue; // not every workspace crate is necessarily locked
        };
        if locked == declared {
            continue;
        }
        findings.push(MigrationFinding {
            check: "workspace_lock_drift".to_string(),
            severity: MigrationSeverity::Warn,
            message: format!(
                "`{}` declares version {declared} but its Cargo.lock entry is {locked} — \
                 the manifest and lockfile have drifted apart.",
                krate.name
            ),
            fix: "Let the daemon commit a bump (it syncs Cargo.lock in the same commit), \
                  or sync the lockfile by hand (`cargo update --workspace`)."
                .to_string(),
        });
    }
    findings
}

/// The `[[package]]` entry version for `name` in a parsed Cargo.lock, when
/// the entry exists and is valid semver.
fn lock_entry_version(doc: &toml_edit::DocumentMut, name: &str) -> Option<semver::Version> {
    doc.get("package")
        .and_then(|p| p.as_array_of_tables())
        .and_then(|entries| {
            entries.iter().find_map(|pkg| {
                let pkg_name = pkg.get("name").and_then(|n| n.as_str())?;
                if pkg_name != name {
                    return None;
                }
                let raw = pkg.get("version").and_then(|v| v.as_str())?;
                // traci: allow -- optional failure is represented by None and handled by the caller.
                semver::Version::parse(raw).ok()
            })
        })
}

/// W2 check `workspace_requirement_unsatisfiable`: a path dependency on a
/// workspace member carries a `version` requirement the member's current
/// version does not satisfy, so `cargo build --locked` fails. Scans
/// `[dependencies]`, `[dev-dependencies]`, and `[build-dependencies]` of
/// every workspace manifest; path-only and registry entries are skipped.
fn detect_workspace_requirement_unsatisfiable(
    manifests: &[(PathBuf, String)],
    members: &[MemberVersion],
) -> Vec<MigrationFinding> {
    let mut findings = Vec::new();
    for (manifest, content) in manifests {
        let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
            continue;
        };
        let Some(base) = manifest.parent() else {
            continue;
        };
        for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(deps) = doc.get(table_name).and_then(|t| t.as_table_like()) else {
                continue;
            };
            for (dep_name, entry) in deps.iter() {
                let (Some(rel), Some(req_str)) = (
                    entry.get("path").and_then(|p| p.as_str()),
                    entry.get("version").and_then(|v| v.as_str()),
                ) else {
                    continue; // path-only or registry-only: nothing to check
                };
                let dep_manifest = normalize_path(&base.join(rel).join("Cargo.toml"));
                let Some(member) = members
                    .iter()
                    .find(|m| normalize_path(&m.manifest) == dep_manifest)
                else {
                    continue; // path dependency outside the workspace
                };
                let Ok(req) = semver::VersionReq::parse(req_str) else {
                    continue;
                };
                if req.matches(&member.version) {
                    continue;
                }
                findings.push(MigrationFinding {
                    check: "workspace_requirement_unsatisfiable".to_string(),
                    severity: MigrationSeverity::Warn,
                    message: format!(
                        "`{dep_name}` requirement `{req_str}` in {} does not match \
                         workspace member `{}` at {} — the requirement is unsatisfiable.",
                        manifest.display(),
                        member.name,
                        member.version
                    ),
                    fix: format!(
                        "Raise the requirement floor to `{}` — the daemon does this \
                         automatically on the next bump.",
                        member.version
                    ),
                });
            }
        }
    }
    findings
}

/// W2 check `workspace_root_only_deflation`: under the default `root_only`
/// policy only the root moves, so a repo whose recent commits all live
/// inside member subtrees is silently deflating member versions. The
/// signature needs at least 5 path-touching commits and every one of them
/// confined to member subtrees; the caller skips the check entirely when
/// git history is unavailable.
fn detect_workspace_root_only_deflation(
    policy: WorkspacePolicy,
    member_dirs: &[PathBuf],
    commit_paths: &[Vec<String>],
) -> Vec<MigrationFinding> {
    if policy != WorkspacePolicy::RootOnly || member_dirs.is_empty() {
        return Vec::new();
    }
    // Only path-touching commits count; merges and empty commits say nothing.
    let touching: Vec<&Vec<String>> = commit_paths.iter().filter(|p| !p.is_empty()).collect();
    if touching.len() < 5 {
        return Vec::new();
    }
    let all_member_only = touching.iter().all(|paths| {
        paths.iter().all(|p| {
            let p = Path::new(p);
            member_dirs.iter().any(|dir| p.starts_with(dir))
        })
    });
    if !all_member_only {
        return Vec::new();
    }
    vec![MigrationFinding {
        check: "workspace_root_only_deflation".to_string(),
        severity: MigrationSeverity::Warn,
        message: format!(
            "[versioning].workspace is \"root_only\" but all of the last {} \
             path-touching commits stayed inside workspace member subtrees — \
             member-only work never moves member versions.",
            touching.len()
        ),
        fix: "Set `[versioning].workspace = \"touched\"` so clusters bump the members \
              they actually touch."
            .to_string(),
    }]
}

/// Run `git log --format=%n --name-only -n 20` and return one path list per
/// commit. The format is `%n`, not empty: git (2.43+) collapses an empty
/// `--format=` header away entirely, so commits would run together with no
/// separator; `%n` makes git itself print the blank line that separates
/// commits. `None` on any git error or in a non-git repo — the deflation
/// check skips silently there.
fn recent_commit_paths(repo: &Path) -> Option<Vec<Vec<String>>> {
    let out = Command::new("git")
        .args(["log", "--format=%n", "--name-only", "-n", "20"])
        .current_dir(repo)
        .output()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        .filter(|o| o.status.success())?;
    Some(split_name_only_log(&String::from_utf8_lossy(&out.stdout)))
}

/// Split `git log --format=%n --name-only` output into one path list per
/// commit. Commits are separated by (possibly several) blank lines; commits
/// with no paths (merges) leave no block.
fn split_name_only_log(output: &str) -> Vec<Vec<String>> {
    let mut commits = Vec::new();
    let mut current = Vec::new();
    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            if !current.is_empty() {
                commits.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        commits.push(current);
    }
    commits
}

/// Lexically normalize a path (resolving `.` and `..`) without touching the
/// filesystem, so dependency paths like `../alpha` compare equal to the
/// discovered member manifest paths.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
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
    // traci: allow -- optional failure is represented by None and handled by the caller.
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_kernel() -> Option<String> {
    if let Ok(v) = std::fs::read_to_string("/proc/version") {
        return Some(v.trim().to_string());
    }
    Command::new("uname")
        .arg("-r")
        .output()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_cpu_model() -> Option<String> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
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
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let kb: u64 = rest
                .split_whitespace()
                .next()
                // traci: allow -- optional failure is represented by None and handled by the caller.
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
    // traci: allow -- optional failure is represented by None and handled by the caller.
    match std::fs::read_to_string(&rot).ok().as_deref().map(str::trim) {
        Some("0") => "ssd".to_string(),
        Some("1") => "hdd".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Find the mount source device for `repo` from `/proc/mounts` (longest match).
fn mount_source(repo: &Path) -> Option<String> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
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
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        // traci: allow -- optional failure is represented by None and handled by the caller.
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
            Err(error) => {
                tracing::error!(
                    ?error,
                    operation = "count_files",
                    source_line = line!(),
                    "count files returned an error"
                );
                continue;
            }
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
    if let Err(error) = std::fs::write(&latest, serde_json::to_string_pretty(report)?) {
        tracing::warn!(
            ?error,
            operation = "write_artifact",
            source_line = line!(),
            "best-effort operation failed"
        );
    }
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
            print_finding(f);
        }
    }

    println!("\n{}", "🗃  Workspace versioning:".bold().cyan());
    if r.workspace.is_empty() {
        println!(
            "  {} {}",
            "✅".green(),
            "No workspace versioning issues detected.".green()
        );
    } else {
        for f in &r.workspace {
            print_finding(f);
        }
    }

    println!(
        "\n{} {}",
        "Artifact:".dimmed(),
        config_relative(&r.repo_path).dimmed()
    );
}

fn print_finding(f: &MigrationFinding) {
    let badge = match f.severity {
        MigrationSeverity::Info => "info".cyan().to_string(),
        MigrationSeverity::Warn => "WARN".yellow().to_string(),
    };
    println!("  {} {}", badge, f.message);
    println!("       {} {}", "fix:".dimmed(), f.fix.as_str().dimmed());
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

    // --- W2 workspace checks ---------------------------------------------

    use kaptaind::version::workspace::Member;

    const ROOT_MANIFEST: &str = "[package]\nname = \"root-crate\"\nversion = \"0.2.0\"\n\n[workspace]\nmembers = [\"crates/*\"]\n";
    const ALPHA_MANIFEST: &str = "[package]\nname = \"alpha\"\nversion = \"0.2.0\"\n";

    fn member(name: &str, manifest: &str, inherits_version: bool) -> Member {
        Member {
            name: name.to_string(),
            manifest: PathBuf::from(manifest),
            inherits_version,
        }
    }

    fn lock(packages: &[(&str, &str)]) -> String {
        let mut text = String::from("version = 4\n");
        for (name, version) in packages {
            text.push_str(&format!(
                "\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n"
            ));
        }
        text
    }

    fn commits(sets: &[&[&str]]) -> Vec<Vec<String>> {
        sets.iter()
            .map(|s| s.iter().map(|p| p.to_string()).collect())
            .collect()
    }

    #[test]
    fn lock_drift_member_ahead_of_lock_flags() {
        // Seeded drift: member manifest at 0.2.0, lock entry still 0.1.0.
        let crates = vec![
            DeclaredVersion {
                name: "root-crate".to_string(),
                version: "0.2.0".to_string(),
            },
            DeclaredVersion {
                name: "alpha".to_string(),
                version: "0.2.0".to_string(),
            },
        ];
        let lock = lock(&[("root-crate", "0.2.0"), ("alpha", "0.1.0")]);
        let fs = detect_workspace_lock_drift(&crates, &lock);
        assert_eq!(fs.len(), 1);
        let f = &fs[0];
        assert_eq!(f.check, "workspace_lock_drift");
        assert_eq!(f.severity, MigrationSeverity::Warn);
        assert!(f.message.contains("`alpha`"));
        assert!(f.message.contains("0.2.0"));
        assert!(f.message.contains("0.1.0"));
        assert!(f.fix.contains("bump"));
    }

    #[test]
    fn lock_drift_in_sync_no_finding() {
        let crates = vec![DeclaredVersion {
            name: "alpha".to_string(),
            version: "0.2.0".to_string(),
        }];
        let fs = detect_workspace_lock_drift(&crates, &lock(&[("alpha", "0.2.0")]));
        assert!(fs.is_empty());
    }

    #[test]
    fn lock_drift_missing_entry_and_unparseable_lock_skip() {
        let crates = vec![DeclaredVersion {
            name: "alpha".to_string(),
            version: "0.2.0".to_string(),
        }];
        assert!(detect_workspace_lock_drift(&crates, &lock(&[("beta", "0.1.0")])).is_empty());
        assert!(detect_workspace_lock_drift(&crates, "[[[broken").is_empty());
    }

    #[test]
    fn root_version_file_wins_over_manifest() {
        let declared =
            root_declared_version(Some(ROOT_MANIFEST), Some("0.3.0\n")).expect("declared");
        assert_eq!(declared.name, "root-crate");
        assert_eq!(declared.version, "0.3.0");
    }

    #[test]
    fn root_declared_version_falls_back_to_manifest() {
        let declared = root_declared_version(Some(ROOT_MANIFEST), None).expect("declared");
        assert_eq!(declared.version, "0.2.0");
    }

    #[test]
    fn member_versions_inherit_workspace_package_version() {
        let layout = WorkspaceLayout::RootCrate {
            members: vec![member("alpha", "/ws/crates/alpha/Cargo.toml", true)],
        };
        let root = "[package]\nname = \"root-crate\"\nversion = \"0.2.0\"\n\n[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"0.4.0\"\n";
        let alpha = "[package]\nname = \"alpha\"\nversion.workspace = true\n";
        let members = member_versions(&layout, Some(root), &[Some(alpha.to_string())]);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "alpha");
        assert_eq!(members[0].version, semver::Version::new(0, 4, 0));
    }

    #[test]
    fn member_versions_plain_member_reads_own_version() {
        let layout = WorkspaceLayout::RootCrate {
            members: vec![member("alpha", "/ws/crates/alpha/Cargo.toml", false)],
        };
        let members = member_versions(
            &layout,
            Some(ROOT_MANIFEST),
            &[Some(ALPHA_MANIFEST.to_string())],
        );
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].version, semver::Version::new(0, 2, 0));
    }

    #[test]
    fn requirement_unsatisfiable_flags() {
        let layout = WorkspaceLayout::RootCrate {
            members: vec![member("alpha", "/ws/crates/alpha/Cargo.toml", false)],
        };
        let members = member_versions(
            &layout,
            Some(ROOT_MANIFEST),
            &[Some(ALPHA_MANIFEST.to_string())],
        );
        // Root depends on alpha at an old floor; beta (another member) does
        // the same from dev-dependencies — both must be flagged.
        let root = "[package]\nname = \"root-crate\"\nversion = \"0.2.0\"\n\n[workspace]\nmembers = [\"crates/*\"]\n\n[dependencies]\nalpha = { path = \"crates/alpha\", version = \"0.1.0\" }\n";
        let beta = "[package]\nname = \"beta\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\nalpha = { path = \"../alpha\", version = \"0.1\" }\n";
        let manifests = vec![
            (PathBuf::from("/ws/Cargo.toml"), root.to_string()),
            (
                PathBuf::from("/ws/crates/beta/Cargo.toml"),
                beta.to_string(),
            ),
        ];
        let fs = detect_workspace_requirement_unsatisfiable(&manifests, &members);
        assert_eq!(fs.len(), 2);
        for f in &fs {
            assert_eq!(f.check, "workspace_requirement_unsatisfiable");
            assert_eq!(f.severity, MigrationSeverity::Warn);
            assert!(f.message.contains("`alpha`"));
            assert!(f.message.contains("0.2.0"));
            assert!(f.fix.contains("0.2.0"));
        }
        assert!(fs[0].message.contains("0.1.0"));
        assert!(fs[1].message.contains("0.1"));
    }

    #[test]
    fn requirement_satisfied_no_finding() {
        let layout = WorkspaceLayout::RootCrate {
            members: vec![member("alpha", "/ws/crates/alpha/Cargo.toml", false)],
        };
        let members = member_versions(
            &layout,
            Some(ROOT_MANIFEST),
            &[Some(ALPHA_MANIFEST.to_string())],
        );
        // Caret requirement satisfied by alpha's 0.2.0.
        let root = "[package]\nname = \"root-crate\"\nversion = \"0.2.0\"\n\n[dependencies]\nalpha = { path = \"crates/alpha\", version = \"0.2\" }\n";
        let manifests = vec![(PathBuf::from("/ws/Cargo.toml"), root.to_string())];
        assert!(detect_workspace_requirement_unsatisfiable(&manifests, &members).is_empty());
    }

    #[test]
    fn requirement_path_only_registry_and_outside_paths_skip() {
        let layout = WorkspaceLayout::RootCrate {
            members: vec![member("alpha", "/ws/crates/alpha/Cargo.toml", false)],
        };
        let members = member_versions(
            &layout,
            Some(ROOT_MANIFEST),
            &[Some(ALPHA_MANIFEST.to_string())],
        );
        let root = "[package]\nname = \"root-crate\"\nversion = \"0.2.0\"\n\n[dependencies]\nalpha = { path = \"crates/alpha\" }\nserde = \"1\"\nother = { path = \"../outside\", version = \"9.9.9\" }\n";
        let manifests = vec![(PathBuf::from("/ws/Cargo.toml"), root.to_string())];
        assert!(detect_workspace_requirement_unsatisfiable(&manifests, &members).is_empty());
    }

    #[test]
    fn split_name_only_log_groups_commits() {
        let out = "crates/alpha/src/lib.rs\n\ncrates/beta/src/main.rs\ncrates/alpha/Cargo.toml\n\n\ncrates/alpha/src/lib.rs\n";
        let commits = split_name_only_log(out);
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0], ["crates/alpha/src/lib.rs"]);
        assert_eq!(
            commits[1],
            ["crates/beta/src/main.rs", "crates/alpha/Cargo.toml"]
        );
        assert_eq!(commits[2], ["crates/alpha/src/lib.rs"]);
        assert!(split_name_only_log("").is_empty());
    }

    #[test]
    fn deflation_member_only_history_flags() {
        let dirs = vec![PathBuf::from("crates/alpha"), PathBuf::from("crates/beta")];
        let history = commits(&[
            &["crates/alpha/src/lib.rs"],
            &["crates/beta/src/main.rs", "crates/alpha/Cargo.toml"],
            &["crates/alpha/src/lib.rs"],
            &["crates/beta/README.md"],
            &["crates/alpha/tests/t.rs"],
        ]);
        let fs = detect_workspace_root_only_deflation(WorkspacePolicy::RootOnly, &dirs, &history);
        let f = check(&fs, "workspace_root_only_deflation").expect("expected finding");
        assert_eq!(f.severity, MigrationSeverity::Warn);
        assert!(f.message.contains("root_only"));
        assert!(f.fix.contains("workspace = \"touched\""));
    }

    #[test]
    fn deflation_touched_policy_no_finding() {
        let dirs = vec![PathBuf::from("crates/alpha")];
        let history = commits(&[
            &["crates/alpha/a.rs"],
            &["crates/alpha/b.rs"],
            &["crates/alpha/c.rs"],
            &["crates/alpha/d.rs"],
            &["crates/alpha/e.rs"],
        ]);
        assert!(
            detect_workspace_root_only_deflation(WorkspacePolicy::Touched, &dirs, &history)
                .is_empty()
        );
    }

    #[test]
    fn deflation_root_touching_commit_no_finding() {
        let dirs = vec![PathBuf::from("crates/alpha")];
        let history = commits(&[
            &["crates/alpha/a.rs"],
            &["crates/alpha/b.rs"],
            &["src/main.rs"], // one root-touching commit breaks the signature
            &["crates/alpha/c.rs"],
            &["crates/alpha/d.rs"],
            &["crates/alpha/e.rs"],
        ]);
        assert!(
            detect_workspace_root_only_deflation(WorkspacePolicy::RootOnly, &dirs, &history)
                .is_empty()
        );
    }

    #[test]
    fn deflation_too_few_commits_no_finding() {
        let dirs = vec![PathBuf::from("crates/alpha")];
        let history = commits(&[
            &["crates/alpha/a.rs"],
            &["crates/alpha/b.rs"],
            &["crates/alpha/c.rs"],
            &["crates/alpha/d.rs"],
        ]);
        assert!(
            detect_workspace_root_only_deflation(WorkspacePolicy::RootOnly, &dirs, &history)
                .is_empty()
        );
    }
}
