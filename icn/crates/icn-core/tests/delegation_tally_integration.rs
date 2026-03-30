#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests proving close-time liquid democracy delegation resolution.
//!
//! Liquid democracy: eligible members who did NOT vote directly can delegate
//! their vote to another member. At proposal close time, the actor resolves
//! delegation chains and applies the delegate's vote choice on behalf of
//! absent members.
//!
//! These tests prove:
//! 1. **Delegation applies**: absent member's weight flows through their delegate
//! 2. **Direct vote wins**: if both delegator and delegate vote, delegator is unaffected
//! 3. **No-delegation baseline**: absent members without delegation produce no vote

use anyhow::Result;
use icn_core::{events::SubscriptionHandle, EventBus, SystemEvent};
use icn_gossip::GossipActor;
use icn_governance::{
    Delegation, DelegationScope, GovernanceDomainId, GovernanceParams, MembershipConfig,
    ProposalId, ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite};
use icn_identity::KeyPair;
use icn_store::SledStore;
use std::sync::{Arc, Mutex};

/// Spawn a 3-member GovernanceActor with configurable quorum/approval thresholds.
///
/// Members: alice (passed in), bob, carol — all in "delegation-test-domain".
/// Returns `(handle, captured_events, alice_did, bob_did, carol_did, domain_id)`.
async fn make_three_member_actor(
    quorum_pct: u8,
    approval_pct: u8,
) -> Result<(
    icn_governance_actor::GovernanceHandle,
    Arc<Mutex<Vec<SystemEvent>>>,
    SubscriptionHandle,
    icn_identity::Did,
    icn_identity::Did,
    icn_identity::Did,
    GovernanceDomainId,
)> {
    let alice = KeyPair::generate()?;
    let bob = KeyPair::generate()?;
    let carol = KeyPair::generate()?;
    let alice_did = alice.did().clone();
    let bob_did = bob.did().clone();
    let carol_did = carol.did().clone();

    let store = Arc::new(SledStore::temporary()?);
    let gossip = GossipActor::spawn(alice_did.clone(), None);
    {
        let mut g = gossip.write().await;
        g.create_topic(icn_gossip::Topic::new(
            "governance:proposal".to_string(),
            icn_gossip::AccessControl::Public,
        ));
    }

    let event_bus = Arc::new(EventBus::new());
    let captured: Arc<Mutex<Vec<SystemEvent>>> = Arc::new(Mutex::new(vec![]));
    let captured_clone = captured.clone();
    let sub_handle = event_bus
        .subscribe(Arc::new(move |event| {
            captured_clone.lock().expect("lock").push(event);
        }))
        .await;

    let resolver = Arc::new(StaticMembershipResolver::new());
    let actor = GovernanceActor::spawn(
        alice_did.clone(),
        store,
        gossip,
        resolver,
        Some(event_bus.clone()),
        None,
    )
    .await?;

    let domain_id = GovernanceDomainId::new("delegation-test-domain");
    actor
        .submit(GovernanceCommand::CreateDomain {
            domain_id: domain_id.clone(),
            name: "Delegation Test Domain".to_string(),
            config: GovernanceConfigLite {
                profile: "cooperative".to_string(),
                params: GovernanceParams::new(quorum_pct, approval_pct, 604_800),
                membership: MembershipConfig::static_list(vec![
                    alice_did.clone(),
                    bob_did.clone(),
                    carol_did.clone(),
                ]),
            },
        })
        .await?;

    Ok((
        actor, captured, sub_handle, alice_did, bob_did, carol_did, domain_id,
    ))
}

/// Create, open, and optionally vote on a proposal, then close it.
async fn lifecycle_proposal(
    actor: &icn_governance_actor::GovernanceHandle,
    domain_id: &GovernanceDomainId,
    voters: &[(icn_identity::Did, VoteChoice)],
) -> Result<ProposalId> {
    let proposal_id = ProposalId::generate();

    actor
        .submit(GovernanceCommand::CreateProposal {
            proposal_id: proposal_id.clone(),
            domain_id: domain_id.clone(),
            title: "Delegation test proposal".to_string(),
            description: "Tests liquid democracy at close time".to_string(),
            payload: ProposalPayload::Text {
                body: "Approve test motion".to_string(),
            },
            scope: ProposalScope::Local,
        })
        .await?;

    actor
        .submit(GovernanceCommand::OpenProposal {
            proposal_id: proposal_id.clone(),
            voting_period_seconds: 3600,
        })
        .await?;

    for (voter, choice) in voters {
        actor
            .submit(GovernanceCommand::CastVote {
                proposal_id: proposal_id.clone(),
                choice: *choice,
                voter: voter.clone(),
                comment: None,
            })
            .await?;
    }

    actor
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
        })
        .await?;

    Ok(proposal_id)
}

