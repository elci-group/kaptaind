//! Regression suite for the workspace version bumping plan
//! (docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md §4).
//!
//! Each test drives the real daemon against a `WorkspaceFixture` and asserts
//! which manifests moved after one cluster: the bump decision is applied to
//! the members the cluster touched (policy `touched`), to everything
//! (`lockstep`), and the workspace N-tuple (manifest == lock entry) holds in
//! the same commit.

#[path = "regressions/harness.rs"]
mod harness;

use harness::WorkspaceFixture;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Spawn the daemon against the fixture project for the duration of `body`,
/// reaping it however the assertions pan out.
fn with_daemon(fixture: &WorkspaceFixture, body: impl FnOnce(&WorkspaceFixture)) {
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(fixture)));
    let _ = daemon.kill();
    let _ = daemon.wait();
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// A member edit substantial enough to clear the patch threshold (mirrors
/// the audit-suite helper): adds public functions to the member's lib.rs.
fn substantial_member_edit(project: &Path, member: &str) {
    std::fs::write(
        project.join(format!("crates/{member}/src/lib.rs")),
        format!(
            "/// Adds two integers.\npub fn add(a: i64, b: i64) -> i64 {{ a + b }}\n\n\
             /// Multiplies two integers.\npub fn mul(a: i64, b: i64) -> i64 {{ a * b }}\n\n\
             /// {member} identity.\npub fn {member}() -> u64 {{ 1 }}\n"
        ),
    )
    .expect("member edit");
}

/// A substantial root-crate edit (paths outside every member subtree).
fn substantial_root_edit(project: &Path) {
    std::fs::write(
        project.join("src/util.rs"),
        "/// Adds two integers.\npub fn add(a: i64, b: i64) -> i64 { a + b }\n\n\
         /// Multiplies two integers.\npub fn mul(a: i64, b: i64) -> i64 { a * b }\n",
    )
    .expect("util module");
    std::fs::write(
        project.join("src/main.rs"),
        "mod util;\nfn main() { println!(\"{}\", util::add(1, 2)); }\n",
    )
    .expect("main edit");
}

/// The raw committed content of a project file at HEAD (for `VERSION`).
fn committed_file(fixture: &WorkspaceFixture, rel: &str) -> String {
    fixture.git(&["show", &format!("HEAD:proj/{rel}")])
}

/// The committed `[package].version` (first `version = "..."` line) of a
/// project file at HEAD.
fn committed_version(fixture: &WorkspaceFixture, rel: &str) -> String {
    let content = fixture.git(&["show", &format!("HEAD:proj/{rel}")]);
    content
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_string)
        })
        .unwrap_or_else(|| panic!("no version line in committed {rel}:\n{content}"))
}

/// The committed `[[package]]` version of `name` in the project Cargo.lock.
fn committed_lock_version(fixture: &WorkspaceFixture, name: &str) -> String {
    let content = fixture.git(&["show", "HEAD:proj/Cargo.lock"]);
    let mut found = None;
    let mut current: Option<&str> = None;
    for line in content.lines() {
        if line == "[[package]]" {
            current = None;
        } else if let Some(n) = line.strip_prefix("name = \"") {
            current = n.strip_suffix('"');
        } else if current == Some(name) && line.starts_with("version = \"") {
            found = line
                .strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_string);
        }
    }
    found.unwrap_or_else(|| panic!("no lock entry for {name}:\n{content}"))
}

fn wait_for(timeout: Duration, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("condition not met within {timeout:?}");
}

/// Wait for the first daemon auto-commit, then let the daemon settle so
/// exactly-one-commit assertions cover any cascade.
fn wait_for_one_commit(fixture: &WorkspaceFixture) {
    wait_for(Duration::from_secs(30), || fixture.kaptaind_commits() >= 1);
    std::thread::sleep(Duration::from_secs(6));
}

