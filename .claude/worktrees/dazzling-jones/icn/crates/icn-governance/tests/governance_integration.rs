//! Integration tests for icn-governance.
//!
//! These tests verify the complete governance flow including
//! domain management, proposal lifecycle, voting, and tallying.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceDomainId, GovernanceStore,
    InMemoryGovernanceStore, MembershipAction, Proposal, ProposalId, ProposalOutcome,
    ProposalPayload, ProposalState, Vote, VoteChoice, VoteTally,
};
use icn_identity::KeyPair;

// =============================================================================
// Domain Management Tests
// =============================================================================

#[test]
fn test_domain_creation_with_cooperative_defaults() {
    let config = GovernanceConfig::cooperative_default();
    let domain = GovernanceDomain::new("Worker Coop".to_string(), config.clone());

    assert_eq!(domain.name, "Worker Coop");
    assert!(domain.description.is_none());
    assert!(domain.created_at > 0);
    assert_eq!(domain.created_at, domain.updated_at);

    // Cooperative defaults should have reasonable values
    assert!(config.params.voting_period_seconds > 0);
    assert!(config.params.quorum_percentage > 0);
    assert!(config.params.approval_threshold_percentage > 0);
}

#[test]
fn test_domain_with_description() {
    let config = GovernanceConfig::cooperative_default();
    let domain = GovernanceDomain::new("Tech Coop".to_string(), config)
        .with_description("A technology workers cooperative".to_string());

    assert_eq!(domain.name, "Tech Coop");
    assert_eq!(
        domain.description,
        Some("A technology workers cooperative".to_string())
    );
}

#[test]
fn test_domain_config_update() {
    let config1 = GovernanceConfig::cooperative_default();
    let mut domain = GovernanceDomain::new("Coop".to_string(), config1);

    let original_updated = domain.updated_at;
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Update config
    let mut config2 = GovernanceConfig::cooperative_default();
    config2.params.quorum_percentage = 75;
    domain.update_config(config2.clone());

    assert_eq!(domain.config.params.quorum_percentage, 75);
    assert!(domain.updated_at >= original_updated);
}

#[test]
fn test_domain_id_generation() {
    let id1 = GovernanceDomainId::generate();
    let id2 = GovernanceDomainId::generate();

    // Generated IDs should be unique
    assert_ne!(id1.0, id2.0);

    // Custom ID should work
    let custom_id = GovernanceDomainId::new("my-coop");
    assert_eq!(custom_id.0, "my-coop");
    assert_eq!(format!("{custom_id}"), "my-coop");
}

// =============================================================================
// Proposal Lifecycle Tests
// =============================================================================

#[test]
fn test_proposal_creation() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let proposal = Proposal::new(
        domain_id.clone(),
        did.clone(),
        "Budget Proposal".to_string(),
        "Allocate funds for equipment".to_string(),
        ProposalPayload::Text {
            body: "Should we allocate $5000 for new equipment?".to_string(),
        },
    );

    assert_eq!(proposal.title, "Budget Proposal");
    assert_eq!(proposal.domain_id, domain_id);
    assert_eq!(proposal.proposer, did);
    assert_eq!(proposal.state, ProposalState::Draft);
    assert!(!proposal.state.is_open());
    assert!(!proposal.state.is_closed());
}

#[test]
fn test_proposal_open_for_voting() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    // Open for 1 hour
    proposal.open(3600).unwrap();

    assert!(proposal.state.is_open());
    assert!(!proposal.state.is_closed());
    assert!(proposal.state.closes_at().is_some());

    let closes_at = proposal.state.closes_at().unwrap();
    assert!(closes_at > proposal.created_at);
}

#[test]
fn test_proposal_cannot_reopen() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    proposal.open(3600).unwrap();

    // Cannot open again
    let result = proposal.open(3600);
    assert!(result.is_err());
}

#[test]
fn test_proposal_close_accepted() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    proposal.open(3600).unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    proposal
        .close(ProposalState::Accepted { closed_at: now })
        .unwrap();

    assert!(proposal.state.is_closed());
    assert!(!proposal.state.is_open());
    assert!(matches!(proposal.state, ProposalState::Accepted { .. }));
}

