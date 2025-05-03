use icn_governance_kernel::{
    GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus, GovernanceError,
    config::{GovernanceConfig, GovernanceStructure, Role, IdentityInfo}
};
use icn_identity::{IdentityId, IdentityScope};
use icn_storage::AsyncInMemoryStorage;
use icn_core_vm::IdentityContext;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Create a test identity context for testing
fn create_test_identity_context() -> Arc<IdentityContext> {
    // Generate a simple keypair for testing
    let private_key = vec![1, 2, 3, 4]; // Dummy private key
    let public_key = vec![5, 6, 7, 8]; // Dummy public key
    let keypair = icn_identity::KeyPair::new(private_key, public_key);
    
    // Create the identity context
    let identity_context = IdentityContext::new(keypair, "did:icn:test");
    
    Arc::new(identity_context)
}

/// Create a governance config with roles for testing
fn create_test_governance_config() -> GovernanceConfig {
    let admin_role = Role {
        name: "admin".to_string(),
        permissions: vec![
            "create_proposals".to_string(),
            "vote_on_proposals".to_string(),
            "finalize_proposals".to_string(),
            "execute_proposals".to_string(),
        ],
    };
    
    let voter_role = Role {
        name: "voter".to_string(),
        permissions: vec![
            "vote_on_proposals".to_string(),
        ],
    };
    
    let governance = GovernanceStructure {
        decision_making: Some("majority".to_string()),
        quorum: Some(0.5),
        majority: Some(0.66),
        term_length: None,
        roles: Some(vec![admin_role, voter_role]),
    };
    
    let identity = IdentityInfo {
        name: Some("Test Community".to_string()),
        description: Some("A test community".to_string()),
        founding_date: None,
        mission_statement: None,
    };
    
    GovernanceConfig {
        template_type: "community_charter".to_string(),
        template_version: "v1".to_string(),
        governing_scope: IdentityScope::Community,
        identity: Some(identity),
        governance: Some(governance),
        membership: None,
        proposals: None,
        working_groups: None,
        dispute_resolution: None,
        economic_model: None,
    }
}

/// Create a test proposal for a specific scope
fn create_test_proposal(proposer: &IdentityId, scope_id: &str) -> Proposal {
    Proposal {
        title: "Test Proposal".to_string(),
        description: "This is a test proposal".to_string(),
        proposer: proposer.clone(),
        scope: IdentityScope::Community,
        scope_id: Some(IdentityId(scope_id.to_string())),
        status: ProposalStatus::Draft,
        voting_end_time: chrono::Utc::now().timestamp() + 86400, // 24 hour voting period
        votes_for: 0,
        votes_against: 0,
        votes_abstain: 0,
        ccl_code: None,
        wasm_bytes: None,
    }
}

/// Create a test vote for a specific proposal and scope
fn create_test_vote(voter: &IdentityId, proposal_id: &str, scope_id: &str) -> Vote {
    Vote {
        voter: voter.clone(),
        proposal_id: proposal_id.to_string(),
        choice: VoteChoice::For,
        weight: 1,
        scope: IdentityScope::Community,
        scope_id: Some(IdentityId(scope_id.to_string())),
        reason: Some("Support test".to_string()),
        timestamp: chrono::Utc::now().timestamp(),
    }
}

