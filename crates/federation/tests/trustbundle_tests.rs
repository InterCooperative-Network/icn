use icn_federation::{
    FederationManager, FederationManagerConfig, TrustBundle,
    roles::NodeRole, network::FederationRequest
};
use icn_storage::AsyncInMemoryStorage;
use icn_identity::{KeyPair, IdentityId};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use futures::StreamExt;
use libp2p::PeerId;

/// Create a test federation manager with in-memory storage
async fn create_test_federation_manager() -> FederationManager {
    // Create in-memory storage
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // Create a basic config
    let config = FederationManagerConfig {
        bootstrap_period: Duration::from_secs(1),
        peer_sync_interval: Duration::from_secs(2),
        trust_bundle_sync_interval: Duration::from_secs(3),
        max_peers: 10,
        ..Default::default()
    };
    
    // Generate test keypair
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    // Create and initialize federation manager
    let manager = FederationManager::new(config, storage, keypair).await.unwrap();
    
    manager
}

/// Create a basic trust bundle for testing
fn create_test_trust_bundle(epoch_id: u64) -> TrustBundle {
    let mut bundle = TrustBundle::new(epoch_id);
    
    // Add some test nodes to the bundle
    let node1_id = IdentityId::new("did:icn:node1");
    let node2_id = IdentityId::new("did:icn:node2");
    
    bundle.add_node(node1_id.clone(), NodeRole::Validator);
    bundle.add_node(node2_id.clone(), NodeRole::Observer);
    
    // Create a simple proof (in a real system this would be cryptographically sound)
    let dummy_proof = vec![9, 10, 11, 12];
    bundle.set_proof(dummy_proof);
    
    bundle
}

#[tokio::test]
async fn test_trust_bundle_storage_and_retrieval() {
    // Create a federation manager
    let manager = create_test_federation_manager().await;
    
    // Create a test trust bundle
    let bundle = create_test_trust_bundle(1);
    
    // Store the bundle
    manager.store_trust_bundle(&bundle).await.unwrap();
    
    // Retrieve the bundle
    let retrieved = manager.get_trust_bundle(1).await.unwrap();
    
    // Verify it's the same bundle
    assert_eq!(retrieved.epoch_id, bundle.epoch_id);
    assert_eq!(retrieved.nodes.len(), bundle.nodes.len());
    
    // Try to get a non-existent bundle
    let result = manager.get_trust_bundle(999).await;
    assert!(result.is_err(), "Getting non-existent bundle should fail");
}

#[tokio::test]
async fn test_trust_bundle_epoch_tracking() {
    // Create a federation manager
    let manager = create_test_federation_manager().await;
    
    // Verify initial epoch is 0
    let initial_epoch = manager.get_latest_known_epoch().await.unwrap();
    assert_eq!(initial_epoch, 0, "Initial epoch should be 0");
    
    // Store bundles with increasing epochs
    for i in 1..=5 {
        let bundle = create_test_trust_bundle(i);
        manager.store_trust_bundle(&bundle).await.unwrap();
        
        // Verify the latest epoch is updated
        let latest = manager.get_latest_known_epoch().await.unwrap();
        assert_eq!(latest, i, "Latest epoch should be updated to {}", i);
    }
    
    // Add a bundle with an older epoch - should not update latest
    let old_bundle = create_test_trust_bundle(3);
    manager.store_trust_bundle(&old_bundle).await.unwrap();
    
    // Verify latest epoch is still 5
    let latest = manager.get_latest_known_epoch().await.unwrap();
    assert_eq!(latest, 5, "Latest epoch should remain at 5");
}

