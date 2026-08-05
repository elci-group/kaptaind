pub mod controller;
pub mod intent;
pub mod provider_matrix;
pub mod task_distribution;

pub use controller::{push, push_multi_remote, MultiRemotePushOptions, PushOptions};
pub use intent::{detect_intent, select_providers_by_intent, validate_saturated_config, ProviderCapabilities};
pub use provider_matrix::{ProviderMatrix, TaskType};
pub use task_distribution::{TaskDistribution, TaskDistributionConfig, TaskDistributionEngine};
