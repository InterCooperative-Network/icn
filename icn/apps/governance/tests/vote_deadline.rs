//! Voting deadlines at the governance actor.
//!
//! A proposal's `closes_at` is part of the decision. These tests require that:
//!
//! * a vote at or after `closes_at` is refused at the actor (the single vote
//!   write boundary) whether or not the scheduled close has run;
//! * an actor restarted over a store holding an Open proposal restores that
//!   proposal's scheduled close — before the deadline it closes when the
//!   deadline passes, after the deadline it closes promptly;
//! * a duplicate close of an already-closed proposal changes nothing.
//!
//! Proposals with chosen deadlines are planted directly in the store (exactly
//! the bytes an earlier actor would have written), so no test depends on the
//! scheduler having run before a restart.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceDomainId, GovernanceError, GovernanceOps, GovernanceParams, MembershipConfig,
    Proposal, ProposalId, ProposalPayload, ProposalState, StaticMembershipResolver, Vote,
    VoteChoice,
};
use icn_governance_actor::actor::{GovernanceActor, GovernanceHandle};
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_governance_actor::GovernanceCommand;
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;

const DOMAIN: &str = "vote-deadline-dom";
/// The actor's scheduler ticks every 10 s; allow two ticks plus slack.
const CLOSE_WAIT: Duration = Duration::from_secs(25);

fn new_did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

fn now() -> u64 {
    icn_time::current_timestamp_secs()
}

/// Spawn as production startup does: the actor, then startup recovery (which
/// releases the scheduler).
async fn spawn_actor(store: Arc<SledStore>) -> GovernanceHandle {
    let handle = spawn_actor_unrecovered(store).await;
    handle.recover_incomplete_closes().await;
    handle
}

