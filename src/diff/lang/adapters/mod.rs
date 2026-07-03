pub mod astro;
pub mod common;
pub mod dart;
pub mod fsharp;
pub mod go;
pub mod htmlcss;
pub mod javascript;
pub mod kotlin;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod scss;
pub mod svelte;
pub mod swift;
pub mod typescript;
pub mod vue;

pub use astro::AstroAdapter;
pub use fsharp::FsharpAdapter;
pub use go::GoAdapter;
pub use htmlcss::HtmlCssAdapter;
pub use javascript::JavaScriptAdapter;
pub use kotlin::KotlinAdapter;
pub use python::PythonAdapter;
pub use ruby::RubyAdapter;
pub use rust::RustAdapter;
pub use scss::ScssAdapter;
pub use svelte::SvelteAdapter;
pub use swift::SwiftAdapter;
pub use typescript::TypeScriptAdapter;
pub use vue::VueAdapter;

use super::registry::AdapterRegistry;

pub fn register_builtin_adapters(registry: &mut AdapterRegistry) {
    registry.register(Box::new(RustAdapter));
    registry.register(Box::new(TypeScriptAdapter));
    registry.register(Box::new(JavaScriptAdapter));
    registry.register(Box::new(PythonAdapter));
    registry.register(Box::new(GoAdapter));
    registry.register(Box::new(SwiftAdapter));
    registry.register(Box::new(KotlinAdapter));
    registry.register(Box::new(VueAdapter));
    registry.register(Box::new(SvelteAdapter));
    registry.register(Box::new(AstroAdapter));
    registry.register(Box::new(ScssAdapter));
    registry.register(Box::new(HtmlCssAdapter));
}
