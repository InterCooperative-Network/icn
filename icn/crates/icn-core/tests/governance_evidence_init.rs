//! Supervisor-owned governance evidence initialization (M-C1 S1).
//!
//! On a node with an identity the daemon opens `<data_dir>/gateway_store`
//! once; the supervisor then establishes the governance evidence path over it
//! (E3 by-grantee backfill, E4 receipt store on the actor, E5 dispatch-evidence
//! backend, E6 close recovery, E7 #1987 backfill, E8 retention cleanup) and
//! settles readiness from that outcome, before any API accepts requests. The
//! gateway only receives the same store.
//!
//! These drive the real functions — `SharedGatewayStore::open` (the daemon's
//! open), `establish_governance_evidence` (the supervisor's step) and
//! `spawn_gateway` — over the production governance actor, a sled governance
//! state store, and the real receipt store. Journaled closes are produced by
//! failing a write in the real close path, then reopening both databases.
//!
//! Limits: reopens are flush-then-reopen, not crash durability; the lifecycle
//! ordering relative to the RPC server is pinned structurally (see the
//! `lifecycle.rs` unit test), not by running the daemon.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use icn_core::config::GatewayConfig;
use icn_core::events::EventBus;
use icn_core::supervisor::init_gateway::{
    spawn_gateway, GatewayHandles, GatewayStartup, GatewayStartupFailure,
};
use icn_core::supervisor::init_governance_evidence::{
    establish_governance_evidence, split_governance_store, EvidenceInitFailure,
    GovernanceEvidenceInit, GovernanceEvidenceInputs, GovernanceEvidenceSettled,
    GovernanceEvidenceStore, GovernanceStoreInjection,
};
use icn_gateway::receipt_store::ReceiptStore;
use icn_gateway::shared_store::SharedGatewayStore;
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, GovernanceDomainId, GovernanceError, GovernanceOps, GovernanceParams,
    Grantee, MembershipConfig, ProposalId, ProposalPayload, ProposalScope, ProposalState,
    StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_governance_actor::{
    DeferredDispatchEvidenceSink, EvidenceReadiness, GovernanceActor, GovernanceCommand,
    GovernanceHandle,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::effects::{EffectOutcome, EffectResult, KernelEffect, SdisEffect};
use icn_kernel_api::events::EventEmitter;
use icn_kernel_api::execution::{ExecutionRecord, EXECUTION_RECORD_KEY_PREFIX};
use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};

const DOMAIN: &str = "evidence-init-dom";
const PROPOSAL_PREFIX: &[u8] = b"gov:proposal:";
const INTENT_PREFIX: &[u8] = b"gov:close-intent:";
const BY_GRANTEE_PREFIX: &[u8] = b"adr0014:grant:by_grantee:";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Sled-backed `Store` with injectable failures:
/// - writes under an armed prefix fail;
/// - once a close intent is written, reads of close intents fail (so a close's
///   execution closure fails BEFORE it applies any lifecycle, leaving the
///   journal entry as a crash right after the intent write would);
/// - scans fail (an unreadable execution-record store).
struct FaultStore {
    inner: SledStore,
    fail_writes: Mutex<Option<Vec<u8>>>,
    fail_intent_reads_after_write: AtomicBool,
    intent_reads_failing: AtomicBool,
    fail_scans: AtomicBool,
    /// Set once a close intent is deleted — recovery clears an entry only
    /// after its close completed (including its execution closure).
    intent_deleted: AtomicBool,
}

impl FaultStore {
    fn new(inner: SledStore) -> Self {
        Self {
            inner,
            fail_writes: Mutex::new(None),
            fail_intent_reads_after_write: AtomicBool::new(false),
            intent_reads_failing: AtomicBool::new(false),
            fail_scans: AtomicBool::new(false),
            intent_deleted: AtomicBool::new(false),
        }
    }
    fn arm_writes(&self, prefix: &[u8]) {
        *self.fail_writes.lock().unwrap() = Some(prefix.to_vec());
    }
    fn write_fails(&self, key: &[u8]) -> bool {
        matches!(&*self.fail_writes.lock().unwrap(), Some(p) if key.starts_with(p))
    }
    fn after_write(&self, key: &[u8]) {
        if key.starts_with(INTENT_PREFIX)
            && self.fail_intent_reads_after_write.load(Ordering::SeqCst)
        {
            self.intent_reads_failing.store(true, Ordering::SeqCst);
        }
    }
}

impl Store for FaultStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        if key.starts_with(INTENT_PREFIX) && self.intent_reads_failing.load(Ordering::SeqCst) {
            anyhow::bail!("injected close-intent read failure");
        }
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        if self.write_fails(key) {
            anyhow::bail!("injected write failure for prefixed key");
        }
        self.inner.put(key, value)?;
        self.after_write(key);
        Ok(())
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)?;
        if key.starts_with(INTENT_PREFIX) {
            self.intent_deleted.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        self.inner.delete_atomic(keys)
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        if self.fail_scans.load(Ordering::SeqCst) {
            anyhow::bail!("injected scan failure");
        }
        self.inner.scan(prefix)
    }
    fn compare_and_put(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new: &[u8],
    ) -> anyhow::Result<icn_store::CompareAndPut> {
        if self.write_fails(key) {
            anyhow::bail!("injected write failure for prefixed key");
        }
        let r = self.inner.compare_and_put(key, expected, new)?;
        self.after_write(key);
        Ok(r)
    }
    fn get_replica_metadata(&self, h: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
        self.inner.get_replica_metadata(h)
    }
    fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
        self.inner.put_replica_metadata(m)
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
        self.inner.list_replica_hashes()
    }
    fn flush(&self) -> anyhow::Result<()> {
        self.inner.flush().map(|_| ())
    }
}

