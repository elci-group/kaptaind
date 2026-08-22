use crate::diff_version::cache::VersionCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Source priority for version detection.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, PartialOrd, Eq, Ord)]
#[serde(rename_all = "snake_case")]
pub enum VersionSource {
    /// Version came from the actual runtime (e.g., `node --version`).
    Runtime,
    /// Version came from a manifest file (e.g., Cargo.toml, package.json).
    Manifest,
    /// Version was inferred or defaulted.
    #[default]
    Inferred,
}

/// Detected version for a single language in this project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageVersion {
    /// Human-readable version string: "2021", "1.21", "3.10", "5.0", etc.
    pub version: String,
    /// Which file the version was read from.
    pub detected_from: String,
    /// Source priority (runtime > manifest > inferred).
    #[serde(default)]
    pub source: VersionSource,
}

impl LanguageVersion {
    pub fn unknown() -> Self {
        Self {
            version: "unknown".to_string(),
            detected_from: "none".to_string(),
            source: VersionSource::Inferred,
        }
    }
}

/// Detect all language versions for the project at `repo_path`.
///
/// Results are read from the version cache when valid (TTL = 1 h); detected
/// versions are written back to the cache before returning.
pub fn detect_all(cache: &mut VersionCache, repo_path: &Path) -> HashMap<String, LanguageVersion> {
    let mut map = HashMap::new();

    type DetectorFn = fn(&Path) -> Option<LanguageVersion>;
    let detectors: &[(&str, DetectorFn)] = &[
        ("rust", detect_rust),
        ("go", detect_go),
        ("python", detect_python),
        ("typescript", detect_typescript),
        ("javascript", detect_javascript),
        ("kotlin", detect_kotlin),
        ("swift", detect_swift),
        ("vue", detect_vue),
        ("svelte", detect_svelte),
        ("astro", detect_astro),
        ("scss", |_| None),
        ("htmlcss", |_| None),
    ];

    for &(lang_key, detector_fn) in detectors {
        // Try cache first
        if let Some(cached) = cache.get(lang_key) {
            map.insert(lang_key.to_string(), cached);
            continue;
        }
        // Run detector
        if let Some(lv) = detector_fn(repo_path) {
            cache.put(lang_key, &lv);
            map.insert(lang_key.to_string(), lv);
        }
    }

    map
}

// ---------------------------------------------------------------------------
// Per-language detectors
// ---------------------------------------------------------------------------

fn detect_rust(repo_path: &Path) -> Option<LanguageVersion> {
    let cargo = repo_path.join("Cargo.toml");
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let content = std::fs::read_to_string(&cargo).ok()?;
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let doc: toml::Value = toml::from_str(&content).ok()?;
    let edition = doc
        .get("package")
        .and_then(|p| p.get("edition"))
        .and_then(|e| e.as_str())
        .unwrap_or("2015");
    Some(LanguageVersion {
        version: edition.to_string(),
        detected_from: "Cargo.toml".to_string(),
        source: VersionSource::Manifest,
    })
}

fn detect_go(repo_path: &Path) -> Option<LanguageVersion> {
    let go_mod = repo_path.join("go.mod");
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let content = std::fs::read_to_string(&go_mod).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(ver) = trimmed.strip_prefix("go ") {
            return Some(LanguageVersion {
                version: ver.trim().to_string(),
                detected_from: "go.mod".to_string(),
                source: VersionSource::Manifest,
            });
        }
    }
    None
}

