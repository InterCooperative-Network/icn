#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for budget enforcement vertical slice.
//!
//! These tests exercise the full flow: CreateBudget → Spend → enforcement
//! through the DecisionExecutor → EffectDispatcher → KernelGovernanceExecutor
//! → BudgetStore pipeline.
//!
//! Three required scenarios:
//! 1. Overspend prevention — spend exceeding budget is rejected
//! 2. Replay idempotency — replayed decision produces no double-spend
//! 3. Crash recovery — budget stays safe across saga boundaries

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use icn_core::services::LedgerServiceImpl;
use icn_identity::Did;
use icn_kernel_api::budget::{BudgetRecord, BudgetStore};
use icn_kernel_api::effects::{KernelEffect, TreasuryEffect};
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};
use icn_kernel_api::protocol_params::StubParamStore;
use icn_kernel_api::AllowAllOracle;
use icn_ledger::Ledger;
use icn_store::SledStore;
use tempfile::TempDir;
use tokio::sync::RwLock as TokioRwLock;

use anyhow::Result;

// Valid `did:icn:z…` strings (LedgerServiceImpl parses treasury as icn_identity::Did).
const TREASURY_OPS: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
const TREASURY_CRASH: &str = "did:icn:z3b5C8qPPH1U8t8jxS7UB2A1mvz5Z5dNLEcEtW3VQ8Zba";
const TREASURY_IDEM: &str = "did:icn:zEdmxWPmx2WH6WgFfTdu9xfkYf3k1g5wD1zccTVySEEh1";
const RECIPIENT_ALICE: &str = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse";
const RECIPIENT_BOB: &str = "did:icn:z9hSR6S7WPtxmTojgo6GG3k4yDPecgJY292j7xrsUGWBu";
const RECIPIENT_CAROL: &str = "did:icn:zEdmxWPmx2WH6WgFfTdu9xfkYf3k1g5wD1zccTVySEEh1";

// ============================================================================
// Test infrastructure
// ============================================================================

/// In-memory execution store for testing.
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
    fn get(&self, decision_hash: &str) -> Result<Option<ExecutionRecord>> {
        Ok(self.records.read().unwrap().get(decision_hash).cloned())
    }

    fn put(&self, record: &ExecutionRecord) -> Result<()> {
        self.records
            .write()
            .unwrap()
            .insert(record.decision_hash.clone(), record.clone());
        Ok(())
    }

    fn delete(&self, decision_hash: &str) -> Result<()> {
        self.records.write().unwrap().remove(decision_hash);
        Ok(())
    }

    fn list_by_status(&self, status: ExecutionStatus) -> Result<Vec<ExecutionRecord>> {
        Ok(self
            .records
            .read()
            .unwrap()
            .values()
            .filter(|r| r.status == status)
            .cloned()
            .collect())
    }

    fn count_by_status(&self) -> Result<HashMap<ExecutionStatus, usize>> {
        let mut counts = HashMap::new();
        for record in self.records.read().unwrap().values() {
            *counts.entry(record.status).or_insert(0) += 1;
        }
        Ok(counts)
    }
}

/// In-memory budget store for testing.
struct MemoryBudgetStore {
    records: RwLock<HashMap<String, BudgetRecord>>,
}

impl MemoryBudgetStore {
    fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }

    fn get_record(&self, budget_id: &str) -> Option<BudgetRecord> {
        self.records.read().unwrap().get(budget_id).cloned()
    }
}

impl BudgetStore for MemoryBudgetStore {
    fn get(&self, budget_id: &str) -> Result<Option<BudgetRecord>> {
        Ok(self.records.read().unwrap().get(budget_id).cloned())
    }

    fn put(&self, record: &BudgetRecord) -> Result<()> {
        self.records
            .write()
            .unwrap()
            .insert(record.budget_id.clone(), record.clone());
        Ok(())
    }

