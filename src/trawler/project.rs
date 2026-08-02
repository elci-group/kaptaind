use std::fs;
use std::path::{Path, PathBuf};

/// Built-in directory names that are never themselves project roots and are not
/// worth descending into while trawling. This is the default blacklist; users can
/// extend it via `TrawlOptions::blacklist` / `--blacklist`.
///
/// Keep this list to *directory basenames* only. Glob/relative-path rules belong in
/// the user blacklist or in `.gitignore`/`.ignore` files (honored by the walker).
pub const DEFAULT_SKIP_DIRS: &[&str] = &[
    // Version control
    ".git",
    ".hg",
    ".svn",
    ".bzr",
    // Build outputs & caches
    "target",
    "build",
    "dist",
    "out",
    "bin",
    "obj",
    "_build",
    "cmake-build-debug",
    "cmake-build-release",
    ".scons",
    ".scons_opt_cache",
    // Dependencies & virtual envs
    "node_modules",
    "__pycache__",
    ".gradle",
    "vendor",
    ".venv",
    "venv",
    "env",
    ".cargo",
    ".composer",
    "Pods",
    "DerivedData",
    ".yarn",
    ".npm",
    ".pnpm-store",
    "site-packages",
    ".eggs",
    ".eggs-info",
    "ebin",
    // Editor & IDE
    ".idea",
    ".vscode",
    ".vscode-server",
    ".vs",
    ".sublime-text",
    ".atom",
    // Language tooling caches
    ".next",
    ".output",
    ".turbo",
    ".sbtserver",
    ".elixir_ls",
    ".erlang_ls",
    ".metals",
    ".bloop",
    ".mix",
    ".rebar3",
    // Testing & coverage
    ".tox",
    ".pytest_cache",
    ".mypy_cache",
    ".coverage",
    ".nyc_output",
    "coverage",
    ".rcov",
    // OS junk
    ".DS_Store",
    ".AppleDouble",
    ".LSOverride",
    "Thumbs.db",
    "$RECYCLE.BIN",
    ".directory",
    // Documentation builds
    "docs",
    "doc",
    "site",
    "_site",
    "gh-pages",
    // Temp / caches
    ".tmp",
    ".cache",
    ".temp",
    "tmp",
    "temp",
    // Kaptaind
    ".kaptaind",
];

/// Represents the type of project detected in a directory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Swift,
    Kotlin,
    Java,
    Ruby,
    Elixir,
    Php,
    Dotnet,
    Cpp,
    Lua,
    Scala,
    Clojure,
    Haskell,
    Julia,
    R,
    Perl,
    Unknown,
}

/// Detection confidence level (ordered from Low to VeryHigh)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// Low confidence (weak indicators only)
    Low,
    /// Medium confidence (primary marker only)
    Medium,
    /// High confidence (primary marker + context clues)
    High,
    /// Very high confidence (multiple strong indicators)
    VeryHigh,
}

impl Confidence {
    pub fn score(&self) -> f32 {
        match self {
            Confidence::VeryHigh => 0.95,
            Confidence::High => 0.80,
            Confidence::Medium => 0.60,
            Confidence::Low => 0.40,
        }
    }
}