/// The actor alone, before startup recovery has run.
async fn spawn_actor_unrecovered(store: Arc<SledStore>) -> GovernanceHandle {
    let node = new_did();
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    GovernanceActor::spawn(
        node,
        store,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap()
}

/// A sled store holding a static-list domain of `members` (created through an
/// actor that is then shut down, releasing the store).
async fn store_with_domain(path: &std::path::Path, members: &[Did]) {
    let store = Arc::new(SledStore::open(path).unwrap());
    let handle = spawn_actor(store.clone()).await;
    GovernanceOps::create_domain(
        &handle,
        GovernanceDomainId(DOMAIN.to_string()),
        "Vote deadline".to_string(),
        "cooperative_default".to_string(),
        GovernanceParams::new(50, 50, 3600),
        MembershipConfig::static_list(members.to_vec()),
    )
    .await
    .unwrap();
    handle.shutdown().await;
}

/// Plant an Open proposal with the given window, as an earlier actor stored it.
fn plant_open(
    path: &std::path::Path,
    proposer: &Did,
    opened_at: u64,
    closes_at: u64,
) -> ProposalId {
    let store = Arc::new(SledStore::open(path).unwrap());
    let state = SledGovernanceStateStore::new(store);
    let mut p = Proposal::new(
        GovernanceDomainId(DOMAIN.to_string()),
        proposer.clone(),
        "deadline".into(),
        "d".into(),
        ProposalPayload::Text { body: "b".into() },
    );
    p.state = ProposalState::Open {
        opened_at,
        closes_at,
    };
    state.create_proposal(&p).unwrap();
    state.flush().unwrap();
    p.id
}

fn plant_vote(path: &std::path::Path, id: &ProposalId, voter: &Did, choice: VoteChoice) {
    let store = Arc::new(SledStore::open(path).unwrap());
    let state = SledGovernanceStateStore::new(store);
    state
        .save_vote(id, &Vote::new(id.clone(), voter.clone(), choice))
        .unwrap();
    state.flush().unwrap();
}

async fn vote(
    handle: &GovernanceHandle,
    id: &ProposalId,
    voter: &Did,
    choice: VoteChoice,
) -> anyhow::Result<()> {
    let ops: &dyn GovernanceOps = handle;
    ops.cast_vote(id.clone(), voter.clone(), choice, None).await
}

async fn state_of(handle: &GovernanceHandle, id: &ProposalId) -> ProposalState {
    handle.get_proposal(id).await.unwrap().unwrap().state
}

/// Wait (bounded) until the proposal leaves Open; returns the final state.
async fn wait_closed(handle: &GovernanceHandle, id: &ProposalId) -> ProposalState {
    let deadline = tokio::time::Instant::now() + CLOSE_WAIT;
    loop {
        let s = state_of(handle, id).await;
        if !s.is_open() || tokio::time::Instant::now() >= deadline {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn typed(r: anyhow::Result<()>) -> GovernanceError {
    match r
        .expect_err("vote must be refused")
        .downcast::<GovernanceError>()
    {
        Ok(g) => g,
        Err(other) => panic!("expected a typed GovernanceError, got: {other:#}"),
    }
}

#[tokio::test]
async fn vote_after_the_deadline_is_refused_before_any_close_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    // Deadline far enough in the past; nothing has closed it.
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 3600);

    // A fresh actor (its rehydrated close fires on the first tick, ~10 s away).
    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    let err = typed(vote(&handle, &id, &alice, VoteChoice::For).await);
    assert!(
        matches!(err, GovernanceError::VotingPeriodEnded(_)),
        "got {err:?}"
    );
    // Release the actor's exclusive sled lock before inspecting the store.
    handle.shutdown().await;
    drop(handle);
    let state = SledGovernanceStateStore::new(Arc::new(SledStore::open(tmp.path()).unwrap()));
    assert!(
        state.list_votes(&id).unwrap().is_empty(),
        "a refused vote writes no row"
    );
}

#[tokio::test]
async fn restart_after_the_deadline_closes_with_the_votes_cast_in_time() {
    let tmp = tempfile::tempdir().unwrap();
    let (alice, bob) = (new_did(), new_did());
    store_with_domain(tmp.path(), &[alice.clone(), bob.clone()]).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);
    plant_vote(tmp.path(), &id, &bob, VoteChoice::For);

    // Before the fix the scheduled close was lost on restart and the proposal
    // stayed Open (and votable) indefinitely.
    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    let s = wait_closed(&handle, &id).await;
    assert!(
        matches!(s, ProposalState::Accepted { .. }),
        "restored close must run: {s:?}"
    );

    // A duplicate close (scheduler event or caller) changes nothing.
    let dup = handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await;
    assert!(dup.is_err(), "closing a closed proposal is refused");
    assert_eq!(state_of(&handle, &id).await, s, "the decision is unchanged");
    handle.shutdown().await;
}

#[tokio::test]
async fn restart_before_the_deadline_keeps_voting_open_until_it_then_closes() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    // Wide enough to cover actor spawn under load; the test waits it out.
    let closes_at = t + 20;
    let id = plant_open(tmp.path(), &alice, t - 60, closes_at);

    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    // Inside the window: admitted.
    vote(&handle, &id, &alice, VoteChoice::For).await.unwrap();
    assert!(
        state_of(&handle, &id).await.is_open(),
        "not closed before the deadline"
    );

    // After the window, before or after the close runs: refused.
    while now() < closes_at {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let late = vote(&handle, &id, &alice, VoteChoice::Against).await;
    if state_of(&handle, &id).await.is_open() {
        assert!(matches!(typed(late), GovernanceError::VotingPeriodEnded(_)));
    } else {
        assert!(late.is_err(), "a closed proposal admits no vote");
    }

    let s = wait_closed(&handle, &id).await;
    assert!(
        matches!(s, ProposalState::Accepted { .. }),
        "restored close runs at the deadline: {s:?}"
    );
    handle.shutdown().await;
}

#[tokio::test]
async fn a_malformed_stored_deadline_does_not_stop_unrelated_governance() {
    let tmp = tempfile::tempdir().unwrap();
    let (alice, bob) = (new_did(), new_did());
    store_with_domain(tmp.path(), &[alice.clone(), bob.clone()]).await;
    let t = now();
    // Unrepresentable stored deadline (malformed state) next to a legitimate
    // proposal whose deadline has passed.
    let malformed = plant_open(
        tmp.path(),
        &alice,
        t - 60,
        icn_governance::MAX_REPRESENTABLE_DEADLINE_SECS + 1,
    );
    // u64::MAX is the fixture that actually overflowed the old rehydration
    // (`Instant + Duration` of ~u64::MAX seconds); MAX+1 alone does not.
    let overflowing = plant_open(tmp.path(), &alice, t - 60, u64::MAX);
    let legit = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &legit, &alice, VoteChoice::For);
    plant_vote(tmp.path(), &legit, &bob, VoteChoice::For);

    // Before the fix, rehydration overflowed an Instant on `overflowing` and
    // the actor panicked at spawn.
    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;

    // Unrelated governance continues: the legitimate proposal closes.
    let s = wait_closed(&handle, &legit).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");

    // The malformed ones stay Open and refuse votes (fail closed). Stored state
    // that cannot be trusted is "not open for voting", not a caller error.
    for id in [&malformed, &overflowing] {
        assert!(state_of(&handle, id).await.is_open());
        let err = typed(vote(&handle, id, &alice, VoteChoice::For).await);
        assert!(
            matches!(err, GovernanceError::ProposalNotOpen(_)),
            "got {err:?}"
        );
    }
    handle.shutdown().await;
}

