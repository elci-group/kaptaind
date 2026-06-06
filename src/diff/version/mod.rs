pub mod cache {
    pub use kaptaind_diff::diff_version::cache::*;
}
pub mod detector {
    pub use kaptaind_diff::diff_version::detector::*;
}

pub use kaptaind_diff::diff_version::{detect_all, LanguageVersion, VersionCache, VersionSource};
