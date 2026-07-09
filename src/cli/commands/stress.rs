//! `kaptaind-cli stress` — deterministic synthetic-fixture pipeline run
//! (Workstream B2 lite).
//!
//! Generates a reproducible repo into a temp dir, then drives the REAL
//! pipeline (ClusterEngine → diff::analyze → weight → version decide) over
//! N change batches. No commit, no daemon. Asserts the version is monotone
//! and records per-stage latency plus the bump distribution.

use chrono::Utc;
use kaptaind::cluster::engine::ClusterEngine;
use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use kaptaind::version::{apply, decide, Bump};
use kaptaind::watcher::{FsEvent, FsEventKind};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::table::print_table;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRecord {
    pub batch: usize,
    pub mutated: usize,
    pub cluster_us: u64,
    pub diff_us: u64,
    pub version_us: u64,
    pub bump: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressReport {
    pub schema: &'static str,
    pub run_id: String,
    pub generated_at: String,
    pub files: usize,
    pub batches: usize,
    pub seed: u64,
    pub langs: Vec<String>,
    pub git_rev: Option<String>,
    pub total_events: usize,
    pub elapsed_ms: u64,
    pub events_per_sec: f64,
    pub diff_p50_us: u64,
    pub diff_p95_us: u64,
    pub cluster_p95_us: u64,
    pub bump_distribution: BTreeMap<String, u64>,
    pub initial_version: String,
    pub final_version: String,
    pub monotone: bool,
    pub pass: bool,
    pub notes: Vec<String>,
    pub batches_detail: Vec<BatchRecord>,
}

pub fn handle_stress(
    config: &Config,
    files: usize,
    batches: usize,
    seed: u64,
    langs: Vec<String>,
    format: &str,
) -> anyhow::Result<()> {
    let report = run(config, files.max(1), batches.max(1), seed, langs)?;
    write_artifact(config, &report)?;

    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }

    if !report.pass {
        // Surface a failed invariant without panicking.
        anyhow::bail!("stress run reported a failing invariant (see artifact)");
    }
    Ok(())
}

