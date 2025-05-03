use std::sync::Arc;
use std::time::Duration;

use icn_core_vm::{IdentityContext, VMContext};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_federation::{FederationManager, FederationManagerConfig};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_execution_tools::derive_authorizations;
use tokio::sync::Mutex;

// Wallet types for integration
use wallet_types::{DagNode, DagNodeMetadata, WalletResult};

/// Simulates a wallet client for testing
struct WalletClient {
    /// Key pair for signing
    keypair: KeyPair,
    /// Identity ID (DID)
    identity_id: IdentityId,
    /// Governance kernel interface
    governance: Arc<GovernanceKernel>,
}

impl WalletClient {
    /// Create a new wallet client
    fn new(
        keypair: KeyPair,
        identity_id: IdentityId,
        governance: Arc<GovernanceKernel>,
    ) -> Self {
        Self {
            keypair,
            identity_id,
            governance,
        }
    }
    
    /// Create a new governance proposal
    async fn create_proposal(
        &self,
        title: String,
        description: String,
        scope: IdentityScope,
        scope_id: Option<IdentityId>,
        voting_period: u64,
    ) -> Result<String, String> {
        // Create proposal
        let proposal = Proposal::new(
            title,
            description,
            self.identity_id.clone(),
            scope,
            scope_id,
            voting_period,
            None, // No CCL code for now
        );
        
        // Process proposal through governance kernel
        let cid = self.governance.process_proposal(proposal)
            .await
            .map_err(|e| format!("Failed to process proposal: {}", e))?;
        
        Ok(cid.to_string())
    }
    
    /// Vote on a proposal
    async fn vote_on_proposal(
        &self,
        proposal_cid_str: &str,
        choice: VoteChoice,
        rationale: Option<String>,
    ) -> Result<(), String> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .map_err(|e| format!("Invalid proposal CID: {}", e))?;
        
        // Create vote
        let vote = Vote::new(
            self.identity_id.clone(),
            proposal_cid,
            choice,
            IdentityScope::Federation, // Assuming federation scope
            None, // No specific scope ID
            rationale,
        );
        
        // Record vote through governance kernel
        self.governance.record_vote(vote)
            .await
            .map_err(|e| format!("Failed to record vote: {}", e))?;
        
        Ok(())
    }
    
    /// Finalize a proposal (admin/guardian function)
    async fn finalize_proposal(&self, proposal_cid_str: &str) -> Result<(), String> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .map_err(|e| format!("Invalid proposal CID: {}", e))?;
        
        // Finalize proposal through governance kernel
        self.governance.finalize_proposal(proposal_cid)
            .await
            .map_err(|e| format!("Failed to finalize proposal: {}", e))?;
        
        Ok(())
    }
    
    /// Execute a proposal
    async fn execute_proposal(&self, proposal_cid_str: &str) -> Result<(), String> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .map_err(|e| format!("Invalid proposal CID: {}", e))?;
        
        // Get the proposal to check template
        let proposal = self.governance.get_proposal(proposal_cid)
            .await
            .map_err(|e| format!("Failed to get proposal: {}", e))?;
        
        // Get template and derive authorizations
        let template = proposal.get_template();
        let authorizations = derive_authorizations(&template);
        
        // Create VM context with identity context and authorizations
        let identity_context = Arc::new(IdentityContext::new(
            self.keypair.clone(),
            self.identity_id.to_string(),
        ));
        
        let vm_context = VMContext::new(
            identity_context,
            authorizations,
        );
        
        // Execute the proposal with context
        self.governance.execute_proposal_with_context(proposal_cid, vm_context)
            .await
            .map_err(|e| format!("Failed to execute proposal: {}", e))?;
        
        Ok(())
    }
    
    /// Get proposal status
    async fn get_proposal_status(&self, proposal_cid_str: &str) -> Result<ProposalStatus, String> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .map_err(|e| format!("Invalid proposal CID: {}", e))?;
        
        // Get proposal through governance kernel
        let proposal = self.governance.get_proposal(proposal_cid)
            .await
            .map_err(|e| format!("Failed to get proposal: {}", e))?;
        
        Ok(proposal.status)
    }
    
    /// Get credentials for a proposal
    async fn get_proposal_credentials(&self, proposal_cid_str: &str) -> Result<Vec<String>, String> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .map_err(|e| format!("Invalid proposal CID: {}", e))?;
        
        // Get credentials through governance kernel
        let credentials = self.governance.get_proposal_credentials(proposal_cid)
            .await;
        
        // Convert to strings for simplicity
        let credential_strings = credentials.iter()
            .map(|cred| format!("{}", cred.id))
            .collect();
        
        Ok(credential_strings)
    }
}

