use std::collections::HashMap;
use std::time::SystemTime;
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::DagNode;
use wallet_core::store::{LocalWalletStore, MemoryStore};
use crate::sync_manager::{SyncManager, SyncManagerConfig, NodeSubmissionResponse};
use crate::error::SyncResult;
use mockito::{mock, server_url};

// Helper to create a test SyncManager with mocked HTTP endpoints
fn create_test_sync_manager() -> (SyncManager<MemoryStore>, MemoryStore) {
    // Create an identity
    let identity = IdentityWallet::generate().unwrap();
    
    // Create a memory store
    let store = MemoryStore::new();
    
    // Configure the sync manager to use mockito server
    let config = SyncManagerConfig {
        federation_urls: vec![server_url()],
        sync_interval_seconds: 1, // Short interval for testing
        auto_sync_on_startup: false,
        auto_sync_periodic: false,
        request_timeout_seconds: 5,
        max_retry_attempts: 1,
        sync_state_path: std::path::PathBuf::from("./test_sync"),
    };
    
    (SyncManager::new(identity, store.clone(), Some(config)), store)
}

// Helper to create a test DAG node
fn create_test_dag_node(cid: &str, content: serde_json::Value) -> DagNode {
    DagNode {
        cid: cid.to_string(),
        parents: vec![],
        epoch: 1,
        creator: "did:icn:test".to_string(),
        timestamp: SystemTime::now(),
        content_type: "test".to_string(),
        content,
        signatures: vec![],
    }
}

#[tokio::test]
async fn test_get_network_status() {
    // Setup mock server for health check
    let _m = mock("GET", "/health")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":"healthy"}"#)
        .create();
    
    // Create test sync manager
    let (sync_manager, _) = create_test_sync_manager();
    
    // Get network status
    let status = sync_manager.get_network_status().await.unwrap();
    
    // Verify
    assert!(status.is_connected);
    assert!(status.primary_node_latency.is_some());
}

#[tokio::test]
async fn test_network_status_disconnected() {
    // Setup mock server for health check with failure
    let _m = mock("GET", "/health")
        .with_status(500)
        .create();
    
    // Create test sync manager
    let (sync_manager, _) = create_test_sync_manager();
    
    // Get network status
    let status = sync_manager.get_network_status().await.unwrap();
    
    // Verify
    assert!(!status.is_connected);
    assert!(status.primary_node_latency.is_some()); // Still measures latency even for errors
}

#[tokio::test]
async fn test_batch_submission() {
    // Create test nodes
    let nodes = vec![
        create_test_dag_node("test1", serde_json::json!({"action": "test1"})),
        create_test_dag_node("test2", serde_json::json!({"action": "test2"})),
    ];
    
    // Setup mock server for batch submission
    let _m = mock("POST", "/nodes/batch")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[
            {"success": true, "cid": "test1"},
            {"success": true, "cid": "test2"}
        ]"#)
        .create();
    
    // Create test sync manager
    let (sync_manager, _) = create_test_sync_manager();
    
    // Submit batch
    let responses = sync_manager.submit_dag_nodes_batch(&nodes).await.unwrap();
    
    // Verify
    assert_eq!(responses.len(), 2);
    assert!(responses[0].success);
    assert_eq!(responses[0].cid, Some("test1".to_string()));
    assert!(responses[1].success);
    assert_eq!(responses[1].cid, Some("test2".to_string()));
}

#[tokio::test]
async fn test_batch_submission_failure() {
    // Create test nodes
    let nodes = vec![
        create_test_dag_node("test1", serde_json::json!({"action": "test1"})),
        create_test_dag_node("test2", serde_json::json!({"action": "test2"})),
    ];
    
    // Setup mock server for batch submission with failure
    let _m = mock("POST", "/nodes/batch")
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error": "Server error"}"#)
        .create();
    
    // Create test sync manager
    let (sync_manager, _) = create_test_sync_manager();
    
    // Submit batch and expect error
    let result = sync_manager.submit_dag_nodes_batch(&nodes).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_batch_submission() {
    // Create empty nodes array
    let nodes: Vec<DagNode> = vec![];
    
    // Create test sync manager
    let (sync_manager, _) = create_test_sync_manager();
    
    // Submit empty batch
    let responses = sync_manager.submit_dag_nodes_batch(&nodes).await.unwrap();
    
    // Verify
    assert!(responses.is_empty());
} 