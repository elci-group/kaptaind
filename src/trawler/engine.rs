use crate::trawler::project::{
    detect_project_type_with_confidence, inspect_cargo_manifest, is_blacklisted, is_git_repo,
    is_kaptaind_initialized, workspace_members, CargoManifestKind, Confidence, ProjectType,
};
use ignore::{WalkBuilder, WalkState};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Options for the trawling operation
#[derive(Debug, Clone)]
pub struct TrawlOptions {
    /// Root directory to start trawling from
    pub root: PathBuf,
    /// Maximum depth to search (None for unlimited)
    pub max_depth: Option<usize>,
    /// Whether to skip directories that are already initialized
    pub skip_initialized: bool,
    /// Whether to skip non-git directories
    pub require_git: bool,
    /// Whether to auto-register discovered projects for monitoring
    pub auto_register: bool,
    /// Specific project types to look for (empty = all types)
    pub filter_types: Vec<ProjectType>,
    /// Minimum confidence threshold (0.0-1.0)
    pub min_confidence: f32,
    /// User-supplied blacklist of directory names or globs to skip, layered on top of
    /// `DEFAULT_SKIP_DIRS` and any `.gitignore`/`.ignore` files. Entries may be plain
    /// basenames (`"scratch"`) or globs matched against the path relative to `root`
    /// (`"vendor/*"`).
    pub blacklist: Vec<String>,
    /// Honor `.gitignore`, `.ignore`, global gitignore and parent gitignore files while
    /// walking (default: true). Disable to surface projects inside ignored directories.
    pub respect_ignore_files: bool,
    /// Follow symbolic links while walking (default: false).
    pub follow_links: bool,
    /// Also *initialize* Cargo workspace member crates with their own `kaptaind.toml`.
    /// Members are always *reported*; this only controls initialization (default: false).
    pub expand_workspaces: bool,
}

impl Default for TrawlOptions {
    fn default() -> Self {
        Self {
            root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_depth: None,
            skip_initialized: true,
            require_git: false,
            auto_register: true,
            filter_types: Vec::new(),
            min_confidence: 0.55, // Medium confidence minimum
            blacklist: Vec::new(),
            respect_ignore_files: true,
            follow_links: false,
            expand_workspaces: false,
        }
    }
}

/// Information about a discovered project
#[derive(Debug, Clone)]
pub struct DiscoveredProject {
    pub path: PathBuf,
    pub project_type: ProjectType,
    pub confidence: Confidence,
    pub confidence_score: f32,
    pub detection_indicators: Vec<String>,
    pub is_git_repo: bool,
    pub is_initialized: bool,
    pub depth: usize,
    /// For Cargo workspace member crates, the workspace root they belong to.
    /// `None` for standalone roots (including the workspace root itself).
    pub workspace_root: Option<PathBuf>,
    /// Parsed classification of the Rust manifest, when `project_type == Rust`.
    pub cargo_kind: Option<CargoManifestKind>,
}

/// Result of a trawling operation
#[derive(Debug, Clone)]
pub struct TrawlResult {
    pub projects: Vec<DiscoveredProject>,
    pub initialized_count: usize,
    pub registered_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
    /// Average confidence score of detected projects
    pub avg_confidence: f32,
    /// Number of high-confidence projects (>=0.80)
    pub high_confidence_count: usize,
    /// Number of very-high-confidence projects (>=0.95)
    pub very_high_confidence_count: usize,
}

