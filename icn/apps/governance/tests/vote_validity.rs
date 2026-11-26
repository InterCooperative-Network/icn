//! Vote validity at the governance actor.
//!
//! The actor is the authoritative write boundary for vote rows: the HTTP
//! handler, the JSON-RPC `governance.vote.cast` handler, the `/v1` vote alias
//! and `GovernanceManager::cast_vote` (with a handle attached) all reach
//! `GovernanceCommand::CastVote` through `GovernanceOps::cast_vote`. These tests
//! drive that command through the real actor over a real sled store and
//! require that:
//!
//! * a vote is admitted only for an existing proposal in the `Open` state, cast
//!   by a voting principal the domain's membership resolver names;
//! * a refused vote writes no row;
//! * a member's vote, a member's vote replacement (#2641) and close-time
//!   delegation keep working;
//! * every close path counts only rows whose principal the resolver names, so
//!   the counted votes are drawn from the same electorate as the quorum
//!   denominator — including rows written before admission was enforced, across
//!   a real sled close-and-reopen.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    Delegation, DelegationScope, GovernanceDomainId, GovernanceError, GovernanceOps,
    GovernanceParams, MembershipConfig, ProposalId, ProposalPayload, ProposalScope, ProposalState,
    StaticMembershipResolver, Vote, VoteChoice,
};
use icn_governance_actor::actor::{GovernanceActor, GovernanceHandle};
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_governance_actor::GovernanceCommand;
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;

const DOMAIN: &str = "vote-validity-dom";

fn new_did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

async fn spawn_actor(node: &Did, store: Arc<SledStore>) -> GovernanceHandle {
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    GovernanceActor::spawn(
        node.clone(),
        store,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap()
}

struct Fx {
    handle: GovernanceHandle,
    state: SledGovernanceStateStore,
    node: Did,
    _tmp: tempfile::TempDir,
}

/// One actor over a fresh sled store, with a static-list domain of `members`.
async fn fx(members: &[Did]) -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let node = new_did();
    let store = Arc::new(SledStore::open(tmp.path()).unwrap());
    let handle = spawn_actor(&node, store.clone()).await;
    let state = SledGovernanceStateStore::new(store);
    GovernanceOps::create_domain(
        &handle,
        GovernanceDomainId(DOMAIN.to_string()),
        "Vote validity".to_string(),
        "cooperative_default".to_string(),
        GovernanceParams::new(50, 50, 3600),
        MembershipConfig::static_list(members.to_vec()),
    )
    .await
    .unwrap();
    Fx {
        handle,
        state,
        node,
        _tmp: tmp,
    }
}

async fn draft(handle: &GovernanceHandle, title: &str) -> ProposalId {
    GovernanceOps::create_proposal(
        handle,
        GovernanceDomainId(DOMAIN.to_string()),
        title.to_string(),
        "d".to_string(),
        ProposalPayload::Text { body: "b".into() },
        ProposalScope::Local,
    )
    .await
    .unwrap()
}

async fn opened(handle: &GovernanceHandle, title: &str) -> ProposalId {
    let id = draft(handle, title).await;
    GovernanceOps::open_proposal(handle, id.clone(), 3600)
        .await
        .unwrap();
    id
}

async fn vote(
    handle: &GovernanceHandle,
    id: &ProposalId,
    voter: &Did,
    choice: VoteChoice,
) -> anyhow::Result<()> {
    // Exactly the call the JSON-RPC handler and the manager make.
    let ops: &dyn GovernanceOps = handle;
    ops.cast_vote(id.clone(), voter.clone(), choice, None).await
}

async fn state_of(handle: &GovernanceHandle, id: &ProposalId) -> ProposalState {
    handle.get_proposal(id).await.unwrap().unwrap().state
}

/// The typed governance error carried by an actor refusal.
fn gov_err(r: anyhow::Result<()>) -> GovernanceError {
    let e = r.expect_err("vote must be refused");
    match e.downcast::<GovernanceError>() {
        Ok(g) => g,
        Err(other) => panic!("expected a typed GovernanceError refusal, got: {other:#}"),
    }
}

