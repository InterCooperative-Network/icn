#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for the Decision Executor runtime loop.
//!
//! These tests prove that:
//! 1. A finalized decision auto-executes via the callback (no manual trigger)
//! 2. Restart recovery picks up in-flight decisions
//! 3. Double-execution is prevented (idempotency)
//! 4. Backpressure doesn't deadlock

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use icn_core::services::LedgerServiceImpl;
use icn_core::supervisor::execution_store::SledExecutionStore;
use icn_governance::{ProposalPayload, TreasuryProposalOperation};
use icn_governance_actor::translate_payload_to_effects;
use icn_identity::Did;
use icn_kernel_api::budget::{BudgetRecord, BudgetStore};
use icn_kernel_api::effects::{FederationEffect, KernelEffect, TreasuryEffect};
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};
use icn_kernel_api::protocol_params::StubParamStore;
use icn_kernel_api::services::LedgerEvent;
use icn_kernel_api::{
    AllowAllOracle, Did as LedgerDid, LedgerService, PolicyOracle, TreasuryEntryRequest,
    TreasuryEntryResult,
};
use icn_ledger::types::{ContentHash, ProvenanceRef};
use icn_ledger::Ledger;
use icn_store::SledStore;
use tempfile::TempDir;
use tokio::sync::RwLock as TokioRwLock;

/// Valid multibase DIDs for ledger-backed treasury tests.
const RT_TEST_TREASURY: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
const RT_TEST_RECIPIENT: &str = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse";

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

    fn delete(&self, decision_hash: &str) -> Result<()> {
        self.records
            .write()
            .map_err(|e| anyhow::anyhow!("lock: {e}"))?
            .remove(decision_hash);
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

/// DecisionExecutor with treasury backed by a temp Sled ledger (`spend_effect` uses [`RT_TEST_TREASURY`]).
fn make_executor_with_ledger(
    ledger_data_dir: &Path,
) -> (
    Arc<icn_core::supervisor::decision_executor::DecisionExecutor>,
    Arc<MemoryExecutionStore>,
) {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    let ledger_path = ledger_data_dir.join("ledger");
    std::fs::create_dir_all(&ledger_path).unwrap();
    let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
    let ledger = Ledger::new(ledger_store).unwrap();
    let ledger = Arc::new(TokioRwLock::new(ledger));
    let treasury_did: Did = RT_TEST_TREASURY.parse().unwrap();
    let ledger_service = Arc::new(LedgerServiceImpl::new(
        ledger,
        Arc::new(AllowAllOracle::wildcard()),
        treasury_did,
    ));
    let kernel_executor =
        KernelGovernanceExecutor::new(Arc::new(StubParamStore)).with_ledger_service(ledger_service);

    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_store = Arc::new(MemoryExecutionStore::new());
    let executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));
    (executor, exec_store)
}

fn make_executor_with_budget_and_ledger(
    budget_store: Arc<dyn BudgetStore>,
    ledger_data_dir: &Path,
) -> (
    Arc<icn_core::supervisor::decision_executor::DecisionExecutor>,
    Arc<MemoryExecutionStore>,
) {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    let ledger_path = ledger_data_dir.join("ledger");
    std::fs::create_dir_all(&ledger_path).unwrap();
    let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
    let ledger = Ledger::new(ledger_store).unwrap();
    let ledger = Arc::new(TokioRwLock::new(ledger));
    let treasury_did: Did = RT_TEST_TREASURY.parse().unwrap();
    let ledger_service = Arc::new(LedgerServiceImpl::new(
        ledger,
        Arc::new(AllowAllOracle::wildcard()),
        treasury_did,
    ));
    let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
        .with_budget_store(budget_store)
        .with_ledger_service(ledger_service);

    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_store = Arc::new(MemoryExecutionStore::new());
    let executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));
    (executor, exec_store)
}

fn spend_effect(decision_hash: &str) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: RT_TEST_TREASURY.to_string(),
        recipient_did: RT_TEST_RECIPIENT.to_string(),
        amount: 100,
        currency: "HOURS".to_string(),
        memo: "Test spend".to_string(),
        budget_id: None,
        expected_nonce: 0,
        decision_receipt_id: "receipt-1".to_string(),
        decision_hash: decision_hash.to_string(),
    })]
}

fn budget_spend_effect(decision_hash: &str, budget_id: &str, amount: i64) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::Spend {
        treasury_did: RT_TEST_TREASURY.to_string(),
        recipient_did: RT_TEST_RECIPIENT.to_string(),
        amount,
        currency: "HOURS".to_string(),
        memo: "Budget spend".to_string(),
        budget_id: Some(budget_id.to_string()),
        expected_nonce: 0,
        decision_receipt_id: "receipt-budget".to_string(),
        decision_hash: decision_hash.to_string(),
    })]
}

