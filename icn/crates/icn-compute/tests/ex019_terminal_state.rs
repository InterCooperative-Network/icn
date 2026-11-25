//! EX-019 acceptance contract: on the legacy single-executor path, a task's terminal state is
//! final and a late, replayed or conflicting result has no effect.
//!
//! The legacy path is the one every production task takes today: `icn-api` builds tasks with
//! `verification: None` and no `estimated_value`, so the task classifies Low, is never registered
//! with the quorum manager, and its results are handled by `accept_task_result`'s single-executor
//! branch. Before EX-019 that branch completed the task again for every later result,
//! overwriting the accepted result and resurrecting cancelled or failed tasks, and each repeat
//! emitted another `TaskCompleted` event and another federation clearing notification.
//!
//! Cases (named as in the EX-019 investigation):
//! - L5: positive control. The first legitimate result completes once and clears once.
//! - L1a/L1b/L1c: identical replay (same envelope, fresh envelope, plain `TaskResult`).
//! - L2: a different executor after completion.
//! - L3a/L3b: the same executor re-signs a different fuel figure / outcome.
//! - L4a/L4b: a result after cancellation / after a timeout failure.
//! - L6*: a duplicate submission is refused and cannot reopen the task or clear again.
//!
//! Each case asserts the authoritative status AND the completion events AND the clearing
//! notifications (count, amount, recipient).
//!
//! Barrier: `ComputeHandle::status` is answered by the actor only after every earlier command,
//! including the fire-and-forget `handle_gossip`, so no sleeps are used to order events. The
//! callbacks write into `std::sync::Mutex` sinks: every read takes one guard in its own
//! statement and never holds it across an `.await`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use ed25519_dalek::SigningKey;
use icn_compute::{
    BalanceCallback, CommonsReserveCallback, ComputeActor, ComputeError, ComputeEvent,
    ComputeHandle, ComputeMessage, ComputeResult, ComputeTask, DeterminismClass, ExecutionOutcome,
    ExecutorCapability, FederationClearingCallback, FederationClearingNotification, FuelLimit,
    PolicyManager, PrivacyClass, ResultQuorumManager, TaskCode, TaskHash, TaskPriority, TaskStatus,
    TaskValue, TaskVerification, TrustCallback, UsageTracker,
};
use icn_identity::Did;

const TASK_ID: &str = "ex019-task";
const SUBMITTER_COOP: &str = "alpha-coop";
const EXECUTOR_COOP: &str = "beta-coop";
const PAYMENT_RATE: u64 = 100;
const FUEL_USED: u64 = 5_000;
/// `PAYMENT_RATE * FUEL_USED / 1_000`.
const CLEARING_AMOUNT: u64 = 500;
const ATTESTATION: [u8; 32] = [0xA7; 32];

const NODE_SEED: u8 = 0x01;
const SUBMITTER_SEED: u8 = 0x02;
const EXECUTOR_A_SEED: u8 = 0x0A;
const EXECUTOR_B_SEED: u8 = 0x0B;

struct Identity {
    did: String,
    key: SigningKey,
}

fn identity(seed: u8) -> Identity {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let did = Did::from_public_key(&key.verifying_key()).to_string();
    Identity { did, key }
}

