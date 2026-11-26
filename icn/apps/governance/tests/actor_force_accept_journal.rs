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
}

/// The actor and state store after a real restart: every handle on the first
/// sled `Db` is dropped and the database is reopened from disk.
struct Restarted {
    handle: GovernanceHandle,
    state: Arc<dyn GovernanceStateStore>,
}

impl Fixture {
    async fn open(db_path: &std::path::Path) -> Self {
        let did = IdentityBundle::generate().unwrap().did().clone();
        let candidate = IdentityBundle::generate().unwrap().did().clone();
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
                "Appoint steward (force-accept journal)".to_string(),
                String::new(),
                ProposalPayload::Sdis {
                    proposal: SdisProposal::AppointSteward {
                        candidate,
                        sponsors: vec![did.clone()],
                        region: "region-force".to_string(),
                        bond_amount: 1_000,
                        term_length: 3600 * 24 * 30,
                    },
                },
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
        let Fixture {
            backend,
            fail_store,
            handle,
            ..
        } = self;
        handle.shutdown().await;
        drop(handle);
        drop(fail_store);
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
        handle.install_receipt_store(backend.clone()).await;
        handle.recover_incomplete_closes().await;
        (
            Restarted {
                handle,
                state: Arc::new(SledGovernanceStateStore::new(store)),
            },
            backend,
        )
    }

    fn stored_state(&self) -> ProposalState {
        self.state()
            .get_proposal(&self.proposal_id)
            .unwrap()
            .unwrap()
            .state
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