#[tokio::test]
async fn a_legitimate_distant_deadline_is_kept_and_votable() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let closes_at = t + 10 * 365 * 86_400;
    let id = plant_open(tmp.path(), &alice, t - 60, closes_at);

    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    vote(&handle, &id, &alice, VoteChoice::For).await.unwrap();
    // Past one scheduler tick: not closed early, deadline unchanged.
    tokio::time::sleep(Duration::from_secs(12)).await;
    match state_of(&handle, &id).await {
        ProposalState::Open { closes_at: c, .. } => assert_eq!(c, closes_at),
        other => panic!("a distant deadline must not close early: {other:?}"),
    }
    handle.shutdown().await;
}

#[tokio::test]
async fn opening_with_an_unrepresentable_period_is_refused_before_any_transition() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    let ops: &dyn GovernanceOps = &handle;
    let id = ops
        .create_proposal(
            GovernanceDomainId(DOMAIN.to_string()),
            "far".into(),
            "d".into(),
            ProposalPayload::Text { body: "b".into() },
            icn_governance::ProposalScope::Local,
        )
        .await
        .unwrap();
    let err = ops
        .open_proposal(id.clone(), u64::MAX)
        .await
        .expect_err("an unrepresentable period is refused");
    assert!(matches!(
        err.downcast_ref::<GovernanceError>(),
        Some(GovernanceError::UnrepresentableDeadline(_))
    ));
    assert_eq!(state_of(&handle, &id).await, ProposalState::Draft);
    handle.shutdown().await;
}

// ---------------------------------------------------------------------------
// Scheduler-originated close: verified at the actor
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Mutex;

use icn_governance::GovernanceDecisionReceipt;
use icn_governance_actor::institutional_effect::InstitutionalEffectRecord;
use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;
use icn_store::{ContentHash, ReplicaMetadata, Store};

#[derive(Default)]
struct RecordingReceipts {
    governance: Mutex<Vec<GovernanceDecisionReceipt>>,
    /// v3 decision receipts, recorded here rather than through the trait's
    /// fail-closed opaque-storage default, so their absence is observable.
    decisions_v3: Mutex<Vec<icn_governance::GovernanceDecisionReceiptV3>>,
    effects: Mutex<Vec<InstitutionalEffectRecord>>,
    mandates: Mutex<Vec<icn_governance::Mandate>>,
    grants: Mutex<HashMap<icn_governance::AuthorityGrantId, icn_governance::AuthorityGrant>>,
}

impl RecordingReceipts {
    fn mandate_for(&self, proposal_id: &str) -> Option<icn_governance::Mandate> {
        self.mandates
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.decision.proposal_id == proposal_id)
            .cloned()
    }
}