#[test]
fn member_only_edit_bumps_member_not_root() {
    let fixture = WorkspaceFixture::new(19117, "touched", false);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        substantial_member_edit(&fixture.project(), "alpha");
        wait_for_one_commit(fixture);
        assert_eq!(fixture.kaptaind_commits(), 1, "one cluster, one commit");

        let alpha = committed_version(fixture, "crates/alpha/Cargo.toml");
        assert_ne!(alpha, "0.1.0", "touched member must bump");
        assert_eq!(
            committed_file(fixture, "VERSION").trim(),
            "0.1.0",
            "root must not move for a member-only cluster"
        );
        assert_eq!(
            committed_version(fixture, "crates/beta/Cargo.toml"),
            "0.1.0"
        );
        // The bumped manifest must land in the same commit — no drift.
        let drift = fixture.git(&[
            "status",
            "--porcelain",
            "--",
            "proj/crates/alpha/Cargo.toml",
            "proj/Cargo.lock",
        ]);
        assert!(
            drift.trim().is_empty(),
            "member bump left uncommitted:\n{drift}"
        );
    });
}

#[test]
fn root_only_edit_bumps_root_not_members() {
    let fixture = WorkspaceFixture::new(19119, "touched", false);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        substantial_root_edit(&fixture.project());
        wait_for_one_commit(fixture);

        assert_ne!(committed_file(fixture, "VERSION").trim(), "0.1.0");
        assert_eq!(
            committed_version(fixture, "crates/alpha/Cargo.toml"),
            "0.1.0"
        );
        assert_eq!(
            committed_version(fixture, "crates/beta/Cargo.toml"),
            "0.1.0"
        );
        let root = fixture.git(&["show", "HEAD:proj/Cargo.toml"]);
        assert!(
            root.contains("[workspace.package]\nversion = \"0.3.0\""),
            "shared version must not move:\n{root}"
        );
    });
}

#[test]
fn cross_member_cluster_bumps_all_touched() {
    let fixture = WorkspaceFixture::new(19121, "touched", false);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        // Written back-to-back, both edits land in one 1s cluster window.
        substantial_member_edit(&fixture.project(), "alpha");
        substantial_member_edit(&fixture.project(), "beta");
        wait_for_one_commit(fixture);
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "one cross-member cluster must produce one commit"
        );

        assert_ne!(
            committed_version(fixture, "crates/alpha/Cargo.toml"),
            "0.1.0"
        );
        assert_ne!(
            committed_version(fixture, "crates/beta/Cargo.toml"),
            "0.1.0"
        );
        assert_eq!(committed_file(fixture, "VERSION").trim(), "0.1.0");
    });
}

#[test]
fn workspace_lock_consistent_after_every_bump() {
    let fixture = WorkspaceFixture::new(19123, "touched", false);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        substantial_member_edit(&fixture.project(), "alpha");
        wait_for_one_commit(fixture);

        let alpha = committed_version(fixture, "crates/alpha/Cargo.toml");
        assert_eq!(
            committed_lock_version(fixture, "alpha"),
            alpha,
            "member manifest and its lock entry must agree at HEAD"
        );
        assert_eq!(committed_lock_version(fixture, "beta"), "0.1.0");
        assert_eq!(committed_lock_version(fixture, "proj"), "0.1.0");
    });
}

#[test]
fn inherited_version_written_at_root_once() {
    let fixture = WorkspaceFixture::new(19125, "touched", false);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        substantial_member_edit(&fixture.project(), "gamma");
        wait_for_one_commit(fixture);

        let root = fixture.git(&["show", "HEAD:proj/Cargo.toml"]);
        assert!(
            !root.contains("[workspace.package]\nversion = \"0.3.0\""),
            "shared version must move:\n{root}"
        );
        let gamma = fixture.git(&["show", "HEAD:proj/crates/gamma/Cargo.toml"]);
        assert!(
            gamma.contains("version.workspace = true") && !gamma.contains("\nversion = "),
            "inheriting member manifest must never be written:\n{gamma}"
        );
        assert_eq!(
            committed_file(fixture, "VERSION").trim(),
            "0.1.0",
            "root crate did not participate in the cluster"
        );
        let shared = root
            .lines()
            .skip_while(|line| *line != "[workspace.package]")
            .find_map(|line| line.strip_prefix("version = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("workspace.package version")
            .to_string();
        assert_eq!(committed_lock_version(fixture, "gamma"), shared);
    });
}