/// Trawl a directory tree for codebases and optionally initialize them.
///
/// Root-down, ignore-aware discovery:
/// 1. Walk with `ignore::WalkBuilder` so `.gitignore`/`.ignore`/global gitignore,
///    `DEFAULT_SKIP_DIRS` and the user blacklist prune heavy subtrees.
/// 2. Detect candidates, gating Rust on a *valid* parsed `Cargo.toml`.
/// 3. Reduce root-down: the outermost valid project wins; Cargo workspace roots also
///    yield their member crates (reported, initialized only with `expand_workspaces`).
pub fn trawl(options: &TrawlOptions) -> anyhow::Result<TrawlResult> {
    let mut errors: Vec<String> = Vec::new();

    let root = &options.root;
    if !root.exists() {
        anyhow::bail!("Root path does not exist: {}", root.display());
    }
    if !root.is_dir() {
        anyhow::bail!("Root path is not a directory: {}", root.display());
    }

    let root_abs = absolutize(root)?;
    let blacklist = compile_blacklist(&options.blacklist, &mut errors);

    // Walk + detect candidates.
    let dirs = collect_dirs(&root_abs, options, &blacklist);
    let candidates: Vec<DiscoveredProject> = dirs
        .iter()
        .filter_map(|dir| detect_candidate(dir, &root_abs, options))
        .collect();

    // Root-down reduction (outermost wins; workspaces yield members).
    let filtered_projects = root_down_reduce(candidates);

    // Confidence metrics over the accepted projects.
    let avg_confidence = if !filtered_projects.is_empty() {
        filtered_projects
            .iter()
            .map(|p| p.confidence_score)
            .sum::<f32>()
            / filtered_projects.len() as f32
    } else {
        0.0
    };

    let high_confidence_count = filtered_projects
        .iter()
        .filter(|p| p.confidence_score >= 0.80)
        .count();
    let very_high_confidence_count = filtered_projects
        .iter()
        .filter(|p| p.confidence_score >= 0.95)
        .count();

    // Initialize projects that need it. Workspace members are reported but only
    // initialized when explicitly requested.
    let mut initialized_count = 0;
    let mut registered_count = 0;
    let mut skipped_count = 0;

    for project in &filtered_projects {
        if project.workspace_root.is_some() && !options.expand_workspaces {
            // Informational member crate: the workspace root owns initialization.
            continue;
        }

        if project.is_initialized && options.skip_initialized {
            skipped_count += 1;
            continue;
        }

        match initialize_project(project) {
            Ok(_) => {
                initialized_count += 1;

                if options.auto_register {
                    if let Err(e) = register_project(&project.path) {
                        tracing::error!(
                            ?e,
                            operation = "trawl",
                            source_line = line!(),
                            "trawl returned an error"
                        );
                        errors.push(format!(
                            "Failed to register {}: {}",
                            project.path.display(),
                            e
                        ));
                    } else {
                        registered_count += 1;
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    ?e,
                    operation = "trawl",
                    source_line = line!(),
                    "trawl returned an error"
                );
                errors.push(format!(
                    "Failed to initialize {}: {}",
                    project.path.display(),
                    e
                ));
            }
        }
    }

    Ok(TrawlResult {
        projects: filtered_projects,
        initialized_count,
        registered_count,
        skipped_count,
        errors,
        avg_confidence,
        high_confidence_count,
        very_high_confidence_count,
    })
}

/// Make `root` absolute without resolving symlinks (consistent with `follow_links`).
fn absolutize(root: &Path) -> anyhow::Result<PathBuf> {
    let abs = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    Ok(normalize_lexical(&abs))
}

/// Collapse `.` and lexical `..` components without touching the filesystem, so
/// reported paths are clean (e.g. `cwd/.` -> `cwd`). Symlinks are intentionally not
/// resolved, staying consistent with `follow_links = false`.
fn normalize_lexical(path: &Path) -> PathBuf {
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

/// Compile user blacklist patterns into globs, recording invalid patterns as errors.
fn compile_blacklist(patterns: &[String], errors: &mut Vec<String>) -> Vec<globset::Glob> {
    let mut globs = Vec::new();
    for pattern in patterns {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        match globset::Glob::new(trimmed) {
            Ok(g) => globs.push(g),
            Err(e) => {
                tracing::warn!(error = ?e, pattern = trimmed, "trawler blacklist pattern is invalid");
                errors.push(format!("Invalid blacklist pattern {:?}: {}", trimmed, e));
            }
        }
    }
    globs
}

/// Walk `root` and collect every directory that is not pruned by the built-in skip
/// list, the user blacklist, or ignore files. The root itself is always included so a
/// project sitting exactly at the trawl root is detected.
fn collect_dirs(root: &Path, options: &TrawlOptions, blacklist: &[globset::Glob]) -> Vec<PathBuf> {
    let mut builder = WalkBuilder::new(root);
    builder
        .max_depth(options.max_depth)
        .hidden(true)
        .follow_links(options.follow_links)
        .parents(options.respect_ignore_files)
        .ignore(options.respect_ignore_files)
        .git_ignore(options.respect_ignore_files)
        .git_global(options.respect_ignore_files)
        .git_exclude(options.respect_ignore_files)
        // Honor .gitignore even outside a git repository: trawling is not limited to
        // repos, and users expect ignore files to apply anywhere.
        .require_git(false);

    let found: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(vec![root.to_path_buf()]));
    let root_arc: Arc<Path> = Arc::from(root.to_path_buf());
    let blacklist = blacklist.to_vec();

    builder.build_parallel().run(|| {
        let found = found.clone();
        let root = root_arc.clone();
        let blacklist = blacklist.clone();
        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        operation = "collect_dirs",
                        source_line = line!(),
                        "collect dirs returned an error"
                    );
                    return WalkState::Continue;
                }
            };
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if !is_dir {
                return WalkState::Continue;
            }
            let path = entry.path();
            let rel = path.strip_prefix(root.as_ref()).unwrap_or(Path::new(""));
            if !rel.as_os_str().is_empty() {
                let name = entry.file_name().to_string_lossy();
                if is_blacklisted(&name, rel, &blacklist) {
                    return WalkState::Skip;
                }
            }
            if let Ok(mut v) = found.lock() {
                v.push(path.to_path_buf());
            }
            WalkState::Continue
        })
    });

    let mut dirs = match Arc::try_unwrap(found) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(shared) => {
            tracing::error!(
                ?shared,
                operation = "collect_dirs",
                source_line = line!(),
                "collect dirs returned an error"
            );
            shared.lock().map(|v| v.clone()).unwrap_or_default()
        }
    };
    // The walker yields the root entry in addition to our seed; de-duplicate so a
    // single directory is never detected twice.
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Run detection on a single directory, applying confidence/type/git filters and gating
/// Rust on a valid manifest. Returns `None` if the directory is not a candidate.
fn detect_candidate(dir: &Path, root: &Path, options: &TrawlOptions) -> Option<DiscoveredProject> {
    let detection = detect_project_type_with_confidence(dir);
    if detection.project_type == ProjectType::Unknown {
        return None;
    }
    if detection.confidence.score() < options.min_confidence {
        return None;
    }
    if !options.filter_types.is_empty() && !options.filter_types.contains(&detection.project_type) {
        return None;
    }

    let cargo_kind = if detection.project_type == ProjectType::Rust {
        let kind = inspect_cargo_manifest(dir);
        if !kind.is_valid() {
            return None;
        }
        Some(kind)
    } else {
        None
    };

    let is_git = is_git_repo(dir);
    if options.require_git && !is_git {
        return None;
    }

    let depth = dir
        .strip_prefix(root)
        .map(|r| r.components().count())
        .unwrap_or(0);

    Some(DiscoveredProject {
        path: dir.to_path_buf(),
        project_type: detection.project_type,
        confidence: detection.confidence,
        confidence_score: detection.confidence.score(),
        detection_indicators: detection.indicators,
        is_git_repo: is_git,
        is_initialized: is_kaptaind_initialized(dir),
        depth,
        workspace_root: None,
        cargo_kind,
    })
}