    fn list_by_scope(&self, scope_id: &str) -> Result<Vec<BudgetRecord>> {
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

/// Build a DecisionExecutor with budget store and temp ledger (treasury DID must match effects).
fn make_test_executor(
    budget_store: Arc<MemoryBudgetStore>,
    ledger_treasury_did: &str,
) -> (
    Arc<icn_core::supervisor::decision_executor::DecisionExecutor>,
    Arc<MemoryExecutionStore>,
    TempDir,
) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ledger_path = tmp.path().join("ledger");
    std::fs::create_dir_all(&ledger_path).unwrap();
    let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
    let ledger = Ledger::new(ledger_store).unwrap();
    let ledger = Arc::new(TokioRwLock::new(ledger));
    let treasury: Did = ledger_treasury_did.parse().expect("treasury did");
    let ledger_service = Arc::new(LedgerServiceImpl::new(
        ledger,
        Arc::new(AllowAllOracle::wildcard()),
        treasury,
    ));
    let param_store = Arc::new(StubParamStore);
    let kernel_executor = Arc::new(
        icn_core::supervisor::governance_executor::KernelGovernanceExecutor::new(param_store)
            .with_budget_store(budget_store)
            .with_ledger_service(ledger_service),
    );
    let dispatcher =
        Arc::new(icn_core::supervisor::effect_dispatcher::EffectDispatcher::new(kernel_executor));
    let exec_store = Arc::new(MemoryExecutionStore::new());

    let decision_executor = Arc::new(
        icn_core::supervisor::decision_executor::DecisionExecutor::new(
            dispatcher,
            exec_store.clone(),
        ),
    );

    (decision_executor, exec_store, tmp)
}

/// Helper: create a CreateBudget effect.
fn create_budget_effect(
    budget_id: &str,
    treasury_did: &str,
    total_amount: i64,
    decision_hash: &str,
) -> KernelEffect {
    KernelEffect::Treasury(TreasuryEffect::CreateBudget {
        treasury_did: treasury_did.to_string(),
        budget_id: budget_id.to_string(),
        total_amount,
        currency: "HOURS".to_string(),
        name: format!("Test budget {}", budget_id),
        validity_start: 0,
        validity_end: u64::MAX,
        decision_receipt_id: format!("receipt:{}", decision_hash),
        decision_hash: decision_hash.to_string(),
    })
}

/// Helper: create a Spend effect with budget_id.
fn spend_from_budget_effect(
    budget_id: &str,
    treasury_did: &str,
    recipient_did: &str,
    amount: i64,
    decision_hash: &str,
    expected_nonce: u64,
) -> KernelEffect {
    KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: treasury_did.to_string(),
        recipient_did: recipient_did.to_string(),
        amount,
        currency: "HOURS".to_string(),
        memo: "Test spend".to_string(),
        budget_id: Some(budget_id.to_string()),
        expected_nonce,
        decision_receipt_id: format!("receipt:{}", decision_hash),
        decision_hash: decision_hash.to_string(),
    })
}

// ============================================================================
// Test 1: Overspend Prevention
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_budget_overspend_rejected() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let treasury_did = TREASURY_OPS;
    let (executor, exec_store, _tmp) = make_test_executor(budget_store.clone(), treasury_did);

    // Step 1: Create budget with 1000 limit
    let create_effects = vec![create_budget_effect(
        "budget-ops-2024",
        treasury_did,
        1000,
        "hash:create-budget",
    )];
    let results = executor
        .execute(
            create_effects,
            "receipt:create-budget",
            "hash:create-budget",
            "proposal:create-budget",
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].success, "CreateBudget should succeed");

    // Verify budget record was created
    let budget = budget_store.get_record("budget-ops-2024").unwrap();
    assert_eq!(budget.total_limit, 1000);
    assert_eq!(budget.spent_total, 0);

    // Step 2: Spend 600 (within budget)
    let spend_ok = vec![spend_from_budget_effect(
        "budget-ops-2024",
        treasury_did,
        RECIPIENT_ALICE,
        600,
        "hash:spend-600",
        0,
    )];
    let results = executor
        .execute(
            spend_ok,
            "receipt:spend-600",
            "hash:spend-600",
            "proposal:spend-600",
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].success,
        "Spend 600 should succeed (within budget)"
    );

    // Verify budget updated
    let budget = budget_store.get_record("budget-ops-2024").unwrap();
    assert_eq!(budget.spent_total, 600);
    assert_eq!(budget.remaining(), 400);

    // Step 3: Attempt to spend 500 (exceeds remaining 400)
    let spend_over = vec![spend_from_budget_effect(
        "budget-ops-2024",
        treasury_did,
        RECIPIENT_BOB,
        500,
        "hash:spend-over",
        1,
    )];
    let results = executor
        .execute(
            spend_over,
            "receipt:spend-over",
            "hash:spend-over",
            "proposal:spend-over",
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        !results[0].success,
        "Overspend should be rejected: {}",
        results[0].message
    );
    assert!(
        results[0].message.contains("over limit") || results[0].message.contains("remaining=400"),
        "Error should explain budget constraint: {}",
        results[0].message
    );

    // Verify budget unchanged
    let budget = budget_store.get_record("budget-ops-2024").unwrap();
    assert_eq!(
        budget.spent_total, 600,
        "spent_total should be unchanged after rejection"
    );
    assert!(
        budget.pending_spend.is_none(),
        "No pending spend should remain after rejection"
    );

    // Step 4: Verify the overspend was marked as Failed in execution store
    let exec_record = exec_store.get("hash:spend-over").unwrap().unwrap();
    assert_eq!(exec_record.status, ExecutionStatus::Failed);

    // Step 5: Spend exactly 400 (use remaining budget)
    let spend_exact = vec![spend_from_budget_effect(
        "budget-ops-2024",
        treasury_did,
        RECIPIENT_CAROL,
        400,
        "hash:spend-exact",
        1,
    )];
    let results = executor
        .execute(
            spend_exact,
            "receipt:spend-exact",
            "hash:spend-exact",
            "proposal:spend-exact",
        )
        .await
        .unwrap();
    assert!(results[0].success, "Exact remaining spend should succeed");

    let budget = budget_store.get_record("budget-ops-2024").unwrap();
    assert_eq!(budget.spent_total, 1000);
    assert_eq!(budget.remaining(), 0);
    assert_eq!(
        budget.status,
        icn_kernel_api::budget::BudgetStatus::Exceeded
    );
}

