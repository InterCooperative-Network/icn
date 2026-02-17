#![allow(clippy::unwrap_used, clippy::expect_used, clippy::expect_fun_call)]
//! Integration tests for the escrow release vertical slice.
//!
//! These tests prove the end-to-end path:
//! GovernanceDecision → TreasuryEffect::ReleaseEscrow → escrow check → ledger mutation
//!
//! Three required tests:
//! 1. Replay doesn't double-spend (idempotency)
//! 2. Crash mid-execution recovers correctly
//! 3. Conflicting release decisions are rejected

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use icn_kernel_api::effects::{KernelEffect, TreasuryEffect};
use icn_kernel_api::escrow::{EscrowRecord, EscrowStatus, EscrowStore};
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};
use icn_kernel_api::protocol_params::*;

use icn_core::supervisor::decision_executor::DecisionExecutor;
use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

// =============================================================================
// In-memory test doubles
// =============================================================================

/// In-memory escrow store for integration testing.
struct MemoryEscrowStore {
    records: RwLock<HashMap<String, EscrowRecord>>,
}

impl MemoryEscrowStore {
    fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }

    /// Pre-load an escrow for testing.
    fn insert(&self, record: EscrowRecord) {
        self.records
            .write()
            .unwrap()
            .insert(record.escrow_id.clone(), record);
    }

    /// Get current escrow state (test inspection).
    fn get_record(&self, escrow_id: &str) -> Option<EscrowRecord> {
        self.records.read().unwrap().get(escrow_id).cloned()
    }
}

impl EscrowStore for MemoryEscrowStore {
    fn get(&self, escrow_id: &str) -> anyhow::Result<Option<EscrowRecord>> {
        Ok(self.records.read().unwrap().get(escrow_id).cloned())
    }

    fn put(&self, record: &EscrowRecord) -> anyhow::Result<()> {
        self.records
            .write()
            .unwrap()
            .insert(record.escrow_id.clone(), record.clone());
        Ok(())
    }

    fn list_by_scope(&self, scope_id: &str) -> anyhow::Result<Vec<EscrowRecord>> {
        Ok(self
            .records
            .read()
            .unwrap()
            .values()
            .filter(|r| r.scope_id == scope_id)
            .cloned()
            .collect())
    }
}

/// In-memory execution store for integration testing.
struct MemoryExecutionStore {
    records: RwLock<HashMap<String, ExecutionRecord>>,
}

impl MemoryExecutionStore {
    fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }
}

impl ExecutionStore for MemoryExecutionStore {
    fn get(&self, decision_hash: &str) -> anyhow::Result<Option<ExecutionRecord>> {
        Ok(self.records.read().unwrap().get(decision_hash).cloned())
    }

    fn put(&self, record: &ExecutionRecord) -> anyhow::Result<()> {
        self.records
            .write()
            .unwrap()
            .insert(record.decision_hash.clone(), record.clone());
        Ok(())
    }

    fn list_by_status(&self, status: ExecutionStatus) -> anyhow::Result<Vec<ExecutionRecord>> {
        Ok(self
            .records
            .read()
            .unwrap()
            .values()
            .filter(|r| r.status == status)
            .cloned()
            .collect())
    }

    fn count_by_status(&self) -> anyhow::Result<HashMap<ExecutionStatus, usize>> {
        let mut counts = HashMap::new();
        for record in self.records.read().unwrap().values() {
            *counts.entry(record.status).or_insert(0) += 1;
        }
        Ok(counts)
    }
}

/// Minimal param store stub (no protocol params needed for escrow tests).
struct StubParamStore;