/// Root-down reduction: sort candidates shallowest-first, then keep a candidate only if
/// no already-accepted project is its ancestor — except Cargo workspace member crates,
/// which are kept and linked to their workspace root. Outermost project always wins.
fn root_down_reduce(mut candidates: Vec<DiscoveredProject>) -> Vec<DiscoveredProject> {
    candidates.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));

    let mut accepted: Vec<DiscoveredProject> = Vec::new();
    let mut accepted_roots: Vec<PathBuf> = Vec::new();
    let mut member_dirs: HashSet<PathBuf> = HashSet::new();

    for c in candidates.into_iter() {
        let ancestor = accepted_roots
            .iter()
            .find(|a| c.path != **a && c.path.starts_with(a))
            .cloned();

        match ancestor {
            None => {
                if c.cargo_kind.map(|k| k.is_workspace()).unwrap_or(false) {
                    for m in workspace_members(&c.path) {
                        member_dirs.insert(m);
                    }
                }
                accepted_roots.push(c.path.clone());
                accepted.push(c);
            }
            Some(root) if member_dirs.contains(&c.path) => {
                let mut member = c;
                member.workspace_root = Some(root);
                accepted.push(member);
            }
            _ => {
                // Nested beneath a non-workspace project: outermost wins, drop it.
            }
        }
    }

    accepted
}

