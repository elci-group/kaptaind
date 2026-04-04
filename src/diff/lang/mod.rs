pub mod adapter;
pub mod heuristics;
pub mod registry;

pub use adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol};
pub use registry::AdapterRegistry;

pub fn normalize(score: f32, lang: Language) -> f32 {
    match lang {
        Language::Rust => score * 1.0,
        Language::Go => score * 1.0,
        Language::TypeScript => score * 0.9,
        Language::JavaScript => score * 0.7,
        Language::Python => score * 0.8,
        Language::HtmlCss => score * 0.4,
    }
}