fn create_budget_effect(decision_hash: &str, budget_id: &str, limit: i64) -> Vec<KernelEffect> {
    vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
        treasury_did: RT_TEST_TREASURY.to_string(),
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

/// A federation-only effect batch carrying a governance `decision_hash`
/// (since #2094). Used to exercise the executor idempotency-key path for
/// federation decisions (#2095).
fn terminate_clearing_effect(decision_hash: &str, reason: &str) -> Vec<KernelEffect> {
    vec![KernelEffect::Federation(
        FederationEffect::TerminateClearing {
            initiating_coop_did: "coop-a".to_string(),
            partner_coop_did: "coop-b".to_string(),
            reason: reason.to_string(),
            decision_hash: decision_hash.to_string(),
        },
    )]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Issue #1987: a terminal execution record persists the per-effect results,
/// so a post-crash dispatch-evidence backfill can re-derive evidence from the
/// durable `(effects, results)` pair without re-executing the effects.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_execute_persists_results_on_terminal_record() {
    let tmp = TempDir::new().unwrap();
    let (executor, exec_store) = make_executor_with_ledger(tmp.path());

    let effects = spend_effect("hash-results-1");
    let returned = executor
        .execute(
            effects,
            "receipt-results-1",
            "hash-results-1",
            "proposal-results-1",
        )
        .await
        .unwrap();
    assert!(
        !returned.is_empty(),
        "spend should produce a per-effect result"
    );

    let record = exec_store.get("hash-results-1").unwrap().unwrap();
    assert_eq!(record.status, ExecutionStatus::Confirmed);
    assert_eq!(
        record.results, returned,
        "terminal record must persist the per-effect results for evidence backfill"
    );
}

/// Test 1: A finalized decision auto-executes via the callback.
///
/// This simulates what happens in the real runtime: the governance app
/// emits effects, the callback fires, and the DecisionExecutor processes
/// them asynchronously.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_callback_auto_executes_decision() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let tmp = TempDir::new().unwrap();
    let (executor, exec_store) = make_executor_with_ledger(tmp.path());
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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_callback_idempotent_no_double_execute() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let tmp = TempDir::new().unwrap();
    let (executor, exec_store) = make_executor_with_ledger(tmp.path());
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

/// Issue #2095: federation-only decisions are keyed by their propagated
/// `decision_hash`, not by `decision_receipt_id`.
///
/// Two federation decisions that share a `decision_receipt_id` but carry
/// distinct `decision_hash` values must BOTH be recorded independently. Before
/// the fix, `extract_decision_hash` ignored federation effects, so both
/// collapsed onto the shared receipt id and the second deduped against the
/// first — exactly the replay hazard #2093/#2094 set out to remove.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_same_receipt_different_hash_not_deduped() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();
    let callback = create_decision_executor_callback(executor);

    // Same receipt id, two distinct federation decision hashes.
    callback(
        terminate_clearing_effect("sha256:fed-decision-a", "first"),
        "receipt-shared".to_string(),
    );
    callback(
        terminate_clearing_effect("sha256:fed-decision-b", "second"),
        "receipt-shared".to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // Each decision is recorded under its own decision_hash key, proving the
    // idempotency key reflects the federation decision_hash.
    assert!(
        exec_store.get("sha256:fed-decision-a").unwrap().is_some(),
        "first federation decision must be recorded under its decision_hash"
    );
    assert!(
        exec_store.get("sha256:fed-decision-b").unwrap().is_some(),
        "second federation decision must be recorded under its decision_hash"
    );
    // The shared receipt id is NOT the idempotency key when a non-empty
    // federation decision_hash is present (it would have collapsed the two).
    assert!(
        exec_store.get("receipt-shared").unwrap().is_none(),
        "receipt id must not key the record when a federation decision_hash is present"
    );
}

/// Issue #2095: an empty/legacy federation `decision_hash` preserves prior
/// behavior — the executor falls back to `decision_receipt_id` as the
/// idempotency key (no regression for pre-#2094 serialized effects).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_empty_hash_falls_back_to_receipt() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();
    let callback = create_decision_executor_callback(executor);

    callback(
        terminate_clearing_effect("", "legacy"),
        "receipt-legacy".to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    assert!(
        exec_store.get("receipt-legacy").unwrap().is_some(),
        "empty federation decision_hash must fall back to the receipt id key"
    );
}

/// Issue #2095 / Codex P1 (#2096): replay safety across the #2094->#2096 upgrade
/// window.
///
/// A node that processed a federation effect *before* this change keyed its
/// terminal `ExecutionRecord` under `decision_receipt_id` (the old extractor
/// returned `None`). After the change, a replay of that same accepted event
/// extracts the propagated federation `decision_hash`; without a compatibility
/// check the executor would probe `store.get(<decision_hash>)`, miss the legacy
/// receipt-keyed record, and re-run the terminate/revoke/settle operation. The
/// executor must detect the legacy terminal record and dedupe instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_replay_dedupes_against_legacy_receipt_keyed_record() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();

    // Simulate a pre-#2096 terminal record keyed under the *receipt id* (because
    // the old extractor ignored federation effects and the callback fell back to
    // the receipt id as the idempotency key).
    let receipt_id = "receipt-upgrade-legacy";
    let mut legacy = ExecutionRecord::new_pending(
        receipt_id,
        "proposal-legacy",
        receipt_id,
        terminate_clearing_effect("sha256:fed-legacy-hash", "legacy"),
    );
    legacy.status = ExecutionStatus::Confirmed;
    exec_store.put(&legacy).unwrap();

    let callback = create_decision_executor_callback(executor);

    // Replay the same accepted event, now carrying the propagated federation hash.
    callback(
        terminate_clearing_effect("sha256:fed-legacy-hash", "replay"),
        receipt_id.to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // No second record is created under the new decision_hash key (deduped
    // against the legacy receipt-keyed record).
    assert!(
        exec_store.get("sha256:fed-legacy-hash").unwrap().is_none(),
        "replay must not create a second record under the federation decision_hash"
    );
    // The legacy receipt-keyed record is untouched and still terminal.
    let still = exec_store.get(receipt_id).unwrap().unwrap();
    assert_eq!(
        still.status,
        ExecutionStatus::Confirmed,
        "legacy receipt-keyed record must be preserved (not re-executed)"
    );
}

/// Issue #2095 / Codex P1 follow-up (#2096): honor *in-flight / retryable* legacy
/// receipt-keyed records, not just terminal ones.
///
/// A pre-#2096 federation record keyed under `decision_receipt_id` that is still
/// non-terminal (e.g. a retryable `Failed` from before max-retry promotion) must
/// continue under its own key on replay. Otherwise the executor would create a
/// fresh record under the newly-extracted `decision_hash` with retries reset to
/// zero, orphaning the legacy record and re-dispatching the operation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_replay_honors_legacy_in_flight_receipt_record() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();

    // Pre-#2096 NON-terminal record keyed under the receipt id: a retryable
    // failure carried over the upgrade (retries already accumulated).
    let receipt_id = "receipt-inflight-legacy";
    let mut legacy = ExecutionRecord::new_pending(
        receipt_id,
        "proposal-legacy",
        receipt_id,
        terminate_clearing_effect("sha256:fed-inflight-hash", "legacy"),
    );
    legacy.status = ExecutionStatus::Failed;
    legacy.retries = 2;
    exec_store.put(&legacy).unwrap();

    let callback = create_decision_executor_callback(executor);
    callback(
        terminate_clearing_effect("sha256:fed-inflight-hash", "replay"),
        receipt_id.to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // No fresh record under the new decision_hash key — the decision continues
    // under its legacy receipt key.
    assert!(
        exec_store
            .get("sha256:fed-inflight-hash")
            .unwrap()
            .is_none(),
        "replay must not create a second record under the federation decision_hash"
    );
    // The legacy record continues under its own key with retry history intact
    // (not reset to zero).
    let cont = exec_store.get(receipt_id).unwrap().unwrap();
    assert!(
        cont.retries >= 2,
        "legacy retry count must be preserved, not reset (was {})",
        cont.retries
    );
}

/// Issue #2095 / Codex P1 (#2096): a legacy receipt-keyed record must NOT collapse
/// a *different* decision that merely shares the same `decision_receipt_id`.
///
/// Multiple distinct federation decisions can share a receipt id but carry
/// distinct `decision_hash` values (the #2095 same-receipt/different-hash case).
/// After an upgrade, a pre-#2096 record sits under the receipt id; a later,
/// distinct decision with the same receipt but a different hash must still
/// execute under its own hash key — the legacy key is adopted only for the same
/// decision (matched by the stored record's effects hash), never an unrelated one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_legacy_record_does_not_collapse_distinct_same_receipt_decision() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();

    // Legacy record for decision A (hash fed-a), keyed under the receipt id.
    let receipt_id = "receipt-shared-upgrade";
    let mut legacy = ExecutionRecord::new_pending(
        receipt_id,
        "proposal-a",
        receipt_id,
        terminate_clearing_effect("sha256:fed-decision-a", "decision-a"),
    );
    legacy.status = ExecutionStatus::Confirmed;
    exec_store.put(&legacy).unwrap();

    let callback = create_decision_executor_callback(executor);

    // A DIFFERENT decision B (hash fed-b) shares the same receipt id.
    callback(
        terminate_clearing_effect("sha256:fed-decision-b", "decision-b"),
        receipt_id.to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // Decision B executes under its own decision_hash key — it is not forced onto
    // the legacy receipt key and deduped against the unrelated decision A.
    assert!(
        exec_store.get("sha256:fed-decision-b").unwrap().is_some(),
        "a distinct same-receipt decision must execute under its own hash key"
    );
    // Decision A's legacy record is untouched.
    let a = exec_store.get(receipt_id).unwrap().unwrap();
    assert_eq!(a.status, ExecutionStatus::Confirmed);
}

/// Issue #2095 / Codex P2 (#2096): when BOTH a canonical hash-keyed record and a
/// stale legacy receipt-keyed record exist for the same decision, the canonical
/// terminal record wins.
///
/// This state is only reachable if a node ran an intermediate hash-keying build
/// (which created the canonical row) before the legacy-key compatibility fix
/// landed, leaving a stale non-terminal receipt-keyed row alongside a terminal
/// hash-keyed row. A replay must dedupe on the terminal canonical record, not
/// re-dispatch under the stale legacy key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_canonical_terminal_record_wins_over_legacy() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let (executor, exec_store) = make_executor();

    let receipt_id = "receipt-both-rows";
    let decision_hash = "sha256:fed-both-rows";

    // Canonical hash-keyed record for the decision: terminal (Confirmed).
    let mut canonical = ExecutionRecord::new_pending(
        decision_hash,
        "proposal-canonical",
        receipt_id,
        terminate_clearing_effect(decision_hash, "canonical"),
    );
    canonical.status = ExecutionStatus::Confirmed;
    exec_store.put(&canonical).unwrap();

    // Stale legacy receipt-keyed record for the SAME decision: still non-terminal.
    let mut legacy = ExecutionRecord::new_pending(
        receipt_id,
        "proposal-legacy",
        receipt_id,
        terminate_clearing_effect(decision_hash, "legacy"),
    );
    legacy.status = ExecutionStatus::Failed;
    legacy.retries = 1;
    exec_store.put(&legacy).unwrap();

    let callback = create_decision_executor_callback(executor);
    callback(
        terminate_clearing_effect(decision_hash, "replay"),
        receipt_id.to_string(),
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // The terminal canonical record wins: the stale legacy record is NOT
    // re-dispatched (its status and retry count are untouched).
    let legacy_after = exec_store.get(receipt_id).unwrap().unwrap();
    assert_eq!(
        legacy_after.status,
        ExecutionStatus::Failed,
        "stale legacy record must not be re-dispatched when the canonical record is terminal"
    );
    assert_eq!(
        legacy_after.retries, 1,
        "stale legacy record retry count must be untouched"
    );
}

/// Test 3: Startup recovery picks up an Executing record and completes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_startup_recovery_completes_executing_decision() {
    let tmp = TempDir::new().unwrap();
    let (executor, exec_store) = make_executor_with_ledger(tmp.path());

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
        report.recovered_not_executed, 1,
        "NoOp recovery should complete as structurally not executed"
    );
    assert_eq!(report.recovered_confirmed, 0);
    assert_eq!(report.recovered_failed, 0);

    // NoOp does not execute; terminal NotExecuted does not bump retries like mark_failed
    let after = exec_store.get("hash-fail-retry").unwrap().unwrap();
    assert_eq!(after.status, ExecutionStatus::NotExecuted);
    assert_eq!(after.retries, 1);
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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_callback_e2e_budget_create_and_spend() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback;

    let budget_store = Arc::new(MemoryBudgetStore::new());
    let tmp = TempDir::new().unwrap();
    let (executor, exec_store) =
        make_executor_with_budget_and_ledger(budget_store.clone(), tmp.path());
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

    // Count terminal not-executed records (NoOp is honest non-success, not Confirmed theater)
    let not_executed = exec_store
        .list_by_status(ExecutionStatus::NotExecuted)
        .unwrap();
    assert_eq!(
        not_executed.len(),
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

/// Test: TreasuryEffect::Allocate with a populated decision_hash executes and produces a
/// real ledger entry (debit treasury → credit budget liability account).
///
/// This test proves "governance allocation → actual economic state change":
/// - `decision_hash` on the effect flows through `treasury_effect_to_operation`
/// - `execute_treasury_operation` does NOT hit the Deferred path
/// - A ledger entry is committed with Governance provenance
/// - The execution record reaches `Confirmed`
#[tokio::test(flavor = "multi_thread")]
async fn test_allocate_with_decision_hash_executes_and_produces_ledger_entry() {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    let tmp = TempDir::new().unwrap();
    let ledger_store_path = tmp.path().join("ledger");
    let exec_store_path = tmp.path().join("execution");
    std::fs::create_dir_all(&ledger_store_path).unwrap();
    std::fs::create_dir_all(&exec_store_path).unwrap();

    let decision_hash = "hash-allocate-executes-1";
    let decision_receipt_id = "receipt-allocate-executes-1";
    let treasury_did = RT_TEST_TREASURY;
    let budget_id = "alloc-member-services-receipt-allocate-executes-1";
    let recipient_did = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse";

    let ledger_store = Arc::new(SledStore::open(&ledger_store_path).unwrap());
    let ledger = Arc::new(TokioRwLock::new(
        icn_ledger::Ledger::new(ledger_store).unwrap(),
    ));

    let ledger_service = Arc::new(LedgerServiceImpl::new(
        ledger.clone(),
        Arc::new(AllowAllOracle::wildcard()),
        treasury_did.parse().unwrap(),
    ));
    let kernel_executor =
        KernelGovernanceExecutor::new(Arc::new(StubParamStore)).with_ledger_service(ledger_service);

    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
    let exec_store = Arc::new(SledExecutionStore::new(exec_backend));
    let executor = DecisionExecutor::new(dispatcher, exec_store.clone());

    let effects = vec![KernelEffect::Treasury(TreasuryEffect::Allocate {
        treasury_did: treasury_did.to_string(),
        budget_id: budget_id.to_string(),
        recipient_did: recipient_did.to_string(),
        amount: 1_000,
        currency: "HOURS".to_string(),
        decision_hash: decision_hash.to_string(),
    })];

    let results = executor
        .execute(
            effects,
            decision_receipt_id,
            decision_hash,
            "proposal-alloc-1",
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "expected one EffectResult");
    assert!(
        results[0].success,
        "Allocate with decision_hash must succeed (not deferred), got: {:?}",
        results[0].message
    );
    assert!(
        !results[0].not_executed,
        "Allocate must not be marked not_executed when decision_hash is present"
    );
    assert!(
        !results[0].message.contains("Deferred"),
        "expected no Deferred message, got: {:?}",
        results[0].message
    );

    let record = exec_store.get(decision_hash).unwrap().unwrap();
    assert_eq!(
        record.status,
        ExecutionStatus::Confirmed,
        "execution record must be Confirmed after successful Allocate"
    );
    assert_eq!(
        record.ledger_entry_ids.len(),
        1,
        "exactly one ledger entry expected for Allocate"
    );

    let entry_hash = content_hash_from_hex(&record.ledger_entry_ids[0]).unwrap();
    let ledger_guard = ledger.read().await;
    let entry = ledger_guard.get_entry(&entry_hash).unwrap().unwrap();
    assert!(
        matches!(
            &entry.provenance,
            ProvenanceRef::Governance { receipt_id, decision_hash: dh }
                if receipt_id == decision_receipt_id && dh == decision_hash
        ),
        "ledger entry provenance must carry governance receipt_id and decision_hash"
    );
    // Verify account deltas: treasury debit + budget liability credit
    assert_eq!(
        entry.accounts.len(),
        2,
        "Allocate must produce exactly 2 account deltas"
    );
    let debit = entry
        .accounts
        .iter()
        .find(|a| a.debit.is_some())
        .expect("must have a debit delta");
    let credit = entry
        .accounts
        .iter()
        .find(|a| a.credit.is_some())
        .expect("must have a credit delta");
    assert_eq!(
        debit.account_id.as_str(),
        treasury_did,
        "debit must be from treasury"
    );
    assert_eq!(debit.debit, Some(1_000));
    assert_eq!(credit.credit, Some(1_000));
    // Budget liability DID is a synthetic hash-derived DID — verify it's NOT the recipient_did
    // (which would be wrong: recipient_did is audit metadata, not the ledger routing key)
    assert_ne!(
        credit.account_id.as_str(),
        recipient_did,
        "credit must go to budget liability account (budget_id), not directly to recipient_did"
    );
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
            expected_nonce: 0,
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
        assert_eq!(record.state_change_hashes.len(), 1);
        let entry_hash = record.ledger_entry_ids[0].clone();
        assert_eq!(record.state_change_hashes[0], entry_hash);

        let hash = content_hash_from_hex(&entry_hash);
        assert!(hash.is_ok(), "ledger entry id must parse as ContentHash");
        let hash = hash.unwrap();
        let ledger_guard = ledger.read().await;
        let entry = ledger_guard.get_entry(&hash).unwrap().unwrap();
        assert!(
            matches!(
                &entry.provenance,
                ProvenanceRef::Governance { receipt_id, decision_hash: dh }
                    if receipt_id == decision_receipt_id && dh == decision_hash
            ),
            "entry provenance must be Governance with matching receipt_id and decision_hash"
        );
        drop(ledger_guard);

        entry_hash
    };

    let reopened_ledger_store = Arc::new(SledStore::open(&ledger_store_path).unwrap());
    let reopened_ledger = Ledger::new(reopened_ledger_store).unwrap();
    let reopened_hash = content_hash_from_hex(&entry_hash).unwrap();
    let reopened_entry = reopened_ledger.get_entry(&reopened_hash).unwrap().unwrap();
    assert!(
        matches!(
            &reopened_entry.provenance,
            ProvenanceRef::Governance { receipt_id, decision_hash: dh }
                if receipt_id == decision_receipt_id && dh == decision_hash
        ),
        "reopened entry provenance must be Governance with matching receipt_id and decision_hash"
    );

    let reopened_exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
    let reopened_exec_store = SledExecutionStore::new(reopened_exec_backend);
    let reopened_record = reopened_exec_store.get(decision_hash).unwrap().unwrap();
    assert_eq!(reopened_record.ledger_entry_ids, vec![entry_hash]);
}

