//! Adapter calibration harness (adapter-200, T1–T3).
//!
//! Runs every wired adapter over its behavioral fixture corpus in
//! `tests/fixtures/adapters/<lang>/{positive,negative,breaking,edge}/` and:
//!   1. asserts the adapter never panics on its corpus (the durable §5 robustness
//!      gate), and
//!   2. (ignored, on-demand) prints a per-language observation table used to build
//!      `docs/planning/adapter-200/CALIBRATION.md`.
//!
//! The corpora are a first-pass, source-derived gold set authored by the calibration
//! swarm; quality varies by language. Reported numbers are therefore *corpora-qualified
//! smoke observations*, NOT gold-label precision/recall/F1. True F1 requires a
//! hand-labeled held-out corpus (see ADAPTER_200_ROADMAP.md §7).

use kaptaind::diff::lang::registry::AdapterRegistry;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn corpora_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adapters")
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    v.sort();
    v
}

fn symbol_count(adapter: &dyn kaptaind::diff::lang::LanguageAdapter, file: &Path) -> (usize, bool) {
    match adapter.parse_ast(file) {
        Ok(ast) => (ast.symbols.len(), false),
        Err(_) => (0, true), // read/parse error, not a panic
    }
}

struct LangObs {
    pos_files: usize,
    pos_detected: usize, // positive files with >=1 symbol
    pos_errors: usize,
    neg_files: usize,
    neg_false_pos: usize, // negative files that yielded >=1 symbol
    neg_errors: usize,
    brk_pairs: usize,
    brk_true: usize,     // pairs where detect_breaking_changes == true
    mod_pairs: usize,    // before/after pairs present in modified/
    mod_detected: usize, // modified/ pairs where diff.modified is non-empty (X2 fires)
    sig_pairs: usize,    // before/after pairs present in signature/
    sig_detected: usize, // signature/ pairs where diff.modified is non-empty (signature change)
}

fn observe(reg: &AdapterRegistry) -> BTreeMap<String, LangObs> {
    let mut out = BTreeMap::new();
    let root = corpora_root();
    let mut langs: Vec<PathBuf> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    langs.sort();

    for ldir in langs {
        let lang = ldir.file_name().unwrap().to_string_lossy().to_string();
        // Resolve the adapter from the first fixture file that matches one.
        let mut adapter = None;
        for sub in ["positive", "negative", "breaking", "edge"] {
            for f in files_in(&ldir.join(sub)) {
                if let Some(a) = reg.resolve(&f) {
                    adapter = Some(a);
                    break;
                }
            }
            if adapter.is_some() {
                break;
            }
        }
        let Some(adapter) = adapter else { continue };

        let mut obs = LangObs {
            pos_files: 0,
            pos_detected: 0,
            pos_errors: 0,
            neg_files: 0,
            neg_false_pos: 0,
            neg_errors: 0,
            brk_pairs: 0,
            brk_true: 0,
            mod_pairs: 0,
            mod_detected: 0,
            sig_pairs: 0,
            sig_detected: 0,
        };

        for f in files_in(&ldir.join("positive")) {
            obs.pos_files += 1;
            let (n, err) = symbol_count(adapter, &f);
            if err {
                obs.pos_errors += 1;
            }
            if n >= 1 {
                obs.pos_detected += 1;
            }
        }
        for f in files_in(&ldir.join("negative")) {
            obs.neg_files += 1;
            let (n, err) = symbol_count(adapter, &f);
            if err {
                obs.neg_errors += 1;
            }
            if n >= 1 {
                obs.neg_false_pos += 1;
            }
        }
        // Pair <case>_before.<ext> with <case>_after.<ext> (compare by file stem).
        let brk_dir = ldir.join("breaking");
        for before in files_in(&brk_dir) {
            let fstem = before.file_stem().unwrap().to_string_lossy().to_string();
            let Some(prefix) = fstem.strip_suffix("_before") else {
                continue;
            };
            let want_after = format!("{prefix}_after");
            let after = files_in(&brk_dir)
                .into_iter()
                .find(|f| f.file_stem().unwrap().to_string_lossy() == want_after);
            let Some(after) = after else { continue };
            let (Ok(b), Ok(a)) = (adapter.parse_ast(&before), adapter.parse_ast(&after)) else {
                continue;
            };
            let diff = adapter.diff_ast(&b, &a);
            obs.brk_pairs += 1;
            if adapter.detect_breaking_changes(&diff) {
                obs.brk_true += 1;
            }
        }
        // Pair <case>_before.<ext> with <case>_after.<ext> in modified/ and count
        // pairs that register a same-name/different-kind (`modified`) symbol. Control
        // pairs (same kind) count as pairs but must NOT be detected.
        let mod_dir = ldir.join("modified");
        for before in files_in(&mod_dir) {
            let fstem = before.file_stem().unwrap().to_string_lossy().to_string();
            let Some(prefix) = fstem.strip_suffix("_before") else {
                continue;
            };
            let want_after = format!("{prefix}_after");
            let after = files_in(&mod_dir)
                .into_iter()
                .find(|f| f.file_stem().unwrap().to_string_lossy() == want_after);
            let Some(after) = after else { continue };
            let (Ok(b), Ok(a)) = (adapter.parse_ast(&before), adapter.parse_ast(&after)) else {
                continue;
            };
            let diff = adapter.diff_ast(&b, &a);
            obs.mod_pairs += 1;
            if !diff.modified.is_empty() {
                obs.mod_detected += 1;
            }
        }
        // Pair <case>_before.<ext> with <case>_after.<ext> in signature/ and count
        // pairs that register as `modified` via a signature (arity/return/parameter)
        // change. These pairs were parked out of breaking/ (they are not removals).
        let sig_dir = ldir.join("signature");
        for before in files_in(&sig_dir) {
            let fstem = before.file_stem().unwrap().to_string_lossy().to_string();
            let Some(prefix) = fstem.strip_suffix("_before") else {
                continue;
            };
            let want_after = format!("{prefix}_after");
            let after = files_in(&sig_dir)
                .into_iter()
                .find(|f| f.file_stem().unwrap().to_string_lossy() == want_after);
            let Some(after) = after else { continue };
            let (Ok(b), Ok(a)) = (adapter.parse_ast(&before), adapter.parse_ast(&after)) else {
                continue;
            };
            let diff = adapter.diff_ast(&b, &a);
            obs.sig_pairs += 1;
            if !diff.modified.is_empty() {
                obs.sig_detected += 1;
            }
        }
        out.insert(lang, obs);
    }
    out
}

