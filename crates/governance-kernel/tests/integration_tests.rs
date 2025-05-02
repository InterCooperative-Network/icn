use icn_governance_kernel::{
    GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus, GovernanceEventType,
};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_core_vm::IdentityContext;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Create a test identity context for testing
fn create_test_identity_context() -> Arc<IdentityContext> {
    // Generate a simple keypair for testing
    let private_key = vec![1, 2, 3, 4]; // Dummy private key
    let public_key = vec![5, 6, 7, 8]; // Dummy public key
    let keypair = KeyPair::new(private_key, public_key);
    
    // Create the identity context
    let identity_context = IdentityContext::new(keypair, "did:icn:test");
    
    Arc::new(identity_context)
}

#[tokio::test]
async fn test_governance_event_emission() {
    // Set up test environment with storage and identity
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Create a test proposal directly
    let proposal = Proposal {
        title: "Test Proposal".to_string(),
        description: "This is a test proposal for event emission".to_string(),
        proposer: IdentityId::new("did:icn:test"),
        scope: IdentityScope::Federation,
        scope_id: Some(IdentityId::new("did:icn:federation:test")),
        status: ProposalStatus::Draft,
        voting_end_time: chrono::Utc::now().timestamp() + 86400, // 24 hour voting period
        votes_for: 0,
        votes_against: 0,
        votes_abstain: 0,
        ccl_code: None,
        wasm_bytes: None,
    };
    
    // Submit the proposal and capture the CID
    let proposal_id = kernel.process_proposal(proposal.clone()).await.unwrap();
    
    // Verify a ProposalCreated event was emitted
    let events = kernel.get_proposal_events(proposal_id.clone()).await;
    assert_eq!(events.len(), 1, "Expected exactly one event");
    assert_eq!(events[0].event_type, GovernanceEventType::ProposalCreated);
    
    // Verify event contains correct identifiers
    assert_eq!(events[0].proposal_cid, Some(proposal_id.clone()));
    
    // Cast a vote and verify VoteCast event
    let vote = Vote {
        voter: IdentityId::new("did:icn:test"),
        proposal_id: proposal_id.clone(),
        choice: VoteChoice::For,
        weight: 1,
        scope: IdentityScope::Federation,
        scope_id: Some(IdentityId::new("did:icn:federation:test")),
        reason: Some("Support test".to_string()),
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    kernel.record_vote(vote).await.unwrap();
    
    // Verify a VoteCast event was emitted
    let events = kernel.get_proposal_events(proposal_id.clone()).await;
    assert_eq!(events.len(), 2, "Expected two events after voting");
    assert!(events.iter().any(|e| e.event_type == GovernanceEventType::VoteCast));
    
    // Finalize the proposal and verify event
    kernel.finalize_proposal(proposal_id.clone()).await.unwrap();
    
    // Verify a ProposalFinalized event was emitted
    let events = kernel.get_proposal_events(proposal_id.clone()).await;
    assert_eq!(events.len(), 3, "Expected three events after finalization");
    assert!(events.iter().any(|e| e.event_type == GovernanceEventType::ProposalFinalized));
    
    // Execute the proposal and verify event
    kernel.execute_proposal(proposal_id.clone()).await.unwrap();
    
    // Verify a ProposalExecuted event was emitted
    let events = kernel.get_proposal_events(proposal_id.clone()).await;
    assert_eq!(events.len(), 4, "Expected four events after execution");
    assert!(events.iter().any(|e| e.event_type == GovernanceEventType::ProposalExecuted));
    
    // Check the final proposal state
    let final_proposal = kernel.get_proposal(proposal_id).await.unwrap();
    assert_eq!(final_proposal.status, ProposalStatus::Executed);
}

#[tokio::test]
async fn test_proposal_event_filtering() {
    // Set up test environment
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Create two different proposals
    let proposal1 = Proposal {
        title: "First Proposal".to_string(),
        description: "This is the first test proposal".to_string(),
        proposer: IdentityId::new("did:icn:test"),
        scope: IdentityScope::Federation,
        scope_id: Some(IdentityId::new("did:icn:federation:test")),
        status: ProposalStatus::Draft,
        voting_end_time: chrono::Utc::now().timestamp() + 86400,
        votes_for: 0,
        votes_against: 0,
        votes_abstain: 0,
        ccl_code: None,
        wasm_bytes: None,
    };
    
    let proposal2 = Proposal {
        title: "Second Proposal".to_string(),
        description: "This is the second test proposal".to_string(),
        proposer: IdentityId::new("did:icn:test"),
        scope: IdentityScope::Federation,
        scope_id: Some(IdentityId::new("did:icn:federation:test")),
        status: ProposalStatus::Draft,
        voting_end_time: chrono::Utc::now().timestamp() + 86400,
        votes_for: 0,
        votes_against: 0,
        votes_abstain: 0,
        ccl_code: None,
        wasm_bytes: None,
    };
    
    // Submit both proposals
    let cid1 = kernel.process_proposal(proposal1).await.unwrap();
    let cid2 = kernel.process_proposal(proposal2).await.unwrap();
    
    // Vote on the first proposal only
    let vote = Vote {
        voter: IdentityId::new("did:icn:test"),
        proposal_id: cid1.clone(),
        choice: VoteChoice::For,
        weight: 1,
        scope: IdentityScope::Federation,
        scope_id: Some(IdentityId::new("did:icn:federation:test")),
        reason: None,
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    kernel.record_vote(vote).await.unwrap();
    
    // Verify events are properly filtered by proposal
    let events1 = kernel.get_proposal_events(cid1.clone()).await;
    let events2 = kernel.get_proposal_events(cid2.clone()).await;
    
    assert_eq!(events1.len(), 2, "First proposal should have 2 events");
    assert_eq!(events2.len(), 1, "Second proposal should have 1 event");
    
    assert!(events1.iter().any(|e| e.event_type == GovernanceEventType::VoteCast),
            "First proposal should have a VoteCast event");
    assert!(!events2.iter().any(|e| e.event_type == GovernanceEventType::VoteCast),
            "Second proposal should not have a VoteCast event");
} 