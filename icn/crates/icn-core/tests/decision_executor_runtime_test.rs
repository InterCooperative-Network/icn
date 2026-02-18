#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for the Decision Executor runtime loop.
//!
//! These tests prove that:
//! 1. A finalized decision auto-executes via the callback (no manual trigger)
//! 2. Restart recovery picks up in-flight decisions
//! 3. Double-execution is prevented (idempotency)
//! 4. Backpressure doesn't deadlock

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use icn_core::services::LedgerServiceImpl;
use icn_core::supervisor::execution_store::SledExecutionStore;
use icn_governance::{ProposalPayload, TreasuryProposalOperation};
use icn_governance_actor::translate_payload_to_effects;
use icn_kernel_api::budget::{BudgetRecord, BudgetStore};
use icn_kernel_api::effects::{KernelEffect, TreasuryEffect};
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};
use icn_kernel_api::AllowAllOracle;
use icn_ledger::types::ContentHash;
use icn_ledger::Ledger;
use icn_store::SledStore;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// In-memory stores for testing
// ---------------------------------------------------------------------------

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
        Ok(self
            .records
            .read()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .get(decision_hash)
            .cloned())
    }

    fn put(&self, record: &ExecutionRecord) -> Result<()> {
        self.records
            .write()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .insert(record.decision_hash.clone(), record.clone());
        Ok(())
    }

    fn list_by_status(&self, status: ExecutionStatus) -> Result<Vec<ExecutionRecord>> {
        Ok(self
            .records
            .read()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .values()
            .filter(|r| r.status == status)
            .cloned()
            .collect())
    }

    fn count_by_status(&self) -> Result<HashMap<ExecutionStatus, usize>> {
        let mut counts = HashMap::new();
        for record in self
            .records
            .read()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .values()
        {
            *counts.entry(record.status).or_insert(0) += 1;
        }
        Ok(counts)
    }
}

struct MemoryBudgetStore {
    budgets: RwLock<HashMap<String, BudgetRecord>>,
}

impl MemoryBudgetStore {
    fn new() -> Self {
        Self {
            budgets: RwLock::new(HashMap::new()),
        }
    }
}

impl BudgetStore for MemoryBudgetStore {
    fn get(&self, budget_id: &str) -> Result<Option<BudgetRecord>> {
        Ok(self
            .budgets
            .read()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .get(budget_id)
            .cloned())
    }

    fn put(&self, record: &BudgetRecord) -> Result<()> {
        self.budgets
            .write()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .insert(record.budget_id.clone(), record.clone());
        Ok(())
    }

    fn list_by_scope(&self, scope_id: &str) -> Result<Vec<BudgetRecord>> {
        Ok(self
            .budgets
            .read()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .values()
            .filter(|b| b.scope_id == scope_id)
            .cloned()
            .collect())
    }
}

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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_executor_with_budget(
    budget_store: Arc<dyn BudgetStore>,
) -> (
    Arc<icn_core::supervisor::decision_executor::DecisionExecutor>,
    Arc<MemoryExecutionStore>,
) {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    let kernel_executor =
        KernelGovernanceExecutor::new(Arc::new(StubParamStore)).with_budget_store(budget_store);

    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_store = Arc::new(MemoryExecutionStore::new());
    let executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));
    (executor, exec_store)
}

fn make_executor() -> (
    Arc<icn_core::supervisor::decision_executor::DecisionExecutor>,
    Arc<MemoryExecutionStore>,
) {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore));

    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_store = Arc::new(MemoryExecutionStore::new());
    let executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));
    (executor, exec_store)
}

fn spend_effect(decision_hash: &str) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: "did:icn:treasury".to_string(),
        recipient_did: "did:icn:recipient".to_string(),
        amount: 100,
        currency: "HOURS".to_string(),
        memo: "Test spend".to_string(),
        budget_id: None,
        decision_receipt_id: "receipt-1".to_string(),
        decision_hash: decision_hash.to_string(),
    })]
}

