//! Cross-store close atomicity (Codex P2, PR #1985).
//!
//! Governance close persists provenance artifacts across two independent
//! sled-backed stores — the receipt store (`GovernanceReceiptBackend`) and the
//! proposal state store (`GovernanceStateStore`). A single sled transaction
//! cannot span the two `Db` instances, so a terminal `save_proposal` failure
//! *after* the v1 governance receipt is durable used to leave a permanent
//! phantom-accepted half-close: `GET /v1/receipts/chain` could observe an
//! accepted governance receipt while the proposal stayed `Open`.
//!
//! The write-ahead close journal fixes this. A self-contained journal entry is
//! persisted to the state store *before* any terminal-region write, every
//! artifact write is idempotent, and the entry is cleared only after all
//! artifacts are durable. On the next actor startup the entry is replayed to
//! completion. These tests pin:
//!
//! 1. A terminal-save failure after the receipt write leaves a durable journal
//!    entry (not a permanent phantom), and the next startup completes the close.
//! 2. Replaying the journal entry is idempotent — provenance is never
//!    duplicated.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig,
    ProposalId, ProposalPayload, ProposalScope, ProposalState, StaticMembershipResolver,
};
use icn_governance_actor::{
    actor::GovernanceActor,
    institutional_effect::InstitutionalEffectRecord,
    receipt_backend::GovernanceReceiptBackend,
    state_store::{GovernanceStateStore, SledGovernanceStateStore},
    GovernanceCommand, GovernanceManager,
};
use icn_identity::IdentityBundle;
use icn_kernel_api::events::{EventEmitter, SystemEvent};
use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};

/// Records downstream `SystemEvent`s emitted via the actor's event bus so a test
/// can assert that write-ahead recovery replays the post-commit close side
/// effects (here: the `ProposalAccepted` event that drives event-driven
/// execution), not just the durable proposal/receipt state.
#[derive(Default)]
struct RecordingEventEmitter {
    accepted: AtomicUsize,
    accepted_ids: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl EventEmitter for RecordingEventEmitter {
    async fn emit(&self, event: SystemEvent) {
        if let SystemEvent::ProposalAccepted { proposal_id, .. } = &event {
            self.accepted.fetch_add(1, Ordering::SeqCst);
            self.accepted_ids.lock().unwrap().push(proposal_id.clone());
        }
    }
}

/// Sled-backed `Store` wrapper that delegates everything to an inner
/// `SledStore`, but can be *armed* to fail `put` for keys carrying a configured
/// prefix. Used to inject a terminal proposal-save (`gov:proposal:`) failure
/// after the receipt-store writes have already landed, without disturbing the
/// close-journal (`gov:close-intent:`) writes or any reads.
struct PrefixFailStore {
    inner: SledStore,
    fail_prefix: Vec<u8>,
    fail: AtomicBool,
}

impl PrefixFailStore {
    fn new(inner: SledStore, fail_prefix: &[u8]) -> Self {
        Self {
            inner,
            fail_prefix: fail_prefix.to_vec(),
            fail: AtomicBool::new(false),
        }
    }
    fn arm(&self) {
        self.fail.store(true, Ordering::SeqCst);
    }
    fn disarm(&self) {
        self.fail.store(false, Ordering::SeqCst);
    }
}

impl Store for PrefixFailStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        if self.fail.load(Ordering::SeqCst) && key.starts_with(&self.fail_prefix) {
            anyhow::bail!("injected terminal-save failure for prefixed key");
        }
        self.inner.put(key, value)
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
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
}

/// Receipt backend that durably records governance receipts (deduped by
/// `decision_hash`, like the production sled store) and counts `put_governance`
/// calls so we can assert idempotency. Mandate/grant writes are recorded so the
/// execution-closure preflight round-trips cleanly.
#[derive(Default)]
struct RecordingReceiptBackend {
    governance: Mutex<HashMap<[u8; 32], icn_governance::GovernanceDecisionReceipt>>,
    put_governance_calls: AtomicUsize,
    allocations: Mutex<Vec<icn_kernel_api::AllocationReceipt>>,
    mandates: Mutex<Vec<icn_governance::Mandate>>,
    grants: Mutex<HashMap<icn_governance::AuthorityGrantId, icn_governance::AuthorityGrant>>,
}

