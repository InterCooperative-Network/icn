//! Gateway settlements for a named operation (icn#2871 part 2), through the real
//! `LedgerManager` and a real reopen of its sled store.
//!
//! `create_settlement` gives its journal entry no nonce, so two identical settlements issued in
//! one millisecond claim one entry id. `create_settlement_for_operation` takes the caller's
//! durable operation id and settles each operation at most once:
//!
//! - N identical requests with N operation ids are N durable entries, whatever the timing;
//! - a repeated operation id is one settlement, also after a restart, and its repeat consumes no
//!   budget and is not refused by a budget the first attempt used up;
//! - an operation id cannot be reused for a different settlement.
//!
//! No sleeps. Back-to-back calls make same-millisecond pairs near-certain, and none of the
//! assertions depends on whether a given pair shared a millisecond.
//!
//! The operation ids below are test stand-ins. How a production caller derives one from its own
//! durable source (escrow id, schedule cycle, client idempotency key) is not decided here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gateway::error::GatewayError;
use icn_gateway::ledger_mgr::LedgerManager;
use icn_identity::IdentityBundle;
use icn_ledger::OperationId;
use icn_ledger_app::{Budget, BudgetPeriod, BudgetStatus, BudgetStore};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::SystemTime;

const COOP: &str = "opid-coop";
const UNIT: &str = "hours";

/// A distinct stand-in operation id per `n`.
fn op(n: u64) -> OperationId {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&n.to_be_bytes());
    bytes[31] = 0x01;
    OperationId::from_bytes(bytes)
}

async fn journal_len(mgr: &LedgerManager) -> usize {
    let ledger = mgr.get_ledger(&COOP.to_string()).await.unwrap();
    let guard = ledger.read().await;
    guard.get_all_entries().unwrap().len()
}

/// The invariant icn#2878 records as RED for nonce-less settlements, satisfied when each
/// request names its operation.
#[tokio::test]
async fn identical_settlements_with_distinct_operations_are_distinct_durable_entries() {
    const N: u64 = 200;
    let dir = tempfile::tempdir().unwrap();
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();

    {
        let mgr = LedgerManager::new_with_storage(dir.path().to_path_buf());
        let mut ids = HashSet::new();
        for i in 0..N {
            let settled = mgr
                .create_settlement_for_operation(
                    &COOP.to_string(),
                    alice.did(),
                    bob.did(),
                    1,
                    UNIT.into(),
                    None,
                    op(i),
                )
                .await
                .unwrap_or_else(|e| panic!("settlement {i} of {N} was refused: {e}"));
            assert!(!settled.replayed, "settlement {i} is a new operation");
            ids.insert(settled.entry_id);
        }
        assert_eq!(ids.len(), N as usize, "every operation has its own entry");
        assert_eq!(journal_len(&mgr).await, N as usize);
    }

    // Reopen: the journal, not the in-memory cache, is the record.
    let reopened = LedgerManager::new_with_storage(dir.path().to_path_buf());
    let ledger = reopened.get_ledger(&COOP.to_string()).await.unwrap();
    let guard = ledger.read().await;
    assert_eq!(guard.get_all_entries().unwrap().len(), N as usize);
    assert_eq!(guard.get_balance(bob.did(), UNIT).abs(), N as i64);
    assert_eq!(guard.get_balance(alice.did(), UNIT).abs(), N as i64);
    guard
        .verify_integrity()
        .expect("cached balances must reconcile with the journal after reopen");
}

#[tokio::test]
async fn a_repeated_operation_is_one_settlement_across_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();
    let coop = COOP.to_string();

    let first = {
        let mgr = LedgerManager::new_with_storage(dir.path().to_path_buf());
        let first = mgr
            .create_settlement_for_operation(
                &coop,
                alice.did(),
                bob.did(),
                7,
                UNIT.into(),
                None,
                op(1),
            )
            .await
            .unwrap();
        assert!(!first.replayed);

        let again = mgr
            .create_settlement_for_operation(
                &coop,
                alice.did(),
                bob.did(),
                7,
                UNIT.into(),
                None,
                op(1),
            )
            .await
            .unwrap();
        assert!(again.replayed);
        assert_eq!(again.entry_id, first.entry_id);
        assert_eq!(journal_len(&mgr).await, 1);
        first
    };

    let reopened = LedgerManager::new_with_storage(dir.path().to_path_buf());
    let after_restart = reopened
        .create_settlement_for_operation(&coop, alice.did(), bob.did(), 7, UNIT.into(), None, op(1))
        .await
        .unwrap();
    assert!(after_restart.replayed);
    assert_eq!(after_restart.entry_id, first.entry_id);

    let ledger = reopened.get_ledger(&coop).await.unwrap();
    let guard = ledger.read().await;
    assert_eq!(guard.get_all_entries().unwrap().len(), 1);
    assert_eq!(guard.get_balance(bob.did(), UNIT).abs(), 7);
    guard.verify_integrity().unwrap();
}