fn assert_no_rows(f: &Fx, id: &ProposalId) {
    assert!(
        f.state.list_votes(id).unwrap().is_empty(),
        "a refused vote must not write a vote row"
    );
}

// ---------------------------------------------------------------------------
// Admission
// ---------------------------------------------------------------------------

#[tokio::test]
async fn non_member_vote_is_refused_and_writes_nothing() {
    let alice = new_did();
    let mallory = new_did();
    let f = fx(std::slice::from_ref(&alice)).await;
    let id = opened(&f.handle, "p").await;

    let err = gov_err(vote(&f.handle, &id, &mallory, VoteChoice::For).await);
    assert!(
        matches!(err, GovernanceError::NotAMember(_)),
        "expected NotAMember, got {err:?}"
    );
    assert_no_rows(&f, &id);

    // The node's own identity is not a member either: being the node that runs
    // the actor confers no vote.
    let err = gov_err(vote(&f.handle, &id, &f.node, VoteChoice::For).await);
    assert!(matches!(err, GovernanceError::NotAMember(_)), "{err:?}");
    assert_no_rows(&f, &id);
}

#[tokio::test]
async fn vote_on_missing_proposal_is_refused_and_writes_nothing() {
    let alice = new_did();
    let f = fx(std::slice::from_ref(&alice)).await;
    let ghost = ProposalId("no-such-proposal".to_string());

    let err = gov_err(vote(&f.handle, &ghost, &alice, VoteChoice::For).await);
    assert!(
        matches!(err, GovernanceError::ProposalNotFound(ref p) if p == "no-such-proposal"),
        "expected ProposalNotFound, got {err:?}"
    );
    assert_no_rows(&f, &ghost);
}

#[tokio::test]
async fn vote_on_draft_or_deliberation_proposal_is_refused() {
    let alice = new_did();
    let f = fx(std::slice::from_ref(&alice)).await;

    let d = draft(&f.handle, "draft").await;
    let err = gov_err(vote(&f.handle, &d, &alice, VoteChoice::For).await);
    assert!(
        matches!(err, GovernanceError::ProposalNotOpen(_)),
        "{err:?}"
    );
    assert_no_rows(&f, &d);

    let delib = draft(&f.handle, "delib").await;
    f.handle
        .start_deliberation(delib.clone(), 3600)
        .await
        .unwrap();
    assert!(matches!(
        state_of(&f.handle, &delib).await,
        ProposalState::Deliberation { .. }
    ));
    let err = gov_err(vote(&f.handle, &delib, &alice, VoteChoice::For).await);
    assert!(
        matches!(err, GovernanceError::ProposalNotOpen(_)),
        "{err:?}"
    );
    assert_no_rows(&f, &delib);
}

#[tokio::test]
async fn vote_on_closed_proposal_is_refused_and_decision_is_unchanged() {
    let alice = new_did();
    let bob = new_did();
    let f = fx(&[alice.clone(), bob.clone()]).await;
    let id = opened(&f.handle, "p").await;
    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();
    vote(&f.handle, &id, &bob, VoteChoice::For).await.unwrap();
    GovernanceOps::close_proposal(&f.handle, id.clone())
        .await
        .unwrap();
    let closed = state_of(&f.handle, &id).await;
    assert!(
        matches!(closed, ProposalState::Accepted { .. }),
        "{closed:?}"
    );
    let rows_before = f.state.list_votes(&id).unwrap();

    // A member changing their vote after close is refused, and so is a
    // late member who never voted.
    let err = gov_err(vote(&f.handle, &id, &alice, VoteChoice::Against).await);
    assert!(
        matches!(err, GovernanceError::ProposalNotOpen(_)),
        "{err:?}"
    );

    assert_eq!(state_of(&f.handle, &id).await, closed);
    let rows_after = f.state.list_votes(&id).unwrap();
    assert_eq!(
        serde_json::to_value(&rows_before).unwrap(),
        serde_json::to_value(&rows_after).unwrap(),
        "a refused post-close vote must not touch the stored rows"
    );
}

