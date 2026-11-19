//! Characterization of compute result *acceptance* for registered multi-executor tasks.
//!
//! Test-first evidence for two engineering-journal findings: EX-018 (a registered task's refused
//! quorum result falls back to single-executor completion) and EX-003 (a federated result fires a
//! clearing notification before the quorum has accepted anything). Nothing here changes
//! production behaviour, and nothing here proposes a fix.
//!
//! # The boundary under test
//!
//! A task whose verification needs more than one executor is registered with the
//! `ResultQuorumManager` when it is submitted. `on_task_result` (`actor/lifecycle.rs`) hands each
//! signed result to that manager and, on `Ok`, completes the task only from the manager's canonical
//! result. On *any* `Err` from `submit_result` it falls through to the legacy single-confirmation
//! path, whose own comment says it "handles tasks not registered with quorum manager". Both "this
//! task is not registered" and "this registered task refused this result" are `Err`.
//! `on_federated_task_result` (`actor/placement.rs`) then fires the federation clearing callback
//! whenever `on_task_result` returned `Ok(())`, which includes "still collecting".
//!
//! # How to read this file
//!
//! * **Controls** run in normal CI and must pass. They prove the fixture reaches the branches the
//!   RED contracts depend on: an unregistered task takes the legacy path; a registered task is
//!   registered, keeps collecting after one result, and completes from two agreeing executors; the
//!   quorum manager refuses a repeated result from one executor; and the clearing callback is
//!   installed and fires on the path it was written for.
//! * **RED contracts** assert the invariant the component *should* hold and are `#[ignore]`d with a
//!   reason of the form `RED EX-0xx: …`, because current `main` violates them. Run them explicitly:
//!
//!   ```text
//!   cargo test -p icn-compute --test result_quorum_acceptance -- --ignored
//!   ```
//!
//!   A RED test that starts passing means the behaviour changed: re-classify it, do not delete it.
//!
//! # Fixture
//!
//! Everything goes through the public actor surface: `ComputeActor::spawn`, `ComputeHandle::submit`,
//! `ComputeHandle::handle_gossip` and `ComputeHandle::status`. The actor drains its command channel
//! one command at a time and awaits each gossip message before taking the next, so a `status`
//! request sent after `handle_gossip` observes that message's full effect. Nothing sleeps or depends
//! on scheduling. Keys come from fixed seeds. The collection window is ten minutes, so the actor's
//! periodic expiry sweep cannot retire an aggregator during a test.
//!
//! # Where each invariant comes from
//!
//! * EX-018: the component's own design. `TaskVerification::required_executors` is the number of
//!   executors a task's result needs, and the legacy path is documented as the path for tasks that
//!   are *not* registered. No domain owner in `ops/state/truth/sources.json` covers compute result
//!   acceptance yet; that ownership question is open and this file does not settle it.
//! * EX-003: `on_federated_task_result` fires clearing "on successful federated completion" (its own
//!   comment). A result the quorum is still collecting is not a completion.
//!
//! # Deliberately out of scope
//!
//! * Any production fix, an accepted-result type, the quorum rule, and the clearing callback itself.
//! * EX-017 (a failed quorum registration also permits fallback), EX-019 (a completed status can be
//!   overwritten), EX-016 (fuel is executor-signed, not quorum-established), EX-020 (federation
//!   envelope fields sit outside the signed result) and EX-015 (daemon gossip composition).
//! * The #2838 follow-ups (signing-payload framing, threshold arithmetic, sub-threshold
//!   accusations, dispute-id collisions).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ed25519_dalek::SigningKey;
use icn_compute::{
    ComputeActor, ComputeError, ComputeEvent, ComputeHandle, ComputeMessage, ComputeResult,
    ComputeTask, DeterminismClass, ExecutionOutcome, ExecutorCapability,
    FederationClearingCallback, FederationClearingNotification, FuelLimit, PrivacyClass,
    QuorumClock, QuorumStatus, ResultQuorumManager, TaskCode, TaskHash, TaskPriority, TaskStatus,
    TaskValue, TaskVerification, TrustCallback, VerificationConfig, CLOCK_SKEW_TOLERANCE_MS,
};
use icn_identity::Did;

// ── fixture ──────────────────────────────────────────────────────────────────────────────────

