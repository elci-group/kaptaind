use std::process::Command;
use tempfile::tempdir;

fn write_default_config(dir: &std::path::Path) {
    let config = r#"
repo_path = "."

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5

[weights]
s = 0.35
a = 0.3
d = 0.2
r = 0.15

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10

[test]
command = "cargo test"
required = false
"#;
    std::fs::write(dir.join("kaptaind.toml"), config).unwrap();
}

#[test]
fn test_status_command() {
    let dir = tempdir().expect("temp dir");
    write_default_config(dir.path());
    std::fs::write(dir.path().join("VERSION"), "1.2.3").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("status")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1.2.3"));
}

#[test]
fn test_log_command_with_artifacts() {
    let dir = tempdir().expect("temp dir");
    write_default_config(dir.path());

    let analysis_dir = dir.path().join(".kaptaind").join("analysis");
    std::fs::create_dir_all(&analysis_dir).unwrap();

    let json = r#"{
        "cluster_id": "test-cluster",
        "version": "2.0.0",
        "bump": "Major",
        "event_count": 3,
        "started_at": "2026-04-02T12:00:00Z",
        "ended_at": "2026-04-02T12:00:05Z",
        "diff": {
            "structural": 0.5,
            "api": 0.8,
            "deps": 0.0,
            "runtime": 0.0,
            "api_breaking": true,
            "api_added": false,
            "touched_paths": 2,
            "api_touches": 1,
            "api_signatures": 3,
            "dependency_manifests": 0,
            "dependency_nodes": 0,
            "dependency_edges": 0,
            "runtime_paths": 0
        },
        "weight": {
            "score": 0.85,
            "api_breaking": true,
            "api_added": false
        }
    }"#;

    std::fs::write(analysis_dir.join("test-cluster.json"), json).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("log")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2.0.0"));
    assert!(stdout.contains("Major"));
    assert!(stdout.contains("0.850"));
}

#[test]
fn test_analyze_command_on_clean_repo() {
    let dir = tempdir().expect("temp dir");
    write_default_config(dir.path());

    init_git(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("analyze")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Working tree is clean"));
}

#[test]
fn test_analyze_command_on_dirty_repo() {
    let dir = tempdir().expect("temp dir");
    write_default_config(dir.path());

    init_git(dir.path());

    let file_path = dir.path().join("src_file.rs");
    std::fs::write(&file_path, "pub fn hello() {}").unwrap();
    git(dir.path(), &["add", "src_file.rs"]);
    git(dir.path(), &["commit", "-m", "init"]);

    std::fs::write(&file_path, "pub fn hello() {}\npub fn world() {}").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("analyze")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Dry-run Analysis Result"));
    assert!(stdout.contains("Touched Paths"));
}

#[test]
fn test_init_detects_node_project() {
    let dir = tempdir().expect("temp dir");
    init_git(dir.path());
    std::fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("init")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Node"));

    let toml_content = std::fs::read_to_string(dir.path().join("kaptaind.toml")).unwrap();
    assert!(toml_content.contains("npm test"));

    let ignore_content = std::fs::read_to_string(dir.path().join(".kaptainignore")).unwrap();
    assert!(ignore_content.contains("node_modules"));
}

#[test]
fn test_init_does_not_overwrite_existing() {
    let dir = tempdir().expect("temp dir");
    init_git(dir.path());
    std::fs::write(dir.path().join("kaptaind.toml"), "# existing config").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("init")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("already exists"));

    let content = std::fs::read_to_string(dir.path().join("kaptaind.toml")).unwrap();
    assert_eq!(content, "# existing config");
}

fn init_git(path: &std::path::Path) {
    git(path, &["init"]);
    git(path, &["config", "user.name", "Kaptaind Test"]);
    git(path, &["config", "user.email", "kaptaind@example.com"]);
    git(path, &["add", "-A"]);
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["diff", "--cached", "--quiet"])
        .output()
        .expect("run git diff");
    if !output.status.success() {
        git(path, &["commit", "-m", "initial fixtures"]);
    }
}

#[test]
fn test_ship_plan_dry_run() {
    let dir = tempdir().expect("temp dir");
    let config = r#"
repo_path = "."

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5

[weights]
s = 0.35
a = 0.3
d = 0.2
r = 0.15

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10

[test]
command = "echo test"
required = false

[ship]
enabled = true

[ship.installers]
shell = true
"#;
    std::fs::write(dir.path().join("kaptaind.toml"), config).unwrap();
    std::fs::write(dir.path().join("VERSION"), "1.2.3").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .args(["ship", "plan"])
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Kaptaind Ship"));
    assert!(stdout.contains("1.2.3"));
    assert!(stdout.contains("Dry-run plan complete"));

    // Ensure no artifacts were produced in dry-run mode.
    let ship_dir = dir.path().join(".kaptaind").join("ship");
    assert!(!ship_dir.exists() || ship_dir.read_dir().unwrap().next().is_none());
}

fn git(path: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ship_config() -> &'static str {
    r#"
repo_path = "."

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5

[weights]
s = 0.35
a = 0.3
d = 0.2
r = 0.15

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10

[test]
command = "echo test"
required = false

[ship]
enabled = true

[ship.installers]
shell = true
"#
}

#[test]
fn test_ship_stable_dry_run() {
    let dir = tempdir().expect("temp dir");
    std::fs::write(dir.path().join("kaptaind.toml"), ship_config()).unwrap();
    std::fs::write(dir.path().join("VERSION"), "4.5.6").unwrap();
    init_git(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .args(["ship", "stable", "--dry-run", "--channels", "binaries"])
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Kaptaind Ship"));
    assert!(stdout.contains("4.5.6"));
    assert!(stdout.contains("stable"));
    assert!(stdout.contains("Dry-run plan complete"));
}

#[test]
fn test_ship_nightly_dry_run_uses_prerelease_version() {
    let dir = tempdir().expect("temp dir");
    std::fs::write(dir.path().join("kaptaind.toml"), ship_config()).unwrap();
    std::fs::write(dir.path().join("VERSION"), "4.5.6").unwrap();
    init_git(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .args(["ship", "nightly", "--dry-run", "--channels", "binaries"])
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Kaptaind Ship"));
    assert!(stdout.contains("nightly"));
    assert!(stdout.contains("4.5.6-nightly."));
    assert!(stdout.contains("Dry-run plan complete"));
}

#[test]
fn test_ship_status_json_when_empty() {
    let dir = tempdir().expect("temp dir");
    std::fs::write(dir.path().join("kaptaind.toml"), ship_config()).unwrap();
    std::fs::write(dir.path().join("VERSION"), "1.0.0").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .args(["ship", "status", "--format", "json"])
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "null");
}