/// Test 11: Withdraw proposal payload translates through the real boundary and
/// executes as a single durable treasury mutation across restart/replay.
#[tokio::test(flavor = "multi_thread")]
async fn test_withdraw_payload_restart_resume_single_mutation() {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;
    use icn_identity::Did;

    let tmp = TempDir::new().unwrap();
    let ledger_store_path = tmp.path().join("ledger");
    let exec_store_path = tmp.path().join("execution");
    std::fs::create_dir_all(&ledger_store_path).unwrap();
    std::fs::create_dir_all(&exec_store_path).unwrap();

    let decision_hash = "hash-withdraw-restart-1";
    let decision_receipt_id = "receipt-withdraw-restart-1";
    let domain_id = "test-domain-withdraw-restart";
    let treasury_did_str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
    let treasury_did: Did = treasury_did_str.parse().unwrap();
    let recipient_did: Did = treasury_did_str.parse().unwrap();

    let payload = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Withdraw {
            treasury_did: treasury_did.clone(),
            recipient: recipient_did,
            amount: 100,
            currency: "HOURS".to_string(),
            purpose: "Withdraw restart safety test".to_string(),
            budget_id: None,
            nonce: 0,
        },
    };

    let effects =
        translate_payload_to_effects(&payload, decision_receipt_id, decision_hash, domain_id)
            .expect("withdraw payload should translate");
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        KernelEffect::Treasury(TreasuryEffect::Spend {
            decision_receipt_id: rid,
            decision_hash: dh,
            ..
        }) => {
            assert_eq!(rid, decision_receipt_id);
            assert_eq!(dh, decision_hash);
        }
        other => panic!("expected translated treasury spend effect, got {other:?}"),
    }

    let entry_hash = {
        let ledger_store = Arc::new(SledStore::open(&ledger_store_path).unwrap());
        let ledger = Ledger::new(ledger_store).unwrap();
        let ledger = Arc::new(tokio::sync::RwLock::new(ledger));

        let ledger_service = Arc::new(LedgerServiceImpl::new(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did_str.parse().unwrap(),
        ));
        let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_ledger_service(ledger_service);
        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
        let exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
        let exec_store = Arc::new(SledExecutionStore::new(exec_backend));
        let executor = DecisionExecutor::new(dispatcher, exec_store.clone());

        let results = executor
            .execute(
                effects.clone(),
                decision_receipt_id,
                decision_hash,
                "proposal-withdraw-restart-1",
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        let record = exec_store.get(decision_hash).unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
        assert_eq!(record.ledger_entry_ids.len(), 1);
        let entry_hash = record.ledger_entry_ids[0].clone();

        let parsed = content_hash_from_hex(&entry_hash).unwrap();
        let entry = ledger.read().await.get_entry(&parsed).unwrap().unwrap();
        assert!(
            matches!(
                &entry.provenance,
                ProvenanceRef::Governance { receipt_id, decision_hash: dh }
                    if receipt_id == decision_receipt_id && dh == decision_hash
            ),
            "entry provenance must be Governance with matching receipt_id and decision_hash"
        );
        assert_eq!(ledger.read().await.count_entries().unwrap(), 1);

        entry_hash
    };

    let replay_effects =
        translate_payload_to_effects(&payload, decision_receipt_id, decision_hash, domain_id)
            .expect("withdraw replay payload should translate");
    let reopened_ledger_store = Arc::new(SledStore::open(&ledger_store_path).unwrap());
    let reopened_ledger = Ledger::new(reopened_ledger_store).unwrap();
    let reopened_ledger = Arc::new(tokio::sync::RwLock::new(reopened_ledger));

    let reopened_ledger_service = Arc::new(LedgerServiceImpl::new(
        reopened_ledger.clone(),
        Arc::new(AllowAllOracle::wildcard()),
        treasury_did_str.parse().unwrap(),
    ));
    let reopened_kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
        .with_ledger_service(reopened_ledger_service);
    let reopened_dispatcher = Arc::new(EffectDispatcher::new(Arc::new(reopened_kernel_executor)));
    let reopened_exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
    let reopened_exec_store = Arc::new(SledExecutionStore::new(reopened_exec_backend));
    let reopened_executor = DecisionExecutor::new(reopened_dispatcher, reopened_exec_store.clone());

    let report = reopened_executor.recover_in_flight().await.unwrap();
    assert_eq!(
        report.total(),
        0,
        "confirmed records should not be re-executed"
    );

    let replay_results = reopened_executor
        .execute(
            replay_effects,
            decision_receipt_id,
            decision_hash,
            "proposal-withdraw-restart-1",
        )
        .await
        .unwrap();
    assert!(
        replay_results.is_empty(),
        "terminal decision replay should be skipped by idempotency gate"
    );
    assert_eq!(reopened_ledger.read().await.count_entries().unwrap(), 1);

    let parsed = content_hash_from_hex(&entry_hash).unwrap();
    let reopened_entry = reopened_ledger
        .read()
        .await
        .get_entry(&parsed)
        .unwrap()
        .unwrap();
    assert!(
        matches!(
            &reopened_entry.provenance,
            ProvenanceRef::Governance { receipt_id, decision_hash: dh }
                if receipt_id == decision_receipt_id && dh == decision_hash
        ),
        "reopened entry provenance must be Governance with matching receipt_id and decision_hash"
    );

    let reopened_record = reopened_exec_store.get(decision_hash).unwrap().unwrap();
    assert_eq!(reopened_record.status, ExecutionStatus::Confirmed);
    assert_eq!(reopened_record.ledger_entry_ids, vec![entry_hash]);
    assert_eq!(
        reopened_record.state_change_hashes,
        reopened_record.ledger_entry_ids
    );
}

