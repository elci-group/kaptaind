//! Chaos soak: drive the real daemon binary against a synthetic workload
//! generator and assert the long-run invariants from
//! docs/planning/AUTONOMOUS_COMMIT_SAFETY_PLAN.md §7.5:
//!
//!   a) at most one daemon commit per genuine cluster (one per workload wave),
//!   b) VERSION / Cargo.toml / Cargo.lock agree at EVERY commit in history,
//!   c) `cargo metadata --locked --offline` succeeds at every commit (plus a
//!      full `cargo build --locked` at HEAD when `KAPTAIND_SOAK_BUILD=1`),
//!   d) zero ERROR lines in the daemon log.
//!
//! The test is `#[ignore]`d so normal `cargo test` stays fast; the nightly
//! workflow (.github/workflows/soak.yml) runs it for 30 minutes. Locally:
//!
//! ```sh
//! cargo test --test soak -- --ignored --nocapture            # 90s soak
//! KAPTAIND_SOAK_SECS=600 cargo test --test soak -- --ignored --nocapture
//! ```
//!
//! Knobs: `KAPTAIND_SOAK_SECS` (duration, default 90), `KAPTAIND_SOAK_SEED`
//! (workload PRNG seed, default fixed), `KAPTAIND_SOAK_BUILD=1` (also build
//! HEAD), `KAPTAIND_SOAK_LOG_DIR` (copy daemon log + report JSON there).
//!
//! A second `#[ignore]`d test, `daemon_soak_workspace_member_waves` (same
//! duration/seed/log knobs), drives the daemon against `WorkspaceFixture`
//! with `[versioning] workspace = "touched"`: every wave lands in exactly
//! one member subtree (alpha, beta, gamma) or the root crate, and invariant
//! (b) becomes the N-tuple check from
//! docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md §4 — proj/alpha/beta
//! manifest == lock entry, gamma's inherited version == root
//! `[workspace.package]`, VERSION == root manifest, at EVERY commit.

#[path = "regressions/harness.rs"]
mod harness;

use harness::{MonorepoFixture, WorkspaceFixture};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Fresh port: the regression suite occupies 19099–19111.
const HEALTH_PORT: u16 = 19113;
/// Must match the harness fixture's `[cluster] window = 1`.
const CLUSTER_WINDOW_SECS: u64 = 1;
/// Idle gap between waves: 3× the cluster window guarantees each wave is
/// exactly one genuine cluster (the daemon is fully idle before the next).
const IDLE_GAP_SECS: u64 = 3 * CLUSTER_WINDOW_SECS;

#[derive(Clone, Copy, PartialEq)]
enum WaveKind {
    /// Substantial src edits: clears the patch threshold -> bump commit.
    Code,
    /// Trivial docs edits: below threshold -> chore commit.
    Docs,
    /// Cargo.toml dependency-section toggle + lockfile regen + src edit.
    Deps,
    /// Code + docs in one cluster -> bump commit.
    Mixed,
}

impl WaveKind {
    fn name(self) -> &'static str {
        match self {
            WaveKind::Code => "code",
            WaveKind::Docs => "docs",
            WaveKind::Deps => "deps",
            WaveKind::Mixed => "mixed",
        }
    }
}

struct WaveRecord {
    index: usize,
    kind: &'static str,
    files: usize,
    commits: usize,
}

/// Deterministic xorshift64 — the workload must be reproducible from a seed
/// (same approach as src/cli/commands/stress.rs; `rand` is not a dependency).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    fn below(&mut self, n: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % n as u64) as usize
    }
}

/// Wave composition: the first three waves deterministically cover code,
/// docs, and deps so even a short soak exercises both commit paths; after
/// that, deps waves recur every fifth wave and the PRNG mixes the rest.
fn pick_kind(wave: usize, rng: &mut Rng) -> WaveKind {
    match wave {
        0 => WaveKind::Code,
        1 => WaveKind::Docs,
        2 => WaveKind::Deps,
        w if w % 5 == 2 => WaveKind::Deps,
        _ => match rng.below(3) {
            0 => WaveKind::Code,
            1 => WaveKind::Docs,
            _ => WaveKind::Mixed,
        },
    }
}