/// Result of project type detection
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub project_type: ProjectType,
    pub confidence: Confidence,
    pub indicators: Vec<String>,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust"),
            ProjectType::Node => write!(f, "Node.js"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Swift => write!(f, "Swift"),
            ProjectType::Kotlin => write!(f, "Kotlin"),
            ProjectType::Java => write!(f, "Java"),
            ProjectType::Ruby => write!(f, "Ruby"),
            ProjectType::Elixir => write!(f, "Elixir"),
            ProjectType::Php => write!(f, "PHP"),
            ProjectType::Dotnet => write!(f, ".NET"),
            ProjectType::Cpp => write!(f, "C++"),
            ProjectType::Lua => write!(f, "Lua"),
            ProjectType::Scala => write!(f, "Scala"),
            ProjectType::Clojure => write!(f, "Clojure"),
            ProjectType::Haskell => write!(f, "Haskell"),
            ProjectType::Julia => write!(f, "Julia"),
            ProjectType::R => write!(f, "R"),
            ProjectType::Perl => write!(f, "Perl"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

impl ProjectType {
    /// Get the primary marker files that identify this project type
    pub fn marker_files(&self) -> &'static [&'static str] {
        match self {
            ProjectType::Rust => &["Cargo.toml"],
            ProjectType::Node => &["package.json"],
            ProjectType::Python => &["pyproject.toml", "setup.py", "requirements.txt", "Pipfile"],
            ProjectType::Go => &["go.mod"],
            ProjectType::Swift => &["Package.swift"],
            ProjectType::Kotlin => &["build.gradle.kts"],
            ProjectType::Java => &["pom.xml"],
            ProjectType::Ruby => &["Gemfile"],
            ProjectType::Elixir => &["mix.exs"],
            ProjectType::Php => &["composer.json"],
            ProjectType::Dotnet => &["*.csproj", "*.fsproj", "*.vbproj"],
            ProjectType::Cpp => &["CMakeLists.txt"],
            ProjectType::Lua => &["*.lua", "*.rockspec"],
            ProjectType::Scala => &["build.sbt"],
            ProjectType::Clojure => &["project.clj", "deps.edn"],
            ProjectType::Haskell => &["*.cabal", "stack.yaml"],
            ProjectType::Julia => &["Project.toml"],
            ProjectType::R => &["DESCRIPTION", "NAMESPACE"],
            ProjectType::Perl => &["Makefile.PL", "Build.PL"],
            ProjectType::Unknown => &[],
        }
    }

    /// Get secondary indicators to increase confidence
    pub fn secondary_indicators(&self) -> &'static [&'static str] {
        match self {
            ProjectType::Rust => &["Cargo.lock", "src/main.rs", "src/lib.rs"],
            ProjectType::Node => &[
                "package-lock.json",
                "yarn.lock",
                "pnpm-lock.yaml",
                "node_modules",
                "src/index.js",
                "src/index.ts",
            ],
            ProjectType::Python => &["__pycache__", "*.py", ".venv", "venv", "tox.ini"],
            ProjectType::Go => &["go.sum", "main.go", "go.mod"],
            ProjectType::Swift => &["Sources", "*.swift", "Package.resolved"],
            ProjectType::Kotlin => &["src/main/kotlin", "build.gradle", "gradlew"],
            ProjectType::Java => &["build.gradle", "src/main/java", "mvnw", "*.jar"],
            ProjectType::Ruby => &["Gemfile.lock", "*.rb", "config.ru"],
            ProjectType::Elixir => &["mix.lock", "lib/*.ex"],
            ProjectType::Php => &["composer.lock", "index.php", "public/index.php"],
            ProjectType::Dotnet => &["*.sln", "bin/", "obj/", "packages.config"],
            ProjectType::Cpp => &["Makefile", "CMakeLists.txt", "*.cpp", "*.h", "meson.build"],
            ProjectType::Lua => &["*.lua", "rocks/"],
            ProjectType::Scala => &["project/", "*.scala"],
            ProjectType::Clojure => &["src/clj", "*.clj"],
            ProjectType::Haskell => &["src/", "*.hs", "cabal.project"],
            ProjectType::Julia => &["src/*.jl"],
            ProjectType::R => &["R/", "data/", "DESCRIPTION"],
            ProjectType::Perl => &["lib/", "*.pl", "t/"],
            ProjectType::Unknown => &[],
        }
    }

    /// Get the test command for this project type
    pub fn test_command(&self) -> &'static str {
        match self {
            ProjectType::Rust => "cargo test",
            ProjectType::Node => "npm test",
            ProjectType::Python => "pytest",
            ProjectType::Go => "go test ./...",
            ProjectType::Swift => "swift test",
            ProjectType::Kotlin => "./gradlew test",
            ProjectType::Java => "./mvnw test",
            ProjectType::Ruby => "bundle exec rspec",
            ProjectType::Elixir => "mix test",
            ProjectType::Php => "composer test",
            ProjectType::Dotnet => "dotnet test",
            ProjectType::Cpp => "make test",
            ProjectType::Lua => "busted",
            ProjectType::Scala => "sbt test",
            ProjectType::Clojure => "lein test",
            ProjectType::Haskell => "cabal test",
            ProjectType::Julia => "julia --project runtests.jl",
            ProjectType::R => "R CMD check",
            ProjectType::Perl => "prove t/",
            ProjectType::Unknown => "echo 'no test command configured'",
        }
    }

    /// Get the build command for this project type (if applicable)
    pub fn build_command(&self) -> Option<&'static str> {
        match self {
            ProjectType::Rust => Some("cargo build --release"),
            ProjectType::Node => Some("npm run build"),
            ProjectType::Python => Some("python -m build"),
            ProjectType::Go => Some("go build"),
            ProjectType::Swift => Some("swift build"),
            ProjectType::Kotlin => Some("./gradlew build"),
            ProjectType::Java => Some("./mvnw package"),
            ProjectType::Ruby => None,
            ProjectType::Elixir => Some("mix compile"),
            ProjectType::Php => None,
            ProjectType::Dotnet => Some("dotnet build"),
            ProjectType::Cpp => Some("make"),
            ProjectType::Lua => Some("luarocks make"),
            ProjectType::Scala => Some("sbt package"),
            ProjectType::Clojure => Some("lein uberjar"),
            ProjectType::Haskell => Some("cabal build"),
            ProjectType::Julia => Some("julia --project -e 'using Pkg; Pkg.build()'"),
            ProjectType::R => Some("R CMD build"),
            ProjectType::Perl => Some("perl Makefile.PL && make"),
            ProjectType::Unknown => None,
        }
    }

    /// Get default ignore patterns for this project type
    pub fn ignore_patterns(&self) -> &'static [&'static str] {
        match self {
            ProjectType::Rust => &["# Rust", "target", "Cargo.lock"],
            ProjectType::Node => &[
                "# Node.js",
                "node_modules",
                ".next",
                "dist",
                "build",
                ".turbo",
                ".vercel",
                ".output",
                "coverage",
                ".yarn",
            ],
            ProjectType::Python => &[
                "# Python",
                "__pycache__",
                ".venv",
                ".pytest_cache",
                "*.egg-info",
                "dist",
                "*.pyc",
                ".tox",
                ".mypy_cache",
            ],
            ProjectType::Go => &["# Go", "vendor"],
            ProjectType::Swift => &[
                "# Swift",
                ".build",
                "DerivedData",
                "*.xcodeproj/xcuserdata",
                "*.xcworkspace/xcuserdata",
                "Pods",
            ],
            ProjectType::Kotlin | ProjectType::Java => &[
                "# Java/Kotlin/Gradle",
                "build",
                ".gradle",
                "*.iml",
                ".idea",
                "local.properties",
            ],
            ProjectType::Ruby => &["# Ruby", "vendor/bundle", ".bundle", "*.gem"],
            ProjectType::Elixir => &["# Elixir", "_build", "deps", ".elixir_ls"],
            ProjectType::Php => &["# PHP", "vendor", "composer.lock"],
            ProjectType::Dotnet => &["# .NET", "bin", "obj", "*.user"],
            ProjectType::Cpp => &[
                "# C++",
                "build",
                "cmake-build-*",
                "*.o",
                "*.a",
                "*.so",
                "*.exe",
            ],
            ProjectType::Lua => &["# Lua", "rocks/", "luarocks.lock"],
            ProjectType::Scala => &["# Scala", "target/", ".sbtserver"],
            ProjectType::Clojure => &["# Clojure", "target/", ".lein-repl-history"],
            ProjectType::Haskell => &[
                "# Haskell",
                "dist/",
                "dist-newstyle/",
                "cabal.project.local",
            ],
            ProjectType::Julia => &["# Julia", "Manifest.toml"],
            ProjectType::R => &["# R", ".Rhistory", ".RData"],
            ProjectType::Perl => &["# Perl", "blib/", "MANIFEST.bak", "pm_to_blib"],
            ProjectType::Unknown => &[],
        }
    }
}

