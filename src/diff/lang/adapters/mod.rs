pub mod astro;
pub mod c;
pub mod clojure;
pub mod common;
pub mod cpp;
pub mod csharp;
pub mod dart;
pub mod elixir;
pub mod erlang;
pub mod fsharp;
pub mod go;
pub mod groovy;
pub mod haskell;
pub mod hcl;
pub mod htmlcss;
pub mod java;
pub mod javascript;
pub mod julia;
pub mod kotlin;
pub mod lua;
pub mod objc;
pub mod ocaml;
pub mod perl;
pub mod php;
pub mod python;
pub mod r;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod scss;
pub mod solidity;
pub mod sql;
pub mod svelte;
pub mod swift;
pub mod typescript;
pub mod vue;
pub mod zig;

pub use astro::AstroAdapter;
pub use c::CAdapter;
pub use clojure::ClojureAdapter;
pub use cpp::CppAdapter;
pub use csharp::CsharpAdapter;
pub use dart::DartAdapter;
pub use elixir::ElixirAdapter;
pub use erlang::ErlangAdapter;
pub use fsharp::FsharpAdapter;
pub use go::GoAdapter;
pub use groovy::GroovyAdapter;
pub use haskell::HaskellAdapter;
pub use hcl::HclAdapter;
pub use htmlcss::HtmlCssAdapter;
pub use java::JavaAdapter;
pub use javascript::JavaScriptAdapter;
pub use julia::JuliaAdapter;
pub use kotlin::KotlinAdapter;
pub use lua::LuaAdapter;
pub use objc::ObjCAdapter;
pub use ocaml::OcamlAdapter;
pub use perl::PerlAdapter;
pub use php::PhpAdapter;
pub use python::PythonAdapter;
pub use r::RAdapter;
pub use ruby::RubyAdapter;
pub use rust::RustAdapter;
pub use scala::ScalaAdapter;
pub use scss::ScssAdapter;
pub use solidity::SolidityAdapter;
pub use sql::SqlAdapter;
pub use svelte::SvelteAdapter;
pub use swift::SwiftAdapter;
pub use typescript::TypeScriptAdapter;
pub use vue::VueAdapter;
pub use zig::ZigAdapter;

use super::registry::AdapterRegistry;

pub fn register_builtin_adapters(registry: &mut AdapterRegistry) {
    // Original 12 (registration order preserved for resolve() precedence).
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
    // T1/T2/T3 promotions: previously orphaned adapters, now wired.
    // `.h` resolves to C (registered before Cpp); Cpp owns .cpp/.cc/.cxx/.hpp.
    registry.register(Box::new(CAdapter));
    registry.register(Box::new(CppAdapter));
    registry.register(Box::new(CsharpAdapter));
    registry.register(Box::new(JavaAdapter));
    registry.register(Box::new(PhpAdapter));
    registry.register(Box::new(ScalaAdapter));
    registry.register(Box::new(ClojureAdapter));
    registry.register(Box::new(HaskellAdapter));
    registry.register(Box::new(ElixirAdapter));
    registry.register(Box::new(ErlangAdapter));
    registry.register(Box::new(LuaAdapter));
    registry.register(Box::new(OcamlAdapter));
    registry.register(Box::new(PerlAdapter));
    registry.register(Box::new(FsharpAdapter));
    registry.register(Box::new(RubyAdapter));
    registry.register(Box::new(DartAdapter));
    // T2 promotion (adapter-200 item 10): SQL schema objects as API surface.
    registry.register(Box::new(SqlAdapter));
    // T2 promotion (adapter-200 item 10): Terraform/HCL labeled blocks as API surface.
    registry.register(Box::new(HclAdapter));
    // T2 promotion (adapter-200 item 10): Solidity ABI surface with selector-form signatures.
    registry.register(Box::new(SolidityAdapter));
    // T2 promotion (adapter-200 item 10): Groovy public-by-default members + properties.
    registry.register(Box::new(GroovyAdapter));
    // T2 promotion (adapter-200 item 10): Julia convention-gated surface + struct fields.
    registry.register(Box::new(JuliaAdapter));
    // T2 promotion (adapter-200 item 10): R function assignments + R6/S4 classes.
    registry.register(Box::new(RAdapter));
    // T2 promotion (adapter-200 item 10): Objective-C runtime surface with selector identity.
    registry.register(Box::new(ObjCAdapter));
    // T2 promotion (adapter-200 item 10): Zig explicit-pub surface + struct fields.
    registry.register(Box::new(ZigAdapter));
}
