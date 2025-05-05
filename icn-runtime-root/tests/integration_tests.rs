use icn_core_vm::{IdentityContext, VMContext, ResourceAuthorization};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_federation::{FederationManager, FederationManagerConfig, TrustBundle};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_execution_tools::derive_authorizations;
use icn_agoranet_integration::AgoraNetIntegration;

use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use cid::Cid;

// Helper function to create test identity
fn create_test_identity(did: &str) -> (KeyPair, IdentityId) {
    // Generate test keypair
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    let identity_id = IdentityId::new(did);
    
    (keypair, identity_id)
}

/// Simulates what a wallet would do to interact with the ICN Runtime
#[tokio::test]
async fn test_wallet_integration_flow() {
    // 1. Set up common storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Create identities (similar to what a wallet would manage)
    let (user_keypair, user_id) = create_test_identity("did:icn:user1");
    let federation_id = IdentityId::new("did:icn:federation:test");
    
    // 3. Create identity context for VM operations
    let identity_context = Arc::new(IdentityContext::new(
        user_keypair.clone(),
        user_id.to_string()
    ));
    
    // 4. Initialize governance kernel (core governance engine)
    let governance_kernel = GovernanceKernel::new(
        storage.clone(),
        identity_context.clone()
    );
    
    // 5. Initialize federation manager (for TrustBundle sync)
    let config = FederationManagerConfig {
        bootstrap_period: Duration::from_secs(1),
        peer_sync_interval: Duration::from_secs(5),
        trust_bundle_sync_interval: Duration::from_secs(10),
        max_peers: 10,
        ..Default::default()
    };
    
    let federation_manager = FederationManager::new(
        config,
        storage.clone(),
        user_keypair.clone()
    ).await.unwrap();
    
    // 6. Initialize AgoraNet integration for handling emitted events
    let agoranet = AgoraNetIntegration::new(storage.clone());
    
    // --- TEST SCENARIO: Creating and processing a governance proposal ---
    
    // Simulate wallet creating a proposal
    let proposal = Proposal::new(
        "Test Wallet Integration".to_string(),
        "This proposal tests wallet integration with the ICN Runtime".to_string(),
        user_id.clone(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        86400, // 24-hour voting period
        Some("// Sample CCL code\nrule test_rule { always allow }".to_string()),
    );
    
    // Submit the proposal
    let proposal_cid = governance_kernel.process_proposal(proposal.clone()).await.unwrap();
    println!("Created proposal with CID: {}", proposal_cid);
    
    // Verify proposal exists
    let retrieved_proposal = governance_kernel.get_proposal(proposal_cid).await.unwrap();
    assert_eq!(retrieved_proposal.title, "Test Wallet Integration");
    
    // Check if event was emitted
    let events = governance_kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 1, "Should have one event for proposal creation");
    
    // Obtain the VC for the proposal
    let credentials = governance_kernel.get_proposal_credentials(proposal_cid).await;
    assert_eq!(credentials.len(), 1, "Should have one credential for the proposal");
    
    // Send the emitted event to AgoraNet (this is what the runtime would do)
    let event = events[0].clone();
    agoranet.register_governance_event(&event).await.unwrap();
    
    // Verify AgoraNet received the event
    let agoranet_events = agoranet.get_events_for_proposal(proposal_cid).await.unwrap();
    assert_eq!(agoranet_events.len(), 1, "AgoraNet should have the proposal event");
    
    // --- TEST SCENARIO: Voting on a proposal ---
    
    // Simulate wallet casting a vote
    let vote = Vote::new(
        user_id.clone(),
        proposal_cid,
        VoteChoice::For,
        IdentityScope::Federation,
        Some(federation_id.clone()),
        Some("Supporting this proposal".to_string()),
    );
    
    // Record the vote
    governance_kernel.record_vote(vote).await.unwrap();
    
    // Verify vote event was emitted
    let events = governance_kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 2, "Should have two events now");
    
    // --- TEST SCENARIO: TrustBundle synchronization ---
    
    // Create and publish a new TrustBundle
    let mut trust_bundle = TrustBundle::new(1);
    trust_bundle.add_node(user_id.clone(), icn_federation::roles::NodeRole::Validator);
    trust_bundle.set_proof(vec![1, 2, 3, 4]); // Dummy proof
    
    // Store the bundle
    federation_manager.store_trust_bundle(&trust_bundle).await.unwrap();
    
    // Verify latest epoch is tracked
    let latest_epoch = federation_manager.get_latest_known_epoch().await.unwrap();
    assert_eq!(latest_epoch, 1, "Latest epoch should be updated");
    
    // Retrieve bundle (as wallet would)
    let retrieved_bundle = federation_manager.get_trust_bundle(1).await.unwrap();
    assert_eq!(retrieved_bundle.epoch_id, 1);
    assert_eq!(retrieved_bundle.nodes.len(), 1);
    
    // --- TEST SCENARIO: Proposal finalization and execution ---
    
    // Finalize the proposal
    governance_kernel.finalize_proposal(proposal_cid).await.unwrap();
    
    // Verify finalization event was emitted
    let events = governance_kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 3, "Should have three events now");
    
    // Check the proposal status
    let proposal = governance_kernel.get_proposal(proposal_cid).await.unwrap();
    assert_eq!(proposal.status, ProposalStatus::Passed, "Proposal should have passed");
    
    // Execute the proposal with the right authorizations
    let template = proposal.get_template();
    let authorizations = derive_authorizations(&template);
    
    // Create VM context for execution
    let vm_context = VMContext::new(
        identity_context.clone(),
        authorizations
    );
    
    // Execute the proposal
    governance_kernel.execute_proposal_with_context(proposal_cid, vm_context).await.unwrap();
    
    // Verify execution event was emitted
    let events = governance_kernel.get_proposal_events(proposal_cid).await;
    assert_eq!(events.len(), 4, "Should have four events now");
    
    // Final proposal status check
    let final_proposal = governance_kernel.get_proposal(proposal_cid).await.unwrap();
    assert_eq!(final_proposal.status, ProposalStatus::Executed, "Proposal should be executed");
    
    // Verify AgoraNet has all the events
    for event in events.iter() {
        agoranet.register_governance_event(event).await.unwrap();
    }
    
    let agoranet_events = agoranet.get_events_for_proposal(proposal_cid).await.unwrap();
    assert_eq!(agoranet_events.len(), 4, "AgoraNet should have all proposal events");
} 