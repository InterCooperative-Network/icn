//! Integration tests for the commons resource pool and credit accounting (Epic 6 #949).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_compute::commons_pool::{CommonsParticipant, CommonsPool};
use icn_compute::{CapacityBudget, ComputeActor, ComputeMessage, NodeCapacity, TrustCallback};
use icn_kernel_api::CellId;
use std::sync::Arc;
use std::time::Instant;

/// Helper to create a NodeCapacity for testing.
fn test_capacity(cpu: f64, mem: u64, storage: u64) -> NodeCapacity {
    NodeCapacity {
        cpu_cores_total: cpu,
        cpu_cores_available: cpu,
        memory_mb_total: mem,
        memory_mb_available: mem,
        storage_mb_available: storage,
        network_mbps: 100.0,
        gpu_devices: vec![],
        updated_at: 1000,
    }
}

/// Test 1: Unaffiliated node joins pool with full commons share.
#[test]
fn test_unaffiliated_node_joins_pool() {
    let mut pool = CommonsPool::new();

    // Unaffiliated node: no cell_id, full commons share (1.0)
    let participant = CommonsParticipant {
        did: "did:icn:unaffiliated1".to_string(),
        capacity: test_capacity(8.0, 16384, 100000),
        budget: CapacityBudget {
            local_reserve: 0.0,
            cell_share: 0.0,
            org_share: 0.0,
            federation_share: 0.0,
            commons_share: 1.0,
        },
        trust_score: 0.5,
        is_affiliated: false,
        last_announce: Instant::now(),
    };

    pool.add_participant(participant);
    assert!(pool.contains("did:icn:unaffiliated1"));
    assert_eq!(pool.participant_count(), 1);

    let agg = pool.total_commons_capacity();
    assert_eq!(agg.node_count, 1);
    // Full share: 8.0 * 1.0 = 8.0 cores
    assert!((agg.cpu_cores - 8.0).abs() < f64::EPSILON);
    // Full share: 16384 * 1.0 = 16384 MB
    assert_eq!(agg.memory_mb, 16384);
    assert_eq!(agg.storage_mb, 100000);
}

/// Test 2: Earn credits from metered resources.
#[test]
fn test_earn_credits_from_receipt() {
    use icn_ledger::commons_credits::{
        build_earn_entry, compute_credits_earned, COMMONS_CREDIT_CURRENCY,
    };

    // Simulate metered resource usage
    let cpu_millis = 5_000;
    let memory_mb_millis = 2_000_000;
    let storage_bytes = 50_000_000;
    let egress_bytes = 1_000_000;

    let credits = compute_credits_earned(cpu_millis, memory_mb_millis, storage_bytes, egress_bytes);
    // 5000 + 2000000/1000 + 50000000/1000000 + 1000000/100000
    // = 5000 + 2000 + 50 + 10 = 7060
    assert_eq!(credits, 7060);

    // Build earn entry
    let contributor =
        icn_identity::Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9")
            .unwrap();
    let entry = build_earn_entry(&contributor, credits as i64).unwrap();

    // Verify double-entry: 2 account deltas
    assert_eq!(entry.accounts.len(), 2);

    // One should be a debit (commons-mint), one a credit (contributor)
    let has_debit = entry
        .accounts
        .iter()
        .any(|a| a.debit.is_some() && a.currency == COMMONS_CREDIT_CURRENCY);
    let has_credit = entry
        .accounts
        .iter()
        .any(|a| a.credit.is_some() && a.currency == COMMONS_CREDIT_CURRENCY);
    assert!(has_debit);
    assert!(has_credit);
}

/// Test 3: Earn then spend — balance tracking.
#[test]
fn test_earn_then_spend() {
    use icn_ledger::commons_credits::{
        build_earn_entry, build_spend_entry, check_sufficient_balance,
    };

    let did = icn_identity::Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9")
        .unwrap();

    // Earn 500
    let earn = build_earn_entry(&did, 500);
    assert!(earn.is_ok());

    // Simulate balance of 500 after earning
    let balance = 500_i64;

    // Spend 200
    let spend = build_spend_entry(&did, 200);
    assert!(spend.is_ok());

    // Check remaining balance
    let remaining = check_sufficient_balance(balance, 200).unwrap();
    assert_eq!(remaining, 300);
}

