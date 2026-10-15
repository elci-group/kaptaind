//! Kaptaind observation corpus.
//!
//! Deterministic Kaptaind remains the authority. This module turns repository
//! history into traceable observation units and refuses to treat labelled
//! commits, Kaptaind-era commits, and recoverable analyses as the same thing.

pub mod census;
pub mod corpus;
pub mod dedupe;
pub mod extract;
pub mod gitio;
pub mod kds;
pub mod label;
pub mod metrics;
pub mod render;
pub mod represent;
pub mod schema;

pub use census::{census_estate, census_repository, CensusOptions};
pub use corpus::{build_manifest, export_corpus, load_manifest, read_jsonl, validate, write_jsonl};
pub use dedupe::{cluster_and_partition, representatives, PartitionPlan};
pub use extract::{extract, ExtractOptions};
pub use kds::{readiness, score, KdsReport};
pub use schema::{KaptaindObservation, SemverClass, TrainingPermission, KOU_SCHEMA_VERSION};
