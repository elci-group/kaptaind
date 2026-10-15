//! Integration tests for the one-shot `kaptaind commit` transaction (AGX-1).
//!
//! Every test runs the real binary against its own tempdir git repository
//! with an isolated HOME, so nothing touches the developer's checkout.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct Fixture {
    _home: TempDir,
    dir: TempDir,
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

impl Fixture {
    fn new(extra_config: &str) -> Self {
        let home = tempfile::tempdir().expect("home");
        let dir = tempfile::tempdir().expect("repo");
        let root = dir.path();
        git(root, &["init", "-q", "-b", "work"]);
        git(root, &["config", "user.name", "Kaptaind Test"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        std::fs::write(root.join(".gitignore"), ".kaptaind/\n").unwrap();
        std::fs::write(root.join(".kaptainignore"), ".git\n.kaptaind\n").unwrap();
        std::fs::write(root.join("VERSION"), "1.0.0\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn existing() {}\n").unwrap();
        std::fs::write(root.join("notes.txt"), "notes\n").unwrap();
        std::fs::write(
            root.join("kaptaind.toml"),
            format!(
                r#"
[trust]
execution = "trusted"

[operation]
mode = "actuate"

[watch]
ignore_file = ".kaptainignore"

[test]
command = "true"
required = true

[ratelimit]
min_commit_interval = 0

{extra_config}
"#
            ),
        )
        .unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "initial"]);
        Self { _home: home, dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kaptaind"))
            .args(args)
            .current_dir(self.root())
            .env("HOME", self._home.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("KAPTAIND_CONFIG")
            .env("NO_COLOR", "1")
            .output()
            .expect("run kaptaind")
    }

    fn commit_json(&self, extra: &[&str]) -> (i32, serde_json::Value) {
        let mut args = vec!["commit", "--json"];
        args.extend_from_slice(extra);
        let out = self.run(&args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let line = stdout
            .lines()
            .rev()
            .find(|l| l.trim_start().starts_with('{'))
            .unwrap_or_else(|| {
                panic!(
                    "no JSON on stdout: {stdout}\nstderr: {}",
                    String::from_utf8_lossy(&out.stderr)
                )
            });
        let json: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
        (out.status.code().expect("exit code"), json)
    }

    fn head(&self) -> String {
        git(self.root(), &["rev-parse", "HEAD"])
    }

    fn write(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.root().join(rel);
        std::fs::write(&path, content).unwrap();
        path
    }
}

#[test]
fn commit_runs_one_transaction_and_reports_json() {
    let fx = Fixture::new("");
    let before = fx.head();
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");

    let (code, json) = fx.commit_json(&[]);
    assert_eq!(code, 0, "{json}");
    let outcome = json["outcome"].as_str().unwrap();
    assert!(outcome == "commit" || outcome == "chore_commit", "{json}");
    let sha = json["commit"].as_str().expect("commit sha");
    assert_ne!(sha, before);
    assert_eq!(sha, fx.head());
    assert_eq!(json["version_before"], "1.0.0");
    assert!(json["cluster_id"].is_string());
    // Push was not requested and is disabled: reported, not silent.
    assert_eq!(json["push"]["status"], "disabled");
    if outcome == "commit" {
        let after = json["version_after"].as_str().unwrap();
        assert_ne!(after, "1.0.0");
        let version = std::fs::read_to_string(fx.root().join("VERSION")).unwrap();
        assert_eq!(version.trim(), after);
    }
    // The tree is clean afterwards and the process exited (no daemon loop).
    assert_eq!(git(fx.root(), &["status", "--porcelain"]), "");

    // A second run has nothing to do.
    let (code, json) = fx.commit_json(&[]);
    assert_eq!(code, 3, "{json}");
    assert_eq!(json["outcome"], "clean_tree");
    assert!(json["commit"].is_null());
    assert_eq!(json["push"]["status"], "skipped");
}

#[test]
fn paths_restrict_cluster_and_staging() {
    let fx = Fixture::new("");
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");
    fx.write("notes.txt", "unrelated dirty edit\n");

    let (code, json) = fx.commit_json(&["--paths", "src"]);
    assert_eq!(code, 0, "{json}");
    assert_eq!(json["paths"], serde_json::json!(["src/lib.rs"]));
    let committed = git(fx.root(), &["show", "--name-only", "--format=", "HEAD"]);
    assert!(committed.lines().any(|l| l == "src/lib.rs"), "{committed}");
    assert!(!committed.lines().any(|l| l == "notes.txt"), "{committed}");
    // The unrelated file is still dirty and unstaged.
    let unstaged = git(fx.root(), &["diff", "--name-only"]);
    assert_eq!(unstaged, "notes.txt");
    let staged = git(fx.root(), &["diff", "--cached", "--name-only"]);
    assert_eq!(staged, "", "nothing left staged");
}

#[test]
fn failing_tests_refuse_and_skip_tests_records_reason() {
    let fx = Fixture::new("");
    // Make the test hook fail.
    let toml = std::fs::read_to_string(fx.root().join("kaptaind.toml"))
        .unwrap()
        .replace("command = \"true\"", "command = \"false\"");
    std::fs::write(fx.root().join("kaptaind.toml"), toml).unwrap();
    git(fx.root(), &["commit", "-qam", "failing hook"]);
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");

    let (code, json) = fx.commit_json(&[]);
    assert_eq!(code, 4, "{json}");
    assert_eq!(json["outcome"], "test_failed");
    assert!(json["commit"].is_null());

    let (code, json) = fx.commit_json(&["--skip-tests", "hook broken, tracked in #42"]);
    assert_eq!(code, 0, "{json}");
    assert_eq!(json["skipped_tests"], "hook broken, tracked in #42");
    let body = git(fx.root(), &["log", "-1", "--format=%B"]);
    assert!(
        body.contains("Kaptaind-Skipped-Tests: hook broken, tracked in #42"),
        "{body}"
    );
    let decisions = std::fs::read_to_string(fx.root().join(".kaptaind/decisions.jsonl")).unwrap();
    let last = decisions.lines().last().unwrap();
    assert!(last.contains("tests skipped: hook broken"), "{last}");

    // An empty reason is rejected.
    let (code, _) = fx.commit_json(&["--skip-tests", "  "]);
    assert_eq!(code, 1);
}

#[test]
fn live_daemon_pid_refuses_unless_forced() {
    let fx = Fixture::new("");
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");
    std::fs::create_dir_all(fx.root().join(".kaptaind")).unwrap();
    // The test process itself is a live pid that is not the CLI's own.
    std::fs::write(
        fx.root().join(".kaptaind/daemon.pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    let (code, json) = fx.commit_json(&[]);
    assert_eq!(code, 4, "{json}");
    assert_eq!(json["outcome"], "daemon_running");

    let (code, json) = fx.commit_json(&["--force"]);
    assert_eq!(code, 0, "{json}");
}

#[test]
fn suspended_daemon_is_a_gate_refusal() {
    let fx = Fixture::new("");
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");
    let out = fx.run(&["suspend", "--reason", "hold"]);
    assert!(out.status.success(), "{out:?}");

    let (code, json) = fx.commit_json(&[]);
    assert_eq!(code, 4, "{json}");
    assert_eq!(json["outcome"], "suspended");
}

#[test]
fn requested_push_while_disabled_is_reported() {
    let fx = Fixture::new("");
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");

    let (code, json) = fx.commit_json(&["--push"]);
    assert_eq!(code, 4, "{json}");
    assert!(json["commit"].is_string(), "commit still happens: {json}");
    assert_eq!(json["push"]["status"], "disabled");
    assert!(json["push"]["detail"]
        .as_str()
        .unwrap()
        .contains("push is disabled"));
}

#[test]
fn push_goes_through_orchestrator_once() {
    let remote = tempfile::tempdir().expect("remote");
    git(remote.path(), &["init", "-q", "--bare"]);
    let fx = Fixture::new(
        "[push]\nenabled = true\nremote = \"origin\"\nbranch = \"work\"\n\n[capabilities]\nnetwork_push = true\n",
    );
    git(
        fx.root(),
        &[
            "remote",
            "add",
            "origin",
            &remote.path().display().to_string(),
        ],
    );
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");

    let (code, json) = fx.commit_json(&["--push"]);
    assert_eq!(code, 0, "{json}");
    assert_eq!(json["push"]["status"], "ok", "{json}");
    let remote_head = git(remote.path(), &["rev-parse", "refs/heads/work"]);
    assert_eq!(remote_head, json["commit"].as_str().unwrap());
}

#[test]
fn push_to_protected_branch_is_refused() {
    let remote = tempfile::tempdir().expect("remote");
    git(remote.path(), &["init", "-q", "--bare"]);
    let fx = Fixture::new(
        "[push]\nenabled = true\nremote = \"origin\"\n\n[capabilities]\nnetwork_push = true\n",
    );
    git(fx.root(), &["checkout", "-q", "-b", "main"]);
    git(
        fx.root(),
        &[
            "remote",
            "add",
            "origin",
            &remote.path().display().to_string(),
        ],
    );
    fx.write("src/lib.rs", "pub fn existing() {}\npub fn added() {}\n");

    let (code, json) = fx.commit_json(&["--push"]);
    assert_eq!(code, 4, "{json}");
    assert!(json["commit"].is_string());
    assert_eq!(json["push"]["status"], "refused", "{json}");
}