/// Test 4: Insufficient credits rejected.
#[test]
fn test_insufficient_credits_rejected() {
    use icn_ledger::commons_credits::check_sufficient_balance;

    let result = check_sufficient_balance(100, 200);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.balance, 100);
    assert_eq!(err.required, 200);
}

/// Test 5: Scheduling enforcement rejects tasks when credits are insufficient.
///
/// This test documents the intended integration point for credit enforcement
/// at scheduling time. Currently deferred — Epic 6 is accounting only.
#[test]
#[ignore = "enforcement not yet implemented — Epic 6 is accounting infrastructure only"]
fn test_scheduling_enforces_credits() {
    // When integrated with the scheduler, this should look like:
    //
    //   let result = schedule_task_on_commons(task, submitter_did);
    //   assert!(matches!(
    //       result,
    //       Err(icn_compute::ComputeError::InsufficientCommonsCredits { .. })
    //   ));
    //
    // The InsufficientCommonsCredits error variant is defined in
    // icn-compute/src/error.rs but not yet emitted in any code path.
    unimplemented!("credit enforcement at scheduling time is deferred to a future epic");
}

/// Test 6: Receipt-backed earn entries produce distinct hashes (replay protection).
#[test]
fn test_receipt_backed_earn_entries_are_distinct() {
    use icn_ledger::commons_credits::{build_earn_entry_with_receipt, SettlementDedup};

    let contributor =
        icn_identity::Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9")
            .unwrap();

    // Simulate two separate execution receipts for the same amount
    let receipt_a = [0xAA; 32];
    let receipt_b = [0xBB; 32];

    let entry_a = build_earn_entry_with_receipt(&contributor, 1000, receipt_a).unwrap();
    let entry_b = build_earn_entry_with_receipt(&contributor, 1000, receipt_b).unwrap();

    // Different receipt_ids → different content hashes
    assert_ne!(
        entry_a.id.as_ref().unwrap().0,
        entry_b.id.as_ref().unwrap().0,
        "two receipts for same amount must produce distinct journal entries"
    );

    // Both have nonces set
    assert_eq!(entry_a.nonce, Some(receipt_a));
    assert_eq!(entry_b.nonce, Some(receipt_b));

    // SettlementDedup prevents double-settlement
    let mut dedup = SettlementDedup::new();
    assert!(dedup.try_settle(receipt_a).is_ok(), "first settlement ok");
    assert!(dedup.try_settle(receipt_b).is_ok(), "second receipt ok");
    assert!(
        dedup.try_settle(receipt_a).is_err(),
        "replaying receipt_a must fail"
    );
}

/// Test 7: Pool participant lifecycle — add, aggregate, remove, re-aggregate.
#[test]
fn test_pool_participant_lifecycle() {
    let mut pool = CommonsPool::new();

    // Add 3 nodes with different commons shares
    let nodes = vec![
        ("did:icn:node1", 4.0, 8192_u64, 50000_u64, 1.0_f64),
        ("did:icn:node2", 8.0, 16384, 100000, 0.5),
        ("did:icn:node3", 2.0, 4096, 20000, 0.1),
    ];

    for (did, cpu, mem, storage, share) in &nodes {
        pool.add_participant(CommonsParticipant {
            did: did.to_string(),
            capacity: test_capacity(*cpu, *mem, *storage),
            budget: CapacityBudget {
                commons_share: *share,
                ..CapacityBudget::default()
            },
            trust_score: 0.5,
            is_affiliated: false,
            last_announce: Instant::now(),
        });
    }

    assert_eq!(pool.participant_count(), 3);

    let agg = pool.total_commons_capacity();
    assert_eq!(agg.node_count, 3);
    // node1: 4*1.0=4.0, node2: 8*0.5=4.0, node3: 2*0.1=0.2 → 8.2
    assert!((agg.cpu_cores - 8.2).abs() < 1e-10);

    // Remove node2
    let removed = pool.remove_participant("did:icn:node2");
    assert!(removed.is_some());
    assert_eq!(pool.participant_count(), 2);

    let agg2 = pool.total_commons_capacity();
    assert_eq!(agg2.node_count, 2);
    // node1: 4*1.0=4.0, node3: 2*0.1=0.2 → 4.2
    assert!((agg2.cpu_cores - 4.2).abs() < 1e-10);
    // node1: 8192*1.0=8192, node3: 4096*0.1=409 → 8601
    assert_eq!(agg2.memory_mb, 8601);
}