#[tokio::test]
async fn vote_on_vetoed_proposal_is_refused() {
    let alice = new_did();
    let f = fx(std::slice::from_ref(&alice)).await;
    let id = opened(&f.handle, "p").await;
    f.handle
        .submit(GovernanceCommand::VetoProposal {
            proposal_id: id.clone(),
            reason: "test veto".to_string(),
        })
        .await
        .unwrap();
    assert!(matches!(
        state_of(&f.handle, &id).await,
        ProposalState::Vetoed { .. }
    ));

    let err = gov_err(vote(&f.handle, &id, &alice, VoteChoice::For).await);
    assert!(
        matches!(err, GovernanceError::ProposalNotOpen(_)),
        "{err:?}"
    );
    assert_no_rows(&f, &id);
}

// ---------------------------------------------------------------------------
// Controls: legitimate voting keeps working
// ---------------------------------------------------------------------------

#[tokio::test]
async fn member_vote_and_replacement_still_work() {
    let alice = new_did();
    let f = fx(std::slice::from_ref(&alice)).await;
    let id = opened(&f.handle, "p").await;

    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();
    // Changing a vote is this path's existing semantics (#2641): one row,
    // overwritten in place.
    vote(&f.handle, &id, &alice, VoteChoice::Against)
        .await
        .unwrap();

    let rows = f.state.list_votes(&id).unwrap();
    assert_eq!(rows.len(), 1, "one principal, one row");
    assert_eq!(rows[0].voter, alice);
    assert_eq!(rows[0].choice, VoteChoice::Against);

    GovernanceOps::close_proposal(&f.handle, id.clone())
        .await
        .unwrap();
    let s = state_of(&f.handle, &id).await;
    assert!(matches!(s, ProposalState::Rejected { .. }), "{s:?}");
}

#[tokio::test]
async fn delegated_voting_still_counts_at_close() {
    let alice = new_did();
    let bob = new_did();
    let carol = new_did();
    let f = fx(&[alice.clone(), bob.clone(), carol.clone()]).await;
    let id = opened(&f.handle, "p").await;

    // bob and carol delegate to alice for this domain; only alice votes.
    for delegator in [&bob, &carol] {
        f.handle
            .create_delegation(Delegation::new(
                delegator.clone(),
                alice.clone(),
                DelegationScope::Domain(GovernanceDomainId(DOMAIN.to_string())),
            ))
            .await
            .unwrap();
    }
    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();

    GovernanceOps::close_proposal(&f.handle, id.clone())
        .await
        .unwrap();
    // Only alice's direct vote (1 of 3, 33%) would miss the 50% quorum;
    // delegation is what carries it.
    let s = state_of(&f.handle, &id).await;
    assert!(
        matches!(s, ProposalState::Accepted { .. }),
        "delegated weight must still count: {s:?}"
    );
}

// ---------------------------------------------------------------------------
// Rows written before admission was enforced
// ---------------------------------------------------------------------------

/// Write `count` vote rows for principals the domain does not name, exactly
/// as the pre-fix actor would have persisted them.
fn plant_non_member_rows(state: &SledGovernanceStateStore, id: &ProposalId, count: usize) {
    for _ in 0..count {
        let outsider = new_did();
        state
            .save_vote(id, &Vote::new(id.clone(), outsider, VoteChoice::Against))
            .unwrap();
    }
}

#[tokio::test]
async fn stored_non_member_rows_are_not_counted_at_unfiltered_close() {
    let alice = new_did();
    let bob = new_did();
    let f = fx(&[alice.clone(), bob.clone()]).await;
    let id = opened(&f.handle, "p").await;
    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();
    vote(&f.handle, &id, &bob, VoteChoice::For).await.unwrap();
    plant_non_member_rows(&f.state, &id, 3);
    assert_eq!(f.state.list_votes(&id).unwrap().len(), 5);

    // The scheduler, the RPC close and `GovernanceManager::close_proposal`
    // all submit `eligible_voters: None`.
    f.handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await
        .unwrap();

    // Counted: 2 For of 2 members => Accepted. Counting the planted rows
    // would make it 2 For / 3 Against => Rejected.
    let s = state_of(&f.handle, &id).await;
    assert!(
        matches!(s, ProposalState::Accepted { .. }),
        "non-member rows must not be counted: {s:?}"
    );
    // History is not rewritten: the planted rows are still stored.
    assert_eq!(f.state.list_votes(&id).unwrap().len(), 5);
}