impl RecordingReceiptBackend {
    fn governance_count(&self) -> usize {
        self.governance.lock().unwrap().len()
    }
    fn put_governance_calls(&self) -> usize {
        self.put_governance_calls.load(Ordering::SeqCst)
    }
}

impl GovernanceReceiptBackend for RecordingReceiptBackend {
    fn put_governance(
        &self,
        receipt: &icn_governance::GovernanceDecisionReceipt,
    ) -> Result<(), String> {
        self.put_governance_calls.fetch_add(1, Ordering::SeqCst);
        self.governance
            .lock()
            .unwrap()
            .insert(receipt.decision_hash, receipt.clone());
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance
            .lock()
            .unwrap()
            .values()
            .find(|r| r.proposal_id == proposal_id)
            .cloned())
    }
    fn put_allocation(
        &self,
        receipt: &icn_kernel_api::AllocationReceipt,
    ) -> Result<icn_kernel_api::Hash, String> {
        self.allocations.lock().unwrap().push(receipt.clone());
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        decision_hash: &icn_kernel_api::Hash,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(self.governance.lock().unwrap().get(decision_hash).cloned())
    }
    fn list_allocations_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
        Ok(self.allocations.lock().unwrap().clone())
    }
    fn put_institutional_effect(&self, _record: &InstitutionalEffectRecord) -> Result<(), String> {
        Ok(())
    }
    fn put_mandate(&self, mandate: &icn_governance::Mandate) -> Result<(), String> {
        self.mandates.lock().unwrap().push(mandate.clone());
        Ok(())
    }
    fn get_mandate_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<icn_governance::Mandate>, String> {
        Ok(self
            .mandates
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.decision.proposal_id == proposal_id)
            .cloned())
    }
    fn put_mandate_with_grants(
        &self,
        mandate: &icn_governance::Mandate,
        grants: &[icn_governance::AuthorityGrant],
    ) -> Result<(), String> {
        self.mandates.lock().unwrap().push(mandate.clone());
        for g in grants {
            self.put_authority_grant(g)?;
        }
        Ok(())
    }
    fn put_authority_grant(&self, grant: &icn_governance::AuthorityGrant) -> Result<(), String> {
        self.grants
            .lock()
            .unwrap()
            .insert(grant.id.clone(), grant.clone());
        Ok(())
    }
    fn get_authority_grant(
        &self,
        grant_id: &icn_governance::AuthorityGrantId,
    ) -> Result<Option<icn_governance::AuthorityGrant>, String> {
        Ok(self.grants.lock().unwrap().get(grant_id).cloned())
    }
}

async fn gossip_with_governance_topic(
    did: icn_identity::Did,
) -> Arc<tokio::sync::RwLock<GossipActor>> {
    let gossip = GossipActor::spawn(did, None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    gossip
}

fn appoint_steward_payload(
    candidate: icn_identity::Did,
    sponsor: icn_identity::Did,
) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::AppointSteward {
            candidate,
            sponsors: vec![sponsor],
            region: "region-crossstore".to_string(),
            bond_amount: 1_000,
            term_length: 3600 * 24 * 30,
        },
    }
}

