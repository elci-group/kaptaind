//! Regression suite for the 20-finding live-fire audit
//! (docs/planning/AUTONOMOUS_COMMIT_SAFETY_PLAN.md).
//!
//! Daemon-level regressions live here; unit-level regressions live next to the
//! code they cover (src/commit/orchestrator.rs, src/daemon/scheduler.rs, ...).

#[path = "regressions/harness.rs"]
mod harness;

use harness::MonorepoFixture;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A change substantial enough to clear the patch threshold (0.1): a trivial
/// one-liner scores `Bump::None` and legitimately produces no commit.
fn substantial_edit(project: &Path) {
    std::fs::write(
        project.join("src/util.rs"),
        "/// Adds two integers.\npub fn add(a: i64, b: i64) -> i64 { a + b }\n\n\
         /// Multiplies two integers.\npub fn mul(a: i64, b: i64) -> i64 { a * b }\n\n\
         /// Returns the maximum of two integers.\n\
         pub fn max(a: i64, b: i64) -> i64 { if a > b { a } else { b } }\n",
    )
    .expect("add module");
    std::fs::write(
        project.join("src/main.rs"),
        "mod util;\nfn main() { println!(\"{}\", util::add(1, 2)); }\n",
    )
    .expect("edit source");
}

/// Finding #3: a genuine change must produce exactly one auto-commit; the
/// daemon's own version writeback must not re-cluster into further commits.
///
/// Before the self-write guard this fixture cascaded: commit -> save_version
/// writes VERSION/Cargo.toml -> watcher clusters them -> next commit, ad
/// infinitum (observed live: three self-commits in under three minutes,
/// stopped only by killing the daemon).
#[test]
fn daemon_does_not_cascade_on_version_writeback() {
    let fixture = MonorepoFixture::new(19099);
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
        substantial_edit(&fixture.project());

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

/// Finding #9: changes made while the daemon was down must be reconciled at
/// startup into exactly one catch-up commit through the normal pipeline.
#[test]
fn daemon_reconciles_pending_changes_on_startup() {
    let fixture = MonorepoFixture::new(19101);

    // Edit while no daemon is running.
    substantial_edit(&fixture.project());
    assert_eq!(fixture.kaptaind_commits(), 0);

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for(Duration::from_secs(30), || fixture.kaptaind_commits() >= 1);
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "expected exactly one catch-up commit on startup"
        );

        std::thread::sleep(Duration::from_secs(5));
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "catch-up commit must not cascade"
        );
    }));

    let _ = daemon.kill();
    let _ = daemon.wait();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// C4: the decisions log records commits AND skips — a substantial edit
/// produces a "commit" record; a below-threshold docs edit produces a
/// "no_bump" record carrying the achieved score instead of vanishing.
#[test]
fn decisions_log_records_commit_and_skip() {
    let fixture = MonorepoFixture::new(19103);

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Let the watcher settle before editing.
        std::thread::sleep(Duration::from_secs(2));

        substantial_edit(&fixture.project());
        wait_for(Duration::from_secs(30), || fixture.kaptaind_commits() >= 1);
        wait_for(Duration::from_secs(10), || {
            kaptaind::daemon::decisions::tail_decisions(&fixture.project(), 10)
                .map(|records| records.iter().any(|r| r.outcome == "commit"))
                .unwrap_or(false)
        });
        // The bump commit moved VERSION; remember where it landed.
        let bumped_version = fixture
            .git(&["show", "HEAD:proj/VERSION"])
            .trim()
            .to_string();

        // Trivial docs edit: below the patch threshold, so it must be logged
        // as a skip rather than silently dropped.
        std::fs::write(fixture.project().join("README.md"), "# proj\n").expect("docs edit");
        wait_for(Duration::from_secs(15), || {
            kaptaind::daemon::decisions::tail_decisions(&fixture.project(), 10)
                .map(|records| records.iter().any(|r| r.outcome == "no_bump"))
                .unwrap_or(false)
        });

        let records = kaptaind::daemon::decisions::tail_decisions(&fixture.project(), 10)
            .expect("read decisions log");
        let commit = records
            .iter()
            .find(|r| r.outcome == "commit")
            .expect("commit decision recorded");
        assert!(
            commit.version.is_some(),
            "commit record must carry the new version"
        );
        let skip = records
            .iter()
            .find(|r| r.outcome == "no_bump")
            .expect("no_bump decision recorded");
        assert!(
            skip.scores.is_some(),
            "skip record must carry the achieved score"
        );

        // Default behavior (`require_bump` unset ⇒ true): the below-threshold
        // docs edit is logged but NOT committed, and VERSION does not move.
        std::thread::sleep(Duration::from_secs(3));
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "below-threshold edit must not commit while require_bump is on"
        );
        assert_eq!(
            fixture.git(&["show", "HEAD:proj/VERSION"]).trim(),
            bumped_version,
            "no_bump must never move VERSION"
        );
    }));

    let _ = daemon.kill();
    let _ = daemon.wait();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// D1 (#7): with `require_bump = false`, a below-threshold docs edit is