/// `verification: None`, no `estimated_value`: classifies Low, one executor, never registered.
/// This is the shape `icn-api` builds for every submitted task.
fn task(submitter: &Identity, deadline: Option<u64>) -> ComputeTask {
    ComputeTask {
        id: TASK_ID.into(),
        submitter: submitter.did.clone(),
        coop_id: Some(SUBMITTER_COOP.into()),
        code: TaskCode::Ccl(
            r#"{
                "name": "SimpleReturn",
                "participants": [],
                "currency": null,
                "state_vars": [],
                "rules": [{
                    "name": "run",
                    "params": [],
                    "requires": [],
                    "body": [{ "Return": { "value": { "Literal": { "Int": 42 } } } }]
                }],
                "triggers": []
            }"#
            .into(),
        ),
        inputs: vec![],
        fuel_limit: FuelLimit::default(),
        required_capabilities: vec![ExecutorCapability::Ccl],
        priority: TaskPriority::Normal,
        created_at: 1_000,
        deadline,
        payment_rate: Some(PAYMENT_RATE),
        payment_currency: Some("credits".into()),
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
        scope: Default::default(),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn success() -> ExecutionOutcome {
    ExecutionOutcome::Success(b"42".to_vec())
}

fn signed(
    executor: &Identity,
    task_hash: TaskHash,
    outcome: ExecutionOutcome,
    fuel_used: u64,
) -> ComputeResult {
    let mut result = ComputeResult {
        task_hash,
        task_id: TASK_ID.into(),
        executor: executor.did.clone(),
        outcome,
        fuel_used,
        duration_ms: 50,
        completed_at: 1_000,
        signature: Vec::new(),
    };
    result.sign(&executor.key);
    let did: Did = executor.did.parse().unwrap();
    result
        .verify_signature(&did)
        .expect("fixture result must carry a valid signature");
    result
}

/// The authoritative result a status holds: (executor, outcome is Success, fuel).
fn completed_by(status: &Option<TaskStatus>) -> Option<(String, bool, u64)> {
    match status {
        Some(TaskStatus::Completed { result }) => Some((
            result.executor.clone(),
            matches!(result.outcome, ExecutionOutcome::Success(_)),
            result.fuel_used,
        )),
        _ => None,
    }
}

struct Node {
    handle: ComputeHandle,
    quorum: Arc<ResultQuorumManager>,
    completions: Arc<Mutex<Vec<String>>>,
    clearings: Arc<Mutex<Vec<FederationClearingNotification>>>,
}

fn node_from(mut actor: ComputeActor) -> Node {
    let quorum = Arc::clone(actor.quorum_manager());

    let completions = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&completions);
    actor.set_event_callback(Arc::new(move |event| {
        if let ComputeEvent::TaskCompleted { executor, .. } = event {
            sink.lock().unwrap().push(executor);
        }
    }));

    let clearings = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&clearings);
    let clearing: FederationClearingCallback = Arc::new(move |notification| {
        sink.lock().unwrap().push(notification);
    });
    actor.set_federation_clearing_callback(clearing);

    Node {
        handle: actor.spawn(),
        quorum,
        completions,
        clearings,
    }
}

fn node() -> Node {
    let trust: TrustCallback = Arc::new(|_did: &str| 1.0_f64);
    node_from(ComputeActor::new(identity(NODE_SEED).did, trust))
}

impl Node {
    async fn submit(&self, task: ComputeTask) -> TaskHash {
        let hash = self.handle.submit(task).await.expect("submit");
        assert!(
            matches!(
                self.quorum.get_status(&hash),
                Err(ComputeError::TaskNotFound(_))
            ),
            "fixture: a verification:None task must not be registered with the quorum manager"
        );
        hash
    }

    async fn deliver(&self, result: &ComputeResult) {
        self.handle
            .handle_gossip(ComputeMessage::TaskResult(result.clone()))
            .await
            .expect("deliver");
    }

    async fn deliver_federated(&self, result: &ComputeResult, attestation: [u8; 32]) {
        self.handle
            .handle_gossip(ComputeMessage::FederatedTaskResult {
                result: result.clone(),
                executor_coop: EXECUTOR_COOP.into(),
                attestation_hash: attestation,
            })
            .await
            .expect("deliver federated");
    }

    /// Barrier + observation.
    async fn status(&self, hash: TaskHash) -> Option<TaskStatus> {
        self.handle.status(hash).await.expect("status")
    }

    fn completions(&self) -> Vec<String> {
        let guard = self.completions.lock().unwrap();
        guard.clone()
    }

    /// (recipient DID, amount) of every clearing notification, in order.
    fn clearings(&self) -> Vec<(String, u64)> {
        let guard = self.clearings.lock().unwrap();
        guard.iter().map(|n| (n.to_did.clone(), n.amount)).collect()
    }

    /// Asserts the post-L5 state: completed once by `a`, cleared once to `a` for 500.
    fn assert_single_completion_by(&self, case: &str, status: &Option<TaskStatus>, a: &Identity) {
        assert_eq!(
            completed_by(status),
            Some((a.did.clone(), true, FUEL_USED)),
            "{case}: the authoritative result must be the first accepted one"
        );
        assert_eq!(
            self.completions(),
            vec![a.did.clone()],
            "{case}: one task, one TaskCompleted event"
        );
        assert_eq!(
            self.clearings(),
            vec![(a.did.clone(), CLEARING_AMOUNT)],
            "{case}: one task, one clearing, to the first executor, for the metered amount"
        );
    }
}

// ── L5 positive control ─────────────────────────────────────────────────────────────────────

