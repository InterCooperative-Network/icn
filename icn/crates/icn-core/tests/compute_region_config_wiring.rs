//! #2920: the operator-configured compute region reaches the `ComputeActor` through the
//! production configuration path.
//!
//! Since #2919 a legacy executor refuses a task whose placement constraints require a region it
//! is not configured in, and fails closed when it has no region at all. Before #2920 nothing in
//! the daemon ever configured a region, so every node refused every region-constrained task.
//!
//! Every node here is built from a daemon `Config` parsed from a TOML file (`Config::from_file`)
//! and accepted by `Config::validate`, exactly as `icnd` loads it, and the `[compute]` section is
//! applied by `configure_compute_actor`, the same function `init_compute_services` calls before
//! it spawns the actor. No test calls `ComputeActor::set_region`.
//!
//! The actors are spawned and driven only through `ComputeHandle`, as the daemon drives them.
//! Gossip is modelled by a hand-pumped FIFO loopback that delivers every message to every node,
//! the sender included. The actor handles gossip inline in its command loop, so a `status` query
//! sent after a message cannot be answered before that message has been processed: it is the
//! per-delivery barrier. No sleeps, and the pump is bounded.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use icn_compute::{
    ComputeActor, ComputeEvent, ComputeHandle, ComputeMessage, ComputeTask, CoopSchedulingPolicy,
    DeterminismClass, EnforcementMode, ExecutionOutcome, ExecutorCapability, FuelLimit,
    MemberQuota, PolicyManager, PrivacyClass, SchedulingRule, TaskCode, TaskHash, TaskPriority,
    TaskStatus, UsageTracker,
};
use icn_core::config::Config;
use icn_core::supervisor::init_compute::configure_compute_actor;

const COOP: &str = "region-coop";
const OPEN_COOP: &str = "open-coop";
const REGION: &str = "eu-central";
const OTHER_REGION: &str = "us-west";
const SUBMITTER_SEED: u8 = 0x51;
const EXECUTOR_SEED: u8 = 0x52;
const PUMP_BOUND: usize = 64;

/// The sections every daemon config file must carry; `extra` supplies the rest.
const REQUIRED_SECTIONS: &str = r#"
[network]
mdns_enabled = false
listen_addr = "127.0.0.1:0"
rpc_port = 0
bootstrap_peers = []

[observability]
metrics_port = 0
health_port = 0
log_level = "info"
"#;

/// A complete `[topology]` section naming `REGION` as the network-topology region.
const TOPOLOGY_EU_CENTRAL: &str = r#"
[topology]
region = "eu-central"
cluster_id = "fra-1"
role = "edge"

[topology.neighbor_limits]
max_local_cluster = 10
max_regional = 10
max_backbone = 5
max_trusted = 20

[topology.fanout]
local_cluster = 8
regional = 6
global = 4
"#;

/// Write a daemon config file (the required sections followed by `extra`), load it with
/// `Config::from_file` and require `Config::validate` to accept it.
fn daemon_config(extra: &str) -> Config {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("icn.toml");
    let toml_src = format!(
        "data_dir = {:?}\n{REQUIRED_SECTIONS}\n{extra}",
        dir.path().display().to_string()
    );
    std::fs::write(&path, toml_src).expect("write config");
    let config = Config::from_file(&path).expect("daemon config parses");
    if let Err(errors) = config.validate() {
        panic!("daemon config must validate, got {errors:?}");
    }
    config
}

struct Node {
    handle: ComputeHandle,
    own: String,
    outbox: Arc<Mutex<VecDeque<ComputeMessage>>>,
    completed: Arc<Mutex<HashMap<String, usize>>>,
    payments: Arc<Mutex<usize>>,
    commons: Arc<Mutex<usize>>,
}

impl Node {
    /// Returns once every command sent to this actor before it has been processed.
    async fn barrier(&self) {
        self.handle.status([0u8; 32]).await.expect("actor alive");
    }

