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

use icn_core::services::LedgerServiceImpl;
use icn_core::supervisor::decision_executor::DecisionExecutor;
use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;
use icn_identity::Did;
use icn_kernel_api::AllowAllOracle;
use icn_ledger::Ledger;
use icn_store::SledStore;
use tempfile::TempDir;
use tokio::sync::RwLock as TokioRwLock;

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

    fn delete(&self, decision_hash: &str) -> anyhow::Result<()> {
        self.records.write().unwrap().remove(decision_hash);
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

// =============================================================================
// Test helpers
// =============================================================================

/// Treasury DID on ReleaseEscrow effects in this module (valid multibase).
const ESCROW_TEST_TREASURY_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
const ESCROW_BENEFICIARY_ALICE: &str = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse";
const ESCROW_BENEFICIARY_BOB: &str = "did:icn:z9hSR6S7WPtxmTojgo6GG3k4yDPecgJY292j7xrsUGWBu";
const ESCROW_BENEFICIARY_CAROL: &str = "did:icn:zEdmxWPmx2WH6WgFfTdu9xfkYf3k1g5wD1zccTVySEEh1";
const ESCROW_BENEFICIARY_DAVE: &str = "did:icn:z3b5C8qPPH1U8t8jxS7UB2A1mvz5Z5dNLEcEtW3VQ8Zba";

/// Build a test executor with a shared escrow store and temp ledger for treasury release.
fn make_executor(
    escrow_store: Arc<MemoryEscrowStore>,
) -> (Arc<DecisionExecutor>, Arc<MemoryExecutionStore>, TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ledger_path = tmp.path().join("ledger");
    std::fs::create_dir_all(&ledger_path).unwrap();
    let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
    let ledger = Ledger::new(ledger_store).unwrap();
    let ledger = Arc::new(TokioRwLock::new(ledger));
    let treasury: Did = ESCROW_TEST_TREASURY_DID.parse().unwrap();
    let ledger_service = Arc::new(LedgerServiceImpl::new(
        ledger,
        Arc::new(AllowAllOracle::wildcard()),
        treasury,
    ));
    let param_store = Arc::new(StubParamStore);
    let kernel_executor = KernelGovernanceExecutor::new(param_store)
        .with_escrow_store(escrow_store as Arc<dyn EscrowStore>)
        .with_ledger_service(ledger_service);
    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_store = Arc::new(MemoryExecutionStore::new());
    let decision_executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));
    (decision_executor, exec_store, tmp)
}

/// Build a ReleaseEscrow effect.
fn release_escrow_effect(escrow_id: &str, decision_hash: &str) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::ReleaseEscrow {
        escrow_id: escrow_id.to_string(),
        treasury_did: ESCROW_TEST_TREASURY_DID.to_string(),
        beneficiary_did: ESCROW_BENEFICIARY_ALICE.to_string(),
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
#[tokio::test(flavor = "multi_thread")]
async fn test_replay_does_not_double_spend() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    // Pre-load a locked escrow
    escrow_store.insert(EscrowRecord::new_locked(
        "esc-replay",
        "coop-alpha",
        ESCROW_TEST_TREASURY_DID,
        ESCROW_BENEFICIARY_ALICE,
        5000,
        "HOURS",
    ));

    let (executor, exec_store, _tmp) = make_executor(escrow_store.clone());

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
#[tokio::test(flavor = "multi_thread")]
async fn test_crash_recovery_no_double_spend() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    // Pre-load a locked escrow
    escrow_store.insert(EscrowRecord::new_locked(
        "esc-crash",
        "coop-alpha",
        ESCROW_TEST_TREASURY_DID,
        ESCROW_BENEFICIARY_BOB,
        3000,
        "HOURS",
    ));

    // --- Phase 1: Normal execution succeeds ---
    let (executor1, _exec_store1, _tmp1) = make_executor(escrow_store.clone());
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
    let (executor2, _exec_store2, _tmp2) = make_executor(escrow_store.clone());

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
        ESCROW_TEST_TREASURY_DID,
        ESCROW_BENEFICIARY_CAROL,
        2000,
        "HOURS",
    ));

    let (executor3, exec_store3, _tmp3) = make_executor(escrow_store2.clone());

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
#[tokio::test(flavor = "multi_thread")]
async fn test_conflicting_release_rejected() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    // Pre-load a locked escrow
    escrow_store.insert(EscrowRecord::new_locked(
        "esc-conflict",
        "coop-alpha",
        ESCROW_TEST_TREASURY_DID,
        ESCROW_BENEFICIARY_DAVE,
        7000,
        "HOURS",
    ));

    let (executor, exec_store, _tmp) = make_executor(escrow_store.clone());

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
#[tokio::test(flavor = "multi_thread")]
async fn test_release_nonexistent_escrow_fails() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());
    // No escrow pre-loaded
    let (executor, _, _tmp) = make_executor(escrow_store);

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
#[tokio::test(flavor = "multi_thread")]
async fn test_release_cancelled_escrow_fails() {
    let escrow_store = Arc::new(MemoryEscrowStore::new());

    let mut cancelled = EscrowRecord::new_locked(
        "esc-cancelled",
        "coop-alpha",
        ESCROW_TEST_TREASURY_DID,
        ESCROW_BENEFICIARY_ALICE,
        1000,
        "HOURS",
    );
    cancelled.status = EscrowStatus::Cancelled;
    escrow_store.insert(cancelled);

    let (executor, _, _tmp) = make_executor(escrow_store);

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
