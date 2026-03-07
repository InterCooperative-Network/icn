#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests proving Invariant 7 gate enforcement at the actor boundary.
//!
//! These tests verify that `check_execution_gate` is wired into the
//! `GovernanceActor` proposal-closing path:
//!
//! - Accepted proposals: gate passes → `SystemEvent::ProposalAccepted` is emitted
//! - Rejected/NoQuorum proposals: gate not called → `ProposalAccepted` is NOT emitted
//!
//! The gate failure path (gate returns Err on a tampered receipt) is covered by
//! unit tests in `icn_governance::gate::tests`. It cannot be triggered through
//! the actor interface because the actor always constructs the receipt from live
//! vote state — a receipt with a corrupted hash cannot be injected externally
//! without function-level injection (a future improvement if needed).

use anyhow::Result;
use icn_core::{events::SubscriptionHandle, EventBus, SystemEvent};
use icn_gossip::GossipActor;
use icn_governance::{
    GovernanceDomainId, GovernanceParams, MembershipConfig, ProposalId, ProposalPayload,
    ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite};
use icn_identity::KeyPair;
use icn_store::SledStore;
use std::sync::{Arc, Mutex};

/// Spawn a minimal single-member GovernanceActor wired to an EventBus.
///
/// Returns `(actor_handle, event_bus, subscription_handle, captured_events)`.
/// The `SubscriptionHandle` MUST be kept alive in the caller for the duration
/// of the test — dropping it unsubscribes the event capture callback.
async fn make_actor() -> Result<(
    icn_governance_actor::GovernanceHandle,
    Arc<EventBus>,
    SubscriptionHandle,
    Arc<Mutex<Vec<SystemEvent>>>,
)> {
    let alice = KeyPair::generate()?;
    let alice_did = alice.did().clone();

    let store = Arc::new(SledStore::temporary()?);
    let gossip = GossipActor::spawn(alice_did.clone(), None);

    // Create the governance topic the actor publishes to.
    {
        let mut g = gossip.write().await;
        g.create_topic(icn_gossip::Topic::new(
            "governance:proposal".to_string(),
            icn_gossip::AccessControl::Public,
        ));
    }

    let event_bus = Arc::new(EventBus::new());

    // Capture all events
    let captured: Arc<Mutex<Vec<SystemEvent>>> = Arc::new(Mutex::new(vec![]));
    let captured_clone = captured.clone();
    // Keep the handle alive — dropping it auto-unsubscribes.
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
        None, // no signing key
    )
    .await?;

    // Create a domain with Alice as sole member; 1% quorum/approval so a single
    // vote always meets the threshold.
    let domain_id = GovernanceDomainId::new("gate-test-domain");
    actor
        .submit(GovernanceCommand::CreateDomain {
            domain_id: domain_id.clone(),
            name: "Gate Test Domain".to_string(),
            config: GovernanceConfigLite {
                profile: "cooperative".to_string(),
                params: GovernanceParams::new(1, 1, 604800), // 1% quorum, 1% approval
                membership: MembershipConfig::static_list(vec![alice_did]),
            },
        })
        .await?;

    Ok((actor, event_bus, sub_handle, captured))
}

/// Helper: create, open, (optionally vote), and close a proposal.
async fn run_proposal(
    actor: &icn_governance_actor::GovernanceHandle,
    domain_id: &GovernanceDomainId,
    cast_vote: bool,
) -> Result<()> {
    let proposal_id = ProposalId::generate();

    actor
        .submit(GovernanceCommand::CreateProposal {
            proposal_id: proposal_id.clone(),
            domain_id: domain_id.clone(),
            title: "Test proposal".to_string(),
            description: "Gate invariant test".to_string(),
            payload: ProposalPayload::Text {
                body: "Nothing to allocate".to_string(),
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

    if cast_vote {
        actor
            .submit(GovernanceCommand::CastVote {
                proposal_id: proposal_id.clone(),
                choice: VoteChoice::For,
                comment: None,
            })
            .await?;
    }

    actor
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
        })
        .await?;

    Ok(())
}

/// Invariant 7 — happy path:
/// An accepted proposal (vote passes) must emit exactly one `ProposalAccepted` event.
/// This proves `check_execution_gate` is called and allows valid receipts through.
#[tokio::test]
async fn test_gate_passes_for_accepted_proposal() -> Result<()> {
    let (actor, _bus, _sub, captured) = make_actor().await?;
    let domain_id = GovernanceDomainId::new("gate-test-domain");

    run_proposal(&actor, &domain_id, true /* cast For vote */).await?;

    let events = captured.lock().expect("lock");
    let accepted: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, SystemEvent::ProposalAccepted { .. }))
        .collect();

    assert_eq!(
        accepted.len(),
        1,
        "Exactly one ProposalAccepted event expected after a passing vote; got: {}",
        events.len()
    );

    Ok(())
}

/// Invariant 7 — gate not on non-accepted path:
/// A proposal that fails quorum (no votes cast) must NOT emit `ProposalAccepted`.
/// The gate is skipped for Rejected/NoQuorum outcomes — they produce no effects.
#[tokio::test]
async fn test_no_proposal_accepted_event_when_no_quorum() -> Result<()> {
    let (actor, _bus, _sub, captured) = make_actor().await?;
    let domain_id = GovernanceDomainId::new("gate-test-domain");

    run_proposal(&actor, &domain_id, false /* no votes → NoQuorum */).await?;

    let events = captured.lock().expect("lock");
    let accepted: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, SystemEvent::ProposalAccepted { .. }))
        .collect();

    assert_eq!(
        accepted.len(),
        0,
        "No ProposalAccepted event expected for NoQuorum outcome; got {} accepted events",
        accepted.len()
    );

    Ok(())
}