impl ProtocolParameterStore for StubParamStore {
    fn get(&self, _: &str) -> anyhow::Result<Option<ProtocolParameter>> {
        Ok(None)
    }
    fn get_effective(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> anyhow::Result<Option<ProtocolParameter>> {
        Ok(None)
    }
    fn set(
        &self,
        _: ProtocolParameter,
        _: Option<String>,
        _: Option<String>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    fn list(&self) -> anyhow::Result<Vec<ProtocolParameter>> {
        Ok(vec![])
    }
    fn list_by_category(&self, _: &str) -> anyhow::Result<Vec<ProtocolParameter>> {
        Ok(vec![])
    }
    fn get_history(&self, _: &str) -> anyhow::Result<Vec<ParameterChange>> {
        Ok(vec![])
    }
    fn get_history_paginated(
        &self,
        _: &str,
        _: usize,
        _: usize,
    ) -> anyhow::Result<(Vec<ParameterChange>, usize)> {
        Ok((vec![], 0))
    }
    fn prune_history(&self, _: &str, _: usize) -> anyhow::Result<usize> {
        Ok(0)
    }
    fn delete(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn exists(&self, _: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
    fn count(&self) -> anyhow::Result<usize> {
        Ok(0)
    }
    fn total_history_count(&self) -> anyhow::Result<usize> {
        Ok(0)
    }
    fn validate(
        &self,
        _: &str,
        _: &ParameterValue,
    ) -> std::result::Result<(), ParameterValidationError> {
        Ok(())
    }
    fn list_scoped_parameters(&self) -> anyhow::Result<Vec<ProtocolParameter>> {
        Ok(vec![])
    }
    fn delete_scoped_parameter(&self, _: &str, _: &ParameterScope) -> anyhow::Result<bool> {
        Ok(false)
    }
    fn add_pending_change(&self, _: PendingParameterChange) -> anyhow::Result<()> {
        Ok(())
    }
    fn get_pending_change(
        &self,
        _: &PendingChangeId,
    ) -> anyhow::Result<Option<PendingParameterChange>> {
        Ok(None)
    }
    fn list_pending_changes(&self) -> anyhow::Result<Vec<PendingParameterChange>> {
        Ok(vec![])
    }
    fn list_pending_changes_for_parameter(
        &self,
        _: &str,
    ) -> anyhow::Result<Vec<PendingParameterChange>> {
        Ok(vec![])
    }
    fn get_changes_due_before(&self, _: u64) -> anyhow::Result<Vec<PendingParameterChange>> {
        Ok(vec![])
    }
    fn update_pending_change(&self, _: PendingParameterChange) -> anyhow::Result<()> {
        Ok(())
    }
    fn cancel_pending_change(&self, _: &PendingChangeId, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn count_pending_changes(&self) -> anyhow::Result<usize> {
        Ok(0)
    }
}

// =============================================================================
// Test helpers
// =============================================================================

/// Build a test executor with a shared escrow store.
fn make_executor(
    escrow_store: Arc<MemoryEscrowStore>,
) -> (Arc<DecisionExecutor>, Arc<MemoryExecutionStore>) {
    let param_store = Arc::new(StubParamStore);
    let kernel_executor = KernelGovernanceExecutor::new(param_store)
        .with_escrow_store(escrow_store as Arc<dyn EscrowStore>);
    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_store = Arc::new(MemoryExecutionStore::new());
    let decision_executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));
    (decision_executor, exec_store)
}

/// Build a ReleaseEscrow effect.
fn release_escrow_effect(escrow_id: &str, decision_hash: &str) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::ReleaseEscrow {
        escrow_id: escrow_id.to_string(),
        treasury_did: "did:icn:treasury:coop-alpha".to_string(),
        beneficiary_did: "did:icn:member:alice".to_string(),
        amount: 5000,
        currency: "HOURS".to_string(),
        decision_receipt_id: format!("receipt-{}", decision_hash),
        decision_hash: decision_hash.to_string(),
    })]
}

// =============================================================================
// Test 1: Replay doesn't double-spend
// =============================================================================

/// Replay the same decision event 100 times. The escrow must be released
/// exactly once — subsequent replays are idempotent at two levels:
/// 1. Decision-level: ExecutionStore returns Confirmed → short-circuit
/// 2. Domain-level: EscrowRecord.release() returns AlreadyReleasedSameDecision
#[tokio::test]
async fn test_replay_does_not_double_spend() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    // Pre-load a locked escrow
    escrow_store.insert(EscrowRecord::new_locked(
        "esc-replay",
        "coop-alpha",
        "did:icn:treasury:coop-alpha",
        "did:icn:member:alice",
        5000,
        "HOURS",
    ));

    let (executor, exec_store) = make_executor(escrow_store.clone());

    // First execution — should succeed
    let results = executor
        .execute(
            release_escrow_effect("esc-replay", "hash-replay-1"),
            "receipt-hash-replay-1",
            "hash-replay-1",
            "proposal-replay",
        )
        .await
        .expect("first execution should succeed");

    assert_eq!(results.len(), 1);
    assert!(results[0].success, "First execution should succeed");

    // Verify escrow is now Released
    let escrow = escrow_store.get_record("esc-replay").unwrap();
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(
        escrow.release_decision_hash.as_deref(),
        Some("hash-replay-1")
    );

    // Verify execution record is Confirmed
    let record = exec_store.get("hash-replay-1").unwrap().unwrap();
    assert_eq!(record.status, ExecutionStatus::Confirmed);

    // Replay 99 more times — all should short-circuit via decision-level idempotency
    for i in 2..=100 {
        let results = executor
            .execute(
                release_escrow_effect("esc-replay", "hash-replay-1"),
                "receipt-hash-replay-1",
                "hash-replay-1",
                "proposal-replay",
            )
            .await
            .expect(&format!("replay {} should succeed", i));

        // DecisionExecutor returns empty vec for already-confirmed decisions
        assert!(
            results.is_empty(),
            "Replay {} should return empty (idempotent skip)",
            i
        );
    }

    // Escrow should still show exactly one release
    let escrow_final = escrow_store.get_record("esc-replay").unwrap();
    assert_eq!(escrow_final.status, EscrowStatus::Released);
    assert_eq!(
        escrow_final.release_decision_hash.as_deref(),
        Some("hash-replay-1"),
        "Should still be released by original decision"
    );
}