/// Test 12: Treasury Spend payload translates to a treasury spend effect.
#[test]
fn test_treasury_spend_payload_translation_produces_treasury_spend_effect() {
    use icn_identity::Did;

    let decision_receipt_id = "gov:test-domain:test-proposal:receipt";
    let decision_hash = "test-decision-hash";
    let domain_id = "test-domain-for-treasury-spend";
    let treasury_did: Did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
        .parse()
        .unwrap();
    let recipient: Did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
        .parse()
        .unwrap();

    let payload = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Spend {
            treasury_did: treasury_did.clone(),
            amount: 100,
            currency: "HOURS".to_string(),
            recipient,
            memo: "PR-2 treasury spend".to_string(),
            nonce: 0,
        },
    };

    let effects =
        translate_payload_to_effects(&payload, decision_receipt_id, decision_hash, domain_id)
            .expect("treasury spend payload should translate");
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: effect_treasury_did,
            currency,
            decision_receipt_id: rid,
            decision_hash: dh,
            ..
        }) => {
            assert_eq!(effect_treasury_did, &treasury_did.to_string());
            assert_eq!(currency, "HOURS");
            assert_eq!(rid, decision_receipt_id);
            assert_eq!(dh, decision_hash);
        }
        other => panic!("expected Treasury Spend effect, got {other:?}"),
    }
}