#[tokio::test]
async fn test_unauthorized_proposal_creation() {
    // Set up test environment with storage and identity
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Define scope and identities
    let scope_id = "did:icn:community:test";
    let admin_id = IdentityId("did:icn:admin".to_string());
    let user_id = IdentityId("did:icn:user".to_string());
    
    // Create and store a governance config
    let config = create_test_governance_config();
    kernel.store_governance_config(scope_id, config).await.unwrap();
    
    // Assign the admin role to admin_id
    kernel.assign_roles(&admin_id, scope_id, vec!["admin".to_string()]).await.unwrap();
    
    // Create a test proposal as a user without permissions
    let user_proposal = create_test_proposal(&user_id, scope_id);
    
    // Try to process the proposal, should fail due to lack of permissions
    let result = kernel.process_proposal(user_proposal).await;
    
    // Verify that an Unauthorized error was returned
    assert!(matches!(result, Err(GovernanceError::Unauthorized(_))));
    
    // Now try with the admin who has permission
    let admin_proposal = create_test_proposal(&admin_id, scope_id);
    
    // Should succeed
    let result = kernel.process_proposal(admin_proposal).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_unauthorized_vote() {
    // Set up test environment with storage and identity
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Define scope and identities
    let scope_id = "did:icn:community:test";
    let admin_id = IdentityId("did:icn:admin".to_string());
    let voter_id = IdentityId("did:icn:voter".to_string());
    let user_id = IdentityId("did:icn:user".to_string());
    
    // Create and store a governance config
    let config = create_test_governance_config();
    kernel.store_governance_config(scope_id, config).await.unwrap();
    
    // Assign roles
    kernel.assign_roles(&admin_id, scope_id, vec!["admin".to_string()]).await.unwrap();
    kernel.assign_roles(&voter_id, scope_id, vec!["voter".to_string()]).await.unwrap();
    
    // Create a proposal as admin
    let admin_proposal = create_test_proposal(&admin_id, scope_id);
    let proposal_id = kernel.process_proposal(admin_proposal).await.unwrap();
    
    // Try to vote as a regular user, should fail
    let user_vote = create_test_vote(&user_id, &proposal_id, scope_id);
    let result = kernel.record_vote(user_vote).await;
    
    // Verify that an Unauthorized error was returned
    assert!(matches!(result, Err(GovernanceError::Unauthorized(_))));
    
    // Try to vote as an authorized voter, should succeed
    let voter_vote = create_test_vote(&voter_id, &proposal_id, scope_id);
    let result = kernel.record_vote(voter_vote).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_missing_governance_config() {
    // Set up test environment with storage and identity
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Define scope and identities
    let scope_id = "did:icn:community:nonexistent";
    let user_id = IdentityId("did:icn:user".to_string());
    
    // Create a test proposal for a scope without a governance config
    let proposal = create_test_proposal(&user_id, scope_id);
    
    // Try to process the proposal, should fail due to missing governance config
    let result = kernel.process_proposal(proposal).await;
    
    // Verify that an Unauthorized error was returned
    assert!(matches!(result, Err(GovernanceError::Unauthorized(_))));
}

#[tokio::test]
async fn test_permission_inheritance() {
    // Set up test environment with storage and identity
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Define scope and identities
    let scope_id = "did:icn:community:test";
    let admin_id = IdentityId("did:icn:admin".to_string());
    
    // Create a governance config with multiple roles
    let config = create_test_governance_config();
    kernel.store_governance_config(scope_id, config).await.unwrap();
    
    // Assign multiple roles to admin
    kernel.assign_roles(&admin_id, scope_id, vec!["admin".to_string(), "voter".to_string()]).await.unwrap();
    
    // Create a test proposal
    let proposal = create_test_proposal(&admin_id, scope_id);
    
    // Process the proposal, should succeed since admin has the create_proposals permission
    let result = kernel.process_proposal(proposal).await;
    assert!(result.is_ok());
    
    // Get the roles assigned to admin
    let roles = kernel.get_assigned_roles(&admin_id, scope_id).await.unwrap();
    
    // Verify that the admin has both roles
    assert_eq!(roles.len(), 2);
    assert!(roles.contains(&"admin".to_string()));
    assert!(roles.contains(&"voter".to_string()));
}

#[tokio::test]
async fn test_role_assignment() {
    // Set up test environment with storage and identity
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let identity_ctx = create_test_identity_context();
    
    // Create a governance kernel instance
    let kernel = GovernanceKernel::new(storage.clone(), identity_ctx.clone());
    
    // Define scope and identities
    let scope_id = "did:icn:community:test";
    let user_id = IdentityId("did:icn:user".to_string());
    
    // Assign roles to a user
    let roles = vec!["custom_role".to_string(), "another_role".to_string()];
    kernel.assign_roles(&user_id, scope_id, roles.clone()).await.unwrap();
    
    // Get the roles assigned to the user
    let assigned_roles = kernel.get_assigned_roles(&user_id, scope_id).await.unwrap();
    
    // Verify that the assigned roles match
    assert_eq!(assigned_roles.len(), 2);
    assert!(assigned_roles.contains(&"custom_role".to_string()));
    assert!(assigned_roles.contains(&"another_role".to_string()));
    
    // Update the roles
    let new_roles = vec!["new_role".to_string()];
    kernel.assign_roles(&user_id, scope_id, new_roles.clone()).await.unwrap();
    
    // Get the updated roles
    let updated_roles = kernel.get_assigned_roles(&user_id, scope_id).await.unwrap();
    
    // Verify that the roles were updated
    assert_eq!(updated_roles.len(), 1);
    assert!(updated_roles.contains(&"new_role".to_string()));
    assert!(!updated_roles.contains(&"custom_role".to_string()));
} 