const TASK_ID: &str = "acceptance-task";
const SUBMITTER_COOP: &str = "alpha-coop";
const EXECUTOR_COOP: &str = "beta-coop";
/// Credits per 1_000 fuel units, the divisor `on_federated_task_result` uses.
const PAYMENT_RATE: u64 = 100;
const FUEL_USED: u64 = 5_000;
/// `PAYMENT_RATE * FUEL_USED / 1_000`.
const CLEARING_AMOUNT: u64 = 500;
const COLLECTION_WINDOW_MS: u64 = 600_000;

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

/// Two executors, two needed to agree. Passed explicitly so the test does not depend on the
/// value-classification thresholds.
fn two_executors() -> TaskVerification {
    TaskVerification {
        value: TaskValue::Medium,
        required_executors: 2,
        consensus_threshold: 0.67,
        assigned_executors: Vec::new(),
    }
}

/// `verification: None` with no `estimated_value` classifies as `TaskValue::Low`: one executor,
/// never registered with the quorum manager.
fn task(submitter: &Identity, verification: Option<TaskVerification>) -> ComputeTask {
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
        deadline: None,
        payment_rate: Some(PAYMENT_RATE),
        payment_currency: Some("credits".into()),
        resource_profile: None,
        actor_mode: None,
        placement_constraints: None,
        federation_constraints: None,
        estimated_value: None,
        verification,
        inputs_hash: None,
        policy_hash: None,
        determinism_class: DeterminismClass::default(),
        privacy_class: PrivacyClass::default(),
        storage_class: None,
        data_locality: None,
        scope: Default::default(),
    }
}

fn success() -> ExecutionOutcome {
    ExecutionOutcome::Success(b"42".to_vec())
}