/// Production-shaped path: ledger is wired (as in `lifecycle`); `TreasuryEffect::Allocate` maps to
/// a `TreasuryOperation` without `decision_hash`, so `KernelTreasuryExecutor` returns
/// `ExecutionOutcome::Deferred` → `EffectResult.not_executed` → durable `NotExecuted` (Sled reopen).
#[tokio::test(flavor = "multi_thread")]
async fn test_deferred_outcome_sled_persists_not_executed() {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    struct LedgerWiredDeferTestStub {
        oracle: Arc<dyn PolicyOracle>,
    }

    impl LedgerService for LedgerWiredDeferTestStub {
        fn oracle(&self) -> Arc<dyn PolicyOracle> {
            self.oracle.clone()
        }

        fn balance(&self, _account: &LedgerDid, _currency: &str) -> i64 {
            0
        }

        fn credit_limit(&self, _account: &LedgerDid, _currency: &str) -> i64 {
            0
        }

        fn record_event(&self, _event: LedgerEvent) {}

        fn submit_treasury_entry(
            &self,
            _entry: TreasuryEntryRequest,
        ) -> Result<TreasuryEntryResult, String> {
            panic!(
                "submit_treasury_entry must not run: missing decision_hash defers before ledger call"
            );
        }

        fn get_treasury_nonce(&self, _treasury_id: &str) -> Result<u64, String> {
            Ok(0)
        }
    }

    let tmp = TempDir::new().unwrap();
    let exec_store_path = tmp.path().join("exec-deferred");
    std::fs::create_dir_all(&exec_store_path).unwrap();

    let decision_hash = "hash-sled-deferred-not-executed-1";
    let decision_receipt_id = "receipt-sled-deferred-1";
    let proposal_id = "proposal-sled-deferred-1";

    let effects = vec![KernelEffect::Treasury(TreasuryEffect::Allocate {
        treasury_did: RT_TEST_TREASURY.to_string(),
        budget_id: "budget-sled-defer".to_string(),
        recipient_did: "did:icn:member-test".to_string(),
        amount: 50,
        currency: "HOURS".to_string(),
        // Deliberately empty: empty decision_hash → None in TreasuryOperation → Deferred path.
        // Tests backward compat for effects persisted before decision_hash was added to Allocate.
        decision_hash: String::new(),
    })];

    {
        let oracle: Arc<dyn PolicyOracle> = Arc::new(AllowAllOracle::wildcard());
        let ledger_service: Arc<dyn LedgerService> = Arc::new(LedgerWiredDeferTestStub {
            oracle: oracle.clone(),
        });
        let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_ledger_service(ledger_service);
        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
        let exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
        let exec_store = Arc::new(SledExecutionStore::new(exec_backend));
        let executor = DecisionExecutor::new(dispatcher, exec_store.clone());

        let results = executor
            .execute(effects, decision_receipt_id, decision_hash, proposal_id)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].not_executed);
        assert!(results[0].message.contains("Deferred:"));
        assert!(
            results[0].message.contains("decision_hash"),
            "expected missing-provenance defer, got {:?}",
            results[0].message
        );

        let record = exec_store.get(decision_hash).unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::NotExecuted);
        assert_eq!(record.retries, 0);
        assert_ne!(record.status, ExecutionStatus::Confirmed);
        assert_ne!(record.status, ExecutionStatus::Failed);
    }

    let reopened = SledStore::open(&exec_store_path).unwrap();
    let reopened_store = SledExecutionStore::new(Arc::new(reopened));
    let reread = reopened_store.get(decision_hash).unwrap().unwrap();
    assert_eq!(reread.status, ExecutionStatus::NotExecuted);
    assert_eq!(reread.retries, 0);
    assert!(reread.error.as_deref().unwrap_or("").contains("Deferred:"));
}

