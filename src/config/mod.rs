// Pre-existing manual `Default` impls in `loader.rs` are equivalent to a derive and
// are intentionally left untouched per project guidance. Allow the lint so the
// rest of the crate can remain warning-free under `-D warnings`.
#![allow(clippy::derivable_impls)]

pub mod loader;

pub use loader::Config;