// ─── s24-t2: live path through on_capacity_announce (#947) ───────────────────
//
// The unit tests (test_unaffiliated_zero_trust_admitted, test_affiliated_zero_trust_rejected)
// prove the CommonsPool admission logic in isolation. These actor-level tests prove the
// wiring: NodeCapacityAnnounce gossip message → on_capacity_announce() → trust_callback
// invocation → pool admission decision.
//
// Seam: ComputeActor::commons_pool_for_test() returns a clone of the internal pool Arc,
// allowing direct pool state inspection after async message processing without adding a
// production observability API.

/// Test 8 (s24-t2): Unaffiliated node with trust 0.0 is admitted via NodeCapacityAnnounce.
///
/// Proves the full gossip → actor loop → on_capacity_announce → pool admission path.
/// Before s24-t1: trust 0.0 < min_trust_score 0.1 → rejected.
/// After s24-t1: unaffiliated nodes use commons_admission_min_trust 0.0 → admitted.
#[tokio::test]
async fn test_unaffiliated_zero_trust_admitted_via_capacity_announce() {
    // trust_callback returns 0.0 for all peers — simulates a node the trust graph
    // has never seen (trust score defaults to 0.0 for unknown DIDs).
    let trust_cb: TrustCallback = Arc::new(|_| 0.0);
    let actor = ComputeActor::new("did:icn:scheduler".into(), trust_cb);

    // Clone pool Arc before spawn — the actor holds the same Arc internally.
    let pool_ref = actor.commons_pool_for_test();

    let handle = actor.spawn();

    // Unaffiliated announce: cell_id=None → is_affiliated=false → commons_admission_min_trust (0.0)
    // capacity_budget=None → effective_budget = full commons (commons_share: 1.0)
    handle
        .handle_gossip(ComputeMessage::NodeCapacityAnnounce {
            executor: "did:icn:newcomer".into(),
            capacity: test_capacity(4.0, 8192, 50000),
            cell_id: None,
            capacity_budget: None,
        })
        .await
        .expect("NodeCapacityAnnounce must be delivered");

    // Allow the actor's async loop to process the message.
    tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;

    let pool = pool_ref.read().await;
    assert!(
        pool.contains("did:icn:newcomer"),
        "unaffiliated node with trust 0.0 must be admitted via on_capacity_announce"
    );
    assert_eq!(pool.participant_count(), 1);
}

/// Test 9 (s24-t2): Affiliated node with trust 0.0 is rejected via NodeCapacityAnnounce.
///
/// Mirror of Test 8. cell_id=Some(...) → is_affiliated=true → min_trust_score (0.1).
/// Trust 0.0 < 0.1 → rejected (evicted if previously present).
#[tokio::test]
async fn test_affiliated_zero_trust_rejected_via_capacity_announce() {
    let trust_cb: TrustCallback = Arc::new(|_| 0.0);
    let actor = ComputeActor::new("did:icn:scheduler".into(), trust_cb);

    let pool_ref = actor.commons_pool_for_test();
    let handle = actor.spawn();

    // Affiliated announce: cell_id=Some(...) → is_affiliated=true → min_trust_score (0.1)
    handle
        .handle_gossip(ComputeMessage::NodeCapacityAnnounce {
            executor: "did:icn:affiliated-newcomer".into(),
            capacity: test_capacity(4.0, 8192, 50000),
            cell_id: Some(CellId([1u8; 32])),
            capacity_budget: None,
        })
        .await
        .expect("NodeCapacityAnnounce must be delivered");

    tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;

    let pool = pool_ref.read().await;
    assert!(
        !pool.contains("did:icn:affiliated-newcomer"),
        "affiliated node with trust 0.0 must be rejected via on_capacity_announce (min_trust_score 0.1)"
    );
    assert_eq!(pool.participant_count(), 0);
}