/// Detect the project type with confidence scoring
pub fn detect_project_type_with_confidence(path: &Path) -> DetectionResult {
    let mut best_result = DetectionResult {
        project_type: ProjectType::Unknown,
        confidence: Confidence::Low,
        indicators: Vec::new(),
    };

    // Try each project type and collect results
    let all_types = vec![
        ProjectType::Rust,
        ProjectType::Node,
        ProjectType::Python,
        ProjectType::Go,
        ProjectType::Swift,
        ProjectType::Kotlin,
        ProjectType::Java,
        ProjectType::Ruby,
        ProjectType::Elixir,
        ProjectType::Php,
        ProjectType::Dotnet,
        ProjectType::Cpp,
        ProjectType::Lua,
        ProjectType::Scala,
        ProjectType::Clojure,
        ProjectType::Haskell,
        ProjectType::Julia,
        ProjectType::R,
        ProjectType::Perl,
    ];

    for project_type in all_types {
        if let Some(result) = check_project_type(path, project_type) {
            if result.confidence > best_result.confidence {
                best_result = result;
            }
        }
    }

    best_result
}

/// Check if a path matches a specific project type with confidence
fn check_project_type(path: &Path, proj_type: ProjectType) -> Option<DetectionResult> {
    let mut indicators = Vec::new();
    let mut score = 0.0;

    // Check primary markers
    let primary_markers = proj_type.marker_files();
    let mut primary_match = false;
    for marker in primary_markers {
        if is_marker_present(path, marker) {
            primary_match = true;
            indicators.push(format!("Primary marker: {}", marker));
            score += 0.6;
        }
    }

    if !primary_match {
        return None; // Must have at least one primary marker
    }

    // Check secondary indicators
    let secondary_markers = proj_type.secondary_indicators();
    let mut secondary_matches = 0;
    for marker in secondary_markers {
        if is_marker_present(path, marker) {
            secondary_matches += 1;
            indicators.push(format!("Secondary indicator: {}", marker));
        }
    }

    // Boost score based on secondary matches
    if secondary_matches >= 2 {
        score += 0.25;
    } else if secondary_matches == 1 {
        score += 0.15;
    }

    // Check for monorepo patterns (should reduce confidence for nested projects)
    if is_monorepo_root(path, proj_type) {
        score += 0.10;
        indicators.push("Monorepo root detected".to_string());
    }

    // Determine confidence level
    let confidence = match score {
        s if s >= 0.85 => Confidence::VeryHigh,
        s if s >= 0.70 => Confidence::High,
        s if s >= 0.55 => Confidence::Medium,
        _ => Confidence::Low,
    };

    if confidence > Confidence::Low {
        Some(DetectionResult {
            project_type: proj_type,
            confidence,
            indicators,
        })
    } else {
        None
    }
}

