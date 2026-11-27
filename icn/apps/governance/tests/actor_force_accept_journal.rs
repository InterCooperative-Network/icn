//! Force-accept must not leave execution authority behind a decision that did
//! not become durable.
//!
//! `ForceCloseProposal { Accept }` of an execution-required proposal writes an
//! institutional-effect record and an ADR-0014 mandate (with any derived
//! grants, and any SDIS lifecycle revocations) into the receipt store. Those
//! writes are authority: the mandate gate resolves grants from them, and
//! lifecycle revocations end other grants. These tests pin that such
//! authority exists only behind a durable decision record — the write-ahead
//! close journal — and that the journal's recovery completes the decision
//! exactly once.
//!
//! Boundaries exercised, in the order the force-accept writes happen:
//!
//! 1. close-intent write fails: nothing is written at all;
//! 2. intent durable, the decision's application fails (terminal proposal
//!    save, or an execution-closure write): the intent survives and a retry or
//!    restart completes the same decision;
//! 3. everything durable but acknowledgment interrupted (flush failure): the
//!    intent survives and replay converges without duplicating authority.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, ForcedOutcome, GovernanceDomainId, GovernanceOps, GovernanceParams,
    MembershipConfig, ProposalId, ProposalOutcome, ProposalPayload, ProposalScope, ProposalState,
    StaticMembershipResolver,
};
use icn_governance_actor::{
    actor::GovernanceActor,
    institutional_effect::InstitutionalEffectRecord,
    receipt_backend::GovernanceReceiptBackend,
    state_store::{GovernanceStateStore, SledGovernanceStateStore},
    GovernanceCommand, GovernanceHandle, GovernanceManager,
};
use icn_identity::IdentityBundle;
use icn_kernel_api::events::{EventEmitter, SystemEvent};
use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Sled-backed `Store` that can be armed to fail writes to keys under one
/// prefix (`gov:proposal:` = terminal proposal save, `gov:close-intent:` = the
/// write-ahead intent).
struct PrefixFailStore {
    inner: SledStore,
    fail_prefix: Mutex<Option<Vec<u8>>>,
}

impl PrefixFailStore {
    fn new(inner: SledStore) -> Self {
        Self {
            inner,
            fail_prefix: Mutex::new(None),
        }
    }
    fn arm(&self, prefix: &[u8]) {
        *self.fail_prefix.lock().unwrap() = Some(prefix.to_vec());
    }
    fn disarm(&self) {
        *self.fail_prefix.lock().unwrap() = None;
    }
    fn fails(&self, key: &[u8]) -> bool {
        matches!(&*self.fail_prefix.lock().unwrap(), Some(p) if key.starts_with(p))
    }
}

