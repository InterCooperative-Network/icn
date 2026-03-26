#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Vertical slice integration test: full receipt chain in CI.
//!
//! Proves the 6-link chain end-to-end:
//!   Identity (DID) → Governance (proposal + vote) → Execution Gate
//!   → Effect Translation → Treasury Effects → Audit (receipt provenance)
//!
//! Uses an `Allocation` proposal (participatory budgeting) — the newest
//! governance primitive — to exercise the complete path from proposal
//! creation through to kernel-safe treasury effects.

use anyhow::Result;
use icn_core::{events::SubscriptionHandle, EventBus, SystemEvent};
use icn_gossip::GossipActor;
use icn_governance::{
    AllocationOption, GovernanceDomainId, GovernanceParams, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite};
use icn_identity::KeyPair;
use icn_kernel_api::effects::{KernelEffect, TreasuryEffect};
use icn_store::SledStore;
use std::sync::{Arc, Mutex};

/// Spawn a minimal single-member GovernanceActor wired to an EventBus.
async fn make_actor() -> Result<(
    icn_governance_actor::GovernanceHandle,
    Arc<EventBus>,
    SubscriptionHandle,
    Arc<Mutex<Vec<SystemEvent>>>,
    icn_identity::Did,
)> {
    let alice = KeyPair::generate()?;
    let alice_did = alice.did().clone();

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

    let domain_id = GovernanceDomainId::new("vertical-slice-domain");
    actor
        .submit(GovernanceCommand::CreateDomain {
            domain_id: domain_id.clone(),
            name: "Vertical Slice Test Domain".to_string(),
            config: GovernanceConfigLite {
                profile: "cooperative".to_string(),
                params: GovernanceParams::new(1, 1, 604800),
                membership: MembershipConfig::static_list(vec![alice_did.clone()]),
            },
        })
        .await?;

    Ok((actor, event_bus, sub_handle, captured, alice_did))
}

/// Full receipt chain: Allocation proposal → vote → gate → effects → provenance.
///
/// This is the sprint 15 "demo truth" test. It proves:
/// 1. Identity: Alice has a DID and can propose/vote
/// 2. Governance: Allocation proposal is created, voted on, and accepted
/// 3. Execution Gate: `check_execution_gate` passes → `ProposalAccepted` event emitted
/// 4. Effect Translation: Serialized payload deserializes back to `ProposalPayload::Allocation`
/// 5. Treasury Effects: `translate_payload_to_effects` produces CreateBudget + Allocate (not NoOp)
/// 6. Audit: Decision provenance (receipt ID, hash) threads through to effects
#[tokio::test]
async fn test_allocation_receipt_chain_end_to_end() -> Result<()> {
    let (actor, _bus, _sub, captured, alice_did) = make_actor().await?;
    let domain_id = GovernanceDomainId::new("vertical-slice-domain");
    let proposal_id = ProposalId::generate();

    let recipient_a = alice_did.clone();
    let recipient_b: icn_identity::Did = {
        let kp = KeyPair::generate()?;
        kp.did().clone()
    };

    // Step 1-2: Create Allocation proposal (participatory budgeting)
    actor
        .submit(GovernanceCommand::CreateProposal {
            proposal_id: proposal_id.clone(),
            domain_id: domain_id.clone(),
            title: "Q1 compute allocation".to_string(),
            description: "Distribute compute-hours to infrastructure and education".to_string(),
            payload: ProposalPayload::Allocation {
                pool_amount: 10_000,
                unit: "compute-hours".to_string(),
                options: vec![
                    AllocationOption {
                        label: "Infrastructure".to_string(),
                        description: "Cluster maintenance".to_string(),
                        recipient: recipient_a.clone(),
                        requested_amount: 6_000,
                    },
                    AllocationOption {
                        label: "Education".to_string(),
                        description: "Training programs".to_string(),
                        recipient: recipient_b.clone(),
                        requested_amount: 4_000,
                    },
                ],
                purpose: "q1-compute".to_string(),
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

    // Step 2: Vote (Alice is sole member, 1% quorum → auto-passes)
    actor
        .submit(GovernanceCommand::CastVote {
            proposal_id: proposal_id.clone(),
            choice: VoteChoice::For,
            voter: alice_did.clone(),
            comment: None,
        })
        .await?;

    // Step 3: Close → triggers check_execution_gate → emits ProposalAccepted
    actor
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
        })
        .await?;

    // Step 3 (verify): Exactly one ProposalAccepted event
    let events = captured.lock().expect("lock");
    let accepted: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SystemEvent::ProposalAccepted {
                proposal_id,
                domain_id,
                payload,
                ..
            } => Some((proposal_id.clone(), domain_id.clone(), payload.clone())),
            _ => None,
        })
        .collect();

    assert_eq!(
        accepted.len(),
        1,
        "Expected exactly 1 ProposalAccepted event; got {}",
        accepted.len()
    );

    let (_event_proposal_id, event_domain_id, event_payload) = &accepted[0];

    // Step 4: Deserialize the event payload back to ProposalPayload
    let payload: ProposalPayload =
        serde_json::from_value(event_payload.clone()).expect("payload should deserialize");

    match &payload {
        ProposalPayload::Allocation {
            pool_amount,
            unit,
            options,
            purpose,
        } => {
            assert_eq!(*pool_amount, 10_000);
            assert_eq!(unit, "compute-hours");
            assert_eq!(options.len(), 2);
            assert_eq!(purpose, "q1-compute");
        }
        other => panic!("Expected Allocation payload, got: {other:?}"),
    }

    // Step 5: Translate to kernel effects (the meaning firewall boundary)
    let effects = icn_governance_actor::translate_payload_to_effects(
        &payload,
        "receipt-vertical-slice",
        "decision-hash-vertical-slice",
        event_domain_id.as_str(),
    );

    // 1 CreateBudget + 2 Allocate = 3 effects
    assert_eq!(
        effects.len(),
        3,
        "Expected 3 effects (1 CreateBudget + 2 Allocate); got {effects:?}"
    );

    // No NoOp effects (proves the wiring is complete)
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, KernelEffect::NoOp { .. })),
        "Allocation must not produce NoOp effects"
    );

    // Step 6: Verify audit provenance threads through
    match &effects[0] {
        KernelEffect::Treasury(TreasuryEffect::CreateBudget {
            decision_receipt_id,
            decision_hash,
            total_amount,
            currency,
            ..
        }) => {
            assert_eq!(decision_receipt_id, "receipt-vertical-slice");
            assert_eq!(decision_hash, "decision-hash-vertical-slice");
            assert_eq!(*total_amount, 10_000);
            assert_eq!(currency, "compute-hours");
        }
        other => panic!("First effect should be CreateBudget, got: {other:?}"),
    }

    // Verify individual allocations
    for (i, expected_amount) in [(1, 6_000i64), (2, 4_000i64)] {
        match &effects[i] {
            KernelEffect::Treasury(TreasuryEffect::Allocate {
                amount, currency, ..
            }) => {
                assert_eq!(*amount, expected_amount);
                assert_eq!(currency, "compute-hours");
            }
            other => panic!("Effect {i} should be Allocate, got: {other:?}"),
        }
    }

    Ok(())
}
