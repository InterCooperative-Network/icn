#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Multi-node commons contribution integration test (issue #949).
//!
//! Proves that:
//! 1. `settle_commons_receipt()` produces a balanced earn/spend pair for a
//!    `ScopeLevel::Commons` task.
//! 2. After the entries are appended to one node's ledger and replicated to a
//!    second node via `handle_sync_message`, both nodes agree on the final
//!    contributor and consumer balances.
//! 3. Dedup is enforced: re-submitting the same receipt hash is rejected.
//!
//! Two in-process `Ledger` instances stand in for two network nodes.
//! `LedgerSyncMessage::NewEntry` is pushed manually — no sockets, no convergence
//! wait — keeping the test fast and deterministic.

use icn_identity::KeyPair;
use icn_kernel_api::ScopeLevel;
use icn_ledger::{
    commons_credits::COMMONS_CREDIT_CURRENCY,
    settlement::{CommonsSettlementRequest, SettlementEngine},
    sync::LedgerSyncMessage,
    Ledger,
};
use icn_store::SledStore;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_ledger() -> Ledger {
    let store = Arc::new(SledStore::temporary().expect("temp store"));
    Ledger::new(store).expect("ledger")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Core scenario: one contributor, one consumer, two ledger replicas.
///
/// After settlement on node A and replication to node B:
///   - contributor balance on both nodes == +amount
///   - consumer balance on both nodes == -amount
#[tokio::test]
async fn test_commons_settlement_replicates_to_second_node() {
    let contributor = KeyPair::generate().unwrap();
    let consumer = KeyPair::generate().unwrap();

    let engine = SettlementEngine::new();

    let mut ledger_a = make_ledger();
    let mut ledger_b = make_ledger();

    let receipt_hash: [u8; 32] = [0x01; 32];
    let amount: i64 = 500;

    let req = CommonsSettlementRequest {
        receipt_hash,
        contributor: contributor.did().clone(),
        consumer: consumer.did().clone(),
        scope: ScopeLevel::Commons,
        amount,
        consumer_balance: 10_000,
        executor_verified: true,
    };

    // Settle on node A.
    let (earn_entry, spend_entry) = engine
        .settle_commons_receipt(&req)
        .expect("settlement must succeed");

    let hash_earn = ledger_a
        .append_entry(earn_entry.clone())
        .await
        .expect("append earn entry");
    let hash_spend = ledger_a
        .append_entry(spend_entry.clone())
        .await
        .expect("append spend entry");

    // Node A balance sanity check before replication.
    // Sign convention: earn entry credits the contributor (net_change = -amount),
    // spend entry debits the consumer (net_change = +amount).
    assert_eq!(
        ledger_a.get_balance(contributor.did(), COMMONS_CREDIT_CURRENCY),
        -amount,
        "node A: contributor must have earned {amount} credits (balance = -amount in ledger convention)"
    );
    assert_eq!(
        ledger_a.get_balance(consumer.did(), COMMONS_CREDIT_CURRENCY),
        amount,
        "node A: consumer must have spent {amount} credits (balance = +amount in ledger convention)"
    );

    // Replicate to node B via manual sync messages (simulates gossip transport).
    ledger_b
        .handle_sync_message(LedgerSyncMessage::NewEntry {
            hash: hash_earn,
            entry: earn_entry,
        })
        .await
        .expect("sync earn entry to node B");
    ledger_b
        .handle_sync_message(LedgerSyncMessage::NewEntry {
            hash: hash_spend,
            entry: spend_entry,
        })
        .await
        .expect("sync spend entry to node B");

    // Both nodes must agree on balances.
    assert_eq!(
        ledger_b.get_balance(contributor.did(), COMMONS_CREDIT_CURRENCY),
        -amount,
        "node B: contributor balance must match node A after replication"
    );
    assert_eq!(
        ledger_b.get_balance(consumer.did(), COMMONS_CREDIT_CURRENCY),
        amount,
        "node B: consumer balance must match node A after replication"
    );
}

/// Three nodes: settle on node A, replicate to B and C independently.
/// All three must converge to the same balances.
#[tokio::test]
async fn test_commons_settlement_replicates_to_three_nodes() {
    let contributor = KeyPair::generate().unwrap();
    let consumer = KeyPair::generate().unwrap();

    let engine = SettlementEngine::new();

    let mut ledger_a = make_ledger();
    let mut ledger_b = make_ledger();
    let mut ledger_c = make_ledger();

    let req = CommonsSettlementRequest {
        receipt_hash: [0x02; 32],
        contributor: contributor.did().clone(),
        consumer: consumer.did().clone(),
        scope: ScopeLevel::Commons,
        amount: 1_000,
        consumer_balance: 50_000,
        executor_verified: true,
    };

    let (earn_entry, spend_entry) = engine.settle_commons_receipt(&req).unwrap();

    let hash_earn = ledger_a.append_entry(earn_entry.clone()).await.unwrap();
    let hash_spend = ledger_a.append_entry(spend_entry.clone()).await.unwrap();

    for ledger in [&mut ledger_b, &mut ledger_c] {
        ledger
            .handle_sync_message(LedgerSyncMessage::NewEntry {
                hash: hash_earn.clone(),
                entry: earn_entry.clone(),
            })
            .await
            .unwrap();
        ledger
            .handle_sync_message(LedgerSyncMessage::NewEntry {
                hash: hash_spend.clone(),
                entry: spend_entry.clone(),
            })
            .await
            .unwrap();
    }

    for (label, ledger) in [("A", &ledger_a), ("B", &ledger_b), ("C", &ledger_c)] {
        assert_eq!(
            ledger.get_balance(contributor.did(), COMMONS_CREDIT_CURRENCY),
            -1_000,
            "node {label}: contributor balance mismatch"
        );
        assert_eq!(
            ledger.get_balance(consumer.did(), COMMONS_CREDIT_CURRENCY),
            1_000,
            "node {label}: consumer balance mismatch"
        );
    }
}

/// Dedup: re-submitting the same receipt hash must be rejected after first settlement.
#[tokio::test]
async fn test_commons_settlement_dedup_rejects_replay() {
    let contributor = KeyPair::generate().unwrap();
    let consumer = KeyPair::generate().unwrap();

    let engine = SettlementEngine::new();

    let req = CommonsSettlementRequest {
        receipt_hash: [0x03; 32],
        contributor: contributor.did().clone(),
        consumer: consumer.did().clone(),
        scope: ScopeLevel::Commons,
        amount: 250,
        consumer_balance: 5_000,
        executor_verified: true,
    };

    // First submission: must succeed.
    engine
        .settle_commons_receipt(&req)
        .expect("first settlement must succeed");

    // Replay: same receipt_hash must be rejected.
    let result = engine.settle_commons_receipt(&req);
    assert!(
        result.is_err(),
        "replay of the same receipt hash must be rejected by the dedup gate"
    );
}

/// Scope guard: non-Commons scope must be hard-rejected by the settlement engine.
#[test]
fn test_commons_settlement_rejects_non_commons_scope() {
    let contributor = KeyPair::generate().unwrap();
    let consumer = KeyPair::generate().unwrap();

    let engine = SettlementEngine::new();

    for scope in [ScopeLevel::Local, ScopeLevel::Org, ScopeLevel::Federation] {
        let req = CommonsSettlementRequest {
            receipt_hash: [0x04; 32],
            contributor: contributor.did().clone(),
            consumer: consumer.did().clone(),
            scope,
            amount: 100,
            consumer_balance: 1_000,
            executor_verified: true,
        };
        assert!(
            engine.settle_commons_receipt(&req).is_err(),
            "scope {scope:?} must be rejected — only ScopeLevel::Commons is valid"
        );
    }
}

/// Multiple sequential contributions from the same contributor accumulate correctly
/// on all nodes.
#[tokio::test]
async fn test_commons_multiple_contributions_accumulate() {
    let contributor = KeyPair::generate().unwrap();
    let consumer = KeyPair::generate().unwrap();

    let engine = SettlementEngine::new();
    let mut ledger_a = make_ledger();
    let mut ledger_b = make_ledger();

    // Three separate tasks with distinct receipt hashes.
    let receipts: [[u8; 32]; 3] = [[0x10; 32], [0x11; 32], [0x12; 32]];
    let amounts: [i64; 3] = [100, 200, 300];

    for (receipt_hash, amount) in receipts.into_iter().zip(amounts) {
        let req = CommonsSettlementRequest {
            receipt_hash,
            contributor: contributor.did().clone(),
            consumer: consumer.did().clone(),
            scope: ScopeLevel::Commons,
            amount,
            consumer_balance: 100_000,
            executor_verified: true,
        };
        let (earn_entry, spend_entry) = engine.settle_commons_receipt(&req).unwrap();
        let h_earn = ledger_a.append_entry(earn_entry.clone()).await.unwrap();
        let h_spend = ledger_a.append_entry(spend_entry.clone()).await.unwrap();

        ledger_b
            .handle_sync_message(LedgerSyncMessage::NewEntry {
                hash: h_earn,
                entry: earn_entry,
            })
            .await
            .unwrap();
        ledger_b
            .handle_sync_message(LedgerSyncMessage::NewEntry {
                hash: h_spend,
                entry: spend_entry,
            })
            .await
            .unwrap();
    }

    let expected_total: i64 = amounts.iter().sum(); // 600

    for (label, ledger) in [("A", &ledger_a), ("B", &ledger_b)] {
        assert_eq!(
            ledger.get_balance(contributor.did(), COMMONS_CREDIT_CURRENCY),
            -expected_total,
            "node {label}: accumulated contributor balance mismatch"
        );
        assert_eq!(
            ledger.get_balance(consumer.did(), COMMONS_CREDIT_CURRENCY),
            expected_total,
            "node {label}: accumulated consumer balance mismatch"
        );
    }
}