impl Store for PrefixFailStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        if self.fails(key) {
            anyhow::bail!("injected write failure for prefixed key");
        }
        self.inner.put(key, value)
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        self.inner.delete_atomic(keys)
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
    }
    fn compare_and_put(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new: &[u8],
    ) -> anyhow::Result<icn_store::CompareAndPut> {
        if self.fails(key) {
            anyhow::bail!("injected write failure for prefixed key");
        }
        self.inner.compare_and_put(key, expected, new)
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

/// In-memory receipt store that keeps every authority artifact a force-accept
/// can write (institutional-effect records, mandates, grants) and counts the
/// writes, with switches to fail the effect write or the durability flush.
/// It outlives actor restarts, modelling the separate receipt `Db`.
#[derive(Default)]
struct AuthorityBackend {
    effects: Mutex<Vec<InstitutionalEffectRecord>>,
    effect_writes: AtomicUsize,
    fail_effect_write: AtomicBool,
    mandates: Mutex<Vec<icn_governance::Mandate>>,
    mandate_writes: AtomicUsize,
    grants: Mutex<HashMap<icn_governance::AuthorityGrantId, icn_governance::AuthorityGrant>>,
    fail_flush: AtomicBool,
    fail_mandate_write: AtomicBool,
}

impl AuthorityBackend {
    fn mandates(&self) -> Vec<icn_governance::Mandate> {
        self.mandates.lock().unwrap().clone()
    }
    fn effects(&self) -> Vec<InstitutionalEffectRecord> {
        self.effects.lock().unwrap().clone()
    }
    fn grant_count(&self) -> usize {
        self.grants.lock().unwrap().len()
    }
    fn mandate_writes(&self) -> usize {
        self.mandate_writes.load(Ordering::SeqCst)
    }
    fn effect_writes(&self) -> usize {
        self.effect_writes.load(Ordering::SeqCst)
    }
}

impl GovernanceReceiptBackend for AuthorityBackend {
    fn put_governance(&self, _: &icn_governance::GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    // Stores no governance receipts, so it holds none for any proposal.
    fn list_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Vec<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(vec![])
    }
    fn put_allocation(
        &self,
        _: &icn_kernel_api::AllocationReceipt,
    ) -> Result<icn_kernel_api::Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn flush(&self) -> Result<(), String> {
        if self.fail_flush.load(Ordering::SeqCst) {
            return Err("injected receipt-store flush failure".to_string());
        }
        Ok(())
    }
    fn put_institutional_effect(&self, record: &InstitutionalEffectRecord) -> Result<(), String> {
        if self.fail_effect_write.load(Ordering::SeqCst) {
            return Err("injected institutional-effect write failure".to_string());
        }
        self.effect_writes.fetch_add(1, Ordering::SeqCst);
        self.effects.lock().unwrap().push(record.clone());
        Ok(())
    }
    fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(self
            .effects
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.proposal_id == proposal_id)
            .cloned()
            .collect())
    }
    fn put_mandate(&self, mandate: &icn_governance::Mandate) -> Result<(), String> {
        if self.fail_mandate_write.load(Ordering::SeqCst) {
            return Err("injected mandate write failure".to_string());
        }
        self.mandate_writes.fetch_add(1, Ordering::SeqCst);
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
        if self.fail_mandate_write.load(Ordering::SeqCst) {
            return Err("injected mandate write failure".to_string());
        }
        self.mandate_writes.fetch_add(1, Ordering::SeqCst);
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

/// Records `ProposalAccepted` events (proposal id + governance decision hash).
#[derive(Default)]
struct AcceptedEvents {
    seen: Mutex<Vec<(String, Option<String>)>>,
}

#[async_trait::async_trait]
impl EventEmitter for AcceptedEvents {
    async fn emit(&self, event: SystemEvent) {
        if let SystemEvent::ProposalAccepted {
            proposal_id,
            governance_decision_hash,
            ..
        } = event
        {
            self.seen
                .lock()
                .unwrap()
                .push((proposal_id, governance_decision_hash));
        }
    }
}

async fn gossip(did: icn_identity::Did) -> Arc<tokio::sync::RwLock<GossipActor>> {
    let gossip = GossipActor::spawn(did, None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    gossip
}

/// An open, execution-required (SDIS steward appointment) proposal on an actor
/// whose state store can be fault-injected and whose receipt store is
/// `backend`.
struct Fixture {
    backend: Arc<AuthorityBackend>,
    fail_store: Arc<PrefixFailStore>,
    handle: GovernanceHandle,
    proposal_id: ProposalId,
    domain_id: GovernanceDomainId,
    /// The domain's sole member.
    member: icn_identity::Did,
}

/// The actor and state store after a real restart: every handle on the first
/// sled `Db` is dropped and the database is reopened from disk.
struct Restarted {
    handle: GovernanceHandle,
    state: Arc<dyn GovernanceStateStore>,
}

impl Fixture {
    async fn open(db_path: &std::path::Path) -> Self {
        let candidate = IdentityBundle::generate().unwrap().did().clone();
        Self::open_with(db_path, move |did| ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate,
                sponsors: vec![did.clone()],
                region: "region-force".to_string(),
                bond_amount: 1_000,
                term_length: 3600 * 24 * 30,
            },
        })
        .await
    }

    /// Like [`Self::open`], with the proposal payload built from the member DID.
    async fn open_with(
        db_path: &std::path::Path,
        payload: impl FnOnce(&icn_identity::Did) -> ProposalPayload,
    ) -> Self {
        let did = IdentityBundle::generate().unwrap().did().clone();
        let payload = payload(&did);
        let fail_store = Arc::new(PrefixFailStore::new(SledStore::open(db_path).unwrap()));
        let store: Arc<dyn Store> = fail_store.clone();
        let handle = GovernanceActor::spawn(
            did.clone(),
            store,
            gossip(did.clone()).await,
            Arc::new(StaticMembershipResolver::new()),
            None,
            None,
        )
        .await
        .unwrap();
        let domain_id = GovernanceDomainId("force-accept-journal-domain".to_string());
        let manager = GovernanceManager::with_handle(
            Arc::new(handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
        );
        manager
            .create_domain(
                domain_id.clone(),
                "Force-accept journal domain".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![did.clone()]),
            )
            .await
            .unwrap();
        let proposal_id = manager
            .create_proposal(
                ProposalId("_ignored".to_string()),
                domain_id.clone(),
                did.clone(),
                "Force-accept journal proposal".to_string(),
                String::new(),
                payload,
                ProposalScope::Local,
            )
            .await
            .unwrap();
        manager
            .open_proposal(proposal_id.clone(), 3600)
            .await
            .unwrap();
        let backend = Arc::new(AuthorityBackend::default());
        handle.install_receipt_store(backend.clone()).await;
        Self {
            backend,
            fail_store,
            handle,
            proposal_id,
            domain_id,
            member: did,
        }
    }

    fn state(&self) -> Arc<dyn GovernanceStateStore> {
        Arc::new(SledGovernanceStateStore::new(self.fail_store.clone()))
    }

    async fn force(&self, outcome: ForcedOutcome) -> anyhow::Result<()> {
        self.handle
            .submit(GovernanceCommand::ForceCloseProposal {
                proposal_id: self.proposal_id.clone(),
                forced_outcome: outcome,
                reason: "force-accept journal test".to_string(),
            })
            .await
    }

    /// Shut the actor down, release the sled `Db`, reopen it from `db_path`
    /// without fault injection, install the same receipt store and run startup
    /// recovery.
    async fn restart(
        self,
        db_path: &std::path::Path,
        events: Option<Arc<dyn EventEmitter>>,
    ) -> (Restarted, Arc<AuthorityBackend>) {
        let backend = self.backend.clone();
        self.release().await;
        let restarted = start(db_path, backend.clone(), events).await;
        (restarted, backend)
    }

    /// Shut the actor down and drop every handle on the state `Db`.
    async fn release(self) {
        let Fixture {
            fail_store, handle, ..
        } = self;
        handle.shutdown().await;
        drop(handle);
        drop(fail_store);
    }

    /// The raw stored bytes of this proposal's close intent.
    fn raw_intent(&self) -> Option<Vec<u8>> {
        self.fail_store
            .get(format!("gov:close-intent:{}", self.proposal_id.0).as_bytes())
            .unwrap()
    }

    fn stored_state(&self) -> ProposalState {
        self.state()
            .get_proposal(&self.proposal_id)
            .unwrap()
            .unwrap()
            .state
    }
}

/// Reopen the state `Db` at `db_path` (no fault injection), spawn an actor on
/// it with `receipts` installed and run startup recovery.
async fn start(
    db_path: &std::path::Path,
    receipts: Arc<dyn GovernanceReceiptBackend>,
    events: Option<Arc<dyn EventEmitter>>,
) -> Restarted {
    let store: Arc<dyn Store> = Arc::new(
        SledStore::open(db_path).expect("reopen sled: the first Db must have been released"),
    );
    let did = IdentityBundle::generate().unwrap().did().clone();
    let handle = GovernanceActor::spawn(
        did.clone(),
        store.clone(),
        gossip(did).await,
        Arc::new(StaticMembershipResolver::new()),
        events,
        None,
    )
    .await
    .unwrap();
    handle.install_receipt_store(receipts).await;
    handle.recover_incomplete_closes().await;
    Restarted {
        handle,
        state: Arc::new(SledGovernanceStateStore::new(store)),
    }
}

fn is_force_accepted(state: &ProposalState) -> bool {
    matches!(
        state,
        ProposalState::ForceClosed {
            outcome: ProposalOutcome::Accepted,
            ..
        }
    )
}

// ---------------------------------------------------------------------------
// Fail-first regressions
// ---------------------------------------------------------------------------

/// A force-accept whose terminal proposal save fails must not leave a mandate
/// that no durable decision record stands behind. Before the fix the mandate
/// (and its grant, and the institutional-effect record) were written in a
/// non-journaled preflight, so this left authority for a proposal that is
/// still Open, with nothing that would ever complete or reconcile it.
#[tokio::test(flavor = "current_thread")]
async fn failed_force_accept_save_leaves_no_authority_without_a_durable_decision() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.fail_store.arm(b"gov:proposal:");
    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("injected write failure"), "{err}");

    assert!(
        matches!(fx.stored_state(), ProposalState::Open { .. }),
        "the terminal save failed, so the proposal is still Open"
    );
    let intents = fx.state().list_close_intents().unwrap();
    let mandates = fx.backend.mandates();
    // Every mandate for this proposal must be backed by a durable decision
    // record that recovery will complete.
    assert_eq!(
        intents.len(),
        mandates.len(),
        "mandates written: {}, durable decision records: {} — authority without a decision",
        mandates.len(),
        intents.len()
    );
    assert_eq!(intents.len(), 1, "the decided force-accept is journaled");
    assert!(is_force_accepted(&intents[0].proposal.state));
    assert_eq!(intents[0].proposal.id, fx.proposal_id);

    fx.handle.shutdown().await;
}