/// Backward compatible function
pub fn detect_project_type(path: &Path) -> ProjectType {
    detect_project_type_with_confidence(path).project_type
}

/// Check if a marker file/pattern exists in the directory
fn is_marker_present(path: &Path, marker: &str) -> bool {
    if marker.contains('*') || marker.contains('?') {
        has_glob_match(path, marker)
    } else {
        path.join(marker).exists()
    }
}

/// Check if directory is a monorepo root (Cargo workspace, pnpm-workspace, lerna, etc.)
fn is_monorepo_root(path: &Path, proj_type: ProjectType) -> bool {
    match proj_type {
        ProjectType::Rust => {
            // Check for Cargo workspace
            if let Ok(content) = fs::read_to_string(path.join("Cargo.toml")) {
                content.contains("[workspace]")
            } else {
                false
            }
        }
        ProjectType::Node => {
            // Check for pnpm workspace
            path.join("pnpm-workspace.yaml").exists()
                || path.join("lerna.json").exists()
                || path.join("workspaces").exists()
        }
        ProjectType::Python => {
            // Check for monorepo patterns
            path.join("pyproject.toml").exists() && path.join("packages").exists()
        }
        _ => false,
    }
}

/// Check if a directory contains any files matching a glob pattern
fn has_glob_match(path: &Path, pattern: &str) -> bool {
    use glob::Pattern;

    let pattern = match Pattern::new(pattern) {
        Ok(p) => p,
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "has_glob_match",
                source_line = line!(),
                "has glob match returned an error"
            );
            return false;
        }
    };

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if pattern.matches(name) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if a directory is a valid git repository
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").is_dir()
}