#[test]
fn test_proposal_close_rejected() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    proposal.open(3600).unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    proposal
        .close(ProposalState::Rejected { closed_at: now })
        .unwrap();

    assert!(matches!(proposal.state, ProposalState::Rejected { .. }));
}

#[test]
fn test_proposal_close_no_quorum() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    proposal.open(3600).unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    proposal
        .close(ProposalState::NoQuorum { closed_at: now })
        .unwrap();

    assert!(matches!(proposal.state, ProposalState::NoQuorum { .. }));
}

#[test]
fn test_proposal_cancellation() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    // Can cancel from Draft
    proposal.cancel().unwrap();
    assert!(matches!(proposal.state, ProposalState::Cancelled { .. }));
    assert!(proposal.state.is_closed());

    // Cannot cancel after already cancelled
    assert!(proposal.cancel().is_err());
}

#[test]
fn test_proposal_veto_from_draft() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Controversial Proposal".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    proposal
        .veto("Security vulnerability detected".to_string())
        .unwrap();

    assert!(proposal.state.is_closed());
    if let ProposalState::Vetoed { reason, .. } = &proposal.state {
        assert_eq!(reason, "Security vulnerability detected");
    } else {
        panic!("Expected Vetoed state");
    }
}

#[test]
fn test_proposal_veto_from_open() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    proposal.open(3600).unwrap();
    proposal.veto("Emergency action".to_string()).unwrap();

    assert!(proposal.state.is_closed());
    assert!(matches!(proposal.state, ProposalState::Vetoed { .. }));
}

#[test]
fn test_proposal_force_close() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let mut proposal = Proposal::new(
        domain_id,
        did,
        "Test".to_string(),
        "Test".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );

    // Cannot force close from Draft
    assert!(proposal
        .force_close(ProposalOutcome::Accepted, "Emergency".to_string())
        .is_err());

    proposal.open(3600).unwrap();

    // Can force close from Open
    proposal
        .force_close(ProposalOutcome::Rejected, "Emergency rejection".to_string())
        .unwrap();

    if let ProposalState::ForceClosed {
        outcome, reason, ..
    } = &proposal.state
    {
        assert_eq!(*outcome, ProposalOutcome::Rejected);
        assert_eq!(reason, "Emergency rejection");
    } else {
        panic!("Expected ForceClosed state");
    }
}

// =============================================================================
// Voting Tests
// =============================================================================

#[test]
fn test_vote_creation() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let proposal_id = ProposalId::generate();

    let vote = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::For);

    assert_eq!(vote.proposal_id, proposal_id);
    assert_eq!(vote.voter, did);
    assert_eq!(vote.choice, VoteChoice::For);
    assert_eq!(vote.weight, 1);
    assert!(vote.timestamp > 0);
    assert!(vote.comment.is_none());
}

#[test]
fn test_vote_with_comment() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let proposal_id = ProposalId::generate();

    let vote = Vote::new(proposal_id, did, VoteChoice::Against)
        .with_comment("I disagree because...".to_string());

    assert_eq!(vote.choice, VoteChoice::Against);
    assert_eq!(vote.comment, Some("I disagree because...".to_string()));
}

#[test]
fn test_vote_with_weight() {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let proposal_id = ProposalId::generate();

    let vote = Vote::new(proposal_id, did, VoteChoice::For).with_weight(5);

    assert_eq!(vote.weight, 5);
}

// =============================================================================
// Tally Tests
// =============================================================================

#[test]
fn test_empty_tally() {
    let tally = VoteTally::empty();

    assert_eq!(tally.for_votes, 0);
    assert_eq!(tally.against_votes, 0);
    assert_eq!(tally.abstain_votes, 0);
    assert_eq!(tally.total_votes(), 0);
    assert_eq!(tally.deciding_votes(), 0);
    assert_eq!(tally.approval_percentage(), 0);
}