#[tokio::test]
async fn stored_non_member_rows_are_not_counted_at_filtered_close() {
    let alice = new_did();
    let bob = new_did();
    let f = fx(&[alice.clone(), bob.clone()]).await;
    let id = opened(&f.handle, "p").await;
    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();
    vote(&f.handle, &id, &bob, VoteChoice::For).await.unwrap();
    let outsider = new_did();
    f.state
        .save_vote(
            &id,
            &Vote::new(id.clone(), outsider.clone(), VoteChoice::Against),
        )
        .unwrap();
    let outsider2 = new_did();
    f.state
        .save_vote(
            &id,
            &Vote::new(id.clone(), outsider2.clone(), VoteChoice::Against),
        )
        .unwrap();
    let outsider3 = new_did();
    f.state
        .save_vote(
            &id,
            &Vote::new(id.clone(), outsider3.clone(), VoteChoice::Against),
        )
        .unwrap();

    // A standing filter that (wrongly or not) names the outsiders still
    // cannot admit a principal the membership resolver does not name.
    let standing: std::collections::HashSet<Did> = [alice, bob, outsider, outsider2, outsider3]
        .into_iter()
        .collect();
    GovernanceOps::close_proposal_filtered(&f.handle, id.clone(), &standing)
        .await
        .unwrap();
    let s = state_of(&f.handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
}

/// Real sled close-and-reopen: rows planted while no actor is running are
/// still excluded by a fresh actor, and admission is still enforced after the
/// restart.
#[tokio::test]
async fn restart_non_member_rows_excluded_and_admission_persists() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_path_buf();
    let node = new_did();
    let alice = new_did();
    let bob = new_did();
    let mallory = new_did();

    let id = {
        let store = Arc::new(SledStore::open(&path).unwrap());
        let handle = spawn_actor(&node, store.clone()).await;
        GovernanceOps::create_domain(
            &handle,
            GovernanceDomainId(DOMAIN.to_string()),
            "Vote validity".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![alice.clone(), bob.clone()]),
        )
        .await
        .unwrap();
        let id = opened(&handle, "p").await;
        vote(&handle, &id, &alice, VoteChoice::For).await.unwrap();
        handle.shutdown().await;
        drop(handle);
        id
    };

    // Offline: plant legacy outsider rows directly in the store.
    {
        let store = Arc::new(SledStore::open(&path).unwrap());
        let state = SledGovernanceStateStore::new(store);
        plant_non_member_rows(&state, &id, 3);
        assert_eq!(state.list_votes(&id).unwrap().len(), 4);
    }

    let store = Arc::new(SledStore::open(&path).unwrap());
    let handle = spawn_actor(&node, store.clone()).await;
    let state = SledGovernanceStateStore::new(store);

    let err = gov_err(vote(&handle, &id, &mallory, VoteChoice::Against).await);
    assert!(matches!(err, GovernanceError::NotAMember(_)), "{err:?}");
    assert_eq!(state.list_votes(&id).unwrap().len(), 4);

    vote(&handle, &id, &bob, VoteChoice::For).await.unwrap();
    GovernanceOps::close_proposal(&handle, id.clone())
        .await
        .unwrap();
    let s = state_of(&handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    handle.shutdown().await;
}