/// A legitimate first successful federated result completes exactly once and fires exactly one
/// clearing of the intended amount to the intended executor and entities.
#[tokio::test]
async fn l5_control_first_success_completes_and_clears_once() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;

    node.deliver_federated(&signed(&a, hash, success(), FUEL_USED), ATTESTATION)
        .await;
    let status = node.status(hash).await;

    node.assert_single_completion_by("L5", &status, &a);
    let notifications = {
        let guard = node.clearings.lock().unwrap();
        guard.clone()
    };
    assert_eq!(notifications[0].from_entity_id, SUBMITTER_COOP);
    assert_eq!(notifications[0].to_entity_id, EXECUTOR_COOP);
    assert_eq!(notifications[0].attestation_hash, ATTESTATION);
}

// ── L1 identical replay ─────────────────────────────────────────────────────────────────────

/// Byte-identical federated result delivered twice to the actor. (At the gossip layer an
/// identical payload on the same topic is content-deduplicated while retained; this measures
/// the actor when it is delivered anyway.)
#[tokio::test]
async fn l1a_identical_federated_replay_completes_and_clears_once() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;
    let r = signed(&a, hash, success(), FUEL_USED);

    node.deliver_federated(&r, ATTESTATION).await;
    node.status(hash).await;
    node.deliver_federated(&r, ATTESTATION).await;
    let status = node.status(hash).await;

    node.assert_single_completion_by("L1a", &status, &a);
}

/// Same signed result; the envelope differs only in its unsigned `attestation_hash`, so gossip
/// content-dedup does not suppress it.
#[tokio::test]
async fn l1b_same_signed_result_fresh_envelope_clears_once() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;
    let r = signed(&a, hash, success(), FUEL_USED);

    node.deliver_federated(&r, ATTESTATION).await;
    node.status(hash).await;
    node.deliver_federated(&r, [0x5E; 32]).await;
    let status = node.status(hash).await;

    node.assert_single_completion_by("L1b", &status, &a);
}

/// Plain `TaskResult` (the `compute:result` gossip shape) replayed. There is no clearing path
/// here; this isolates repeated completion and repeated `TaskCompleted` events.
#[tokio::test]
async fn l1c_identical_plain_result_replay_completes_once() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;
    let r = signed(&a, hash, success(), FUEL_USED);

    node.deliver(&r).await;
    node.status(hash).await;
    node.deliver(&r).await;
    let status = node.status(hash).await;

    assert_eq!(
        completed_by(&status),
        Some((a.did.clone(), true, FUEL_USED))
    );
    assert_eq!(
        node.completions(),
        vec![a.did.clone()],
        "one task, one TaskCompleted event"
    );
    assert!(
        node.clearings().is_empty(),
        "a plain result has no clearing path"
    );
}

// ── L2 different executor after completion ──────────────────────────────────────────────────

#[tokio::test]
async fn l2_second_executor_after_completion_changes_nothing() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let b = identity(EXECUTOR_B_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;

    node.deliver_federated(&signed(&a, hash, success(), FUEL_USED), ATTESTATION)
        .await;
    node.status(hash).await;
    node.deliver_federated(&signed(&b, hash, success(), FUEL_USED), [0xB2; 32])
        .await;
    let status = node.status(hash).await;

    node.assert_single_completion_by("L2", &status, &a);
}

// ── L3 conflicting replay ───────────────────────────────────────────────────────────────────

/// The same executor re-signs a larger fuel figure: neither the stored metering nor the owed
/// amount may change.
#[tokio::test]
async fn l3a_conflicting_fuel_replay_changes_nothing() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;

    node.deliver_federated(&signed(&a, hash, success(), FUEL_USED), ATTESTATION)
        .await;
    node.status(hash).await;
    node.deliver_federated(&signed(&a, hash, success(), 90_000), [0xC3; 32])
        .await;
    let status = node.status(hash).await;

    node.assert_single_completion_by("L3a", &status, &a);
}

/// The same executor re-signs a Failed outcome after Success.
#[tokio::test]
async fn l3b_conflicting_outcome_replay_changes_nothing() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;

    node.deliver_federated(&signed(&a, hash, success(), FUEL_USED), ATTESTATION)
        .await;
    node.status(hash).await;
    node.deliver_federated(
        &signed(&a, hash, ExecutionOutcome::Failed("late".into()), FUEL_USED),
        [0xC4; 32],
    )
    .await;
    let status = node.status(hash).await;

    node.assert_single_completion_by("L3b", &status, &a);
}

// ── L4 terminal-state preservation ──────────────────────────────────────────────────────────

