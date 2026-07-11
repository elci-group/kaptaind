pub mod adapter;
pub mod adapters;
pub mod plugin;
pub mod registry;

pub use adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol};
pub use registry::AdapterRegistry;

pub fn normalize(score: f32, lang: Language) -> f32 {
    match lang.as_str() {
        "rust" | "go" | "swift" | "kotlin" => score * 1.0,
        "typescript" => score * 0.9,
        "vue" | "svelte" | "astro" | "java" | "csharp" => score * 0.85,
        "python" | "php" | "scala" | "elixir" | "erlang" | "dart" => score * 0.8,
        "plugin" | "ruby" | "clojure" => score * 0.75,
        "javascript" | "c" | "cpp" | "haskell" | "lua" | "ocaml" | "perl" | "fsharp" => score * 0.7,
        "scss" => score * 0.5,
        "htmlcss" => score * 0.4,
        _ => score * 0.75,
    }
}