    async fn status(&self, hash: TaskHash) -> Option<TaskStatus> {
        self.handle.status(hash).await.expect("actor alive")
    }

    fn effects(&self) -> (usize, usize) {
        (
            *self.payments.lock().expect("payments"),
            *self.commons.lock().expect("commons"),
        )
    }
}

/// A node configured from `config`, carrying `policies` in its policy manager, spawned.
///
/// `at_capacity` sets the node's concurrency to zero so it never executes: the submitter in
/// these tests. `compute.max_concurrent_tasks` is validated but not applied by the daemon (an
/// inert field, out of scope for #2920), so this one setting is made directly.
async fn node(
    seed: u8,
    config: &Config,
    policies: &[(&str, Vec<SchedulingRule>)],
    at_capacity: bool,
) -> Node {
    let key = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
    let own = icn_identity::Did::from_public_key(&key.verifying_key()).to_string();
    let mut actor = ComputeActor::new(own.clone(), Arc::new(|_: &str| 1.0));

    // The production wiring under test.
    configure_compute_actor(&mut actor, &config.compute).expect("validated [compute] applies");

    actor.set_signing_key(key.to_bytes().to_vec());
    if at_capacity {
        actor.set_max_concurrent_tasks(0);
    }

    let pm = Arc::new(PolicyManager::new(Arc::new(UsageTracker::new())));
    for (coop, rules) in policies {
        pm.set_policy(CoopSchedulingPolicy {
            coop_id: (*coop).into(),
            governance_domain: None,
            rules: rules.clone(),
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 100.0,
                max_concurrent_tasks: 10,
                max_priority: TaskPriority::Critical,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Strict,
        })
        .await;
    }
    actor.set_policy_manager(pm);

    let outbox: Arc<Mutex<VecDeque<ComputeMessage>>> = Arc::default();
    let sink = Arc::clone(&outbox);
    actor.set_send_callback(Arc::new(move |m: ComputeMessage| {
        sink.lock().expect("outbox").push_back(m);
    }));
    let completed: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
    let seen = Arc::clone(&completed);
    actor.set_event_callback(Arc::new(move |e: ComputeEvent| {
        if let ComputeEvent::TaskCompleted { task_hash, .. } = e {
            *seen.lock().expect("events").entry(task_hash).or_insert(0) += 1;
        }
    }));
    let payments: Arc<Mutex<usize>> = Arc::default();
    let p = Arc::clone(&payments);
    actor.set_payment_callback(Arc::new(move |_| {
        *p.lock().expect("payments") += 1;
    }));
    let commons: Arc<Mutex<usize>> = Arc::default();
    let c = Arc::clone(&commons);
    actor.set_commons_settlement_callback(Arc::new(move |_| {
        *c.lock().expect("commons") += 1;
    }));

    Node {
        handle: actor.spawn(),
        own,
        outbox,
        completed,
        payments,
        commons,
    }
}

/// Every node learns every node's executor entry (its own included), as capacity gossip
/// delivers it.
async fn introduce(nodes: &[&Node]) {
    for announced in nodes {
        for n in nodes {
            n.handle
                .handle_gossip(ComputeMessage::ExecutorAnnounce {
                    executor: announced.own.clone(),
                    capabilities: vec![ExecutorCapability::Ccl],
                })
                .await
                .expect("actor alive");
            n.barrier().await;
        }
    }
}

/// Deliver every queued message, FIFO across `nodes`, to every node (the sender included), until
/// all outboxes are empty. Returns every `(task_hash, executor)` claim seen.
async fn pump(nodes: &[&Node]) -> Vec<(TaskHash, String)> {
    let mut claims = Vec::new();
    let mut delivered = 0;
    loop {
        let next = nodes
            .iter()
            .find_map(|n| n.outbox.lock().expect("outbox").pop_front());
        let Some(msg) = next else { break };
        delivered += 1;
        assert!(delivered <= PUMP_BOUND, "loopback did not go quiet");
        if let ComputeMessage::TaskClaimed {
            task_hash,
            executor,
        } = &msg
        {
            claims.push((*task_hash, executor.clone()));
        }
        for n in nodes {
            n.handle
                .handle_gossip(msg.clone())
                .await
                .expect("actor alive");
            n.barrier().await;
        }
    }
    claims
}

