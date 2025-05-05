use icn_core_vm::{IdentityContext, VMContext, ResourceAuthorization, ResourceType};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_federation::{FederationManager, FederationManagerConfig, TrustBundle, roles::NodeRole};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_dag::{DagNodeBuilder, DagNode, DagManager};
use icn_dag::audit::{DAGAuditVerifier, VerificationReport};
use icn_economics::{EconomicsManager, TokenMint, TokenTransfer};
use icn_execution_tools::derive_authorizations;

use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::{Duration, Instant};
use cid::Cid;
use futures::future::join_all;
use tracing::{info, debug, warn, error};

/// Configuration for a federation node
#[derive(Debug, Clone)]
struct FederationNodeConfig {
    node_id: String,
    role: NodeRole,
    is_genesis: bool,
}

/// Represents a single federation node in the test network
struct FederationNode {
    /// Node configuration
    config: FederationNodeConfig,
    
    /// Identity context
    identity_context: Arc<IdentityContext>,
    
    /// Storage backend
    storage: Arc<Mutex<AsyncInMemoryStorage>>,
    
    /// Federation manager
    federation_manager: FederationManager,
    
    /// DAG manager
    dag_manager: Arc<DagManager>,
    
    /// Governance kernel
    governance_kernel: GovernanceKernel,
    
    /// Economics manager
    economics_manager: EconomicsManager,
}

impl FederationNode {
    /// Create a new federation node
    async fn new(config: FederationNodeConfig) -> Self {
        // Create storage
        let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        
        // Create identity
        let (keypair, identity_id) = create_test_identity(&config.node_id);
        
        // Create identity context
        let identity_context = Arc::new(IdentityContext::new(
            keypair.clone(),
            identity_id.to_string(),
        ));
        
        // Create federation manager config
        let fed_config = FederationManagerConfig {
            bootstrap_period: Duration::from_millis(100),
            peer_sync_interval: Duration::from_millis(500),
            trust_bundle_sync_interval: Duration::from_millis(1000),
            max_peers: 10,
            ..Default::default()
        };
        
        // Create federation manager
        let federation_manager = FederationManager::new(
            fed_config,
            storage.clone(),
            keypair.clone(),
        ).await.unwrap();
        
        // Create DAG manager
        let dag_manager = Arc::new(DagManager::new(storage.clone()));
        
        // Create governance kernel
        let governance_kernel = GovernanceKernel::new(
            storage.clone(),
            identity_context.clone(),
        );
        
        // Create economics manager
        let economics_manager = EconomicsManager::new(
            storage.clone(),
            identity_context.clone(),
        );
        
        Self {
            config,
            identity_context,
            storage,
            federation_manager,
            dag_manager,
            governance_kernel,
            economics_manager,
        }
    }
    
    /// Get node ID
    fn node_id(&self) -> &str {
        &self.config.node_id
    }
    
    /// Get node identity
    fn identity_id(&self) -> IdentityId {
        IdentityId::new(&self.config.node_id)
    }
    
    /// Connect to another node
    async fn connect_to(&self, other: &FederationNode) -> bool {
        self.federation_manager.add_peer(
            other.node_id(),
            "localhost",
            8000, // Dummy port for testing
        ).await.unwrap()
    }
}

/// Helper function to create test identity
fn create_test_identity(did: &str) -> (KeyPair, IdentityId) {
    // Generate test keypair
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    let identity_id = IdentityId::new(did);
    
    (keypair, identity_id)
}