/// Pins the delegation half of the close-time narrowing: only *direct* counting
/// is restricted to the electorate. A member's delegated weight still follows
/// the delegate that member chose, even where that delegate's own row (stored
/// before admission was enforced) is not itself counted. Whether a non-member
/// may serve as a delegate at all is an open maintainer decision; this test
/// records today's behaviour so that a change to it is deliberate.
#[tokio::test]
async fn member_delegation_to_a_non_member_delegate_row_still_carries_weight() {
    let alice = new_did();
    let bob = new_did();
    let carol = new_did();
    let outsider = new_did();
    let f = fx(&[alice.clone(), bob.clone(), carol.clone()]).await;
    let id = opened(&f.handle, "p").await;

    f.handle
        .create_delegation(Delegation::new(
            bob.clone(),
            outsider.clone(),
            DelegationScope::Domain(GovernanceDomainId(DOMAIN.to_string())),
        ))
        .await
        .unwrap();
    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();

    // The outsider cannot cast a vote now...
    let err = gov_err(vote(&f.handle, &id, &outsider, VoteChoice::For).await);
    assert!(matches!(err, GovernanceError::NotAMember(_)), "{err:?}");
    // ...but a row stored before enforcement still sources bob's delegation.
    f.state
        .save_vote(
            &id,
            &Vote::new(id.clone(), outsider.clone(), VoteChoice::For),
        )
        .unwrap();

    GovernanceOps::close_proposal(&f.handle, id.clone())
        .await
        .unwrap();
    // alice direct + bob via delegation = 2 of 3 (quorum met) => Accepted.
    // Without bob's delegated weight it is 1 of 3 => NoQuorum. The outsider's
    // own row is not counted either way.
    let s = state_of(&f.handle, &id).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
}

// ---------------------------------------------------------------------------
// Proposal creation: the domain must exist on this node
// ---------------------------------------------------------------------------

#[tokio::test]
async fn proposal_in_an_unknown_domain_is_refused_and_not_persisted() {
    let alice = new_did();
    let f = fx(std::slice::from_ref(&alice)).await;

    let err = GovernanceOps::create_proposal(
        &f.handle,
        GovernanceDomainId("no-such-domain".to_string()),
        "t".to_string(),
        "d".to_string(),
        ProposalPayload::Text { body: "b".into() },
        ProposalScope::Local,
    )
    .await
    .expect_err("a proposal in an unknown domain must be refused");
    match err.downcast::<GovernanceError>() {
        Ok(GovernanceError::DomainNotFound(d)) => assert_eq!(d, "no-such-domain"),
        other => panic!("expected DomainNotFound, got {other:?}"),
    }
    assert!(
        f.handle.list_proposals().await.unwrap().is_empty(),
        "a refused proposal must not be persisted"
    );

    // Control: the same call against the existing domain succeeds.
    draft(&f.handle, "ok").await;
    assert_eq!(f.handle.list_proposals().await.unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Read-only tally agrees with what a close would count
// ---------------------------------------------------------------------------

#[tokio::test]
async fn read_tally_of_an_open_proposal_counts_only_the_electorate() {
    let alice = new_did();
    let bob = new_did();
    let f = fx(&[alice.clone(), bob.clone()]).await;
    let id = opened(&f.handle, "p").await;
    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();
    vote(&f.handle, &id, &bob, VoteChoice::Abstain)
        .await
        .unwrap();
    plant_non_member_rows(&f.state, &id, 3);

    let t = f.handle.get_vote_tally(&id).await.unwrap();
    assert_eq!(
        (t.for_votes, t.against_votes, t.abstain_votes),
        (1, 0, 1),
        "outsider rows must not appear in the tally a close would not count"
    );
    // Rows are untouched by the read.
    assert_eq!(f.state.list_votes(&id).unwrap().len(), 5);
}

/// A terminal proposal's decision is fixed in its receipt; the read path does
/// not re-derive eligibility for it (the electorate at decision time is not
/// recoverable here). Its read tally stays the stored rows, as before.
#[tokio::test]
async fn read_tally_of_a_closed_proposal_is_not_re_derived() {
    let alice = new_did();
    let f = fx(std::slice::from_ref(&alice)).await;
    let id = opened(&f.handle, "p").await;
    vote(&f.handle, &id, &alice, VoteChoice::For).await.unwrap();
    GovernanceOps::close_proposal(&f.handle, id.clone())
        .await
        .unwrap();
    plant_non_member_rows(&f.state, &id, 2);

    let t = f.handle.get_vote_tally(&id).await.unwrap();
    assert_eq!((t.for_votes, t.against_votes, t.abstain_votes), (1, 2, 0));
}
