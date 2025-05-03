use mockito::{mock, server_url};
use wallet_core::identity::IdentityWallet;
use wallet_core::store::file::FileStore;
use wallet_core::dag::{DagNode, DagThread, ThreadType};
use wallet_sync::sync_manager::{SyncManager, SyncManagerConfig, NodeSubmissionResponse};
use wallet_sync::error::SyncResult;
use wallet_agent::governance::TrustBundle;
use std::collections::HashMap;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde_json::json;
use tokio::fs;
use uuid::Uuid;

async fn setup_test_env() -> (IdentityWallet, FileStore, String) {
    // Create a unique test directory
    let test_id = Uuid::new_v4().to_string();
    let test_dir = PathBuf::from(format!("./target/test_storage/{}", test_id));
    
    // Clean up any existing directory
    if test_dir.exists() {
        fs::remove_dir_all(&test_dir).await.unwrap();
    }
    fs::create_dir_all(&test_dir).await.unwrap();
    
    // Create a test identity
    let identity = IdentityWallet {
        did: format!("did:icn:test:{}", test_id),
        controller: "test-controller".to_string(),
        verification_method: HashMap::new(),
        authentication: vec![],
        assertion_method: vec![],
        capability_invocation: vec![],
        capability_delegation: vec![],
        key_agreement: vec![],
        service: vec![],
        created: Utc::now(),
        updated: Utc::now(),
    };
    
    // Create a test store
    let store = FileStore::new(&test_dir);
    store.init().await.unwrap();
    
    // Get mockito server URL
    let server_url = server_url();
    
    (identity, store, server_url)
}

async fn cleanup_test_env(test_dir: &PathBuf) {
    if test_dir.exists() {
        fs::remove_dir_all(test_dir).await.unwrap();
    }
}

#[tokio::test]
async fn test_fetch_trust_bundle() -> SyncResult<()> {
    // Setup test environment
    let (identity, store, server_url) = setup_test_env().await;
    let test_dir = store.base_path().clone();
    
    // Create mock bundle response
    let mock_bundle = TrustBundle {
        id: "test-federation".to_string(),
        version: 1,
        epoch: 42,
        guardians: vec!["did:icn:guardian1".to_string(), "did:icn:guardian2".to_string()],
        signatures: vec![],
        created_at: Utc::now(),
        valid_until: Utc::now() + chrono::Duration::days(30),
        parameters: HashMap::new(),
    };
    
    // Setup mockito to return our bundle
    let bundle_mock = mock("GET", "/bundles/latest")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_bundle).unwrap())
        .create();
    
    // Create SyncManager with mockito URL
    let config = SyncManagerConfig {
        federation_urls: vec![server_url],
        sync_state_path: test_dir.join("sync"),
        sync_interval_seconds: 3600,
        auto_sync_on_startup: false,
        auto_sync_periodic: false,
        request_timeout_seconds: 30,
        max_retry_attempts: 3,
    };
    
    let sync_manager = SyncManager::new(identity, store, Some(config));
    
    // Test fetching the bundle
    let result = sync_manager.fetch_latest_trust_bundle(&server_url).await?;
    
    // Verify bundle was retrieved
    assert_eq!(result.id, mock_bundle.id);
    assert_eq!(result.epoch, mock_bundle.epoch);
    assert_eq!(result.guardians.len(), mock_bundle.guardians.len());
    
    // Verify mock was called
    bundle_mock.assert();
    
    // Cleanup
    cleanup_test_env(&test_dir).await;
    
    Ok(())
}