#[test]
fn test_tally_from_votes() {
    let proposal_id = ProposalId::generate();

    let mut votes = Vec::new();
    for _ in 0..3 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::For,
        ));
    }
    for _ in 0..2 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::Against,
        ));
    }
    let kp = KeyPair::generate().unwrap();
    votes.push(Vote::new(
        proposal_id.clone(),
        kp.did().clone(),
        VoteChoice::Abstain,
    ));

    let tally = VoteTally::from(votes);

    assert_eq!(tally.for_votes, 3);
    assert_eq!(tally.against_votes, 2);
    assert_eq!(tally.abstain_votes, 1);
    assert_eq!(tally.total_votes(), 6);
    assert_eq!(tally.deciding_votes(), 5);
    assert_eq!(tally.approval_percentage(), 60); // 3/5 = 60%
}

#[test]
fn test_tally_with_weighted_votes() {
    let proposal_id = ProposalId::generate();

    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let votes = vec![
        Vote::new(proposal_id.clone(), kp1.did().clone(), VoteChoice::For).with_weight(10),
        Vote::new(proposal_id.clone(), kp2.did().clone(), VoteChoice::Against).with_weight(5),
    ];

    let tally = VoteTally::from(votes);

    assert_eq!(tally.for_votes, 10);
    assert_eq!(tally.against_votes, 5);
    assert_eq!(tally.total_votes(), 15);
    assert_eq!(tally.approval_percentage(), 66); // 10/15 = 66%
}

#[test]
fn test_tally_unanimous() {
    let proposal_id = ProposalId::generate();

    let mut votes = Vec::new();
    for _ in 0..5 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::For,
        ));
    }

    let tally = VoteTally::from(votes);

    assert_eq!(tally.for_votes, 5);
    assert_eq!(tally.against_votes, 0);
    assert_eq!(tally.approval_percentage(), 100);
}

#[test]
fn test_tally_all_abstain() {
    let proposal_id = ProposalId::generate();

    let mut votes = Vec::new();
    for _ in 0..3 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::Abstain,
        ));
    }

    let tally = VoteTally::from(votes);

    assert_eq!(tally.total_votes(), 3);
    assert_eq!(tally.deciding_votes(), 0);
    assert_eq!(tally.approval_percentage(), 0);
}

// =============================================================================
// Store Integration Tests
// =============================================================================

#[test]
fn test_store_domain_roundtrip() {
    let store = InMemoryGovernanceStore::new();
    let config = GovernanceConfig::cooperative_default();
    let domain = GovernanceDomain::new("Test Coop".to_string(), config)
        .with_description("A test".to_string());

    store.store_domain(&domain).unwrap();

    let retrieved = store.get_domain(&domain.id).unwrap().unwrap();
    assert_eq!(retrieved.name, "Test Coop");
    assert_eq!(retrieved.description, Some("A test".to_string()));

    let all_domains = store.list_domains().unwrap();
    assert_eq!(all_domains.len(), 1);
}

#[test]
fn test_store_multiple_domains() {
    let store = InMemoryGovernanceStore::new();

    for i in 0..5 {
        let config = GovernanceConfig::cooperative_default();
        let domain = GovernanceDomain::new(format!("Coop {i}"), config);
        store.store_domain(&domain).unwrap();
    }

    let domains = store.list_domains().unwrap();
    assert_eq!(domains.len(), 5);
}

#[test]
fn test_store_proposal_roundtrip() {
    let store = InMemoryGovernanceStore::new();
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let domain_id = GovernanceDomainId::new("test-domain");

    let proposal = Proposal::new(
        domain_id.clone(),
        did,
        "Test Proposal".to_string(),
        "Description".to_string(),
        ProposalPayload::Text {
            body: "Should we do this?".to_string(),
        },
    );

    store.store_proposal(&proposal).unwrap();

    let retrieved = store.get_proposal(&proposal.id).unwrap().unwrap();
    assert_eq!(retrieved.title, "Test Proposal");

    let proposals = store.list_proposals(&domain_id).unwrap();
    assert_eq!(proposals.len(), 1);
}

#[test]
fn test_store_proposals_by_domain() {
    let store = InMemoryGovernanceStore::new();
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();

    let domain_a = GovernanceDomainId::new("domain-a");
    let domain_b = GovernanceDomainId::new("domain-b");

    // Add 3 proposals to domain A
    for i in 0..3 {
        let proposal = Proposal::new(
            domain_a.clone(),
            did.clone(),
            format!("Proposal A{i}"),
            "Desc".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );
        store.store_proposal(&proposal).unwrap();
    }

    // Add 2 proposals to domain B
    for i in 0..2 {
        let proposal = Proposal::new(
            domain_b.clone(),
            did.clone(),
            format!("Proposal B{i}"),
            "Desc".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );
        store.store_proposal(&proposal).unwrap();
    }

    assert_eq!(store.list_proposals(&domain_a).unwrap().len(), 3);
    assert_eq!(store.list_proposals(&domain_b).unwrap().len(), 2);
}

