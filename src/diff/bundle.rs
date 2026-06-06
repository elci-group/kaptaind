use crate::config::loader::BundleConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

const BUNDLE_STATE_FILE: &str = ".kaptaind/bundle.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BundleState {
    total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct BundleResult {
    pub score: f32,
}

/// Compute a bundle size score by comparing current output size to previous.
/// Returns 0.0 if no build command is configured or the build fails.
pub fn bundle_score(config: &BundleConfig, repo_path: &Path) -> BundleResult {
    let Some(command) = &config.command else {
        return BundleResult { score: 0.0 };
    };

    // Run the build command
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(repo_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let Ok(status) = status else {
        return BundleResult { score: 0.0 };
    };

    if !status.success() {
        return BundleResult { score: 0.0 };
    }

    // Measure output directory size
    let output_dir = resolve_output_dir(config, repo_path);
    let new_size = dir_size(&output_dir);

    // Load previous state
    let state_path = repo_path.join(BUNDLE_STATE_FILE);
    let previous = load_state(&state_path);

    // Save new state
    let new_state = BundleState {
        total_bytes: new_size,
    };
    save_state(&state_path, &new_state);

    // Compute score as ratio of change
    let Some(prev) = previous else {
        return BundleResult { score: 0.0 };
    };

    if prev.total_bytes == 0 {
        return BundleResult { score: 0.0 };
    }

    let delta = (new_size as f64 - prev.total_bytes as f64).abs();
    let ratio = delta / prev.total_bytes as f64;
    BundleResult {
        score: (ratio as f32).clamp(0.0, 1.0),
    }
}

fn resolve_output_dir(config: &BundleConfig, repo_path: &Path) -> std::path::PathBuf {
    let dir_name = &config.output_dir;
    if !dir_name.is_empty() {
        return repo_path.join(dir_name);
    }

    // Check common output directories
    for candidate in ["dist", "build", ".next", "out"] {
        let path = repo_path.join(candidate);
        if path.is_dir() {
            return path;
        }
    }

    repo_path.join("dist")
}

fn dir_size(path: &Path) -> u64 {
    if !path.is_dir() {
        return 0;
    }
    walkdir(path)
}

fn walkdir(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            total += walkdir(&entry.path());
        } else {
            total += meta.len();
        }
    }
    total
}

fn load_state(path: &Path) -> Option<BundleState> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_state(path: &Path, state: &BundleState) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        path,
        serde_json::to_string_pretty(state).unwrap_or_default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::BundleConfig;
    use tempfile::tempdir;

    #[test]
    fn score_zero_when_no_command() {
        let config = BundleConfig {
            command: None,
            output_dir: "dist".to_string(),
        };
        let result = bundle_score(&config, Path::new("/tmp"));
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn score_zero_on_first_run_no_previous() {
        let dir = tempdir().unwrap();
        let dist = dir.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(dist.join("bundle.js"), "console.log('hello')").unwrap();

        let config = BundleConfig {
            command: Some("true".to_string()), // no-op build
            output_dir: "dist".to_string(),
        };

        let result = bundle_score(&config, dir.path());
        // First run has no previous state → score 0.0
        assert_eq!(result.score, 0.0);
        // But state file should now exist
        assert!(dir.path().join(".kaptaind/bundle.json").exists());
    }

    #[test]
    fn score_proportional_to_size_delta() {
        let dir = tempdir().unwrap();
        let dist = dir.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();

        // Seed previous state: 1000 bytes
        let state_dir = dir.path().join(".kaptaind");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("bundle.json"), r#"{"total_bytes": 1000}"#).unwrap();

        // Create output that is 1500 bytes (50% increase)
        std::fs::write(dist.join("bundle.js"), "x".repeat(1500)).unwrap();

        let config = BundleConfig {
            command: Some("true".to_string()),
            output_dir: "dist".to_string(),
        };

        let result = bundle_score(&config, dir.path());
        // 500/1000 = 0.5
        assert!((result.score - 0.5).abs() < 0.01);
    }

    #[test]
    fn b_weight_defaults_to_zero() {
        let toml_str = r#"
            s = 0.35
            a = 0.30
            d = 0.20
            r = 0.15
        "#;
        let cfg: crate::weight::WeightConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.b, 0.0);
    }

    #[test]
    fn old_analysis_json_without_bundle_deserializes() {
        let json = r#"{
            "structural": 0.5,
            "api": 0.3,
            "deps": 0.1,
            "runtime": 0.0,
            "api_breaking": false,
            "api_added": false,
            "touched_paths": 3,
            "api_touches": 1,
            "api_signatures": 0,
            "dependency_manifests": 0,
            "dependency_nodes": 0,
            "dependency_edges": 0,
            "runtime_paths": 0
        }"#;
        let analysis: crate::diff::DiffAnalysis = serde_json::from_str(json).unwrap();
        assert_eq!(analysis.bundle, 0.0);
    }
}