#[tokio::test]
async fn test_fetch_dag_node() -> SyncResult<()> {
    // Setup test environment
    let (identity, store, server_url) = setup_test_env().await;
    let test_dir = store.base_path().clone();
    
    // Create mock DAG node
    let mock_node = DagNode {
        cid: "bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy".to_string(),
        parents: vec!["bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string()],
        epoch: 42,
        creator: "did:icn:guardian1".to_string(),
        timestamp: Utc::now(),
        content_type: "proposal".to_string(),
        content: json!({"title": "Test Proposal", "description": "This is a test proposal"}),
        signatures: HashMap::new(),
        links: HashMap::new(),
    };
    
    // Setup mockito to return our node
    let node_mock = mock("GET", "/nodes/bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_node).unwrap())
        .create();
    
    // Create SyncManager with mockito URL
    let config = SyncManagerConfig {
        federation_urls: vec![server_url],
        sync_state_path: test_dir.join("sync"),
        sync_interval_seconds: 3600,
        auto_sync_on_startup: false,
        auto_sync_periodic: false,
        request_timeout_seconds: 30,
        max_retry_attempts: 3,
    };
    
    let sync_manager = SyncManager::new(identity, store.clone(), Some(config));
    
    // Test fetching the node
    let result = sync_manager.fetch_dag_node("bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy").await?;
    
    // Verify node was retrieved
    assert_eq!(result.cid, mock_node.cid);
    assert_eq!(result.content_type, mock_node.content_type);
    
    // Verify node was saved to store
    let stored_node = store.load_dag_node("bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy").await.unwrap();
    assert_eq!(stored_node.cid, mock_node.cid);
    
    // Verify mock was called
    node_mock.assert();
    
    // Cleanup
    cleanup_test_env(&test_dir).await;
    
    Ok(())
}

#[tokio::test]
async fn test_submit_dag_node() -> SyncResult<()> {
    // Setup test environment
    let (identity, store, server_url) = setup_test_env().await;
    let test_dir = store.base_path().clone();
    
    // Create node to submit
    let node = DagNode {
        cid: "".to_string(), // Empty CID, will be assigned by the server
        parents: vec!["bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string()],
        epoch: 42,
        creator: "did:icn:guardian1".to_string(),
        timestamp: Utc::now(),
        content_type: "proposal".to_string(),
        content: json!({"title": "Test Proposal", "description": "This is a test proposal"}),
        signatures: HashMap::new(),
        links: HashMap::new(),
    };
    
    // Create mock submission response
    let mock_response = NodeSubmissionResponse {
        success: true,
        cid: Some("bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy".to_string()),
        error: None,
    };
    
    // Setup mockito for submission
    let submit_mock = mock("POST", "/nodes")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_response).unwrap())
        .create();
    
    // Create SyncManager with mockito URL
    let config = SyncManagerConfig {
        federation_urls: vec![server_url],
        sync_state_path: test_dir.join("sync"),
        sync_interval_seconds: 3600,
        auto_sync_on_startup: false,
        auto_sync_periodic: false,
        request_timeout_seconds: 30,
        max_retry_attempts: 3,
    };
    
    let sync_manager = SyncManager::new(identity, store.clone(), Some(config));
    
    // Test submitting the node
    let result = sync_manager.submit_dag_node(&node).await?;
    
    // Verify response
    assert!(result.success);
    assert_eq!(result.cid, mock_response.cid);
    
    // Verify node was saved with the assigned CID
    let stored_node = store.load_dag_node("bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy").await.unwrap();
    assert_eq!(stored_node.cid, mock_response.cid.unwrap());
    
    // Verify mock was called
    submit_mock.assert();
    
    // Cleanup
    cleanup_test_env(&test_dir).await;
    
    Ok(())
}

#[tokio::test]
async fn test_fetch_thread_info() -> SyncResult<()> {
    // Setup test environment
    let (identity, store, server_url) = setup_test_env().await;
    let test_dir = store.base_path().clone();
    
    // Create mock thread
    let thread_id = "test-thread-1";
    let mock_thread = DagThread {
        thread_type: ThreadType::Proposal,
        creator: "did:icn:guardian1".to_string(),
        root_cid: "bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string(),
        latest_cid: "bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy".to_string(),
        title: Some("Test Proposal".to_string()),
        description: Some("This is a test proposal".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    // Setup mockito to return our thread
    let thread_mock = mock("GET", format!("/threads/{}", thread_id).as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_thread).unwrap())
        .create();
    
    // Create SyncManager with mockito URL
    let config = SyncManagerConfig {
        federation_urls: vec![server_url],
        sync_state_path: test_dir.join("sync"),
        sync_interval_seconds: 3600,
        auto_sync_on_startup: false,
        auto_sync_periodic: false,
        request_timeout_seconds: 30,
        max_retry_attempts: 3,
    };
    
    let sync_manager = SyncManager::new(identity, store.clone(), Some(config));
    
    // Test fetching the thread
    let result = sync_manager.fetch_dag_thread_info(thread_id).await?;
    
    // Verify thread was retrieved
    assert_eq!(result.root_cid, mock_thread.root_cid);
    assert_eq!(result.latest_cid, mock_thread.latest_cid);
    
    // Verify thread was saved to store
    let stored_thread = store.load_dag_thread(thread_id).await.unwrap();
    assert_eq!(stored_thread.root_cid, mock_thread.root_cid);
    
    // Verify thread cache was created
    let thread_cache = store.load_dag_thread_cache(thread_id).await.unwrap();
    assert_eq!(thread_cache.head_cid, mock_thread.root_cid);
    assert_eq!(thread_cache.tail_cid, mock_thread.latest_cid);
    assert!(thread_cache.node_cids.contains(&mock_thread.root_cid));
    assert!(thread_cache.node_cids.contains(&mock_thread.latest_cid));
    
    // Verify mock was called
    thread_mock.assert();
    
    // Cleanup
    cleanup_test_env(&test_dir).await;
    
    Ok(())
}

#[tokio::test]
async fn test_fetch_complete_dag_thread() -> SyncResult<()> {
    // Setup test environment
    let (identity, store, server_url) = setup_test_env().await;
    let test_dir = store.base_path().clone();
    
    // Create mock thread
    let thread_id = "test-thread-2";
    let mock_thread = DagThread {
        thread_type: ThreadType::Proposal,
        creator: "did:icn:guardian1".to_string(),
        root_cid: "bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string(),
        latest_cid: "bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy".to_string(),
        title: Some("Test Proposal".to_string()),
        description: Some("This is a test proposal".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    // Create 3 mock nodes forming a chain
    let root_node = DagNode {
        cid: "bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string(),
        parents: vec![],
        epoch: 42,
        creator: "did:icn:guardian1".to_string(),
        timestamp: Utc::now(),
        content_type: "proposal".to_string(),
        content: json!({"title": "Test Proposal", "description": "This is a test proposal"}),
        signatures: HashMap::new(),
        links: {
            let mut links = HashMap::new();
            links.insert("next".to_string(), "bafybeihgfrwmmcvlfkefqwzhelafym51r4zcyvxtpadu5phgjqrwvolmjq".to_string());
            links
        },
    };
    
    let middle_node = DagNode {
        cid: "bafybeihgfrwmmcvlfkefqwzhelafym51r4zcyvxtpadu5phgjqrwvolmjq".to_string(),
        parents: vec!["bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string()],
        epoch: 42,
        creator: "did:icn:guardian1".to_string(),
        timestamp: Utc::now(),
        content_type: "proposal".to_string(),
        content: json!({"vote": "approve", "reason": "Good proposal"}),
        signatures: HashMap::new(),
        links: {
            let mut links = HashMap::new();
            links.insert("parent".to_string(), "bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string());
            links.insert("next".to_string(), "bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy".to_string());
            links
        },
    };
    
    let latest_node = DagNode {
        cid: "bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy".to_string(),
        parents: vec!["bafybeihgfrwmmcvlfkefqwzhelafym51r4zcyvxtpadu5phgjqrwvolmjq".to_string()],
        epoch: 42,
        creator: "did:icn:guardian2".to_string(),
        timestamp: Utc::now(),
        content_type: "proposal".to_string(),
        content: json!({"vote": "approve", "reason": "I agree"}),
        signatures: HashMap::new(),
        links: {
            let mut links = HashMap::new();
            links.insert("parent".to_string(), "bafybeihgfrwmmcvlfkefqwzhelafym51r4zcyvxtpadu5phgjqrwvolmjq".to_string());
            links
        },
    };
    
    // Setup mockito to return our thread and nodes
    let thread_mock = mock("GET", format!("/threads/{}", thread_id).as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_thread).unwrap())
        .create();
        
    let root_mock = mock("GET", "/nodes/bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&root_node).unwrap())
        .create();
        
    let middle_mock = mock("GET", "/nodes/bafybeihgfrwmmcvlfkefqwzhelafym51r4zcyvxtpadu5phgjqrwvolmjq")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&middle_node).unwrap())
        .create();
        
    let latest_mock = mock("GET", "/nodes/bafybeibvh6hmpjxlzjdvdz6ij32ctbpdfhwp5o6x7x7nschbtpul5jqphy")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&latest_node).unwrap())
        .create();
    
    // Create SyncManager with mockito URL
    let config = SyncManagerConfig {
        federation_urls: vec![server_url],
        sync_state_path: test_dir.join("sync"),
        sync_interval_seconds: 3600,
        auto_sync_on_startup: false,
        auto_sync_periodic: false,
        request_timeout_seconds: 30,
        max_retry_attempts: 3,
    };
    
    let sync_manager = SyncManager::new(identity, store.clone(), Some(config));
    
    // Test fetching the complete thread
    sync_manager.fetch_complete_dag_thread(thread_id).await?;
    
    // Verify all nodes were saved
    let stored_root = store.load_dag_node(&root_node.cid).await.unwrap();
    let stored_middle = store.load_dag_node(&middle_node.cid).await.unwrap();
    let stored_latest = store.load_dag_node(&latest_node.cid).await.unwrap();
    
    assert_eq!(stored_root.cid, root_node.cid);
    assert_eq!(stored_middle.cid, middle_node.cid);
    assert_eq!(stored_latest.cid, latest_node.cid);
    
    // Verify thread cache contains all nodes
    let thread_cache = store.load_dag_thread_cache(thread_id).await.unwrap();
    assert_eq!(thread_cache.head_cid, root_node.cid);
    assert_eq!(thread_cache.tail_cid, latest_node.cid);
    assert!(thread_cache.node_cids.contains(&root_node.cid));
    assert!(thread_cache.node_cids.contains(&middle_node.cid));
    assert!(thread_cache.node_cids.contains(&latest_node.cid));
    
    // Verify mocks were called
    thread_mock.assert();
    root_mock.assert();
    middle_mock.assert();
    latest_mock.assert();
    
    // Cleanup
    cleanup_test_env(&test_dir).await;
    
    Ok(())
} 