#[test]
fn test_store_vote_roundtrip() {
    let store = InMemoryGovernanceStore::new();
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let proposal_id = ProposalId::generate();

    let vote = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::For);
    store.store_vote(&vote).unwrap();

    let retrieved = store.get_vote(&proposal_id, &did).unwrap().unwrap();
    assert_eq!(retrieved.choice, VoteChoice::For);

    let all_votes = store.list_votes(&proposal_id).unwrap();
    assert_eq!(all_votes.len(), 1);
}

#[test]
fn test_store_vote_replacement() {
    let store = InMemoryGovernanceStore::new();
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    let proposal_id = ProposalId::generate();

    // Vote For
    let vote1 = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::For);
    store.store_vote(&vote1).unwrap();

    // Change to Against
    let vote2 = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::Against);
    store.store_vote(&vote2).unwrap();

    // Should only have one vote
    let votes = store.list_votes(&proposal_id).unwrap();
    assert_eq!(votes.len(), 1);
    assert_eq!(votes[0].choice, VoteChoice::Against);

    // Tally should reflect the change
    let tally = store.compute_tally(&proposal_id).unwrap();
    assert_eq!(tally.for_votes, 0);
    assert_eq!(tally.against_votes, 1);
}

#[test]
fn test_store_compute_tally() {
    let store = InMemoryGovernanceStore::new();
    let proposal_id = ProposalId::generate();

    // Add multiple votes
    for _ in 0..4 {
        let kp = KeyPair::generate().unwrap();
        store
            .store_vote(&Vote::new(
                proposal_id.clone(),
                kp.did().clone(),
                VoteChoice::For,
            ))
            .unwrap();
    }

    for _ in 0..2 {
        let kp = KeyPair::generate().unwrap();
        store
            .store_vote(&Vote::new(
                proposal_id.clone(),
                kp.did().clone(),
                VoteChoice::Against,
            ))
            .unwrap();
    }

    let kp = KeyPair::generate().unwrap();
    store
        .store_vote(&Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::Abstain,
        ))
        .unwrap();

    let tally = store.compute_tally(&proposal_id).unwrap();
    assert_eq!(tally.for_votes, 4);
    assert_eq!(tally.against_votes, 2);
    assert_eq!(tally.abstain_votes, 1);
    assert_eq!(tally.total_votes(), 7);
}

// =============================================================================
// Complete Governance Flow Tests
// =============================================================================

#[test]
fn test_complete_governance_flow_accepted() {
    let store = InMemoryGovernanceStore::new();

    // 1. Create domain
    let config = GovernanceConfig::cooperative_default();
    let domain = GovernanceDomain::new("Worker Coop".to_string(), config);
    store.store_domain(&domain).unwrap();

    // 2. Create proposal
    let proposer_kp = KeyPair::generate().unwrap();
    let mut proposal = Proposal::new(
        domain.id.clone(),
        proposer_kp.did().clone(),
        "Budget Allocation".to_string(),
        "Allocate funds for equipment".to_string(),
        ProposalPayload::Budget {
            amount: 5000,
            currency: "USD".to_string(),
            recipient: proposer_kp.did().clone(),
            purpose: "New equipment".to_string(),
        },
    );
    store.store_proposal(&proposal).unwrap();

    // 3. Open for voting
    proposal.open(3600).unwrap();
    store.store_proposal(&proposal).unwrap();

    // 4. Members vote (4 for, 1 against)
    for _ in 0..4 {
        let kp = KeyPair::generate().unwrap();
        let vote = Vote::new(proposal.id.clone(), kp.did().clone(), VoteChoice::For);
        store.store_vote(&vote).unwrap();
    }
    let against_kp = KeyPair::generate().unwrap();
    store
        .store_vote(&Vote::new(
            proposal.id.clone(),
            against_kp.did().clone(),
            VoteChoice::Against,
        ))
        .unwrap();

    // 5. Compute tally
    let tally = store.compute_tally(&proposal.id).unwrap();
    assert_eq!(tally.for_votes, 4);
    assert_eq!(tally.against_votes, 1);
    assert_eq!(tally.approval_percentage(), 80);

    // 6. Close as accepted (80% > 50% threshold)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    proposal
        .close(ProposalState::Accepted { closed_at: now })
        .unwrap();
    store.store_proposal(&proposal).unwrap();

    // Verify final state
    let final_proposal = store.get_proposal(&proposal.id).unwrap().unwrap();
    assert!(matches!(
        final_proposal.state,
        ProposalState::Accepted { .. }
    ));
}