/// Check if a directory already has kaptaind initialized
pub fn is_kaptaind_initialized(path: &Path) -> bool {
    path.join("kaptaind.toml").exists()
}

/// Check if a directory should be skipped during trawling using the built-in
/// default blacklist (`DEFAULT_SKIP_DIRS`).
///
/// This is the backward-compatible entry point. For user-supplied blacklists use
/// `is_blacklisted` together with `DEFAULT_SKIP_DIRS`.
pub fn should_skip_directory(dir_name: &str) -> bool {
    DEFAULT_SKIP_DIRS.contains(&dir_name) || dir_name.starts_with("cmake-build-")
}

/// Check whether a directory is excluded by the built-in skip list or a user-supplied
/// blacklist.
///
/// - `dir_name`: the directory's basename (e.g. `"target"`).
/// - `rel_path`: the directory's path relative to the trawl root (e.g.
///   `"vendor/legacy"`); used for glob blacklist entries.
/// - `blacklist`: compiled user globs. An entry matches if it matches the basename
///   **or** the relative path.
pub fn is_blacklisted(dir_name: &str, rel_path: &Path, blacklist: &[globset::Glob]) -> bool {
    if should_skip_directory(dir_name) {
        return true;
    }
    blacklist.iter().any(|g| {
        let m = g.compile_matcher();
        m.is_match(dir_name) || m.is_match(rel_path)
    })
}

/// Classification of a `Cargo.toml` manifest found in a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoManifestKind {
    /// A regular package (`[package]` only).
    Package,
    /// A virtual workspace root (`[workspace]` only, no `[package]`).
    Workspace,
    /// A workspace root that is also a package (`[package]` + `[workspace]`).
    PackageAndWorkspace,
    /// No `Cargo.toml`, unparseable, or contains neither `[package]` nor `[workspace]`.
    Invalid,
}

impl CargoManifestKind {
    pub fn is_valid(self) -> bool {
        !matches!(self, CargoManifestKind::Invalid)
    }

    pub fn is_workspace(self) -> bool {
        matches!(
            self,
            CargoManifestKind::Workspace | CargoManifestKind::PackageAndWorkspace
        )
    }
}

/// Inspect the `Cargo.toml` in `dir` and classify it.
///
/// SOTA detection requires *parsing* the manifest rather than checking existence: a
/// stray or malformed `Cargo.toml` (or one from a non-Rust tool) must not register as
/// a Rust project. A directory is a Rust root iff its manifest parses and contains a
/// `[package]` and/or `[workspace]` table.
pub fn inspect_cargo_manifest(dir: &Path) -> CargoManifestKind {
    let manifest_path = dir.join("Cargo.toml");
    let content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "inspect_cargo_manifest",
                source_line = line!(),
                "inspect cargo manifest returned an error"
            );
            return CargoManifestKind::Invalid;
        }
    };
    let value: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "inspect_cargo_manifest",
                source_line = line!(),
                "inspect cargo manifest returned an error"
            );
            return CargoManifestKind::Invalid;
        }
    };

    let has_package = value.get("package").and_then(|v| v.as_table()).is_some();
    let has_workspace = value.get("workspace").and_then(|v| v.as_table()).is_some();

    match (has_package, has_workspace) {
        (true, true) => CargoManifestKind::PackageAndWorkspace,
        (true, false) => CargoManifestKind::Package,
        (false, true) => CargoManifestKind::Workspace,
        (false, false) => CargoManifestKind::Invalid,
    }
}

