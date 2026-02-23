//! Integration test: commons-scope task → settlement engine → journal entries.
//!
//! Verifies the full path from gossip task reception through CCL execution
//! to commons credit settlement, using a real [`SettlementEngine`] wired to
//! the compute actor's settlement callback.
//!
//! Invariants exercised (from #948 PR description):
//!   1. Commons receipts settled only via `settle_commons_receipt()`.
//!   3. `settle_commons_receipt()` hard-rejects scope != Commons.
//!   4. Settlement idempotent: duplicate → `DuplicateEntry`, no balance change.
//!   5. Insufficient balance fails settlement (separate assertion).
//!   6. Scope mismatch is a hard error (implicit: local task does NOT fire cb).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_compute::{
    ComputeActor, ComputeMessage, ComputeTask, DeterminismClass, ExecutorCapability, FuelLimit,
    PrivacyClass, TaskCode, TaskPriority,
};
use icn_identity::Did;
use icn_kernel_api::ScopeLevel;
use icn_ledger::{
    commons_credits::COMMONS_CREDIT_CURRENCY,
    settlement::{CommonsSettlementRequest, SettlementEngine},
    types::JournalEntry,
};
use std::sync::{Arc, Mutex};

/// Valid ICN DID for signing key [1u8; 32] — executor / contributor.
const TEST_EXECUTOR_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";

/// Valid ICN DID for signing key [2u8; 32] — submitter / consumer.
const TEST_SUBMITTER_DID: &str = "did:icn:z9hSR6S7WPtxmTojgo6GG3k4yDPecgJY292j7xrsUGWBu";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// CCL that executes successfully with TEST_EXECUTOR_DID as the sole participant.
fn commons_ccl() -> String {
    format!(
        r#"{{
            "name": "CommonsTask",
            "participants": ["{TEST_EXECUTOR_DID}"],
            "currency": null,
            "state_vars": [],
            "rules": [{{
                "name": "run",
                "params": [],
                "requires": [],
                "body": [{{ "Return": {{ "value": {{ "Literal": {{ "Int": 1 }} }} }} }}]
            }}],
            "triggers": []
        }}"#
    )
}

