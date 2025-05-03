use icn_core_vm::{IdentityContext, VMContext, ResourceAuthorization};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_dag::{DagStore, DagNode, DagBuilder};
use icn_execution_tools::derive_authorizations;

use std::sync::Arc;
use tokio::sync::Mutex;
use cid::Cid;
use std::collections::HashSet;

// Helper function to create test identity
fn create_test_identity(did: &str) -> (KeyPair, IdentityId) {
    // Generate test keypair
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    let identity_id = IdentityId::new(did);
    
    (keypair, identity_id)
}

/// Tests consistency of governance state across a series of operations
#[tokio::test]
async fn test_governance_state_consistency() {
    // Set up test environment
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // Create identity
    let (admin_keypair, admin_id) = create_test_identity("did:icn:admin");
    let (voter1_keypair, voter1_id) = create_test_identity("did:icn:voter1");
    let (voter2_keypair, voter2_id) = create_test_identity("did:icn:voter2");
    
    let federation_id = IdentityId::new("did:icn:federation:test");
    
    // Create admin identity context
    let admin_context = Arc::new(IdentityContext::new(
        admin_keypair.clone(),
        admin_id.to_string()
    ));
    
    // Initialize governance kernel
    let governance_kernel = GovernanceKernel::new(
        storage.clone(),
        admin_context.clone()
    );
    
    // Step 1: Create multiple proposals in sequence
    let mut proposal_cids = Vec::new();
    
    for i in 1..=3 {
        let proposal = Proposal::new(
            format!("Test Proposal {}", i),
            format!("Description for proposal {}", i),
            admin_id.clone(),
            IdentityScope::Federation,
            Some(federation_id.clone()),
            86400, // 24-hour voting period
            Some(format!("// CCL code for proposal {}\nrule test_rule_{} {{ always allow }}", i, i)),
        );
        
        let cid = governance_kernel.process_proposal(proposal).await.unwrap();
        proposal_cids.push(cid);
    }
    
    // Verify all proposals exist
    for (i, cid) in proposal_cids.iter().enumerate() {
        let proposal = governance_kernel.get_proposal(*cid).await.unwrap();
        assert_eq!(proposal.title, format!("Test Proposal {}", i+1));
    }
    
    // Step 2: Cast votes with different identities
    
    // Create voter1 context
    let voter1_context = Arc::new(IdentityContext::new(
        voter1_keypair.clone(),
        voter1_id.to_string()
    ));
    
    // Create voter1 governance kernel
    let voter1_kernel = GovernanceKernel::new(
        storage.clone(),
        voter1_context.clone()
    );
    
    // Create voter2 context
    let voter2_context = Arc::new(IdentityContext::new(
        voter2_keypair.clone(),
        voter2_id.to_string()
    ));
    
    // Create voter2 governance kernel
    let voter2_kernel = GovernanceKernel::new(
        storage.clone(),
        voter2_context.clone()
    );
    
    // Cast votes on the first proposal
    let vote1 = Vote::new(
        voter1_id.clone(),
        proposal_cids[0],
        VoteChoice::For,
        IdentityScope::Federation,
        Some(federation_id.clone()),
        Some("Vote from voter1".to_string()),
    );
    
    let vote2 = Vote::new(
        voter2_id.clone(),
        proposal_cids[0],
        VoteChoice::Against,
        IdentityScope::Federation,
        Some(federation_id.clone()),
        Some("Vote from voter2".to_string()),
    );
    
    // Record votes from different kernels
    voter1_kernel.record_vote(vote1).await.unwrap();
    voter2_kernel.record_vote(vote2).await.unwrap();
    
    // Verify votes were recorded correctly
    let proposal1 = governance_kernel.get_proposal(proposal_cids[0]).await.unwrap();
    assert_eq!(proposal1.votes.len(), 2, "Proposal should have 2 votes");
    
    // Verify events were recorded
    let events = governance_kernel.get_proposal_events(proposal_cids[0]).await;
    assert_eq!(events.len(), 3, "Should have 3 events (1 proposal creation + 2 votes)");
    
    // Cast votes on the second proposal, but only from voter1
    let vote3 = Vote::new(
        voter1_id.clone(),
        proposal_cids[1],
        VoteChoice::For,
        IdentityScope::Federation,
        Some(federation_id.clone()),
        None,
    );
    
    voter1_kernel.record_vote(vote3).await.unwrap();
    
    // Verify all proposals maintain their correct state
    for (i, cid) in proposal_cids.iter().enumerate() {
        let proposal = governance_kernel.get_proposal(*cid).await.unwrap();
        
        match i {
            0 => assert_eq!(proposal.votes.len(), 2, "First proposal should have 2 votes"),
            1 => assert_eq!(proposal.votes.len(), 1, "Second proposal should have 1 vote"),
            2 => assert_eq!(proposal.votes.len(), 0, "Third proposal should have 0 votes"),
            _ => unreachable!()
        }
    }
    
    // Step 3: Finalize proposals in different order
    
    // Finalize the second proposal first
    governance_kernel.finalize_proposal(proposal_cids[1]).await.unwrap();
    
    // Then the first proposal
    governance_kernel.finalize_proposal(proposal_cids[0]).await.unwrap();
    
    // Check the status of all proposals
    let proposal1 = governance_kernel.get_proposal(proposal_cids[0]).await.unwrap();
    let proposal2 = governance_kernel.get_proposal(proposal_cids[1]).await.unwrap();
    let proposal3 = governance_kernel.get_proposal(proposal_cids[2]).await.unwrap();
    
    // Verify status based on voting outcomes
    assert_eq!(proposal1.status, ProposalStatus::Tied, "First proposal should be tied (1 for, 1 against)");
    assert_eq!(proposal2.status, ProposalStatus::Passed, "Second proposal should have passed (1 for, 0 against)");
    assert_eq!(proposal3.status, ProposalStatus::Active, "Third proposal should still be active");
    
    // Step 4: Execute proposals and check DAG consistency
    
    // Only execute passed proposals
    if proposal2.status == ProposalStatus::Passed {
        let template = proposal2.get_template();
        let authorizations = derive_authorizations(&template);
        
        let vm_context = VMContext::new(admin_context.clone(), authorizations);
        
        governance_kernel.execute_proposal_with_context(proposal_cids[1], vm_context).await.unwrap();
    }
    
    // Check final proposal statuses
    let final_proposal2 = governance_kernel.get_proposal(proposal_cids[1]).await.unwrap();
    assert_eq!(final_proposal2.status, ProposalStatus::Executed, "Second proposal should be executed");
    
    // The other proposals should maintain their previous statuses
    let final_proposal1 = governance_kernel.get_proposal(proposal_cids[0]).await.unwrap();
    let final_proposal3 = governance_kernel.get_proposal(proposal_cids[2]).await.unwrap();
    
    assert_eq!(final_proposal1.status, ProposalStatus::Tied, "First proposal status should not change");
    assert_eq!(final_proposal3.status, ProposalStatus::Active, "Third proposal status should not change");
    
    // Verify all events were recorded correctly
    let events1 = governance_kernel.get_proposal_events(proposal_cids[0]).await;
    let events2 = governance_kernel.get_proposal_events(proposal_cids[1]).await;
    let events3 = governance_kernel.get_proposal_events(proposal_cids[2]).await;
    
    assert_eq!(events1.len(), 4, "First proposal should have 4 events (creation, 2 votes, finalization)");
    assert_eq!(events2.len(), 4, "Second proposal should have 4 events (creation, 1 vote, finalization, execution)");
    assert_eq!(events3.len(), 1, "Third proposal should have 1 event (creation)");
}