#[test]
#[ignore = "nightly chaos soak (.github/workflows/soak.yml); run manually with `cargo test --test soak -- --ignored --nocapture`"]
fn daemon_soak_chaos_invariants() {
    let duration_secs = env_u64("KAPTAIND_SOAK_SECS", 90);
    let seed = env_u64("KAPTAIND_SOAK_SEED", 0x5EED);
    let build_check = std::env::var("KAPTAIND_SOAK_BUILD").as_deref() == Ok("1");

    let fixture = MonorepoFixture::new(HEALTH_PORT);
    let project = fixture.project();

    // A vendored path dependency lets deps waves edit Cargo.toml's
    // [dependencies] section while keeping `cargo metadata --locked --offline`
    // green (no registry access ever needed).
    write_helper_crate(&project);
    fixture.git(&["add", "-A"]);
    fixture.git(&["commit", "-qm", "test: add helper crate for soak dep waves"]);

    let kaptaind_dir = project.join(".kaptaind");
    std::fs::create_dir_all(&kaptaind_dir).expect("mkdir .kaptaind");
    let log_path = kaptaind_dir.join("soak-daemon.log");
    let report_path = kaptaind_dir.join(format!(
        "soak-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));

    let log_file = std::fs::File::create(&log_path).expect("daemon log file");
    let log_file_err = log_file.try_clone().expect("clone log handle");
    // tracing_subscriber::fmt writes to stdout; stderr carries panics.
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(&project)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .spawn()
        .expect("daemon spawns");

    let mut notes: Vec<String> = Vec::new();
    let mut waves_detail: Vec<WaveRecord> = Vec::new();
    // Check results are collected (not asserted immediately) so the JSON
    // report captures the full picture even when a check fails.
    let mut triple_failures: Vec<String> = Vec::new();
    let mut lock_failures: Vec<String> = Vec::new();
    let mut head_build_ok: Option<bool> = None;
    let mut waves = 0usize;
    let mut total_commits = 0usize;
    let mut bumps = 0usize;
    let mut chores = 0usize;
    let mut error_lines: Vec<String> = Vec::new();
    let mut drift = String::new();
    let started = Instant::now();

    // Ensure the daemon is reaped however the assertions pan out.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Wait for the health endpoint, then let the watcher settle.
        assert!(
            poll_until(Duration::from_secs(30), || {
                http_get(HEALTH_PORT, "/health")
                    .map(|body| body.contains("\"status\":\"ok\""))
                    .unwrap_or(false)
            }),
            "daemon health endpoint never came up on port {HEALTH_PORT}"
        );
        std::thread::sleep(Duration::from_secs(2));
        let mut prev_commits = count_commits(&fixture);
        assert_eq!(
            prev_commits, 0,
            "unexpected daemon commits before the first wave"
        );

        let deadline = Instant::now() + Duration::from_secs(duration_secs);
        let mut rng = Rng::new(seed);
        let mut dep_on = false;
        let mut modules: Vec<usize> = Vec::new();

        while Instant::now() < deadline {
            let kind = pick_kind(waves, &mut rng);
            let files = write_wave(&project, kind, waves, &mut dep_on, &mut modules);

            // One genuine cluster -> exactly one commit. Generous bound: the
            // cluster window is 1s and the pipeline is sub-second on this
            // tiny fixture.
            let committed = poll_until(Duration::from_secs(45), || {
                count_commits(&fixture) > prev_commits
            });
            assert!(
                committed,
                "wave {waves} ({}): no daemon commit within 45s — daemon stalled or died; see {}",
                kind.name(),
                log_path.display()
            );
            std::thread::sleep(Duration::from_secs(IDLE_GAP_SECS));

            let now = count_commits(&fixture);
            let delta = now - prev_commits;
            assert_eq!(
                delta,
                1,
                "wave {waves} ({}): expected exactly 1 daemon commit, got {delta} \
                 (cascade or self-write re-cluster?)",
                kind.name()
            );
            waves_detail.push(WaveRecord {
                index: waves,
                kind: kind.name(),
                files,
                commits: delta,
            });
            prev_commits = now;
            waves += 1;
        }

        // Drain: commit count must stay flat for 3s past the last wave.
        assert!(
            poll_until(Duration::from_secs(20), || {
                let before = count_commits(&fixture);
                std::thread::sleep(Duration::from_secs(3));
                count_commits(&fixture) == before
            }),
            "daemon never went idle after the last wave"
        );

        // The daemon is idle; reap it so the log is final, then scan it.
        let _ = daemon.kill();
        let _ = daemon.wait();

        total_commits = count_commits(&fixture);
        bumps = fixture.kaptaind_commits();
        chores = fixture.chore_commits();

        let log = std::fs::read_to_string(&log_path).expect("read daemon log");
        error_lines = log
            .lines()
            .filter(|line| line.contains("ERROR"))
            .map(str::to_string)
            .collect();

        // Invariant (b): version triple consistency at EVERY commit.
        let shas: Vec<String> = fixture
            .git(&["rev-list", "--reverse", "HEAD"])
            .lines()
            .map(str::to_string)
            .collect();
        for sha in &shas {
            let version = fixture.git(&["show", &format!("{sha}:proj/VERSION")]);
            let version = version.trim().to_string();
            let toml = fixture.git(&["show", &format!("{sha}:proj/Cargo.toml")]);
            let lock = fixture.git(&["show", &format!("{sha}:proj/Cargo.lock")]);
            let toml_v = package_version(&toml);
            let lock_v = lock_own_version(&lock, "proj");
            if toml_v.as_deref() != Some(version.as_str())
                || lock_v.as_deref() != Some(version.as_str())
            {
                triple_failures.push(format!(
                    "{sha}: VERSION={version} Cargo.toml={} Cargo.lock={}",
                    toml_v.as_deref().unwrap_or("<none>"),
                    lock_v.as_deref().unwrap_or("<none>")
                ));
            }
        }

        // Invariant (c): lockfile consistency at every commit, checked in a
        // scratch clone so the fixture worktree is never disturbed.
        let scratch = tempfile::tempdir().expect("scratch dir");
        let fixture_path = fixture.dir.path().to_string_lossy().into_owned();
        let scratch_path = scratch.path().to_string_lossy().into_owned();
        run_cmd(
            None,
            &["git", "clone", "--quiet", &fixture_path, &scratch_path],
        )
        .expect("clone fixture");
        let manifest = scratch.path().join("proj/Cargo.toml");
        let manifest = manifest.to_string_lossy().into_owned();
        for sha in &shas {
            if let Err(err) = run_cmd(Some(scratch.path()), &["git", "checkout", "--quiet", sha]) {
                lock_failures.push(format!("{sha}: checkout failed: {err}"));
                continue;
            }
            let output = Command::new(cargo_bin())
                .args([
                    "metadata",
                    "--locked",
                    "--offline",
                    "--format-version",
                    "1",
                    "--manifest-path",
                    &manifest,
                ])
                .output()
                .expect("cargo metadata runs");
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                lock_failures.push(format!(
                    "{sha}: cargo metadata --locked failed: {}",
                    stderr.lines().next().unwrap_or("<no stderr>")
                ));
            }
        }

        // Full build only at HEAD (the fixture crate is tiny) and only when
        // opted in — CI sets KAPTAIND_SOAK_BUILD=1.
        if build_check {
            let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
            run_cmd(Some(scratch.path()), &["git", "checkout", "--quiet", &head])
                .expect("checkout HEAD");
            let output = Command::new(cargo_bin())
                .args(["build", "--locked", "--offline"])
                .current_dir(scratch.path().join("proj"))
                .output()
                .expect("cargo build runs");
            head_build_ok = Some(output.status.success());
            if !output.status.success() {
                notes.push(format!(
                    "cargo build --locked at HEAD failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .take(5)
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
        }

        // No version drift may be left uncommitted after the final commit.
        drift = fixture.git(&[
            "status",
            "--porcelain",
            "--",
            "proj/VERSION",
            "proj/Cargo.toml",
            "proj/Cargo.lock",
        ]);
    }));

    // Reap the daemon if the body panicked before doing so itself.
    let _ = daemon.kill();
    let _ = daemon.wait();

    let one_commit_per_wave =
        total_commits == waves && waves_detail.iter().all(|w| w.commits == 1) && result.is_ok();
    let both_paths = bumps >= 1 && chores >= 1;
    let pass = one_commit_per_wave
        && triple_failures.is_empty()
        && lock_failures.is_empty()
        && head_build_ok != Some(false)
        && error_lines.is_empty()
        && drift.trim().is_empty()
        && both_paths
        && waves >= 3;

    for failure in triple_failures.iter().take(5) {
        notes.push(format!("version triple mismatch: {failure}"));
    }
    if triple_failures.len() > 5 {
        notes.push(format!(
            "...and {} more triple mismatches",
            triple_failures.len() - 5
        ));
    }
    for failure in lock_failures.iter().take(5) {
        notes.push(format!("lockfile check: {failure}"));
    }
    if lock_failures.len() > 5 {
        notes.push(format!(
            "...and {} more lockfile failures",
            lock_failures.len() - 5
        ));
    }
    for line in error_lines.iter().take(5) {
        notes.push(format!("daemon log ERROR: {line}"));
    }
    if error_lines.len() > 5 {
        notes.push(format!("...and {} more ERROR lines", error_lines.len() - 5));
    }
    if !both_paths {
        notes.push(format!(
            "soak did not exercise both commit paths (bumps={bumps}, chores={chores})"
        ));
    }
    if waves < 3 {
        notes.push(format!(
            "only {waves} waves completed; raise KAPTAIND_SOAK_SECS for a meaningful soak"
        ));
    }
    if !drift.trim().is_empty() {
        notes.push(format!("version drift left uncommitted:\n{drift}"));
    }

    write_report(
        &report_path,
        seed,
        duration_secs,
        started.elapsed(),
        waves,
        total_commits,
        bumps,
        chores,
        error_lines.len(),
        one_commit_per_wave,
        triple_failures.is_empty(),
        lock_failures.is_empty(),
        head_build_ok,
        drift.trim().is_empty(),
        both_paths,
        pass,
        &notes,
        &waves_detail,
    );

    println!("=== kaptaind chaos soak report ===");
    println!(
        "duration:   {}s (budget {}s)",
        started.elapsed().as_secs(),
        duration_secs
    );
    println!("waves:      {waves}");
    println!("commits:    {total_commits} (bumps={bumps}, chores={chores})");
    println!("errors:     {}", error_lines.len());
    println!(
        "checks:     one_commit_per_wave={} triple={} lockfile={} head_build={} no_errors={} clean_tree={}",
        one_commit_per_wave,
        triple_failures.is_empty(),
        lock_failures.is_empty(),
        head_build_ok
            .map(|ok| if ok { "ok" } else { "FAIL" })
            .unwrap_or("skipped"),
        error_lines.is_empty(),
        drift.trim().is_empty()
    );
    println!("artifact:   {}", report_path.display());
    println!("verdict:    {}", if pass { "PASS" } else { "FAIL" });
    for note in &notes {
        println!("  - {note}");
    }

    // Export artifacts for CI upload even when the soak failed.
    export_artifacts(&log_path, &report_path);

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }

    assert_eq!(
        total_commits, waves,
        "invariant (a): daemon commits ({total_commits}) must equal genuine waves ({waves})"
    );
    assert!(
        triple_failures.is_empty(),
        "invariant (b): version triple drifted at {} commit(s), first: {}",
        triple_failures.len(),
        triple_failures.first().map(String::as_str).unwrap_or("")
    );
    assert!(
        lock_failures.is_empty(),
        "invariant (c): cargo metadata --locked failed at {} commit(s), first: {}",
        lock_failures.len(),
        lock_failures.first().map(String::as_str).unwrap_or("")
    );
    assert_ne!(
        head_build_ok,
        Some(false),
        "invariant (c): cargo build --locked failed at HEAD"
    );
    assert!(
        error_lines.is_empty(),
        "daemon logged {} ERROR line(s), first: {}",
        error_lines.len(),
        error_lines.first().map(String::as_str).unwrap_or("")
    );
    assert!(
        drift.trim().is_empty(),
        "version drift left uncommitted after soak:\n{drift}"
    );
    assert!(both_paths, "soak must exercise both bump and chore paths");
    assert!(
        waves >= 3,
        "only {waves} waves completed; raise KAPTAIND_SOAK_SECS"
    );
}