/// `ForceCloseProposal { Accept }` on a proposal that is already closed (here:
/// force-rejected) is refused by `Proposal::force_close`. Before the fix the
/// execution-closure preflight ran first, so the refused command still wrote a
/// mandate, a grant and an institutional-effect record for a rejected proposal.
#[tokio::test(flavor = "current_thread")]
async fn refused_force_accept_of_a_rejected_proposal_writes_no_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.force(ForcedOutcome::Reject).await.unwrap();
    assert!(matches!(
        fx.stored_state(),
        ProposalState::ForceClosed {
            outcome: ProposalOutcome::Rejected,
            ..
        }
    ));

    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("Can only force close proposals in Deliberation or Open state"),
        "{err}"
    );
    assert_eq!(
        fx.backend.mandates().len(),
        0,
        "no mandate for a rejected proposal"
    );
    assert_eq!(
        fx.backend.grant_count(),
        0,
        "no grant for a rejected proposal"
    );
    assert_eq!(
        fx.backend.effects().len(),
        0,
        "no effect record for a rejected proposal"
    );
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 0);
    assert!(matches!(
        fx.stored_state(),
        ProposalState::ForceClosed {
            outcome: ProposalOutcome::Rejected,
            ..
        }
    ));

    fx.handle.shutdown().await;
}

/// If the write-ahead intent itself cannot be written, the force-accept fails
/// and nothing — no mandate, grant, effect record or terminal state — is
/// written.
#[tokio::test(flavor = "current_thread")]
async fn force_accept_intent_write_failure_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.fail_store.arm(b"gov:close-intent:");
    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("injected write failure"), "{err}");

    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(fx.backend.mandates().len(), 0);
    assert_eq!(fx.backend.grant_count(), 0);
    assert_eq!(fx.backend.effects().len(), 0);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 0);

    fx.handle.shutdown().await;
}

// ---------------------------------------------------------------------------
// Completion: retry, restart, replay, interrupted acknowledgment
// ---------------------------------------------------------------------------

/// The forced decision hash: fixed by (proposal, domain, Accepted, empty tally,
/// no votes), so every completion must carry exactly this value.
fn forced_decision_hash(fx: &Fixture) -> String {
    let receipt = icn_governance::GovernanceDecisionReceipt::new(
        fx.proposal_id.0.clone(),
        fx.domain_id.0.clone(),
        icn_governance::proof::ProofOutcome::Accepted,
        icn_governance::VoteTally::empty(),
        &[],
    );
    hex::encode(receipt.decision_hash)
}

/// Fail the terminal save, then restart from disk: recovery completes the same
/// decision with exactly one mandate (bound to the forced decision hash), one
/// effect record, the forced `ProposalAccepted` event, and a cleared journal.
#[tokio::test(flavor = "current_thread")]
async fn failed_force_accept_is_completed_on_restart_with_one_mandate() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let expected_hash = forced_decision_hash(&fx);
    let proposal_id = fx.proposal_id.clone();

    fx.fail_store.arm(b"gov:proposal:");
    fx.force(ForcedOutcome::Accept).await.unwrap_err();
    let intent = fx.state().list_close_intents().unwrap().remove(0);
    assert!(
        intent.execution_closure,
        "the force-accept intent carries its closure"
    );
    assert_eq!(
        intent.governance_decision_hash.as_deref(),
        Some(expected_hash.as_str())
    );
    let mandate_before = fx.backend.mandates();
    assert_eq!(mandate_before.len(), 1);

    let events = Arc::new(AcceptedEvents::default());
    let (r, backend) = fx.restart(tmp.path(), Some(events.clone())).await;

    let stored = r.state.get_proposal(&proposal_id).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&stored).unwrap(),
        serde_json::to_value(&intent.proposal).unwrap(),
        "recovery commits exactly the journaled terminal proposal"
    );
    assert!(is_force_accepted(&stored.state));
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    let mandates = backend.mandates();
    assert_eq!(mandates.len(), 1, "exactly one mandate (ADR-0019)");
    assert_eq!(
        mandates[0].id, mandate_before[0].id,
        "the same mandate, not a second one"
    );
    assert_eq!(
        hex::encode(mandates[0].decision.decision_hash),
        expected_hash
    );
    assert_eq!(backend.mandate_writes(), 1);
    assert_eq!(backend.effects().len(), 1);
    assert_eq!(backend.effect_writes(), 1);
    assert_eq!(
        events.seen.lock().unwrap().clone(),
        vec![(proposal_id.0.clone(), Some(expected_hash))],
        "recovery emits the forced ProposalAccepted once"
    );

    r.handle.shutdown().await;
}

/// Replaying an already-completed force-accept intent changes nothing: no
/// second mandate or effect record, the terminal proposal is unchanged, and the
/// journal is cleared again. Also pins that an intent serialized before the
/// `execution_closure` field existed reads back as `false`.
#[tokio::test(flavor = "current_thread")]
async fn replaying_a_completed_force_accept_intent_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let proposal_id = fx.proposal_id.clone();

    fx.fail_store.arm(b"gov:proposal:");
    fx.force(ForcedOutcome::Accept).await.unwrap_err();
    let intent = fx.state().list_close_intents().unwrap().remove(0);

    let (r, backend) = fx.restart(tmp.path(), None).await;
    let completed = r.state.get_proposal(&proposal_id).unwrap().unwrap();
    assert_eq!(backend.mandate_writes(), 1);

    r.state.save_close_intent(&intent).unwrap();
    r.handle.recover_incomplete_closes().await;

    assert_eq!(backend.mandates().len(), 1);
    assert_eq!(backend.mandate_writes(), 1, "replay must not mint again");
    assert_eq!(
        backend.effect_writes(),
        1,
        "replay must not record the effect again"
    );
    assert_eq!(
        serde_json::to_value(r.state.get_proposal(&proposal_id).unwrap().unwrap()).unwrap(),
        serde_json::to_value(&completed).unwrap()
    );
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);

    let mut legacy = serde_json::to_value(&intent).unwrap();
    legacy.as_object_mut().unwrap().remove("execution_closure");
    let legacy: icn_governance_actor::close_journal::CloseJournalEntry =
        serde_json::from_value(legacy).unwrap();
    assert!(!legacy.execution_closure);

    r.handle.shutdown().await;
}