/// Test 15: DistributeSurplus execution produces a single ledger entry with 2*N account deltas.
///
/// This proves:
/// 1. TreasuryEffect::DistributeSurplus no longer hits the `not_executed: true` wall
/// 2. N-recipient fan-out is a single JournalEntry (one-receipt-one-entry invariant)
/// 3. Each recipient gets a debit-from-treasury + credit-to-member pair
/// 4. Governance provenance is recorded on the entry
#[tokio::test(flavor = "multi_thread")]
async fn test_distribute_surplus_executes_single_entry_with_n_deltas() {
    use icn_core::supervisor::decision_executor::DecisionExecutor;
    use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
    use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

    let tmp = TempDir::new().unwrap();
    let ledger_store_path = tmp.path().join("ledger");
    let exec_store_path = tmp.path().join("execution");
    std::fs::create_dir_all(&ledger_store_path).unwrap();
    std::fs::create_dir_all(&exec_store_path).unwrap();

    let decision_hash = "hash-distribute-surplus-executes-1";
    let decision_receipt_id = "receipt-distribute-surplus-executes-1";
    let treasury_did = RT_TEST_TREASURY;

    // Two recipients with known valid multibase DIDs
    let member_a = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
    let member_b = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse";
    let distributions = vec![
        (member_a.to_string(), 300_i64),
        (member_b.to_string(), 200_i64),
    ];
    let total_amount = 500_i64;

    let ledger_store = Arc::new(SledStore::open(&ledger_store_path).unwrap());
    let ledger = Arc::new(TokioRwLock::new(
        icn_ledger::Ledger::new(ledger_store).unwrap(),
    ));

    let ledger_service = Arc::new(LedgerServiceImpl::new(
        ledger.clone(),
        Arc::new(AllowAllOracle::wildcard()),
        treasury_did.parse().unwrap(),
    ));
    let kernel_executor =
        KernelGovernanceExecutor::new(Arc::new(StubParamStore)).with_ledger_service(ledger_service);

    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_backend = Arc::new(SledStore::open(&exec_store_path).unwrap());
    let exec_store = Arc::new(SledExecutionStore::new(exec_backend));
    let executor = DecisionExecutor::new(dispatcher, exec_store.clone());

    let effects = vec![KernelEffect::Treasury(TreasuryEffect::DistributeSurplus {
        treasury_did: treasury_did.to_string(),
        total_amount,
        currency: "HOURS".to_string(),
        distributions: distributions.clone(),
        decision_receipt_id: decision_receipt_id.to_string(),
        decision_hash: decision_hash.to_string(),
    })];

    let results = executor
        .execute(
            effects,
            decision_receipt_id,
            decision_hash,
            "proposal-distribute-surplus-1",
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "expected one EffectResult");
    assert!(
        results[0].success,
        "DistributeSurplus must succeed (not hit not_executed wall), got: {:?}",
        results[0].message
    );
    assert!(
        !results[0].not_executed,
        "DistributeSurplus must not be marked not_executed when decision_hash is present"
    );
    assert!(
        !results[0].message.contains("Deferred"),
        "expected no Deferred message, got: {:?}",
        results[0].message
    );

    // One-receipt-one-entry invariant: N recipients → 1 JournalEntry
    let record = exec_store.get(decision_hash).unwrap().unwrap();
    assert_eq!(
        record.status,
        ExecutionStatus::Confirmed,
        "execution record must be Confirmed after DistributeSurplus"
    );
    assert_eq!(
        record.ledger_entry_ids.len(),
        1,
        "exactly one ledger entry expected for DistributeSurplus regardless of recipient count"
    );

    // Verify the single entry has 2*N account deltas
    let entry_hash = content_hash_from_hex(&record.ledger_entry_ids[0]).unwrap();
    let ledger_guard = ledger.read().await;
    let entry = ledger_guard.get_entry(&entry_hash).unwrap().unwrap();

    assert!(
        matches!(
            &entry.provenance,
            ProvenanceRef::Governance { receipt_id, decision_hash: dh }
                if receipt_id == decision_receipt_id && dh == decision_hash
        ),
        "ledger entry provenance must carry governance receipt_id and decision_hash"
    );

    // 2 recipients × 2 deltas (debit treasury + credit member) = 4 account deltas
    assert_eq!(
        entry.accounts.len(),
        4,
        "DistributeSurplus with 2 recipients must produce 4 account deltas (debit+credit per member)"
    );

    let debits: Vec<_> = entry
        .accounts
        .iter()
        .filter(|a| a.debit.is_some())
        .collect();
    let credits: Vec<_> = entry
        .accounts
        .iter()
        .filter(|a| a.credit.is_some())
        .collect();
    assert_eq!(debits.len(), 2, "must have 2 treasury debits");
    assert_eq!(credits.len(), 2, "must have 2 member credits");

    // All debits from treasury
    for debit in &debits {
        assert_eq!(
            debit.account_id.as_str(),
            treasury_did,
            "all debits must come from treasury"
        );
    }

    // Credits go to member DIDs, total = total_amount
    let credit_total: i64 = credits.iter().filter_map(|a| a.credit).sum();
    assert_eq!(
        credit_total, total_amount,
        "total credits must equal total_amount"
    );
    let credit_dids: std::collections::HashSet<&str> =
        credits.iter().map(|a| a.account_id.as_str()).collect();
    assert!(
        credit_dids.contains(member_a),
        "member_a must appear in credits"
    );
    assert!(
        credit_dids.contains(member_b),
        "member_b must appear in credits"
    );
}