fn budget_spend_effect(decision_hash: &str, budget_id: &str, amount: i64) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: "did:icn:treasury".to_string(),
        recipient_did: "did:icn:recipient".to_string(),
        amount,
        currency: "HOURS".to_string(),
        memo: "Budget spend".to_string(),
        budget_id: Some(budget_id.to_string()),
        decision_receipt_id: "receipt-budget".to_string(),
        decision_hash: decision_hash.to_string(),
    })]
}

fn create_budget_effect(decision_hash: &str, budget_id: &str, limit: i64) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
        treasury_did: "did:icn:treasury".to_string(),
        budget_id: budget_id.to_string(),
        total_amount: limit,
        currency: "HOURS".to_string(),
        name: "Test Budget".to_string(),
        validity_start: 0,
        validity_end: u64::MAX,
        decision_receipt_id: "receipt-create".to_string(),
        decision_hash: decision_hash.to_string(),
    })]
}

fn content_hash_from_hex(hash_hex: &str) -> Result<ContentHash> {
    let bytes = hex::decode(hash_hex).map_err(|e| anyhow::anyhow!("invalid hex: {e}"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32-byte content hash"))?;
    Ok(ContentHash::from_bytes(bytes))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test 1: A finalized decision auto-executes via the callback.
///
/// This simulates what happens in the real runtime: the governance app
/// emits effects, the callback fires, and the DecisionExecutor processes
/// them asynchronously.
#[tokio::test]
async fn test_callback_auto_executes_decision() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();
    let callback = create_decision_executor_callback(executor);

    // Simulate governance app emitting effects
    let effects = spend_effect("hash-auto-1");
    callback(effects, "receipt-auto-1".to_string());

    // Wait for the spawned task to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Assert execution record is Confirmed
    let record = exec_store.get("hash-auto-1").unwrap().unwrap();
    assert_eq!(
        record.status,
        ExecutionStatus::Confirmed,
        "Decision should be confirmed after callback execution"
    );
    assert_eq!(record.decision_hash, "hash-auto-1");
}

/// Test 2: Calling the callback twice with the same decision_hash
/// does not double-execute.
#[tokio::test]
async fn test_callback_idempotent_no_double_execute() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();
    let callback = create_decision_executor_callback(executor);

    // First execution
    let effects = spend_effect("hash-idem-1");
    callback(effects.clone(), "receipt-idem-1".to_string());
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let record1 = exec_store.get("hash-idem-1").unwrap().unwrap();
    assert_eq!(record1.status, ExecutionStatus::Confirmed);

    // Second execution (replay) — should be skipped
    callback(effects, "receipt-idem-1".to_string());
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Still confirmed, not re-executed
    let record2 = exec_store.get("hash-idem-1").unwrap().unwrap();
    assert_eq!(record2.status, ExecutionStatus::Confirmed);
}

/// Test 3: Startup recovery picks up an Executing record and completes it.
#[tokio::test]
async fn test_startup_recovery_completes_executing_decision() {
    let (executor, exec_store) = make_executor();

    // Pre-seed an Executing record (simulates crash mid-execution)
    let effects = spend_effect("hash-crash-1");
    let mut crashed_record =
        ExecutionRecord::new_pending("hash-crash-1", "proposal-crash", "receipt-crash", effects);
    crashed_record.mark_executing();
    exec_store.put(&crashed_record).unwrap();

    // Verify it's in Executing state
    let before = exec_store.get("hash-crash-1").unwrap().unwrap();
    assert_eq!(before.status, ExecutionStatus::Executing);

    // Run recovery
    let report = executor.recover_in_flight().await.unwrap();

    assert_eq!(report.recovered_confirmed, 1, "Should recover 1 decision");
    assert_eq!(report.recovered_failed, 0);

    // Verify it's now Confirmed
    let after = exec_store.get("hash-crash-1").unwrap().unwrap();
    assert_eq!(
        after.status,
        ExecutionStatus::Confirmed,
        "Recovered decision should be confirmed"
    );
}

/// Test 4: Startup recovery skips records that have no stored effects
/// (pre-upgrade records from before effects storage was added).
#[tokio::test]
async fn test_startup_recovery_skips_records_without_effects() {
    let (executor, exec_store) = make_executor();

    // Pre-seed a record with empty effects (pre-upgrade)
    let mut old_record =
        ExecutionRecord::new_pending("hash-old-1", "proposal-old", "receipt-old", vec![]);
    old_record.mark_executing();
    exec_store.put(&old_record).unwrap();

    // Run recovery
    let report = executor.recover_in_flight().await.unwrap();

    assert_eq!(
        report.skipped_no_effects, 1,
        "Should skip 1 record with no effects"
    );
    assert_eq!(report.recovered_confirmed, 0);

    // Record should still be in Executing state (not modified)
    let after = exec_store.get("hash-old-1").unwrap().unwrap();
    assert_eq!(after.status, ExecutionStatus::Executing);
}

/// Test 5: Startup recovery re-attempts Failed records.
#[tokio::test]
async fn test_startup_recovery_retries_failed_decision() {
    let (executor, exec_store) = make_executor();

    // Pre-seed a Failed record with stored effects (retry 1 of 3)
    let effects = vec![KernelEffect::NoOp {
        reason: "test".to_string(),
    }];
    let mut failed_record =
        ExecutionRecord::new_pending("hash-fail-retry", "proposal-fail", "receipt-fail", effects);
    failed_record.mark_executing();
    failed_record.mark_failed("Transient error");
    assert_eq!(failed_record.retries, 1);
    exec_store.put(&failed_record).unwrap();

    // Run recovery
    let report = executor.recover_in_flight().await.unwrap();

    assert_eq!(
        report.recovered_confirmed, 1,
        "Should recover the failed decision on retry"
    );

    // Verify it's now Confirmed
    let after = exec_store.get("hash-fail-retry").unwrap().unwrap();
    assert_eq!(after.status, ExecutionStatus::Confirmed);
}

/// Test 6: Retry counter accumulates across recovery attempts
/// and enforces MAX_RETRIES → PermanentlyFailed.
///
/// This uses a budget spend against a non-existent budget so the
/// effect fails deterministically on every attempt.
#[tokio::test]
async fn test_retry_accumulation_and_max_retries() {
    // Budget store with NO budgets — spend effects will always fail
    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, exec_store) = make_executor_with_budget(budget_store);

    // Pre-seed a Failed record with 1 prior retry
    let effects = budget_spend_effect("hash-retry-acc", "nonexistent-budget", 100);
    let mut record =
        ExecutionRecord::new_pending("hash-retry-acc", "proposal-retry", "receipt-retry", effects);
    record.mark_executing();
    record.mark_failed("First failure");
    assert_eq!(record.retries, 1);
    exec_store.put(&record).unwrap();

    // Recovery attempt 2: should fail again, retries → 2
    let _report = executor.recover_in_flight().await.unwrap();
    let after = exec_store.get("hash-retry-acc").unwrap().unwrap();
    assert_eq!(after.retries, 2, "Retry counter should accumulate to 2");
    assert_eq!(
        after.status,
        ExecutionStatus::Failed,
        "Should still be Failed (under MAX_RETRIES=3)"
    );

    // Recovery attempt 3: should fail, retries → 3 → hits MAX_RETRIES
    let _report = executor.recover_in_flight().await.unwrap();
    let after = exec_store.get("hash-retry-acc").unwrap().unwrap();
    // At retries == 3 (== MAX_RETRIES), the next recovery call should mark PermanentlyFailed
    assert_eq!(after.retries, 3);

    // Recovery attempt 4: should hit MAX_RETRIES gate → PermanentlyFailed
    let _report = executor.recover_in_flight().await.unwrap();
    let after = exec_store.get("hash-retry-acc").unwrap().unwrap();
    assert_eq!(
        after.status,
        ExecutionStatus::PermanentlyFailed,
        "Should be PermanentlyFailed after exhausting MAX_RETRIES"
    );
}

