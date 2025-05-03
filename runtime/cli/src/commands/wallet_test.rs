use clap::{Arg, ArgMatches, Command};
use icn_core_vm::{IdentityContext, VMContext};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_federation::{FederationManager, FederationManagerConfig};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_execution_tools::derive_authorizations;
use wallet_types::{DagNode, DagNodeMetadata, WalletResult};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use anyhow::{Result, Context, bail};

pub fn cli() -> Command {
    Command::new("wallet-test")
        .about("Test wallet runtime integration")
        .subcommand(
            Command::new("governance-cycle")
                .about("Test full governance cycle integration")
                .arg(
                    Arg::new("user_did")
                        .long("user-did")
                        .help("User DID for testing")
                        .default_value("did:icn:test:user1")
                )
                .arg(
                    Arg::new("guardian_did")
                        .long("guardian-did")
                        .help("Guardian DID for testing")
                        .default_value("did:icn:test:guardian1")
                )
                .arg(
                    Arg::new("voting_period")
                        .long("voting-period")
                        .help("Voting period in seconds")
                        .default_value("86400")
                )
        )
}

pub async fn execute(subcmd: &str, args: &ArgMatches) -> Result<()> {
    match subcmd {
        "governance-cycle" => {
            let user_did = args.get_one::<String>("user_did").unwrap();
            let guardian_did = args.get_one::<String>("guardian_did").unwrap();
            let voting_period = args.get_one::<String>("voting_period").unwrap().parse::<u64>()
                .context("Invalid voting period")?;
            
            run_governance_cycle(user_did, guardian_did, voting_period).await?;
            Ok(())
        }
        _ => bail!("Unknown wallet-test subcommand: {}", subcmd),
    }
}

/// Helper function to create a test identity
fn create_test_identity(did: &str) -> (KeyPair, IdentityId) {
    // Generate test keypair
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    let identity_id = IdentityId::new(did);
    
    (keypair, identity_id)
}

/// Wrapper for wallet client
struct WalletClient {
    /// Keypair for signing
    keypair: KeyPair,
    /// Identity ID
    identity_id: IdentityId,
    /// Governance kernel
    governance: Arc<GovernanceKernel>,
}

impl WalletClient {
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
    
    async fn create_proposal(
        &self,
        title: String,
        description: String,
        scope: IdentityScope,
        scope_id: Option<IdentityId>,
        voting_period: u64,
    ) -> Result<String> {
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
        
        // Process proposal
        let cid = self.governance.process_proposal(proposal)
            .await
            .context("Failed to process proposal")?;
        
        Ok(cid.to_string())
    }
    
    async fn vote_on_proposal(
        &self,
        proposal_cid_str: &str,
        choice: VoteChoice,
        rationale: Option<String>,
    ) -> Result<()> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .context("Invalid proposal CID")?;
        
        // Create vote
        let vote = Vote::new(
            self.identity_id.clone(),
            proposal_cid,
            choice,
            IdentityScope::Federation,
            None,
            rationale,
        );
        
        // Record vote
        self.governance.record_vote(vote)
            .await
            .context("Failed to record vote")?;
        
        Ok(())
    }
    
    async fn finalize_proposal(&self, proposal_cid_str: &str) -> Result<()> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .context("Invalid proposal CID")?;
        
        // Finalize proposal
        self.governance.finalize_proposal(proposal_cid)
            .await
            .context("Failed to finalize proposal")?;
        
        Ok(())
    }
    
    async fn execute_proposal(&self, proposal_cid_str: &str) -> Result<()> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .context("Invalid proposal CID")?;
        
        // Get the proposal
        let proposal = self.governance.get_proposal(proposal_cid)
            .await
            .context("Failed to get proposal")?;
        
        // Get template and derive authorizations
        let template = proposal.get_template();
        let authorizations = derive_authorizations(&template);
        
        // Create VM context
        let identity_context = Arc::new(IdentityContext::new(
            self.keypair.clone(),
            self.identity_id.to_string(),
        ));
        
        let vm_context = VMContext::new(
            identity_context,
            authorizations,
        );
        
        // Execute proposal
        self.governance.execute_proposal_with_context(proposal_cid, vm_context)
            .await
            .context("Failed to execute proposal")?;
        
        Ok(())
    }
    
    async fn get_proposal_status(&self, proposal_cid_str: &str) -> Result<ProposalStatus> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .context("Invalid proposal CID")?;
        
        // Get proposal
        let proposal = self.governance.get_proposal(proposal_cid)
            .await
            .context("Failed to get proposal")?;
        
        Ok(proposal.status)
    }
    
    async fn get_proposal_credentials(&self, proposal_cid_str: &str) -> Result<Vec<String>> {
        // Parse CID
        let proposal_cid = proposal_cid_str.parse()
            .context("Invalid proposal CID")?;
        
        // Get credentials
        let credentials = self.governance.get_proposal_credentials(proposal_cid)
            .await;
        
        // Convert to strings
        let credential_strings = credentials.iter()
            .map(|cred| format!("{}", cred.id))
            .collect();
        
        Ok(credential_strings)
    }
}