// ---------------------------------------------------------------------------
// Workload generator
// ---------------------------------------------------------------------------

/// Write one wave's files; returns the number of files touched. Every wave is
/// a single genuine cluster: all writes land well inside one cluster window.
fn write_wave(
    project: &Path,
    kind: WaveKind,
    wave: usize,
    dep_on: &mut bool,
    modules: &mut Vec<usize>,
) -> usize {
    match kind {
        WaveKind::Code => write_code_wave(project, wave, modules),
        WaveKind::Docs => write_docs_wave(project, wave),
        WaveKind::Deps => {
            // Dependency-section edit plus a substantial src edit, so the
            // wave clears the patch threshold regardless of how the deps
            // component alone scores.
            *dep_on = !*dep_on;
            write_manifest(project, *dep_on);
            regenerate_lockfile(project);
            2 + write_code_wave(project, wave, modules)
        }
        WaveKind::Mixed => {
            let docs = write_docs_wave(project, wave);
            write_code_wave(project, wave, modules) + docs
        }
    }
}

/// New module with several pub fns + main.rs wiring — mirrors the regression
/// suite's substantial_edit, which reliably clears the 0.1 patch threshold.
fn write_code_wave(project: &Path, wave: usize, modules: &mut Vec<usize>) -> usize {
    let module = format!(
        "//! Synthetic workload module for soak wave {wave}.\n\n\
         /// Adds the wave constant.\n\
         pub fn add_{wave}(a: i64, b: i64) -> i64 {{ a + b + {wave} }}\n\n\
         /// Multiplies and adds the wave constant.\n\
         pub fn mul_{wave}(a: i64, b: i64) -> i64 {{ a * b + {wave} }}\n\n\
         /// Returns the maximum plus the wave constant.\n\
         pub fn max_{wave}(a: i64, b: i64) -> i64 {{ (if a > b {{ a }} else {{ b }}) + {wave} }}\n"
    );
    std::fs::write(project.join(format!("src/w{wave}.rs")), module).expect("write module");
    modules.push(wave);

    let mut main = String::new();
    for m in modules.iter() {
        main.push_str(&format!("mod w{m};\n"));
    }
    main.push_str("fn main() {}\n");
    std::fs::write(project.join("src/main.rs"), main).expect("rewrite main.rs");
    2
}