/// Test 7 (was 6): Full end-to-end: create budget → spend against it → auto-execute.
/// This proves the complete pipeline works through the callback.
#[tokio::test]
async fn test_callback_e2e_budget_create_and_spend() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let budget_store = Arc::new(MemoryBudgetStore::new());
    let (executor, exec_store) = make_executor_with_budget(budget_store.clone());
    let callback = create_decision_executor_callback(executor);

    // Step 1: Create a budget via callback
    let create_effects = create_budget_effect("hash-create-b1", "budget-1", 1000);
    callback(create_effects, "receipt-create-b1".to_string());
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify budget was created
    let budget = budget_store.get("budget-1").unwrap().unwrap();
    assert_eq!(budget.total_limit, 1000);
    assert_eq!(budget.spent_total, 0);

    // Verify execution record
    let create_record = exec_store.get("hash-create-b1").unwrap().unwrap();
    assert_eq!(create_record.status, ExecutionStatus::Confirmed);

    // Step 2: Spend against the budget via callback
    let spend_effects = budget_spend_effect("hash-spend-b1", "budget-1", 300);
    callback(spend_effects, "receipt-spend-b1".to_string());
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify budget was debited
    let budget_after = budget_store.get("budget-1").unwrap().unwrap();
    assert_eq!(budget_after.spent_total, 300);
    assert_eq!(budget_after.remaining(), 700);

    // Verify spend execution record
    let spend_record = exec_store.get("hash-spend-b1").unwrap().unwrap();
    assert_eq!(spend_record.status, ExecutionStatus::Confirmed);
}