#[tokio::test]
async fn l4a_result_after_cancellation_neither_completes_nor_clears() {
    let node = node();
    let submitter = identity(SUBMITTER_SEED);
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&submitter, None)).await;

    node.handle
        .cancel_task(&hash, &submitter.did, "submitter cancelled".into())
        .await
        .expect("cancel");
    let cancelled = node.status(hash).await;
    assert!(matches!(cancelled, Some(TaskStatus::Cancelled { .. })));

    node.deliver_federated(&signed(&a, hash, success(), FUEL_USED), ATTESTATION)
        .await;
    let status = node.status(hash).await;

    assert_eq!(status, cancelled, "terminal Cancelled must hold, unchanged");
    assert!(
        node.completions().is_empty(),
        "no TaskCompleted for a cancelled task"
    );
    assert!(
        node.clearings().is_empty(),
        "no clearing for a cancelled task"
    );
}

#[tokio::test]
async fn l4b_result_after_timeout_failure_neither_completes_nor_clears() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    // `submit` refuses a deadline already passed, so set one 200 ms ahead, then wait until the
    // wall clock is past that exact instant before the sweep (a computed condition, not a guess).
    let deadline = now_ms() + 200;
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(deadline)))
        .await;
    while now_ms() <= deadline {
        let remaining = deadline + 1 - now_ms().min(deadline);
        tokio::time::sleep(std::time::Duration::from_millis(remaining)).await;
    }

    node.handle
        .trigger_timeout_sweep_for_test()
        .await
        .expect("sweep");
    let failed = node.status(hash).await;
    assert!(matches!(failed, Some(TaskStatus::Failed { .. })));

    node.deliver_federated(&signed(&a, hash, success(), FUEL_USED), ATTESTATION)
        .await;
    let status = node.status(hash).await;

    assert_eq!(status, failed, "terminal Failed must hold, unchanged");
    assert!(
        node.completions().is_empty(),
        "no TaskCompleted for a failed task"
    );
    assert!(node.clearings().is_empty(), "no clearing for a failed task");
}

/// Quorum path: the timeout sweep fails a registered task but does not close its quorum, so
/// agreeing results that arrive afterwards still reach `QuorumDecision::Accepted`. The failed
/// task must stay failed, with no completion event and no clearing.
///
/// The first result is delivered as a plain `TaskResult`: the quorum only counts it, and a
/// counted federated result fires a clearing today. That is EX-003 / T01 (pinned RED in
/// `result_quorum_acceptance.rs`), not this contract. The accepting result is federated, so the
/// clearing assertion measures the terminal-state refusal alone.
#[tokio::test]
async fn l4c_quorum_acceptance_after_timeout_failure_neither_completes_nor_clears() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let b = identity(EXECUTOR_B_SEED);
    let deadline = now_ms() + 200;
    let mut registered = task(&identity(SUBMITTER_SEED), Some(deadline));
    registered.verification = Some(TaskVerification {
        value: TaskValue::Medium,
        required_executors: 2,
        consensus_threshold: 0.67,
        assigned_executors: Vec::new(),
    });
    let hash = node.handle.submit(registered).await.expect("submit");
    assert!(
        node.quorum.get_status(&hash).is_ok(),
        "fixture: the task must be registered with the quorum manager"
    );
    while now_ms() <= deadline {
        let remaining = deadline + 1 - now_ms().min(deadline);
        tokio::time::sleep(std::time::Duration::from_millis(remaining)).await;
    }
    node.handle
        .trigger_timeout_sweep_for_test()
        .await
        .expect("sweep");
    let failed = node.status(hash).await;
    assert!(matches!(failed, Some(TaskStatus::Failed { .. })));

    node.deliver(&signed(&a, hash, success(), FUEL_USED)).await;
    node.deliver_federated(&signed(&b, hash, success(), FUEL_USED), [0xB4; 32])
        .await;
    let status = node.status(hash).await;

    assert_eq!(status, failed, "terminal Failed must hold, unchanged");
    assert!(
        node.completions().is_empty(),
        "no TaskCompleted for a failed task"
    );
    assert!(node.clearings().is_empty(), "no clearing for a failed task");
}

// ── L6 duplicate submission ─────────────────────────────────────────────────────────────────

