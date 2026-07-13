pub mod adapter;
pub mod adapters;
pub mod plugin;
pub mod registry;

pub use adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol};
pub use registry::AdapterRegistry;

/// Normalize a raw API-surface score by language confidence. Tiers encode
/// STRUCTURAL capability, not corpus-measured rankings: parser-grade (rust) >
/// visibility-aware line parsers (go/swift/kotlin/java/csharp/scala/dart) >
/// export/underscore-gated (typescript/python/php/elixir/erlang/javascript/
/// ruby/clojure) > no-visibility line parsers (c/cpp/haskell/lua/ocaml/perl/
/// fsharp) > line/macro-granular (scss/htmlcss). Validated by the two-tier gold
/// corpus (clean + messy F1 1.000 across all 28 adapters; see
/// docs/planning/adapter-200/CALIBRATION.md rev-22..27). Re-table only on a
/// per-language real-world recall corpus — the probe corpora cannot resolve
/// 0.05-step tier placements.
pub fn normalize(score: f32, lang: Language) -> f32 {
    match lang.as_str() {
        "rust" | "go" | "swift" | "kotlin" => score * 1.0,
        "typescript" => score * 0.9,
        "vue" | "svelte" | "astro" | "java" | "csharp" => score * 0.85,
        "python" | "php" | "scala" | "elixir" | "erlang" | "dart" | "solidity" | "groovy"
        | "julia" | "r" | "zig" => score * 0.8,
        "plugin" | "ruby" | "clojure" => score * 0.75,
        "javascript" | "c" | "cpp" | "haskell" | "hcl" | "lua" | "ocaml" | "objc" | "perl"
        | "fsharp" | "sql" => score * 0.7,
        "scss" => score * 0.5,
        "htmlcss" => score * 0.4,
        _ => score * 0.75,
    }
}