/// Count how many `ProposalAccepted` events have been captured.
fn accepted_count(captured: &Arc<Mutex<Vec<SystemEvent>>>) -> usize {
    captured
        .lock()
        .expect("lock")
        .iter()
        .filter(|e| matches!(e, SystemEvent::ProposalAccepted { .. }))
        .count()
}

/// --- Test 1: Delegation raises quorum past threshold ---
///
/// Setup:
/// - 3 members: Alice, Bob, Carol
/// - Quorum = 67%, approval = 50%
/// - Bob delegates to Alice (blanket scope)
/// - Only Alice votes For; Bob and Carol do not vote
///
/// Quorum math (integer division, strictly-less check):
///   quorum_required = (3 * 67) / 100 = 2
///
/// Without delegation:
///   votes = 1/3, total_votes=1 < quorum_required=2 → NoQuorum (no ProposalAccepted)
///
/// With delegation (close-time resolution):
///   Bob's absent vote is attributed to Alice's For choice
///   effective votes = 2 (Alice direct + Bob delegated For)
///   total_votes=2, quorum_required=2 → 2 < 2 is FALSE → quorum met
///   for_votes=2, approval_required=(2*50)/100=1, 2>1 → Accepted
#[tokio::test]
async fn test_delegation_raises_participation_above_quorum() -> Result<()> {
    let (actor, captured, _sub, alice_did, bob_did, _carol_did, domain_id) =
        make_three_member_actor(67, 50).await?;

    // Bob delegates to Alice (blanket scope)
    let delegation = Delegation::new(bob_did.clone(), alice_did.clone(), DelegationScope::Blanket);
    actor
        .submit(GovernanceCommand::CreateDelegation {
            delegation: delegation.clone(),
        })
        .await?;

    // Only Alice votes; Bob (delegated) and Carol (no delegation) do not
    lifecycle_proposal(&actor, &domain_id, &[(alice_did.clone(), VoteChoice::For)]).await?;

    assert_eq!(
        accepted_count(&captured),
        1,
        "Delegation should raise Bob's vote to Alice's For, reaching 2/3 = 66.7% ≥ 60% quorum"
    );

    Ok(())
}

/// --- Test 2: Without delegation, same setup fails quorum ---
///
/// Same as Test 1 but Bob has NO delegation. Only Alice votes.
/// quorum_required = (3 * 67) / 100 = 2; total_votes=1 < 2 → NoQuorum
/// ProposalAccepted NOT emitted.
#[tokio::test]
async fn test_no_delegation_fails_quorum() -> Result<()> {
    let (actor, captured, _sub, alice_did, _bob_did, _carol_did, domain_id) =
        make_three_member_actor(67, 50).await?;

    // No delegations created — Alice votes alone
    lifecycle_proposal(&actor, &domain_id, &[(alice_did.clone(), VoteChoice::For)]).await?;

    assert_eq!(
        accepted_count(&captured),
        0,
        "Without delegation, 1/3 votes = 33.3% < 60% quorum → no ProposalAccepted"
    );

    Ok(())
}