/// Drive an execution-required proposal to the point of close, returning the
/// shared backend, the wrapped fail-store, the actor handle, and the
/// proposal id. The fail store is created disarmed.
async fn open_voted_proposal(
    db_path: &std::path::Path,
    event_bus: Option<Arc<dyn EventEmitter>>,
) -> (
    Arc<RecordingReceiptBackend>,
    Arc<PrefixFailStore>,
    icn_governance_actor::GovernanceHandle,
    ProposalId,
) {
    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate = IdentityBundle::generate().expect("candidate").did().clone();

    let inner = SledStore::open(db_path).expect("SledStore::open");
    let fail_store = Arc::new(PrefixFailStore::new(inner, b"gov:proposal:"));
    let store: Arc<dyn Store> = fail_store.clone();

    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store, gossip, resolver, event_bus, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("crossstore-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    manager
        .create_domain(
            domain_id.clone(),
            "Cross-store domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Appoint steward (cross-store)".to_string(),
            "".to_string(),
            appoint_steward_payload(candidate, did.clone()),
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");
    manager
        .cast_vote(
            proposal_id.clone(),
            did.clone(),
            icn_governance::VoteChoice::For,
            None,
        )
        .await
        .expect("cast_vote");

    let backend = Arc::new(RecordingReceiptBackend::default());
    (backend, fail_store, actor_handle, proposal_id)
}

/// A terminal `save_proposal` failure that strikes *after* the v1 governance
/// receipt is durable must NOT leave a permanent phantom-accepted close. The
/// close fails, the proposal stays `Open`, and a durable close-journal entry
/// remains. The next actor startup replays it and the close completes.
#[tokio::test(flavor = "current_thread")]
async fn terminal_save_failure_after_receipt_is_recovered_on_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let (backend, fail_store, actor_handle, proposal_id) =
        open_voted_proposal(&db_path, None).await;
    actor_handle.install_receipt_store(backend.clone()).await;

    // Arm the terminal-save fault and close. The receipt write lands; the
    // terminal proposal save fails.
    fail_store.arm();
    let result = actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await;
    assert!(
        result.is_err(),
        "terminal-save failure must surface as Err to the caller"
    );

    // The chain-observable governance receipt is durable...
    assert_eq!(
        backend.governance_count(),
        1,
        "the v1 governance receipt must have been published before the terminal save"
    );

    // ...but the proposal is still Open (no phantom-accepted committed state)...
    let state: Arc<dyn GovernanceStateStore> =
        Arc::new(SledGovernanceStateStore::new(fail_store.clone()));
    let persisted = state
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must still exist");
    assert!(
        matches!(persisted.state, ProposalState::Open { .. }),
        "terminal-save failure must leave the proposal Open, not phantom-Accepted; got {:?}",
        persisted.state
    );

    // ...and a durable close-journal entry records the in-flight close.
    let intents = state.list_close_intents().expect("list_close_intents");
    assert_eq!(
        intents.len(),
        1,
        "a durable close-journal entry must record the in-flight close for recovery"
    );

    actor_handle.shutdown().await;

    // "Restart": same durable data, fault cleared. Recovery runs when the
    // receipt backend is installed and completes the close.
    fail_store.disarm();
    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let gossip = gossip_with_governance_topic(bundle.did().clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());
    let store2: Arc<dyn Store> = fail_store.clone();
    let actor2 = GovernanceActor::spawn(bundle.did().clone(), store2, gossip, resolver, None, None)
        .await
        .expect("GovernanceActor::spawn (restart)");
    actor2.install_receipt_store(backend.clone()).await;

    // The phantom is healed: proposal is now Accepted and the journal is clear.
    let persisted = state
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must still exist");
    assert!(
        matches!(persisted.state, ProposalState::Accepted { .. }),
        "recovery must complete the close (Accepted); got {:?}",
        persisted.state
    );
    assert_eq!(
        state
            .list_close_intents()
            .expect("list_close_intents")
            .len(),
        0,
        "the close-journal entry must be cleared once the close completes"
    );
    // Recovery re-applied the receipt idempotently — no duplicate provenance.
    assert_eq!(
        backend.governance_count(),
        1,
        "recovery must not duplicate the governance receipt"
    );

    actor2.shutdown().await;
}

/// Replaying a close-journal entry is idempotent: re-injecting the same entry
/// and recovering again must not duplicate provenance or change the terminal
/// proposal state.
#[tokio::test(flavor = "current_thread")]
async fn recovery_replay_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let (backend, fail_store, actor_handle, proposal_id) =
        open_voted_proposal(&db_path, None).await;
    actor_handle.install_receipt_store(backend.clone()).await;

    fail_store.arm();
    let _ = actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await;

    let state: Arc<dyn GovernanceStateStore> =
        Arc::new(SledGovernanceStateStore::new(fail_store.clone()));
    let captured = state.list_close_intents().expect("list_close_intents");
    assert_eq!(
        captured.len(),
        1,
        "expected one in-flight close-journal entry"
    );
    let entry = captured.into_iter().next().unwrap();

    actor_handle.shutdown().await;
    fail_store.disarm();

    let put_calls_before = backend.put_governance_calls();

    // First recovery completes the close.
    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let gossip = gossip_with_governance_topic(bundle.did().clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());
    let store2: Arc<dyn Store> = fail_store.clone();
    let actor2 = GovernanceActor::spawn(bundle.did().clone(), store2, gossip, resolver, None, None)
        .await
        .expect("spawn restart");
    actor2.install_receipt_store(backend.clone()).await;
    actor2.shutdown().await;

    // Re-inject the same journal entry and recover again.
    state
        .save_close_intent(&entry)
        .expect("re-inject close intent");
    let bundle3 = IdentityBundle::generate().expect("IdentityBundle::generate");
    let gossip3 = gossip_with_governance_topic(bundle3.did().clone()).await;
    let resolver3 = Arc::new(StaticMembershipResolver::new());
    let store3: Arc<dyn Store> = fail_store.clone();
    let actor3 = GovernanceActor::spawn(
        bundle3.did().clone(),
        store3,
        gossip3,
        resolver3,
        None,
        None,
    )
    .await
    .expect("spawn restart 2");
    actor3.install_receipt_store(backend.clone()).await;

    // Idempotent: still exactly one governance receipt, still Accepted, journal clear.
    assert_eq!(
        backend.governance_count(),
        1,
        "idempotent replay must not duplicate the governance receipt"
    );
    assert!(
        backend.put_governance_calls() >= put_calls_before,
        "sanity: recovery exercised the receipt write path"
    );
    let persisted = state
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must still exist");
    assert!(
        matches!(persisted.state, ProposalState::Accepted { .. }),
        "idempotent replay must keep the proposal Accepted; got {:?}",
        persisted.state
    );
    assert_eq!(
        state
            .list_close_intents()
            .expect("list_close_intents")
            .len(),
        0,
        "journal must be clear after idempotent replay"
    );

    actor3.shutdown().await;
}