#[tokio::test]
async fn test_trust_bundle_request_handling() {
    // Create two federation managers (simulating different nodes)
    let manager1 = create_test_federation_manager().await;
    let manager2 = create_test_federation_manager().await;
    
    // Get the peer ID of manager2 for requests
    let peer_id2 = manager2.local_peer_id().clone();
    
    // Add some bundles to manager1
    for i in 1..=3 {
        let bundle = create_test_trust_bundle(i);
        manager1.store_trust_bundle(&bundle).await.unwrap();
    }
    
    // Simulate manager2 requesting the latest bundle from manager1
    let request = FederationRequest::TrustBundleRequest { 
        epoch_id: None // Request latest
    };
    
    // Process the request in manager1 (in a real system this would be via the network)
    let response = manager1.handle_trust_bundle_request(peer_id2, request).await.unwrap();
    
    // Verify we got the expected bundle
    match response {
        FederationRequest::TrustBundleResponse { bundle } => {
            assert!(bundle.is_some(), "Bundle should be present");
            let received_bundle = bundle.unwrap();
            assert_eq!(received_bundle.epoch_id, 3, "Should receive latest bundle (epoch 3)");
        },
        _ => panic!("Expected TrustBundleResponse"),
    }
    
    // Now request a specific epoch
    let request = FederationRequest::TrustBundleRequest { 
        epoch_id: Some(2) // Request epoch 2
    };
    
    let response = manager1.handle_trust_bundle_request(peer_id2, request).await.unwrap();
    
    // Verify we got the requested bundle
    match response {
        FederationRequest::TrustBundleResponse { bundle } => {
            assert!(bundle.is_some(), "Bundle should be present");
            let received_bundle = bundle.unwrap();
            assert_eq!(received_bundle.epoch_id, 2, "Should receive requested bundle (epoch 2)");
        },
        _ => panic!("Expected TrustBundleResponse"),
    }
    
    // Request a non-existent epoch
    let request = FederationRequest::TrustBundleRequest { 
        epoch_id: Some(999) // Request non-existent epoch
    };
    
    let response = manager1.handle_trust_bundle_request(peer_id2, request).await.unwrap();
    
    // Verify we got a response with no bundle
    match response {
        FederationRequest::TrustBundleResponse { bundle } => {
            assert!(bundle.is_none(), "Bundle should be None for non-existent epoch");
        },
        _ => panic!("Expected TrustBundleResponse"),
    }
}

#[tokio::test]
async fn test_trust_bundle_sync_client() {
    // This test simulates what the Wallet's SyncClient would do
    
    // Create two federation managers
    let manager_server = create_test_federation_manager().await;
    let manager_client = create_test_federation_manager().await;
    
    // Get the peer IDs
    let server_peer_id = manager_server.local_peer_id().clone();
    
    // Add some bundles to the server
    for i in 1..=5 {
        let bundle = create_test_trust_bundle(i);
        manager_server.store_trust_bundle(&bundle).await.unwrap();
    }
    
    // Add the server as a known peer to the client
    manager_client.add_peer(server_peer_id.clone()).await;
    
    // Simulate what a wallet would do:
    // 1. Check local latest epoch
    let client_latest = manager_client.get_latest_known_epoch().await.unwrap();
    assert_eq!(client_latest, 0, "Client should start with epoch 0");
    
    // 2. Request sync from the latest epoch it knows
    let sync_request = manager_client.create_trust_bundle_sync_request(client_latest).await;
    
    // 3. Handle the request on the server side
    let response = manager_server.handle_trust_bundle_request(
        manager_client.local_peer_id().clone(),
        sync_request
    ).await.unwrap();
    
    // 4. Process the response on the client side
    match response {
        FederationRequest::TrustBundleResponse { bundle: Some(bundle) } => {
            // Store the received bundle
            manager_client.store_trust_bundle(&bundle).await.unwrap();
            
            // Verify the client's latest epoch is updated
            let new_latest = manager_client.get_latest_known_epoch().await.unwrap();
            assert_eq!(new_latest, 5, "Client's latest epoch should be updated to 5");
            
            // Verify the bundle content
            let retrieved = manager_client.get_trust_bundle(5).await.unwrap();
            assert_eq!(retrieved.nodes.len(), 2, "Bundle should contain 2 nodes");
        },
        _ => panic!("Expected TrustBundleResponse with a bundle"),
    }
} 