/// Initialize a discovered project with kaptaind.toml and .kaptainignore
fn initialize_project(project: &DiscoveredProject) -> anyhow::Result<()> {
    let root = &project.path;
    let project_type = project.project_type;

    // Generate and write kaptaind.toml
    let toml_path = root.join("kaptaind.toml");
    if !toml_path.exists() {
        let toml_content = generate_toml(&project_type);
        std::fs::write(&toml_path, toml_content)?;
    }

    // Generate and write .kaptainignore
    let ignore_path = root.join(".kaptainignore");
    if !ignore_path.exists() {
        let ignore_content = generate_ignore(&project_type);
        std::fs::write(&ignore_path, ignore_content)?;
    }

    // Create VERSION file if it doesn't exist
    let version_path = root.join("VERSION");
    if !version_path.exists() {
        std::fs::write(&version_path, "0.1.0\n")?;
    }

    // Create .kaptaind directory structure
    let kaptaind_dir = root.join(".kaptaind");
    std::fs::create_dir_all(&kaptaind_dir)?;
    std::fs::create_dir_all(kaptaind_dir.join("analysis"))?;
    std::fs::create_dir_all(kaptaind_dir.join("traces"))?;
    std::fs::create_dir_all(kaptaind_dir.join("aoc").join("manifests"))?;
    std::fs::create_dir_all(kaptaind_dir.join("releases"))?;

    Ok(())
}

/// Register a project for auto-start monitoring
fn register_project(path: &Path) -> anyhow::Result<()> {
    crate::monitor::add(path, None, None, Some(true))
}

/// Generate kaptaind.toml content for a project type
fn generate_toml(project: &ProjectType) -> String {
    let test_cmd = project.test_command();
    let weights = "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15";

    format!(
        r#"# kaptaind configuration - auto-generated by `kaptaind-cli trawl`

# Observe-only by default: the daemon scores every change and records the
# decision to .kaptaind/decisions.jsonl, but never stages, commits, writes
# VERSION, pushes, or ships. Uncomment to let it actually commit — pushing
# additionally needs [push] enabled = true and
# [capabilities] network_push = true below.
# [operation]
# mode = "actuate"

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5

[weights]
{weights}

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10

# Repository-supplied configuration defaults to untrusted execution; this
# file was generated locally by `trawl`, so its own test command is trusted.
[trust]
execution = "trusted"

[test]
command = "{test_cmd}"
required = true

# [staging]
# mode = "all"        # "all" (default), "cluster" (only changed files), or "pattern"
# include = ["src/**"] # only used in "pattern" mode
# exclude = ["*.log", ".env*"]
"#
    )
}

/// Generate .kaptainignore content for a project type
fn generate_ignore(project: &ProjectType) -> String {
    let mut lines = vec![
        "# Common",
        ".git",
        ".kaptaind",
        ".DS_Store",
        "*.swp",
        "*.swo",
        "",
    ];

    lines.extend(project.ignore_patterns());
    lines.push(""); // trailing newline

    lines.join("\n")
}