/// Test 8: Backpressure does not deadlock under concurrent load.
#[tokio::test]
async fn test_backpressure_no_deadlock() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();
    let callback = create_decision_executor_callback(executor);

    // Fire 32 decisions concurrently (exceeds MAX_CONCURRENT_EXECUTIONS = 16).
    // NoOp effects don't carry a decision_hash, so the callback uses
    // receipt_id as the decision_hash fallback.
    for i in 0..32 {
        let effects = vec![KernelEffect::NoOp {
            reason: format!("backpressure-test-{}", i),
        }];
        callback(effects, format!("receipt-bp-{}", i));
    }

    // Wait for all to complete (with backpressure, some will queue)
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Count confirmed records
    let confirmed = exec_store
        .list_by_status(ExecutionStatus::Confirmed)
        .unwrap();
    assert_eq!(
        confirmed.len(),
        32,
        "All 32 decisions should complete despite backpressure"
    );
}

/// Test 9: Recovery after restart does not re-execute already confirmed decisions.
#[tokio::test]
async fn test_recovery_does_not_re_execute_confirmed() {
    let (executor, exec_store) = make_executor();

    // Pre-seed a Confirmed record
    let effects = spend_effect("hash-confirmed-already");
    let mut confirmed_record = ExecutionRecord::new_pending(
        "hash-confirmed-already",
        "proposal-done",
        "receipt-done",
        effects,
    );
    confirmed_record.mark_executing();
    confirmed_record.mark_confirmed(vec!["entry-1".into()], vec!["hash-1".into()]);
    exec_store.put(&confirmed_record).unwrap();

    // Run recovery — should find nothing to recover
    let report = executor.recover_in_flight().await.unwrap();
    assert_eq!(report.total(), 0, "No records should need recovery");

    // Verify record is untouched
    let after = exec_store.get("hash-confirmed-already").unwrap().unwrap();
    assert_eq!(after.status, ExecutionStatus::Confirmed);
    assert_eq!(after.ledger_entry_ids, vec!["entry-1"]);
}