fn run(
    config: &Config,
    files: usize,
    batches: usize,
    seed: u64,
    langs: Vec<String>,
) -> anyhow::Result<StressReport> {
    let langs = if langs.is_empty() {
        vec!["rust".into(), "ts".into(), "py".into(), "go".into()]
    } else {
        langs
    };

    let run_id = format!("stress-{}", Utc::now().format("%Y%m%dT%H%M%SZ"));
    let tmp = TempDir::new("kaptaind-stress")?;
    let mut rng = Rng::new(seed);

    // --- generate fixture -------------------------------------------------
    let specs: Vec<LangSpec> = langs.iter().map(|l| lang_spec(l)).collect();
    let mut generated: Vec<GeneratedFile> = Vec::with_capacity(files);
    std::fs::create_dir_all(tmp.path().join("src"))?;
    write_manifests(tmp.path(), &specs)?;

    for i in 0..files {
        let spec = &specs[i % specs.len()];
        let rel = format!("src/file_{i}{}", spec.ext);
        let gen = GeneratedFile {
            rel,
            spec: spec.clone(),
            revision: 0,
            index: i,
        };
        std::fs::write(tmp.path().join(&gen.rel), gen.content())?;
        generated.push(gen);
    }

    // --- run batches ------------------------------------------------------
    let git_rev = git_rev(&config.repo_path);
    let started = Instant::now();
    let mut version = Version::new(0, 1, 0);
    let initial_version = version.to_string();
    let mut monotone = true;
    let mut bump_distribution: BTreeMap<String, u64> = BTreeMap::new();
    let mut detail = Vec::with_capacity(batches);
    let mut diff_latencies = Vec::with_capacity(batches);
    let mut cluster_latencies = Vec::with_capacity(batches);
    let mut total_events = 0usize;
    let mut notes = Vec::new();

    for b in 0..batches {
        // Mutate a deterministic subset (~20%, at least one).
        let subset = (files / 5).max(1);
        let mut mutated_paths = Vec::with_capacity(subset);
        for _ in 0..subset {
            let idx = rng.gen_range(files);
            let g = &mut generated[idx];
            g.revision += 1;
            std::fs::write(tmp.path().join(&g.rel), g.content())?;
            mutated_paths.push(PathBuf::from(&g.rel));
        }
        mutated_paths.sort();
        mutated_paths.dedup();
        let mutated = mutated_paths.len();

        // Cluster stage: feed events, then flush (wide window => one cluster).
        let c0 = Instant::now();
        let mut engine = ClusterEngine::new(Duration::from_secs(60));
        let now = Utc::now();
        for path in &mutated_paths {
            let _ = engine.ingest(FsEvent {
                paths: vec![path.clone()],
                kind: FsEventKind::Modify,
                timestamp: now,
            });
        }
        let cluster = engine.flush();
        let cluster_us = c0.elapsed().as_micros() as u64;
        total_events += mutated;

        let Some(cluster) = cluster else {
            notes.push(format!("batch {b}: cluster engine produced no cluster"));
            continue;
        };

        // Diff stage.
        let d0 = Instant::now();
        let diff = kaptaind::diff::analyze(&cluster, tmp.path());
        let diff_us = d0.elapsed().as_micros() as u64;

        // Weight + version stage.
        let v0 = Instant::now();
        let weight = kaptaind::weight::compute(&diff, &config.weights);
        let bump = decide(&weight, &config.version_thresholds);
        let next = apply(version.clone(), bump);
        if next < version {
            monotone = false;
        }
        version = next;
        let version_us = v0.elapsed().as_micros() as u64;

        let bump_name = bump_name(bump).to_string();
        *bump_distribution.entry(bump_name.clone()).or_insert(0) += 1;
        diff_latencies.push(diff_us);
        cluster_latencies.push(cluster_us);

        detail.push(BatchRecord {
            batch: b,
            mutated,
            cluster_us,
            diff_us,
            version_us,
            bump: bump_name,
            version: version.to_string(),
        });
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let events_per_sec = if elapsed_ms == 0 {
        total_events as f64
    } else {
        total_events as f64 / (elapsed_ms as f64 / 1000.0)
    };

    let pass = monotone && detail.len() == batches;
    if !monotone {
        notes.push("version decreased between batches (monotonicity violated)".to_string());
    }

    Ok(StressReport {
        schema: "kaptaind.stress.v1",
        run_id,
        generated_at: Utc::now().to_rfc3339(),
        files,
        batches,
        seed,
        langs,
        git_rev,
        total_events,
        elapsed_ms,
        events_per_sec,
        diff_p50_us: percentile(&mut diff_latencies, 50),
        diff_p95_us: percentile(&mut diff_latencies, 95),
        cluster_p95_us: percentile(&mut cluster_latencies, 95),
        bump_distribution,
        initial_version,
        final_version: version.to_string(),
        monotone,
        pass,
        notes,
        batches_detail: detail,
    })
}

fn bump_name(b: Bump) -> &'static str {
    match b {
        Bump::Major => "Major",
        Bump::Minor => "Minor",
        Bump::Patch => "Patch",
        Bump::None => "None",
    }
}

fn percentile(samples: &mut [u64], p: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let idx = ((samples.len() - 1) * p) / 100;
    samples[idx]
}

fn write_manifests(root: &Path, specs: &[LangSpec]) -> anyhow::Result<()> {
    if specs.iter().any(|s| s.ext == ".rs") {
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"stress\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
    }
    if specs.iter().any(|s| s.ext == ".ts") {
        std::fs::write(
            root.join("package.json"),
            "{\"name\":\"stress\",\"version\":\"0.1.0\",\"dependencies\":{}}\n",
        )?;
    }
    if specs.iter().any(|s| s.ext == ".py") {
        std::fs::write(root.join("requirements.txt"), "")?;
    }
    if specs.iter().any(|s| s.ext == ".go") {
        std::fs::write(root.join("go.mod"), "module stress\n\ngo 1.22\n")?;
    }
    Ok(())
}

fn write_artifact(config: &Config, report: &StressReport) -> anyhow::Result<()> {
    let dir = config.repo_path.join(".kaptaind").join("stress");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", report.run_id));
    std::fs::write(&path, serde_json::to_string_pretty(report)?)?;
    Ok(())
}