/// A result signed by `executor`. The signature is checked here so that no test can pass or fail
/// because the actor rejected a malformed signature.
fn signed(executor: &Identity, task_hash: TaskHash, outcome: ExecutionOutcome) -> ComputeResult {
    let mut result = ComputeResult {
        task_hash,
        task_id: TASK_ID.into(),
        executor: executor.did.clone(),
        outcome,
        fuel_used: FUEL_USED,
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

/// A short, stable rendering of a task status. `Debug` would print signature bytes.
fn label(status: &Option<TaskStatus>) -> String {
    match status {
        None => "absent".into(),
        Some(TaskStatus::Pending) => "Pending".into(),
        Some(TaskStatus::Completed { result }) => format!(
            "Completed(executor={}, fuel={})",
            result.executor, result.fuel_used
        ),
        Some(other) => format!("{other:?}"),
    }
}

/// A spawned `ComputeActor`, a handle to its quorum manager, and recorders for the two callbacks
/// that announce acceptance: the `TaskCompleted` event and the federation clearing notification.
struct Node {
    handle: ComputeHandle,
    quorum: Arc<ResultQuorumManager>,
    completions: Arc<Mutex<Vec<String>>>,
    clearings: Arc<Mutex<Vec<FederationClearingNotification>>>,
}

fn node() -> Node {
    build_node(None)
}

/// A node whose quorum manager reads time from `clock`, so collection windows can be stepped
/// past deterministically.
fn clocked_node(clock: Arc<AtomicU64>) -> Node {
    let source: QuorumClock = Arc::new(move || clock.load(Ordering::SeqCst));
    build_node(Some(source))
}

fn build_node(clock: Option<QuorumClock>) -> Node {
    let trust: TrustCallback = Arc::new(|_did: &str| 1.0_f64);
    let mut actor = ComputeActor::new(identity(NODE_SEED).did, trust);
    let config = VerificationConfig {
        collection_window_ms: COLLECTION_WINDOW_MS,
        ..VerificationConfig::default()
    };
    match clock {
        None => actor.set_verification_config(config),
        Some(clock) => {
            actor.set_quorum_manager(Arc::new(ResultQuorumManager::with_clock(config, clock)))
        }
    }
    // Taken after `set_verification_config`, which replaces the manager.
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

impl Node {
    async fn submit(&self, task: ComputeTask) -> TaskHash {
        self.handle.submit(task).await.expect("submit")
    }

    /// Delivered the way a `compute:result` gossip entry is.
    async fn deliver(&self, result: &ComputeResult) {
        self.handle
            .handle_gossip(ComputeMessage::TaskResult(result.clone()))
            .await
            .expect("deliver");
    }

    async fn deliver_federated(&self, result: &ComputeResult) {
        self.handle
            .handle_gossip(ComputeMessage::FederatedTaskResult {
                result: result.clone(),
                executor_coop: EXECUTOR_COOP.into(),
                attestation_hash: [0xA7; 32],
            })
            .await
            .expect("deliver federated");
    }

    /// Also the barrier: the actor answers only after handling every earlier command.
    async fn status(&self, hash: TaskHash) -> Option<TaskStatus> {
        self.handle.status(hash).await.expect("status")
    }

    fn quorum_status(&self, hash: &TaskHash) -> Result<QuorumStatus, ComputeError> {
        self.quorum.get_status(hash)
    }

    fn completions(&self) -> Vec<String> {
        self.completions.lock().unwrap().clone()
    }

    fn clearings(&self) -> Vec<FederationClearingNotification> {
        self.clearings.lock().unwrap().clone()
    }
}

const COLLECTING_ONE_OF_TWO: QuorumStatus = QuorumStatus::Collecting {
    received: 1,
    required: 2,
};

fn is_duplicate_refusal(outcome: &Result<QuorumStatus, ComputeError>) -> bool {
    matches!(outcome, Err(ComputeError::InvalidResult(message))
        if message.starts_with("Duplicate result from executor"))
}

// ── controls ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn control_unregistered_task_completes_from_one_valid_result() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node.submit(task(&identity(SUBMITTER_SEED), None)).await;
    assert!(
        matches!(
            node.quorum_status(&hash),
            Err(ComputeError::TaskNotFound(_))
        ),
        "a single-executor task must not be registered with the quorum manager"
    );

    let result = signed(&a, hash, success());
    node.deliver(&result).await;

    assert_eq!(
        node.status(hash).await,
        Some(TaskStatus::Completed {
            result: Box::new(result)
        })
    );
    assert_eq!(node.completions(), vec![a.did]);
}

#[tokio::test]
async fn control_registered_task_keeps_collecting_after_its_first_result() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    assert_eq!(
        node.quorum_status(&hash).unwrap(),
        QuorumStatus::Collecting {
            received: 0,
            required: 2
        },
        "a two-executor task must be registered when it is submitted"
    );

    node.deliver(&signed(&a, hash, success())).await;

    assert_eq!(node.status(hash).await, Some(TaskStatus::Pending));
    assert_eq!(node.quorum_status(&hash).unwrap(), COLLECTING_ONE_OF_TWO);
    assert!(node.completions().is_empty());
}

#[tokio::test]
async fn control_registered_task_completes_from_two_agreeing_executors() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let b = identity(EXECUTOR_B_SEED);
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;

    let from_a = signed(&a, hash, success());
    let from_b = signed(&b, hash, success());
    node.deliver(&from_a).await;
    node.deliver(&from_b).await;

    // The quorum's canonical member is the agreeing result with the smallest executor DID.
    let canonical = if a.did < b.did { from_a } else { from_b };
    assert_eq!(
        node.status(hash).await,
        Some(TaskStatus::Completed {
            result: Box::new(canonical.clone())
        })
    );
    assert!(
        matches!(
            node.quorum_status(&hash),
            Err(ComputeError::TaskNotFound(_))
        ),
        "consensus must retire the task's aggregator"
    );
    assert_eq!(node.completions(), vec![canonical.executor]);
}

#[tokio::test]
async fn control_quorum_refuses_a_repeated_result_from_the_same_executor() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    let first = signed(&a, hash, success());
    node.deliver(&first).await;
    assert_eq!(node.status(hash).await, Some(TaskStatus::Pending));

    // The repeat is byte-identical and validly signed: the refusal is about the executor.
    let refusal = node.quorum.submit_result(first);
    assert!(
        is_duplicate_refusal(&refusal),
        "expected a duplicate-executor refusal, got {refusal:?}"
    );
    assert_eq!(
        node.quorum_status(&hash).unwrap(),
        COLLECTING_ONE_OF_TWO,
        "a refused result must not be counted"
    );
}

#[tokio::test]
async fn control_clearing_callback_fires_for_an_unregistered_federated_task() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let submitter = identity(SUBMITTER_SEED);
    let hash = node.submit(task(&submitter, None)).await;

    node.deliver_federated(&signed(&a, hash, success())).await;

    assert_eq!(
        label(&node.status(hash).await),
        format!("Completed(executor={}, fuel={FUEL_USED})", a.did)
    );
    let clearings = node.clearings();
    assert_eq!(
        clearings.len(),
        1,
        "the clearing callback must be installed"
    );
    assert_eq!(clearings[0].from_entity_id, SUBMITTER_COOP);
    assert_eq!(clearings[0].from_did, submitter.did);
    assert_eq!(clearings[0].to_entity_id, EXECUTOR_COOP);
    assert_eq!(clearings[0].to_did, a.did);
    assert_eq!(clearings[0].amount, CLEARING_AMOUNT);
}