/// Re-issuing the same force-accept after a failed attempt completes the FIRST
/// attempt's decision (its closed_at, its mandate) and reports success.
#[tokio::test(flavor = "current_thread")]
async fn retrying_force_accept_completes_the_first_decision() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.fail_store.arm(b"gov:proposal:");
    fx.force(ForcedOutcome::Accept).await.unwrap_err();
    let intent = fx.state().list_close_intents().unwrap().remove(0);
    let first_mandate = fx.backend.mandates()[0].id.clone();

    fx.fail_store.disarm();
    fx.force(ForcedOutcome::Accept).await.unwrap();

    let stored = fx.state().get_proposal(&fx.proposal_id).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&stored).unwrap(),
        serde_json::to_value(&intent.proposal).unwrap(),
        "the retry committed the first attempt's decision, not a new one"
    );
    assert_eq!(fx.backend.mandates().len(), 1);
    assert_eq!(fx.backend.mandates()[0].id, first_mandate);
    assert_eq!(fx.backend.mandate_writes(), 1);
    assert_eq!(fx.backend.effect_writes(), 1);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 0);

    fx.handle.shutdown().await;
}

/// A different forced outcome after a failed force-accept does not replace the
/// durable decision: the accept is completed, then the reject is refused.
#[tokio::test(flavor = "current_thread")]
async fn force_reject_after_failed_force_accept_completes_the_accept_and_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.fail_store.arm(b"gov:proposal:");
    fx.force(ForcedOutcome::Accept).await.unwrap_err();
    fx.fail_store.disarm();

    let err = fx
        .force(ForcedOutcome::Reject)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("was already decided; it has now been completed"),
        "{err}"
    );
    assert!(is_force_accepted(&fx.stored_state()));
    assert_eq!(fx.backend.mandates().len(), 1);
    assert_eq!(fx.backend.mandate_writes(), 1);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 0);

    fx.handle.shutdown().await;
}

/// The execution closure itself fails after the intent is durable (effect
/// record write refused): no mandate is minted, the proposal is still Open and
/// the intent survives; once the store recovers, recovery completes it.
#[tokio::test(flavor = "current_thread")]
async fn execution_closure_failure_after_intent_is_completed_by_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.backend.fail_effect_write.store(true, Ordering::SeqCst);
    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("execution closure for proposal")
            && err.contains("injected institutional-effect write failure"),
        "{err}"
    );
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(
        fx.backend.mandates().len(),
        0,
        "no mandate when the closure failed first"
    );
    assert_eq!(fx.backend.grant_count(), 0);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.backend.fail_effect_write.store(false, Ordering::SeqCst);
    fx.handle.recover_incomplete_closes().await;

    assert!(is_force_accepted(&fx.stored_state()));
    assert_eq!(fx.backend.mandates().len(), 1);
    assert_eq!(fx.backend.effects().len(), 1);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 0);

    fx.handle.shutdown().await;
}

/// Everything durable but the acknowledgment interrupted (receipt-store flush
/// fails): the caller sees an error, the intent is kept, and a restart replays
/// it without a second mandate or effect record.
#[tokio::test(flavor = "current_thread")]
async fn interrupted_acknowledgment_replays_without_duplicate_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let proposal_id = fx.proposal_id.clone();

    fx.backend.fail_flush.store(true, Ordering::SeqCst);
    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("durability flush failed"), "{err}");
    assert!(is_force_accepted(&fx.stored_state()));
    assert_eq!(fx.backend.mandates().len(), 1);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.backend.fail_flush.store(false, Ordering::SeqCst);
    let (r, backend) = fx.restart(tmp.path(), None).await;

    assert!(is_force_accepted(
        &r.state.get_proposal(&proposal_id).unwrap().unwrap().state
    ));
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    assert_eq!(backend.mandates().len(), 1);
    assert_eq!(backend.mandate_writes(), 1);
    assert_eq!(backend.effect_writes(), 1);

    r.handle.shutdown().await;
}

/// Force reject is not journaled and writes no authority (unchanged path).
#[tokio::test(flavor = "current_thread")]
async fn force_reject_writes_no_intent_and_no_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.force(ForcedOutcome::Reject).await.unwrap();
    assert!(matches!(
        fx.stored_state(),
        ProposalState::ForceClosed {
            outcome: ProposalOutcome::Rejected,
            ..
        }
    ));
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 0);
    assert_eq!(fx.backend.mandate_writes(), 0);
    assert_eq!(fx.backend.effect_writes(), 0);

    fx.handle.shutdown().await;
}

/// A journaled execution closure is applied only for an entry that records an
/// accepted outcome. A malformed intent (closure flag set, outcome not accepted)
/// must fail closed: no mandate, grant or effect record, proposal untouched, the
/// intent retained for an authorized reconciliation.
#[tokio::test(flavor = "current_thread")]
async fn execution_closure_for_a_non_accepted_intent_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    let mut proposal = fx.state().get_proposal(&fx.proposal_id).unwrap().unwrap();
    proposal
        .force_close(ProposalOutcome::Rejected, "malformed".to_string())
        .unwrap();
    let entry = icn_governance_actor::close_journal::CloseJournalEntry {
        proposal,
        proof_bytes: None,
        receipts: Default::default(),
        decided_at: 1,
        governance_decision_hash: Some(forced_decision_hash(&fx)),
        execution_closure: true,
    };
    fx.state().save_close_intent(&entry).unwrap();
    fx.handle.recover_incomplete_closes().await;

    assert_eq!(fx.backend.mandate_writes(), 0);
    assert_eq!(fx.backend.grant_count(), 0);
    assert_eq!(fx.backend.effect_writes(), 0);
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.handle.shutdown().await;
}

// ---------------------------------------------------------------------------
// Follow-ups: stored format, decision identity, wedged intents, lost receipt
// writes, legacy replay, non-execution-required force-accept
// ---------------------------------------------------------------------------

/// The shape of a close-journal entry as a binary that predates
/// `execution_closure` reads it: every field it requires, no others.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct PreClosureReaderEntry {
    proposal: icn_governance::Proposal,
    proof_bytes: Option<Vec<u8>>,
    receipts: icn_governance_actor::close_journal::CloseReceipts,
    decided_at: u64,
    governance_decision_hash: Option<String>,
}

