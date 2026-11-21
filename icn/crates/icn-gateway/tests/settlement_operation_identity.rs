//! Operation identity of gateway settlements (icn#2871 part 2), through the real
//! `LedgerManager::create_settlement` path and a real reopen of its sled store.
//!
//! `create_settlement` builds its journal entry with no nonce, so the entry id is a function of
//! (millisecond, payer, payee, amount, unit) only. Two legitimate identical settlements issued in
//! one millisecond therefore claim one id:
//! - on `main` the second silently overwrites the first journal record and applies its balance
//!   delta again, so the cached balances disagree with the journal (visible after a reopen);
//! - with the duplicate-id refusal of icn#2873 the second is refused: consistent, but a
//!   legitimate request fails.
//!
//! The invariant below is the target behaviour: N distinct requests produce N distinct, durable
//! journal entries, whatever the timing. It is `#[ignore]`d as a RED contract until an operation
//! identity is decided (ADR-0091, proposed). It is not a passing safeguard.
//!
//! No sleeps: back-to-back calls make same-millisecond pairs near-certain today, and a correct
//! design must pass regardless of timing, so the RED/GREEN signal does not depend on a delay.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gateway::ledger_mgr::LedgerManager;
use icn_identity::IdentityBundle;
use std::collections::HashSet;

const COOP: &str = "opid-coop";
const UNIT: &str = "hours";

async fn journal_len(mgr: &LedgerManager) -> usize {
    let ledger = mgr.get_ledger(&COOP.to_string()).await.unwrap();
    let guard = ledger.read().await;
    guard.get_all_entries().unwrap().len()
}

/// Control: the counting below is not vacuous; one settlement is one entry and reconciles.
#[tokio::test]
async fn one_settlement_is_one_reconciled_entry_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();

    {
        let mgr = LedgerManager::new_with_storage(dir.path().to_path_buf());
        mgr.create_settlement(
            &COOP.to_string(),
            alice.did(),
            bob.did(),
            1,
            UNIT.into(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(journal_len(&mgr).await, 1);
    }

    let reopened = LedgerManager::new_with_storage(dir.path().to_path_buf());
    let ledger = reopened.get_ledger(&COOP.to_string()).await.unwrap();
    let guard = ledger.read().await;
    assert_eq!(guard.get_all_entries().unwrap().len(), 1);
    assert_eq!(guard.get_balance(bob.did(), UNIT).abs(), 1);
    guard.verify_integrity().unwrap();
}

/// RED contract: N identical, distinct settlement requests are N durable journal entries.
#[tokio::test]
#[ignore = "RED contract icn#2871 part 2: nonce-less gateway settlements collide within one millisecond (ADR-0091 proposed)"]
async fn identical_settlements_are_distinct_durable_entries() {
    const N: usize = 200;
    let dir = tempfile::tempdir().unwrap();
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();

    {
        let mgr = LedgerManager::new_with_storage(dir.path().to_path_buf());
        let mut ids = HashSet::new();
        for i in 0..N {
            let id = mgr
                .create_settlement(
                    &COOP.to_string(),
                    alice.did(),
                    bob.did(),
                    1,
                    UNIT.into(),
                    None,
                )
                .await
                .unwrap_or_else(|e| panic!("settlement {i} of {N} was refused: {e}"));
            ids.insert(id);
        }
        assert_eq!(ids.len(), N, "every settlement needs its own entry id");
        assert_eq!(journal_len(&mgr).await, N, "every settlement is journaled");
    }

    // Reopen: the journal, not the in-memory cache, is the record.
    let reopened = LedgerManager::new_with_storage(dir.path().to_path_buf());
    let ledger = reopened.get_ledger(&COOP.to_string()).await.unwrap();
    let guard = ledger.read().await;
    assert_eq!(guard.get_all_entries().unwrap().len(), N);
    assert_eq!(guard.get_balance(bob.did(), UNIT).abs(), N as i64);
    assert_eq!(guard.get_balance(alice.did(), UNIT).abs(), N as i64);
    guard
        .verify_integrity()
        .expect("cached balances must reconcile with the journal after reopen");
}
