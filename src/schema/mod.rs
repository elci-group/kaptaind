//! `.kaptaind` semantic-state schema subsystem.
//!
//! Treats the `.kaptaind/state.toml` document as a versioned contract:
//! format identity, capabilities, canonical serialization with a stable
//! digest, validation, and a deterministic stepwise migrator with an
//! append-only migration ledger. The daemon never rewrites the document
//! implicitly; `kaptaind-cli migrate` is the only mutation path.

pub mod document;
pub mod migrator;
pub mod registry;
pub mod version;

pub use document::{
    Baseline, Capabilities, Exception, Invariant, KaptaindHeader, MemoryBudget, SemanticDocument,
    Surface, VersioningPolicy,
};
pub use migrator::{
    append_ledger, detect, legacy_document, migrate_document, plan, AppliedStep, LedgerEntry,
    Migration,
};
pub use registry::{find, latest_version, schemas, SchemaInfo};
pub use version::SchemaVersion;

/// File the semantic-state document lives in, relative to `repo_path`.
pub const STATE_FILE: &str = ".kaptaind/state.toml";

/// Directory holding the append-only migration ledger.
pub const MIGRATIONS_DIR: &str = ".kaptaind/migrations";