// ============================================================================
// Test 2: Replay Idempotency
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_budget_spend_replay_idempotent() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let treasury_did = TREASURY_OPS;
    let (executor, _exec_store, _tmp) = make_test_executor(budget_store.clone(), treasury_did);

    // Step 1: Create budget
    let create_effects = vec![create_budget_effect(
        "budget-replay",
        treasury_did,
        1000,
        "hash:create-replay",
    )];
    executor
        .execute(
            create_effects,
            "receipt:create-replay",
            "hash:create-replay",
            "proposal:create-replay",
        )
        .await
        .unwrap();

    // Step 2: Spend 300
    let spend = vec![spend_from_budget_effect(
        "budget-replay",
        treasury_did,
        RECIPIENT_ALICE,
        300,
        "hash:spend-300",
        0,
    )];
    let results = executor
        .execute(
            spend.clone(),
            "receipt:spend-300",
            "hash:spend-300",
            "proposal:spend-300",
        )
        .await
        .unwrap();
    assert!(results[0].success);

    let budget_after_first = budget_store.get_record("budget-replay").unwrap();
    assert_eq!(budget_after_first.spent_total, 300);

    // Step 3: Replay same decision — should be caught by DecisionExecutor
    let results2 = executor
        .execute(
            spend,
            "receipt:spend-300",
            "hash:spend-300",
            "proposal:spend-300",
        )
        .await
        .unwrap();
    assert!(
        results2.is_empty(),
        "Replayed decision should return empty (idempotent skip)"
    );

    // Verify budget unchanged after replay
    let budget_after_replay = budget_store.get_record("budget-replay").unwrap();
    assert_eq!(
        budget_after_replay.spent_total, 300,
        "spent_total must not change on replay"
    );
}