#[test]
fn test_complete_governance_flow_rejected() {
    let store = InMemoryGovernanceStore::new();

    let config = GovernanceConfig::cooperative_default();
    let domain = GovernanceDomain::new("Coop".to_string(), config);
    store.store_domain(&domain).unwrap();

    let kp = KeyPair::generate().unwrap();
    let mut proposal = Proposal::new(
        domain.id.clone(),
        kp.did().clone(),
        "Controversial Change".to_string(),
        "Description".to_string(),
        ProposalPayload::Text {
            body: "Test".to_string(),
        },
    );
    store.store_proposal(&proposal).unwrap();
    proposal.open(3600).unwrap();
    store.store_proposal(&proposal).unwrap();

    // Vote against (1 for, 4 against)
    let for_kp = KeyPair::generate().unwrap();
    store
        .store_vote(&Vote::new(
            proposal.id.clone(),
            for_kp.did().clone(),
            VoteChoice::For,
        ))
        .unwrap();

    for _ in 0..4 {
        let against_kp = KeyPair::generate().unwrap();
        store
            .store_vote(&Vote::new(
                proposal.id.clone(),
                against_kp.did().clone(),
                VoteChoice::Against,
            ))
            .unwrap();
    }

    let tally = store.compute_tally(&proposal.id).unwrap();
    assert_eq!(tally.approval_percentage(), 20);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    proposal
        .close(ProposalState::Rejected { closed_at: now })
        .unwrap();

    assert!(matches!(proposal.state, ProposalState::Rejected { .. }));
}

#[test]
fn test_membership_proposal() {
    let kp = KeyPair::generate().unwrap();
    let proposer = kp.did().clone();
    let new_member = KeyPair::generate().unwrap();
    let domain_id = GovernanceDomainId::new("test-domain");

    let proposal = Proposal::new(
        domain_id,
        proposer,
        "Add New Member".to_string(),
        "Proposal to add a new member".to_string(),
        ProposalPayload::Membership {
            action: MembershipAction::Add,
            member: new_member.did().clone(),
        },
    );

    if let ProposalPayload::Membership { action, member } = &proposal.payload {
        assert_eq!(*action, MembershipAction::Add);
        assert_eq!(*member, new_member.did().clone());
    } else {
        panic!("Expected Membership payload");
    }
}

#[test]
fn test_config_change_proposal() {
    let kp = KeyPair::generate().unwrap();
    let domain_id = GovernanceDomainId::new("test-domain");

    let proposal = Proposal::new(
        domain_id,
        kp.did().clone(),
        "Change Quorum".to_string(),
        "Increase quorum requirement".to_string(),
        ProposalPayload::ConfigChange {
            new_config: r#"{"quorum_percent": 75}"#.to_string(),
        },
    );

    if let ProposalPayload::ConfigChange { new_config } = &proposal.payload {
        assert!(new_config.contains("75"));
    } else {
        panic!("Expected ConfigChange payload");
    }
}

// =============================================================================
// Emergency Quorum Threshold Tests (Issue #477)
// =============================================================================

