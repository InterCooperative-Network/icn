use std::collections::HashMap;
use std::time::SystemTime;
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::DagNode;
use wallet_core::store::{LocalWalletStore, MemoryStore};
use crate::sync_manager::{SyncManager, SyncManagerConfig, NodeSubmissionResponse};
use crate::error::SyncResult;
use mockito::{mock, server_url};
use wallet_types::TrustBundle;

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

#[cfg(test)]
mod network_status_tests {
    use super::*;
    use crate::sync_manager::{SyncManager, SyncManagerConfig, NetworkStatus};
    use crate::error::{SyncError, SyncResult};
    use wallet_core::identity::IdentityWallet;
    use wallet_core::crypto::KeyPair;
    use wallet_core::store::LocalWalletStore;
    use wallet_core::dag::DagNode;
    use std::time::SystemTime;
    use tempfile::tempdir;
    
    // Create a test identity
    fn create_test_identity() -> IdentityWallet {
        let keypair = KeyPair {
            public_key: vec![1, 2, 3, 4],
            private_key: Some(vec![5, 6, 7, 8]),
            key_type: "ed25519".to_string(),
        };
        
        IdentityWallet {
            did: "did:icn:test".to_string(),
            keypair,
            created_at: SystemTime::now(),
            metadata: serde_json::json!({}),
        }
    }
    
    // Create a test DAG node
    fn create_test_dag_node() -> DagNode {
        DagNode {
            cid: "bafy123".to_string(),
            parents: vec![],
            epoch: 0,
            creator: "did:icn:test".to_string(),
            timestamp: SystemTime::now(),
            content_type: "test".to_string(),
            content: serde_json::json!({}),
            signatures: vec![],
        }
    }
    
    #[tokio::test]
    async fn test_network_status_offline_effects() {
        // Create a mock store
        let temp_dir = tempdir().unwrap();
        let store = create_mock_store(temp_dir.path().to_str().unwrap());
        
        // Create a test identity
        let identity = create_test_identity();
        
        // Create a configuration with no federation URLs to ensure it's offline
        let config = SyncManagerConfig {
            federation_urls: vec![],
            sync_interval_seconds: 60,
            auto_sync_on_startup: false,
            auto_sync_periodic: false,
            sync_state_path: std::path::PathBuf::from("./target/test/sync"),
            request_timeout_seconds: 10,
            max_retry_attempts: 3,
        };
        
        // Create a sync manager
        let sync_manager = SyncManager::new(identity, store, Some(config));
        
        // Get network status - should be offline since no federation URLs
        let status = sync_manager.get_network_status().await.unwrap();
        assert!(!status.is_connected, "Network should be offline");
        
        // Test that sync operations respect the offline status
        
        // 1. Test sync_trust_bundles
        let result = sync_manager.sync_trust_bundles("http://localhost:8080").await;
        assert!(matches!(result, Err(SyncError::Offline(_))), "Should return Offline error");
        
        // 2. Test fetch_trust_bundle_by_epoch
        let result = sync_manager.fetch_trust_bundle_by_epoch("http://localhost:8080", 1).await;
        assert!(matches!(result, Err(SyncError::Offline(_))), "Should return Offline error");
        
        // 3. Test submit_dag_node
        let node = create_test_dag_node();
        let result = sync_manager.submit_dag_node(&node).await;
        assert!(result.is_ok(), "Should not fail with error");
        let response = result.unwrap();
        assert!(!response.success, "Submission should not be successful");
        assert!(response.error.is_some(), "Should have an error message");
        assert!(response.error.unwrap().contains("offline"), "Error should mention offline");
        
        // 4. Test fetch_dag_thread
        let result = sync_manager.fetch_dag_thread("thread:123").await;
        assert!(matches!(result, Err(SyncError::Offline(_))), "Should return Offline error");
        
        // 5. Test submit_dag_nodes_batch
        let result = sync_manager.submit_dag_nodes_batch(&[node]).await;
        assert!(result.is_ok(), "Should not fail with error");
        let responses = result.unwrap();
        assert_eq!(responses.len(), 1, "Should have one response");
        assert!(!responses[0].success, "Submission should not be successful");
        assert!(responses[0].error.is_some(), "Should have an error message");
        assert!(responses[0].error.as_ref().unwrap().contains("offline"), "Error should mention offline");
    }
    
