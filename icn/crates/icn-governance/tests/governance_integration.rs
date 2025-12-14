//! Integration tests for icn-governance.
//!
//! These tests verify the complete governance flow including
//! domain management, proposal lifecycle, voting, and tallying.

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
    assert_eq!(format!("{}", custom_id), "my-coop");
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
        let domain = GovernanceDomain::new(format!("Coop {}", i), config);
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
            format!("Proposal A{}", i),
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
            format!("Proposal B{}", i),
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