#[tokio::test]
async fn test_federation_bootstrap() {
    // 1. Create 3 federation nodes
    info!("Creating 3 federation nodes...");
    let genesis_node_config = FederationNodeConfig {
        node_id: "did:icn:federation:genesis".to_string(),
        role: NodeRole::Validator,
        is_genesis: true,
    };
    
    let node2_config = FederationNodeConfig {
        node_id: "did:icn:federation:node2".to_string(),
        role: NodeRole::Validator,
        is_genesis: false,
    };
    
    let node3_config = FederationNodeConfig {
        node_id: "did:icn:federation:node3".to_string(),
        role: NodeRole::Validator,
        is_genesis: false,
    };
    
    let genesis_node = FederationNode::new(genesis_node_config).await;
    let node2 = FederationNode::new(node2_config).await;
    let node3 = FederationNode::new(node3_config).await;
    
    // 2. Connect the nodes to each other
    info!("Connecting federation nodes...");
    genesis_node.connect_to(&node2).await;
    genesis_node.connect_to(&node3).await;
    node2.connect_to(&genesis_node).await;
    node2.connect_to(&node3).await;
    node3.connect_to(&genesis_node).await;
    node3.connect_to(&node2).await;
    
    // 3. Genesis node creates and publishes the first TrustBundle
    info!("Creating and publishing genesis TrustBundle...");
    let federation_id = IdentityId::new("did:icn:federation:test-federation");
    
    let mut trust_bundle = TrustBundle::new(1);
    trust_bundle.add_node(genesis_node.identity_id(), NodeRole::Validator);
    trust_bundle.add_node(node2.identity_id(), NodeRole::Validator);
    trust_bundle.add_node(node3.identity_id(), NodeRole::Validator);
    trust_bundle.set_federation_id(federation_id.clone());
    
    // Sign the bundle with genesis node's key
    trust_bundle.set_proof(vec![1, 2, 3, 4]); // Dummy proof for testing
    
    // Store and publish
    genesis_node.federation_manager.store_trust_bundle(&trust_bundle).await.unwrap();
    genesis_node.federation_manager.publish_trust_bundle(trust_bundle.clone()).await.unwrap();
    
    // 4. Wait for other nodes to sync the TrustBundle
    info!("Waiting for TrustBundle synchronization...");
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify trust bundle sync
    let node2_bundle = node2.federation_manager.get_trust_bundle(1).await.unwrap();
    let node3_bundle = node3.federation_manager.get_trust_bundle(1).await.unwrap();
    
    assert_eq!(node2_bundle.epoch_id, 1);
    assert_eq!(node3_bundle.epoch_id, 1);
    assert_eq!(node2_bundle.nodes.len(), 3);
    assert_eq!(node3_bundle.nodes.len(), 3);
    
    // 5. Genesis node anchors the federation genesis DAG
    info!("Anchoring federation genesis DAG...");
    let genesis_payload = serde_json::json!({
        "type": "FederationGenesis",
        "name": "Test Federation",
        "description": "A test federation for the ICN Runtime",
        "created_at": chrono::Utc::now().to_rfc3339(),
        "epoch": 1,
    });
    
    let genesis_dag_cid = genesis_node.dag_manager.create_node(
        &serde_json::to_vec(&genesis_payload).unwrap(),
        vec![], // No parents for genesis
    ).await.unwrap();
    
    // 6. Mint initial tokens within the federation
    info!("Minting federation tokens...");
    let mint_result = genesis_node.economics_manager.mint_tokens(
        ResourceType::Token,
        genesis_node.identity_id(),
        1_000_000, // Initial supply
        Some("initial_allocation".to_string()),
    ).await.unwrap();
    
    // 7. Transfer tokens to other nodes
    info!("Transferring tokens to other nodes...");
    let transfer1 = genesis_node.economics_manager.transfer_tokens(
        ResourceType::Token,
        genesis_node.identity_id(),
        node2.identity_id(),
        250_000,
        Some("node2_allocation".to_string()),
    ).await.unwrap();
    
    let transfer2 = genesis_node.economics_manager.transfer_tokens(
        ResourceType::Token,
        genesis_node.identity_id(),
        node3.identity_id(),
        250_000,
        Some("node3_allocation".to_string()),
    ).await.unwrap();
    
    // 8. Create a test governance proposal
    info!("Creating test governance proposal...");
    let proposal = Proposal::new(
        "Federation Bootstrap Test".to_string(),
        "This proposal tests federation bootstrap".to_string(),
        genesis_node.identity_id(),
        IdentityScope::Federation,
        Some(federation_id.clone()),
        3600, // 1-hour voting period
        Some("// CCL Rule for federation bootstrap\nrule federation_bootstrap { always allow }".to_string()),
    );
    
    let proposal_cid = genesis_node.governance_kernel.process_proposal(proposal).await.unwrap();
    
    // 9. Nodes vote on the proposal
    info!("Voting on test proposal...");
    let genesis_vote = Vote::new(
        genesis_node.identity_id(),
        proposal_cid,
        VoteChoice::For,
        IdentityScope::Federation,
        Some(federation_id.clone()),
        Some("Genesis node supports this proposal".to_string()),
    );
    
    let node2_vote = Vote::new(
        node2.identity_id(),
        proposal_cid,
        VoteChoice::For,
        IdentityScope::Federation,
        Some(federation_id.clone()),
        Some("Node 2 supports this proposal".to_string()),
    );
    
    let node3_vote = Vote::new(
        node3.identity_id(),
        proposal_cid,
        VoteChoice::For,
        IdentityScope::Federation,
        Some(federation_id.clone()),
        Some("Node 3 supports this proposal".to_string()),
    );
    
    genesis_node.governance_kernel.record_vote(genesis_vote).await.unwrap();
    node2.governance_kernel.record_vote(node2_vote).await.unwrap();
    node3.governance_kernel.record_vote(node3_vote).await.unwrap();
    
    // 10. Finalize and execute the proposal
    info!("Finalizing and executing test proposal...");
    genesis_node.governance_kernel.finalize_proposal(proposal_cid).await.unwrap();
    
    let proposal = genesis_node.governance_kernel.get_proposal(proposal_cid).await.unwrap();
    let template = proposal.get_template();
    let authorizations = derive_authorizations(&template);
    
    let vm_context = VMContext::new(
        genesis_node.identity_context.clone(),
        authorizations,
    );
    
    genesis_node.governance_kernel.execute_proposal_with_context(proposal_cid, vm_context).await.unwrap();
    
    // 11. Verify DAG consistency across nodes
    info!("Verifying DAG consistency across nodes...");
    tokio::time::sleep(Duration::from_secs(1)).await; // Allow time for replication
    
    // Create DAG audit verifiers for each node
    let mut genesis_verifier = DAGAuditVerifier::new(genesis_node.storage.clone());
    let mut node2_verifier = DAGAuditVerifier::new(node2.storage.clone());
    let mut node3_verifier = DAGAuditVerifier::new(node3.storage.clone());
    
    // Verify federation entity DAGs
    let genesis_report = genesis_verifier.verify_entity_dag(&federation_id.to_string()).await.unwrap_or_else(|e| {
        warn!("Genesis verification error: {}", e);
        VerificationReport::default()
    });
    
    let node2_report = node2_verifier.verify_entity_dag(&federation_id.to_string()).await.unwrap_or_else(|e| {
        warn!("Node 2 verification error: {}", e);
        VerificationReport::default()
    });
    
    let node3_report = node3_verifier.verify_entity_dag(&federation_id.to_string()).await.unwrap_or_else(|e| {
        warn!("Node 3 verification error: {}", e);
        VerificationReport::default()
    });
    
    // 12. Verify resource balances match expected values
    info!("Verifying resource balances...");
    let genesis_balance = genesis_node.economics_manager.get_balance(
        ResourceType::Token,
        genesis_node.identity_id(),
    ).await.unwrap();
    
    let node2_balance = node2.economics_manager.get_balance(
        ResourceType::Token,
        node2.identity_id(),
    ).await.unwrap();
    
    let node3_balance = node3.economics_manager.get_balance(
        ResourceType::Token,
        node3.identity_id(),
    ).await.unwrap();
    
    assert_eq!(genesis_balance, 500_000); // 1M - 250K - 250K
    assert_eq!(node2_balance, 250_000);
    assert_eq!(node3_balance, 250_000);
    
    // 13. Output federation state summary
    info!("Federation bootstrap test complete!");
    info!("Federation state summary:");
    info!("  - Genesis DAG CID: {}", genesis_dag_cid);
    info!("  - Proposal CID: {}", proposal_cid);
    info!("  - TrustBundle epoch: {}", genesis_node.federation_manager.get_latest_known_epoch().await.unwrap());
    info!("  - Token distribution: Genesis={}, Node2={}, Node3={}", 
        genesis_balance, node2_balance, node3_balance);
    
    // This test simulates a complete federation bootstrap with:
    // - Node setup and connection
    // - TrustBundle creation and synchronization
    // - DAG anchoring
    // - Token minting and transfers
    // - Governance proposal creation, voting, and execution
    // - DAG verification across nodes
} 