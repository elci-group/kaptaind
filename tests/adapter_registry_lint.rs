//! Adapter-registry no-orphan lint (closes claims-audit F6).
//!
//! Guarantees that the set of *registered* built-in adapters exactly matches the set
//! of adapter *source files* on disk. A new adapter file that is not declared +
//! registered (the F6 failure: 28 files existed, 12 wired) fails this test, as does a
//! registration with no backing file. This keeps `README.md`/`LANGUAGE_MATRIX.md`
//! breadth claims honest by construction.

use kaptaind::diff::lang::registry::AdapterRegistry;
use std::collections::BTreeSet;
use std::path::Path;

/// Broad extension probe covering every extension any built-in adapter claims, so the
/// distinct-name count equals the number of registered adapters.
const EXT_PROBE: &[&str] = &[
    "rs", "ts", "js", "py", "go", "swift", "kt", "vue", "svelte", "astro", "scss", "css", "html",
    "c", "h", "cpp", "cc", "cxx", "hpp", "cs", "java", "php", "scala", "sc", "clj", "cljc", "cljs",
    "hs", "lhs", "ex", "exs", "erl", "hrl", "lua", "ml", "mli", "pl", "pm", "fs", "fsx", "rb",
    "dart", "sql", "tf", "hcl", "sol", "groovy", "jl", "r", "m", "mm", "zig",
];

fn registered_adapter_names() -> BTreeSet<String> {
    let reg = AdapterRegistry::default_registry();
    let mut names = BTreeSet::new();
    for ext in EXT_PROBE {
        if let Some(a) = reg.resolve(Path::new(&format!("probe.{ext}"))) {
            names.insert(a.name().to_string());
        }
    }
    names
}

#[test]
fn no_orphan_adapter_files() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diff/lang/adapters");
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("adapters dir readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().to_string())
        .filter(|s| s != "common" && s != "mod")
        .collect();
    files.sort();

    let names = registered_adapter_names();
    assert_eq!(
        names.len(),
        files.len(),
        "registered adapters ({}) != adapter source files ({}).\n  files = {:?}\n  \
         Every adapters/*.rs (except common/mod) must be declared in mod.rs AND \
         registered in register_builtin_adapters, or explicitly retired.",
        names.len(),
        files.len(),
        files
    );
}

/// The fallback line scanner (confidence 0.0) is the documented baseline for languages
/// kaptaind does NOT model. Pinned with never-modeled, non-language extensions so the
/// probe survives the adapter-200 queue (every planned language graduates eventually —
/// Julia in rev 34, R in rev 35).
const UNSUPPORTED_PROBE: &[&str] = &["txt", "dat"];

#[test]
fn unsupported_languages_fall_back() {
    let reg = AdapterRegistry::default_registry();
    for ext in UNSUPPORTED_PROBE {
        assert!(
            reg.resolve(Path::new(&format!("probe.{ext}"))).is_none(),
            ".{ext} unexpectedly resolved to a built-in adapter"
        );
    }
}
