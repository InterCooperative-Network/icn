use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use std::path::PathBuf;
use std::sync::Arc;
use reqwest::Client;
use serde_json::{json, Value};
use wallet_core::store::{SecurePlatform, create_mock_secure_store};
use wallet_ui_api::state::{AppState, AppConfig};
use wallet_ui_api::api::create_api_router;

// Helper function to find an available local port
async fn find_available_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

// Start a test server with the secure store
async fn start_test_server() -> (SocketAddr, oneshot::Sender<()>) {
    let addr = find_available_port().await;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    
    // Use a temporary directory for test data
    let temp_dir = std::env::temp_dir().join("icn-wallet-test");
    std::fs::create_dir_all(&temp_dir).unwrap();
    
    // Create a secure store for testing
    let store = create_mock_secure_store(SecurePlatform::Generic, temp_dir.to_str().unwrap());
    store.init().await.unwrap();
    
    // Create the application state
    let config = AppConfig {
        federation_url: "https://test-federation.example.com/api".to_string(),
        data_dir: temp_dir.to_str().unwrap().to_string(),
        auto_sync: false, // Disable auto-sync for tests
        sync_interval: 60,
    };
    let state = Arc::new(AppState::new(store, config));
    
    // Create the API router
    let api_router = create_api_router(state);
    
    // Build the application
    let app = axum::Router::new().nest("/api", api_router);
    
    // Start the server
    let server = axum::serve(
        TcpListener::bind(addr).await.unwrap(),
        app
    );
    
    tokio::spawn(async move {
        server.with_graceful_shutdown(async {
            shutdown_rx.await.ok();
        }).await.unwrap();
    });
    
    (addr, shutdown_tx)
}

// Test helper for HTTP client
struct TestClient {
    client: Client,
    base_url: String,
}

impl TestClient {
    fn new(addr: SocketAddr) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://{}/api", addr),
        }
    }
    
    async fn get(&self, path: &str) -> Value {
        let url = format!("{}{}", self.base_url, path);
        let response = self.client.get(&url).send().await.unwrap();
        response.json().await.unwrap()
    }
    
    async fn post(&self, path: &str, body: Value) -> Value {
        let url = format!("{}{}", self.base_url, path);
        let response = self.client.post(&url).json(&body).send().await.unwrap();
        response.json().await.unwrap()
    }
}

#[tokio::test]
async fn test_identity_creation_and_credential_issuance() {
    // Start the test server
    let (addr, shutdown_tx) = start_test_server().await;
    let client = TestClient::new(addr);
    
    // Test 1: Create an identity
    let create_identity_response = client.post(
        "/did/create", 
        json!({
            "scope": "personal",
            "metadata": {
                "name": "Test User",
                "email": "test@example.com"
            }
        })
    ).await;
    
    assert!(create_identity_response["did"].as_str().unwrap().starts_with("did:icn:"));
    let did = create_identity_response["did"].as_str().unwrap().to_string();
    let id = create_identity_response["id"].as_str().unwrap().to_string();
    
    // Test 2: List identities
    let list_identities_response = client.get("/did/list").await;
    assert!(list_identities_response.as_array().unwrap().len() > 0);
    
    // Test 3: Get identity
    let get_identity_response = client.get(&format!("/did/{}", id)).await;
    assert_eq!(get_identity_response["did"].as_str().unwrap(), did);
    
    // Test 4: Issue a credential
    let create_credential_response = client.post(
        &format!("/vc/issue/{}", id),
        json!({
            "subject_data": {
                "id": "test-subject",
                "name": "Test Subject",
                "attributes": {
                    "role": "member",
                    "level": 1
                }
            },
            "credential_types": ["MembershipCredential"]
        })
    ).await;
    
    let credential = create_credential_response["credential"].clone();
    assert!(credential["issuer"].as_str().unwrap() == did);
    
    // Test 5: Verify a credential
    let verify_response = client.post(
        "/vc/verify",
        credential
    ).await;
    
    assert!(verify_response["valid"].as_bool().unwrap());
    assert_eq!(verify_response["issuer"].as_str().unwrap(), did);
    
    // Shutdown the server
    shutdown_tx.send(()).unwrap();
}

#[tokio::test]
async fn test_action_queue_and_sync() {
    // Start the test server
    let (addr, shutdown_tx) = start_test_server().await;
    let client = TestClient::new(addr);
    
    // Create an identity first
    let create_identity_response = client.post(
        "/did/create", 
        json!({
            "scope": "personal",
            "metadata": { "name": "Test User" }
        })
    ).await;
    
    let did = create_identity_response["did"].as_str().unwrap().to_string();
    
    // Test 1: Queue an action
    let queue_action_response = client.post(
        "/actions/queue",
        json!({
            "action_type": "proposal",
            "creator_did": did,
            "content": {
                "type": "governance",
                "title": "Test Proposal",
                "description": "This is a test proposal"
            }
        })
    ).await;
    
    assert!(queue_action_response["action_id"].as_str().unwrap().len() > 0);
    assert_eq!(queue_action_response["status"].as_str().unwrap(), "queued");
    
    let action_id = queue_action_response["action_id"].as_str().unwrap().to_string();
    
    // Test 2: Get action status
    let action_status_response = client.get(
        &format!("/actions/{}", action_id)
    ).await;
    
    assert_eq!(action_status_response["id"].as_str().unwrap(), action_id);
    assert_eq!(action_status_response["status"].as_str().unwrap(), "Pending");
    
    // Test 3: Process the action
    let process_response = client.post(
        &format!("/actions/{}/process", action_id),
        json!({})
    ).await;
    
    assert!(process_response["cid"].as_str().unwrap().len() > 0);
    assert!(process_response["content_type"].as_str().unwrap().contains("proposal"));
    
    // Test 4: Verify the action was processed
    let updated_status_response = client.get(
        &format!("/actions/{}", action_id)
    ).await;
    
    assert_eq!(updated_status_response["status"].as_str().unwrap(), "Completed");
    
    // Test 5: Sync trust bundles
    let sync_response = client.post(
        "/sync/trust-bundles",
        json!({})
    ).await;
    
    assert_eq!(sync_response["status"].as_str().unwrap(), "success");
    assert!(sync_response["bundles_synced"].as_u64().unwrap() > 0);
    
    // Test 6: List trust bundles
    let bundles_response = client.get("/bundles").await;
    
    assert!(bundles_response.as_array().unwrap().len() > 0);
    let bundle = &bundles_response.as_array().unwrap()[0];
    assert!(bundle["id"].as_str().unwrap().len() > 0);
    
    // Shutdown the server
    shutdown_tx.send(()).unwrap();
}

#[tokio::test]
async fn test_sync_dag() {
    // Start the test server
    let (addr, shutdown_tx) = start_test_server().await;
    let client = TestClient::new(addr);
    
    // Create an identity first
    let create_identity_response = client.post(
        "/did/create", 
        json!({
            "scope": "personal",
            "metadata": { "name": "Test User" }
        })
    ).await;
    
    // Test: Sync DAG
    let sync_response = client.post("/sync/dag", json!({})).await;
    assert_eq!(sync_response["status"].as_str().unwrap(), "success");
    
    // Shutdown the server
    shutdown_tx.send(()).unwrap();
} 