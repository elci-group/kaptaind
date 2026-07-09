//! Micro-benchmarks for the event clustering engine (Workstream A1).
//!
//! Exercises the real `ClusterEngine::ingest` / `flush` path over a few event
//! volumes with both a fixed window and an adaptive window.

use chrono::{Duration as ChronoDuration, Utc};
use kaptaind::cluster::engine::ClusterEngine;
use kaptaind::watcher::{FsEvent, FsEventKind};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    divan::main();
}

fn event(i: usize, base: chrono::DateTime<Utc>) -> FsEvent {
    FsEvent {
        paths: vec![PathBuf::from(format!(
            "src/module_{}/file_{}.rs",
            i % 64,
            i
        ))],
        kind: FsEventKind::Modify,
        // Space events 1ms apart so a wide fixed window keeps merging them.
        timestamp: base + ChronoDuration::milliseconds(i as i64),
    }
}

fn run_ingest(n: usize, adaptive: bool) -> usize {
    let mut engine = ClusterEngine::new(Duration::from_secs(60));
    if adaptive {
        // Exercise the adaptive branch by constructing from a config-like shape.
        let cfg = kaptaind::config::loader::ClusterConfig {
            window: Duration::from_secs(2),
            adaptive: true,
            min_window_secs: 2,
            max_window_secs: 30,
            burst_threshold: 10,
            max_paths: 0,
            flush_after: None,
        };
        engine = ClusterEngine::new_from_config(&cfg);
    }

    let base = Utc::now();
    let mut emitted = 0usize;
    for i in 0..n {
        if engine.ingest(event(i, base)).is_some() {
            emitted += 1;
        }
    }
    if engine.flush().is_some() {
        emitted += 1;
    }
    divan::black_box(emitted)
}

#[divan::bench(args = [1_000usize, 10_000usize, 100_000usize])]
fn cluster_ingest_fixed(n: usize) -> usize {
    run_ingest(n, false)
}

#[divan::bench(args = [1_000usize, 10_000usize, 100_000usize])]
fn cluster_ingest_adaptive(n: usize) -> usize {
    run_ingest(n, true)
}