/// An intent that carries an execution closure is stored so that a reader
/// which does not know the field refuses it (it would otherwise replay a
/// force-accept as a rejection with no closure). An intent without one keeps
/// the legacy shape that reader parses.
#[tokio::test(flavor = "current_thread")]
async fn execution_closure_intent_is_refused_by_a_pre_closure_reader() {
    use icn_governance_actor::close_journal::CloseJournalEntry;
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;

    fx.fail_store.arm(b"gov:proposal:");
    fx.force(ForcedOutcome::Accept).await.unwrap_err();
    let raw = fx.raw_intent().expect("the intent is durable");
    let err = serde_json::from_slice::<PreClosureReaderEntry>(&raw)
        .err()
        .expect("a pre-closure reader must refuse an execution-closure intent");
    assert!(
        err.to_string().contains("missing field `proposal`"),
        "{err}"
    );

    let entry = CloseJournalEntry::from_stored_bytes(&raw).unwrap();
    assert!(entry.execution_closure);
    assert!(is_force_accepted(&entry.proposal.state));

    // The same decision without a closure is stored in the legacy shape.
    let mut plain = entry.clone();
    plain.execution_closure = false;
    let plain_raw = plain.to_stored_bytes().unwrap();
    serde_json::from_slice::<PreClosureReaderEntry>(&plain_raw)
        .expect("a legacy entry stays readable by a pre-closure reader");
    assert!(
        !CloseJournalEntry::from_stored_bytes(&plain_raw)
            .unwrap()
            .execution_closure
    );

    // An envelope that does not carry a closure is malformed, not legacy.
    let mut forged = serde_json::Map::new();
    forged.insert(
        "execution_closure_intent_v1".to_string(),
        serde_json::to_value(&plain).unwrap(),
    );
    assert!(CloseJournalEntry::from_stored_bytes(&serde_json::to_vec(&forged).unwrap()).is_err());

    fx.handle.shutdown().await;
}

/// A mandate already stored for the proposal under ANOTHER decision hash is
/// not this force-accept's mandate. Before the fix `AlreadyMinted` matched on
/// proposal id alone, so the force-accept "completed" with the terminal
/// record and event bound to the forced hash and the mandate bound to the
/// other one. Now the closure is refused, nothing new is minted, and the
/// durable intent stays for an authorized reconciliation.
#[tokio::test(flavor = "current_thread")]
async fn force_accept_refuses_a_mandate_bound_to_another_decision() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let payload = fx
        .state()
        .get_proposal(&fx.proposal_id)
        .unwrap()
        .unwrap()
        .payload;
    let other = [0x5au8; 32];
    icn_governance_actor::grant_minting::mint_and_persist_for_accepted(
        fx.backend.as_ref(),
        &icn_governance_actor::decision_commitment::UnavailableCommitments(
            "test: no template provenance consulted",
        ),
        &fx.proposal_id.0,
        &fx.domain_id,
        other,
        &payload,
        1,
    )
    .unwrap();
    let seeded = fx.backend.mandates();

    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(icn_governance_actor::grant_minting::MANDATE_IDENTITY_CONFLICT),
        "{err}"
    );
    assert_eq!(
        fx.backend.mandates(),
        seeded,
        "no second mandate, none replaced"
    );
    assert_eq!(fx.backend.mandates()[0].decision.decision_hash, other);
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.handle.shutdown().await;
}

/// The same for an institutional-effect record bound to another decision:
/// refused before any mandate is minted.
#[tokio::test(flavor = "current_thread")]
async fn force_accept_refuses_an_effect_bound_to_another_decision() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let payload = fx
        .state()
        .get_proposal(&fx.proposal_id)
        .unwrap()
        .unwrap()
        .payload;
    icn_governance_actor::institutional_effect::emit_accepted_effect(
        fx.backend.as_ref(),
        &fx.proposal_id.0,
        &fx.domain_id.0,
        Some([0x5au8; 32]),
        &payload,
        1,
    )
    .unwrap();

    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("already bound to a different decision"),
        "{err}"
    );
    assert_eq!(fx.backend.mandate_writes(), 0);
    assert_eq!(fx.backend.grant_count(), 0);
    assert_eq!(fx.backend.effects().len(), 1);
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.handle.shutdown().await;
}

/// A force-accept intent whose closure keeps failing is NOT discarded: veto
/// and force-reject are refused while it cannot complete, a restart keeps it
/// (and does not report it as a conflict), and once the cause is fixed the
/// next veto completes the accept first and is then refused.
#[tokio::test(flavor = "current_thread")]
async fn incomplete_force_accept_intent_blocks_veto_until_it_completes() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let proposal_id = fx.proposal_id.clone();
    let veto = |id: &ProposalId| GovernanceCommand::VetoProposal {
        proposal_id: id.clone(),
        reason: "veto during a wedged force-accept".to_string(),
    };

    fx.backend.fail_mandate_write.store(true, Ordering::SeqCst);
    let err = fx
        .force(ForcedOutcome::Accept)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("injected mandate write failure"), "{err}");
    assert_eq!(
        fx.backend.effects().len(),
        1,
        "the effect precedes the mandate"
    );
    assert_eq!(fx.backend.mandates().len(), 0);

    let err = fx
        .handle
        .submit(veto(&proposal_id))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("its already-decided close could not be completed first"),
        "{err}"
    );
    let err = fx
        .force(ForcedOutcome::Reject)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("its already-decided close could not be completed first"),
        "{err}"
    );
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    // Restart with the fault still present: recovery keeps the intent.
    let (r, backend) = fx.restart(tmp.path(), None).await;
    assert_eq!(
        r.state.list_close_intents().unwrap().len(),
        1,
        "not discarded"
    );
    assert!(matches!(
        r.state.get_proposal(&proposal_id).unwrap().unwrap().state,
        ProposalState::Open { .. }
    ));
    assert!(r.handle.close_conflicts().await.unwrap().is_empty());

    backend.fail_mandate_write.store(false, Ordering::SeqCst);
    let err = r
        .handle
        .submit(veto(&proposal_id))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("was already decided; it has now been completed"),
        "{err}"
    );
    assert!(is_force_accepted(
        &r.state.get_proposal(&proposal_id).unwrap().unwrap().state
    ));
    assert_eq!(backend.mandates().len(), 1);
    assert_eq!(backend.effects().len(), 1);
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);

    r.handle.shutdown().await;
}

/// Receipt store whose records live in a sled `Db` on disk, so what survives a
/// restart is what that `Db` holds when it is reopened.
struct SledAuthorityBackend {
    store: SledStore,
}