// ============================================================================
// Test 3: Crash Recovery (saga safety)
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_budget_crash_recovery_saga_safe() {
    // This test verifies that if we crash between begin_spend and confirm_spend,
    // the budget stays with a pending_spend and can be retried.
    let budget_store = Arc::new(MemoryBudgetStore::new());

    // Manually create a budget with a pending spend (simulating crash state)
    let mut budget = BudgetRecord::new(
        "budget-crash".into(),
        TREASURY_CRASH.into(),
        TREASURY_CRASH.into(),
        "HOURS".into(),
        1000,
        "hash:create-crash".into(),
        0,
    );

    // Simulate: begin_spend was called but confirm_spend was not (process crashed)
    budget.begin_spend("hash:spend-crash", 500).unwrap();
    budget_store.put(&budget).unwrap();

    // Verify the crash state
    let crashed = budget_store.get_record("budget-crash").unwrap();
    assert!(crashed.pending_spend.is_some());
    assert_eq!(crashed.spent_total, 0, "spent_total not yet incremented");

    // Now create executor and retry the spend
    let (executor, _exec_store, _tmp) = make_test_executor(budget_store.clone(), TREASURY_CRASH);

    let retry_spend = vec![spend_from_budget_effect(
        "budget-crash",
        TREASURY_CRASH,
        RECIPIENT_ALICE,
        500,
        "hash:spend-crash",
        0,
    )];

    let results = executor
        .execute(
            retry_spend,
            "receipt:spend-crash",
            "hash:spend-crash",
            "proposal:spend-crash",
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(
        results[0].success,
        "Retry after crash should succeed: {}",
        results[0].message
    );

    // Verify budget was properly confirmed
    let recovered = budget_store.get_record("budget-crash").unwrap();
    assert_eq!(recovered.spent_total, 500, "spent_total should be updated");
    assert!(
        recovered.pending_spend.is_none(),
        "pending_spend should be cleared"
    );
    assert_eq!(recovered.remaining(), 500);
}

// ============================================================================
// Test 4: CreateBudget idempotency
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_create_budget_idempotent() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, _exec_store, _tmp) = make_test_executor(budget_store.clone(), TREASURY_IDEM);

    let create = vec![create_budget_effect(
        "budget-idem",
        TREASURY_IDEM,
        5000,
        "hash:create-idem",
    )];

    // First creation
    let results = executor
        .execute(
            create.clone(),
            "receipt:create-idem",
            "hash:create-idem",
            "proposal:create-idem",
        )
        .await
        .unwrap();
    assert!(results[0].success);

    // The DecisionExecutor will catch the replay at decision level
    let results2 = executor
        .execute(
            create,
            "receipt:create-idem",
            "hash:create-idem",
            "proposal:create-idem",
        )
        .await
        .unwrap();
    assert!(
        results2.is_empty(),
        "Replayed CreateBudget caught by DecisionExecutor"
    );

    // Verify budget unchanged
    let budget = budget_store.get_record("budget-idem").unwrap();
    assert_eq!(budget.total_limit, 5000);
    assert_eq!(budget.spent_total, 0);
}

// ============================================================================
// Test 5: Spend without budget (backward compatibility)
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_spend_without_budget_passes_through() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, _exec_store, _tmp) = make_test_executor(budget_store.clone(), TREASURY_IDEM);

    // Spend with no budget_id — should pass through to treasury executor directly
    let spend = vec![KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: TREASURY_IDEM.to_string(),
        recipient_did: RECIPIENT_ALICE.to_string(),
        amount: 1000,
        currency: "HOURS".to_string(),
        memo: "No budget".to_string(),
        budget_id: None,
        expected_nonce: 0,
        decision_receipt_id: "receipt:no-budget".to_string(),
        decision_hash: "hash:no-budget".to_string(),
    })];

    let results = executor
        .execute(
            spend,
            "receipt:no-budget",
            "hash:no-budget",
            "proposal:no-budget",
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(
        results[0].success,
        "Spend without budget should pass through: {}",
        results[0].message
    );
}

// ============================================================================
// Test 6: Budget queryability
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_budget_state_queryable() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let treasury_did = TREASURY_OPS;
    let (executor, _exec_store, _tmp) = make_test_executor(budget_store.clone(), treasury_did);

    // Create two budgets for same scope
    executor
        .execute(
            vec![create_budget_effect(
                "budget-a",
                treasury_did,
                5000,
                "hash:create-a",
            )],
            "receipt:create-a",
            "hash:create-a",
            "proposal:create-a",
        )
        .await
        .unwrap();

    executor
        .execute(
            vec![create_budget_effect(
                "budget-b",
                treasury_did,
                3000,
                "hash:create-b",
            )],
            "receipt:create-b",
            "hash:create-b",
            "proposal:create-b",
        )
        .await
        .unwrap();

    // Spend from budget-a
    executor
        .execute(
            vec![spend_from_budget_effect(
                "budget-a",
                treasury_did,
                RECIPIENT_ALICE,
                2000,
                "hash:spend-a",
                0,
            )],
            "receipt:spend-a",
            "hash:spend-a",
            "proposal:spend-a",
        )
        .await
        .unwrap();

    // Query: list budgets by scope
    let budgets = budget_store.list_by_scope(treasury_did).unwrap();
    assert_eq!(budgets.len(), 2, "Should have 2 budgets for this scope");

    // Query: individual budget state
    let budget_a = budget_store.get_record("budget-a").unwrap();
    assert_eq!(budget_a.total_limit, 5000);
    assert_eq!(budget_a.spent_total, 2000);
    assert_eq!(budget_a.remaining(), 3000);

    let budget_b = budget_store.get_record("budget-b").unwrap();
    assert_eq!(budget_b.total_limit, 3000);
    assert_eq!(budget_b.spent_total, 0);
    assert_eq!(budget_b.remaining(), 3000);
}
