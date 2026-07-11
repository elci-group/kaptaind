//! Claims-audit regression harness.
//!
//! Each `claim_*` test locks a quantitative or boolean statement made in the
//! project documentation (README.md, AGENTS.md, SECURITY.md, LANGUAGE_MATRIX.md)
//! against the actual implementation, so documentation drift is caught in CI.
//!
//! Verdicts for the claims these tests guard are recorded in
//! `docs/planning/CLAIMS_AUDIT.md`. Claims that cannot be expressed as a
//! deterministic assertion (endpoint routing, feature existence, marketing
//! figures) are verified by source inspection and documented there.

use kaptaind::cluster::engine::Cluster;
use kaptaind::config::Config;
use kaptaind::diff::lang::registry::AdapterRegistry;
use kaptaind::diff::lang::{normalize, Language};
use kaptaind::diff::text::structural_score;
use kaptaind::diff::DiffAnalysis;
use kaptaind::version::{decide_default, Bump};
use kaptaind::watcher::{FsEvent, FsEventKind};
use kaptaind::weight::{compute, WeightConfig, WeightResult};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// One representative file extension per active built-in adapter. The active set
/// is the 12 original adapters plus the 16 T1/T2/T3 promotions wired in the
/// adapter-200 effort (see docs/planning/ADAPTER_200_ROADMAP.md).
const ACTIVE_EXTS: &[&str] = &[
    // original 12
    "rs", "ts", "js", "py", "go", "swift", "kt", "vue", "svelte", "astro", "scss", "css",
    // T1/T2/T3 promotions
    "c", "cpp", "cs", "java", "php", "scala", "clj", "hs", "ex", "erl", "lua", "ml", "pl", "fs",
    "rb", "dart",
];

/// README/AGENTS/LANGUAGE_MATRIX document a fixed set of active language adapters.
/// Lock the active set: every documented adapter resolves, the set has exactly 28
/// distinct adapters, and languages that are explicitly unsupported (Julia, R) do
/// NOT resolve to a built-in adapter (they fall back to the line scanner).
#[test]
fn claim_active_adapter_set_matches_docs() {
    let reg = AdapterRegistry::default_registry();
    let mut names = BTreeSet::new();
    for ext in ACTIVE_EXTS {
        let p = PathBuf::from(format!("probe.{ext}"));
        let adapter = reg
            .resolve(&p)
            .unwrap_or_else(|| panic!("expected an active built-in adapter for .{ext}"));
        names.insert(adapter.name().to_string());
    }
    assert_eq!(
        names.len(),
        28,
        "expected 28 distinct active adapters, got {}: {:?}",
        names.len(),
        names
    );
    // Fallback boundary: Julia and R are intentionally unsupported.
    assert!(reg.resolve(Path::new("probe.jl")).is_none());
    assert!(reg.resolve(Path::new("probe.r")).is_none());
}

/// AGENTS.md / README: structural = 0.5*event_density + 0.35*path_spread + 0.15*churn.
#[test]
fn claim_structural_formula() {
    let t0 = chrono::Utc::now();
    let t1 = t0 + chrono::Duration::seconds(10);
    let ev1 = FsEvent {
        paths: vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
        kind: FsEventKind::Modify,
        timestamp: t0,
    };
    let ev2 = FsEvent {
        paths: vec![PathBuf::from("c.rs")],
        kind: FsEventKind::Modify,
        timestamp: t1,
    };
    let mut cluster = Cluster::new(ev1);
    cluster.add_event(ev2);

    let score = structural_score(&cluster);
    // density = 2/24, spread = 3/16, churn = (10000ms/1000)/20 = 0.5
    let expected = 0.5 * (2.0 / 24.0) + 0.35 * (3.0 / 16.0) + 0.15 * 0.5;
    assert!(
        approx(score, expected),
        "structural score {score} != documented {expected}"
    );
    assert!(approx(score, 0.182_291_7));
}

/// AGENTS.md / README: score = s*structural + a*api + d*deps + r*runtime + b*bundle.
#[test]
fn claim_weight_formula() {
    let diff = DiffAnalysis {
        structural: 0.5,
        api: 0.4,
        deps: 0.3,
        runtime: 0.2,
        bundle: 0.1,
        ..Default::default()
    };
    let cfg = WeightConfig {
        s: 0.35,
        a: 0.3,
        d: 0.2,
        r: 0.15,
        b: 0.0,
    };
    let r = compute(&diff, &cfg);
    assert!(approx(r.score, 0.385), "weighted score {}", r.score);

    // Bundle term contributes when enabled (b > 0).
    let cfg_b = WeightConfig {
        s: 0.35,
        a: 0.3,
        d: 0.2,
        r: 0.15,
        b: 1.0,
    };
    let r2 = compute(&diff, &cfg_b);
    assert!(approx(r2.score, 0.485), "bundle-aware score {}", r2.score);
}