#[test]
fn test_emergency_freeze_proposal_requires_higher_quorum() {
    use icn_governance::{GovernanceProfile, ProposalThresholds};

    let profile = GovernanceProfile::cooperative_default();
    let proposal_id = ProposalId::generate();

    // Create votes: 5 out of 10 eligible voters (50% turnout)
    // All votes are "For"
    // Note: integer division rounds down: (10 * 67) / 100 = 6 required for 67% quorum
    let mut votes = Vec::new();
    for _ in 0..5 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::For,
        ));
    }

    let tally = VoteTally::from(votes);
    let eligible_count = 10;

    // Normal proposal (50% quorum): should pass with exactly 5 votes
    // (10 * 50) / 100 = 5 required, we have 5 → quorum met
    let normal_thresholds = ProposalThresholds::new(50, 50);
    let result = profile
        .evaluate_with_thresholds(&tally, normal_thresholds, eligible_count)
        .unwrap();
    assert!(
        matches!(result, icn_governance::DecisionOutcome::Accepted),
        "Normal proposal with 50% turnout should pass 50% quorum"
    );

    // Emergency freeze proposal (67% quorum): should fail quorum
    // (10 * 67) / 100 = 6 required, we only have 5 → NoQuorum
    let freeze_thresholds = ProposalThresholds::new(67, 75);
    let result = profile
        .evaluate_with_thresholds(&tally, freeze_thresholds, eligible_count)
        .unwrap();
    assert!(
        matches!(result, icn_governance::DecisionOutcome::NoQuorum),
        "Freeze proposal with 50% turnout should fail 67% quorum (need 6, got 5)"
    );
}

#[test]
fn test_emergency_rollback_requires_supermajority_approval() {
    use icn_governance::{GovernanceProfile, ProposalThresholds};

    let profile = GovernanceProfile::cooperative_default();
    let proposal_id = ProposalId::generate();

    // Create votes: 8 out of 10 eligible (80% turnout - meets 75% quorum)
    // 6 For, 2 Against (75% approval)
    let mut votes = Vec::new();
    for _ in 0..6 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::For,
        ));
    }
    for _ in 0..2 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::Against,
        ));
    }

    let tally = VoteTally::from(votes);
    let eligible_count = 10;

    // Rollback requires 80% approval
    let rollback_thresholds = ProposalThresholds::new(75, 80);
    let result = profile
        .evaluate_with_thresholds(&tally, rollback_thresholds, eligible_count)
        .unwrap();
    // 6/8 = 75% approval, but 80% required - should be rejected
    assert!(
        matches!(result, icn_governance::DecisionOutcome::Rejected),
        "Rollback with 75% approval should fail 80% approval threshold"
    );
}

#[test]
fn test_emergency_veto_passes_with_supermajority() {
    use icn_governance::{GovernanceProfile, ProposalThresholds};

    let profile = GovernanceProfile::cooperative_default();
    let proposal_id = ProposalId::generate();

    // Create votes: 7 out of 10 eligible (70% turnout - meets 67% quorum)
    // All 7 vote For (100% approval - exceeds 75% threshold)
    let mut votes = Vec::new();
    for _ in 0..7 {
        let kp = KeyPair::generate().unwrap();
        votes.push(Vote::new(
            proposal_id.clone(),
            kp.did().clone(),
            VoteChoice::For,
        ));
    }

    let tally = VoteTally::from(votes);
    let eligible_count = 10;

    // Veto requires 67% quorum and 75% approval
    let veto_thresholds = ProposalThresholds::new(67, 75);
    let result = profile
        .evaluate_with_thresholds(&tally, veto_thresholds, eligible_count)
        .unwrap();
    assert!(
        matches!(result, icn_governance::DecisionOutcome::Accepted),
        "Veto with 70% turnout and 100% approval should pass"
    );
}

#[test]
fn test_thresholds_for_proposal_returns_correct_values() {
    use icn_governance::GovernanceConfig;

    let config = GovernanceConfig::cooperative_default();

    // Normal text proposal: 50/50
    let text_payload = ProposalPayload::Text {
        body: "test".to_string(),
    };
    let thresholds = config.thresholds_for_proposal(&text_payload);
    assert_eq!(thresholds.quorum_percentage, 50);
    assert_eq!(thresholds.approval_percentage, 50);

    // Freeze member: 67/75
    let freeze_payload = ProposalPayload::FreezeMember {
        member: icn_identity::KeyPair::generate().unwrap().did().clone(),
        reason: "test".to_string(),
        duration_seconds: None,
    };
    let thresholds = config.thresholds_for_proposal(&freeze_payload);
    assert_eq!(thresholds.quorum_percentage, 67);
    assert_eq!(thresholds.approval_percentage, 75);

    // Rollback ledger: 75/80 (highest thresholds)
    let rollback_payload = ProposalPayload::RollbackLedger {
        target_hash: "checkpoint-1".to_string(),
        reason: "emergency".to_string(),
        affected_accounts: vec![],
    };
    let thresholds = config.thresholds_for_proposal(&rollback_payload);
    assert_eq!(thresholds.quorum_percentage, 75);
    assert_eq!(thresholds.approval_percentage, 80);
}