// ── RED contracts ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "RED EX-018: a repeated result from the one executor that has reported completes a registered two-executor task through the single-executor fallback (observed Completed, expected Pending while the quorum is Collecting 1/2); run with --ignored"]
async fn ex018_refused_quorum_result_does_not_complete_a_registered_task() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    let first = signed(&a, hash, success());
    node.deliver(&first).await;

    // Preconditions. Each one rules out a way this test could fail for some other reason.
    assert_eq!(
        node.status(hash).await,
        Some(TaskStatus::Pending),
        "precondition: one result must not complete a two-executor task"
    );
    assert_eq!(
        node.quorum_status(&hash).unwrap(),
        COLLECTING_ONE_OF_TWO,
        "precondition: the task is registered and has counted one result"
    );
    let refusal = node.quorum.submit_result(first.clone());
    assert!(
        is_duplicate_refusal(&refusal),
        "precondition: the quorum refuses the repeat, got {refusal:?}"
    );

    // The repeat arrives the ordinary way, as a replayed `compute:result` entry would.
    node.deliver(&first).await;
    let status = node.status(hash).await;

    // The quorum has still accepted nothing.
    assert_eq!(node.quorum_status(&hash).unwrap(), COLLECTING_ONE_OF_TWO);
    assert_eq!(node.quorum.get_canonical_result(&hash).unwrap(), None);

    // Invariant: a registered task completes only from the quorum's canonical result.
    assert_eq!(
        label(&status),
        "Pending",
        "EX-018: the registered task was completed from one executor's raw result after the \
         quorum refused that result"
    );
    assert!(
        node.completions().is_empty(),
        "EX-018: TaskCompleted was broadcast for a result the quorum did not accept"
    );
}

#[tokio::test]
#[ignore = "RED EX-003: the first successful federated result of a registered two-executor task fires a clearing notification while the quorum is still collecting (observed 1 notification for 500 credits, expected none); run with --ignored"]
async fn ex003_no_clearing_notification_while_the_quorum_is_collecting() {
    let node = node();
    let a = identity(EXECUTOR_A_SEED);
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;

    node.deliver_federated(&signed(&a, hash, success())).await;

    // Preconditions: nothing has been accepted.
    assert_eq!(
        node.status(hash).await,
        Some(TaskStatus::Pending),
        "precondition: one result must not complete a two-executor task"
    );
    assert_eq!(
        node.quorum_status(&hash).unwrap(),
        COLLECTING_ONE_OF_TWO,
        "precondition: the task is registered and has counted one result"
    );
    assert!(
        node.completions().is_empty(),
        "precondition: no TaskCompleted"
    );

    // Invariant: no clearing obligation before the quorum has accepted a result.
    let clearings: Vec<String> = node
        .clearings()
        .iter()
        .map(|n| {
            format!(
                "{} -> {}: {} {}",
                n.from_entity_id, n.to_entity_id, n.amount, n.currency
            )
        })
        .collect();
    assert!(
        clearings.is_empty(),
        "EX-003: clearing notification emitted before the quorum accepted any result: \
         {clearings:?}"
    );
}

// ── C0 acceptance contracts (#2907 F-1, F-3) ─────────────────────────────────────────────────
//
// A task registered for quorum verification may complete only from its quorum's canonical
// result, and only once. Once its quorum has closed, accepted or not, no later result may
// complete it through the single-executor fallback, and none may add a completion or a clearing.

fn outcome(output: &[u8]) -> ExecutionOutcome {
    ExecutionOutcome::Success(output.to_vec())
}

