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
    if root.is_absolute() {
        Ok(root.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(root))
    }
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
            Err(e) => errors.push(format!("Invalid blacklist pattern {:?}: {}", trimmed, e)),
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
        .git_exclude(options.respect_ignore_files);

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
                Err(_) => return WalkState::Continue,
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

    match Arc::try_unwrap(found) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(shared) => shared.lock().map(|v| v.clone()).unwrap_or_default(),
    }
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


/// Recursively scan a directory for projects
fn scan_directory(
    path: &Path,
    depth: usize,
    options: &TrawlOptions,
    visited: &mut HashSet<PathBuf>,
    projects: &mut Vec<DiscoveredProject>,
    errors: &mut Vec<String>,
) -> anyhow::Result<()> {
    // Check depth limit
    if let Some(max) = options.max_depth {
        if depth > max {
            return Ok(());
        }
    }

    // Canonicalize to avoid symlink cycles
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            errors.push(format!("Failed to canonicalize {}: {}", path.display(), e));
            return Ok(());
        }
    };

    if !visited.insert(canonical.clone()) {
        return Ok(()); // Already visited
    }

    // Check if this directory is a project with confidence scoring
    let detection = detect_project_type_with_confidence(path);
    let is_git = is_git_repo(path);
    let is_initialized = is_kaptaind_initialized(path);

    let is_valid_project = match detection.project_type {
        ProjectType::Unknown => false,
        _ => {
            // Check if we're filtering by type
            if options.filter_types.is_empty() {
                true
            } else {
                options.filter_types.contains(&detection.project_type)
            }
        }
    };

    if is_valid_project && detection.confidence.score() >= options.min_confidence {
        // Check git requirement
        if !options.require_git || is_git {
            projects.push(DiscoveredProject {
                path: path.to_path_buf(),
                project_type: detection.project_type,
                confidence: detection.confidence,
                confidence_score: detection.confidence.score(),
                detection_indicators: detection.indicators,
                is_git_repo: is_git,
                is_initialized,
                depth,
            });

            // Don't recurse into project directories - each project is its own root
            return Ok(());
        }
    }

    // Recurse into subdirectories
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push(format!(
                "Failed to read directory {}: {}",
                path.display(),
                e
            ));
            return Ok(());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Failed to read entry in {}: {}", path.display(), e));
                continue;
            }
        };

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if !file_type.is_dir() {
            continue;
        }

        let dir_name = entry.file_name();
        let dir_name = dir_name.to_string_lossy();

        if should_skip_directory(&dir_name) {
            continue;
        }

        // Skip hidden directories (starting with .)
        if dir_name.starts_with('.') {
            continue;
        }

        let subdir = entry.path();

        if let Err(e) = scan_directory(&subdir, depth + 1, options, visited, projects, errors) {
            errors.push(format!("Error scanning {}: {}", subdir.display(), e));
        }
    }

    Ok(())
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
    use std::fmt::Write;

    let mut report = String::new();

    writeln!(report, "🔍 Trawling Complete").unwrap();
    writeln!(report, "====================").unwrap();
    writeln!(report).unwrap();

    writeln!(report, "📊 Summary:").unwrap();
    writeln!(report, "  Projects discovered: {}", result.projects.len()).unwrap();
    writeln!(report, "  Initialized: {}", result.initialized_count).unwrap();
    writeln!(report, "  Registered: {}", result.registered_count).unwrap();
    writeln!(
        report,
        "  Skipped (already initialized): {}",
        result.skipped_count
    )
    .unwrap();

    writeln!(report).unwrap();
    writeln!(report, "🎯 Detection Confidence:").unwrap();
    writeln!(
        report,
        "  Average confidence: {:.1}%",
        result.avg_confidence * 100.0
    )
    .unwrap();
    writeln!(
        report,
        "  Very high confidence (≥95%): {}",
        result.very_high_confidence_count
    )
    .unwrap();
    writeln!(
        report,
        "  High confidence (≥80%): {}",
        result.high_confidence_count
    )
    .unwrap();

    if !result.errors.is_empty() {
        writeln!(report, "  ⚠️  Errors: {}", result.errors.len()).unwrap();
    }

    if !result.projects.is_empty() {
        writeln!(report).unwrap();
        writeln!(report, "📦 Discovered Projects:").unwrap();

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

            writeln!(
                report,
                "  {} {} {:.0}% {} {} {}",
                git_marker,
                project.project_type,
                project.confidence_score * 100.0,
                confidence_bar,
                project.path.display(),
                status
            )
            .unwrap();

            // Show detection indicators for lower-confidence projects
            if project.confidence < Confidence::High && !project.detection_indicators.is_empty() {
                for indicator in &project.detection_indicators {
                    writeln!(report, "      └─ {}", indicator).unwrap();
                }
            }
        }
    }

    if !result.errors.is_empty() {
        writeln!(report).unwrap();
        writeln!(report, "❌ Errors:").unwrap();
        for error in &result.errors {
            writeln!(report, "  - {}", error).unwrap();
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn trawl_finds_rust_project() {
        let temp = TempDir::new().unwrap();

        // Create a Rust project
        let project_dir = temp.path().join("my-rust-app");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join("Cargo.toml"), "[package]").unwrap();

        let options = TrawlOptions {
            root: temp.path().to_path_buf(),
            max_depth: Some(3),
            skip_initialized: false,
            require_git: false,
            auto_register: false,
            filter_types: Vec::new(),
            min_confidence: 0.55,
        };

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Rust);
        assert!(result.projects[0].path.ends_with("my-rust-app"));
    }

    #[test]
    fn trawl_confidence_scoring() {
        let temp = TempDir::new().unwrap();

        // Create Rust project with multiple confidence indicators
        let project_dir = temp.path().join("rust-app");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(project_dir.join("Cargo.lock"), "").unwrap();
        std::fs::create_dir(project_dir.join("src")).unwrap();
        std::fs::write(project_dir.join("src/main.rs"), "fn main() {}").unwrap();

        let options = TrawlOptions {
            root: temp.path().to_path_buf(),
            max_depth: Some(3),
            skip_initialized: false,
            require_git: false,
            auto_register: false,
            filter_types: Vec::new(),
            min_confidence: 0.55,
        };

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert!(result.projects[0].confidence >= Confidence::High);
        assert!(result.avg_confidence >= 0.80);
    }

    #[test]
    fn trawl_skips_initialized_when_configured() {
        let temp = TempDir::new().unwrap();

        // Create a Rust project with kaptaind.toml
        let project_dir = temp.path().join("existing-project");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(project_dir.join("kaptaind.toml"), "").unwrap();

        let options = TrawlOptions {
            root: temp.path().to_path_buf(),
            max_depth: Some(3),
            skip_initialized: true,
            require_git: false,
            auto_register: false,
            filter_types: Vec::new(),
            min_confidence: 0.55,
        };

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.initialized_count, 0);
    }

    #[test]
    fn trawl_respects_max_depth() {
        let temp = TempDir::new().unwrap();

        // Create nested projects
        let level1 = temp.path().join("level1");
        std::fs::create_dir(&level1).unwrap();
        std::fs::write(level1.join("Cargo.toml"), "").unwrap();

        let level2 = level1.join("level2");
        std::fs::create_dir(&level2).unwrap();
        std::fs::write(level2.join("package.json"), "{}").unwrap();

        let options = TrawlOptions {
            root: temp.path().to_path_buf(),
            max_depth: Some(1),
            skip_initialized: false,
            require_git: false,
            auto_register: false,
            filter_types: Vec::new(),
            min_confidence: 0.55,
        };

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1); // Only level1
    }

    #[test]
    fn trawl_filters_by_project_type() {
        let temp = TempDir::new().unwrap();

        // Create Rust project
        let rust_dir = temp.path().join("rust-project");
        std::fs::create_dir(&rust_dir).unwrap();
        std::fs::write(rust_dir.join("Cargo.toml"), "").unwrap();

        // Create Node project
        let node_dir = temp.path().join("node-project");
        std::fs::create_dir(&node_dir).unwrap();
        std::fs::write(node_dir.join("package.json"), "{}").unwrap();

        let options = TrawlOptions {
            root: temp.path().to_path_buf(),
            max_depth: Some(3),
            skip_initialized: false,
            require_git: false,
            auto_register: false,
            filter_types: vec![ProjectType::Rust],
            min_confidence: 0.55,
        };

        let result = trawl(&options).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Rust);
    }
}
