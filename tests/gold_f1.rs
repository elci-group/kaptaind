//! Gold-label seed F1 harness (adapter-200, Workstream B).
//!
//! Loads `tests/fixtures/gold/labels.json` (hand-labeled expected public symbols for a
//! tiny, unambiguous seed corpus), runs each referenced adapter, and computes per-adapter
//! precision/recall/F1 over the seed. This is the first *true* (not corpora-smoke) F1
//! signal — scoped to the seed set, NOT project-wide. Full labeling is ongoing
//! (see `docs/planning/adapter-200/CALIBRATION.md`).
//!
//!   - `gold_seed_resolves_and_rust_baseline` (guard): seed files resolve to adapters and
//!     parse without error/panic, and the known-good (syn-based, pub-only) rust adapter
//!     holds F1 >= 0.99 on the seed — a regression that over/under-reports fails this.
//!   - `gold_f1_report` (`--ignored --nocapture`): prints the per-adapter P/R/F1 table.

use kaptaind::diff::lang::registry::AdapterRegistry;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

fn gold_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gold")
}

#[derive(Deserialize)]
struct Labels {
    files: Vec<FileLabel>,
}
#[derive(Deserialize)]
struct FileLabel {
    path: String,
    lang: String,
    symbols: Vec<Sym>,
}
#[derive(Deserialize)]
struct Sym {
    name: String,
    kind: String,
}

fn key(name: &str, kind: &str) -> String {
    format!("{name}\x00{kind}")
}

struct Counts {
    tp: usize,
    fp: usize,
    fn_: usize,
}

const MIN_GOLD_LANGUAGES: usize = 36;
const MIN_GOLD_SYMBOLS: usize = 161;
const MIN_SEED_F1: f64 = 0.99;

impl Counts {
    fn zero() -> Self {
        Self {
            tp: 0,
            fp: 0,
            fn_: 0,
        }
    }
    fn add(&mut self, tp: usize, fp: usize, fn_: usize) {
        self.tp += tp;
        self.fp += fp;
        self.fn_ += fn_;
    }
    fn precision(&self) -> f64 {
        let d = self.tp + self.fp;
        if d == 0 {
            1.0
        } else {
            self.tp as f64 / d as f64
        }
    }
    fn recall(&self) -> f64 {
        let d = self.tp + self.fn_;
        if d == 0 {
            1.0
        } else {
            self.tp as f64 / d as f64
        }
    }
    fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }
}

fn evaluate(labels_name: &str) -> BTreeMap<String, Counts> {
    let reg = AdapterRegistry::default_registry();
    let root = gold_root();
    let labels: Labels = {
        let s = std::fs::read_to_string(root.join(labels_name))
            .unwrap_or_else(|e| panic!("{labels_name} readable: {e}"));
        serde_json::from_str(&s).expect("labels valid")
    };
    let mut per_lang: BTreeMap<String, Counts> = BTreeMap::new();
    for fl in &labels.files {
        let path = root.join(&fl.path);
        let adapter = reg
            .resolve(&path)
            .unwrap_or_else(|| panic!("no adapter resolves for {}", fl.path));
        let ast = adapter
            .parse_ast(&path)
            .unwrap_or_else(|e| panic!("parse {} failed: {e}", fl.path));
        let emitted: HashSet<String> = ast.symbols.iter().map(|s| key(&s.name, &s.kind)).collect();
        let gold: HashSet<String> = fl.symbols.iter().map(|s| key(&s.name, &s.kind)).collect();
        let tp = emitted.intersection(&gold).count();
        let fp = emitted.difference(&gold).count();
        let fn_ = gold.difference(&emitted).count();
        per_lang
            .entry(fl.lang.clone())
            .or_insert_with(Counts::zero)
            .add(tp, fp, fn_);
    }
    per_lang
}

#[test]
fn gold_seed_resolves_and_rust_baseline() {
    let per_lang = evaluate("labels.json");
    assert!(
        !per_lang.is_empty(),
        "gold seed produced no evaluated files"
    );
    let rust = per_lang.get("rust").expect("rust present in gold seed");
    let f1 = rust.f1();
    assert!(
        f1 >= 0.99,
        "rust seed F1 regressed: {f1:.3} (tp={} fp={} fn={})",
        rust.tp,
        rust.fp,
        rust.fn_
    );
}

/// CI regression gate for the hand-labelled seed.  The threshold deliberately
/// applies to every labelled language, rather than extrapolating this small
/// corpus into a claim about unlabelled adapters.  A label or fixture change
/// that changes the seed's coverage must therefore be reviewed explicitly.
#[test]
fn gold_seed_quality_regression_guard() {
    let per_lang = evaluate("labels.json");
    assert!(
        per_lang.len() >= MIN_GOLD_LANGUAGES,
        "gold seed coverage regressed: expected at least {MIN_GOLD_LANGUAGES} labelled languages, got {}",
        per_lang.len()
    );

    let mut total = Counts::zero();
    for (lang, counts) in &per_lang {
        let labelled = counts.tp + counts.fn_;
        assert!(labelled > 0, "{lang}: gold seed has no labelled symbols");
        assert!(
            counts.f1() >= MIN_SEED_F1,
            "{lang}: seed F1 regressed below {MIN_SEED_F1:.2}: {:.3} (tp={} fp={} fn={})",
            counts.f1(),
            counts.tp,
            counts.fp,
            counts.fn_
        );
        total.add(counts.tp, counts.fp, counts.fn_);
    }
    assert!(
        total.tp + total.fn_ >= MIN_GOLD_SYMBOLS,
        "gold seed coverage regressed: expected at least {MIN_GOLD_SYMBOLS} labelled symbols, got {}",
        total.tp + total.fn_
    );
    assert!(
        total.f1() >= MIN_SEED_F1,
        "gold seed aggregate F1 regressed below {MIN_SEED_F1:.2}: {:.3} (tp={} fp={} fn={})",
        total.f1(),
        total.tp,
        total.fp,
        total.fn_
    );
}

fn print_report(per_lang: &BTreeMap<String, Counts>) {
    println!("\n| lang | TP | FP | FN | precision | recall | F1 |");
    println!("|------|----|----|----|-----------|--------|----|");
    let mut tot = Counts::zero();
    for (lang, c) in per_lang {
        println!(
            "| {lang} | {} | {} | {} | {:.3} | {:.3} | {:.3} |",
            c.tp,
            c.fp,
            c.fn_,
            c.precision(),
            c.recall(),
            c.f1()
        );
        tot.add(c.tp, c.fp, c.fn_);
    }
    println!(
        "| **TOTAL** | {} | {} | {} | {:.3} | {:.3} | {:.3} |",
        tot.tp,
        tot.fp,
        tot.fn_,
        tot.precision(),
        tot.recall(),
        tot.f1()
    );
}

/// On-demand report: `cargo test --test gold_f1 gold_f1_report -- --ignored --nocapture`
#[test]
#[ignore]
fn gold_f1_report() {
    print_report(&evaluate("labels.json"));
}

/// Messy real-world corpus report (human-oracle labels: comments, docstrings,
/// macro substrings). NOT CI-pinned — divergences here are measured evidence
/// for the confidence re-table, not regressions.
/// `cargo test --test gold_f1 gold_f1_messy_report -- --ignored --nocapture`
#[test]
#[ignore]
fn gold_f1_messy_report() {
    print_report(&evaluate("labels_messy.json"));
}
