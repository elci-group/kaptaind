pub mod engine;
pub mod project;

pub use engine::{trawl, DiscoveredProject, TrawlOptions, TrawlResult};
pub use project::{
    detect_project_type, detect_project_type_with_confidence, Confidence, DetectionResult,
    ProjectType,
};