#[test]
fn inter_member_requirement_stays_satisfiable() {
    let fixture = WorkspaceFixture::new(19127, "touched", false);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        substantial_member_edit(&fixture.project(), "alpha");
        wait_for_one_commit(fixture);

        let alpha = committed_version(fixture, "crates/alpha/Cargo.toml");
        assert_ne!(alpha, "0.1.0");
        let beta = fixture.git(&["show", "HEAD:proj/crates/beta/Cargo.toml"]);
        assert!(
            beta.contains(&format!(
                "alpha = {{ path = \"../alpha\", version = \"{alpha}\" }}"
            )),
            "requirement floor must follow the bumped member:\n{beta}"
        );
        assert!(
            beta.contains("\nversion = \"0.1.0\"\n"),
            "beta itself was not in the cluster and must not bump:\n{beta}"
        );
        let drift = fixture.git(&["status", "--porcelain", "--", "proj/crates/beta/Cargo.toml"]);
        assert!(
            drift.trim().is_empty(),
            "raised floor left uncommitted:\n{drift}"
        );
    });
}

#[test]
fn lockstep_bumps_everything() {
    let fixture = WorkspaceFixture::new(19129, "lockstep", false);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        substantial_member_edit(&fixture.project(), "alpha");
        wait_for_one_commit(fixture);

        assert_ne!(committed_file(fixture, "VERSION").trim(), "0.1.0");
        assert_ne!(
            committed_version(fixture, "crates/alpha/Cargo.toml"),
            "0.1.0"
        );
        assert_ne!(
            committed_version(fixture, "crates/beta/Cargo.toml"),
            "0.1.0"
        );
        let root = fixture.git(&["show", "HEAD:proj/Cargo.toml"]);
        assert!(
            !root.contains("[workspace.package]\nversion = \"0.3.0\""),
            "lockstep must move the shared version too:\n{root}"
        );
        // N-tuple: every member's manifest agrees with its lock entry.
        // gamma inherits: its version lives in root [workspace.package].
        let shared = root
            .lines()
            .skip_while(|line| *line != "[workspace.package]")
            .find_map(|line| line.strip_prefix("version = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("workspace.package version")
            .to_string();
        for (name, manifest) in [
            ("proj", committed_version(fixture, "Cargo.toml")),
            (
                "alpha",
                committed_version(fixture, "crates/alpha/Cargo.toml"),
            ),
            ("beta", committed_version(fixture, "crates/beta/Cargo.toml")),
            ("gamma", shared),
        ] {
            assert_eq!(
                committed_lock_version(fixture, name),
                manifest,
                "{name} drifted"
            );
        }
    });
}

#[test]
fn virtual_workspace_has_no_root_bump() {
    let fixture = WorkspaceFixture::new(19131, "touched", true);
    with_daemon(&fixture, |fixture| {
        std::thread::sleep(Duration::from_secs(2));
        substantial_member_edit(&fixture.project(), "alpha");
        wait_for_one_commit(fixture);

        assert_ne!(
            committed_version(fixture, "crates/alpha/Cargo.toml"),
            "0.1.0"
        );
        let version_tree = fixture.git(&["ls-tree", "HEAD", "proj/VERSION"]);
        assert!(
            version_tree.trim().is_empty(),
            "virtual workspace must never gain a root VERSION"
        );
        let root = fixture.git(&["show", "HEAD:proj/Cargo.toml"]);
        assert!(
            !root.contains("[package]"),
            "virtual root must stay package-less:\n{root}"
        );
    });
}
