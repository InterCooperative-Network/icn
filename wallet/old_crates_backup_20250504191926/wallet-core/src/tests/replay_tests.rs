use crate::dag::{DagStorageManager, DagError};
use crate::replay::replay_and_verify_receipt;
use icn_dag::{DagNode, DagNodeData, DagNodeMetadata, ExecutionSummary};
use icn_wallet_agent::import::ExecutionReceipt;
use icn_wallet_sync::VerifiableCredential;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use async_trait::async_trait;
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::json;

/// Mock DAG storage for testing
struct MockDagStore {
    nodes: Mutex<HashMap<String, DagNode>>,
    metadata: Mutex<HashMap<String, DagNodeMetadata>>,
}

impl MockDagStore {
    fn new() -> Self {
        Self {
            nodes: Mutex::new(HashMap::new()),
            metadata: Mutex::new(HashMap::new()),
        }
    }
    
    fn add_node(&self, cid: &str, node: DagNode, metadata: DagNodeMetadata) {
        self.nodes.lock().unwrap().insert(cid.to_string(), node);
        self.metadata.lock().unwrap().insert(cid.to_string(), metadata);
    }
}

#[async_trait]
impl DagStorageManager for MockDagStore {
    async fn get_node(&self, cid: &str) -> Result<DagNode, DagError> {
        self.nodes
            .lock()
            .unwrap()
            .get(cid)
            .cloned()
            .ok_or_else(|| DagError::NotFound(cid.to_string()))
    }
    
    async fn get_metadata(&self, cid: &str) -> Result<DagNodeMetadata, DagError> {
        self.metadata
            .lock()
            .unwrap()
            .get(cid)
            .cloned()
            .ok_or_else(|| DagError::NotFound(cid.to_string()))
    }
    
    async fn node_exists(&self, cid: &str) -> Result<bool, DagError> {
        Ok(self.nodes.lock().unwrap().contains_key(cid))
    }
}

/// Create a test receipt
fn create_test_receipt(
    proposal_id: &str,
    outcome: &str,
    federation_scope: &str,
    dag_anchor: Option<&str>,
) -> ExecutionReceipt {
    let dag_anchor_json = match dag_anchor {
        Some(cid) => json!(cid),
        None => json!(null),
    };
    
    let credential = VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
        id: format!("receipt-{}", proposal_id),
        types: vec!["VerifiableCredential".to_string(), "ExecutionReceipt".to_string()],
        issuer: "did:icn:test-federation".to_string(),
        issuance_date: "2023-05-01T12:00:00Z".to_string(),
        credential_subject: json!({
            "id": "did:icn:user1",
            "proposal_id": proposal_id,
            "outcome": outcome,
            "federation_scope": federation_scope,
            "dag_anchor": dag_anchor_json,
        }),
        proof: None,
    };
    
    ExecutionReceipt {
        credential,
        proposal_id: proposal_id.to_string(),
        dag_anchor: dag_anchor.map(|s| s.to_string()),
        federation_scope: federation_scope.to_string(),
        outcome: outcome.to_string(),
    }
}

