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

use icn_kernel_api::budget::{BudgetRecord, BudgetStore};
use icn_kernel_api::effects::{KernelEffect, TreasuryEffect};
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};

use anyhow::Result;

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

/// Stub protocol parameter store.
struct StubParamStore;

impl icn_kernel_api::protocol_params::ProtocolParameterStore for StubParamStore {
    fn get(&self, _: &str) -> Result<Option<icn_kernel_api::protocol_params::ProtocolParameter>> {
        Ok(None)
    }
    fn get_effective(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Option<icn_kernel_api::protocol_params::ProtocolParameter>> {
        Ok(None)
    }
    fn set(
        &self,
        _: icn_kernel_api::protocol_params::ProtocolParameter,
        _: Option<String>,
        _: Option<String>,
    ) -> Result<()> {
        Ok(())
    }
    fn list(&self) -> Result<Vec<icn_kernel_api::protocol_params::ProtocolParameter>> {
        Ok(vec![])
    }
    fn list_by_category(
        &self,
        _: &str,
    ) -> Result<Vec<icn_kernel_api::protocol_params::ProtocolParameter>> {
        Ok(vec![])
    }
    fn get_history(
        &self,
        _: &str,
    ) -> Result<Vec<icn_kernel_api::protocol_params::ParameterChange>> {
        Ok(vec![])
    }
    fn get_history_paginated(
        &self,
        _: &str,
        _: usize,
        _: usize,
    ) -> Result<(Vec<icn_kernel_api::protocol_params::ParameterChange>, usize)> {
        Ok((vec![], 0))
    }
    fn prune_history(&self, _: &str, _: usize) -> Result<usize> {
        Ok(0)
    }
    fn delete(&self, _: &str) -> Result<()> {
        Ok(())
    }
    fn exists(&self, _: &str) -> Result<bool> {
        Ok(false)
    }
    fn count(&self) -> Result<usize> {
        Ok(0)
    }
    fn total_history_count(&self) -> Result<usize> {
        Ok(0)
    }
    fn validate(
        &self,
        _: &str,
        _: &icn_kernel_api::protocol_params::ParameterValue,
    ) -> std::result::Result<(), icn_kernel_api::protocol_params::ParameterValidationError> {
        Ok(())
    }
    fn list_scoped_parameters(
        &self,
    ) -> Result<Vec<icn_kernel_api::protocol_params::ProtocolParameter>> {
        Ok(vec![])
    }
    fn delete_scoped_parameter(
        &self,
        _: &str,
        _: &icn_kernel_api::protocol_params::ParameterScope,
    ) -> Result<bool> {
        Ok(false)
    }
    fn add_pending_change(
        &self,
        _: icn_kernel_api::protocol_params::PendingParameterChange,
    ) -> Result<()> {
        Ok(())
    }
    fn get_pending_change(
        &self,
        _: &icn_kernel_api::protocol_params::PendingChangeId,
    ) -> Result<Option<icn_kernel_api::protocol_params::PendingParameterChange>> {
        Ok(None)
    }
    fn list_pending_changes(
        &self,
    ) -> Result<Vec<icn_kernel_api::protocol_params::PendingParameterChange>> {
        Ok(vec![])
    }
    fn list_pending_changes_for_parameter(
        &self,
        _: &str,
    ) -> Result<Vec<icn_kernel_api::protocol_params::PendingParameterChange>> {
        Ok(vec![])
    }
    fn get_changes_due_before(
        &self,
        _: u64,
    ) -> Result<Vec<icn_kernel_api::protocol_params::PendingParameterChange>> {
        Ok(vec![])
    }
    fn update_pending_change(
        &self,
        _: icn_kernel_api::protocol_params::PendingParameterChange,
    ) -> Result<()> {
        Ok(())
    }
    fn cancel_pending_change(
        &self,
        _: &icn_kernel_api::protocol_params::PendingChangeId,
        _: &str,
    ) -> Result<()> {
        Ok(())
    }
    fn count_pending_changes(&self) -> Result<usize> {
        Ok(0)
    }
}

/// Build a DecisionExecutor with budget store wired in.
fn make_test_executor(
    budget_store: Arc<MemoryBudgetStore>,
) -> (
    Arc<icn_core::supervisor::decision_executor::DecisionExecutor>,
    Arc<MemoryExecutionStore>,
) {
    let param_store = Arc::new(StubParamStore);
    let kernel_executor = Arc::new(
        icn_core::supervisor::governance_executor::KernelGovernanceExecutor::new(param_store)
            .with_budget_store(budget_store),
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

    (decision_executor, exec_store)
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
) -> KernelEffect {
    KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: treasury_did.to_string(),
        recipient_did: recipient_did.to_string(),
        amount,
        currency: "HOURS".to_string(),
        memo: "Test spend".to_string(),
        budget_id: Some(budget_id.to_string()),
        expected_nonce: 0,
        decision_receipt_id: format!("receipt:{}", decision_hash),
        decision_hash: decision_hash.to_string(),
    })
}

// ============================================================================
// Test 1: Overspend Prevention
// ============================================================================

#[tokio::test]
async fn test_budget_overspend_rejected() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, exec_store) = make_test_executor(budget_store.clone());

    let treasury_did = "did:icn:treasury:ops";

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
        "did:icn:alice",
        600,
        "hash:spend-600",
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
        "did:icn:bob",
        500,
        "hash:spend-over",
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
        "did:icn:carol",
        400,
        "hash:spend-exact",
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

#[tokio::test]
async fn test_budget_spend_replay_idempotent() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, _exec_store) = make_test_executor(budget_store.clone());

    let treasury_did = "did:icn:treasury:ops";

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
        "did:icn:alice",
        300,
        "hash:spend-300",
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

#[tokio::test]
async fn test_budget_crash_recovery_saga_safe() {
    // This test verifies that if we crash between begin_spend and confirm_spend,
    // the budget stays with a pending_spend and can be retried.
    let budget_store = Arc::new(MemoryBudgetStore::new());

    // Manually create a budget with a pending spend (simulating crash state)
    let mut budget = BudgetRecord::new(
        "budget-crash".into(),
        "did:icn:treasury:crash".into(),
        "did:icn:treasury:crash".into(),
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
    let (executor, _exec_store) = make_test_executor(budget_store.clone());

    let retry_spend = vec![spend_from_budget_effect(
        "budget-crash",
        "did:icn:treasury:crash",
        "did:icn:alice",
        500,
        "hash:spend-crash",
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

#[tokio::test]
async fn test_create_budget_idempotent() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, _exec_store) = make_test_executor(budget_store.clone());

    let create = vec![create_budget_effect(
        "budget-idem",
        "did:icn:treasury",
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

#[tokio::test]
async fn test_spend_without_budget_passes_through() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, _exec_store) = make_test_executor(budget_store.clone());

    // Spend with no budget_id — should pass through to treasury executor directly
    let spend = vec![KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: "did:icn:treasury".to_string(),
        recipient_did: "did:icn:alice".to_string(),
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

#[tokio::test]
async fn test_budget_state_queryable() {
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, _exec_store) = make_test_executor(budget_store.clone());

    let treasury_did = "did:icn:treasury:query";

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
                "did:icn:alice",
                2000,
                "hash:spend-a",
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