fn detect_python(repo_path: &Path) -> Option<LanguageVersion> {
    // 1. .python-version (pyenv)
    if let Ok(content) = std::fs::read_to_string(repo_path.join(".python-version")) {
        let ver = content.trim().to_string();
        if !ver.is_empty() {
            return Some(LanguageVersion {
                version: normalise_python_version(&ver),
                detected_from: ".python-version".to_string(),
                source: VersionSource::Manifest,
            });
        }
    }

    // 2. pyproject.toml requires-python
    if let Ok(content) = std::fs::read_to_string(repo_path.join("pyproject.toml")) {
        if let Ok(doc) = toml::from_str::<toml::Value>(&content) {
            let req = doc
                .get("project")
                .and_then(|p| p.get("requires-python"))
                .and_then(|v| v.as_str());
            if let Some(req) = req {
                // e.g. ">=3.10" → "3.10"
                let ver = req.trim_start_matches(|c: char| !c.is_ascii_digit());
                if !ver.is_empty() {
                    return Some(LanguageVersion {
                        version: normalise_python_version(ver),
                        detected_from: "pyproject.toml".to_string(),
                        source: VersionSource::Manifest,
                    });
                }
            }
        }
    }

    // 3. setup.cfg python_requires
    if let Ok(content) = std::fs::read_to_string(repo_path.join("setup.cfg")) {
        for line in content.lines() {
            if let Some(rest) = line.trim().strip_prefix("python_requires") {
                let ver = rest.trim_start_matches(['=', ' ', '>']);
                if !ver.is_empty() {
                    return Some(LanguageVersion {
                        version: normalise_python_version(ver.trim()),
                        detected_from: "setup.cfg".to_string(),
                        source: VersionSource::Manifest,
                    });
                }
            }
        }
    }

    None
}

/// Reduce "3.10.1" → "3.10", "3.9" → "3.9".
fn normalise_python_version(raw: &str) -> String {
    let parts: Vec<&str> = raw.splitn(3, '.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        raw.to_string()
    }
}

fn detect_typescript(repo_path: &Path) -> Option<LanguageVersion> {
    // tsconfig.json compilerOptions.target
    let tsconfig = repo_path.join("tsconfig.json");
    if let Ok(content) = std::fs::read_to_string(&tsconfig) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(target) = val
                .get("compilerOptions")
                .and_then(|c| c.get("target"))
                .and_then(|t| t.as_str())
            {
                return Some(LanguageVersion {
                    version: target.to_uppercase(),
                    detected_from: "tsconfig.json".to_string(),
                    source: VersionSource::Manifest,
                });
            }
        }
    }

    // Fallback: read typescript version from package.json devDependencies
    detect_npm_dep_version(repo_path, "typescript")
}

fn detect_javascript(repo_path: &Path) -> Option<LanguageVersion> {
    // package.json engines.node
    let pkg = repo_path.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(node_ver) = val
                .get("engines")
                .and_then(|e| e.get("node"))
                .and_then(|n| n.as_str())
            {
                return Some(LanguageVersion {
                    version: node_ver
                        .trim_start_matches(|c: char| !c.is_ascii_digit())
                        .to_string(),
                    detected_from: "package.json (engines.node)".to_string(),
                    source: VersionSource::Manifest,
                });
            }
        }
    }
    None
}

fn detect_kotlin(repo_path: &Path) -> Option<LanguageVersion> {
    // build.gradle.kts or build.gradle
    for filename in &["build.gradle.kts", "build.gradle"] {
        if let Ok(content) = std::fs::read_to_string(repo_path.join(filename)) {
            for line in content.lines() {
                let trimmed = line.trim();
                // jvmTarget = "17" or jvmTarget = "1.8"
                if trimmed.contains("jvmTarget") && trimmed.contains('"') {
                    if let Some(ver) = extract_quoted(trimmed) {
                        return Some(LanguageVersion {
                            version: ver,
                            detected_from: filename.to_string(),
                            source: VersionSource::Manifest,
                        });
                    }
                }
                // kotlin("jvm") version "1.9.0"
                if trimmed.contains("kotlin")
                    && trimmed.contains("version")
                    && trimmed.contains('"')
                {
                    if let Some(ver) = extract_quoted(trimmed) {
                        return Some(LanguageVersion {
                            version: ver,
                            detected_from: filename.to_string(),
                            source: VersionSource::Manifest,
                        });
                    }
                }
            }
        }
    }
    None
}

fn detect_swift(repo_path: &Path) -> Option<LanguageVersion> {
    let pkg = repo_path.join("Package.swift");
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let content = std::fs::read_to_string(&pkg).ok()?;
    // swift-tools-version: 5.9
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix("// swift-tools-version:") {
            return Some(LanguageVersion {
                version: rest.trim().to_string(),
                detected_from: "Package.swift".to_string(),
                source: VersionSource::Manifest,
            });
        }
    }
    // swiftLanguageVersions: [.v5]
    if content.contains(".v5") || content.contains("v5") {
        return Some(LanguageVersion {
            version: "5".to_string(),
            detected_from: "Package.swift".to_string(),
            source: VersionSource::Manifest,
        });
    }
    None
}

