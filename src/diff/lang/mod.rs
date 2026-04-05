pub mod adapter;
pub mod heuristics;
pub mod plugin;
pub mod registry;

pub use adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol};
pub use registry::AdapterRegistry;

pub fn normalize(score: f32, lang: Language) -> f32 {
    match lang {
        Language::Rust => score * 1.0,
        Language::Go => score * 1.0,
        Language::Swift => score * 1.0,
        Language::Kotlin => score * 1.0,
        Language::TypeScript => score * 0.9,
        Language::Vue | Language::Svelte | Language::Astro => score * 0.85,
        Language::Python => score * 0.8,
        Language::JavaScript => score * 0.7,
        Language::Scss => score * 0.5,
        Language::HtmlCss => score * 0.4,
        Language::Plugin => score * 0.75,
    }
}