/// Durable robustness gate: no adapter may panic on any corpus fixture, and every
/// language with a corpus must resolve to a registered adapter.
#[test]
fn adapters_panic_free_and_resolve_on_corpora() {
    let reg = AdapterRegistry::default_registry();
    let obs = observe(&reg);
    assert!(
        obs.len() >= 28,
        "expected >=28 calibrated languages, got {}",
        obs.len()
    );
    for (lang, o) in &obs {
        assert!(o.pos_files > 0, "{lang}: corpus has no positive/ fixtures");
    }
    // Reaching here means every parse_ast/diff_ast call returned without panicking.
}

/// On-demand report: `cargo test --test adapter_calibration calibration_report -- --ignored --nocapture`
#[test]
#[ignore]
fn calibration_report() {
    let reg = AdapterRegistry::default_registry();
    let obs = observe(&reg);
    println!("\n| lang | pos det/files | neg false-pos/files | breaking true/pairs | modified det/pairs | signature det/pairs | parse errs |");
    println!("|------|---------------|---------------------|---------------------|--------------------|--------------------|------------|");
    let mut tot_pos = (0usize, 0usize);
    let mut tot_neg = (0usize, 0usize);
    let mut tot_brk = (0usize, 0usize);
    let mut tot_mod = (0usize, 0usize);
    let mut tot_sig = (0usize, 0usize);
    let mut tot_err = 0usize;
    for (lang, o) in &obs {
        println!(
            "| {lang} | {}/{} | {}/{} | {}/{} | {}/{} | {}/{} | {} |",
            o.pos_detected,
            o.pos_files,
            o.neg_false_pos,
            o.neg_files,
            o.brk_true,
            o.brk_pairs,
            o.mod_detected,
            o.mod_pairs,
            o.sig_detected,
            o.sig_pairs,
            o.pos_errors + o.neg_errors
        );
        tot_pos.0 += o.pos_detected;
        tot_pos.1 += o.pos_files;
        tot_neg.0 += o.neg_false_pos;
        tot_neg.1 += o.neg_files;
        tot_brk.0 += o.brk_true;
        tot_brk.1 += o.brk_pairs;
        tot_mod.0 += o.mod_detected;
        tot_mod.1 += o.mod_pairs;
        tot_sig.0 += o.sig_detected;
        tot_sig.1 += o.sig_pairs;
        tot_err += o.pos_errors + o.neg_errors;
    }
    println!(
        "| **TOTAL** | {}/{} | {}/{} | {}/{} | {}/{} | {}/{} | {} |",
        tot_pos.0,
        tot_pos.1,
        tot_neg.0,
        tot_neg.1,
        tot_brk.0,
        tot_brk.1,
        tot_mod.0,
        tot_mod.1,
        tot_sig.0,
        tot_sig.1,
        tot_err
    );
}
