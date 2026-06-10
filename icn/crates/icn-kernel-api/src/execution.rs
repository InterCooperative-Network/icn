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
//!                    ↘ NotExecuted (terminal: nothing executable applied)
//!                    ↘ Failed → (retry) → Executing
//!                              ↘ PermanentlyFailed
//! ```

use serde::{Deserialize, Serialize};

use crate::effects::{EffectResult, KernelEffect};

/// Canonical key prefix for persisted execution records (`exec:<decision_hash>`).
///
/// Single source of truth shared by the execution store that writes these
/// records (`icn-core`'s `SledExecutionStore`) and any reader that scans them
/// (e.g. the gateway's dispatch-evidence backfill). Centralized here so the two
/// sides cannot silently drift to different prefixes.
pub const EXECUTION_RECORD_KEY_PREFIX: &str = "exec:";

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
    /// No executable kernel work was applied (terminal, not a retryable failure).
    ///
    /// Distinct from [`Failed`]: the pipeline ran but there was nothing to apply
    /// (e.g. `NoOp`, or outcomes mapped as structurally non-executed).
    NotExecuted,
    /// Execution failed (retryable).
    Failed,
    /// Exhausted retries or non-recoverable error.
    PermanentlyFailed,
}

/// Canonical execution-truth summary for boundary surfaces.
///
/// This is the tranche-2 contract for exposing whether execution was complete,
/// independent of aggregate status labels like [`ExecutionStatus::Confirmed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExecutionTruthSummary {
    /// Number of effect rows returned by dispatch.
    pub total_effects: usize,
    /// Number of effect rows that executed successfully.
    pub executed_effects: usize,
    /// Number of structurally non-executed rows (`not_executed = true`).
    pub not_executed_effects: usize,
    /// Number of hard-failure rows (`!success && !not_executed`).
    pub hard_failed_effects: usize,
    /// True only when all effect rows executed and none were skipped/failed.
    pub execution_complete: bool,
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

    /// The per-effect results produced by dispatch, stored alongside
    /// `effects` once execution reaches a terminal status.
    ///
    /// This is the durable input that lets a post-crash backfill re-derive
    /// `EffectDispatchEvidence` (Issue #1987) *without re-executing* the
    /// effects — the dispatch-evidence sink consumes `(effects, results)`
    /// pairs, so persisting the results here makes evidence persistence
    /// idempotently recoverable across a crash/restart in the async
    /// execution window. Empty for non-terminal records and for pre-upgrade
    /// records serialized before this field existed (`#[serde(default)]`).
    #[serde(default)]
    pub results: Vec<EffectResult>,

    /// Canonical execution-truth summary for audit/API/CLI boundary surfaces.
    ///
    /// `None` means this record predates tranche-2 summary persistence and
    /// consumers should use conservative fallback semantics.
    #[serde(default)]
    pub truth_summary: Option<ExecutionTruthSummary>,
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
            results: Vec::new(),
            truth_summary: None,
        }
    }

    /// Store the per-effect dispatch results on this record.
    ///
    /// Called once execution produces results (terminal status), so a later
    /// dispatch-evidence backfill can re-derive evidence from the durable
    /// `(effects, results)` pair without re-executing (Issue #1987).
    pub fn set_results(&mut self, results: Vec<EffectResult>) {
        self.results = results;
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

    /// Terminal completion: execution ran but no kernel effect was applied.
    ///
    /// Does not increment `retries` (this is not a hard failure).
    pub fn mark_not_executed(&mut self, note: impl Into<String>) {
        self.status = ExecutionStatus::NotExecuted;
        self.error = Some(note.into());
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
            ExecutionStatus::Confirmed
                | ExecutionStatus::NotExecuted
                | ExecutionStatus::PermanentlyFailed
        )
    }

    /// Return execution-truth summary with a conservative fallback for legacy rows.
    pub fn truth_summary_or_fallback(&self) -> ExecutionTruthSummary {
        if let Some(summary) = &self.truth_summary {
            return summary.clone();
        }

        // Legacy rows (pre-tranche-2) do not have canonical effect counters.
        // Expose conservative values to avoid over-reporting completeness.
        let total_effects = self.effects.len();
        let not_executed_effects = if matches!(self.status, ExecutionStatus::NotExecuted) {
            total_effects.max(1)
        } else {
            0
        };
        let hard_failed_effects = if matches!(
            self.status,
            ExecutionStatus::Failed | ExecutionStatus::PermanentlyFailed
        ) {
            1
        } else {
            0
        };
        let executed_effects = self.ledger_entry_ids.len().min(total_effects);
        let execution_complete = false;

        ExecutionTruthSummary {
            total_effects,
            executed_effects,
            not_executed_effects,
            hard_failed_effects,
            execution_complete,
        }
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

    /// Delete an execution record by decision hash.
    fn delete(&self, decision_hash: &str) -> anyhow::Result<()>;

    /// List records by status.
    fn list_by_status(&self, status: ExecutionStatus) -> anyhow::Result<Vec<ExecutionRecord>>;

    /// Count records by status.
    fn count_by_status(&self) -> anyhow::Result<std::collections::HashMap<ExecutionStatus, usize>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{EffectOutcome, EffectResult};

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
    fn test_execution_record_not_executed_is_terminal_without_retry_bump() {
        let mut record =
            ExecutionRecord::new_pending("nx-hash", "proposal-nx", "receipt-nx", vec![]);
        record.mark_executing();
        assert_eq!(record.retries, 0);
        record.mark_not_executed("nothing to apply");
        assert_eq!(record.status, ExecutionStatus::NotExecuted);
        assert!(record.is_terminal());
        assert_eq!(record.retries, 0);
        assert!(record.error.as_deref().is_some());
    }

    #[test]
    fn test_execution_status_serde() {
        let status = ExecutionStatus::Confirmed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"confirmed\"");

        let parsed: ExecutionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ExecutionStatus::Confirmed);

        let nx = ExecutionStatus::NotExecuted;
        let nx_json = serde_json::to_string(&nx).unwrap();
        assert_eq!(nx_json, "\"not_executed\"");
        let nx_parsed: ExecutionStatus = serde_json::from_str(&nx_json).unwrap();
        assert_eq!(nx_parsed, ExecutionStatus::NotExecuted);
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

    #[test]
    fn test_execution_record_backward_compat_without_effects() {
        // Pre-upgrade records serialized without the `effects` field
        // must deserialize successfully with an empty effects vec.
        let json = r#"{
            "decision_hash": "old-hash",
            "proposal_id": "old-proposal",
            "decision_receipt_id": "old-receipt",
            "status": "confirmed",
            "started_at": 1700000000,
            "finished_at": 1700000001,
            "ledger_entry_ids": ["e1"],
            "state_change_hashes": [],
            "error": null,
            "retries": 0
        }"#;

        let record: ExecutionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.decision_hash, "old-hash");
        assert_eq!(record.status, ExecutionStatus::Confirmed);
        assert!(
            record.effects.is_empty(),
            "effects should default to empty vec"
        );
    }

    /// The per-effect `results` produced at dispatch are persisted on the
    /// record so a post-crash backfill can re-derive `EffectDispatchEvidence`
    /// without re-executing the effects (Issue #1987).
    #[test]
    fn test_execution_record_persists_results_for_evidence_backfill() {
        let mut record = ExecutionRecord::new_pending("h-res", "prop-res", "receipt-res", vec![]);
        assert!(
            record.results.is_empty(),
            "a fresh record carries no dispatch results yet"
        );

        let results = vec![EffectResult {
            effect_id: "receipt-res".to_string(),
            success: true,
            message: "applied".to_string(),
            state_change_hash: Some("sch-1".to_string()),
            ledger_entry_id: Some("ledger-1".to_string()),
            not_executed: false,
            receipt_ref: Some("steward-hex".to_string()),
            outcome: Some(EffectOutcome::Applied),
        }];
        record.set_results(results.clone());
        record.mark_confirmed(vec!["ledger-1".into()], vec!["sch-1".into()]);

        let json = serde_json::to_string(&record).unwrap();
        let parsed: ExecutionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.results, results, "results survive a serde roundtrip");
        assert_eq!(
            parsed.results[0].receipt_ref.as_deref(),
            Some("steward-hex")
        );
    }

    /// Records persisted before the `results` field existed must still
    /// deserialize, defaulting to an empty results vec (additive, serde-default).
    #[test]
    fn test_execution_record_backward_compat_without_results() {
        let json = r#"{
            "decision_hash": "old-hash",
            "proposal_id": "old-proposal",
            "decision_receipt_id": "old-receipt",
            "status": "confirmed",
            "started_at": 1700000000,
            "finished_at": 1700000001,
            "ledger_entry_ids": ["e1"],
            "state_change_hashes": [],
            "error": null,
            "retries": 0,
            "effects": []
        }"#;

        let record: ExecutionRecord = serde_json::from_str(json).unwrap();
        assert!(
            record.results.is_empty(),
            "results should default to empty vec for pre-upgrade records"
        );
    }
}
