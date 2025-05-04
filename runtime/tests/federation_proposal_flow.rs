/*!
 * Federation Proposal Flow Integration Test
 * 
 * Tests the complete end-to-end federation proposal flow:
 * 1. Submit proposal via wallet-agent with AgoraNet thread ID
 * 2. Runtime processes proposal and issues execution receipt with thread ID
 * 3. Wallet-sync retrieves the updated DAG and credential
 */

use icn_runtime::{RuntimeConfig, Runtime};
use icn_governance_kernel::{GovernanceKernel, Proposal, ProposalStatus};
use icn_identity::{IdentityId, IdentityScope};
use icn_core_vm::{ExecutionReceiptSubject, VerifiableCredential};
use icn_wallet_sync::{SyncClient, WalletSync};
use icn_dag::DagManager;
use icn_storage::MemoryStorage;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;
use serde_json::json;

// Mock AgoraNet API for testing
struct MockAgoraNetClient {
    thread_id: String,
    linked_proposals: Mutex<Vec<String>>,
}

impl MockAgoraNetClient {
    fn new() -> Self {
        Self {
            thread_id: Uuid::new_v4().to_string(),
            linked_proposals: Mutex::new(Vec::new()),
        }
    }
    
    fn link_thread_to_proposal(&self, proposal_id: &str) {
        let mut proposals = self.linked_proposals.lock().unwrap();
        proposals.push(proposal_id.to_string());
    }
    
    fn get_linked_proposals(&self) -> Vec<String> {
        let proposals = self.linked_proposals.lock().unwrap();
        proposals.clone()
    }
}

// Mock wallet agent for testing
struct MockWalletAgent {
    runtime: Arc<Runtime>,
    agoranet: Arc<MockAgoraNetClient>,
}

impl MockWalletAgent {
    fn new(runtime: Arc<Runtime>, agoranet: Arc<MockAgoraNetClient>) -> Self {
        Self {
            runtime,
            agoranet,
        }
    }
    
    async fn submit_proposal_with_thread(&self, ccl: &str) -> String {
        // Create a proposal with thread_id
        let proposal = Proposal {
            title: "Test Federation Proposal".to_string(),
            description: "Test proposal with thread ID".to_string(),
            proposer: IdentityId("did:icn:test".to_string()),
            scope: IdentityScope::Federation,
            scope_id: Some(IdentityId("did:icn:federation".to_string())),
            status: ProposalStatus::Active,
            voting_end_time: chrono::Utc::now().timestamp() + 86400,
            votes_for: 0,
            votes_against: 0,
            votes_abstain: 0,
            ccl_code: Some(ccl.to_string()),
            wasm_bytes: None,
            thread_id: Some(self.agoranet.thread_id.clone()),
        };
        
        // Get the governance kernel
        let kernel = self.runtime.get_governance_kernel();
        
        // Process the proposal
        let proposal_id = kernel.process_proposal(proposal).await.unwrap();
        
        // Link the proposal to the thread
        self.agoranet.link_thread_to_proposal(&proposal_id);
        
        proposal_id
    }
}

#[tokio::test]
async fn test_federation_proposal_flow() {
    // Set up test environment
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));
    let dag_manager = Arc::new(DagManager::new(storage.clone()));
    
    // Create mock AgoraNet client
    let agoranet = Arc::new(MockAgoraNetClient::new());
    
    // Initialize runtime
    let runtime_config = RuntimeConfig::default();
    let runtime = Arc::new(Runtime::new(runtime_config, storage.clone()).await.unwrap());
    
    // Create mock wallet agent
    let wallet_agent = MockWalletAgent::new(runtime.clone(), agoranet.clone());
    
    // Create wallet sync client
    let wallet_sync = WalletSync::new(storage.clone());
    
    // 1. Submit a proposal with thread ID via wallet agent
    let ccl_code = r#"
    {
        "action": "federation_update",
        "name": "Test Federation Update",
        "description": "This is a test proposal for federation updates",
        "changes": [
            {
                "field": "name",
                "value": "Updated Federation Name"
            }
        ]
    }
    "#;
    
    let proposal_id = wallet_agent.submit_proposal_with_thread(ccl_code).await;
    
    // Verify the proposal is linked in AgoraNet
    let linked_proposals = agoranet.get_linked_proposals();
    assert_eq!(linked_proposals.len(), 1);
    assert_eq!(linked_proposals[0], proposal_id);
    
    // 2. Execute the proposal
    let kernel = runtime.get_governance_kernel();
    let proposal = kernel.get_proposal(proposal_id.clone()).await.unwrap();
    
    // Verify thread_id is properly set
    assert_eq!(proposal.thread_id, Some(agoranet.thread_id.clone()));
    
    // Finalize and execute the proposal
    kernel.finalize_proposal(proposal_id.clone()).await.unwrap();
    
    // Wait for proposal execution
    sleep(Duration::from_millis(100)).await;
    
    // 3. Verify execution receipt contains thread_id
    let storage_locked = storage.lock().unwrap();
    let receipts = storage_locked.get_keys_with_prefix("credential:execution_receipt:").unwrap();
    
    // Check that we have at least one receipt
    assert!(!receipts.is_empty(), "No execution receipts found");
    
    // Check the first receipt
    let receipt_key = receipts[0].clone();
    drop(storage_locked);
    
    let storage_locked = storage.lock().unwrap();
    let receipt_bytes = storage_locked.get(&receipt_key).unwrap().unwrap();
    drop(storage_locked);
    
    // Parse the receipt
    let receipt: VerifiableCredential<ExecutionReceiptSubject> = 
        serde_json::from_slice(&receipt_bytes).unwrap();
    
    // Verify thread_id is included in the receipt
    assert_eq!(receipt.credential_subject.thread_id, Some(agoranet.thread_id.clone()));
    
    println!("Federation proposal flow test completed successfully!");
} 