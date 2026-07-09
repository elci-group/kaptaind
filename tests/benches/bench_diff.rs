//! Micro-benchmarks for `diff::analyze` over synthetic working trees
//! (Workstream A1). Exercises the real structural + API + dependency +
//! runtime scoring path against small Rust/TS/Python working trees.

use chrono::Utc;
use kaptaind::cluster::engine::Cluster;
use kaptaind::watcher::{FsEvent, FsEventKind};
use std::path::PathBuf;
use tempfile::TempDir;

fn main() {
    divan::main();
}

/// Build a deterministic working tree of `n` source files and return the
/// temp dir (kept alive by the caller) plus a cluster referencing every file.
fn build_fixture(n: usize) -> (TempDir, Cluster) {
    let dir = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).unwrap();

    // Minimal manifests so dependency scoring has something to read.
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"bench\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        "{\"name\":\"bench\",\"version\":\"0.1.0\",\"dependencies\":{}}",
    )
    .unwrap();

    let mut paths = Vec::with_capacity(n);
    for i in 0..n {
        let (rel, content) = match i % 3 {
            0 => (
                format!("src/mod_{i}.rs"),
                format!("pub fn symbol_{i}(x: u32) -> u32 {{ x + {i} }}\n"),
            ),
            1 => (
                format!("src/mod_{i}.ts"),
                format!("export function symbol_{i}(x: number): number {{ return x + {i}; }}\n"),
            ),
            _ => (
                format!("src/mod_{i}.py"),
                format!("def symbol_{i}(x):\n    return x + {i}\n"),
            ),
        };
        std::fs::write(dir.path().join(&rel), content).unwrap();
        paths.push(PathBuf::from(rel));
    }

    let cluster = Cluster::new(FsEvent {
        paths,
        kind: FsEventKind::Modify,
        timestamp: Utc::now(),
    });
    (dir, cluster)
}

#[divan::bench(args = [10usize, 100usize, 1000usize])]
fn diff_analyze(bencher: divan::Bencher, n: usize) {
    bencher
        .with_inputs(|| build_fixture(n))
        .bench_values(|(dir, cluster)| kaptaind::diff::analyze(&cluster, dir.path()));
}

#[divan::bench(args = [100usize])]
fn diff_analyze_warm(bencher: divan::Bencher, n: usize) {
    bencher
        .with_inputs(|| {
            let (dir, cluster) = build_fixture(n);
            // Warm the AST cache with one full pass.
            let _ = kaptaind::diff::analyze(&cluster, dir.path());
            (dir, cluster)
        })
        .bench_values(|(dir, cluster)| kaptaind::diff::analyze(&cluster, dir.path()));
}