fn new_did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

fn now() -> u64 {
    icn_time::current_timestamp_secs()
}

struct Dirs {
    _tmp: tempfile::TempDir,
    state: PathBuf,
    /// The node data directory; the shared store is `<data>/gateway_store`.
    data: PathBuf,
}

fn dirs() -> Dirs {
    let tmp = tempfile::tempdir().unwrap();
    let state = tmp.path().join("state");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    Dirs {
        _tmp: tmp,
        state,
        data,
    }
}

/// The governance view of the shared store, built exactly as `icnd` builds it.
fn evidence_view(shared: &SharedGatewayStore) -> GovernanceEvidenceStore {
    let receipts = shared.receipt_store().clone();
    let backfill = receipts.clone();
    GovernanceEvidenceStore {
        receipt_backend: receipts as Arc<dyn GovernanceReceiptBackend>,
        backfill_by_grantee_index: Arc::new(move || backfill.backfill_grant_by_grantee_index()),
    }
}

/// A production governance actor over a sled state store.
struct Node {
    handle: GovernanceHandle,
    store: Arc<FaultStore>,
    _bus: Option<Arc<EventBus>>,
}

async fn spawn_node(state: &Path, bus: Option<Arc<EventBus>>) -> Node {
    let store = Arc::new(FaultStore::new(SledStore::open(state).unwrap()));
    let did = new_did();
    let gossip = GossipActor::spawn(did.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        did,
        store.clone() as Arc<dyn Store>,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        bus.clone().map(|b| b as Arc<dyn EventEmitter>),
        None,
    )
    .await
    .unwrap();
    Node {
        handle,
        store,
        _bus: bus,
    }
}

impl Node {
    fn state(&self) -> SledGovernanceStateStore {
        SledGovernanceStateStore::new(self.store.clone())
    }
    fn stored(&self, id: &ProposalId) -> ProposalState {
        self.state().get_proposal(id).unwrap().unwrap().state
    }
    fn intents(&self) -> usize {
        self.state().list_close_intents().unwrap().len()
    }
    async fn ensure_domain(&self, member: &Did) {
        let ops: &dyn GovernanceOps = &self.handle;
        let _ = ops
            .create_domain(
                GovernanceDomainId(DOMAIN.into()),
                "Evidence init".into(),
                "cooperative_default".into(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![member.clone()]),
            )
            .await;
    }
    async fn propose_and_vote(&self, member: &Did, payload: ProposalPayload) -> ProposalId {
        let ops: &dyn GovernanceOps = &self.handle;
        let id = ops
            .create_proposal(
                GovernanceDomainId(DOMAIN.into()),
                "p".into(),
                "d".into(),
                payload,
                ProposalScope::Local,
            )
            .await
            .unwrap();
        ops.open_proposal(id.clone(), 3600).await.unwrap();
        ops.cast_vote(id.clone(), member.clone(), VoteChoice::For, None)
            .await
            .unwrap();
        id
    }
    async fn close(&self, id: &ProposalId) -> anyhow::Result<()> {
        self.handle
            .submit(GovernanceCommand::CloseProposal {
                proposal_id: id.clone(),
                eligible_voters: None,
                excluded_delegators: None,
                capability_scope: None,
            })
            .await
    }
    async fn release(self) {
        self.store.flush().unwrap();
        self.handle.shutdown().await;
    }
}

fn text() -> ProposalPayload {
    ProposalPayload::Text { body: "b".into() }
}

fn appoint(candidate: &Did, sponsor: &Did) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::AppointSteward {
            candidate: candidate.clone(),
            sponsors: vec![sponsor.clone()],
            region: "region-evidence".into(),
            bond_amount: 1_000,
            term_length: 3600 * 24 * 30,
        },
    }
}

fn remove(steward: &Did) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::RemoveSteward {
            steward: steward.clone(),
            reason: "removed".into(),
            return_bond: false,
        },
    }
}

/// A first boot over `d` with evidence established, a domain with `member`,
/// returning the node and its shared store (still held).
async fn first_boot(d: &Dirs, member: &Did) -> (Node, SharedGatewayStore) {
    let node = spawn_node(&d.state, None).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    node.handle
        .install_receipt_store(shared.receipt_store().clone())
        .await;
    node.handle.recover_incomplete_closes().await;
    node.ensure_domain(member).await;
    (node, shared)
}

/// Close `id` with the terminal proposal save failing: the close is decided
/// and journaled (receipts and, for an executable payload, its execution
/// closure written) but not committed.
async fn journal_by_failed_terminal_save(node: &Node, id: &ProposalId) {
    node.store.arm_writes(PROPOSAL_PREFIX);
    node.close(id).await.expect_err("the terminal save fails");
    *node.store.fail_writes.lock().unwrap() = None;
    assert!(node.stored(id).is_open());
    assert_eq!(node.intents(), 1, "the decided close is journaled");
}

/// Release the first boot's actor and store so both databases can be reopened.
async fn shut_down(node: Node, shared: SharedGatewayStore) {
    GovernanceReceiptBackend::flush(shared.receipt_store().as_ref()).unwrap();
    shared.db().flush().unwrap();
    node.release().await;
    drop(shared);
}