impl SledAuthorityBackend {
    fn open(path: &std::path::Path) -> Self {
        Self {
            store: SledStore::open(path).unwrap(),
        }
    }
    fn rows<T: serde::de::DeserializeOwned>(&self, prefix: &str) -> Vec<T> {
        self.store
            .scan(prefix.as_bytes())
            .unwrap()
            .into_iter()
            .map(|(_, v)| serde_json::from_slice(&v).unwrap())
            .collect()
    }
    fn mandates(&self) -> Vec<icn_governance::Mandate> {
        self.rows("mandate:")
    }
    fn effects(&self) -> Vec<InstitutionalEffectRecord> {
        self.rows("effect:")
    }
    fn put_json<T: serde::Serialize>(&self, key: String, value: &T) -> Result<(), String> {
        self.store
            .put(key.as_bytes(), &serde_json::to_vec(value).unwrap())
            .map_err(|e| e.to_string())
    }
}

impl GovernanceReceiptBackend for SledAuthorityBackend {
    fn put_governance(&self, _: &icn_governance::GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    // Stores no governance receipts, so it holds none for any proposal.
    fn list_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Vec<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(vec![])
    }
    fn put_allocation(
        &self,
        _: &icn_kernel_api::AllocationReceipt,
    ) -> Result<icn_kernel_api::Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn flush(&self) -> Result<(), String> {
        self.store.flush().map(|_| ()).map_err(|e| e.to_string())
    }
    fn put_institutional_effect(&self, record: &InstitutionalEffectRecord) -> Result<(), String> {
        self.put_json(format!("effect:{}", record.record_id), record)
    }
    fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(self
            .effects()
            .into_iter()
            .filter(|r| r.proposal_id == proposal_id)
            .collect())
    }
    fn put_mandate(&self, mandate: &icn_governance::Mandate) -> Result<(), String> {
        self.put_json(format!("mandate:{}", mandate.id), mandate)
    }
    fn get_mandate_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<icn_governance::Mandate>, String> {
        Ok(self
            .mandates()
            .into_iter()
            .find(|m| m.decision.proposal_id == proposal_id))
    }
    fn put_mandate_with_grants(
        &self,
        mandate: &icn_governance::Mandate,
        grants: &[icn_governance::AuthorityGrant],
    ) -> Result<(), String> {
        for g in grants {
            self.put_authority_grant(g)?;
        }
        self.put_mandate(mandate)
    }
    fn put_authority_grant(&self, grant: &icn_governance::AuthorityGrant) -> Result<(), String> {
        self.put_json(
            format!("grant:{}", serde_json::to_string(&grant.id).unwrap()),
            grant,
        )
    }
    fn get_authority_grant(
        &self,
        grant_id: &icn_governance::AuthorityGrantId,
    ) -> Result<Option<icn_governance::AuthorityGrant>, String> {
        Ok(self
            .rows::<icn_governance::AuthorityGrant>("grant:")
            .into_iter()
            .find(|g| &g.id == grant_id))
    }
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The receipt `Db` loses the effect record and mandate a failed force-accept
/// wrote but never fsynced (the actor flushes the receipt store only after the
/// terminal save, which failed). The crash is modelled by reopening the
/// receipt `Db` from its last fsynced on-disk image. Replay re-mints exactly
/// once, bound to the forced decision, and that mandate then survives a
/// further restart because completion fsyncs the receipt store.
#[tokio::test(flavor = "current_thread")]
async fn lost_unflushed_receipt_writes_are_reminted_exactly_once_on_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let state_path = tmp.path().join("state");
    let receipts_path = tmp.path().join("receipts");
    let checkpoint_path = tmp.path().join("receipts-fsynced");
    let fx = Fixture::open(&state_path).await;
    let expected_hash = forced_decision_hash(&fx);
    let proposal_id = fx.proposal_id.clone();

    let receipts = Arc::new(SledAuthorityBackend::open(&receipts_path));
    receipts.flush().unwrap();
    copy_dir(&receipts_path, &checkpoint_path);
    fx.handle.install_receipt_store(receipts.clone()).await;

    fx.fail_store.arm(b"gov:proposal:");
    fx.force(ForcedOutcome::Accept).await.unwrap_err();
    let lost = receipts.mandates();
    assert_eq!(lost.len(), 1, "the mandate was written, unflushed");
    assert_eq!(receipts.effects().len(), 1);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    // Crash: the receipt Db keeps only its fsynced image.
    fx.release().await;
    drop(receipts);
    std::fs::remove_dir_all(&receipts_path).unwrap();
    copy_dir(&checkpoint_path, &receipts_path);
    let receipts = Arc::new(SledAuthorityBackend::open(&receipts_path));
    assert_eq!(
        receipts.mandates().len(),
        0,
        "the unflushed mandate is gone"
    );
    assert_eq!(receipts.effects().len(), 0, "the unflushed effect is gone");

    let r = start(&state_path, receipts.clone(), None).await;
    assert!(is_force_accepted(
        &r.state.get_proposal(&proposal_id).unwrap().unwrap().state
    ));
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    let mandates = receipts.mandates();
    assert_eq!(mandates.len(), 1, "exactly one mandate (ADR-0019)");
    assert_ne!(
        mandates[0].id, lost[0].id,
        "a fresh mandate replaced the lost one"
    );
    assert_eq!(
        hex::encode(mandates[0].decision.decision_hash),
        expected_hash
    );
    assert_eq!(receipts.effects().len(), 1);

    // Completion fsynced the receipt store: a further crash keeps the mandate.
    r.handle.shutdown().await;
    drop(r);
    let kept = mandates[0].id.clone();
    drop(receipts);
    let receipts = Arc::new(SledAuthorityBackend::open(&receipts_path));
    let r = start(&state_path, receipts.clone(), None).await;
    assert_eq!(receipts.mandates().len(), 1);
    assert_eq!(receipts.mandates()[0].id, kept);
    r.handle.shutdown().await;
}

/// An ordinary-close intent written before `execution_closure` existed (no
/// such field in the stored JSON) replays end to end through startup recovery
/// exactly as before: the journaled terminal proposal is committed, the
/// `ProposalAccepted` event carries the journaled hash, the intent is cleared,
/// and replay writes no execution closure (that close wrote it itself).
#[tokio::test(flavor = "current_thread")]
async fn legacy_close_intent_without_the_field_replays_unchanged_on_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let proposal_id = fx.proposal_id.clone();

    let mut proposal = fx.state().get_proposal(&proposal_id).unwrap().unwrap();
    proposal
        .close(ProposalState::Accepted { closed_at: 4_242 })
        .unwrap();
    let hash = hex::encode([0x7cu8; 32]);
    let mut legacy = serde_json::to_value(icn_governance_actor::close_journal::CloseJournalEntry {
        proposal: proposal.clone(),
        proof_bytes: None,
        receipts: Default::default(),
        decided_at: 4_242,
        governance_decision_hash: Some(hash.clone()),
        execution_closure: false,
    })
    .unwrap();
    assert!(legacy
        .as_object_mut()
        .unwrap()
        .remove("execution_closure")
        .is_some());
    fx.fail_store
        .put(
            format!("gov:close-intent:{}", proposal_id.0).as_bytes(),
            &serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();

    let events = Arc::new(AcceptedEvents::default());
    let (r, backend) = fx.restart(tmp.path(), Some(events.clone())).await;

    let stored = r.state.get_proposal(&proposal_id).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&stored).unwrap(),
        serde_json::to_value(&proposal).unwrap()
    );
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    assert_eq!(
        events.seen.lock().unwrap().clone(),
        vec![(proposal_id.0.clone(), Some(hash))]
    );
    assert_eq!(backend.mandate_writes(), 0);
    assert_eq!(backend.effect_writes(), 0);

    r.handle.shutdown().await;
}