/// One tiny markdown file — scores well below the patch threshold (a single
/// new file + dir event lands ~0.05 vs the 0.1 bar; two files flirted with
/// it), so the daemon captures the wave with a `chore:` commit
/// (require_bump defaults false).
fn write_docs_wave(project: &Path, wave: usize) -> usize {
    let docs = project.join("docs");
    std::fs::create_dir_all(&docs).expect("mkdir docs");
    std::fs::write(
        docs.join("notes.md"),
        format!("# Notes\n\nSynthetic docs churn, soak wave {wave}.\n"),
    )
    .expect("write docs file");
    1
}

/// Rewrite Cargo.toml, toggling the vendored path dependency while keeping
/// [package].version in sync with VERSION (the daemon moves it on bumps).
fn write_manifest(project: &Path, dep_on: bool) {
    let version = std::fs::read_to_string(project.join("VERSION"))
        .expect("read VERSION")
        .trim()
        .to_string();
    let deps = if dep_on {
        "\n[dependencies]\nhelper = { path = \"crates/helper\" }\n"
    } else {
        ""
    };
    std::fs::write(
        project.join("Cargo.toml"),
        format!("[package]\nname = \"proj\"\nversion = \"{version}\"\nedition = \"2021\"\n{deps}"),
    )
    .expect("write Cargo.toml");
}