/// Resolve the member crate directories of a Cargo workspace root.
///
/// Reads `[workspace].members` (glob list) and honors `[workspace].exclude`. Only
/// member paths that are directories containing a *valid* manifest are returned, so
/// glob entries like `"crates/*"` that match non-crate dirs are filtered out. Returns
/// an empty vec if `root` is not a workspace.
pub fn workspace_members(root: &Path) -> Vec<PathBuf> {
    if !inspect_cargo_manifest(root).is_workspace() {
        return Vec::new();
    }

    let content = match fs::read_to_string(root.join("Cargo.toml")) {
        Ok(c) => c,
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "workspace_members",
                source_line = line!(),
                "workspace members returned an error"
            );
            return Vec::new();
        }
    };
    let value: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "workspace_members",
                source_line = line!(),
                "workspace members returned an error"
            );
            return Vec::new();
        }
    };

    let workspace = match value.get("workspace").and_then(|v| v.as_table()) {
        Some(w) => w,
        None => return Vec::new(),
    };

    let member_patterns = string_array(workspace.get("members"));
    let exclude_patterns = string_array(workspace.get("exclude"));

    let mut members = Vec::new();
    for pattern in member_patterns {
        let full = root.join(&pattern);
        let full_str = match full.to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let paths = match glob::glob(&full_str) {
            Ok(p) => p,
            Err(error) => {
                tracing::error!(
                    ?error,
                    operation = "workspace_members",
                    source_line = line!(),
                    "workspace members returned an error"
                );
                continue;
            }
        };
        for entry in paths.flatten() {
            if !entry.is_dir() {
                continue;
            }
            if !inspect_cargo_manifest(&entry).is_valid() {
                continue;
            }
            // Honor [workspace].exclude by matching against the path relative to root.
            if let Ok(rel) = entry.strip_prefix(root) {
                if exclude_patterns.iter().any(|ex| {
                    globset::Glob::new(ex)
                        .map(|g| g.compile_matcher().is_match(rel))
                        .unwrap_or(false)
                }) {
                    continue;
                }
            }
            members.push(entry);
        }
    }

    members.sort();
    members.dedup();
    members
}