/// The dispatch-evidence sink — when wired via the
///  callback factory — is invoked once per completed
/// execution with the original effects, the per-effect results, and a
/// non-zero  timestamp. This pins the kernel-neutral seam
/// that closes the actor-path dispatch-evidence gap.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_callback_invokes_dispatch_evidence_sink() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback_with_sink;
    use icn_kernel_api::effects::{DispatchEvidenceSink, EffectResult, KernelEffect};
    use std::sync::Mutex;

    type SinkCall = (String, Vec<KernelEffect>, Vec<EffectResult>, u64);
    #[derive(Default)]
    struct CapturingSink {
        calls: Mutex<Vec<SinkCall>>,
    }
    impl DispatchEvidenceSink for CapturingSink {
        fn record_effects(
            &self,
            decision_receipt_id: &str,
            effects: &[KernelEffect],
            results: &[EffectResult],
            recorded_at: u64,
        ) {
            self.calls.lock().unwrap().push((
                decision_receipt_id.to_string(),
                effects.to_vec(),
                results.to_vec(),
                recorded_at,
            ));
        }
    }

    let tmp = TempDir::new().unwrap();
    let (executor, _exec_store) = make_executor_with_ledger(tmp.path());
    let sink = Arc::new(CapturingSink::default());
    let callback = create_decision_executor_callback_with_sink(
        executor,
        Some(sink.clone() as Arc<dyn DispatchEvidenceSink>),
    );

    let effects = spend_effect("hash-sink-1");
    callback(effects.clone(), "receipt-sink-1".to_string());

    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    let calls = sink.calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        1,
        "sink must be invoked exactly once after execute"
    );
    let (receipt_id, seen_effects, seen_results, recorded_at) = &calls[0];
    assert_eq!(receipt_id, "receipt-sink-1");
    assert_eq!(seen_effects.len(), 1);
    assert_eq!(seen_effects[0], effects[0]);
    assert_eq!(seen_results.len(), 1);
    assert!(
        *recorded_at > 0,
        "recorded_at must be set from wall clock at dispatch completion"
    );
}

/// When no sink is wired, the legacy
/// path must still function — no panics, decisions still execute.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_callback_without_sink_still_executes() {
    use icn_core::supervisor::decision_executor::create_decision_executor_callback_with_sink;

    let tmp = TempDir::new().unwrap();
    let (executor, exec_store) = make_executor_with_ledger(tmp.path());
    let callback = create_decision_executor_callback_with_sink(executor, None);

    let effects = spend_effect("hash-no-sink");
    callback(effects, "receipt-no-sink".to_string());

    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    let record = exec_store.get("hash-no-sink").unwrap().unwrap();
    assert_eq!(record.status, ExecutionStatus::Confirmed);
}