// Helper function to create test identity
fn create_test_identity(did: &str) -> (KeyPair, IdentityId) {
    // Generate test keypair
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    let identity_id = IdentityId::new(did);
    
    (keypair, identity_id)
}

/// Test the full governance cycle from proposal to execution
#[tokio::test]
async fn test_full_governance_cycle() {
    // 1. Set up common storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Create identities
    let (user_keypair, user_id) = create_test_identity("did:icn:user1");
    let (guardian_keypair, guardian_id) = create_test_identity("did:icn:guardian1");
    let federation_id = IdentityId::new("did:icn:federation:test");
    
    // 3. Create identity context for runtime
    let identity_context = Arc::new(IdentityContext::new(
        user_keypair.clone(),
        user_id.to_string()
    ));
    
    // 4. Initialize governance kernel
    let governance_kernel = Arc::new(GovernanceKernel::new(
        storage.clone(),
        identity_context.clone()
    ));
    
    // 5. Initialize federation manager
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
    
    // 6. Create wallet clients
    let user_wallet = WalletClient::new(
        user_keypair.clone(),
        user_id.clone(),
        governance_kernel.clone()
    );
    
    let guardian_wallet = WalletClient::new(
        guardian_keypair.clone(), 
        guardian_id.clone(),
        governance_kernel.clone()
    );
    
    println!("=== STARTING FULL GOVERNANCE CYCLE TEST ===");
    
    // STEP 1: User creates a proposal
    println!("STEP 1: Creating proposal from user wallet");
    let proposal_cid = user_wallet.create_proposal(
        "Test Governance Proposal".to_string(),
        "This is a test proposal for the full governance cycle".to_string(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        86400, // 24-hour voting period
    ).await.expect("Failed to create proposal");
    
    println!("Created proposal with CID: {}", proposal_cid);
    
    // Verify proposal exists
    let proposal_status = user_wallet.get_proposal_status(&proposal_cid).await.unwrap();
    assert_eq!(proposal_status, ProposalStatus::Voting, "Proposal should be in voting state");
    
    // STEP 2: User votes on the proposal
    println!("STEP 2: Voting on proposal");
    user_wallet.vote_on_proposal(
        &proposal_cid,
        VoteChoice::For,
        Some("I support this proposal".to_string())
    ).await.expect("Failed to vote on proposal");
    
    // Guardian also votes
    guardian_wallet.vote_on_proposal(
        &proposal_cid,
        VoteChoice::For,
        Some("As a guardian, I approve this proposal".to_string())
    ).await.expect("Failed to vote on proposal");
    
    // STEP 3: Guardian finalizes the proposal
    println!("STEP 3: Finalizing proposal");
    guardian_wallet.finalize_proposal(&proposal_cid)
        .await
        .expect("Failed to finalize proposal");
    
    // Verify proposal is finalized
    let proposal_status = user_wallet.get_proposal_status(&proposal_cid).await.unwrap();
    assert_eq!(proposal_status, ProposalStatus::Passed, "Proposal should have passed");
    
    // STEP 4: Execute the proposal
    println!("STEP 4: Executing proposal");
    user_wallet.execute_proposal(&proposal_cid)
        .await
        .expect("Failed to execute proposal");
    
    // Verify proposal is executed
    let proposal_status = user_wallet.get_proposal_status(&proposal_cid).await.unwrap();
    assert_eq!(proposal_status, ProposalStatus::Executed, "Proposal should be executed");
    
    // STEP 5: Retrieve credentials
    println!("STEP 5: Retrieving credentials");
    let credentials = user_wallet.get_proposal_credentials(&proposal_cid)
        .await
        .expect("Failed to get credentials");
    
    println!("Retrieved {} credentials", credentials.len());
    assert!(!credentials.is_empty(), "Should have received at least one credential");
    
    // STEP 6: Verify events were emitted
    println!("STEP 6: Verifying events");
    let events = governance_kernel.get_proposal_events(proposal_cid.parse().unwrap()).await;
    
    // Should have 4 events: create, 2 votes, finalize, execute
    assert_eq!(events.len(), 5, "Should have 5 events");
    
    println!("=== FULL GOVERNANCE CYCLE TEST COMPLETED SUCCESSFULLY ===");
} 