impl GovernanceReceiptBackend for RecordingReceipts {
    fn put_governance_decision_v3(
        &self,
        receipt: &icn_governance::GovernanceDecisionReceiptV3,
        _recorded_at: u64,
    ) -> Result<(), String> {
        self.decisions_v3.lock().unwrap().push(receipt.clone());
        Ok(())
    }
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        let mut g = self.governance.lock().unwrap();
        if !g.iter().any(|r| r.decision_hash == receipt.decision_hash) {
            g.push(receipt.clone());
        }
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.proposal_id == proposal_id)
            .cloned())
    }
    fn put_allocation(
        &self,
        _: &icn_kernel_api::AllocationReceipt,
    ) -> Result<icn_kernel_api::Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        h: &icn_kernel_api::Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance
            .lock()
            .unwrap()
            .iter()
            .find(|r| &r.decision_hash == h)
            .cloned())
    }
    fn list_allocations_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_institutional_effect(&self, record: &InstitutionalEffectRecord) -> Result<(), String> {
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
        self.mandates.lock().unwrap().push(mandate.clone());
        Ok(())
    }
    fn get_mandate_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<icn_governance::Mandate>, String> {
        Ok(self.mandate_for(proposal_id))
    }
    fn list_mandates_by_decision(
        &self,
        h: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_governance::Mandate>, String> {
        Ok(self
            .mandates
            .lock()
            .unwrap()
            .iter()
            .filter(|m| &m.decision.decision_hash == h)
            .cloned()
            .collect())
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
        id: &icn_governance::AuthorityGrantId,
    ) -> Result<Option<icn_governance::AuthorityGrant>, String> {
        Ok(self.grants.lock().unwrap().get(id).cloned())
    }
}

impl RecordingReceipts {
    /// Every authority-bearing or decision write the actor made.
    fn writes(&self) -> usize {
        self.governance.lock().unwrap().len()
            + self.effects.lock().unwrap().len()
            + self.mandates.lock().unwrap().len()
            + self.grants.lock().unwrap().len()
    }
}

/// Sled store whose proposal reads can be made to fail (a transient storage
/// error on exactly the record a close must read).
struct FlakyProposalReads {
    inner: SledStore,
    fail: std::sync::atomic::AtomicBool,
    /// Key prefix whose reads fail while armed.
    prefix: &'static [u8],
}

impl Store for FlakyProposalReads {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) && key.starts_with(self.prefix) {
            anyhow::bail!("injected transient proposal read failure");
        }
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
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

async fn scheduled_close(handle: &GovernanceHandle, id: &ProposalId) -> anyhow::Result<()> {
    handle
        .submit(GovernanceCommand::CloseScheduled {
            proposal_id: id.clone(),
        })
        .await
}

#[tokio::test]
async fn a_scheduled_close_before_the_deadline_is_refused_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 60, t + 3600);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);

    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    let receipts = Arc::new(RecordingReceipts::default());
    handle.install_receipt_store(receipts.clone()).await;

    let err = scheduled_close(&handle, &id).await.expect_err("not due");
    assert!(matches!(
        err.downcast_ref::<GovernanceError>(),
        Some(GovernanceError::CloseNotDue(_))
    ));
    assert!(
        state_of(&handle, &id).await.is_open(),
        "no premature closure"
    );
    assert_eq!(receipts.writes(), 0, "no receipt, mandate, grant or effect");
    handle.shutdown().await;
}

#[tokio::test]
async fn a_transient_read_failure_never_closes_and_the_close_succeeds_after_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);

    let store = Arc::new(FlakyProposalReads {
        inner: SledStore::open(tmp.path()).unwrap(),
        // Armed before spawn: the scheduler's first (immediate) tick cannot
        // close the past-due proposal while the test inspects it.
        fail: std::sync::atomic::AtomicBool::new(true),
        prefix: b"gov:proposal:",
    });
    let node = new_did();
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        node,
        store.clone() as Arc<dyn Store>,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    handle.install_receipt_store(receipts.clone()).await;

    // The proposal record cannot be read: refused, nothing written.
    assert!(scheduled_close(&handle, &id).await.is_err());
    store.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(
        state_of(&handle, &id).await.is_open(),
        "no closure on unreadable evidence"
    );
    assert_eq!(receipts.writes(), 0, "no receipt, mandate, grant or effect");

    // Readable again and past the deadline: it closes, by this call or by the
    // scheduler's own retry, whichever runs first.
    let _ = scheduled_close(&handle, &id).await;
    let s = wait_closed(&handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert!(
        receipts.writes() > 0,
        "the legitimate close records its decision"
    );
    handle.shutdown().await;
}

