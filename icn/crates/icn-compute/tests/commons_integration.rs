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

/// Test 5: Scheduling enforcement rejects commons tasks when credits are insufficient.
///
/// Proves the credit floor check added to handle_submit() in lifecycle.rs:
/// - balance_callback returning 0 → InsufficientCommonsCredits for Commons-scoped tasks
/// - balance_callback returning 1 → task submitted successfully (Ok)
/// - no balance_callback configured → gate skipped silently (no credit rejection)
#[tokio::test]
async fn test_scheduling_enforces_credits() {
    use icn_compute::{
        BalanceCallback, ComputeError, ComputeTask, DeterminismClass, ExecutorCapability,
        FuelLimit, PrivacyClass, TaskCode, TaskPriority,
    };

    fn make_commons_task(submitter: &str) -> ComputeTask {
        ComputeTask {
            id: "test-enforce".into(),
            submitter: submitter.into(),
            coop_id: None,
            code: TaskCode::Ccl("(define x 1)".into()),
            inputs: vec![],
            fuel_limit: FuelLimit(10_000),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: TaskPriority::Normal,
            created_at: 1000,
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
            scope: icn_kernel_api::ScopeLevel::Commons,
        }
    }

    // --- Negative path: balance 0 → rejected ---
    let trust_cb: TrustCallback = Arc::new(|_| 1.0); // trust above MIN_TRUST_SUBMIT
    let zero_balance: BalanceCallback = Arc::new(|_| 0);
    let mut actor = ComputeActor::new("did:icn:scheduler".into(), trust_cb.clone());
    actor.set_balance_callback(zero_balance);
    let handle = actor.spawn();

    let result = handle
        .submit(make_commons_task("did:icn:broke-submitter"))
        .await;

    assert!(
        matches!(
            result,
            Err(ComputeError::InsufficientCommonsCredits {
                balance: 0,
                required: 10
            })
        ),
        "expected InsufficientCommonsCredits, got: {result:?}"
    );

    // --- Positive path: balance 10 → task accepted (Ok) ---
    // handle_submit() returns Ok(hash) once all gates pass. The task may not be
    // *executed* (no CCL executor configured in this test actor), but submission succeeds.
    let sufficient_balance: BalanceCallback = Arc::new(|_| 10);
    let mut actor2 = ComputeActor::new("did:icn:scheduler".into(), trust_cb.clone());
    actor2.set_balance_callback(sufficient_balance);
    let handle2 = actor2.spawn();

    let result2 = handle2
        .submit(make_commons_task("did:icn:funded-submitter"))
        .await;

    assert!(
        result2.is_ok(),
        "funded submitter must be accepted (Ok) at submit time; got: {result2:?}"
    );

    // --- No-callback path: enforcement skipped silently ---
    // When balance_callback is not configured, the credit gate does not run.
    // Commons-scoped tasks must not be rejected with InsufficientCommonsCredits.
    let actor3 = ComputeActor::new("did:icn:scheduler".into(), trust_cb);
    let handle3 = actor3.spawn();

    let result3 = handle3
        .submit(make_commons_task("did:icn:no-callback-submitter"))
        .await;

    assert!(
        !matches!(
            result3,
            Err(ComputeError::InsufficientCommonsCredits { .. })
        ),
        "no-callback actor must not reject with credit error; got: {result3:?}"
    );
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

// ─────────────────────────────────────────────────────────────────────────────
// Sprint 25: Pre-execution commons credit reservation tests (#1404)
// ─────────────────────────────────────────────────────────────────────────────

/// DIDs reused across reservation tests.
const RES_EXECUTOR_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
const RES_SUBMITTER_DID: &str = "did:icn:z9hSR6S7WPtxmTojgo6GG3k4yDPecgJY292j7xrsUGWBu";

/// Build a commons task with the given `id` and CCL `code`.
fn make_reservation_task(id: &str, code: &str) -> icn_compute::ComputeTask {
    use icn_compute::{
        DeterminismClass, ExecutorCapability, FuelLimit, PrivacyClass, TaskCode, TaskPriority,
    };
    icn_compute::ComputeTask {
        id: id.to_string(),
        submitter: RES_SUBMITTER_DID.to_string(),
        coop_id: None,
        code: TaskCode::Ccl(code.to_string()),
        inputs: vec![],
        fuel_limit: FuelLimit(10_000), // cost::compute_credits_required → 10 credits
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
        scope: icn_kernel_api::ScopeLevel::Commons,
    }
}

/// Valid CCL contract that executes successfully (returns Int 1).
fn valid_ccl() -> String {
    format!(
        r#"{{
            "name": "ReservationTest",
            "participants": ["{RES_EXECUTOR_DID}"],
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

/// Invalid CCL (not JSON) that produces ExecutionOutcome::Failed.
fn failing_ccl() -> &'static str {
    "NOT_VALID_JSON"
}

/// Test: reserve_cb is called at submit time with the correct consumer / amount / task_id.
///
/// The advisory balance check passes (balance 1_000_000 >> required 10).
/// The reserve_cb must be called exactly once before submit returns Ok.
#[tokio::test]
async fn test_reservation_created_at_submit() {
    use icn_compute::{BalanceCallback, CommonsReserveCallback, CommonsReserveRequest};
    use std::sync::Mutex;

    let reserves: Arc<Mutex<Vec<CommonsReserveRequest>>> = Arc::new(Mutex::new(vec![]));
    let reserves_cb = Arc::clone(&reserves);

    let trust_cb: TrustCallback = Arc::new(|_| 1.0);
    let balance_cb: BalanceCallback = Arc::new(|_| 1_000_000);
    let reserve_cb: CommonsReserveCallback = Arc::new(move |req| {
        reserves_cb.lock().unwrap().push(req);
        Ok(())
    });

    let mut actor = ComputeActor::new(RES_EXECUTOR_DID.into(), trust_cb);
    actor.set_balance_callback(balance_cb);
    actor.set_commons_reserve_callback(reserve_cb);
    let handle = actor.spawn();

    let result = handle
        .submit(make_reservation_task("res-submit-1", "{}"))
        .await;
    assert!(
        result.is_ok(),
        "submit must succeed with sufficient balance; got {result:?}"
    );

    let captured = reserves.lock().unwrap();
    assert_eq!(
        captured.len(),
        1,
        "reserve_cb must fire exactly once at submit"
    );
    assert_eq!(
        captured[0].consumer, RES_SUBMITTER_DID,
        "consumer must be the submitter DID"
    );
    assert_eq!(
        captured[0].task_id, "res-submit-1",
        "task_id must match the submitted task"
    );
    assert!(
        captured[0].amount > 0,
        "reserved amount must be positive (fuel=10000 → 10 credits)"
    );
}

/// Test: release_cb is called (and pending map is empty) when a task fails execution.
///
/// Flow: submit creates reservation → gossip path triggers execution with invalid CCL
/// → Failed outcome → release_cb fires with the reserved amount.
#[tokio::test]
async fn test_reservation_released_on_failure() {
    use icn_compute::{
        BalanceCallback, CommonsReleaseCallback, CommonsReleaseRequest, CommonsReserveCallback,
    };
    use std::sync::Mutex;

    let releases: Arc<Mutex<Vec<CommonsReleaseRequest>>> = Arc::new(Mutex::new(vec![]));
    let releases_cb = Arc::clone(&releases);
    let reserve_calls: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![]));
    let reserve_calls_cb = Arc::clone(&reserve_calls);

    let trust_cb: TrustCallback = Arc::new(|_| 1.0);
    let balance_cb: BalanceCallback = Arc::new(|_| 1_000_000);
    let reserve_cb: CommonsReserveCallback = Arc::new(move |req| {
        reserve_calls_cb.lock().unwrap().push(req.amount);
        Ok(())
    });
    let release_cb: CommonsReleaseCallback = Arc::new(move |req| {
        releases_cb.lock().unwrap().push(req);
    });

    let mut actor = ComputeActor::new(RES_EXECUTOR_DID.into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    actor.set_balance_callback(balance_cb);
    actor.set_commons_reserve_callback(reserve_cb);
    actor.set_commons_release_callback(release_cb);
    let handle = actor.spawn();

    let task = make_reservation_task("res-fail-1", failing_ccl());

    // Phase 1: submit creates reservation
    handle
        .submit(task.clone())
        .await
        .expect("submit must succeed");

    // Phase 2: gossip path triggers execution (invalid CCL → Failed)
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .expect("gossip delivery must succeed");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // release_cb must have fired once
    let captured = releases.lock().unwrap();
    assert_eq!(
        captured.len(),
        1,
        "release_cb must fire exactly once on failure"
    );
    assert_eq!(
        captured[0].consumer, RES_SUBMITTER_DID,
        "release consumer must be the submitter"
    );
    assert_eq!(
        captured[0].task_id, "res-fail-1",
        "release task_id must match"
    );
    assert!(
        captured[0].reserved_amount > 0,
        "released amount must be positive"
    );

    // reserve_cb fired → reserved_amount == released_amount
    let reserved = *reserve_calls.lock().unwrap().first().unwrap();
    assert_eq!(
        captured[0].reserved_amount, reserved,
        "released amount must match what was reserved"
    );
}

/// Test: release_cb is NOT called (reservation consumed) when a task succeeds; settlement_cb IS called.
///
/// Flow: submit creates reservation → gossip path triggers execution with valid CCL
/// → Success outcome → settlement_cb fires, release_cb silent.
#[tokio::test]
async fn test_reservation_consumed_on_success() {
    use icn_compute::{
        BalanceCallback, CommonsPaymentRequest, CommonsReleaseCallback, CommonsReserveCallback,
        CommonsSettlementCallback,
    };
    use std::sync::Mutex;

    let releases: Arc<Mutex<Vec<icn_compute::CommonsReleaseRequest>>> =
        Arc::new(Mutex::new(vec![]));
    let releases_cb = Arc::clone(&releases);
    let settlements: Arc<Mutex<Vec<CommonsPaymentRequest>>> = Arc::new(Mutex::new(vec![]));
    let settlements_cb = Arc::clone(&settlements);

    let trust_cb: TrustCallback = Arc::new(|_| 1.0);
    let balance_cb: BalanceCallback = Arc::new(|_| 1_000_000);
    let reserve_cb: CommonsReserveCallback = Arc::new(|_| Ok(()));
    let release_cb: CommonsReleaseCallback =
        Arc::new(move |req| releases_cb.lock().unwrap().push(req));
    let settlement_cb: CommonsSettlementCallback =
        Arc::new(move |req| settlements_cb.lock().unwrap().push(req));

    let mut actor = ComputeActor::new(RES_EXECUTOR_DID.into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    actor.set_balance_callback(balance_cb);
    actor.set_commons_reserve_callback(reserve_cb);
    actor.set_commons_release_callback(release_cb);
    actor.set_commons_settlement_callback(settlement_cb);
    let handle = actor.spawn();

    let task = make_reservation_task("res-success-1", &valid_ccl());

    // Phase 1: submit creates reservation
    handle
        .submit(task.clone())
        .await
        .expect("submit must succeed");

    // Phase 2: gossip path triggers execution (valid CCL → Success)
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .expect("gossip delivery must succeed");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // release_cb must NOT fire on success (reservation consumed, not released)
    let release_count = releases.lock().unwrap().len();
    assert_eq!(
        release_count, 0,
        "release_cb must NOT fire on success; reservation is consumed"
    );

    // settlement_cb must fire (existing settlement path unaffected)
    let settle_count = settlements.lock().unwrap().len();
    assert_eq!(
        settle_count, 1,
        "settlement_cb must fire exactly once on success"
    );
}

/// Test: reserve_cb returning Err causes handle_submit() to return InsufficientCommonsCredits.
///
/// Models a TOCTOU race where the balance was consumed between the advisory check and the
/// atomic reserve. The task must be rejected.
#[tokio::test]
async fn test_reservation_race_rejection() {
    use icn_compute::{BalanceCallback, CommonsReserveCallback, ComputeError};

    let trust_cb: TrustCallback = Arc::new(|_| 1.0);
    let balance_cb: BalanceCallback = Arc::new(|_| 1_000_000); // advisory check passes
    let reserve_cb: CommonsReserveCallback =
        Arc::new(|_| Err("balance consumed by concurrent submission".into()));

    let mut actor = ComputeActor::new(RES_EXECUTOR_DID.into(), trust_cb);
    actor.set_balance_callback(balance_cb);
    actor.set_commons_reserve_callback(reserve_cb);
    let handle = actor.spawn();

    let result = handle
        .submit(make_reservation_task("res-race-1", "{}"))
        .await;

    assert!(
        matches!(result, Err(ComputeError::InsufficientCommonsCredits { .. })),
        "race rejection must return InsufficientCommonsCredits; got {result:?}"
    );
}

/// Test: actor without CommonsReserveCallback accepts Commons tasks without error.
///
/// This is ROLLOUT COMPATIBILITY MODE — advisory balance check only, not race-free.
/// Document the trade-off: tasks without a reserve callback can still race to exhaust credits.
#[tokio::test]
async fn test_no_reserve_callback_is_backward_compatible() {
    use icn_compute::{BalanceCallback, ComputeError};

    let trust_cb: TrustCallback = Arc::new(|_| 1.0);
    let balance_cb: BalanceCallback = Arc::new(|_| 1_000_000);

    // No reserve callback configured — rollout compatibility mode
    let mut actor = ComputeActor::new(RES_EXECUTOR_DID.into(), trust_cb);
    actor.set_balance_callback(balance_cb);
    let handle = actor.spawn();

    let result = handle
        .submit(make_reservation_task("res-compat-1", "{}"))
        .await;

    assert!(
        !matches!(result, Err(ComputeError::InsufficientCommonsCredits { .. })),
        "actor without reserve_cb must NOT reject with InsufficientCommonsCredits; got {result:?}"
    );
    // NOTE: This is advisory-only mode. The task is accepted, but no atomic hold is created.
    // Configure CommonsReserveCallback for race-free correctness.
}

/// Test: reservation is released when task times out via check_timeouts() sweep.
///
/// Uses the test-only `trigger_timeout_sweep` command to avoid a 10-second background
/// timer wait. The task is submitted with a near-future deadline; after sleeping past
/// the deadline, the manual sweep marks the task failed and fires release_cb.
#[tokio::test]
async fn test_reservation_released_on_timeout() {
    use icn_compute::{BalanceCallback, CommonsReleaseCallback, CommonsReserveCallback};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    let releases: Arc<Mutex<Vec<icn_compute::CommonsReleaseRequest>>> =
        Arc::new(Mutex::new(vec![]));
    let releases_cb = Arc::clone(&releases);

    let trust_cb: TrustCallback = Arc::new(|_| 1.0);
    let balance_cb: BalanceCallback = Arc::new(|_| 1_000_000);
    let reserve_cb: CommonsReserveCallback = Arc::new(|_| Ok(()));
    let release_cb: CommonsReleaseCallback =
        Arc::new(move |req| releases_cb.lock().unwrap().push(req));

    let mut actor = ComputeActor::new(RES_EXECUTOR_DID.into(), trust_cb);
    actor.set_balance_callback(balance_cb);
    actor.set_commons_reserve_callback(reserve_cb);
    actor.set_commons_release_callback(release_cb);
    let handle = actor.spawn();

    // Set a deadline 150ms in the future so validation passes but expires quickly.
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut task = make_reservation_task("res-timeout-1", "{}");
    task.deadline = Some(now_ms + 150);
    task.created_at = now_ms;

    // Phase 1: submit creates the reservation
    handle
        .submit(task)
        .await
        .expect("submit must succeed when deadline is in the future");

    // Wait for the deadline to pass
    tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;

    // Phase 2c: trigger timeout sweep — should find the expired task and fire release_cb
    handle
        .trigger_timeout_sweep()
        .await
        .expect("timeout sweep must not error");

    // Give the async release a moment to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let captured = releases.lock().unwrap();
    assert_eq!(
        captured.len(),
        1,
        "release_cb must fire exactly once on timeout; got {} calls",
        captured.len()
    );
    assert_eq!(
        captured[0].consumer, RES_SUBMITTER_DID,
        "release_cb consumer must match submitter"
    );
    assert_eq!(
        captured[0].task_id, "res-timeout-1",
        "release_cb task_id must match submitted task"
    );
    assert!(
        captured[0].reserved_amount > 0,
        "release_cb reserved_amount must be positive"
    );
}