/// Generate a summary report of discovered projects
pub fn generate_report(result: &TrawlResult) -> String {
    let mut report = String::new();
    report.push_str("🔍 Trawling Complete\n====================\n\n📊 Summary:\n");
    report.push_str(&format!(
        "  Projects discovered: {}\n  Initialized: {}\n  Registered: {}\n  Skipped (already initialized): {}\n",
        result.projects.len(),
        result.initialized_count,
        result.registered_count,
        result.skipped_count
    ));
    report.push_str("\n🎯 Detection Confidence:\n");
    report.push_str(&format!(
        "  Average confidence: {:.1}%\n  Very high confidence (≥95%): {}\n  High confidence (≥80%): {}\n",
        result.avg_confidence * 100.0,
        result.very_high_confidence_count,
        result.high_confidence_count
    ));

    if !result.errors.is_empty() {
        report.push_str(&format!("  ⚠️  Errors: {}\n", result.errors.len()));
    }

    if !result.projects.is_empty() {
        report.push_str("\n📦 Discovered Projects:\n");

        for project in &result.projects {
            let confidence_bar = match project.confidence {
                Confidence::VeryHigh => "████████████ ✅",
                Confidence::High => "████████ 👍",
                Confidence::Medium => "████ ⚠️ ",
                Confidence::Low => "██ ❌",
            };

            let status = if project.is_initialized {
                "(already initialized)"
            } else {
                "(new)"
            };

            let git_marker = if project.is_git_repo { "📚" } else { "  " };

            report.push_str(&format!(
                "  {} {} {:.0}% {} {} {}\n",
                git_marker,
                project.project_type,
                project.confidence_score * 100.0,
                confidence_bar,
                project.path.display(),
                status
            ));

            if let Some(root) = &project.workspace_root {
                report.push_str(&format!(
                    "      └─ workspace member of {}\n",
                    root.display()
                ));
            }

            // Show detection indicators for lower-confidence projects
            if project.confidence < Confidence::High && !project.detection_indicators.is_empty() {
                for indicator in &project.detection_indicators {
                    report.push_str(&format!("      └─ {}\n", indicator));
                }
            }
        }
    }

    if !result.errors.is_empty() {
        report.push_str("\n❌ Errors:\n");
        for error in &result.errors {
            report.push_str(&format!("  - {}\n", error));
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn opts(root: &Path) -> TrawlOptions {
        TrawlOptions {
            root: root.to_path_buf(),
            max_depth: Some(6),
            skip_initialized: false,
            require_git: false,
            auto_register: false,
            filter_types: Vec::new(),
            min_confidence: 0.55,
            ..Default::default()
        }
    }

    fn write_pkg(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    }

    #[test]
    fn trawl_finds_rust_project() {
        let temp = TempDir::new().unwrap();
        write_pkg(&temp.path().join("my-rust-app"));

        let result = trawl(&opts(temp.path())).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Rust);
        assert!(result.projects[0].path.ends_with("my-rust-app"));
    }

    #[test]
    fn trawl_confidence_scoring() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join("rust-app");
        write_pkg(&project_dir);
        std::fs::write(project_dir.join("Cargo.lock"), "").unwrap();
        std::fs::create_dir(project_dir.join("src")).unwrap();
        std::fs::write(project_dir.join("src/main.rs"), "fn main() {}").unwrap();

        let result = trawl(&opts(temp.path())).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert!(result.projects[0].confidence >= Confidence::High);
        assert!(result.avg_confidence >= 0.80);
    }

    #[test]
    fn trawl_skips_initialized_when_configured() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join("existing-project");
        write_pkg(&project_dir);
        std::fs::write(project_dir.join("kaptaind.toml"), "").unwrap();

        let mut options = opts(temp.path());
        options.skip_initialized = true;

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.initialized_count, 0);
    }

    #[test]
    fn trawl_respects_max_depth() {
        let temp = TempDir::new().unwrap();

        let level1 = temp.path().join("level1");
        write_pkg(&level1);

        let level2 = level1.join("level2");
        std::fs::create_dir(&level2).unwrap();
        std::fs::write(level2.join("package.json"), "{}").unwrap();

        let mut options = opts(temp.path());
        options.max_depth = Some(1);

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1); // Only level1
        assert!(result.projects[0].path.ends_with("level1"));
    }

    #[test]
    fn trawl_filters_by_project_type() {
        let temp = TempDir::new().unwrap();

        write_pkg(&temp.path().join("rust-project"));

        let node_dir = temp.path().join("node-project");
        std::fs::create_dir(&node_dir).unwrap();
        std::fs::write(node_dir.join("package.json"), "{}").unwrap();

        let mut options = opts(temp.path());
        options.filter_types = vec![ProjectType::Rust];

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Rust);
    }

    #[test]
    fn trawl_rejects_invalid_cargo_manifest() {
        let temp = TempDir::new().unwrap();
        // Empty Cargo.toml is not a valid Rust manifest.
        let bogus = temp.path().join("bogus");
        std::fs::create_dir(&bogus).unwrap();
        std::fs::write(bogus.join("Cargo.toml"), "").unwrap();

        let result = trawl(&opts(temp.path())).unwrap();
        assert!(result.projects.is_empty());
    }

    #[test]
    fn trawl_root_down_outermost_wins() {
        let temp = TempDir::new().unwrap();

        // A package with a nested, standalone Cargo.toml under examples/. The nested
        // manifest is NOT a workspace member, so only the outermost root is reported.
        let pkg = temp.path().join("pkg");
        write_pkg(&pkg);
        write_pkg(&pkg.join("examples/nested"));

        let result = trawl(&opts(temp.path())).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert!(result.projects[0].path.ends_with("pkg"));
        assert!(result.projects[0].workspace_root.is_none());
    }

    #[test]
    fn trawl_expands_cargo_workspace_members() {
        let temp = TempDir::new().unwrap();

        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        write_pkg(&temp.path().join("crates/a"));
        write_pkg(&temp.path().join("crates/b"));
        // Not a crate: no manifest -> must not be reported as a member.
        std::fs::create_dir_all(temp.path().join("crates/notacrate")).unwrap();

        let result = trawl(&opts(temp.path())).unwrap();

        let roots = result
            .projects
            .iter()
            .filter(|p| p.workspace_root.is_none())
            .collect::<Vec<_>>();
        let members = result
            .projects
            .iter()
            .filter(|p| p.workspace_root.is_some())
            .collect::<Vec<_>>();

        assert_eq!(roots.len(), 1, "exactly one workspace root");
        assert_eq!(
            roots[0].cargo_kind,
            Some(CargoManifestKind::Workspace),
            "root is a virtual workspace"
        );
        assert_eq!(members.len(), 2, "two member crates reported");
        for m in &members {
            assert_eq!(m.workspace_root.as_deref(), Some(roots[0].path.as_path()));
        }
        // No project may come from the empty dir.
        assert!(result
            .projects
            .iter()
            .all(|p| !p.path.ends_with("notacrate")));
    }

    #[test]
    fn trawl_blacklist_skips_dir() {
        let temp = TempDir::new().unwrap();
        write_pkg(&temp.path().join("keep"));
        write_pkg(&temp.path().join("skipme"));

        let mut options = opts(temp.path());
        options.blacklist = vec!["skipme".to_string()];

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert!(result.projects[0].path.ends_with("keep"));
    }

    #[test]
    fn trawl_blacklist_glob_skips_relative_path() {
        let temp = TempDir::new().unwrap();
        write_pkg(&temp.path().join("vendor/legacy"));
        write_pkg(&temp.path().join("src"));

        let mut options = opts(temp.path());
        options.blacklist = vec!["vendor/*".to_string()];

        let result = trawl(&options).unwrap();
        assert!(result
            .projects
            .iter()
            .all(|p| !p.path.to_string_lossy().contains("vendor")));
    }

    #[test]
    fn trawl_respects_gitignore() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(".gitignore"), "ignored-dir/\n").unwrap();
        write_pkg(&temp.path().join("ignored-dir/hidden-project"));
        write_pkg(&temp.path().join("visible-project"));

        // Default: ignore files honored -> ignored-dir is pruned.
        let result = trawl(&opts(temp.path())).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert!(result.projects[0].path.ends_with("visible-project"));

        // With ignore files disabled, the previously-ignored project surfaces.
        let mut options = opts(temp.path());
        options.respect_ignore_files = false;
        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 2);
    }
}