// =============================================================================
// Test 2: Crash mid-execution recovers correctly
// =============================================================================

/// Simulate crash recovery:
/// 1. First executor processes escrow release → Confirmed
/// 2. Simulate "crash" by creating a NEW executor with the SAME escrow store
///    but a FRESH execution store (simulating lost in-flight state)
/// 3. Replay the same decision → domain-level check catches it
///
/// Also tests the case where execution store shows Executing (crashed mid-flight):
/// the executor should re-enter and the escrow domain check prevents double-spend.
#[tokio::test]
async fn test_crash_recovery_no_double_spend() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    // Pre-load a locked escrow
    escrow_store.insert(EscrowRecord::new_locked(
        "esc-crash",
        "coop-alpha",
        "did:icn:treasury:coop-alpha",
        "did:icn:member:bob",
        3000,
        "HOURS",
    ));

    // --- Phase 1: Normal execution succeeds ---
    let (executor1, _exec_store1) = make_executor(escrow_store.clone());
    let results = executor1
        .execute(
            release_escrow_effect("esc-crash", "hash-crash-1"),
            "receipt-hash-crash-1",
            "hash-crash-1",
            "proposal-crash",
        )
        .await
        .expect("first execution should succeed");
    assert_eq!(results.len(), 1);
    assert!(results[0].success);

    // Escrow is now Released
    let escrow = escrow_store.get_record("esc-crash").unwrap();
    assert_eq!(escrow.status, EscrowStatus::Released);

    // --- Phase 2: "Crash" — create fresh executor with same escrow store ---
    // The execution store is fresh (simulates lost state), but escrow store
    // retains the Released status (it's durable).
    let (executor2, _exec_store2) = make_executor(escrow_store.clone());

    // Replay the same decision on the "recovered" node.
    // Since execution store is fresh, the decision-level check won't find it.
    // But the escrow domain-level check will catch it.
    let results2 = executor2
        .execute(
            release_escrow_effect("esc-crash", "hash-crash-1"),
            "receipt-hash-crash-1",
            "hash-crash-1",
            "proposal-crash",
        )
        .await
        .expect("crash recovery should not error");

    assert_eq!(results2.len(), 1);
    assert!(
        results2[0].success,
        "Domain-level idempotent release should return success: {}",
        results2[0].message
    );
    assert!(
        results2[0].message.contains("idempotent"),
        "Message should indicate idempotent release: {}",
        results2[0].message
    );

    // Escrow still shows original release
    let escrow_final = escrow_store.get_record("esc-crash").unwrap();
    assert_eq!(escrow_final.status, EscrowStatus::Released);
    assert_eq!(
        escrow_final.release_decision_hash.as_deref(),
        Some("hash-crash-1")
    );

    // --- Phase 3: Test Executing state (mid-flight crash) ---
    let escrow_store2 = Arc::new(MemoryEscrowStore::new());
    escrow_store2.insert(EscrowRecord::new_locked(
        "esc-midcrash",
        "coop-alpha",
        "did:icn:treasury:coop-alpha",
        "did:icn:member:carol",
        2000,
        "HOURS",
    ));

    let (executor3, exec_store3) = make_executor(escrow_store2.clone());

    // Manually insert an Executing record (simulates mid-flight crash)
    let mut crashed_record =
        ExecutionRecord::new_pending("hash-midcrash", "proposal-mid", "receipt-mid", vec![]);
    crashed_record.mark_executing();
    exec_store3.put(&crashed_record).unwrap();

    // Re-execute — should recover and complete
    let results3 = executor3
        .execute(
            release_escrow_effect("esc-midcrash", "hash-midcrash"),
            "receipt-mid",
            "hash-midcrash",
            "proposal-mid",
        )
        .await
        .expect("mid-crash recovery should succeed");

    assert_eq!(results3.len(), 1);
    assert!(results3[0].success, "Recovery should succeed");

    // Escrow should be released
    let escrow_mid = escrow_store2.get_record("esc-midcrash").unwrap();
    assert_eq!(escrow_mid.status, EscrowStatus::Released);
}