/// README/SECURITY/AGENTS: breaking->Major, added||score>0.6 ->Minor, score>0.1 ->Patch,
/// else None. Locks the strict greater-than boundary semantics.
#[test]
fn claim_version_rules() {
    let wr = |score, api_breaking, api_added| WeightResult {
        score,
        api_breaking,
        api_added,
    };
    assert_eq!(decide_default(&wr(0.0, true, false)), Bump::Major);
    assert_eq!(decide_default(&wr(0.0, false, true)), Bump::Minor);
    assert_eq!(decide_default(&wr(0.7, false, false)), Bump::Minor);
    assert_eq!(decide_default(&wr(0.2, false, false)), Bump::Patch);
    assert_eq!(decide_default(&wr(0.05, false, false)), Bump::None);
    // Strict '>' (not '>='): a score exactly on the threshold does not cross it.
    assert_eq!(decide_default(&wr(0.6, false, false)), Bump::Patch);
    assert_eq!(decide_default(&wr(0.1, false, false)), Bump::None);
}

/// README config section / AGENTS.md "observed defaults": lock every documented default.
#[test]
fn claim_config_defaults() {
    let c = Config::default();

    assert!(c.watch.recursive);
    assert_eq!(c.watch.ignore_file, PathBuf::from(".kaptainignore"));
    assert_eq!(c.cluster.window, std::time::Duration::from_secs(5));
    assert!(!c.cluster.adaptive);
    assert_eq!(
        c.ratelimit.min_commit_interval,
        std::time::Duration::from_secs(10)
    );

    assert!(approx(c.weights.s, 0.35));
    assert!(approx(c.weights.a, 0.3));
    assert!(approx(c.weights.d, 0.2));
    assert!(approx(c.weights.r, 0.15));
    assert!(approx(c.weights.b, 0.0));

    assert!(!c.push.enabled, "push must be disabled by default");
    assert_eq!(c.push.branch, "main");
    assert_eq!(c.push.remote, "origin");

    assert_eq!(c.test.command.as_deref(), Some("cargo test"));
    assert!(c.test.required, "test hook must be required by default");

    assert!(
        !c.inference.enabled,
        "inference must be disabled by default"
    );
}

/// AGENTS.md / LANGUAGE_MATRIX.md: per-language confidence multipliers (every wired
/// adapter must have an explicit, documented arm in `normalize()`).
#[test]
fn claim_confidence_weights() {
    let cases: [(Language, f32); 29] = [
        (Language::RUST, 1.0),
        (Language::GO, 1.0),
        (Language::SWIFT, 1.0),
        (Language::KOTLIN, 1.0),
        (Language::TYPESCRIPT, 0.9),
        (Language::VUE, 0.85),
        (Language::SVELTE, 0.85),
        (Language::ASTRO, 0.85),
        (Language("java"), 0.85),
        (Language("csharp"), 0.85),
        (Language::PYTHON, 0.8),
        (Language("php"), 0.8),
        (Language("scala"), 0.8),
        (Language("elixir"), 0.8),
        (Language("erlang"), 0.8),
        (Language("dart"), 0.8),
        (Language::PLUGIN, 0.75),
        (Language("ruby"), 0.75),
        (Language("clojure"), 0.75),
        (Language::JAVASCRIPT, 0.7),
        (Language("c"), 0.7),
        (Language("cpp"), 0.7),
        (Language("haskell"), 0.7),
        (Language("lua"), 0.7),
        (Language("ocaml"), 0.7),
        (Language("perl"), 0.7),
        (Language("fsharp"), 0.7),
        (Language::SCSS, 0.5),
        (Language::HTML_CSS, 0.4),
    ];
    for (lang, want) in cases {
        let got = normalize(1.0, lang);
        assert!(
            approx(got, want),
            "confidence for {:?}: got {got}, documented {want}",
            lang
        );
    }
}

/// Floor guard: the active set must never silently shrink below the documented 28.
#[test]
fn claim_active_adapter_count_regression() {
    let reg = AdapterRegistry::default_registry();
    let probes = [
        "rs", "ts", "js", "py", "go", "swift", "kt", "vue", "svelte", "astro", "scss", "css", "c",
        "cpp", "cs", "java", "php", "scala", "clj", "hs", "ex", "erl", "lua", "ml", "pl", "fs",
        "rb", "dart", "jl", "r",
    ];
    let active = probes
        .iter()
        .filter(|ext| reg.resolve(Path::new(&format!("probe.{ext}"))).is_some())
        .count();
    assert!(
        active >= 28,
        "fewer than 28 documented adapters resolve: {active}"
    );
}