/// A NON-execution-required force-accept (Text payload) is journaled with no
/// execution closure; its pending-grants mandate comes from the replayed
/// downstream effects. Crash at the terminal save: nothing is minted, and the
/// restart completes the forced decision with one mandate bound to the forced
/// hash and the forced `ProposalAccepted` event.
#[tokio::test(flavor = "current_thread")]
async fn non_execution_required_force_accept_recovers_after_a_mid_journal_crash() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open_with(tmp.path(), |_| ProposalPayload::Text {
        body: "forced text".to_string(),
    })
    .await;
    let expected_hash = forced_decision_hash(&fx);
    let proposal_id = fx.proposal_id.clone();

    fx.fail_store.arm(b"gov:proposal:");
    fx.force(ForcedOutcome::Accept).await.unwrap_err();
    let intent = fx.state().list_close_intents().unwrap().remove(0);
    assert!(!intent.execution_closure);
    assert!(is_force_accepted(&intent.proposal.state));
    assert_eq!(fx.backend.mandate_writes(), 0, "nothing before completion");
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));

    let events = Arc::new(AcceptedEvents::default());
    let (r, backend) = fx.restart(tmp.path(), Some(events.clone())).await;

    assert!(is_force_accepted(
        &r.state.get_proposal(&proposal_id).unwrap().unwrap().state
    ));
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    let mandates = backend.mandates();
    assert_eq!(mandates.len(), 1);
    assert_eq!(
        hex::encode(mandates[0].decision.decision_hash),
        expected_hash
    );
    assert_eq!(
        events.seen.lock().unwrap().clone(),
        vec![(proposal_id.0.clone(), Some(expected_hash))]
    );

    // Replaying again does not mint a second one.
    r.state.save_close_intent(&intent).unwrap();
    r.handle.recover_incomplete_closes().await;
    assert_eq!(backend.mandates().len(), 1);
    assert_eq!(backend.mandate_writes(), 1);

    r.handle.shutdown().await;
}
// ---------------------------------------------------------------------------
// Ordinary close: the execution closure stands behind the close intent
// ---------------------------------------------------------------------------

impl Fixture {
    /// Cast the sole member's For vote, so an ordinary close accepts.
    async fn vote_for(&self) {
        self.handle
            .submit(GovernanceCommand::CastVote {
                proposal_id: self.proposal_id.clone(),
                voter: self.member.clone(),
                choice: icn_governance::VoteChoice::For,
                comment: None,
            })
            .await
            .unwrap();
    }

    async fn close(&self) -> anyhow::Result<()> {
        self.handle
            .submit(GovernanceCommand::CloseProposal {
                proposal_id: self.proposal_id.clone(),
                eligible_voters: None,
                excluded_delegators: None,
                capability_scope: None,
            })
            .await
    }

    fn authority_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.backend.mandates().len(),
            self.backend.grant_count(),
            self.backend.effects().len(),
            self.state().list_close_intents().unwrap().len(),
        )
    }
}

/// `CloseProposal` on a vetoed proposal whose tally accepts is refused by the
/// state transition. Before the fix the execution-closure preflight ran first,
/// so the refused close still wrote a mandate, a grant and an effect record.
#[tokio::test(flavor = "current_thread")]
async fn refused_close_of_a_vetoed_proposal_writes_no_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    fx.vote_for().await;
    fx.handle
        .submit(GovernanceCommand::VetoProposal {
            proposal_id: fx.proposal_id.clone(),
            reason: "vetoed before close".to_string(),
        })
        .await
        .unwrap();

    let err = fx.close().await.unwrap_err().to_string();
    assert!(
        err.contains("Can only close proposals in Open state"),
        "{err}"
    );
    assert_eq!(
        fx.authority_counts(),
        (0, 0, 0, 0),
        "mandate, grant, effect, intent"
    );
    assert!(matches!(fx.stored_state(), ProposalState::Vetoed { .. }));

    fx.handle.shutdown().await;
}

/// The close intent cannot be written: nothing — no mandate, grant, effect
/// record or terminal state — is written. Before the fix the preflight minted
/// before the intent.
#[tokio::test(flavor = "current_thread")]
async fn close_intent_write_failure_writes_no_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    fx.vote_for().await;

    fx.fail_store.arm(b"gov:close-intent:");
    let err = fx.close().await.unwrap_err().to_string();
    assert!(err.contains("injected write failure"), "{err}");
    assert_eq!(fx.authority_counts(), (0, 0, 0, 0));
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));

    fx.handle.shutdown().await;
}

/// The terminal save fails after the intent: the mandate (if any) is backed by
/// the durable intent, which carries the closure, and a restart completes the
/// SAME decision with exactly one mandate bound to the journaled hash.
#[tokio::test(flavor = "current_thread")]
async fn failed_close_save_is_completed_on_restart_with_one_mandate() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let proposal_id = fx.proposal_id.clone();
    fx.vote_for().await;

    fx.fail_store.arm(b"gov:proposal:");
    fx.close().await.unwrap_err();
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    let intent = fx.state().list_close_intents().unwrap().remove(0);
    assert!(
        intent.execution_closure,
        "the close intent carries its closure"
    );
    assert!(matches!(
        intent.proposal.state,
        ProposalState::Accepted { .. }
    ));
    let hash = intent.governance_decision_hash.clone().unwrap();
    assert_eq!(fx.backend.mandates().len(), 1, "written behind the intent");

    let (r, backend) = fx.restart(tmp.path(), None).await;
    let stored = r.state.get_proposal(&proposal_id).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&stored).unwrap(),
        serde_json::to_value(&intent.proposal).unwrap()
    );
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    assert_eq!(backend.mandates().len(), 1);
    assert_eq!(backend.mandate_writes(), 1);
    assert_eq!(
        hex::encode(backend.mandates()[0].decision.decision_hash),
        hash
    );
    assert_eq!(backend.effect_writes(), 1);

    r.handle.shutdown().await;
}