fn inputs(
    node: &Node,
    store: Option<Result<GovernanceEvidenceStore, String>>,
    sink: Option<Arc<DeferredDispatchEvidenceSink>>,
    execution_store: Option<Arc<dyn Store>>,
    cleanup: Option<Box<dyn FnOnce() + Send>>,
) -> GovernanceEvidenceInputs {
    GovernanceEvidenceInputs {
        governance: node.handle.clone(),
        store,
        dispatch_evidence_sink: sink,
        execution_store,
        post_backfill_cleanup: cleanup,
    }
}

fn established(
    init: &GovernanceEvidenceInit,
) -> &icn_core::supervisor::init_governance_evidence::EstablishedEvidence {
    match init {
        GovernanceEvidenceInit::Established(e) => e,
        GovernanceEvidenceInit::Failed(cause) => panic!("evidence not established: {cause:?}"),
    }
}

fn gateway_handles(
    node: &Node,
    shared: Option<SharedGatewayStore>,
    settled: Option<GovernanceEvidenceSettled>,
) -> GatewayHandles {
    GatewayHandles {
        governance_actor_handle: Some(node.handle.clone()),
        event_broadcaster: Some(Arc::new(icn_gateway::EventBroadcaster::new())),
        shared_store: shared,
        governance_evidence: settled,
        ..GatewayHandles::default()
    }
}

fn enabled(bind: &str) -> GatewayConfig {
    GatewayConfig {
        enabled: true,
        jwt_secret: "s".repeat(32),
        bind_addr: bind.into(),
        ..GatewayConfig::default()
    }
}

/// The supervisor's sequence on an identity node: the daemon's single open,
/// evidence initialization, then the gateway with the same store.
async fn supervisor_start(
    node: &Node,
    d: &Dirs,
    gateway: &GatewayConfig,
) -> (GovernanceEvidenceInit, EvidenceReadiness, GatewayStartup) {
    let shared = SharedGatewayStore::open(&d.data);
    let (view, gw) = match &shared {
        Ok(s) => (Ok(evidence_view(s)), Some(s.clone())),
        Err(e) => (Err(e.clone()), None),
    };
    let (init, settled) =
        establish_governance_evidence(inputs(node, Some(view), None, None, None)).await;
    let readiness = settled.readiness();
    let startup = spawn_gateway(
        gateway,
        d.data.clone(),
        gateway_handles(node, gw, Some(settled)),
    )
    .await;
    (init, readiness, startup)
}

/// A journaled ordinary close was completed: committed, journal cleared.
fn assert_replayed(node: &Node, id: &ProposalId) {
    let s = node.stored(id);
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert_eq!(node.intents(), 0, "journal entry cleared");
}

/// A Text proposal journaled by a failed terminal save, both databases
/// released.
async fn journaled_text_close(d: &Dirs) -> ProposalId {
    let member = new_did();
    let (node, shared) = first_boot(d, &member).await;
    let id = node.propose_and_vote(&member, text()).await;
    journal_by_failed_terminal_save(&node, &id).await;
    shut_down(node, shared).await;
    id
}

// ---------------------------------------------------------------------------
// (f) readiness is keyed to evidence initialization, not to the gateway
// ---------------------------------------------------------------------------