/// captured by a real git commit whose subject starts with `chore:` instead
/// of being left uncommitted forever (and resurfacing on every rescan).
#[test]
fn chore_commit_captures_docs() {
    let fixture = MonorepoFixture::with_config(19109, "\n[commit]\nrequire_bump = false\n");

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Let the watcher settle before editing.
        std::thread::sleep(Duration::from_secs(2));

        // Trivial docs edit: scores below the patch threshold.
        std::fs::write(fixture.project().join("README.md"), "# proj\n").expect("docs edit");

        wait_for(Duration::from_secs(30), || fixture.chore_commits() >= 1);
        assert_eq!(
            fixture.chore_commits(),
            1,
            "expected exactly one chore commit for the docs edit"
        );
        assert_eq!(
            fixture.kaptaind_commits(),
            0,
            "below-threshold work must not produce a bumping commit"
        );

        // The chore commit actually captured the docs edit.
        let committed = fixture.git(&["show", "--name-only", "--format=", "HEAD"]);
        assert!(
            committed
                .lines()
                .any(|line| line.trim() == "proj/README.md"),
            "chore commit did not include the docs edit:\n{committed}"
        );

        // The decision log records the new outcome, not no_bump.
        wait_for(Duration::from_secs(10), || {
            kaptaind::daemon::decisions::tail_decisions(&fixture.project(), 10)
                .map(|records| records.iter().any(|r| r.outcome == "chore_commit"))
                .unwrap_or(false)
        });

        // VERSION is untouched, both at HEAD and in the worktree.
        assert_eq!(fixture.git(&["show", "HEAD:proj/VERSION"]).trim(), "0.1.0");
        assert_eq!(
            std::fs::read_to_string(fixture.project().join("VERSION"))
                .expect("VERSION")
                .trim(),
            "0.1.0"
        );
    }));

    let _ = daemon.kill();
    let _ = daemon.wait();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// D1 (#17): with `require_bump = false`, capturing below-threshold docs
/// work must not inflate the version — the VERSION / Cargo.toml / Cargo.lock
/// triple stays unchanged at HEAD and clean in the worktree.
#[test]
fn docs_edit_does_not_bump_when_require_bump_off() {
    let fixture = MonorepoFixture::with_config(19111, "\n[commit]\nrequire_bump = false\n");

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Let the watcher settle before editing.
        std::thread::sleep(Duration::from_secs(2));

        std::fs::write(fixture.project().join("README.md"), "# proj\n").expect("docs edit");

        wait_for(Duration::from_secs(30), || fixture.chore_commits() >= 1);

        // The version triple is unchanged at HEAD...
        assert_eq!(
            fixture.git(&["show", "HEAD:proj/VERSION"]).trim(),
            "0.1.0",
            "chore commit must not bump VERSION"
        );
        let toml = fixture.git(&["show", "HEAD:proj/Cargo.toml"]);
        assert!(
            toml.contains("version = \"0.1.0\""),
            "chore commit must not bump Cargo.toml:\n{toml}"
        );
        let lock = fixture.git(&["show", "HEAD:proj/Cargo.lock"]);
        assert!(
            lock.contains("version = \"0.1.0\""),
            "chore commit must not bump Cargo.lock:\n{lock}"
        );

        // ...and no version drift is left uncommitted.
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
            "version drift left uncommitted after chore commit:\n{drift}"
        );

        assert_eq!(
            fixture.kaptaind_commits(),
            0,
            "no bumping commit may accompany the chore capture"
        );
    }));

    let _ = daemon.kill();
    let _ = daemon.wait();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// Finding #11: editing `.kaptainignore` hot-reloads the ignore matcher —
/// a subsequently ignored file must never cluster, while normal edits
/// still commit.
#[test]
fn daemon_hot_reloads_ignore_file() {
    let fixture = MonorepoFixture::new(19105);

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::thread::sleep(Duration::from_secs(2));

        // Baseline: a genuine change commits.
        substantial_edit(&fixture.project());
        wait_for(Duration::from_secs(30), || fixture.kaptaind_commits() >= 1);
        assert_eq!(fixture.kaptaind_commits(), 1);

        // Extend the ignore file and let the reload event land.
        std::fs::write(
            fixture.project().join(".kaptainignore"),
            ".git\n.kaptaind\ntarget\nsrc/frozen.rs\n",
        )
        .expect("edit ignore file");
        std::thread::sleep(Duration::from_secs(3));

        // A change confined to the now-ignored file must not commit.
        std::fs::write(
            fixture.project().join("src/frozen.rs"),
            "/// Frozen.\npub fn frozen(a: i64, b: i64) -> i64 { a - b }\n",
        )
        .expect("edit ignored file");
        std::thread::sleep(Duration::from_secs(8));
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "hot-reloaded ignore pattern was not honored"
        );

        // Clustering still works for non-ignored paths afterwards.
        std::fs::write(
            fixture.project().join("src/util.rs"),
            "/// Adds three integers.\npub fn add3(a: i64, b: i64, c: i64) -> i64 { a + b + c }\n",
        )
        .expect("edit source again");
        wait_for(Duration::from_secs(30), || fixture.kaptaind_commits() >= 2);
        assert_eq!(fixture.kaptaind_commits(), 2);
    }));

    let _ = daemon.kill();
    let _ = daemon.wait();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// Finding #11: a corrupt kaptaind.toml edit must not take the daemon down —
/// it keeps the previous config and keeps committing.
#[test]
fn daemon_survives_invalid_config_edit() {
    let fixture = MonorepoFixture::new(19107);

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(fixture.project())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("daemon spawns");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::thread::sleep(Duration::from_secs(2));

        std::fs::write(fixture.project().join("kaptaind.toml"), "not [valid")
            .expect("corrupt config");
        std::thread::sleep(Duration::from_secs(3));

        substantial_edit(&fixture.project());
        wait_for(Duration::from_secs(30), || fixture.kaptaind_commits() >= 1);
        assert_eq!(
            fixture.kaptaind_commits(),
            1,
            "daemon died or stopped committing after an invalid config edit"
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