async fn run_governance_cycle(user_did: &str, guardian_did: &str, voting_period: u64) -> Result<()> {
    println!("Starting governance cycle test");
    println!("User DID: {}", user_did);
    println!("Guardian DID: {}", guardian_did);
    println!("Voting period: {} seconds", voting_period);
    
    // 1. Set up common storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Create identities
    let (user_keypair, user_id) = create_test_identity(user_did);
    let (guardian_keypair, guardian_id) = create_test_identity(guardian_did);
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
    ).await.context("Failed to create federation manager")?;
    
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
    
    println!("\n=== STARTING FULL GOVERNANCE CYCLE TEST ===\n");
    
    // STEP 1: User creates a proposal
    println!("STEP 1: Creating proposal from user wallet");
    let proposal_cid = user_wallet.create_proposal(
        "CLI Test Governance Proposal".to_string(),
        "This is a test proposal for the full governance cycle".to_string(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        voting_period,
    ).await.context("Failed to create proposal")?;
    
    println!("Created proposal with CID: {}", proposal_cid);
    
    // Verify proposal exists
    let proposal_status = user_wallet.get_proposal_status(&proposal_cid).await?;
    println!("Proposal status: {:?}", proposal_status);
    
    // STEP 2: User votes on the proposal
    println!("\nSTEP 2: Voting on proposal");
    user_wallet.vote_on_proposal(
        &proposal_cid,
        VoteChoice::For,
        Some("I support this proposal".to_string())
    ).await.context("Failed to vote on proposal")?;
    println!("User vote recorded");
    
    // Guardian also votes
    guardian_wallet.vote_on_proposal(
        &proposal_cid,
        VoteChoice::For,
        Some("As a guardian, I approve this proposal".to_string())
    ).await.context("Failed to vote on proposal")?;
    println!("Guardian vote recorded");
    
    // STEP 3: Guardian finalizes the proposal
    println!("\nSTEP 3: Finalizing proposal");
    guardian_wallet.finalize_proposal(&proposal_cid)
        .await
        .context("Failed to finalize proposal")?;
    
    // Verify proposal is finalized
    let proposal_status = user_wallet.get_proposal_status(&proposal_cid).await?;
    println!("Proposal status after finalization: {:?}", proposal_status);
    
    // STEP 4: Execute the proposal
    println!("\nSTEP 4: Executing proposal");
    user_wallet.execute_proposal(&proposal_cid)
        .await
        .context("Failed to execute proposal")?;
    
    // Verify proposal is executed
    let proposal_status = user_wallet.get_proposal_status(&proposal_cid).await?;
    println!("Proposal status after execution: {:?}", proposal_status);
    
    // STEP 5: Retrieve credentials
    println!("\nSTEP 5: Retrieving credentials");
    let credentials = user_wallet.get_proposal_credentials(&proposal_cid)
        .await
        .context("Failed to get credentials")?;
    
    println!("Retrieved {} credentials:", credentials.len());
    for (i, cred) in credentials.iter().enumerate() {
        println!("  {}. {}", i+1, cred);
    }
    
    // STEP 6: Verify events were emitted
    println!("\nSTEP 6: Verifying events");
    let events = governance_kernel.get_proposal_events(proposal_cid.parse()?).await;
    
    println!("Event timeline:");
    for (i, event) in events.iter().enumerate() {
        println!("  {}. {} - {}", i+1, event.event_type, event.description);
    }
    
    println!("\n=== FULL GOVERNANCE CYCLE TEST COMPLETED SUCCESSFULLY ===");
    
    Ok(())
} 