#[tokio::test]
async fn the_scheduler_retries_through_a_read_outage_and_then_closes() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);

    // Proposal reads fail from before spawn: the rehydrated close fires on the
    // first tick but must neither act nor be lost.
    let store = Arc::new(FlakyProposalReads {
        inner: SledStore::open(tmp.path()).unwrap(),
        fail: std::sync::atomic::AtomicBool::new(true),
        prefix: b"gov:proposal:",
    });
    let node = new_did();
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        node,
        store.clone() as Arc<dyn Store>,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    handle.install_receipt_store(receipts.clone()).await;
    handle.recover_incomplete_closes().await;

    // Past at least one scheduler tick during the outage: nothing written.
    tokio::time::sleep(Duration::from_secs(12)).await;
    assert_eq!(receipts.writes(), 0, "no action on unreadable evidence");

    // Reads recover: the scheduler's own retry closes the proposal.
    store.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    let s = wait_closed(&handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert!(receipts.writes() > 0);
    handle.shutdown().await;
}

#[tokio::test]
async fn a_close_that_fails_after_the_scheduler_check_is_retried_not_lost() {
    // The scheduler's own read and the actor's due-check succeed; the close
    // then fails reading its close-intent record (after it has asked the
    // scheduler to cancel this proposal's events). The retry must survive.
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);

    let store = Arc::new(FlakyProposalReads {
        inner: SledStore::open(tmp.path()).unwrap(),
        fail: std::sync::atomic::AtomicBool::new(true),
        prefix: b"gov:close-intent:",
    });
    let node = new_did();
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        node,
        store.clone() as Arc<dyn Store>,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    handle.install_receipt_store(receipts.clone()).await;
    handle.recover_incomplete_closes().await;

    // The first tick's close fails after the cancel point.
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(state_of(&handle, &id).await.is_open());
    assert_eq!(receipts.writes(), 0);

    // Recovery: the retried event (10 s backoff) closes the proposal.
    store.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    let s = wait_closed(&handle, &id).await;
    assert!(
        matches!(s, ProposalState::Accepted { .. }),
        "the retry was lost: {s:?}"
    );
    handle.shutdown().await;
}

