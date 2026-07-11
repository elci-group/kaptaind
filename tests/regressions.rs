//! Regression suite for the 20-finding live-fire audit
//! (docs/planning/AUTONOMOUS_COMMIT_SAFETY_PLAN.md).
//!
//! Daemon-level regressions live here; unit-level regressions live next to the
//! code they cover (src/commit/orchestrator.rs, src/daemon/scheduler.rs, ...).

#[path = "regressions/harness.rs"]
mod harness;

use harness::MonorepoFixture;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Finding #3: a genuine change must produce exactly one auto-commit; the
/// daemon's own version writeback must not re-cluster into further commits.
///
/// Before the self-write guard this fixture cascaded: commit -> save_version
/// writes VERSION/Cargo.toml -> watcher clusters them -> next commit, ad
/// infinitum (observed live: three self-commits in under three minutes,
/// stopped only by killing the daemon).
#[test]
fn daemon_does_not_cascade_on_version_writeback() {
    let fixture = MonorepoFixture::new();
    assert_eq!(fixture.kaptaind_commits(), 0);

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");

    // Ensure the daemon is reaped however the assertions pan out.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Let the watcher settle before editing.
        std::thread::sleep(Duration::from_secs(2));

        // Substantial edit: a trivial one-liner scores below the patch
        // threshold (0.1) and legitimately produces no commit, so the
        // regression needs a change that clears the bar.
        std::fs::write(
            fixture.project().join("src/util.rs"),
            "/// Adds two integers.\npub fn add(a: i64, b: i64) -> i64 { a + b }\n\n\
             /// Multiplies two integers.\npub fn mul(a: i64, b: i64) -> i64 { a * b }\n\n\
             /// Returns the maximum of two integers.\n\
             pub fn max(a: i64, b: i64) -> i64 { if a > b { a } else { b } }\n",
        )
        .expect("add module");
        std::fs::write(
            fixture.project().join("src/main.rs"),
            "mod util;\nfn main() { println!(\"{}\", util::add(1, 2)); }\n",
        )
        .expect("edit source");

        // Wait for the first auto-commit (cluster window is 1s; generous bound).
        wait_for(Duration::from_secs(30), || fixture.kaptaind_commits() >= 1);
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "expected exactly one auto-commit for a single change"
        );

        // Keep watching well beyond the cluster window: no cascade commit
        // may follow the daemon's own writeback.
        std::thread::sleep(Duration::from_secs(8));
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "cascade: a second auto-commit followed the daemon's own writeback"
        );

        // Finding #8: the committed version triple (VERSION, Cargo.toml,
        // Cargo.lock) must agree, and no drift may be left uncommitted.
        let committed_version = fixture
            .git(&["show", "HEAD:proj/VERSION"])
            .trim()
            .to_string();
        assert_ne!(
            committed_version, "0.1.0",
            "auto-commit did not include the VERSION bump"
        );
        let committed_toml = fixture.git(&["show", "HEAD:proj/Cargo.toml"]);
        let committed_lock = fixture.git(&["show", "HEAD:proj/Cargo.lock"]);
        assert!(
            committed_toml.contains(&format!("version = \"{committed_version}\"")),
            "Cargo.toml drifted from VERSION ({committed_version}):\n{committed_toml}"
        );
        assert!(
            committed_lock.contains(&format!("version = \"{committed_version}\"")),
            "Cargo.lock own-package entry drifted from VERSION ({committed_version}):\n{committed_lock}"
        );
        let drift = fixture.git(&[
            "status",
            "--porcelain",
            "--",
            "proj/VERSION",
            "proj/Cargo.toml",
            "proj/Cargo.lock",
        ]);
        assert!(
            drift.trim().is_empty(),
            "version drift left uncommitted after auto-commit:\n{drift}"
        );

        // Finding #11: hook installation must never fabricate a .git
        // directory inside the watched subproject.
        assert!(
            !fixture.project().join(".git").exists(),
            "daemon created a fake .git inside the monorepo subproject"
        );
    }));

    let _ = daemon.kill();
    let _ = daemon.wait();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn wait_for(timeout: Duration, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("condition not met within {:?}", timeout);
}