#[test]
fn test_proposal_type_name_all_variants() {
    // Ensure type_name returns expected strings for metrics
    let test_cases = [
        (
            ProposalPayload::Text {
                body: "".to_string(),
            },
            "text",
        ),
        (
            ProposalPayload::FreezeMember {
                member: icn_identity::KeyPair::generate().unwrap().did().clone(),
                reason: "".to_string(),
                duration_seconds: None,
            },
            "freeze_member",
        ),
        (
            ProposalPayload::VetoProposal {
                target_proposal_id: ProposalId::generate().0,
                reason: "".to_string(),
            },
            "veto_proposal",
        ),
        (
            ProposalPayload::RollbackLedger {
                target_hash: "".to_string(),
                reason: "".to_string(),
                affected_accounts: vec![],
            },
            "rollback_ledger",
        ),
    ];

    for (payload, expected_name) in test_cases {
        assert_eq!(
            payload.type_name(),
            expected_name,
            "type_name for {:?} should be {expected_name}",
            std::mem::discriminant(&payload)
        );
    }
}

#[test]
fn test_proposal_emergency_type_identification() {
    // Emergency proposals
    let freeze = ProposalPayload::FreezeMember {
        member: icn_identity::KeyPair::generate().unwrap().did().clone(),
        reason: "".to_string(),
        duration_seconds: None,
    };
    assert_eq!(freeze.emergency_type(), Some("freeze"));
    assert!(freeze.is_emergency());

    let veto = ProposalPayload::VetoProposal {
        target_proposal_id: ProposalId::generate().0,
        reason: "".to_string(),
    };
    assert_eq!(veto.emergency_type(), Some("veto"));
    assert!(veto.is_emergency());

    let rollback = ProposalPayload::RollbackLedger {
        target_hash: "".to_string(),
        reason: "".to_string(),
        affected_accounts: vec![],
    };
    assert_eq!(rollback.emergency_type(), Some("rollback"));
    assert!(rollback.is_emergency());

    // Non-emergency proposals
    let text = ProposalPayload::Text {
        body: "".to_string(),
    };
    assert_eq!(text.emergency_type(), None);
    assert!(!text.is_emergency());

    let budget = ProposalPayload::Budget {
        amount: 100,
        currency: "USD".to_string(),
        recipient: icn_identity::KeyPair::generate().unwrap().did().clone(),
        purpose: "".to_string(),
    };
    assert_eq!(budget.emergency_type(), None);
    assert!(!budget.is_emergency());
}

// =============================================================================
// GovernanceProof Integration Tests
// =============================================================================

#[test]
fn test_governance_proof_roundtrip() {
    use icn_governance::{GovernanceProof, ProofOutcome};

    let signer_kp = KeyPair::generate().unwrap();
    let voter1 = KeyPair::generate().unwrap();
    let voter2 = KeyPair::generate().unwrap();
    let voter3 = KeyPair::generate().unwrap();
    let proposal_id = ProposalId::generate();

    let votes = vec![
        Vote::new(proposal_id.clone(), voter1.did().clone(), VoteChoice::For),
        Vote::new(proposal_id.clone(), voter2.did().clone(), VoteChoice::For),
        Vote::new(
            proposal_id.clone(),
            voter3.did().clone(),
            VoteChoice::Against,
        ),
    ];

    let tally = VoteTally::new(2, 1, 0);

    let mut proof = GovernanceProof::new(
        proposal_id.0.clone(),
        "test-domain".to_string(),
        ProofOutcome::Accepted,
        tally,
        &votes,
        1234567890,
        signer_kp.did().to_string(),
    );

    // Before signing: binding hash is valid, signature is empty
    assert!(proof.verify_binding());
    assert!(proof.signature.is_empty());

    // Sign with the signer's key
    let signing_key_bytes = signer_kp.to_signing_key_bytes();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
    proof.sign(&signing_key);

    // After signing: both binding and signature should verify
    assert!(proof.verify_binding());
    assert!(proof.verify_signature(&signing_key.verifying_key()));

    // Verify via DID resolution (the key path that Node B would use)
    let resolved_key = signer_kp.did().to_verifying_key().unwrap();
    assert!(proof.verify_signature(&resolved_key));

    // Serialize and deserialize (simulates gossip transport)
    let json = serde_json::to_vec(&proof).unwrap();
    let deserialized: GovernanceProof = serde_json::from_slice(&json).unwrap();
    assert!(deserialized.verify_binding());
    assert!(deserialized.verify_signature(&resolved_key));
}