#[tokio::test]
async fn without_a_gateway_the_released_scheduler_still_closes_overdue_proposals() {
    // Gateway disabled by configuration: no evidence path is provisioned, so
    // the supervisor declares it and scheduled closes proceed as before.
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);

    let handle = spawn_actor_unrecovered(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    assert_eq!(
        handle.declare_evidence_not_provisioned(),
        icn_governance_actor::EvidenceReadiness::NotProvisioned
    );
    let s = wait_closed(&handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    handle.shutdown().await;
}

#[tokio::test]
async fn a_rehydrated_close_waits_for_startup_recovery_before_acting() {
    // Production startup order: the actor spawns (rehydrating its schedule)
    // well before the gateway installs the receipt store and then runs
    // `recover_incomplete_closes`. An overdue close must not run in that
    // window: an accepted close with no receipt store installed would finish
    // without writing its decision receipts (the store-gated artifacts are
    // skipped, not journaled), and nothing would remain to replay.
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);

    let handle = spawn_actor_unrecovered(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    // Past at least one scheduler tick, before startup recovery: no close.
    tokio::time::sleep(Duration::from_secs(12)).await;
    let s = state_of(&handle, &id).await;
    assert!(s.is_open(), "closed before startup recovery: {s:?}");

    let receipts = Arc::new(RecordingReceipts::default());
    handle.install_receipt_store(receipts.clone()).await;
    handle.recover_incomplete_closes().await;
    let s = wait_closed(&handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    // What an accepted declarative (Text) decision writes on a scheduled
    // close: exactly its ADR-0014 mandate record, which confers no authority
    // (zero grants). No v1 chain receipt (execution-required payloads only),
    // no v3 receipt (a scheduled close presents no capability scope), no
    // institutional effect (Text has none), no authority grant.
    let mandates = receipts.mandates.lock().unwrap().clone();
    assert_eq!(
        mandates.len(),
        1,
        "exactly one mandate record: {mandates:?}"
    );
    assert_eq!(mandates[0].decision.proposal_id, id.0);
    assert!(
        mandates[0].grants.is_empty(),
        "a Text mandate confers no grant"
    );
    assert!(receipts.grants.lock().unwrap().is_empty());
    assert!(receipts.governance.lock().unwrap().is_empty());
    assert!(receipts.decisions_v3.lock().unwrap().is_empty());
    assert!(receipts.effects.lock().unwrap().is_empty());
    handle.shutdown().await;
}

fn evidence_refused(r: anyhow::Result<()>) -> bool {
    matches!(
        r.err().and_then(|e| e.downcast::<GovernanceError>().ok()),
        Some(GovernanceError::EvidenceUnavailable(_))
    )
}

#[tokio::test]
async fn a_failed_evidence_path_holds_scheduled_work_and_refuses_terminal_transitions() {
    // A configured evidence path failed before recovery: no decision may
    // complete without its evidence, by the scheduler or by hand, while
    // unrelated governance activity continues.
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let overdue = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &overdue, &alice, VoteChoice::For);
    let live = plant_open(tmp.path(), &alice, t - 60, t + 3600);

    let handle = spawn_actor_unrecovered(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    let deliberating = GovernanceOps::create_proposal(
        &handle,
        GovernanceDomainId(DOMAIN.to_string()),
        "deliberating".into(),
        "d".into(),
        ProposalPayload::Text { body: "b".into() },
        icn_governance::ProposalScope::Local,
    )
    .await
    .unwrap();
    assert_eq!(
        handle.mark_evidence_unavailable(),
        icn_governance_actor::EvidenceReadiness::Failed
    );
    // Not downgraded or upgraded by a later disabled-declaration.
    assert_eq!(
        handle.declare_evidence_not_provisioned(),
        icn_governance_actor::EvidenceReadiness::Failed
    );

    // Past at least one scheduler tick: the overdue proposal is preserved.
    tokio::time::sleep(Duration::from_secs(12)).await;
    assert!(state_of(&handle, &overdue).await.is_open());

    // Every terminal transition path is refused, typed.
    assert!(evidence_refused(
        handle
            .submit(GovernanceCommand::CloseProposal {
                proposal_id: overdue.clone(),
                eligible_voters: None,
                excluded_delegators: None,
                capability_scope: None,
            })
            .await
    ));
    assert!(evidence_refused(scheduled_close(&handle, &overdue).await));
    assert!(evidence_refused(
        handle
            .submit(GovernanceCommand::VetoProposal {
                proposal_id: live.clone(),
                reason: "r".into(),
            })
            .await
    ));
    assert!(evidence_refused(
        handle
            .submit(GovernanceCommand::ForceCloseProposal {
                proposal_id: live.clone(),
                forced_outcome: icn_governance::ForcedOutcome::Accept,
                reason: "r".into(),
            })
            .await
    ));
    assert!(state_of(&handle, &overdue).await.is_open());
    assert!(state_of(&handle, &live).await.is_open());

    // Unrelated activity continues: a vote inside the live window, and a
    // scheduled end of deliberation (it writes no evidence) still opens voting.
    vote(&handle, &live, &alice, VoteChoice::For).await.unwrap();
    handle
        .start_deliberation(deliberating.clone(), 1)
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + CLOSE_WAIT;
    let opened = loop {
        let s = state_of(&handle, &deliberating).await;
        if s.is_open() || tokio::time::Instant::now() >= deadline {
            break s;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert!(
        opened.is_open(),
        "deliberation end must not be held: {opened:?}"
    );
    assert!(state_of(&handle, &overdue).await.is_open());
    handle.shutdown().await;
}

#[tokio::test]
async fn a_failure_after_recovery_keeps_governance_ready_and_closing_with_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    let alice = new_did();
    store_with_domain(tmp.path(), std::slice::from_ref(&alice)).await;
    let t = now();
    let id = plant_open(tmp.path(), &alice, t - 7200, t - 60);
    plant_vote(tmp.path(), &id, &alice, VoteChoice::For);

    let handle = spawn_actor_unrecovered(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    let receipts = Arc::new(RecordingReceipts::default());
    handle.install_receipt_store(receipts.clone()).await;
    handle.recover_incomplete_closes().await;
    // A later API failure does not downgrade established evidence.
    assert_eq!(
        handle.mark_evidence_unavailable(),
        icn_governance_actor::EvidenceReadiness::Ready
    );
    let s = wait_closed(&handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert!(receipts.writes() > 0);
    handle.shutdown().await;
}
