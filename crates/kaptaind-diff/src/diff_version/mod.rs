pub mod cache;
pub mod detector;

pub use cache::VersionCache;
pub use detector::{detect_all, LanguageVersion, VersionSource};