/// Tests DAG consistency with multiple parallel operations
#[tokio::test]
async fn test_dag_consistency() {
    // Set up storage
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // Create DAG store
    let dag_store = DagStore::new(storage.clone());
    
    // Create multiple DAG nodes in parallel
    let mut node_cids = Vec::new();
    let mut handles = Vec::new();
    
    for i in 0..5 {
        let dag_store_clone = dag_store.clone();
        
        let handle = tokio::spawn(async move {
            // Create a new DAG node
            let mut builder = DagBuilder::new();
            builder.set_data(format!("Node {}", i).into_bytes());
            
            // Link to parent nodes if applicable
            if i > 0 {
                // Link to the previous node
                builder.add_parent(Cid::default()); // This would be the actual parent CID in practice
            }
            
            // Build and store the node
            let node = builder.build().unwrap();
            let cid = dag_store_clone.store_node(&node).await.unwrap();
            
            (cid, node)
        });
        
        handles.push(handle);
    }
    
    // Collect results
    for handle in handles {
        let (cid, _node) = handle.await.unwrap();
        node_cids.push(cid);
    }
    
    // Verify all nodes can be retrieved
    for cid in &node_cids {
        let node = dag_store.get_node(cid).await.unwrap();
        assert!(node.is_some(), "Node should exist in the DAG");
    }
    
    // Test retrieving multiple nodes in parallel
    let mut get_handles = Vec::new();
    
    for cid in &node_cids {
        let cid_copy = *cid;
        let dag_store_clone = dag_store.clone();
        
        let handle = tokio::spawn(async move {
            dag_store_clone.get_node(&cid_copy).await.unwrap()
        });
        
        get_handles.push(handle);
    }
    
    // Verify parallel retrieval
    for handle in get_handles {
        let node_opt = handle.await.unwrap();
        assert!(node_opt.is_some(), "Node should be retrievable in parallel");
    }
} 