/// Extract a `Vec<String>` from a TOML array value, tolerating missing/non-array values.
fn string_array(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_rust_project() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect_project_type(temp.path()), ProjectType::Rust);
    }

    #[test]
    fn detect_rust_with_high_confidence() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(temp.path().join("Cargo.lock"), "").unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/main.rs"), "").unwrap();

        let result = detect_project_type_with_confidence(temp.path());
        assert_eq!(result.project_type, ProjectType::Rust);
        assert!(result.confidence >= Confidence::High);
    }

    #[test]
    fn detect_node_project() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
        assert_eq!(detect_project_type(temp.path()), ProjectType::Node);
    }

    #[test]
    fn detect_node_with_very_high_confidence() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
        std::fs::write(temp.path().join("package-lock.json"), "").unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/index.js"), "").unwrap();

        let result = detect_project_type_with_confidence(temp.path());
        assert_eq!(result.project_type, ProjectType::Node);
        assert_eq!(result.confidence, Confidence::VeryHigh);
    }

    #[test]
    fn detect_python_project() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("pyproject.toml"), "[build-system]").unwrap();
        assert_eq!(detect_project_type(temp.path()), ProjectType::Python);
    }

    #[test]
    fn detect_go_project() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("go.mod"), "module example.com").unwrap();
        assert_eq!(detect_project_type(temp.path()), ProjectType::Go);
    }

    #[test]
    fn detect_elixir_project() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("mix.exs"), "defmodule Test do end").unwrap();
        assert_eq!(detect_project_type(temp.path()), ProjectType::Elixir);
    }

    #[test]
    fn detect_clojure_project() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("project.clj"), "(defproject test)").unwrap();
        assert_eq!(detect_project_type(temp.path()), ProjectType::Clojure);
    }

    #[test]
    fn detect_unknown_project() {
        let temp = TempDir::new().unwrap();
        assert_eq!(detect_project_type(temp.path()), ProjectType::Unknown);
    }

    #[test]
    fn is_git_repo_detects_dot_git() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".git")).unwrap();
        assert!(is_git_repo(temp.path()));
    }

    #[test]
    fn is_kaptaind_initialized_detects_toml() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("kaptaind.toml"), "").unwrap();
        assert!(is_kaptaind_initialized(temp.path()));
    }

    #[test]
    fn should_skip_common_directories() {
        assert!(should_skip_directory("node_modules"));
        assert!(should_skip_directory("target"));
        assert!(should_skip_directory(".git"));
        assert!(should_skip_directory("__pycache__"));
        assert!(should_skip_directory("_build"));
        assert!(should_skip_directory(".venv"));
        assert!(!should_skip_directory("src"));
        assert!(!should_skip_directory("my-project"));
    }

    #[test]
    fn confidence_scoring() {
        assert!(Confidence::VeryHigh.score() > Confidence::High.score());
        assert!(Confidence::High.score() > Confidence::Medium.score());
        assert!(Confidence::Medium.score() > Confidence::Low.score());
    }

    #[test]
    fn detect_cargo_workspace() {
        let temp = TempDir::new().unwrap();
        let workspace_toml = r#"
[workspace]
members = ["foo", "bar"]

[package]
name = "root"
"#;
        std::fs::write(temp.path().join("Cargo.toml"), workspace_toml).unwrap();
        let result = detect_project_type_with_confidence(temp.path());
        assert_eq!(result.project_type, ProjectType::Rust);
        assert!(result.indicators.iter().any(|i| i.contains("Monorepo")));
    }

    #[test]
    fn inspect_cargo_manifest_variants() {
        // Plain package
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert_eq!(
            inspect_cargo_manifest(temp.path()),
            CargoManifestKind::Package
        );

        // Virtual workspace
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"*\"]\n",
        )
        .unwrap();
        let kind = inspect_cargo_manifest(temp.path());
        assert_eq!(kind, CargoManifestKind::Workspace);
        assert!(kind.is_workspace() && kind.is_valid());

        // Package + workspace
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"r\"\n\n[workspace]\nmembers = []\n",
        )
        .unwrap();
        assert_eq!(
            inspect_cargo_manifest(temp.path()),
            CargoManifestKind::PackageAndWorkspace
        );

        // Empty / neither table
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "").unwrap();
        assert_eq!(
            inspect_cargo_manifest(temp.path()),
            CargoManifestKind::Invalid
        );

        // Malformed TOML
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "this is = not valid = =").unwrap();
        assert_eq!(
            inspect_cargo_manifest(temp.path()),
            CargoManifestKind::Invalid
        );

        // No manifest at all
        let temp = TempDir::new().unwrap();
        assert_eq!(
            inspect_cargo_manifest(temp.path()),
            CargoManifestKind::Invalid
        );
    }

    #[test]
    fn workspace_members_resolves_and_filters() {
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        let a = temp.path().join("crates/a");
        let b = temp.path().join("crates/b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("Cargo.toml"), "[package]\nname = \"a\"\n").unwrap();
        std::fs::write(b.join("Cargo.toml"), "[package]\nname = \"b\"\n").unwrap();
        // Not a crate: no manifest.
        std::fs::create_dir_all(temp.path().join("crates/empty")).unwrap();

        let members = workspace_members(temp.path());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&a));
        assert!(members.contains(&b));
    }

    #[test]
    fn workspace_members_honors_exclude() {
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/b\"]\n",
        )
        .unwrap();
        let a = temp.path().join("crates/a");
        let b = temp.path().join("crates/b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("Cargo.toml"), "[package]\nname = \"a\"\n").unwrap();
        std::fs::write(b.join("Cargo.toml"), "[package]\nname = \"b\"\n").unwrap();

        let members = workspace_members(temp.path());
        assert_eq!(members, vec![a]);
    }

    #[test]
    fn workspace_members_empty_for_non_workspace() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert!(workspace_members(temp.path()).is_empty());
    }

    #[test]
    fn is_blacklisted_matches_basename_and_rel_glob() {
        let scratch = globset::Glob::new("scratch").unwrap();
        assert!(is_blacklisted(
            "scratch",
            std::path::Path::new("scratch"),
            &[scratch]
        ));

        let vendor = globset::Glob::new("vendor/*").unwrap();
        assert!(is_blacklisted(
            "legacy",
            std::path::Path::new("vendor/legacy"),
            &[vendor]
        ));

        let none = globset::Glob::new("nope").unwrap();
        assert!(!is_blacklisted("src", std::path::Path::new("src"), &[none]));

        // Built-in skip list always applies, even with an empty user blacklist.
        assert!(is_blacklisted(
            "target",
            std::path::Path::new("target"),
            &[]
        ));
    }
}
