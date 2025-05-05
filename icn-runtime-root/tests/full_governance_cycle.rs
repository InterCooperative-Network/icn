use std::sync::Arc;
use std::time::Duration;

use icn_core_vm::{IdentityContext, VMContext};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_federation::{FederationManager, FederationManagerConfig};
use icn_identity::{IdentityId, IdentityScope, KeyPair, IdentityError};
use icn_storage::AsyncInMemoryStorage;
use icn_execution_tools::derive_authorizations;
use tokio::sync::Mutex;

// Wallet types for integration
use wallet_types::{DagNode, DagNodeMetadata, WalletResult, WalletError, FromRuntimeError};

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
    
    /// Create a proposal with binary data as the description
    async fn create_binary_proposal(
        &self,
        title: String,
        binary_description: Vec<u8>,
        scope: IdentityScope,
        scope_id: Option<IdentityId>,
        voting_period: u64,
    ) -> Result<String, String> {
        // Create proposal with binary data
        let mut proposal = Proposal::new(
            title,
            "Binary data proposal".to_string(), // Placeholder normal description
            self.identity_id.clone(),
            scope,
            scope_id,
            voting_period,
            None, // No CCL code for now
        );
        
        // Set binary data as extra field
        proposal.set_extra_data(binary_description);
        
        // Process proposal through governance kernel
        let cid = self.governance.process_proposal(proposal)
            .await
            .map_err(|e| format!("Failed to process proposal: {}", e))?;
        
        Ok(cid.to_string())
    }
    
    /// Create a proposal but return WalletResult for testing error handling
    async fn create_proposal_with_wallet_result(
        &self,
        title: String,
        description: String,
        scope: IdentityScope,
        scope_id: Option<IdentityId>,
        voting_period: u64,
    ) -> WalletResult<String> {
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
        
        // Process proposal through governance kernel with proper error conversion
        let result = self.governance.process_proposal(proposal).await;
        
        // Convert the result to WalletResult using FromRuntimeError
        let cid = result.convert_runtime_error()?;
        
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
    
    /// Get the binary extra data from a proposal if available
    async fn get_proposal_binary_data(&self, proposal_cid_str: &str) -> Result<Option<Vec<u8>>, String> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .map_err(|e| format!("Invalid proposal CID: {}", e))?;
        
        // Get proposal through governance kernel
        let proposal = self.governance.get_proposal(proposal_cid)
            .await
            .map_err(|e| format!("Failed to get proposal: {}", e))?;
        
        Ok(proposal.get_extra_data().cloned())
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
    
    /// Test function to simulate error propagation
    async fn simulate_identity_error(&self) -> WalletResult<()> {
        // Simulate an identity error from the runtime
        let identity_error = IdentityError::VerificationFailed("Signature verification failed".to_string());
        
        // Convert the error using FromRuntimeError trait
        let result: Result<(), _> = Err(identity_error);
        result.convert_runtime_error()
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
    
    // STEP 7: Binary data proposal test
    println!("STEP 7: Testing binary data proposal");
    
    // Create arbitrary binary data (simulating non-UTF8 content)
    let binary_data = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, // JPEG header
        0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00, 0x48,
        0x00, 0x48, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 
        // Random binary data
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0
    ];
    
    // Create a proposal with binary data
    let binary_proposal_cid = user_wallet.create_binary_proposal(
        "Binary Data Test Proposal".to_string(),
        binary_data.clone(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        86400,
    ).await.expect("Failed to create binary proposal");
    
    println!("Created binary proposal with CID: {}", binary_proposal_cid);
    
    // Verify proposal exists
    let proposal_status = user_wallet.get_proposal_status(&binary_proposal_cid).await.unwrap();
    assert_eq!(proposal_status, ProposalStatus::Voting, "Binary proposal should be in voting state");
    
    // Vote and finalize the binary proposal
    guardian_wallet.vote_on_proposal(
        &binary_proposal_cid,
        VoteChoice::For,
        Some("Approving binary proposal".to_string())
    ).await.expect("Failed to vote on binary proposal");
    
    guardian_wallet.finalize_proposal(&binary_proposal_cid)
        .await
        .expect("Failed to finalize binary proposal");
    
    // Execute the binary proposal
    user_wallet.execute_proposal(&binary_proposal_cid)
        .await
        .expect("Failed to execute binary proposal");
    
    // Retrieve and verify the binary data
    let retrieved_binary = user_wallet.get_proposal_binary_data(&binary_proposal_cid)
        .await
        .expect("Failed to get binary data")
        .expect("Binary data should be present");
    
    assert_eq!(retrieved_binary, binary_data, "Binary data should be preserved exactly");
    
    // STEP 8: Test edge cases with binary data
    println!("STEP 8: Testing binary data edge cases");
    
    // Test with empty data
    let empty_data: Vec<u8> = vec![];
    let empty_proposal_cid = user_wallet.create_binary_proposal(
        "Empty Binary Data Test".to_string(),
        empty_data.clone(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        86400,
    ).await.expect("Failed to create empty data proposal");
    
    // Retrieve and verify the empty data
    let retrieved_empty = user_wallet.get_proposal_binary_data(&empty_proposal_cid)
        .await
        .expect("Failed to get empty data")
        .expect("Empty data should be present");
    
    assert_eq!(retrieved_empty, empty_data, "Empty data should be preserved");
    
    // Test with large binary data
    let large_data = vec![0xAA; 100_000]; // 100KB of data
    let large_proposal_cid = user_wallet.create_binary_proposal(
        "Large Binary Data Test".to_string(),
        large_data.clone(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        86400,
    ).await.expect("Failed to create large data proposal");
    
    // Retrieve and verify the large data
    let retrieved_large = user_wallet.get_proposal_binary_data(&large_proposal_cid)
        .await
        .expect("Failed to get large data")
        .expect("Large data should be present");
    
    assert_eq!(retrieved_large.len(), large_data.len(), "Large data size should be preserved");
    assert_eq!(retrieved_large[0], 0xAA, "Large data content should be preserved");
    assert_eq!(retrieved_large[99_999], 0xAA, "Large data content should be preserved");
    
    // STEP 9: Test error propagation
    println!("STEP 9: Testing error propagation");
    
    // Use WalletResult directly to test error conversion
    let result = user_wallet.create_proposal_with_wallet_result(
        "Error Test Proposal".to_string(),
        "This is a test for error handling".to_string(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        86400,
    ).await;
    
    assert!(result.is_ok(), "Proposal creation with WalletResult should succeed");
    
    // Test direct identity error conversion
    let identity_err_result = user_wallet.simulate_identity_error().await;
    assert!(identity_err_result.is_err(), "Identity error simulation should fail");
    
    if let Err(err) = identity_err_result {
        match err {
            WalletError::ValidationError(msg) => {
                assert!(msg.contains("verification failed"), 
                       "Error should be properly converted to ValidationError");
            },
            _ => panic!("Expected ValidationError but got {:?}", err),
        }
    }
    
    println!("=== FULL GOVERNANCE CYCLE TEST COMPLETED SUCCESSFULLY ===");
}

/// This test simulates error handling between Runtime and Wallet
#[tokio::test]
async fn test_error_propagation() {
    // 1. Set up common storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Create identities
    let (user_keypair, user_id) = create_test_identity("did:icn:user1");
    
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
    
    // 5. Create wallet client
    let user_wallet = WalletClient::new(
        user_keypair.clone(),
        user_id.clone(),
        governance_kernel.clone()
    );
    
    println!("=== TESTING ERROR PROPAGATION ===");
    
    // Test identity error propagation
    let identity_err_result = user_wallet.simulate_identity_error().await;
    assert!(identity_err_result.is_err(), "Identity error simulation should fail");
    
    if let Err(err) = identity_err_result {
        match err {
            WalletError::ValidationError(msg) => {
                println!("Correctly converted identity error to validation error: {}", msg);
                assert!(msg.contains("verification failed"), 
                       "Error should be properly converted to ValidationError");
            },
            _ => panic!("Expected ValidationError but got {:?}", err),
        }
    }
    
    // Test propagation with invalid input (using nonexistent CID)
    let invalid_cid = "bafybeigxbykuxlsaeyu7e5etb3br3blm7shcdhs7eubakg5xcdmxppyxly"; // Made up CID
    let status_result = user_wallet.get_proposal_status(invalid_cid).await;
    
    assert!(status_result.is_err(), "Should fail with invalid CID");
    println!("Error with invalid CID: {}", status_result.unwrap_err());
    
    println!("=== ERROR PROPAGATION TESTS COMPLETED SUCCESSFULLY ===");
} 