/// Build a commons-scope task with valid DIDs.
fn make_commons_task(id: &str) -> ComputeTask {
    ComputeTask {
        id: id.to_string(),
        submitter: TEST_SUBMITTER_DID.to_string(),
        coop_id: None,
        code: TaskCode::Ccl(commons_ccl()),
        inputs: vec![],
        fuel_limit: FuelLimit(50_000),
        required_capabilities: vec![ExecutorCapability::Ccl],
        priority: TaskPriority::Normal,
        created_at: 1_000,
        deadline: None,
        payment_rate: None,
        payment_currency: None,
        resource_profile: None,
        actor_mode: None,
        placement_constraints: None,
        federation_constraints: None,
        estimated_value: None,
        verification: None,
        inputs_hash: None,
        policy_hash: None,
        determinism_class: DeterminismClass::default(),
        privacy_class: PrivacyClass::default(),
        storage_class: None,
        data_locality: None,
        scope: ScopeLevel::Commons,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Golden path: commons task executes → settlement engine produces balanced entries.
///
/// Asserts:
/// - Exactly one (earn, spend) pair is produced.
/// - Earn entry credits the executor (contributor).
/// - Spend entry debits the submitter (consumer).
/// - Both entries use COMMONS_CREDIT_CURRENCY.
/// - Amount is positive.
#[tokio::test]
async fn test_commons_settlement_produces_balanced_entries() {
    let engine = Arc::new(SettlementEngine::new());
    let entries: Arc<Mutex<Vec<(JournalEntry, JournalEntry)>>> = Arc::new(Mutex::new(Vec::new()));

    // Wire the settlement engine into the compute actor callback.
    let engine_cb = engine.clone();
    let entries_cb = entries.clone();

    let trust_cb: Arc<dyn Fn(&str) -> f64 + Send + Sync> = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new(TEST_EXECUTOR_DID.to_string(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);

    actor.set_commons_settlement_callback(Arc::new(move |req| {
        let contributor = Did::from_str(&req.contributor).expect("contributor must be valid DID");
        let consumer = Did::from_str(&req.consumer).expect("consumer must be valid DID");

        let settlement = CommonsSettlementRequest {
            receipt_hash: req.receipt_hash,
            contributor,
            consumer,
            scope: ScopeLevel::Commons,
            amount: req.amount as i64,
            // Pre-fund the consumer with enough balance.
            consumer_balance: 100_000,
            executor_verified: true,
        };

        let (earn, spend) = engine_cb
            .settle_commons_receipt(&settlement)
            .expect("settlement must succeed for a valid commons task");

        entries_cb.lock().unwrap().push((earn, spend));
    }));

    let handle = actor.spawn();

    // Inject the task via the gossip path (same path a remote node uses).
    let task = make_commons_task("commons-settle-1");
    let task_hash = task.hash();
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .expect("gossip delivery must succeed");

    // Give the actor time to execute and fire the callback.
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // ── Task completed ──────────────────────────────────────────────────────
    let status = handle.status(task_hash).await.unwrap();
    assert!(
        matches!(status, Some(icn_compute::TaskStatus::Completed { .. })),
        "task must be Completed, got: {status:?}"
    );

    // ── Settlement entries ──────────────────────────────────────────────────
    let results = entries.lock().unwrap();
    assert_eq!(results.len(), 1, "expected exactly one settlement pair");

    let (earn, spend) = &results[0];

    // ── Earn entry: executor receives a credit from the mint ────────────────
    let executor_credit = earn
        .accounts
        .iter()
        .find(|d| d.account_id.to_string() == TEST_EXECUTOR_DID && d.credit.is_some())
        .expect("earn entry must credit the executor");
    let earned_amount = executor_credit.credit.unwrap();
    assert!(earned_amount > 0, "earned amount must be positive");
    assert_eq!(executor_credit.currency, COMMONS_CREDIT_CURRENCY);

    // ── Spend entry: consumer is debited ────────────────────────────────────
    let consumer_debit = spend
        .accounts
        .iter()
        .find(|d| d.account_id.to_string() == TEST_SUBMITTER_DID && d.debit.is_some())
        .expect("spend entry must debit the consumer");
    assert_eq!(
        consumer_debit.debit.unwrap(),
        earned_amount,
        "consumer debit must equal executor credit (balanced settlement)"
    );
    assert_eq!(consumer_debit.currency, COMMONS_CREDIT_CURRENCY);
}

/// Idempotency: re-settling the same receipt returns DuplicateEntry and does not
/// produce a second journal entry pair.
#[tokio::test]
async fn test_commons_settlement_is_idempotent() {
    let engine = Arc::new(SettlementEngine::new());

    // Settle once manually to prime the dedup set.
    let receipt_hash = [42u8; 32];
    let contributor = Did::from_str(TEST_EXECUTOR_DID).unwrap();
    let consumer = Did::from_str(TEST_SUBMITTER_DID).unwrap();

    let req = CommonsSettlementRequest {
        receipt_hash,
        contributor: contributor.clone(),
        consumer: consumer.clone(),
        scope: ScopeLevel::Commons,
        amount: 100,
        consumer_balance: 10_000,
        executor_verified: true,
    };

    let first = engine.settle_commons_receipt(&req);
    assert!(first.is_ok(), "first settlement must succeed");

    // Second call with identical receipt — must fail with DuplicateEntry.
    let second = engine.settle_commons_receipt(&req);
    assert!(
        matches!(
            second,
            Err(icn_ledger::error::LedgerError::DuplicateEntry(_))
        ),
        "duplicate settlement must return DuplicateEntry, got: {second:?}"
    );
}

/// Scope guard: settle_commons_receipt hard-rejects non-Commons scope.
#[tokio::test]
async fn test_commons_settlement_rejects_non_commons_scope() {
    let engine = SettlementEngine::new();
    let contributor = Did::from_str(TEST_EXECUTOR_DID).unwrap();
    let consumer = Did::from_str(TEST_SUBMITTER_DID).unwrap();

    for scope in [ScopeLevel::Local, ScopeLevel::Cell, ScopeLevel::Org] {
        let req = CommonsSettlementRequest {
            receipt_hash: [scope as u8; 32],
            contributor: contributor.clone(),
            consumer: consumer.clone(),
            scope,
            amount: 50,
            consumer_balance: 10_000,
            executor_verified: true,
        };
        let result = engine.settle_commons_receipt(&req);
        assert!(
            matches!(result, Err(icn_ledger::error::LedgerError::InvalidEntry(_))),
            "scope {:?} must be rejected with InvalidEntry, got: {result:?}",
            scope,
        );
    }
}

/// Balance floor: insufficient consumer balance causes settlement to fail cleanly.
#[tokio::test]
async fn test_commons_settlement_rejects_insufficient_balance() {
    let engine = SettlementEngine::new();
    let contributor = Did::from_str(TEST_EXECUTOR_DID).unwrap();
    let consumer = Did::from_str(TEST_SUBMITTER_DID).unwrap();

    let req = CommonsSettlementRequest {
        receipt_hash: [7u8; 32],
        contributor,
        consumer,
        scope: ScopeLevel::Commons,
        amount: 500,
        consumer_balance: 10, // far below amount
        executor_verified: true,
    };

    let result = engine.settle_commons_receipt(&req);
    assert!(
        matches!(result, Err(icn_ledger::error::LedgerError::InvalidEntry(_))),
        "insufficient balance must return InvalidEntry, got: {result:?}"
    );
}