/// A legacy (no resource profile) commons-scope task for `coop` with a payment rate and a
/// contract that executes successfully, so a run has observable payment and commons-settlement
/// effects on the executor.
fn task(id: &str, coop: &str) -> ComputeTask {
    let submitter = ed25519_dalek::SigningKey::from_bytes(&[0x5F; 32]);
    let submitter = icn_identity::Did::from_public_key(&submitter.verifying_key()).to_string();
    let ccl = format!(
        r#"{{
        "name": "RegionTask",
        "participants": ["{submitter}"],
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
    );
    ComputeTask {
        id: id.into(),
        submitter,
        coop_id: Some(coop.into()),
        code: TaskCode::Ccl(ccl),
        inputs: vec![],
        fuel_limit: FuelLimit::default(),
        required_capabilities: vec![ExecutorCapability::Ccl],
        priority: TaskPriority::Normal,
        created_at: 1000,
        deadline: None,
        payment_rate: Some(1_000_000),
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

fn sovereignty(region: &str) -> SchedulingRule {
    SchedulingRule::DataSovereignty {
        region: region.into(),
        tags: vec![],
    }
}

struct Run {
    claims: Vec<(TaskHash, String)>,
    hash: TaskHash,
    submitter: Node,
    executor: Node,
    coop: &'static str,
}

/// Submit one task for `coop` (whose policy is `rules`) from an at-capacity submitter with no
/// region to one executor configured from `executor_toml`, and pump until quiet.
async fn run_one(
    id: &str,
    coop: &'static str,
    rules: Vec<SchedulingRule>,
    executor_toml: &str,
) -> Run {
    let policies = [(coop, rules)];
    let submitter = node(SUBMITTER_SEED, &daemon_config(""), &policies, true).await;
    let executor = node(
        EXECUTOR_SEED,
        &daemon_config(executor_toml),
        &policies,
        false,
    )
    .await;
    introduce(&[&submitter, &executor]).await;
    let hash = submitter
        .handle
        .submit(task(id, coop))
        .await
        .expect("submission accepted");
    let claims = pump(&[&submitter, &executor]).await;
    Run {
        claims,
        hash,
        submitter,
        executor,
        coop,
    }
}

/// The executor claimed and executed the task exactly once, with its normal effects.
async fn assert_executed_once(what: &str, run: &Run) {
    let Run {
        claims,
        hash,
        submitter,
        executor,
        coop,
    } = run;
    assert_eq!(
        claims,
        &vec![(*hash, executor.own.clone())],
        "{what}: exactly one claim, by the executor"
    );
    for (name, n) in [("submitter", submitter), ("executor", executor)] {
        let status = n.status(*hash).await;
        assert!(
            matches!(&status, Some(TaskStatus::Completed { result })
                if result.executor == executor.own
                    && matches!(result.outcome, ExecutionOutcome::Success(_))),
            "{what}: {name} must hold the executor's successful result, got {status:?}"
        );
    }
    assert_eq!(
        submitter.completed.lock().expect("events").clone(),
        HashMap::from([(hex::encode(hash), 1)]),
        "{what}: exactly one TaskCompleted on the submitter"
    );
    // Positive control for the refusal assertions: these effects are live on this path.
    assert_eq!(
        executor.effects(),
        (1, 1),
        "{what}: (payments, commons settlements) on the executor"
    );
    assert_eq!(
        submitter.effects(),
        (0, 0),
        "{what}: the submitter executed nothing"
    );
    assert!(
        !executor
            .handle
            .list_coop_usage(coop)
            .await
            .expect("usage")
            .is_empty(),
        "{what}: the executor recorded policy usage for the task it ran"
    );
}

/// The executor refused the task: no claim, no execution, no completion, no payment, no commons
/// settlement and no policy usage, and the task is still Pending on both nodes.
async fn assert_refused(what: &str, run: &Run) {
    let Run {
        claims,
        hash,
        submitter,
        executor,
        coop,
    } = run;
    assert!(
        !claims.iter().any(|(h, _)| h == hash),
        "{what}: the task must not be claimed, claims = {claims:?}"
    );
    for (name, n) in [("submitter", submitter), ("executor", executor)] {
        let status = n.status(*hash).await;
        assert!(
            matches!(status, Some(TaskStatus::Pending)),
            "{what}: {name} must still hold the task as Pending, got {status:?}"
        );
        assert!(
            n.completed.lock().expect("events").is_empty(),
            "{what}: {name} saw a TaskCompleted"
        );
        assert_eq!(
            n.effects(),
            (0, 0),
            "{what}: (payments, commons settlements) on the {name}"
        );
        assert!(
            n.handle
                .list_coop_usage(coop)
                .await
                .expect("usage")
                .is_empty(),
            "{what}: the {name} recorded policy usage for a task nobody ran"
        );
    }
}

/// A daemon configured with the region the coop's policy requires executes the task.
#[tokio::test]
async fn issue2920_configured_matching_region_executes() {
    let run = run_one(
        "cfg-region-match",
        COOP,
        vec![sovereignty(REGION)],
        "[compute]\nregion = \"eu-central\"\n",
    )
    .await;
    assert_executed_once("configured matching region", &run).await;
}

/// A daemon configured with another region refuses the task.
#[tokio::test]
async fn issue2920_configured_other_region_refuses() {
    let run = run_one(
        "cfg-region-other",
        COOP,
        vec![sovereignty(REGION)],
        &format!("[compute]\nregion = \"{OTHER_REGION}\"\n"),
    )
    .await;
    assert_refused("configured other region", &run).await;
}

/// Region matching is exact: a case variant of the required region is a different region.
#[tokio::test]
async fn issue2920_configured_region_is_matched_exactly() {
    let run = run_one(
        "cfg-region-case",
        COOP,
        vec![sovereignty(REGION)],
        "[compute]\nregion = \"EU-Central\"\n",
    )
    .await;
    assert_refused("case-variant region", &run).await;
}

/// A daemon with no `compute.region` fails closed on a region-constrained task.
#[tokio::test]
async fn issue2920_absent_region_fails_closed() {
    let run = run_one(
        "cfg-region-absent",
        COOP,
        vec![sovereignty(REGION)],
        "[compute]\nmax_concurrent_tasks = 4\n",
    )
    .await;
    assert_refused("absent compute.region", &run).await;
}

/// `topology.region` is a network-topology label, not a compute region: naming the required
/// region there does not let a daemon with no `compute.region` execute the task.
#[tokio::test]
async fn issue2920_topology_region_never_supplies_compute_region() {
    let run = run_one(
        "cfg-region-topology",
        COOP,
        vec![sovereignty(REGION)],
        TOPOLOGY_EU_CENTRAL,
    )
    .await;
    assert_refused("topology.region only", &run).await;
}

/// An unconstrained task executes on a daemon with a configured region.
#[tokio::test]
async fn issue2920_unconstrained_task_executes_with_region() {
    let run = run_one(
        "cfg-open-with-region",
        OPEN_COOP,
        vec![],
        "[compute]\nregion = \"eu-central\"\n",
    )
    .await;
    assert_executed_once("unconstrained, region configured", &run).await;
}

/// An unconstrained task executes on a daemon with no configured region.
#[tokio::test]
async fn issue2920_unconstrained_task_executes_without_region() {
    let run = run_one("cfg-open-no-region", OPEN_COOP, vec![], "").await;
    assert_executed_once("unconstrained, no region", &run).await;
}