/// The effect write fails after the intent is durable: no mandate, the
/// proposal is still Open, the intent survives; recovery then completes it.
#[tokio::test(flavor = "current_thread")]
async fn close_execution_closure_failure_is_completed_by_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    fx.vote_for().await;

    fx.backend.fail_effect_write.store(true, Ordering::SeqCst);
    let err = fx.close().await.unwrap_err().to_string();
    assert!(
        err.contains("injected institutional-effect write failure"),
        "{err}"
    );
    assert_eq!(fx.authority_counts(), (0, 0, 0, 1));
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));

    fx.backend.fail_effect_write.store(false, Ordering::SeqCst);
    fx.handle.recover_incomplete_closes().await;
    assert!(matches!(fx.stored_state(), ProposalState::Accepted { .. }));
    assert_eq!(fx.authority_counts(), (1, 1, 1, 0));

    fx.handle.shutdown().await;
}

/// Committed but unacknowledged (receipt-store flush fails): the caller sees
/// an error, the intent stays, and a restart replays it without a second
/// mandate or effect record.
#[tokio::test(flavor = "current_thread")]
async fn unacknowledged_close_replays_without_duplicate_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let proposal_id = fx.proposal_id.clone();
    fx.vote_for().await;

    fx.backend.fail_flush.store(true, Ordering::SeqCst);
    // The live close path keeps the intent and reports success for a flush
    // failure only in its log; either way the intent must remain.
    let _ = fx.close().await;
    assert!(matches!(fx.stored_state(), ProposalState::Accepted { .. }));
    assert_eq!(fx.backend.mandates().len(), 1);
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.backend.fail_flush.store(false, Ordering::SeqCst);
    let (r, backend) = fx.restart(tmp.path(), None).await;
    assert!(matches!(
        r.state.get_proposal(&proposal_id).unwrap().unwrap().state,
        ProposalState::Accepted { .. }
    ));
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    assert_eq!(backend.mandates().len(), 1);
    assert_eq!(backend.mandate_writes(), 1);
    assert_eq!(backend.effect_writes(), 1);

    r.handle.shutdown().await;
}

/// Lost unflushed receipt writes across a real reopen of a sled-backed
/// receipt `Db`: replay re-mints exactly once, bound to the journaled hash.
#[tokio::test(flavor = "current_thread")]
async fn close_lost_unflushed_receipt_writes_are_reminted_exactly_once() {
    let tmp = tempfile::tempdir().unwrap();
    let state_path = tmp.path().join("state");
    let receipts_path = tmp.path().join("receipts");
    let checkpoint_path = tmp.path().join("receipts-fsynced");
    let fx = Fixture::open(&state_path).await;
    let proposal_id = fx.proposal_id.clone();
    fx.vote_for().await;

    let receipts = Arc::new(SledAuthorityBackend::open(&receipts_path));
    receipts.flush().unwrap();
    copy_dir(&receipts_path, &checkpoint_path);
    fx.handle.install_receipt_store(receipts.clone()).await;

    fx.fail_store.arm(b"gov:proposal:");
    fx.close().await.unwrap_err();
    let hash = fx.state().list_close_intents().unwrap()[0]
        .governance_decision_hash
        .clone()
        .unwrap();
    assert_eq!(receipts.mandates().len(), 1);

    fx.release().await;
    drop(receipts);
    std::fs::remove_dir_all(&receipts_path).unwrap();
    copy_dir(&checkpoint_path, &receipts_path);
    let receipts = Arc::new(SledAuthorityBackend::open(&receipts_path));
    assert_eq!(receipts.mandates().len(), 0);

    let r = start(&state_path, receipts.clone(), None).await;
    assert!(matches!(
        r.state.get_proposal(&proposal_id).unwrap().unwrap().state,
        ProposalState::Accepted { .. }
    ));
    assert_eq!(r.state.list_close_intents().unwrap().len(), 0);
    let mandates = receipts.mandates();
    assert_eq!(mandates.len(), 1);
    assert_eq!(hex::encode(mandates[0].decision.decision_hash), hash);
    assert_eq!(receipts.effects().len(), 1);
    r.handle.shutdown().await;
}

/// The journal alone does not authorize. An intent whose closure hash does not
/// match the governance receipt it carries fails closed: nothing minted, the
/// proposal untouched, the intent kept.
#[tokio::test(flavor = "current_thread")]
async fn close_intent_with_an_inconsistent_decision_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    fx.vote_for().await;

    fx.fail_store.arm(b"gov:proposal:");
    fx.backend.fail_effect_write.store(true, Ordering::SeqCst);
    fx.close().await.unwrap_err();
    fx.fail_store.disarm();
    fx.backend.fail_effect_write.store(false, Ordering::SeqCst);
    let mut intent = fx.state().list_close_intents().unwrap().remove(0);
    assert!(intent.receipts.governance_receipt.is_some());
    intent.governance_decision_hash = Some(hex::encode([0x33u8; 32]));
    fx.state().save_close_intent(&intent).unwrap();

    fx.handle.recover_incomplete_closes().await;
    assert_eq!(fx.backend.mandate_writes(), 0);
    assert_eq!(fx.backend.effect_writes(), 0);
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.handle.shutdown().await;
}

/// An institutional-effect record already stored for the proposal under
/// ANOTHER decision hash is not this close's effect. Before the fix the close
/// treated `AlreadyEmitted` as success and went on to mint a mandate bound to
/// its own hash beside it. Now the closure is refused: no mandate, the
/// proposal stays Open and the durable intent is kept for reconciliation.
#[tokio::test(flavor = "current_thread")]
async fn close_refuses_an_effect_bound_to_another_decision() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = Fixture::open(tmp.path()).await;
    let payload = fx
        .state()
        .get_proposal(&fx.proposal_id)
        .unwrap()
        .unwrap()
        .payload;
    icn_governance_actor::institutional_effect::emit_accepted_effect(
        fx.backend.as_ref(),
        &fx.proposal_id.0,
        &fx.domain_id.0,
        Some([0x5au8; 32]),
        &payload,
        1,
    )
    .unwrap();
    fx.vote_for().await;

    let err = fx.close().await.unwrap_err().to_string();
    assert!(
        err.contains("already bound to a different decision"),
        "{err}"
    );
    assert_eq!(fx.backend.mandate_writes(), 0);
    assert_eq!(fx.backend.grant_count(), 0);
    assert_eq!(fx.backend.effects().len(), 1);
    assert!(matches!(fx.stored_state(), ProposalState::Open { .. }));
    assert_eq!(fx.state().list_close_intents().unwrap().len(), 1);

    fx.handle.shutdown().await;
}