fn detect_vue(repo_path: &Path) -> Option<LanguageVersion> {
    detect_npm_dep_version(repo_path, "vue")
}

fn detect_svelte(repo_path: &Path) -> Option<LanguageVersion> {
    detect_npm_dep_version(repo_path, "svelte")
}

fn detect_astro(repo_path: &Path) -> Option<LanguageVersion> {
    detect_npm_dep_version(repo_path, "astro")
}

/// Read the installed version of an npm package from `package.json`
/// (checking both `dependencies` and `devDependencies`).
fn detect_npm_dep_version(repo_path: &Path, package: &str) -> Option<LanguageVersion> {
    let pkg = repo_path.join("package.json");
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let content = std::fs::read_to_string(&pkg).ok()?;
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;

    for section in &["dependencies", "devDependencies"] {
        if let Some(ver) = val
            .get(*section)
            .and_then(|d| d.get(package))
            .and_then(|v| v.as_str())
        {
            let clean = ver.trim_start_matches(|c: char| {
                c == '^' || c == '~' || c == '>' || c == '=' || c == ' '
            });
            return Some(LanguageVersion {
                version: clean.to_string(),
                detected_from: format!("package.json ({section}.{package})"),
                source: VersionSource::Manifest,
            });
        }
    }
    None
}

fn extract_quoted(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_string())
}

// ---------------------------------------------------------------------------
// Version comparison helpers (used by adapters)
// ---------------------------------------------------------------------------

/// Parse a Python version string like "3.10" into `(major, minor)`.
pub fn parse_python_semver(version: &str) -> (u32, u32) {
    let mut parts = version.splitn(3, '.');
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor)
}

/// Parse a Go version string like "1.21" into `(major, minor)`.
pub fn parse_go_semver(version: &str) -> (u32, u32) {
    let clean = version.trim_start_matches('v');
    let mut parts = clean.splitn(3, '.');
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor)
}

/// Parse a TypeScript version like "4.9" or "5.0" into `(major, minor)`.
pub fn parse_ts_semver(version: &str) -> (u32, u32) {
    // tsconfig target might be "ES2022" or a package version like "5.0.2"
    let clean = version.trim_start_matches(|c: char| !c.is_ascii_digit());
    parse_go_semver(clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detects_rust_edition_from_cargo_toml() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let v = detect_rust(dir.path()).unwrap();
        assert_eq!(v.version, "2021");
        assert_eq!(v.detected_from, "Cargo.toml");
    }

    #[test]
    fn detects_go_version_from_go_mod() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("go.mod"),
            "module example.com/foo\n\ngo 1.21\n",
        )
        .unwrap();
        let v = detect_go(dir.path()).unwrap();
        assert_eq!(v.version, "1.21");
    }

    #[test]
    fn detects_python_from_python_version_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".python-version"), "3.11.2\n").unwrap();
        let v = detect_python(dir.path()).unwrap();
        assert_eq!(v.version, "3.11");
    }

    #[test]
    fn detects_python_from_pyproject_toml() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nrequires-python = \">=3.10\"\n",
        )
        .unwrap();
        let v = detect_python(dir.path()).unwrap();
        assert_eq!(v.version, "3.10");
    }

    #[test]
    fn detects_typescript_from_tsconfig() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"target":"ES2022"}}"#,
        )
        .unwrap();
        let v = detect_typescript(dir.path()).unwrap();
        assert_eq!(v.version, "ES2022");
    }

    #[test]
    fn normalises_python_version_strips_patch() {
        assert_eq!(normalise_python_version("3.10.4"), "3.10");
        assert_eq!(normalise_python_version("3.9"), "3.9");
    }

    #[test]
    fn parse_python_semver_parses_correctly() {
        assert_eq!(parse_python_semver("3.10"), (3, 10));
        assert_eq!(parse_python_semver("3.9"), (3, 9));
        assert_eq!(parse_python_semver("unknown"), (3, 0));
    }

    #[test]
    fn parse_go_semver_parses_correctly() {
        assert_eq!(parse_go_semver("1.21"), (1, 21));
        assert_eq!(parse_go_semver("1.18"), (1, 18));
    }
}