/// #2907 F-1 (a). Two executors disagree, so the quorum closes divergent with no canonical
/// result. Replaying either result must not complete the task.
#[tokio::test]
async fn c0_divergent_quorum_then_replay_does_not_complete() {
    let node = node();
    let (a, b) = (identity(EXECUTOR_A_SEED), identity(EXECUTOR_B_SEED));
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    let from_a = signed(&a, hash, outcome(b"42"));
    let from_b = signed(&b, hash, outcome(b"43"));
    node.deliver(&from_a).await;
    node.deliver(&from_b).await;
    assert_eq!(
        label(&node.status(hash).await),
        "Pending",
        "precondition: a divergent quorum completes nothing"
    );
    assert!(node.completions().is_empty(), "precondition");

    for replay in [&from_a, &from_b] {
        node.deliver(replay).await;
        assert_eq!(
            label(&node.status(hash).await),
            "Pending",
            "C0: a replay after divergence completed the task through the fallback"
        );
    }
    assert!(
        node.completions().is_empty(),
        "C0: TaskCompleted after a divergent quorum: {:?}",
        node.completions()
    );
}

/// #2907 F-1 (a), federated delivery. Whatever the divergent submission itself emitted (that is
/// T01's), a replay after the quorum closed must add no clearing and no completion.
#[tokio::test]
async fn c0_replay_after_divergence_adds_no_clearing() {
    let node = node();
    let (a, b) = (identity(EXECUTOR_A_SEED), identity(EXECUTOR_B_SEED));
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    let from_a = signed(&a, hash, outcome(b"42"));
    let from_b = signed(&b, hash, outcome(b"43"));
    node.deliver_federated(&from_a).await;
    node.deliver_federated(&from_b).await;
    let _ = node.status(hash).await; // barrier
    let clearings_before = node.clearings().len();

    node.deliver_federated(&from_a).await;
    let status = node.status(hash).await;

    assert_eq!(
        label(&status),
        "Pending",
        "C0: a replayed federated result completed a divergent task"
    );
    assert_eq!(
        node.clearings().len(),
        clearings_before,
        "C0: a replay after the quorum closed emitted a clearing"
    );
    assert!(node.completions().is_empty());
}

/// #2907 F-1 (d). After consensus, a replay from either executor changes neither the completed
/// result nor the number of completions or clearings.
#[tokio::test]
async fn c0_replay_after_consensus_changes_nothing() {
    let node = node();
    let (a, b) = (identity(EXECUTOR_A_SEED), identity(EXECUTOR_B_SEED));
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    let from_a = signed(&a, hash, success());
    let from_b = signed(&b, hash, success());
    node.deliver_federated(&from_a).await;
    node.deliver_federated(&from_b).await;
    // The quorum's canonical member is the agreeing result with the smallest executor DID.
    let canonical = if a.did < b.did { &from_a } else { &from_b };
    let other = if a.did < b.did { &from_b } else { &from_a };
    let accepted = node.status(hash).await;
    assert_eq!(
        accepted,
        Some(TaskStatus::Completed {
            result: Box::new(canonical.clone())
        }),
        "precondition: consensus completes the task from the canonical result"
    );
    let completions = node.completions();
    let clearings = node.clearings().len();
    assert_eq!(completions.len(), 1, "precondition: one completion");

    // The non-canonical executor replays first, then the canonical one.
    for replay in [other, canonical] {
        node.deliver_federated(replay).await;
        node.deliver(replay).await;
    }

    assert_eq!(
        node.status(hash).await,
        accepted,
        "C0: a replay after consensus changed the completed result"
    );
    assert_eq!(
        node.completions(),
        completions,
        "C0: a replay after consensus broadcast another TaskCompleted"
    );
    assert_eq!(
        node.clearings().len(),
        clearings,
        "C0: a replay after consensus emitted another clearing"
    );
}

/// Cancellation closes the quorum. Results that arrive after the submitter cancelled must not
/// complete the task, even when they agree.
#[tokio::test]
async fn c0_cancelled_task_is_not_completed_by_later_agreeing_results() {
    let node = node();
    let submitter = identity(SUBMITTER_SEED);
    let (a, b) = (identity(EXECUTOR_A_SEED), identity(EXECUTOR_B_SEED));
    let hash = node.submit(task(&submitter, Some(two_executors()))).await;
    node.handle
        .cancel_task(&hash, &submitter.did, "changed my mind".into())
        .await
        .expect("cancel");
    assert!(
        label(&node.status(hash).await).starts_with("Cancelled"),
        "precondition: the submitter cancelled the task"
    );

    node.deliver(&signed(&a, hash, success())).await;
    node.deliver(&signed(&b, hash, success())).await;

    let status = label(&node.status(hash).await);
    assert!(
        status.starts_with("Cancelled"),
        "C0: agreeing results after cancellation completed the task: {status}"
    );
    assert!(
        node.completions().is_empty(),
        "C0: TaskCompleted after cancellation"
    );
}

