//! Micro-benchmarks for weight calculation + version bump decision
//! (Workstream A1). These should be effectively O(1); the bench guards
//! against accidental growth in the hot decision path.

use kaptaind::config::loader::VersionThresholdConfig;
use kaptaind::diff::DiffAnalysis;
use kaptaind::version::{apply, decide};
use kaptaind::weight::calculator::{compute, WeightConfig};
use semver::Version;

fn main() {
    divan::main();
}

const WEIGHTS: WeightConfig = WeightConfig {
    s: 0.35,
    a: 0.3,
    d: 0.2,
    r: 0.15,
    b: 0.0,
};

fn sample_diff(i: u64) -> DiffAnalysis {
    let f = (i % 100) as f32 / 100.0;
    DiffAnalysis {
        structural: f,
        api: f * 0.5,
        deps: f * 0.25,
        runtime: f * 0.1,
        api_breaking: i.is_multiple_of(17),
        api_added: i.is_multiple_of(5),
        touched_paths: (i as usize) + 1,
        api_touches: (i as usize) % 7,
        api_signatures: (i as usize) % 11,
        ..DiffAnalysis::default()
    }
}

#[divan::bench(args = [1_000u64, 100_000u64])]
fn weight_compute(n: u64) -> f32 {
    let mut acc = 0.0f32;
    for i in 0..n {
        let w = compute(&sample_diff(i), &WEIGHTS);
        acc += divan::black_box(w.score);
    }
    acc
}

#[divan::bench(args = [1_000u64, 100_000u64])]
fn version_decide(n: u64) -> usize {
    let thresholds = VersionThresholdConfig::default();
    let mut major = 0usize;
    for i in 0..n {
        let w = compute(&sample_diff(i), &WEIGHTS);
        let bump = decide(&w, &thresholds);
        if matches!(divan::black_box(bump), kaptaind::version::Bump::Major) {
            major += 1;
        }
    }
    major
}

#[divan::bench(args = [1_000u64, 100_000u64])]
fn version_apply(n: u64) -> Version {
    let mut v = Version::new(1, 0, 0);
    for i in 0..n {
        let w = compute(&sample_diff(i), &WEIGHTS);
        let bump = decide(&w, &VersionThresholdConfig::default());
        v = apply(v, divan::black_box(bump));
    }
    v
}