/// Gateway disabled on an identity node: the journal is replayed and
/// governance is Ready. (Before S1 the gateway owned recovery, so a disabled
/// gateway left the journal unreplayed and readiness NotProvisioned.)
#[tokio::test]
async fn gateway_disabled_on_identity_node_replays_journal_and_is_ready() {
    let d = dirs();
    let id = journaled_text_close(&d).await;
    let node = spawn_node(&d.state, None).await;
    let (init, readiness, startup) = supervisor_start(&node, &d, &GatewayConfig::default()).await;
    assert_eq!(startup, GatewayStartup::Disabled);
    assert_eq!(readiness, EvidenceReadiness::Ready);
    assert_eq!(node.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let e = established(&init);
    assert_eq!(
        (e.close_recovery.scanned, e.close_recovery.completed),
        (1, 1)
    );
    assert_replayed(&node, &id);
    node.release().await;
}

/// Evidence initialization succeeds and the HTTP bind fails: governance stays
/// Ready; the gateway's failure does not reach governance.
#[tokio::test]
async fn http_bind_failure_after_evidence_init_leaves_governance_ready() {
    let d = dirs();
    let id = journaled_text_close(&d).await;
    let node = spawn_node(&d.state, None).await;
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = occupied.local_addr().unwrap().to_string();
    let (init, readiness, startup) = supervisor_start(&node, &d, &enabled(&addr)).await;
    assert!(
        matches!(
            startup,
            GatewayStartup::Failed(GatewayStartupFailure::ExitedBeforeReadiness(_))
        ),
        "{startup:?}"
    );
    established(&init);
    assert_eq!(readiness, EvidenceReadiness::Ready);
    assert_eq!(node.handle.evidence_readiness(), EvidenceReadiness::Ready);
    assert_replayed(&node, &id);
    node.release().await;
}

/// The store is locked by another holder: evidence init fails, governance is
/// held (Failed) and a close is refused with the typed evidence error; no
/// recovery runs.
#[tokio::test]
async fn store_locked_by_another_holder_fails_and_holds_governance() {
    let d = dirs();
    let id = journaled_text_close(&d).await;
    let holder = SharedGatewayStore::open(&d.data).unwrap();
    let node = spawn_node(&d.state, None).await;
    let (init, readiness, _startup) = supervisor_start(&node, &d, &GatewayConfig::default()).await;
    assert!(
        matches!(
            init,
            GovernanceEvidenceInit::Failed(EvidenceInitFailure::StoreOpen(_))
        ),
        "{init:?}"
    );
    assert_eq!(readiness, EvidenceReadiness::Failed);
    assert_eq!(node.handle.evidence_readiness(), EvidenceReadiness::Failed);
    assert!(node.stored(&id).is_open(), "no recovery on Failed");
    assert_eq!(node.intents(), 1, "the journal entry is retained");
    let err = node.close(&id).await.expect_err("close refused");
    assert!(
        matches!(
            err.downcast::<GovernanceError>(),
            Ok(GovernanceError::EvidenceUnavailable(_))
        ),
        "refusal must be the typed evidence error"
    );
    drop(holder);
    node.release().await;
}

/// No store injected while an actor is present: Failed(NotInjected), held.
#[tokio::test]
async fn no_injected_store_fails_and_holds_governance() {
    let d = dirs();
    let node = spawn_node(&d.state, None).await;
    let (init, readiness) =
        establish_governance_evidence(inputs(&node, None, None, None, None)).await;
    assert!(
        matches!(
            init,
            GovernanceEvidenceInit::Failed(EvidenceInitFailure::NotInjected)
        ),
        "{init:?}"
    );
    assert_eq!(readiness, EvidenceReadiness::Failed);
    node.release().await;
}

// ---------------------------------------------------------------------------
// (h) the gateway does not own evidence storage
// ---------------------------------------------------------------------------

/// Ingress ordering (runtime half): a gateway given a governance actor but no
/// proof that evidence initialization settled refuses to start, before
/// opening anything, and leaves readiness untouched (Pending).
#[tokio::test]
async fn gateway_with_actor_but_unsettled_evidence_refuses_to_start() {
    let d = dirs();
    let node = spawn_node(&d.state, None).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    let startup = spawn_gateway(
        &enabled("127.0.0.1:0"),
        d.data.clone(),
        gateway_handles(&node, Some(shared), None),
    )
    .await;
    assert_eq!(
        startup,
        GatewayStartup::Failed(GatewayStartupFailure::GovernanceEvidenceNotSettled)
    );
    assert_eq!(node.handle.evidence_readiness(), EvidenceReadiness::Pending);
    node.release().await;
}

/// A gateway given a governance actor (and a settled evidence init) but no
/// injected store refuses to start: no implicit or temporary store, and it
/// does not change governance readiness.
#[tokio::test]
async fn gateway_with_actor_but_no_injected_store_refuses_to_start() {
    let d = dirs();
    let node = spawn_node(&d.state, None).await;
    let (_init, settled) =
        establish_governance_evidence(inputs(&node, None, None, None, None)).await;
    assert_eq!(settled, EvidenceReadiness::Failed);
    let startup = spawn_gateway(
        &enabled("127.0.0.1:0"),
        d.data.clone(),
        gateway_handles(&node, None, Some(settled)),
    )
    .await;
    match &startup {
        GatewayStartup::Failed(GatewayStartupFailure::ExitedBeforeReadiness(Some(msg))) => {
            assert!(msg.contains("shared gateway store"), "{msg}");
        }
        other => panic!("must refuse to start: {other:?}"),
    }
    assert_eq!(node.handle.evidence_readiness(), EvidenceReadiness::Failed);
    assert!(
        !d.data.join("gateway_store").exists(),
        "the refusing gateway opened no store of its own"
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// Injection arms (the daemon's single open, split by the supervisor)
// ---------------------------------------------------------------------------

/// Ok arm: evidence init receives the backend (a journaled close whose first
/// attempt wrote no mandate is replayed, and the mandate recovery writes is
/// readable through the gateway's store) and the gateway receives the shared
/// store (it starts).
#[tokio::test]
async fn ok_injection_feeds_evidence_init_and_the_gateway_the_same_store() {
    let d = dirs();
    let (member, candidate) = (new_did(), new_did());
    let (node, shared) = first_boot(&d, &member).await;
    let id = node
        .propose_and_vote(&member, appoint(&candidate, &member))
        .await;
    // Journal it with the execution closure failing BEFORE any mandate is
    // written, so only recovery can write it (non-vacuity: assert absent).
    node.store
        .fail_intent_reads_after_write
        .store(true, Ordering::SeqCst);
    node.close(&id).await.expect_err("closure fails");
    assert_eq!(node.intents(), 1);
    assert!(
        shared
            .receipt_store()
            .get_mandate_by_proposal(&id.0)
            .unwrap()
            .is_none(),
        "boot 1 must leave no mandate; only recovery may write it"
    );
    shut_down(node, shared).await;

    let node = spawn_node(&d.state, None).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    let injection = GovernanceStoreInjection {
        evidence: evidence_view(&shared),
        gateway: shared,
    };
    let (evidence, gateway_store) = split_governance_store(Some(Ok(injection)));
    let gateway_store = gateway_store.expect("Ok arm: the gateway receives the shared store");
    let (init, settled) =
        establish_governance_evidence(inputs(&node, evidence, None, None, None)).await;
    assert_eq!(established(&init).close_recovery.completed, 1);
    assert_eq!(settled, EvidenceReadiness::Ready);
    assert_replayed(&node, &id);
    assert!(
        gateway_store
            .receipt_store()
            .get_mandate_by_proposal(&id.0)
            .unwrap()
            .is_some(),
        "recovery wrote through the backend into the gateway's store"
    );
    let startup = spawn_gateway(
        &enabled("127.0.0.1:0"),
        d.data.clone(),
        gateway_handles(&node, Some(gateway_store), Some(settled)),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Ready);
    node.release().await;
}

/// Err arm: the open failure reaches evidence init as StoreOpen (governance
/// Failed, held), the gateway receives no store and refuses; the split itself
/// never fails, so nothing stops the node.
#[tokio::test]
async fn err_injection_fails_governance_and_the_gateway_gets_no_store() {
    let d = dirs();
    let node = spawn_node(&d.state, None).await;
    let (evidence, gateway_store) = split_governance_store(Some(Err("locked".into())));
    assert!(
        gateway_store.is_none(),
        "Err arm: the gateway gets no store"
    );
    let (init, settled) =
        establish_governance_evidence(inputs(&node, evidence, None, None, None)).await;
    match &init {
        GovernanceEvidenceInit::Failed(EvidenceInitFailure::StoreOpen(cause)) => {
            assert_eq!(cause, "locked")
        }
        other => panic!("expected StoreOpen: {other:?}"),
    }
    assert_eq!(settled, EvidenceReadiness::Failed);
    let startup = spawn_gateway(
        &enabled("127.0.0.1:0"),
        d.data.clone(),
        gateway_handles(&node, gateway_store, Some(settled)),
    )
    .await;
    assert!(
        matches!(
            startup,
            GatewayStartup::Failed(GatewayStartupFailure::ExitedBeforeReadiness(_))
        ),
        "{startup:?}"
    );
    assert_eq!(node.handle.evidence_readiness(), EvidenceReadiness::Failed);
    node.release().await;
}

// ---------------------------------------------------------------------------
// (a) E3 before E6
// ---------------------------------------------------------------------------

/// A legacy grant with no by-grantee index entry, and a journaled accepted
/// RemoveSteward whose execution closure had not run: after evidence init the
/// grant is revoked. Recovery's lifecycle looks grants up by grantee, so the
/// by-grantee backfill must run first; a skip would be permanent (the mandate
/// is written and later retries short-circuit).
#[tokio::test]
async fn legacy_grant_is_revoked_by_journaled_remove_steward_after_init() {
    let d = dirs();
    let (member, steward) = (new_did(), new_did());
    let (node, shared) = first_boot(&d, &member).await;
    let appoint_id = node
        .propose_and_vote(&member, appoint(&steward, &member))
        .await;
    node.close(&appoint_id).await.unwrap();
    let grants = shared
        .receipt_store()
        .list_authority_grants_by_grantee(&Grantee::Person(steward.clone()))
        .unwrap();
    assert_eq!(grants.len(), 1, "{grants:?}");
    let grant_id = grants[0].id.clone();

    // RemoveSteward decided and journaled; its closure fails before applying
    // any lifecycle (as a crash right after the intent write would leave it).
    let remove_id = node.propose_and_vote(&member, remove(&steward)).await;
    node.store
        .fail_intent_reads_after_write
        .store(true, Ordering::SeqCst);
    node.close(&remove_id).await.expect_err("closure fails");
    assert_eq!(node.intents(), 1);
    assert!(node.stored(&remove_id).is_open());
    let receipts = shared.receipt_store().clone();
    assert!(
        receipts
            .get_authority_grant(&grant_id)
            .unwrap()
            .unwrap()
            .revoked_at
            .is_none(),
        "precondition: the removal's lifecycle has not run"
    );
    drop(receipts);
    shut_down(node, shared).await;

    // Make the grant legacy: drop its by-grantee index entries.
    {
        let db = sled::open(d.data.join("gateway_store")).unwrap();
        let keys: Vec<_> = db
            .scan_prefix(BY_GRANTEE_PREFIX)
            .map(|kv| kv.unwrap().0)
            .collect();
        assert!(!keys.is_empty());
        for k in keys {
            db.remove(k).unwrap();
        }
        db.flush().unwrap();
        let rs = ReceiptStore::new(db);
        assert!(rs
            .list_authority_grants_by_grantee(&Grantee::Person(steward.clone()))
            .unwrap()
            .is_empty());
    }

    let node = spawn_node(&d.state, None).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();

    // Ordering probe. The backfill is wrapped: before running the real
    // backfill it waits (bounded) for recovery to have COMPLETED the journaled
    // removal — observed as recovery clearing the close intent, which it does
    // only after the close's execution closure (and its by-grantee lookup) ran.
    // In the required order (E3 awaited before E6) recovery cannot start while
    // the backfill runs, so the wait always times out and the real backfill
    // then runs first. If the backfill instead overlaps or follows recovery,
    // the wait ends because recovery finished, so the real backfill runs
    // strictly after the lookup: the grant is not revoked (deterministic within
    // the 2 s bound: a mutant survives only if recovery stalls > 2 s and then
    // loses the race),
    // and the probe records it.
    let mut view = evidence_view(&shared);
    let real_backfill = view.backfill_by_grantee_index.clone();
    let probe_store = node.store.clone();
    let recovery_finished_before_backfill = Arc::new(AtomicBool::new(false));
    let probe_flag = recovery_finished_before_backfill.clone();
    view.backfill_by_grantee_index = Arc::new(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if probe_store.intent_deleted.load(Ordering::SeqCst) {
                probe_flag.store(true, Ordering::SeqCst);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        real_backfill()
    });
    let (init, readiness) =
        establish_governance_evidence(inputs(&node, Some(Ok(view)), None, None, None)).await;
    assert!(
        !recovery_finished_before_backfill.load(Ordering::SeqCst),
        "the by-grantee backfill must complete before recovery starts"
    );
    assert!(
        node.store.intent_deleted.load(Ordering::SeqCst),
        "recovery did complete and clear the journaled removal (probe signal is live)"
    );
    assert_eq!(readiness, EvidenceReadiness::Ready);
    let e = established(&init);
    assert_eq!(e.by_grantee_backfill, Ok(1));
    assert_eq!(e.close_recovery.completed, 1, "{:?}", e.close_recovery);
    assert!(matches!(
        node.stored(&remove_id),
        ProposalState::Accepted { .. }
    ));
    let grant = shared
        .receipt_store()
        .get_authority_grant(&grant_id)
        .unwrap()
        .unwrap();
    assert!(
        grant.revoked_at.is_some(),
        "the accepted removal revoked the legacy grant: {grant:?}"
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// (b) E5 before E6, (c) E4 before E6
// ---------------------------------------------------------------------------

fn ok_result() -> EffectResult {
    EffectResult {
        effect_id: "eff".into(),
        success: true,
        message: "applied".into(),
        state_change_hash: None,
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: None,
        outcome: Some(EffectOutcome::Applied),
    }
}

/// A journaled executable accept: its recovered `ProposalAccepted` reaches
/// the effect subscription during recovery, and the dispatch evidence it
/// produces is durable because the sink's backend was installed first.
#[tokio::test]
async fn journaled_executable_accept_produces_dispatch_evidence() {
    let d = dirs();
    let (member, candidate) = (new_did(), new_did());
    let (node, shared) = first_boot(&d, &member).await;
    let id = node
        .propose_and_vote(&member, appoint(&candidate, &member))
        .await;
    journal_by_failed_terminal_save(&node, &id).await;
    shut_down(node, shared).await;

    // The supervisor's effect subscription: accepted effects are dispatched
    // and reported through the deferred dispatch-evidence sink.
    let sink = Arc::new(DeferredDispatchEvidenceSink::new());
    let bus = Arc::new(EventBus::new());
    let sink_for_callback = sink.clone();
    let dispatched = Arc::new(AtomicBool::new(false));
    let dispatched_flag = dispatched.clone();
    let subscription = bus
        .subscribe(icn_governance_actor::create_effect_subscription(
            move |effects: Vec<KernelEffect>, receipt_id: String| {
                let results: Vec<_> = effects.iter().map(|_| ok_result()).collect();
                icn_kernel_api::effects::DispatchEvidenceSink::record_effects(
                    sink_for_callback.as_ref(),
                    &receipt_id,
                    &effects,
                    &results,
                    now(),
                );
                dispatched_flag.store(true, Ordering::SeqCst);
            },
        ))
        .await;

    let node = spawn_node(&d.state, Some(bus.clone())).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    let (init, _) = establish_governance_evidence(inputs(
        &node,
        Some(Ok(evidence_view(&shared))),
        Some(sink.clone()),
        None,
        None,
    ))
    .await;
    assert_eq!(established(&init).close_recovery.completed, 1);
    assert!(
        dispatched.load(Ordering::SeqCst),
        "recovery drove the effect"
    );
    let iers = shared
        .receipt_store()
        .list_institutional_effects_by_proposal(&id.0)
        .unwrap();
    assert_eq!(iers.len(), 1, "{iers:?}");
    let evidence = shared
        .receipt_store()
        .list_effect_dispatch_evidence_by_record(&iers[0].record_id)
        .unwrap();
    assert_eq!(
        evidence.len(),
        1,
        "dispatch evidence is durable: {evidence:?}"
    );
    drop(subscription);
    node.release().await;
}

/// A journal entry whose receipts were already written completes with no
/// residual: the receipt store is on the actor before recovery.
#[tokio::test]
async fn journal_entry_with_receipts_completes_without_residual() {
    let d = dirs();
    let (member, candidate) = (new_did(), new_did());
    let (node, shared) = first_boot(&d, &member).await;
    let id = node
        .propose_and_vote(&member, appoint(&candidate, &member))
        .await;
    journal_by_failed_terminal_save(&node, &id).await;
    shut_down(node, shared).await;
    {
        let shared = SharedGatewayStore::open(&d.data).unwrap();
        assert_eq!(
            shared
                .receipt_store()
                .list_governance_by_proposal(&id.0)
                .unwrap()
                .len(),
            1,
            "precondition: the decision receipt was written ahead of the failed save"
        );
    }
    let node = spawn_node(&d.state, None).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    let (init, readiness) = establish_governance_evidence(inputs(
        &node,
        Some(Ok(evidence_view(&shared))),
        None,
        None,
        None,
    ))
    .await;
    assert_eq!(readiness, EvidenceReadiness::Ready);
    let r = &established(&init).close_recovery;
    assert_eq!((r.scanned, r.completed), (1, 1), "{r:?}");
    assert!(r.residual.is_empty() && r.scan_error.is_none(), "{r:?}");
    assert_replayed(&node, &id);
    assert_eq!(
        shared
            .receipt_store()
            .list_governance_by_proposal(&id.0)
            .unwrap()
            .len(),
        1
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// (d) E5 before E7, (e) E8 only after a successful E7, never on Failed
// ---------------------------------------------------------------------------

fn approve_effect(proposal_id: &str) -> KernelEffect {
    KernelEffect::Sdis(SdisEffect::ApproveSteward {
        steward_did: "did:icn:zSteward".into(),
        jurisdiction_id: DOMAIN.into(),
        term_length_seconds: 3600,
        bond_amount: 100,
        region: Some("r".into()),
        proposal_id: proposal_id.into(),
        capabilities_hash: String::new(),
    })
}

/// A terminal execution record whose dispatch evidence was lost, with its
/// institutional effect record present in the receipt store.
fn plant_unhealed(shared: &SharedGatewayStore, exec: &Arc<dyn Store>, proposal_id: &str) -> String {
    let ier = icn_governance_actor::InstitutionalEffectRecord::new(
        proposal_id,
        DOMAIN,
        None,
        "appoint_steward",
        Some("did:icn:zSteward".into()),
        Some("r".into()),
        None,
        1,
        serde_json::json!({}),
    );
    shared
        .receipt_store()
        .put_institutional_effect(&ier)
        .unwrap();
    let mut record = ExecutionRecord::new_pending(
        "hash-unhealed",
        proposal_id,
        format!("gov:{DOMAIN}:{proposal_id}:receipt"),
        vec![approve_effect(proposal_id)],
    );
    record.set_results(vec![ok_result()]);
    record.mark_confirmed(vec![], vec![]);
    exec.put(
        format!("{EXECUTION_RECORD_KEY_PREFIX}hash-unhealed").as_bytes(),
        &serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    ier.record_id
}

fn evidence_count(shared: &SharedGatewayStore, record_id: &str) -> usize {
    shared
        .receipt_store()
        .list_effect_dispatch_evidence_by_record(record_id)
        .unwrap()
        .len()
}

/// Missing dispatch evidence is healed, and retention cleanup runs only after
/// the heal (it observes the evidence already written).
#[tokio::test]
async fn missing_evidence_is_healed_before_cleanup_runs() {
    let d = dirs();
    let node = spawn_node(&d.state, None).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    let exec_inner = Arc::new(FaultStore::new(SledStore::temporary().unwrap()));
    let exec: Arc<dyn Store> = exec_inner.clone();
    let record_id = plant_unhealed(&shared, &exec, "prop-unhealed");
    assert_eq!(evidence_count(&shared, &record_id), 0);

    let observed = Arc::new(Mutex::new(None));
    let observed_in_cleanup = observed.clone();
    let shared_in_cleanup = shared.clone();
    let rid = record_id.clone();
    let cleanup: Box<dyn FnOnce() + Send> = Box::new(move || {
        *observed_in_cleanup.lock().unwrap() = Some(evidence_count(&shared_in_cleanup, &rid));
    });
    let (init, _) = establish_governance_evidence(inputs(
        &node,
        Some(Ok(evidence_view(&shared))),
        Some(Arc::new(DeferredDispatchEvidenceSink::new())),
        Some(exec),
        Some(cleanup),
    ))
    .await;
    let e = established(&init);
    let backfill = e
        .dispatch_evidence_backfill
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(backfill.redriven, 1);
    assert_eq!(evidence_count(&shared, &record_id), 1, "evidence healed");
    assert!(e.retention_cleanup_ran);
    assert_eq!(
        *observed.lock().unwrap(),
        Some(1),
        "cleanup ran after the heal, never before it"
    );
    node.release().await;
}

/// The dispatch-evidence backfill fails: retention cleanup is skipped, so it
/// cannot prune a record that was not healed. Governance is still Ready.
#[tokio::test]
async fn cleanup_is_skipped_when_the_backfill_fails() {
    let d = dirs();
    let node = spawn_node(&d.state, None).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    let exec_inner = Arc::new(FaultStore::new(SledStore::temporary().unwrap()));
    let exec: Arc<dyn Store> = exec_inner.clone();
    let record_id = plant_unhealed(&shared, &exec, "prop-unreadable");
    exec_inner.fail_scans.store(true, Ordering::SeqCst);
    let ran = Arc::new(AtomicBool::new(false));
    let ran_flag = ran.clone();
    let (init, readiness) = establish_governance_evidence(inputs(
        &node,
        Some(Ok(evidence_view(&shared))),
        Some(Arc::new(DeferredDispatchEvidenceSink::new())),
        Some(exec),
        Some(Box::new(move || ran_flag.store(true, Ordering::SeqCst))),
    ))
    .await;
    let e = established(&init);
    assert!(
        matches!(e.dispatch_evidence_backfill, Some(Err(_))),
        "{e:?}"
    );
    assert!(!e.retention_cleanup_ran);
    assert!(!ran.load(Ordering::SeqCst), "cleanup must not run");
    assert_eq!(evidence_count(&shared, &record_id), 0);
    assert_eq!(readiness, EvidenceReadiness::Ready);
    node.release().await;
}

/// Evidence init fails: neither recovery nor retention cleanup runs.
#[tokio::test]
async fn cleanup_is_skipped_when_evidence_init_fails() {
    let d = dirs();
    let node = spawn_node(&d.state, None).await;
    let ran = Arc::new(AtomicBool::new(false));
    let ran_flag = ran.clone();
    let (init, readiness) = establish_governance_evidence(inputs(
        &node,
        Some(Err("locked".into())),
        Some(Arc::new(DeferredDispatchEvidenceSink::new())),
        Some(Arc::new(SledStore::temporary().unwrap())),
        Some(Box::new(move || ran_flag.store(true, Ordering::SeqCst))),
    ))
    .await;
    assert!(matches!(init, GovernanceEvidenceInit::Failed(_)));
    assert_eq!(readiness, EvidenceReadiness::Failed);
    assert!(
        !ran.load(Ordering::SeqCst),
        "cleanup must not run on Failed"
    );
    node.release().await;
}

// ---------------------------------------------------------------------------
// (g) residual surfaced, governance Ready
// ---------------------------------------------------------------------------

fn gauge(snapshotter: &metrics_util::debugging::Snapshotter, name: &str) -> Option<f64> {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .find_map(|(key, _, _, value)| match value {
            metrics_util::debugging::DebugValue::Gauge(v) if key.key().name() == name => {
                Some(v.into_inner())
            }
            _ => None,
        })
}

/// A journal entry contradicting its stored terminal proposal is not
/// completed: governance is Ready, the report carries one residual, and the
/// residual gauge says 1. (Current-thread runtime so the thread-local
/// recorder sees the metric.)
#[test]
fn conflicting_journal_entry_is_ready_with_one_residual_and_metric() {
    let recorder = metrics_util::debugging::DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    metrics::with_local_recorder(&recorder, || {
        rt.block_on(async {
            let d = dirs();
            let id = journaled_text_close(&d).await;
            // The stored proposal says Rejected; the journal says Accepted.
            {
                let state =
                    SledGovernanceStateStore::new(Arc::new(SledStore::open(&d.state).unwrap()));
                let mut p = state.get_proposal(&id).unwrap().unwrap();
                p.state = ProposalState::Rejected { closed_at: now() };
                state.save_proposal(&p).unwrap();
                state.flush().unwrap();
            }
            let node = spawn_node(&d.state, None).await;
            let shared = SharedGatewayStore::open(&d.data).unwrap();
            let (init, readiness) = establish_governance_evidence(inputs(
                &node,
                Some(Ok(evidence_view(&shared))),
                None,
                None,
                None,
            ))
            .await;
            assert_eq!(readiness, EvidenceReadiness::Ready);
            let r = &established(&init).close_recovery;
            assert_eq!(
                (r.scanned, r.completed, r.residual.len()),
                (1, 0, 1),
                "{r:?}"
            );
            assert_eq!(r.residual[0].proposal_id, id.0);
            assert_eq!(node.intents(), 1, "the contradicting entry is retained");
            node.release().await;
        });
    });
    assert_eq!(
        gauge(&snapshotter, "icn_governance_close_recovery_residual"),
        Some(1.0)
    );
    assert_eq!(
        gauge(&snapshotter, "icn_governance_close_recovery_scan_failed"),
        Some(0.0)
    );
}

/// The close journal cannot be scanned (maintainer policy): evidence is still
/// Established and governance Ready; the unknown residual is surfaced by the
/// report and the scan-failed metric, and the entry stays for the next boot.
#[test]
fn journal_scan_error_is_established_and_surfaced() {
    let recorder = metrics_util::debugging::DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    metrics::with_local_recorder(&recorder, || {
        rt.block_on(async {
            let d = dirs();
            let id = journaled_text_close(&d).await;
            let node = spawn_node(&d.state, None).await;
            let shared = SharedGatewayStore::open(&d.data).unwrap();
            node.store.fail_scans.store(true, Ordering::SeqCst);
            let (init, readiness) = establish_governance_evidence(inputs(
                &node,
                Some(Ok(evidence_view(&shared))),
                None,
                None,
                None,
            ))
            .await;
            node.store.fail_scans.store(false, Ordering::SeqCst);
            assert_eq!(readiness, EvidenceReadiness::Ready);
            let r = &established(&init).close_recovery;
            assert!(r.scan_error.is_some(), "{r:?}");
            assert_eq!((r.scanned, r.completed), (0, 0));
            assert!(node.stored(&id).is_open());
            assert_eq!(node.intents(), 1, "retained for the next startup");
            node.release().await;
        });
    });
    assert_eq!(
        gauge(&snapshotter, "icn_governance_close_recovery_scan_failed"),
        Some(1.0)
    );
}

/// E6 before E7: a recovered acceptance's execution reaches a terminal record
/// whose dispatch evidence was lost (the async evidence write never ran); the
/// backfill, running after recovery, heals it. A backfill that ran before
/// recovery would find no record to heal.
#[tokio::test]
async fn recovered_execution_lacking_evidence_is_healed_by_the_backfill() {
    let d = dirs();
    let (member, candidate) = (new_did(), new_did());
    let (node, shared) = first_boot(&d, &member).await;
    let id = node
        .propose_and_vote(&member, appoint(&candidate, &member))
        .await;
    journal_by_failed_terminal_save(&node, &id).await;
    shut_down(node, shared).await;

    // The executor persisted its terminal record with results, then the
    // evidence write was lost: the callback writes the record only.
    let exec: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
    let exec_for_callback = exec.clone();
    let bus = Arc::new(EventBus::new());
    let subscription = bus
        .subscribe(icn_governance_actor::create_effect_subscription(
            move |effects: Vec<KernelEffect>, receipt_id: String| {
                let mut record = ExecutionRecord::new_pending(
                    "hash-recovered",
                    "p",
                    &receipt_id,
                    effects.clone(),
                );
                record.set_results(effects.iter().map(|_| ok_result()).collect());
                record.mark_confirmed(vec![], vec![]);
                exec_for_callback
                    .put(
                        format!("{EXECUTION_RECORD_KEY_PREFIX}hash-recovered").as_bytes(),
                        &serde_json::to_vec(&record).unwrap(),
                    )
                    .unwrap();
            },
        ))
        .await;

    let node = spawn_node(&d.state, Some(bus.clone())).await;
    let shared = SharedGatewayStore::open(&d.data).unwrap();
    let (init, _) = establish_governance_evidence(inputs(
        &node,
        Some(Ok(evidence_view(&shared))),
        Some(Arc::new(DeferredDispatchEvidenceSink::new())),
        Some(exec),
        None,
    ))
    .await;
    let e = established(&init);
    assert_eq!(e.close_recovery.completed, 1);
    let iers = shared
        .receipt_store()
        .list_institutional_effects_by_proposal(&id.0)
        .unwrap();
    assert_eq!(iers.len(), 1, "{iers:?}");
    assert_eq!(
        evidence_count(&shared, &iers[0].record_id),
        1,
        "the recovered execution's lost evidence was healed: {e:?}"
    );
    drop(subscription);
    node.release().await;
}
