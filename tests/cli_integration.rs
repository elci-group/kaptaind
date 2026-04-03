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
    assert!(output.status.success(), "Command failed with stderr: {}", stderr);
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
    assert!(output.status.success(), "Command failed with stderr: {}", stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2.0.0"));
    assert!(stdout.contains("Major"));
    assert!(stdout.contains("0.850"));
}

#[test]
fn test_analyze_command_on_clean_repo() {
    let dir = tempdir().expect("temp dir");
    write_default_config(dir.path());
    
    let _repo = git2::Repository::init(dir.path()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("analyze")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "Command failed with stderr: {}", stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Working tree is clean"));
}

#[test]
fn test_analyze_command_on_dirty_repo() {
    let dir = tempdir().expect("temp dir");
    write_default_config(dir.path());
    
    let repo = git2::Repository::init(dir.path()).unwrap();
    
    let file_path = dir.path().join("src_file.rs");
    std::fs::write(&file_path, "pub fn hello() {}").unwrap();
    
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("src_file.rs")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    
    // Create a dummy signature
    let sig = git2::Signature::now("test", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();

    std::fs::write(&file_path, "pub fn hello() {}\npub fn world() {}").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kaptaind-cli"))
        .current_dir(dir.path())
        .arg("analyze")
        .output()
        .expect("run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "Command failed with stderr: {}", stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Dry-run Analysis Result"));
    assert!(stdout.contains("Touched Paths"));
}