/// --- Test 3: Direct vote takes precedence over delegation ---
///
/// Bob delegates to Carol (Against), but Bob also votes For directly.
/// Bob's direct For vote must NOT be replaced by Carol's Against choice.
///
/// Setup: 3 members, quorum=67%, approval=50%
///   All 3 vote: Alice For, Bob For (direct), Carol Against
///   quorum_required = (3*67)/100 = 2; total_votes=3 ≥ 2 → quorum met
///   for_votes=2, approval_required=(3*50)/100=1; 2>1 → Accepted
///
/// If delegation INCORRECTLY replaced Bob's direct For with Carol's Against:
///   for_votes=1 (only Alice), approval_required=1; 1>1 → FALSE → Rejected
///   (observable behavior change proves direct-vote-wins logic is working)
#[tokio::test]
async fn test_direct_vote_overrides_delegation() -> Result<()> {
    let (actor, captured, _sub, alice_did, bob_did, carol_did, domain_id) =
        make_three_member_actor(67, 50).await?;

    // Bob delegates to Carol (who will vote Against), but Bob will vote For directly
    let delegation = Delegation::new(bob_did.clone(), carol_did.clone(), DelegationScope::Blanket);
    actor
        .submit(GovernanceCommand::CreateDelegation { delegation })
        .await?;

    // All three vote: Alice For, Bob For (direct overrides delegation), Carol Against
    lifecycle_proposal(
        &actor,
        &domain_id,
        &[
            (alice_did.clone(), VoteChoice::For),
            (bob_did.clone(), VoteChoice::For),
            (carol_did.clone(), VoteChoice::Against),
        ],
    )
    .await?;

    // Correct: For=2 (Alice+Bob direct), Against=1 (Carol), 2>1 approval → Accepted
    // Buggy (if delegation replaced Bob's For with Carol's Against): For=1, 1>1 → Rejected
    assert_eq!(
        accepted_count(&captured),
        1,
        "Bob's direct For vote should not be replaced by Carol's Against via delegation"
    );

    Ok(())
}

/// --- Test 4: Standing revalidation blocks delegation for ineligible members ---
///
/// A member who LOST standing AND delegated their vote should NOT have their
/// absent weight flow through delegation when standing revalidation is active.
///
/// This tests the interaction between PR #1461 (standing revalidation) and
/// PR #1462 (delegation resolution).
///
/// Setup: 3 members, quorum=67%, approval=50%
///   Bob loses standing before close (simulated via eligible_voters filter)
///   Bob delegated to Alice (who votes For)
///   Only Alice votes directly
///
/// With correct interaction:
///   eligible_voters = {Alice, Carol} (Bob excluded — lost standing)
///   delegation_scope = intersection of eligible_members ∩ eligible_voters = {Alice, Carol}
///   Bob is NOT in delegation_scope → delegation NOT applied for Bob
///   effective votes = 1 (Alice direct For only)
///   quorum_required = (3*67)/100 = 2; 1 < 2 → NoQuorum → no ProposalAccepted
///
/// Without the fix (buggy):
///   delegation_scope = all eligible_members including Bob
///   Bob's delegation to Alice fires → For=2 (Alice direct + Bob delegated)
///   2 >= 2 quorum → Accepted (wrong!)
#[tokio::test]
async fn test_lost_standing_member_delegation_blocked() -> Result<()> {
    let (actor, captured, _sub, alice_did, bob_did, carol_did, domain_id) =
        make_three_member_actor(67, 50).await?;

    // Bob delegates to Alice before losing standing
    let delegation = Delegation::new(bob_did.clone(), alice_did.clone(), DelegationScope::Blanket);
    actor
        .submit(GovernanceCommand::CreateDelegation { delegation })
        .await?;

    // Create and open proposal
    let proposal_id = ProposalId::generate();
    actor
        .submit(GovernanceCommand::CreateProposal {
            proposal_id: proposal_id.clone(),
            domain_id: domain_id.clone(),
            title: "Standing-delegation interaction test".to_string(),
            description: "Bob lost standing; should not delegate".to_string(),
            payload: ProposalPayload::Text {
                body: "Test motion".to_string(),
            },
            scope: ProposalScope::Local,
        })
        .await?;
    actor
        .submit(GovernanceCommand::OpenProposal {
            proposal_id: proposal_id.clone(),
            voting_period_seconds: 3600,
        })
        .await?;

    // Only Alice votes
    actor
        .submit(GovernanceCommand::CastVote {
            proposal_id: proposal_id.clone(),
            choice: VoteChoice::For,
            voter: alice_did.clone(),
            comment: None,
        })
        .await?;

    // Simulate standing revalidation: Bob lost standing before close.
    // Only Alice and Carol are currently eligible (Bob excluded).
    let mut eligible = std::collections::HashSet::new();
    eligible.insert(alice_did.clone());
    eligible.insert(carol_did.clone());

    actor
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: Some(eligible),
        })
        .await?;

    // With correct behavior: Bob's delegation is blocked (he lost standing).
    // Only Alice's direct For vote counts. 1 < quorum_required(2) → NoQuorum.
    assert_eq!(
        accepted_count(&captured),
        0,
        "Member who lost standing must not have delegation applied — their absence \
         should contribute to quorum failure, not be covered by a pre-existing delegation"
    );

    Ok(())
}