/// A byte-identical resubmission of a completed task (a client retrying `POST` submit through
/// `icn-api`) is refused with a typed error, does not reopen the task, and a later
/// fresh-envelope result cannot complete or clear it again.
#[tokio::test]
async fn l6_identical_resubmit_after_completion_is_refused_and_cannot_clear_again() {
    let node = node();
    let submitter = identity(SUBMITTER_SEED);
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&submitter, None)).await;
    let r = signed(&a, hash, success(), FUEL_USED);

    node.deliver_federated(&r, ATTESTATION).await;
    let completed = node.status(hash).await;

    let resubmit = node.handle.submit(task(&submitter, None)).await;
    assert!(
        matches!(&resubmit, Err(ComputeError::DuplicateTask { task }) if *task == hex::encode(hash)),
        "a duplicate submission must be refused with DuplicateTask; got {resubmit:?}"
    );
    assert_eq!(
        node.status(hash).await,
        completed,
        "a refused resubmission must not reopen the task"
    );

    node.deliver_federated(&r, [0x6E; 32]).await;
    let status = node.status(hash).await;

    node.assert_single_completion_by("L6", &status, &a);
}

/// A duplicate submission of a still-open task is refused too, so it can neither reset a claim
/// nor repeat the submission's side effects.
#[tokio::test]
async fn l6b_identical_resubmit_while_pending_is_refused() {
    let node = node();
    let submitter = identity(SUBMITTER_SEED);
    let hash = node.submit(task(&submitter, None)).await;

    let resubmit = node.handle.submit(task(&submitter, None)).await;

    assert!(
        matches!(&resubmit, Err(ComputeError::DuplicateTask { .. })),
        "got {resubmit:?}"
    );
    assert_eq!(node.status(hash).await, Some(TaskStatus::Pending));
}

/// Production shape: the daemon wires a `PolicyManager`, so a submitted task is stored under the
/// hash of its policy-adjusted form, not the hash of the task the client sent. A Commons task
/// with a reservation callback must be refused BEFORE its credits are reserved a second time.
#[tokio::test]
async fn l6c_duplicate_commons_submission_with_policy_is_refused_before_reserving() {
    let reserves = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reserves);
    let reserve: CommonsReserveCallback = Arc::new(move |req| {
        sink.lock().unwrap().push(req.task_id);
        Ok(())
    });
    let balance: BalanceCallback = Arc::new(|_| 1_000_000);

    let trust: TrustCallback = Arc::new(|_did: &str| 1.0_f64);
    let mut actor = ComputeActor::new(identity(NODE_SEED).did, trust);
    actor.set_policy_manager(Arc::new(PolicyManager::new(Arc::new(UsageTracker::new()))));
    actor.set_balance_callback(balance);
    actor.set_commons_reserve_callback(reserve);
    let node = node_from(actor);

    let submitter = identity(SUBMITTER_SEED);
    let mut commons = task(&submitter, None);
    commons.scope = icn_kernel_api::ScopeLevel::Commons;

    let hash = node.submit(commons.clone()).await;
    eprintln!(
        "OBS L6c: stored hash {} raw task hash {} (policy adjustment changes the identity: {})",
        hex::encode(hash),
        hex::encode(commons.hash()),
        hash != commons.hash()
    );
    let reserved_once = {
        let guard = reserves.lock().unwrap();
        guard.clone()
    };
    assert_eq!(
        reserved_once,
        vec![TASK_ID.to_string()],
        "fixture: one reservation"
    );

    let resubmit = node.handle.submit(commons).await;

    assert!(
        matches!(&resubmit, Err(ComputeError::DuplicateTask { task }) if *task == hex::encode(hash)),
        "got {resubmit:?}"
    );
    let reserved = {
        let guard = reserves.lock().unwrap();
        guard.clone()
    };
    assert_eq!(
        reserved, reserved_once,
        "a refused duplicate must not reserve the submitter's credits again"
    );
    assert_eq!(node.status(hash).await, Some(TaskStatus::Pending));
}

/// A `TaskSubmitted` gossip message for a task this node already holds (a peer's re-broadcast,
/// or a replay) keeps the stored state: before EX-019 it reset the completed task to Pending,
/// and this node then claimed and executed it again, completing it a second time.
#[tokio::test]
async fn l6d_gossiped_submission_of_a_completed_task_is_ignored() {
    let node = node();
    let submitter = identity(SUBMITTER_SEED);
    let a = identity(EXECUTOR_A_SEED);
    let held = task(&submitter, None);
    let hash = node.submit(held.clone()).await;
    assert_eq!(
        hash,
        held.hash(),
        "fixture: no policy adjustment, so gossip carries the same hash"
    );

    node.deliver_federated(&signed(&a, hash, success(), FUEL_USED), ATTESTATION)
        .await;
    let completed = node.status(hash).await;

    node.handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(held)))
        .await
        .expect("deliver submission");
    let status = node.status(hash).await;

    assert_eq!(
        status, completed,
        "a re-delivered submission must not reopen the task"
    );
    node.assert_single_completion_by("L6d", &status, &a);
}
