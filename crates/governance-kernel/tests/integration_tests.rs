use icn_governance_kernel::{
    GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus, GovernanceEventType,
};
use icn_identity::{IdentityId, IdentityScope, KeyPair, IdentityContext};
use icn_storage::AsyncInMemoryStorage;
use icn_core_vm::IdentityContext as VmIdentityContext;
use std::sync::Arc;
use tokio::sync::Mutex;
use cid::Cid;

/// Create a test identity context for testing
fn create_test_identity_context() -> Arc<VmIdentityContext> {
    // Generate a simple keypair for testing
    let private_key = vec![1, 2, 3, 4]; // Dummy private key
    let public_key = vec![5, 6, 7, 8]; // Dummy public key
    let keypair = KeyPair::new(private_key, public_key);
    
    // Create the identity context
    let identity_context = VmIdentityContext::new(keypair, "did:icn:test");
    
    Arc::new(identity_context)
}

#[tokio::test]
async fn test_governance_event_emission() {
    // Set up test environment with storage and identity
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Create a test proposal
    let proposal = Proposal::new(
        "Test Proposal".to_string(),
        "This is a test proposal for event emission".to_string(),
        IdentityId::new("did:icn:test"),
        IdentityScope::Federation,
        Some(IdentityId::new("did:icn:federation:test")),
        86400, // 24 hour voting period
        None,  // No CCL code for this test
    );
    
    // Submit the proposal and capture the CID
    let proposal_cid = kernel.process_proposal(proposal.clone()).await.unwrap();
    
    // Verify a ProposalCreated event was emitted
    let events = kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 1, "Expected exactly one event");
    assert_eq!(events[0].event_type, GovernanceEventType::ProposalCreated);
    
    // Verify corresponding VC was generated
    let credentials = kernel.get_proposal_credentials(proposal_cid).await;
    assert_eq!(credentials.len(), 1, "Expected exactly one credential");
    assert!(credentials[0].credential_type.contains(&"ProposalCreationCredential".to_string()));
    
    // Verify event/VC contains correct identifiers
    assert_eq!(events[0].proposal_cid, Some(proposal_cid));
    assert!(credentials[0].credentialSubject.as_object().unwrap().contains_key("proposalCid"));
    
    // Cast a vote and verify VoteCast event
    let vote = Vote::new(
        IdentityId::new("did:icn:test"),
        proposal_cid,
        VoteChoice::For,
        IdentityScope::Federation,
        Some(IdentityId::new("did:icn:federation:test")),
        Some("Support test".to_string()),
    );
    
    kernel.record_vote(vote).await.unwrap();
    
    // Verify a VoteCast event was emitted
    let events = kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 2, "Expected two events after voting");
    assert!(events.iter().any(|e| e.event_type == GovernanceEventType::VoteCast));
    
    // Finalize the proposal and verify event
    kernel.finalize_proposal(proposal_cid).await.unwrap();
    
    // Verify a ProposalFinalized event was emitted
    let events = kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 3, "Expected three events after finalization");
    assert!(events.iter().any(|e| e.event_type == GovernanceEventType::ProposalFinalized));
    
    // Verify finalized VC contains both proposal and status info
    let credentials = kernel.get_proposal_credentials(proposal_cid).await;
    assert_eq!(credentials.len(), 2, "Expected two credentials after finalization");
    
    // Find the finalization credential
    let finalization_credential = credentials.iter()
        .find(|vc| vc.credential_type.contains(&"ProposalFinalizationCredential".to_string()))
        .expect("Finalization credential should exist");
    
    // Verify credential contains status information
    let subject = finalization_credential.credentialSubject.as_object().unwrap();
    let event_data = subject.get("eventData").and_then(|d| d.as_object()).unwrap();
    assert!(event_data.contains_key("status"), "Credential should contain status information");
    
    // Execute the proposal and verify event
    kernel.execute_proposal(proposal_cid).await.unwrap();
    
    // Verify a ProposalExecuted event was emitted
    let events = kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 4, "Expected four events after execution");
    assert!(events.iter().any(|e| e.event_type == GovernanceEventType::ProposalExecuted));
    
    // Check the final proposal state
    let final_proposal = kernel.get_proposal(proposal_cid).await.unwrap();
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
    let proposal1 = Proposal::new(
        "First Proposal".to_string(),
        "This is the first test proposal".to_string(),
        IdentityId::new("did:icn:test"),
        IdentityScope::Federation,
        Some(IdentityId::new("did:icn:federation:test")),
        86400,
        None,
    );
    
    let proposal2 = Proposal::new(
        "Second Proposal".to_string(),
        "This is the second test proposal".to_string(),
        IdentityId::new("did:icn:test"),
        IdentityScope::Federation,
        Some(IdentityId::new("did:icn:federation:test")),
        86400,
        None,
    );
    
    // Submit both proposals
    let cid1 = kernel.process_proposal(proposal1).await.unwrap();
    let cid2 = kernel.process_proposal(proposal2).await.unwrap();
    
    // Vote on the first proposal only
    let vote = Vote::new(
        IdentityId::new("did:icn:test"),
        cid1,
        VoteChoice::For,
        IdentityScope::Federation,
        Some(IdentityId::new("did:icn:federation:test")),
        None,
    );
    
    kernel.record_vote(vote).await.unwrap();
    
    // Verify events are properly filtered by proposal
    let events1 = kernel.get_proposal_events(cid1).await;
    let events2 = kernel.get_proposal_events(cid2).await;
    
    assert_eq!(events1.len(), 2, "First proposal should have 2 events");
    assert_eq!(events2.len(), 1, "Second proposal should have 1 event");
    
    assert!(events1.iter().any(|e| e.event_type == GovernanceEventType::VoteCast),
            "First proposal should have a VoteCast event");
    assert!(!events2.iter().any(|e| e.event_type == GovernanceEventType::VoteCast),
            "Second proposal should not have a VoteCast event");
    
    // Verify credentials are properly filtered by proposal
    let credentials1 = kernel.get_proposal_credentials(cid1).await;
    let credentials2 = kernel.get_proposal_credentials(cid2).await;
    
    assert_eq!(credentials1.len(), 1, "First proposal should have one credential");
    assert_eq!(credentials2.len(), 1, "Second proposal should have one credential");
} 