/// Test 10: Treasury execution appends a real ledger entry with governance provenance,
/// and the entry + execution record survive restart.
#[tokio::test(flavor = "multi_thread")]
async fn test_treasury_entry_persisted_with_provenance() {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    let tmp = TempDir::new().unwrap();
    let ledger_store_path = tmp.path().join("ledger");
    let exec_store_path = tmp.path().join("execution");
    std::fs::create_dir_all(&ledger_store_path).unwrap();
    std::fs::create_dir_all(&exec_store_path).unwrap();

    let decision_hash = "hash-ledger-provenance-1";
    let decision_receipt_id = "receipt-ledger-provenance-1";
    let treasury_did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";

    let entry_hash = {
        let ledger_store = Arc::new(SledStore::open(&ledger_store_path).unwrap());
        let ledger = Ledger::new(ledger_store).unwrap();
        let ledger = Arc::new(tokio::sync::RwLock::new(ledger));

        let ledger_service = Arc::new(LedgerServiceImpl::new(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.parse().unwrap(),
        ));
        let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_ledger_service(ledger_service);

        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
        let exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
        let exec_store = Arc::new(SledExecutionStore::new(exec_backend));
        let executor = DecisionExecutor::new(dispatcher, exec_store.clone());

        let effects = vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: treasury_did.to_string(),
            recipient_did: treasury_did.to_string(),
            amount: 100,
            currency: "HOURS".to_string(),
            memo: "Provenance persistence test".to_string(),
            budget_id: None,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })];

        let results = executor
            .execute(
                effects,
                decision_receipt_id,
                decision_hash,
                "proposal-ledger-provenance-1",
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        let record = exec_store.get(decision_hash).unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
        assert_eq!(record.ledger_entry_ids.len(), 1);
        let entry_hash = record.ledger_entry_ids[0].clone();

        let hash = content_hash_from_hex(&entry_hash);
        assert!(hash.is_ok(), "ledger entry id must parse as ContentHash");
        let hash = hash.unwrap();
        let ledger_guard = ledger.read().await;
        let entry = ledger_guard.get_entry(&hash).unwrap().unwrap();
        assert_eq!(
            entry.decision_receipt_id.as_deref(),
            Some(decision_receipt_id)
        );
        assert_eq!(entry.decision_hash.as_deref(), Some(decision_hash));
        drop(ledger_guard);

        entry_hash
    };

    let reopened_ledger_store = Arc::new(SledStore::open(&ledger_store_path).unwrap());
    let reopened_ledger = Ledger::new(reopened_ledger_store).unwrap();
    let reopened_hash = content_hash_from_hex(&entry_hash).unwrap();
    let reopened_entry = reopened_ledger.get_entry(&reopened_hash).unwrap().unwrap();
    assert_eq!(
        reopened_entry.decision_receipt_id.as_deref(),
        Some(decision_receipt_id)
    );
    assert_eq!(reopened_entry.decision_hash.as_deref(), Some(decision_hash));

    let reopened_exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
    let reopened_exec_store = SledExecutionStore::new(reopened_exec_backend);
    let reopened_record = reopened_exec_store.get(decision_hash).unwrap().unwrap();
    assert_eq!(reopened_record.ledger_entry_ids, vec![entry_hash]);
}

/// Test 11: Treasury Spend payload is explicitly unsupported at translation
/// boundary until payload carries treasury identity + currency.
#[test]
fn test_treasury_spend_payload_translation_is_explicitly_unsupported() {
    use icn_identity::Did;

    let decision_receipt_id = "gov:test-domain:test-proposal:receipt";
    let decision_hash = "test-decision-hash";
    let recipient: Did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
        .parse()
        .unwrap();

    let payload = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Spend {
            amount: 100,
            recipient,
            memo: "PR-2 treasury spend".to_string(),
            nonce: 0,
        },
    };

    let effects = translate_payload_to_effects(&payload, decision_receipt_id, decision_hash);
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        KernelEffect::NoOp { reason } => {
            assert!(
                reason.contains(
                    "Treasury Spend translation unsupported: missing treasury_did and currency in payload"
                ),
                "NoOp reason must explain unsupported Spend translation boundary"
            );
        }
        other => panic!("expected NoOp for unsupported Spend payload, got {other:?}"),
    }
}