/// A fixed start time for clocked tests; the deadline is `START + COLLECTION_WINDOW_MS`.
const START_MS: u64 = 1_800_000_000_000;
const DEADLINE_MS: u64 = START_MS + COLLECTION_WINDOW_MS;

/// #2907 F-1 (b). A result that arrives after the deadline but inside the clock-skew tolerance is
/// counted, and closes the quorum as timed out. A later agreeing result must not complete the
/// task through the fallback.
#[tokio::test]
async fn c0_timed_out_quorum_is_not_completed_by_a_late_result() {
    let clock = Arc::new(AtomicU64::new(START_MS));
    let node = clocked_node(Arc::clone(&clock));
    let (a, b) = (identity(EXECUTOR_A_SEED), identity(EXECUTOR_B_SEED));
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;

    clock.store(DEADLINE_MS + CLOCK_SKEW_TOLERANCE_MS / 2, Ordering::SeqCst);
    node.deliver(&signed(&a, hash, success())).await;
    assert_eq!(label(&node.status(hash).await), "Pending", "precondition");

    node.deliver(&signed(&b, hash, success())).await;
    assert_eq!(
        label(&node.status(hash).await),
        "Pending",
        "C0: a result after the quorum timed out completed the task"
    );
    assert!(
        node.completions().is_empty(),
        "C0: TaskCompleted after timeout"
    );
}

/// #2907 F-1 (c). The expiry sweep retires a collecting quorum. A result that arrives afterwards
/// must not complete the task. The sweep is invoked directly at a stepped clock: no waiting.
#[tokio::test]
async fn c0_expired_quorum_is_not_completed_by_a_late_result() {
    let clock = Arc::new(AtomicU64::new(START_MS));
    let node = clocked_node(Arc::clone(&clock));
    let (a, b) = (identity(EXECUTOR_A_SEED), identity(EXECUTOR_B_SEED));
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    node.deliver(&signed(&a, hash, success())).await;
    let _ = node.status(hash).await; // barrier: the actor has handled the result
    assert_eq!(
        node.quorum_status(&hash).unwrap(),
        COLLECTING_ONE_OF_TWO,
        "precondition"
    );

    clock.store(DEADLINE_MS + 1, Ordering::SeqCst);
    assert_eq!(
        node.quorum.cleanup_expired().unwrap(),
        vec![hash],
        "precondition: the sweep retires the quorum"
    );

    // Inside the skew tolerance, so only the sweep's retirement can refuse it.
    node.deliver(&signed(&b, hash, success())).await;
    assert_eq!(
        label(&node.status(hash).await),
        "Pending",
        "C0: a result after the sweep expired the quorum completed the task"
    );
    assert!(
        node.completions().is_empty(),
        "C0: TaskCompleted after expiry"
    );
}

/// #2907 F-3. A result after the collection window has closed is refused by the quorum
/// ("Result collection window has closed"); that refusal must not fall through to the fallback.
#[tokio::test]
async fn c0_result_after_the_collection_window_does_not_complete() {
    let clock = Arc::new(AtomicU64::new(START_MS));
    let node = clocked_node(Arc::clone(&clock));
    let (a, b) = (identity(EXECUTOR_A_SEED), identity(EXECUTOR_B_SEED));
    let hash = node
        .submit(task(&identity(SUBMITTER_SEED), Some(two_executors())))
        .await;
    node.deliver(&signed(&a, hash, success())).await;
    let _ = node.status(hash).await; // barrier: the actor has handled the result
    assert_eq!(
        node.quorum_status(&hash).unwrap(),
        COLLECTING_ONE_OF_TWO,
        "precondition"
    );

    clock.store(DEADLINE_MS + CLOCK_SKEW_TOLERANCE_MS + 1, Ordering::SeqCst);
    let late = signed(&b, hash, success());
    let refusal = node.quorum.submit_result(late.clone());
    assert!(
        matches!(&refusal, Err(ComputeError::InvalidResult(_))),
        "precondition: the quorum refuses a result after the window, got {refusal:?}"
    );

    node.deliver(&late).await;
    assert_eq!(
        label(&node.status(hash).await),
        "Pending",
        "C0: a result after the collection window completed the task"
    );
    assert!(
        node.completions().is_empty(),
        "C0: TaskCompleted after the window closed"
    );
}
