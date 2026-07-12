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

#[path = "regressions/harness.rs"]
mod harness;

use harness::MonorepoFixture;
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
    // Isolate the daemon test hook's `cargo test` target dir inside the
    // ignored .kaptaind/ tree: cargo creates its target dir via a
    // `targetXXXXXX` tempdir next to it, and without this that transient
    // dir lands in the watched root, clusters, and produces a phantom
    // second commit (daemon-side gap: the self-write guard only covers the
    // version meta files).
    let cargo_target_dir = kaptaind_dir.join("soak-target");
    // tracing_subscriber::fmt writes to stdout; stderr carries panics.
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
        .current_dir(&project)
        .env("CARGO_TARGET_DIR", &cargo_target_dir)
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
/// workflow can upload them on failure.
fn export_artifacts(log_path: &Path, report_path: &Path) {
    let Ok(dir) = std::env::var("KAPTAIND_SOAK_LOG_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::copy(log_path, dir.join("soak-daemon.log"));
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