// =============================================================================
// Test 3: Conflicting release decisions are rejected
// =============================================================================

/// Two different governance decisions try to release the same escrow.
/// The first one succeeds. The second one is rejected with a conflict error.
#[tokio::test]
async fn test_conflicting_release_rejected() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    // Pre-load a locked escrow
    escrow_store.insert(EscrowRecord::new_locked(
        "esc-conflict",
        "coop-alpha",
        "did:icn:treasury:coop-alpha",
        "did:icn:member:dave",
        7000,
        "HOURS",
    ));

    let (executor, exec_store) = make_executor(escrow_store.clone());

    // Decision A releases the escrow
    let results_a = executor
        .execute(
            release_escrow_effect("esc-conflict", "hash-decision-A"),
            "receipt-A",
            "hash-decision-A",
            "proposal-A",
        )
        .await
        .expect("decision A should succeed");

    assert_eq!(results_a.len(), 1);
    assert!(results_a[0].success, "Decision A should succeed");

    // Verify escrow is released by decision A
    let escrow = escrow_store.get_record("esc-conflict").unwrap();
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(
        escrow.release_decision_hash.as_deref(),
        Some("hash-decision-A")
    );

    // Decision B tries to release the same escrow — different decision_hash
    let results_b = executor
        .execute(
            release_escrow_effect("esc-conflict", "hash-decision-B"),
            "receipt-B",
            "hash-decision-B",
            "proposal-B",
        )
        .await
        .expect("decision B should not error");

    assert_eq!(results_b.len(), 1);
    assert!(
        !results_b[0].success,
        "Decision B should FAIL (conflict): {}",
        results_b[0].message
    );
    assert!(
        results_b[0].message.contains("already released"),
        "Error message should mention already released: {}",
        results_b[0].message
    );
    assert!(
        results_b[0].message.contains("hash-decision-A"),
        "Error message should mention the conflicting decision: {}",
        results_b[0].message
    );

    // Verify decision B's execution record shows failure
    let record_b = exec_store.get("hash-decision-B").unwrap().unwrap();
    assert_eq!(
        record_b.status,
        ExecutionStatus::Failed,
        "Conflicting decision should be marked Failed"
    );

    // Escrow is still released by decision A (unchanged)
    let escrow_final = escrow_store.get_record("esc-conflict").unwrap();
    assert_eq!(
        escrow_final.release_decision_hash.as_deref(),
        Some("hash-decision-A"),
        "Escrow should still be released by decision A, not B"
    );
}

// =============================================================================
// Additional edge case tests
// =============================================================================

/// Releasing a nonexistent escrow should fail gracefully.
#[tokio::test]
async fn test_release_nonexistent_escrow_fails() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());
    // No escrow pre-loaded
    let (executor, _) = make_executor(escrow_store);

    let results = executor
        .execute(
            release_escrow_effect("esc-does-not-exist", "hash-ghost"),
            "receipt-ghost",
            "hash-ghost",
            "proposal-ghost",
        )
        .await
        .expect("should not error");

    assert_eq!(results.len(), 1);
    assert!(!results[0].success, "Should fail for missing escrow");
    assert!(results[0].message.contains("not found"));
}

/// Releasing a cancelled escrow should fail.
#[tokio::test]
async fn test_release_cancelled_escrow_fails() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    let mut cancelled = EscrowRecord::new_locked(
        "esc-cancelled",
        "coop-alpha",
        "did:icn:treasury:coop-alpha",
        "did:icn:member:eve",
        1000,
        "HOURS",
    );
    cancelled.status = EscrowStatus::Cancelled;
    escrow_store.insert(cancelled);

    let (executor, _) = make_executor(escrow_store);

    let results = executor
        .execute(
            release_escrow_effect("esc-cancelled", "hash-cancel"),
            "receipt-cancel",
            "hash-cancel",
            "proposal-cancel",
        )
        .await
        .expect("should not error");

    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert!(results[0].message.contains("cancelled"));
}
