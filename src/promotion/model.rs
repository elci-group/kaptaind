//! Domain model for repository lifecycle promotion (ELCI KAPTAIND-RTL-001).
//!
//! A promotion is not a Git command's exit code: it is an explicit,
//! evidence-backed state transition that a Git operation later realises.
//! Every type here is serialisable so the CLI's `--json` surface and the
//! append-only [`super::store`] can share one representation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    /// Resolve to [`Operation::FastForward`] or [`Operation::Merge`] at plan
    /// time from actual ancestry (ELCI KAPTAIND-RTL-001 §11: "given the
    /// current repository state... what repository operation produces the
    /// authorised target state?"). A persisted [`super::PromotionPlan`]
    /// never carries `Auto` itself — planning always records the concrete,
    /// resolved operation as evidence of the decision (§11 "the decision
    /// SHALL be recorded").
    Auto,
    Merge,
    Rebase,
    FastForward,
    CherryPick,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Auto => "auto",
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::FastForward => "fast-forward",
            Self::CherryPick => "cherry-pick",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchSnapshot {
    pub branch: String,
    pub role: String,
    pub protected: bool,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    Fact,
    Observation,
    Validation,
    PolicyDecision,
    ExecutionResult,
}

/// One material fact behind a decision. Kaptaind never substitutes a
/// model-generated assertion for evidence (ELCI KAPTAIND-RTL-001 §16).
#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub check: String,
    pub result: bool,
    pub source: String,
    pub detail: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl Evidence {
    pub fn new(
        kind: EvidenceKind,
        check: impl Into<String>,
        result: bool,
        source: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            check: check.into(),
            result,
            source: source.into(),
            detail: None,
            timestamp: Utc::now(),
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Machine-readable eligibility result for one configured transition,
/// computed without mutating the repository (ELCI KAPTAIND-RTL-001 §8).
#[derive(Debug, Clone, Serialize)]
pub struct Eligibility {
    pub transition: String,
    pub source_branch: String,
    pub target_branch: String,
    pub eligible: bool,
    pub blocking_reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub evidence: Vec<Evidence>,
}

/// The non-mutating output of `lifecycle plan` (ELCI KAPTAIND-RTL-001 §9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionPlan {
    pub repository: String,
    pub transition: String,
    pub source_branch: String,
    pub target_branch: String,
    pub source_role: String,
    pub target_role: String,
    pub source_revision: String,
    pub target_revision: String,
    pub commits: usize,
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub conflicts: usize,
    pub operation: Operation,
    pub requires: Vec<String>,
}

/// The canonical promotion lifecycle (ELCI KAPTAIND-RTL-001 §15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromotionStatus {
    Requested,
    Inspected,
    Planned,
    Validated,
    AwaitingApproval,
    Approved,
    Executing,
    Verifying,
    Completed,
    Blocked,
    Failed,
    Cancelled,
    RecoveryRequired,
}

impl PromotionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    /// True while a promotion still claims exclusive intent to move this
    /// transition's target. A new `plan` for the same source/target must
    /// wait for the holder to complete, fail, block, or be cancelled first
    /// (ELCI KAPTAIND-RTL-001 §8 "outstanding promotions", §26).
    pub fn holds_transition_lock(self) -> bool {
        matches!(
            self,
            Self::Requested
                | Self::Inspected
                | Self::Planned
                | Self::Validated
                | Self::AwaitingApproval
                | Self::Approved
                | Self::Executing
                | Self::Verifying
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationGate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionEvent {
    pub at: DateTime<Utc>,
    pub status: PromotionStatus,
    pub note: Option<String>,
}

/// A stable-identity promotion record, persisted after every state change
/// (ELCI KAPTAIND-RTL-001 §14, §28).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promotion {
    pub id: String,
    pub repository: String,
    pub plan: PromotionPlan,
    pub status: PromotionStatus,
    pub gates: Vec<ValidationGate>,
    pub approved_by: Option<String>,
    pub executed_operation: Option<Operation>,
    pub result_commit: Option<String>,
    pub failure_reason: Option<String>,
    pub recovery_action: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub history: Vec<PromotionEvent>,
}

impl Promotion {
    pub fn transition_to(&mut self, status: PromotionStatus, note: Option<String>) {
        self.status = status;
        self.updated_at = Utc::now();
        self.history.push(PromotionEvent {
            at: self.updated_at,
            status,
            note,
        });
    }
}