/// Keep Cargo.lock consistent with the manifest after a deps edit, fully
/// offline (the helper crate is a path dependency — no registry involved).
fn regenerate_lockfile(project: &Path) {
    let output = Command::new(cargo_bin())
        .args(["generate-lockfile", "--offline"])
        .current_dir(project)
        .output()
        .expect("cargo generate-lockfile runs");
    assert!(
        output.status.success(),
        "cargo generate-lockfile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_helper_crate(project: &Path) {
    let helper = project.join("crates/helper");
    std::fs::create_dir_all(helper.join("src")).expect("mkdir helper");
    std::fs::write(
        helper.join("Cargo.toml"),
        "[package]\nname = \"helper\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("helper manifest");
    std::fs::write(helper.join("src/lib.rs"), "pub fn h() {}\n").expect("helper lib");
}

// ---------------------------------------------------------------------------
// Verification helpers
// ---------------------------------------------------------------------------

/// Bumping commits (body `kaptaind: <Bump> -> v...`) plus chore captures.
/// The two never overlap: chore bodies read `kaptaind: no-bump` and bump
/// subjects never start with `chore:` (src/commit/message.rs).
fn count_commits(fixture: &MonorepoFixture) -> usize {
    fixture.kaptaind_commits() + fixture.chore_commits()
}

/// `[package] version` from a Cargo.toml rendered as text.
fn package_version(toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package {
            if let Some(v) = line
                .strip_prefix("version")
                .and_then(|rest| rest.trim().strip_prefix('='))
                .map(|rest| rest.trim().trim_matches('"'))
            {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// The `[[package]]` block for `name` in a Cargo.lock rendered as text.
fn lock_own_version(lock: &str, name: &str) -> Option<String> {
    for block in lock.split("[[package]]").skip(1) {
        let block_name = block.lines().find_map(|line| {
            line.trim()
                .strip_prefix("name")
                .and_then(|rest| rest.trim().strip_prefix('='))
                .map(|rest| rest.trim().trim_matches('"'))
        });
        if block_name == Some(name) {
            return block.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("version")
                    .and_then(|rest| rest.trim().strip_prefix('='))
                    .map(|rest| rest.trim().trim_matches('"').to_string())
            });
        }
    }
    None
}

fn http_get(port: u16, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream
        .write_all(format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
        .ok()?;
    let mut body = String::new();
    stream.read_to_string(&mut body).ok()?;
    Some(body)
}

fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn poll_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn run_cmd(dir: Option<&Path>, argv: &[&str]) -> Result<(), String> {
    let mut command = Command::new(argv[0]);
    command.args(&argv[1..]);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

// ---------------------------------------------------------------------------
// Report + artifact export
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn write_report(
    path: &Path,
    seed: u64,
    budget_secs: u64,
    elapsed: Duration,
    waves: usize,
    commits: usize,
    bumps: usize,
    chores: usize,
    errors: usize,
    one_commit_per_wave: bool,
    triple_ok: bool,
    lockfile_ok: bool,
    head_build_ok: Option<bool>,
    clean_tree: bool,
    both_paths: bool,
    pass: bool,
    notes: &[String],
    waves_detail: &[WaveRecord],
) {
    let head_build_json = match head_build_ok {
        Some(ok) => ok.to_string(),
        None => "null".to_string(),
    };
    let notes_json = notes
        .iter()
        .map(|n| format!("\"{}\"", json_escape(n)))
        .collect::<Vec<_>>()
        .join(", ");
    let waves_json = waves_detail
        .iter()
        .map(|w| {
            format!(
                "{{\"wave\":{},\"kind\":\"{}\",\"files\":{},\"commits\":{}}}",
                w.index, w.kind, w.files, w.commits
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let json = format!(
        "{{\n  \"schema\": \"kaptaind.soak.v1\",\n  \"run_id\": \"{}\",\n  \
         \"generated_at\": \"{}\",\n  \"seed\": {seed},\n  \
         \"budget_secs\": {budget_secs},\n  \"elapsed_secs\": {},\n  \
         \"health_port\": {HEALTH_PORT},\n  \"waves\": {waves},\n  \
         \"commits\": {commits},\n  \"bumps\": {bumps},\n  \"chores\": {chores},\n  \
         \"errors\": {errors},\n  \"checks\": {{\n    \
         \"one_commit_per_wave\": {one_commit_per_wave},\n    \
         \"version_triple_consistent\": {triple_ok},\n    \
         \"lockfile_consistent_every_commit\": {lockfile_ok},\n    \
         \"head_build_locked\": {head_build_json},\n    \
         \"no_daemon_errors\": {},\n    \
         \"worktree_clean\": {clean_tree},\n    \
         \"both_commit_paths_exercised\": {both_paths}\n  }},\n  \
         \"pass\": {pass},\n  \"notes\": [{notes_json}],\n  \
         \"waves_detail\": [{waves_json}]\n}}\n",
        path.file_stem().unwrap_or_default().to_string_lossy(),
        chrono::Utc::now().to_rfc3339(),
        elapsed.as_secs(),
        errors == 0,
    );
    std::fs::write(path, json).expect("write soak report");
}

/// Copy the daemon log and soak report to $KAPTAIND_SOAK_LOG_DIR so the
/// workflow can upload them on failure. File names are preserved so the two
/// soak tests' artifacts never clobber each other.
fn export_artifacts(log_path: &Path, report_path: &Path) {
    let Ok(dir) = std::env::var("KAPTAIND_SOAK_LOG_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::copy(log_path, dir.join(log_path.file_name().unwrap_or_default()));
    if report_path.exists() {
        let _ = std::fs::copy(
            report_path,
            dir.join(report_path.file_name().unwrap_or_default()),
        );
    }
    println!("exported artifacts to {}", dir.display());
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

// ---------------------------------------------------------------------------
// Workspace member-wave soak (W2, docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md §4)
// ---------------------------------------------------------------------------

/// Fresh port: the workspace regression suite occupies 19117–19131.
const WS_HEALTH_PORT: u16 = 19133;

/// Which subtree a wave lands in.
#[derive(Clone, Copy, PartialEq)]
enum WaveTarget {
    /// `crates/alpha` — beta carries an exact `=0.1.0` requirement on alpha,
    /// so every alpha bump also exercises the G4 requirement-floor raise.
    Alpha,
    /// `crates/beta`.
    Beta,
    /// `crates/gamma` — inherits `[workspace.package].version`.
    Gamma,
    /// Root crate (`src/`), outside every member subtree.
    Root,
}

impl WaveTarget {
    fn name(self) -> &'static str {
        match self {
            WaveTarget::Alpha => "alpha",
            WaveTarget::Beta => "beta",
            WaveTarget::Gamma => "gamma",
            WaveTarget::Root => "root",
        }
    }

    fn index(self) -> usize {
        match self {
            WaveTarget::Alpha => 0,
            WaveTarget::Beta => 1,
            WaveTarget::Gamma => 2,
            WaveTarget::Root => 3,
        }
    }
}

/// Wave schedule: the first three waves deterministically cover alpha, beta,
/// and gamma so even a short soak bumps every member once; after that the
/// PRNG mixes the member subtrees with root-crate edits. The default seed
/// (0xCAFE) puts a root wave in the very first PRNG draw, so a short soak
/// still covers all four subtrees.
fn pick_target(wave: usize, rng: &mut Rng) -> WaveTarget {
    match wave {
        0 => WaveTarget::Alpha,
        1 => WaveTarget::Beta,
        2 => WaveTarget::Gamma,
        _ => match rng.below(4) {
            0 => WaveTarget::Alpha,
            1 => WaveTarget::Beta,
            2 => WaveTarget::Gamma,
            _ => WaveTarget::Root,
        },
    }
}

#[test]
#[ignore = "nightly workspace soak (.github/workflows/soak.yml); run manually with `cargo test --test soak -- --ignored --nocapture`"]
fn daemon_soak_workspace_member_waves() {
    let duration_secs = env_u64("KAPTAIND_SOAK_SECS", 90);
    // Distinct default from the chaos soak: this seed puts a root wave in
    // the first PRNG draw, so even a short soak covers all four subtrees.
    let seed = env_u64("KAPTAIND_SOAK_SEED", 0xCAFE);

    let fixture = WorkspaceFixture::new(WS_HEALTH_PORT, "touched", false);
    let project = fixture.project();

    // The daemon's state dir is runtime noise, not drift: gitignore it (as
    // real deployments do) so `git status --porcelain -- proj/` stays a
    // meaningful invariant. Written and committed before the daemon starts.
    std::fs::write(project.join(".gitignore"), ".kaptaind\n").expect(".gitignore");
    fixture.git(&["add", "-A"]);
    fixture.git(&["commit", "-qm", "test: ignore daemon state dir"]);

    let kaptaind_dir = project.join(".kaptaind");
    std::fs::create_dir_all(&kaptaind_dir).expect("mkdir .kaptaind");
    let log_path = kaptaind_dir.join("soak-workspace-daemon.log");
    let report_path = kaptaind_dir.join(format!(
        "soak-workspace-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));

    let log_file = std::fs::File::create(&log_path).expect("daemon log file");
    let log_file_err = log_file.try_clone().expect("clone log handle");
    // tracing_subscriber::fmt writes to stdout; stderr carries panics.
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(&project)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .spawn()
        .expect("daemon spawns");

    let mut notes: Vec<String> = Vec::new();
    let mut waves_detail: Vec<WaveRecord> = Vec::new();
    // Check results are collected (not asserted immediately) so the JSON
    // report captures the full picture even when a check fails.
    let mut tuple_failures: Vec<String> = Vec::new();
    let mut lock_failures: Vec<String> = Vec::new();
    let mut covered = [false; 4];
    let mut waves = 0usize;
    let mut total_commits = 0usize;
    let mut error_lines: Vec<String> = Vec::new();
    let mut drift = String::new();
    let started = Instant::now();

    // Ensure the daemon is reaped however the assertions pan out.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Wait for the health endpoint, then let the watcher settle.
        assert!(
            poll_until(Duration::from_secs(30), || {
                http_get(WS_HEALTH_PORT, "/health")
                    .map(|body| body.contains("\"status\":\"ok\""))
                    .unwrap_or(false)
            }),
            "daemon health endpoint never came up on port {WS_HEALTH_PORT}"
        );
        std::thread::sleep(Duration::from_secs(2));
        let mut prev_commits = count_workspace_commits(&fixture);
        assert_eq!(
            prev_commits, 0,
            "unexpected daemon commits before the first wave"
        );

        let deadline = Instant::now() + Duration::from_secs(duration_secs);
        let mut rng = Rng::new(seed);
        let mut workload = WorkspaceWorkload::default();

        while Instant::now() < deadline {
            let target = pick_target(waves, &mut rng);
            let files = write_workspace_wave(&project, target, waves, &mut workload);
            covered[target.index()] = true;

            // One genuine cluster -> exactly one commit. Generous bound: the
            // cluster window is 1s and the pipeline is sub-second on this
            // tiny fixture.
            let committed = poll_until(Duration::from_secs(45), || {
                count_workspace_commits(&fixture) > prev_commits
            });
            assert!(
                committed,
                "wave {waves} ({}): no daemon commit within 45s — daemon stalled or died; see {}",
                target.name(),
                log_path.display()
            );
            std::thread::sleep(Duration::from_secs(IDLE_GAP_SECS));

            let now = count_workspace_commits(&fixture);
            let delta = now - prev_commits;
            assert_eq!(
                delta,
                1,
                "wave {waves} ({}): expected exactly 1 daemon commit, got {delta} \
                 (cascade or self-write re-cluster?)",
                target.name()
            );
            waves_detail.push(WaveRecord {
                index: waves,
                kind: target.name(),
                files,
                commits: delta,
            });
            prev_commits = now;
            waves += 1;
        }

        // Drain: commit count must stay flat for 3s past the last wave.
        assert!(
            poll_until(Duration::from_secs(20), || {
                let before = count_workspace_commits(&fixture);
                std::thread::sleep(Duration::from_secs(3));
                count_workspace_commits(&fixture) == before
            }),
            "daemon never went idle after the last wave"
        );

        // The daemon is idle; reap it so the log is final, then scan it.
        let _ = daemon.kill();
        let _ = daemon.wait();

        total_commits = count_workspace_commits(&fixture);

        let log = std::fs::read_to_string(&log_path).expect("read daemon log");
        error_lines = log
            .lines()
            .filter(|line| line.contains("ERROR"))
            .map(str::to_string)
            .collect();

        // Invariant (b): workspace N-tuple consistency at EVERY commit —
        // each member's manifest version equals its lock entry (gamma's
        // inherited version lives in root [workspace.package]), and VERSION
        // agrees with the root manifest (the daemon only ever moves the two
        // together).
        let shas: Vec<String> = fixture
            .git(&["rev-list", "--reverse", "HEAD"])
            .lines()
            .map(str::to_string)
            .collect();
        for sha in &shas {
            let version = fixture.git(&["show", &format!("{sha}:proj/VERSION")]);
            let version = version.trim().to_string();
            let root_toml = fixture.git(&["show", &format!("{sha}:proj/Cargo.toml")]);
            let lock = fixture.git(&["show", &format!("{sha}:proj/Cargo.lock")]);
            let proj_v = package_version(&root_toml);
            if proj_v.as_deref() != Some(version.as_str()) {
                tuple_failures.push(format!(
                    "{sha}: VERSION={version} proj manifest={}",
                    proj_v.as_deref().unwrap_or("<none>")
                ));
            }
            let alpha_toml = fixture.git(&["show", &format!("{sha}:proj/crates/alpha/Cargo.toml")]);
            let beta_toml = fixture.git(&["show", &format!("{sha}:proj/crates/beta/Cargo.toml")]);
            for (name, manifest_v, lock_v) in [
                ("proj", proj_v, lock_own_version(&lock, "proj")),
                (
                    "alpha",
                    package_version(&alpha_toml),
                    lock_own_version(&lock, "alpha"),
                ),
                (
                    "beta",
                    package_version(&beta_toml),
                    lock_own_version(&lock, "beta"),
                ),
                (
                    "gamma",
                    workspace_package_version(&root_toml),
                    lock_own_version(&lock, "gamma"),
                ),
            ] {
                if manifest_v != lock_v {
                    tuple_failures.push(format!(
                        "{sha}: {name} manifest={} lock={}",
                        manifest_v.as_deref().unwrap_or("<none>"),
                        lock_v.as_deref().unwrap_or("<none>")
                    ));
                }
            }
        }

        // Invariant (c): lockfile consistency at every commit, checked in a
        // scratch clone so the fixture worktree is never disturbed. After an
        // alpha bump, beta's raised requirement floor must still resolve
        // against the committed lock (G4).
        let scratch = tempfile::tempdir().expect("scratch dir");
        let fixture_path = fixture.dir.path().to_string_lossy().into_owned();
        let scratch_path = scratch.path().to_string_lossy().into_owned();
        run_cmd(
            None,
            &["git", "clone", "--quiet", &fixture_path, &scratch_path],
        )
        .expect("clone fixture");
        let manifest = scratch.path().join("proj/Cargo.toml");
        let manifest = manifest.to_string_lossy().into_owned();
        for sha in &shas {
            if let Err(err) = run_cmd(Some(scratch.path()), &["git", "checkout", "--quiet", sha]) {
                lock_failures.push(format!("{sha}: checkout failed: {err}"));
                continue;
            }
            let output = Command::new(cargo_bin())
                .args([
                    "metadata",
                    "--locked",
                    "--offline",
                    "--format-version",
                    "1",
                    "--manifest-path",
                    &manifest,
                ])
                .output()
                .expect("cargo metadata runs");
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                lock_failures.push(format!(
                    "{sha}: cargo metadata --locked failed: {}",
                    stderr.lines().next().unwrap_or("<no stderr>")
                ));
            }
        }

        // Invariant (d): no writeback may be left uncommitted after the
        // final commit.
        drift = fixture.git(&["status", "--porcelain", "--", "proj/"]);
    }));

    // Reap the daemon if the body panicked before doing so itself.
    let _ = daemon.kill();
    let _ = daemon.wait();

    let target_names = ["alpha", "beta", "gamma", "root"];
    let missing: Vec<&str> = target_names
        .iter()
        .zip(covered.iter())
        .filter(|(_, hit)| !**hit)
        .map(|(name, _)| *name)
        .collect();
    let all_targets = missing.is_empty();
    let member_waves = waves_detail.iter().filter(|w| w.kind != "root").count();
    let root_waves = waves_detail.iter().filter(|w| w.kind == "root").count();

    let one_commit_per_wave =
        total_commits == waves && waves_detail.iter().all(|w| w.commits == 1) && result.is_ok();
    let pass = one_commit_per_wave
        && tuple_failures.is_empty()
        && lock_failures.is_empty()
        && error_lines.is_empty()
        && drift.trim().is_empty()
        && all_targets
        && waves >= 3;

    for failure in tuple_failures.iter().take(5) {
        notes.push(format!("N-tuple mismatch: {failure}"));
    }
    if tuple_failures.len() > 5 {
        notes.push(format!(
            "...and {} more N-tuple mismatches",
            tuple_failures.len() - 5
        ));
    }
    for failure in lock_failures.iter().take(5) {
        notes.push(format!("lockfile check: {failure}"));
    }
    if lock_failures.len() > 5 {
        notes.push(format!(
            "...and {} more lockfile failures",
            lock_failures.len() - 5
        ));
    }
    for line in error_lines.iter().take(5) {
        notes.push(format!("daemon log ERROR: {line}"));
    }
    if error_lines.len() > 5 {
        notes.push(format!("...and {} more ERROR lines", error_lines.len() - 5));
    }
    if !all_targets {
        notes.push(format!("subtrees never hit: {}", missing.join(", ")));
    }
    if waves < 3 {
        notes.push(format!(
            "only {waves} waves completed; raise KAPTAIND_SOAK_SECS for a meaningful soak"
        ));
    }
    if !drift.trim().is_empty() {
        notes.push(format!("workspace drift left uncommitted:\n{drift}"));
    }

    write_workspace_report(
        &report_path,
        seed,
        duration_secs,
        started.elapsed(),
        waves,
        total_commits,
        member_waves,
        root_waves,
        error_lines.len(),
        one_commit_per_wave,
        tuple_failures.is_empty(),
        lock_failures.is_empty(),
        drift.trim().is_empty(),
        all_targets,
        pass,
        &notes,
        &waves_detail,
    );

    println!("=== kaptaind workspace member-wave soak report ===");
    println!(
        "duration:   {}s (budget {}s)",
        started.elapsed().as_secs(),
        duration_secs
    );
    println!("waves:      {waves} (member={member_waves}, root={root_waves})");
    println!("commits:    {total_commits}");
    println!("errors:     {}", error_lines.len());
    println!(
        "checks:     one_commit_per_wave={} n_tuple={} lockfile={} no_errors={} clean_tree={} all_targets={}",
        one_commit_per_wave,
        tuple_failures.is_empty(),
        lock_failures.is_empty(),
        error_lines.is_empty(),
        drift.trim().is_empty(),
        all_targets
    );
    println!("artifact:   {}", report_path.display());
    println!("verdict:    {}", if pass { "PASS" } else { "FAIL" });
    for note in &notes {
        println!("  - {note}");
    }

    // Export artifacts for CI upload even when the soak failed.
    export_artifacts(&log_path, &report_path);

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }

    assert_eq!(
        total_commits, waves,
        "one commit per genuine wave: daemon commits ({total_commits}) must equal waves ({waves})"
    );
    assert!(
        tuple_failures.is_empty(),
        "invariant (b): workspace N-tuple drifted at {} commit(s), first: {}",
        tuple_failures.len(),
        tuple_failures.first().map(String::as_str).unwrap_or("")
    );
    assert!(
        lock_failures.is_empty(),
        "invariant (c): cargo metadata --locked failed at {} commit(s), first: {}",
        lock_failures.len(),
        lock_failures.first().map(String::as_str).unwrap_or("")
    );
    assert!(
        error_lines.is_empty(),
        "invariant (a): daemon logged {} ERROR line(s), first: {}",
        error_lines.len(),
        error_lines.first().map(String::as_str).unwrap_or("")
    );
    assert!(
        drift.trim().is_empty(),
        "invariant (d): workspace drift left uncommitted after soak:\n{drift}"
    );
    assert!(
        all_targets,
        "soak must hit all four subtrees; never hit: {}",
        missing.join(", ")
    );
    assert!(
        waves >= 3,
        "only {waves} waves completed; raise KAPTAIND_SOAK_SECS"
    );
}

// ---------------------------------------------------------------------------
// Workspace workload generator
// ---------------------------------------------------------------------------

/// Waves that have hit each subtree so far. Edits are regenerated
/// cumulatively from these lists, so a repeat hit only ADDS public functions
/// and never reads as a breaking removal.
#[derive(Default)]
struct WorkspaceWorkload {
    alpha: Vec<usize>,
    beta: Vec<usize>,
    gamma: Vec<usize>,
    root: Vec<usize>,
}

/// Write one wave's files; returns the number of files touched. Every wave
/// lands in exactly one subtree, so it is a single genuine cluster.
fn write_workspace_wave(
    project: &Path,
    target: WaveTarget,
    wave: usize,
    workload: &mut WorkspaceWorkload,
) -> usize {
    match target {
        WaveTarget::Alpha => write_member_wave(project, "alpha", wave, &mut workload.alpha),
        WaveTarget::Beta => write_member_wave(project, "beta", wave, &mut workload.beta),
        WaveTarget::Gamma => write_member_wave(project, "gamma", wave, &mut workload.gamma),
        WaveTarget::Root => write_root_wave(project, wave, &mut workload.root),
    }
}

/// A pair of wave-numbered public functions in the member's lib.rs — mirrors
/// the regression suite's substantial_member_edit, which reliably clears the
/// 0.1 patch threshold.
fn write_member_wave(project: &Path, member: &str, wave: usize, hits: &mut Vec<usize>) -> usize {
    hits.push(wave);
    let mut lib = String::new();
    for h in hits.iter() {
        lib.push_str(&format!(
            "/// Soak wave {h}: first synthetic export.\n\
             pub fn wave_{h}_add(a: i64, b: i64) -> i64 {{ a + b + {h} }}\n\n\
             /// Soak wave {h}: second synthetic export.\n\
             pub fn wave_{h}_mul(a: i64, b: i64) -> i64 {{ a * b + {h} }}\n\n"
        ));
    }
    lib.push_str(&format!(
        "/// {member} identity.\npub fn {member}() -> u64 {{ 1 }}\n"
    ));
    std::fs::write(project.join(format!("crates/{member}/src/lib.rs")), lib).expect("member lib");
    1
}

/// The root-crate counterpart: wave-numbered public functions in src/util.rs
/// plus the main.rs wiring (both paths sit outside every member subtree).
fn write_root_wave(project: &Path, wave: usize, hits: &mut Vec<usize>) -> usize {
    hits.push(wave);
    let mut util = String::new();
    let mut calls = String::new();
    for h in hits.iter() {
        util.push_str(&format!(
            "/// Soak wave {h}: first synthetic export.\n\
             pub fn util_{h}_add(a: i64, b: i64) -> i64 {{ a + b + {h} }}\n\n\
             /// Soak wave {h}: second synthetic export.\n\
             pub fn util_{h}_mul(a: i64, b: i64) -> i64 {{ a * b + {h} }}\n\n"
        ));
        calls.push_str(&format!("    let _ = util::util_{h}_add(1, 2);\n"));
    }
    std::fs::write(project.join("src/util.rs"), util).expect("util module");
    std::fs::write(
        project.join("src/main.rs"),
        format!("mod util;\n\nfn main() {{\n{calls}}}\n"),
    )
    .expect("main.rs");
    2
}

// ---------------------------------------------------------------------------
// Workspace verification helpers
// ---------------------------------------------------------------------------

/// Bumping commits (body `kaptaind: <Bump> -> v...`) plus chore captures —
/// the same pairing the chaos soak counts.
fn count_workspace_commits(fixture: &WorkspaceFixture) -> usize {
    let chores = fixture
        .git(&["log", "--format=%s"])
        .lines()
        .filter(|subject| subject.starts_with("chore:"))
        .count();
    fixture.kaptaind_commits() + chores
}

/// `[workspace.package] version` from a root Cargo.toml rendered as text.
/// gamma's inherited version lives here, never in its own manifest.
fn workspace_package_version(toml: &str) -> Option<String> {
    let mut in_section = false;
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[workspace.package]";
            continue;
        }
        if in_section {
            if let Some(v) = line
                .strip_prefix("version")
                .and_then(|rest| rest.trim().strip_prefix('='))
                .map(|rest| rest.trim().trim_matches('"'))
            {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Workspace report
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn write_workspace_report(
    path: &Path,
    seed: u64,
    budget_secs: u64,
    elapsed: Duration,
    waves: usize,
    commits: usize,
    member_waves: usize,
    root_waves: usize,
    errors: usize,
    one_commit_per_wave: bool,
    n_tuple_ok: bool,
    lockfile_ok: bool,
    clean_tree: bool,
    all_targets: bool,
    pass: bool,
    notes: &[String],
    waves_detail: &[WaveRecord],
) {
    let notes_json = notes
        .iter()
        .map(|n| format!("\"{}\"", json_escape(n)))
        .collect::<Vec<_>>()
        .join(", ");
    let waves_json = waves_detail
        .iter()
        .map(|w| {
            format!(
                "{{\"wave\":{},\"target\":\"{}\",\"files\":{},\"commits\":{}}}",
                w.index, w.kind, w.files, w.commits
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let json = format!(
        "{{\n  \"schema\": \"kaptaind.soak.workspace.v1\",\n  \"run_id\": \"{}\",\n  \
         \"generated_at\": \"{}\",\n  \"seed\": {seed},\n  \
         \"budget_secs\": {budget_secs},\n  \"elapsed_secs\": {},\n  \
         \"health_port\": {WS_HEALTH_PORT},\n  \"waves\": {waves},\n  \
         \"commits\": {commits},\n  \"member_waves\": {member_waves},\n  \
         \"root_waves\": {root_waves},\n  \"errors\": {errors},\n  \
         \"checks\": {{\n    \
         \"one_commit_per_wave\": {one_commit_per_wave},\n    \
         \"n_tuple_consistent\": {n_tuple_ok},\n    \
         \"lockfile_consistent_every_commit\": {lockfile_ok},\n    \
         \"no_daemon_errors\": {},\n    \
         \"worktree_clean\": {clean_tree},\n    \
         \"all_wave_targets_covered\": {all_targets}\n  }},\n  \
         \"pass\": {pass},\n  \"notes\": [{notes_json}],\n  \
         \"waves_detail\": [{waves_json}]\n}}\n",
        path.file_stem().unwrap_or_default().to_string_lossy(),
        chrono::Utc::now().to_rfc3339(),
        elapsed.as_secs(),
        errors == 0,
    );
    std::fs::write(path, json).expect("write workspace soak report");
}
