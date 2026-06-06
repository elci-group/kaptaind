use std::fs;
use std::path::Path;

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
        Err(_) => return false,
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

/// Check if a directory should be skipped during trawling
pub fn should_skip_directory(dir_name: &str) -> bool {
    // Skip common non-project directories
    const SKIP_DIRS: &[&str] = &[
        // Git & version control
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
        "node_modules",
        "__pycache__",
        ".gradle",
        "vendor",
        ".next",
        ".output",
        ".turbo",
        ".sbtserver",
        // Editor & IDE
        ".idea",
        ".vscode",
        ".vscode-server",
        ".vs",
        ".sublime-text",
        ".atom",
        // Dependencies & virtual envs
        ".venv",
        "venv",
        "env",
        "vendor/bundle",
        "vendor",
        ".cargo",
        ".composer",
        "Pods",
        "DerivedData",
        "node_modules",
        ".yarn",
        ".npm",
        ".pnpm-store",
        // Testing & coverage
        ".tox",
        ".pytest_cache",
        ".mypy_cache",
        ".coverage",
        ".nyc_output",
        "coverage",
        ".rcov",
        // Language-specific
        ".elixir_ls",
        ".erlang_ls",
        ".metals",
        ".bloop",
        "_build",
        "deps",
        ".mix",
        ".rebar3",
        "ebin",
        "site-packages",
        ".eggs",
        ".eggs-info",
        // OS
        ".DS_Store",
        ".AppleDouble",
        ".LSOverride",
        "Thumbs.db",
        "$RECYCLE.BIN",
        ".directory",
        // Build systems
        "cmake-build-debug",
        "cmake-build-release",
        "cmake-build-*",
        ".scons",
        ".scons_opt_cache",
        // Documentation
        "docs",
        "doc",
        "site",
        "_site",
        "gh-pages",
        // Archives & misc
        ".tmp",
        ".cache",
        ".temp",
        "tmp",
        "temp",
        // Kaptaind
        ".kaptaind",
    ];

    SKIP_DIRS.contains(&dir_name) || dir_name.starts_with("cmake-build-")
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
}
