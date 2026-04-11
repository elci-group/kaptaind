pub mod engine;
pub mod project;

pub use engine::{trawl, TrawlOptions, TrawlResult, DiscoveredProject};
pub use project::{detect_project_type, detect_project_type_with_confidence, ProjectType, Confidence, DetectionResult};