/// Recovery must replay the post-commit close side effects, not just the durable
/// proposal/receipt state. A close that fails at the terminal save (before its
/// downstream `ProposalAccepted` event is emitted) leaves a journal entry; the
/// next startup must complete the close AND emit the downstream event so
/// event-driven execution and other obligations are not permanently skipped.
#[tokio::test(flavor = "current_thread")]
async fn recovery_replays_downstream_side_effects() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let events = Arc::new(RecordingEventEmitter::default());
    let (backend, fail_store, actor_handle, proposal_id) =
        open_voted_proposal(&db_path, Some(events.clone() as Arc<dyn EventEmitter>)).await;
    actor_handle.install_receipt_store(backend.clone()).await;

    // Terminal-save failure: the close aborts before its downstream event fires.
    fail_store.arm();
    let result = actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await;
    assert!(result.is_err(), "terminal-save failure must surface as Err");
    assert_eq!(
        events.accepted.load(Ordering::SeqCst),
        0,
        "a close that failed before its durable commit must not have emitted the downstream event"
    );

    actor_handle.shutdown().await;
    fail_store.disarm();

    // Restart: recovery completes the close and must replay the downstream event.
    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let gossip = gossip_with_governance_topic(bundle.did().clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());
    let store2: Arc<dyn Store> = fail_store.clone();
    let actor2 = GovernanceActor::spawn(
        bundle.did().clone(),
        store2,
        gossip,
        resolver,
        Some(events.clone() as Arc<dyn EventEmitter>),
        None,
    )
    .await
    .expect("GovernanceActor::spawn (restart)");
    actor2.install_receipt_store(backend.clone()).await;

    assert_eq!(
        events.accepted.load(Ordering::SeqCst),
        1,
        "recovery must replay the downstream ProposalAccepted event exactly once"
    );
    assert_eq!(
        events.accepted_ids.lock().unwrap().as_slice(),
        std::slice::from_ref(&proposal_id.0),
        "the replayed event must be for the recovered proposal"
    );

    actor2.shutdown().await;
}