#[tokio::test]
async fn test_successful_receipt_verification() {
    // Create a mock DAG store
    let dag_store = Arc::new(MockDagStore::new());
    
    // Add a mock DAG node
    let cid = "bafybeihczzwsuj5huiqnuoo7nmwdkahxi7ny2qgwib4g34lqebzs5mmz4q";
    let proposal_id = "test-proposal-123";
    let federation_scope = "cooperative";
    
    let node = DagNode {
        parents: vec![],
        data: DagNodeData::ExecutionSummary(ExecutionSummary {
            proposal_id: proposal_id.to_string(),
            success: true,
            result: json!({"status": "completed"}),
            resource_use: None,
        }),
    };
    
    let metadata = DagNodeMetadata {
        scope: federation_scope.to_string(),
        timestamp: Some(SystemTime::now()),
        author: None,
    };
    
    dag_store.add_node(cid, node, metadata);
    
    // Create a test receipt with matching data
    let receipt = create_test_receipt(
        proposal_id,
        "Success",
        federation_scope,
        Some(cid),
    );
    
    // Verify the receipt
    let result = replay_and_verify_receipt(&receipt, &dag_store).await;
    
    // Check that verification succeeded
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[tokio::test]
async fn test_failed_receipt_verification_wrong_proposal() {
    // Create a mock DAG store
    let dag_store = Arc::new(MockDagStore::new());
    
    // Add a mock DAG node
    let cid = "bafybeihczzwsuj5huiqnuoo7nmwdkahxi7ny2qgwib4g34lqebzs5mmz4q";
    let proposal_id = "test-proposal-123";
    let federation_scope = "cooperative";
    
    let node = DagNode {
        parents: vec![],
        data: DagNodeData::ExecutionSummary(ExecutionSummary {
            proposal_id: proposal_id.to_string(),
            success: true,
            result: json!({"status": "completed"}),
            resource_use: None,
        }),
    };
    
    let metadata = DagNodeMetadata {
        scope: federation_scope.to_string(),
        timestamp: Some(SystemTime::now()),
        author: None,
    };
    
    dag_store.add_node(cid, node, metadata);
    
    // Create a test receipt with WRONG proposal ID
    let receipt = create_test_receipt(
        "wrong-proposal-id",
        "Success",
        federation_scope,
        Some(cid),
    );
    
    // Verify the receipt
    let result = replay_and_verify_receipt(&receipt, &dag_store).await;
    
    // Check that verification failed
    assert!(result.is_err());
    match result {
        Err(e) => {
            let err_str = e.to_string();
            assert!(err_str.contains("Proposal ID mismatch"));
        },
        _ => panic!("Expected error"),
    }
}

#[tokio::test]
async fn test_failed_receipt_verification_wrong_outcome() {
    // Create a mock DAG store
    let dag_store = Arc::new(MockDagStore::new());
    
    // Add a mock DAG node
    let cid = "bafybeihczzwsuj5huiqnuoo7nmwdkahxi7ny2qgwib4g34lqebzs5mmz4q";
    let proposal_id = "test-proposal-123";
    let federation_scope = "cooperative";
    
    let node = DagNode {
        parents: vec![],
        data: DagNodeData::ExecutionSummary(ExecutionSummary {
            proposal_id: proposal_id.to_string(),
            success: true, // Success
            result: json!({"status": "completed"}),
            resource_use: None,
        }),
    };
    
    let metadata = DagNodeMetadata {
        scope: federation_scope.to_string(),
        timestamp: Some(SystemTime::now()),
        author: None,
    };
    
    dag_store.add_node(cid, node, metadata);
    
    // Create a test receipt with WRONG outcome
    let receipt = create_test_receipt(
        proposal_id,
        "Failure", // Should be Success to match the node
        federation_scope,
        Some(cid),
    );
    
    // Verify the receipt
    let result = replay_and_verify_receipt(&receipt, &dag_store).await;
    
    // Check that verification failed
    assert!(result.is_err());
    match result {
        Err(e) => {
            let err_str = e.to_string();
            assert!(err_str.contains("Outcome mismatch"));
        },
        _ => panic!("Expected error"),
    }
}

#[tokio::test]
async fn test_receipt_missing_dag_anchor() {
    // Create a mock DAG store
    let dag_store = Arc::new(MockDagStore::new());
    
    // Create a test receipt with NO DAG anchor
    let receipt = create_test_receipt(
        "test-proposal-123",
        "Success",
        "cooperative",
        None,
    );
    
    // Verify the receipt
    let result = replay_and_verify_receipt(&receipt, &dag_store).await;
    
    // Check that verification failed due to missing anchor
    assert!(result.is_err());
    match result {
        Err(e) => {
            let err_str = e.to_string();
            assert!(err_str.contains("Missing DAG anchor"));
        },
        _ => panic!("Expected error"),
    }
} 