#[test]
fn test_governance_proof_tamper_detection() {
    use icn_governance::{GovernanceProof, ProofOutcome};

    let signer_kp = KeyPair::generate().unwrap();
    let voter1 = KeyPair::generate().unwrap();
    let proposal_id = ProposalId::generate();

    let votes = vec![Vote::new(
        proposal_id.clone(),
        voter1.did().clone(),
        VoteChoice::For,
    )];

    let tally = VoteTally::new(1, 0, 0);

    let mut proof = GovernanceProof::new(
        proposal_id.0.clone(),
        "test-domain".to_string(),
        ProofOutcome::Accepted,
        tally,
        &votes,
        1234567890,
        signer_kp.did().to_string(),
    );

    let signing_key_bytes = signer_kp.to_signing_key_bytes();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
    proof.sign(&signing_key);

    // Tamper with the outcome
    proof.outcome = ProofOutcome::Rejected;

    // Binding hash should now fail (fields don't match proof_hash)
    assert!(!proof.verify_binding());
}

#[test]
fn test_governance_proof_v2_cross_node_decision_hash_stability() {
    use icn_governance::{
        GovernanceDecisionAttestation, GovernanceDecisionReceipt, GovernanceProofV2, ProofOutcome,
    };

    let signer_a = KeyPair::generate().unwrap();
    let signer_b = KeyPair::generate().unwrap();
    let voter1 = KeyPair::generate().unwrap();
    let voter2 = KeyPair::generate().unwrap();
    let proposal_id = ProposalId::generate();

    let votes = vec![
        Vote::new(proposal_id.clone(), voter1.did().clone(), VoteChoice::For),
        Vote::new(
            proposal_id.clone(),
            voter2.did().clone(),
            VoteChoice::Against,
        ),
    ];
    let tally = VoteTally::new(1, 1, 0);

    let receipt_a = GovernanceDecisionReceipt::new(
        proposal_id.0.clone(),
        "test-domain".to_string(),
        ProofOutcome::Accepted,
        tally.clone(),
        &votes,
    );
    let receipt_b = GovernanceDecisionReceipt::new(
        proposal_id.0.clone(),
        "test-domain".to_string(),
        ProofOutcome::Accepted,
        tally,
        &votes,
    );
    assert_eq!(receipt_a.decision_hash, receipt_b.decision_hash);

    let signing_key_a = ed25519_dalek::SigningKey::from_bytes(&signer_a.to_signing_key_bytes());
    let signing_key_b = ed25519_dalek::SigningKey::from_bytes(&signer_b.to_signing_key_bytes());
    let attestation_a = GovernanceDecisionAttestation::sign(
        receipt_a.decision_hash,
        signer_a.did().to_string(),
        1_700_000_001,
        &signing_key_a,
    );
    let attestation_b = GovernanceDecisionAttestation::sign(
        receipt_b.decision_hash,
        signer_b.did().to_string(),
        1_700_000_123,
        &signing_key_b,
    );
    assert_ne!(attestation_a.signature, attestation_b.signature);

    let proof_a = GovernanceProofV2::new(receipt_a.clone(), vec![attestation_a]);
    let proof_b = GovernanceProofV2::new(receipt_b.clone(), vec![attestation_b]);
    assert_eq!(proof_a, proof_b);
    assert!(proof_a.verify_receipt());
    assert!(proof_b.verify_receipt());
}