fn git_rev(repo: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn print_human(r: &StressReport) {
    println!("{} {}", "🌪️ ".blue(), "Kaptaind Stress".bold().blue());
    println!("{}", "==================".blue());
    println!(
        "{} {} files × {} batches (seed {}, langs: {})",
        "Run:".bold().cyan(),
        r.files.to_string().blue(),
        r.batches.to_string().blue(),
        r.seed.to_string().blue(),
        r.langs.join(",").blue()
    );

    let rows = vec![
        vec![
            "diff p50".to_string(),
            format!("{:.2} ms", r.diff_p50_us as f64 / 1000.0),
        ],
        vec![
            "diff p95".to_string(),
            format!("{:.2} ms", r.diff_p95_us as f64 / 1000.0),
        ],
        vec![
            "cluster p95".to_string(),
            format!("{:.2} ms", r.cluster_p95_us as f64 / 1000.0),
        ],
        vec![
            "throughput".to_string(),
            format!("{:.0} events/sec", r.events_per_sec),
        ],
        vec![
            "version".to_string(),
            format!("{} → {}", r.initial_version, r.final_version),
        ],
    ];
    print_table(&["Metric", "Value"], &rows);

    if !r.bump_distribution.is_empty() {
        let bump_rows: Vec<Vec<String>> = r
            .bump_distribution
            .iter()
            .map(|(k, v)| vec![k.clone(), v.to_string()])
            .collect();
        println!("\n{}", "bump distribution:".bold().cyan());
        print_table(&["Bump", "Count"], &bump_rows);
    }

    let verdict = if r.pass {
        "PASS".green().to_string()
    } else {
        "FAIL".red().to_string()
    };
    println!(
        "\n{} {} (monotone={}, batches={}/{})",
        "Verdict:".bold().cyan(),
        verdict,
        r.monotone,
        r.batches_detail.len(),
        r.batches
    );
    for n in &r.notes {
        println!("  {} {}", "•".yellow(), n);
    }
    println!(
        "\n{} {}",
        "Artifact:".dimmed(),
        format!(".kaptaind/stress/{}.json", r.run_id).dimmed()
    );
}

// ---------------------------------------------------------------------------
// Fixture model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct LangSpec {
    ext: &'static str,
    /// Render file contents for (index, revision).
    render: fn(usize, u32) -> String,
}

#[derive(Debug, Clone)]
struct GeneratedFile {
    rel: String,
    spec: LangSpec,
    revision: u32,
    index: usize,
}

impl GeneratedFile {
    fn content(&self) -> String {
        (self.spec.render)(self.index, self.revision)
    }
}

fn lang_spec(name: &str) -> LangSpec {
    match name.to_lowercase().as_str() {
        "rust" | "rs" => LangSpec {
            ext: ".rs",
            render: |i, r| {
                format!(
                    "pub struct Thing{i} {{ pub value: u32, rev: u32 }}\n\
                     pub fn sym_{i}(x: u32) -> u32 {{ x + {r} }}\n"
                )
            },
        },
        "ts" | "typescript" => LangSpec {
            ext: ".ts",
            render: |i, r| {
                format!(
                    "export interface Thing{i} {{ value: number; rev: number; }}\n\
                     export function sym_{i}(x: number): number {{ return x + {r}; }}\n"
                )
            },
        },
        "py" | "python" => LangSpec {
            ext: ".py",
            render: |i, r| {
                format!(
                    "class Thing{i}:\n    value = {r}\n\n\
                     def sym_{i}(x):\n    return x + {r}\n"
                )
            },
        },
        "go" => LangSpec {
            ext: ".go",
            render: |i, r| {
                format!(
                    "package main\n\n\
                     type Thing{i} struct {{ Value int }}\n\n\
                     func Sym{i}(x int) int {{ return x + {r} }}\n"
                )
            },
        },
        other => {
            eprintln!(
                "{} unknown language '{other}', falling back to rust",
                "⚠️".yellow()
            );
            lang_spec("rust")
        }
    }
}

// ---------------------------------------------------------------------------
// Tiny deterministic RNG (xorshift64) — no extra dependency.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn gen_range(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() as usize) % n
    }
}

// ---------------------------------------------------------------------------
// Minimal temp-dir guard with cleanup (tempfile is a dev-dependency only).
// ---------------------------------------------------------------------------

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> anyhow::Result<Self> {
        let base = std::env::temp_dir();
        let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let path = base.join(format!("{prefix}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
