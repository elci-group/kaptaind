//! Repository lifecycle & promotion orchestration (ELCI KAPTAIND-RTL-001).
//!
//! Distinct from [`crate::lifecycle`] (Desktop/Mobile branch governance) and
//! [`crate::environment`] (external deployment evidence): this module treats
//! branch promotion as a declaratively configured state transition rather
//! than a sequence of Git operations. Source, target, eligibility, plan,
//! validation, execution, and recovery are represented and persisted
//! explicitly; a Git command's exit code is never on its own treated as
//! proof of the resulting repository state (§15, §25).
//!
//! Existing Kaptaind Git execution primitives are reused rather than
//! duplicated: [`git`] only adds the checkout-free plumbing this engine
//! needs on top of what [`crate::git::repo`] already provides.

pub mod batch;
pub mod engine;
pub mod feed;
pub mod git;
pub mod model;
pub mod policy;
pub mod queue;
pub mod store;

pub use batch::{
    plan_batch, promote_batch, status as batch_status, validate_batch, PromotionBatch,
};
pub use engine::{
    cancel, has_outstanding, history, inspect, plan, promote, recover, status, validate,
    InspectionReport,
};
pub use feed::{compute_metrics, read_events, PromotionFeedEvent, PromotionMetrics, Severity};
pub use model::{
    BranchSnapshot, Eligibility, Evidence, EvidenceKind, Operation, Promotion, PromotionPlan,
    PromotionStatus, ValidationGate,
};
pub use policy::LifecyclePolicy;
pub use queue::{
    drain, enqueue, list as list_queue, remove as remove_from_queue, DrainOutcome, QueuedRequest,
};