#[tokio::test]
async fn an_operation_cannot_be_reused_for_another_settlement() {
    let dir = tempfile::tempdir().unwrap();
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();
    let carol = IdentityBundle::generate().unwrap();
    let coop = COOP.to_string();
    let mgr = LedgerManager::new_with_storage(dir.path().to_path_buf());

    mgr.create_settlement_for_operation(&coop, alice.did(), bob.did(), 7, UNIT.into(), None, op(1))
        .await
        .unwrap();

    let other_amount = mgr
        .create_settlement_for_operation(&coop, alice.did(), bob.did(), 8, UNIT.into(), None, op(1))
        .await;
    assert!(
        matches!(other_amount, Err(GatewayError::Conflict(_))),
        "expected Conflict, got {other_amount:?}"
    );

    let other_payee = mgr
        .create_settlement_for_operation(
            &coop,
            alice.did(),
            carol.did(),
            7,
            UNIT.into(),
            None,
            op(1),
        )
        .await;
    assert!(
        matches!(other_payee, Err(GatewayError::Conflict(_))),
        "expected Conflict, got {other_payee:?}"
    );

    let ledger = mgr.get_ledger(&coop).await.unwrap();
    let guard = ledger.read().await;
    assert_eq!(guard.get_all_entries().unwrap().len(), 1);
    assert_eq!(guard.get_balance(bob.did(), UNIT).abs(), 7);
    assert_eq!(guard.get_balance(carol.did(), UNIT), 0);
    guard.verify_integrity().unwrap();
}

/// A replay is answered from the journal: it records no second spending, and it is not refused
/// by a budget that the first attempt (and only the first attempt) consumed.
#[tokio::test]
async fn a_replayed_operation_consumes_no_budget_and_is_not_budget_gated() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let budget_store = Arc::new(BudgetStore::new(db));
    let mut mgr = LedgerManager::new();
    mgr.set_budget_store(budget_store.clone());

    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();
    let coop = COOP.to_string();
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    budget_store
        .insert(Budget {
            id: "budget-1".to_string(),
            owner: alice.did().to_string(),
            account: alice.did().to_string(),
            currency: UNIT.to_string(),
            limit: 100,
            spent: 0,
            period: BudgetPeriod::Monthly,
            period_start: now,
            period_end: now + 2_592_000,
            status: BudgetStatus::Active,
            notification_thresholds: vec![80, 100],
            notified_thresholds: vec![],
            description: "Test Budget".to_string(),
            created_at: now,
            updated_at: now,
        })
        .unwrap();
    let spent = || budget_store.get("budget-1").unwrap().unwrap().spent;

    let first = mgr
        .create_settlement_for_operation(
            &coop,
            alice.did(),
            bob.did(),
            60,
            UNIT.into(),
            None,
            op(1),
        )
        .await
        .unwrap();
    assert!(!first.replayed);
    assert_eq!(spent(), 60);

    let replay = mgr
        .create_settlement_for_operation(
            &coop,
            alice.did(),
            bob.did(),
            60,
            UNIT.into(),
            None,
            op(1),
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.entry_id, first.entry_id);
    assert_eq!(spent(), 60, "a replay records no spending");

    let new_operation = mgr
        .create_settlement_for_operation(
            &coop,
            alice.did(),
            bob.did(),
            60,
            UNIT.into(),
            None,
            op(2),
        )
        .await;
    assert!(
        matches!(new_operation, Err(GatewayError::BudgetExceeded(_))),
        "a new operation is still budget-gated, got {new_operation:?}"
    );
    assert_eq!(spent(), 60);

    let replay_after_refusal = mgr
        .create_settlement_for_operation(
            &coop,
            alice.did(),
            bob.did(),
            60,
            UNIT.into(),
            None,
            op(1),
        )
        .await
        .unwrap();
    assert!(replay_after_refusal.replayed);
    assert_eq!(journal_len(&mgr).await, 1);
}

/// Sixteen callers present one operation at once. One settles; all are given the same entry.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_for_one_operation_settle_once() {
    const CALLERS: usize = 16;
    let dir = tempfile::tempdir().unwrap();
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();
    let mgr = Arc::new(LedgerManager::new_with_storage(dir.path().to_path_buf()));
    let start = Arc::new(tokio::sync::Barrier::new(CALLERS));

    let mut tasks = Vec::new();
    for _ in 0..CALLERS {
        let mgr = mgr.clone();
        let start = start.clone();
        let from = alice.did().clone();
        let to = bob.did().clone();
        tasks.push(tokio::spawn(async move {
            start.wait().await;
            mgr.create_settlement_for_operation(
                &COOP.to_string(),
                &from,
                &to,
                3,
                UNIT.into(),
                None,
                op(1),
            )
            .await
            .unwrap()
        }));
    }

    let mut outcomes = Vec::new();
    for task in tasks {
        outcomes.push(task.await.unwrap());
    }

    assert_eq!(
        outcomes.iter().filter(|o| !o.replayed).count(),
        1,
        "exactly one caller settles"
    );
    let entry_id = outcomes[0].entry_id.clone();
    assert!(outcomes.iter().all(|o| o.entry_id == entry_id));

    let ledger = mgr.get_ledger(&COOP.to_string()).await.unwrap();
    let guard = ledger.read().await;
    assert_eq!(guard.get_all_entries().unwrap().len(), 1);
    assert_eq!(guard.get_balance(bob.did(), UNIT).abs(), 3);
    guard.verify_integrity().unwrap();
}
