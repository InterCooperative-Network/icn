//! Execution record types for the Decision Executor.
//!
//! These types persist the status of governance decision execution,
//! providing idempotency (no double-execution) and auditability
//! (every decision's execution history is durable).
//!
//! # Idempotency
//!
//! The idempotency key is `decision_hash` — the canonical blake3 hash
//! from `GovernanceDecisionReceipt`. This hash is deterministic across
//! nodes for the same decision, so replayed events are safely rejected.
//!
//! # Status Machine
//!
//! ```text
//! Pending → Executing → Confirmed
//!                    ↘ Failed → (retry) → Executing
//!                              ↘ PermanentlyFailed
//! ```

use serde::{Deserialize, Serialize};

use crate::effects::KernelEffect;

/// Status of a governance decision's execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Recorded but not yet started.
    Pending,
    /// Execution in progress.
    Executing,
    /// All effects applied successfully.
    Confirmed,
    /// Execution failed (retryable).
    Failed,
    /// Exhausted retries or non-recoverable error.
    PermanentlyFailed,
}

/// Persistent record of a governance decision execution.
///
/// Keyed by `decision_hash` in the execution store.
/// Survives restarts, prevents re-execution, enables audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    /// Canonical decision hash (blake3, 32 bytes hex-encoded).
    /// This is the idempotency key.
    pub decision_hash: String,

    /// The proposal ID that produced this decision.
    pub proposal_id: String,

    /// The decision receipt ID linking to `GovernanceDecisionReceipt`.
    pub decision_receipt_id: String,

    /// Current execution status.
    pub status: ExecutionStatus,

    /// Unix timestamp (seconds) when execution was first attempted.
    pub started_at: u64,

    /// Unix timestamp (seconds) when execution completed (if any).
    pub finished_at: Option<u64>,

    /// Ledger entry IDs produced by this execution.
    pub ledger_entry_ids: Vec<String>,

    /// State change hashes from effect results.
    pub state_change_hashes: Vec<String>,

    /// Error message if execution failed.
    pub error: Option<String>,

    /// Number of retry attempts.
    pub retries: u32,

    /// The kernel effects to execute (stored for crash recovery).
    /// Empty for pre-existing records that don't have effects stored.
    #[serde(default)]
    pub effects: Vec<KernelEffect>,
}

impl ExecutionRecord {
    /// Create a new pending execution record.
    pub fn new_pending(
        decision_hash: impl Into<String>,
        proposal_id: impl Into<String>,
        decision_receipt_id: impl Into<String>,
        effects: Vec<KernelEffect>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            decision_hash: decision_hash.into(),
            proposal_id: proposal_id.into(),
            decision_receipt_id: decision_receipt_id.into(),
            status: ExecutionStatus::Pending,
            started_at: now,
            finished_at: None,
            ledger_entry_ids: Vec::new(),
            state_change_hashes: Vec::new(),
            error: None,
            retries: 0,
            effects,
        }
    }

    /// Transition to Executing.
    pub fn mark_executing(&mut self) {
        self.status = ExecutionStatus::Executing;
    }

    /// Transition to Confirmed with results.
    pub fn mark_confirmed(
        &mut self,
        ledger_entry_ids: Vec<String>,
        state_change_hashes: Vec<String>,
    ) {
        self.status = ExecutionStatus::Confirmed;
        self.ledger_entry_ids = ledger_entry_ids;
        self.state_change_hashes = state_change_hashes;
        self.finished_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    /// Transition to Failed with error.
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = ExecutionStatus::Failed;
        self.error = Some(error.into());
        self.retries += 1;
        self.finished_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    /// Transition to PermanentlyFailed.
    pub fn mark_permanently_failed(&mut self, error: impl Into<String>) {
        self.status = ExecutionStatus::PermanentlyFailed;
        self.error = Some(error.into());
        self.finished_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    /// Whether this record represents a terminal state (no more transitions).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::Confirmed | ExecutionStatus::PermanentlyFailed
        )
    }
}

/// Trait for persistent storage of execution records.
///
/// Implementations must be durable (survive restarts).
/// The store is keyed by `decision_hash`.
pub trait ExecutionStore: Send + Sync {
    /// Get an execution record by decision hash.
    fn get(&self, decision_hash: &str) -> anyhow::Result<Option<ExecutionRecord>>;

    /// Insert or update an execution record.
    fn put(&self, record: &ExecutionRecord) -> anyhow::Result<()>;

    /// List records by status.
    fn list_by_status(&self, status: ExecutionStatus) -> anyhow::Result<Vec<ExecutionRecord>>;

    /// Count records by status.
    fn count_by_status(&self) -> anyhow::Result<std::collections::HashMap<ExecutionStatus, usize>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_record_lifecycle() {
        let mut record =
            ExecutionRecord::new_pending("abc123hash", "proposal-42", "receipt-42", vec![]);

        assert_eq!(record.status, ExecutionStatus::Pending);
        assert!(!record.is_terminal());

        record.mark_executing();
        assert_eq!(record.status, ExecutionStatus::Executing);

        record.mark_confirmed(vec!["entry-1".to_string()], vec!["hash-1".to_string()]);
        assert_eq!(record.status, ExecutionStatus::Confirmed);
        assert!(record.is_terminal());
        assert!(record.finished_at.is_some());
        assert_eq!(record.ledger_entry_ids, vec!["entry-1"]);
    }

    #[test]
    fn test_execution_record_failure_path() {
        let mut record =
            ExecutionRecord::new_pending("abc123hash", "proposal-42", "receipt-42", vec![]);

        record.mark_executing();
        record.mark_failed("Ledger unavailable");

        assert_eq!(record.status, ExecutionStatus::Failed);
        assert_eq!(record.retries, 1);
        assert_eq!(record.error.as_deref(), Some("Ledger unavailable"));
        assert!(!record.is_terminal());

        record.mark_permanently_failed("Max retries exceeded");
        assert!(record.is_terminal());
    }

    #[test]
    fn test_execution_status_serde() {
        let status = ExecutionStatus::Confirmed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"confirmed\"");

        let parsed: ExecutionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ExecutionStatus::Confirmed);
    }

    #[test]
    fn test_execution_record_serde_roundtrip() {
        let mut record =
            ExecutionRecord::new_pending("deadbeef", "proposal-1", "receipt-1", vec![]);
        record.mark_confirmed(vec!["e1".into()], vec!["h1".into()]);

        let json = serde_json::to_string(&record).unwrap();
        let parsed: ExecutionRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.decision_hash, "deadbeef");
        assert_eq!(parsed.status, ExecutionStatus::Confirmed);
        assert_eq!(parsed.ledger_entry_ids, vec!["e1"]);
    }
}