    #[tokio::test]
    async fn test_custom_network_status() {
        // Create a mock store
        let temp_dir = tempdir().unwrap();
        let store = create_mock_store(temp_dir.path().to_str().unwrap());
        
        // Create a test identity
        let identity = create_test_identity();
        
        // Create a configuration with federation URLs
        let config = SyncManagerConfig {
            federation_urls: vec!["http://localhost:8080".to_string()],
            sync_interval_seconds: 60,
            auto_sync_on_startup: false,
            auto_sync_periodic: false,
            sync_state_path: std::path::PathBuf::from("./target/test/sync"),
            request_timeout_seconds: 10,
            max_retry_attempts: 3,
        };
        
        // Create a sync manager with the mock
        let sync_manager = SyncManager::new(identity, store, Some(config));
        
        // Assuming there's a way to mock the network status
        // For example, by injecting a custom network status provider
        // or by using a test-only method in SyncManager
        
        // In a real implementation, you would have a way to set a network status for testing
        // Here we're assuming that the SyncManager::set_network_status_for_testing method exists
        
        // Test with a mock offline status
        let offline_status = NetworkStatus {
            is_connected: false,
            primary_node_latency: None,
            last_successful_sync: None,
            pending_submissions: 0,
            active_federation_url: "".to_string(),
            successful_operations: 0,
            failed_operations: 0,
        };
        
        // Simulate network going offline
        // In a real test, you would use a method like this to set the status:
        // sync_manager.set_network_status_for_testing(offline_status).await;
        
        // Then you would test network operations with the offline status
        // This part is left as a placeholder for when the actual implementation
        // has a way to inject mock network status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};
    use wallet_core::store::file::FileStore;
    use wallet_core::identity::IdentityWallet;
    use wallet_types::TrustBundle;
    use wallet_core::dag::DagNode;
    use std::collections::HashMap;
    use mockito;
    use serde_json::json;

    fn setup_test_identity() -> IdentityWallet {
        IdentityWallet::new("test", Some("Test Identity")).unwrap()
    }

    fn setup_test_store() -> FileStore {
        let temp_dir = tempfile::tempdir().unwrap();
        FileStore::new(temp_dir.path())
    }

    fn setup_sync_manager(store: FileStore, identity: IdentityWallet) -> SyncManager<FileStore> {
        // Setup config with mock server URL
        let mut config = SyncManagerConfig::default();
        config.federation_urls = vec![mockito::server_url()];
        
        SyncManager::new(identity, store, Some(config))
    }

    fn create_valid_trust_bundle() -> TrustBundle {
        TrustBundle {
            id: "test-bundle-1".to_string(),
            epoch: 1,
            threshold: 2,
            guardians: vec![
                "did:icn:guardian1".to_string(),
                "did:icn:guardian2".to_string(),
                "did:icn:guardian3".to_string(),
            ],
            active: true,
            created_at: SystemTime::now(),
            expires_at: None,
            links: HashMap::new(),
            signatures: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    fn create_valid_dag_node() -> DagNode {
        DagNode {
            cid: "bafyreihpcgxa6wjz2cl3mfpxssjcm54chzoj66xtnxekyxuio5h5tsuxsy".to_string(),
            parents: vec!["bafyreia4k7k7qpx52pe6je6zymkmufetmdllycnvhg2bopjadhdvw2a3m4".to_string()],
            epoch: 1,
            creator: "did:icn:creator1".to_string(),
            timestamp: SystemTime::now(),
            content_type: "test".to_string(),
            content: serde_json::json!({"test": "data"}),
            signatures: vec!["signature1".to_string()],
        }
    }

    #[tokio::test]
    async fn test_trust_bundle_validation() {
        let identity = setup_test_identity();
        let store = setup_test_store();
        let sync_manager = setup_sync_manager(store, identity);
        
        // Test valid bundle
        let valid_bundle = create_valid_trust_bundle();
        let result = sync_manager.validate_trust_bundle(&valid_bundle);
        assert!(result.is_ok());
        
        // Test invalid bundle - future timestamp
        let mut future_bundle = valid_bundle.clone();
        future_bundle.created_at = SystemTime::now() + Duration::from_secs(3600); // 1 hour in future
        let result = sync_manager.validate_trust_bundle(&future_bundle);
        assert!(result.is_err());
        
        // Test invalid bundle - bad threshold
        let mut bad_threshold_bundle = valid_bundle.clone();
        bad_threshold_bundle.threshold = 5; // More than # of guardians
        let result = sync_manager.validate_trust_bundle(&bad_threshold_bundle);
        assert!(result.is_err());
        
        // Test invalid bundle - expired
        let mut expired_bundle = valid_bundle.clone();
        expired_bundle.expires_at = Some(SystemTime::now() - Duration::from_secs(3600)); // 1 hour ago
        let result = sync_manager.validate_trust_bundle(&expired_bundle);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dag_node_validation() {
        let identity = setup_test_identity();
        let store = setup_test_store();
        let sync_manager = setup_sync_manager(store, identity);
        
        // Test valid node
        let valid_node = create_valid_dag_node();
        let result = sync_manager.validate_dag_node(&valid_node, None);
        assert!(result.is_ok());
        
        // Test valid node with expected CID
        let result = sync_manager.validate_dag_node(&valid_node, 
            Some("bafyreihpcgxa6wjz2cl3mfpxssjcm54chzoj66xtnxekyxuio5h5tsuxsy"));
        assert!(result.is_ok());
        
        // Test CID mismatch
        let result = sync_manager.validate_dag_node(&valid_node, Some("wrong-cid"));
        assert!(result.is_err());
        
        // Test invalid node - missing CID
        let mut no_cid_node = valid_node.clone();
        no_cid_node.cid = "".to_string();
        let result = sync_manager.validate_dag_node(&no_cid_node, None);
        assert!(result.is_err());
        
        // Test invalid node - future timestamp
        let mut future_node = valid_node.clone();
        future_node.timestamp = SystemTime::now() + Duration::from_secs(3600); // 1 hour in future
        let result = sync_manager.validate_dag_node(&future_node, None);
        assert!(result.is_err());
        
        // Test invalid node - empty parent CID
        let mut bad_parent_node = valid_node.clone();
        bad_parent_node.parents = vec!["".to_string()];
        let result = sync_manager.validate_dag_node(&bad_parent_node, None);
        assert!(result.is_err());
        
        // Test invalid node - no signatures
        let mut no_sig_node = valid_node.clone();
        no_sig_node.signatures = vec![];
        let result = sync_manager.validate_dag_node(&no_sig_node, None);
        assert!(result.is_err());
        
        // Test invalid node - no creator
        let mut no_creator_node = valid_node.clone();
        no_creator_node.creator = "".to_string();
        let result = sync_manager.validate_dag_node(&no_creator_node, None);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sync_with_mock_server() {
        let identity = setup_test_identity();
        let store = setup_test_store();
        let sync_manager = setup_sync_manager(store, identity);
        
        // Setup a mock response for the latest bundle
        let valid_bundle = create_valid_trust_bundle();
        let bundle_json = serde_json::to_string(&valid_bundle).unwrap();
        
        let mock = mockito::mock("GET", "/bundles/latest")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(&bundle_json)
            .create();
        
        // Test syncing from the mock server
        let result = sync_manager.fetch_latest_trust_bundle(&mockito::server_url()).await;
        mock.assert();
        
        assert!(result.is_ok());
        let fetched_bundle = result.unwrap();
        assert_eq!(fetched_bundle.id, valid_bundle.id);
        
        // Test with invalid data
        let mut future_bundle = valid_bundle.clone();
        future_bundle.created_at = SystemTime::now() + Duration::from_secs(3600); // 1 hour in future
        let invalid_bundle_json = serde_json::to_string(&future_bundle).unwrap();
        
        let mock = mockito::mock("GET", "/bundles/latest")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(&invalid_bundle_json)
            .create();
        
        // This should return an error due to validation failure
        let result = sync_manager.sync_trust_bundles(&mockito::server_url()).await;
        mock.assert();
        
        assert!(result.is_err());
        if let Err(SyncError::ValidationError(_)) = result {
            // Expected validation error
        } else {
            panic!("Expected ValidationError, got: {:?}", result);
        }
    }
} 