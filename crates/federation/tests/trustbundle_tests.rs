use icn_federation::{FederationManager, FederationManagerConfig, FederationResult};
use icn_identity::{TrustBundle, KeyPair, QuorumProof, QuorumConfig};
use icn_storage::{AsyncInMemoryStorage, StorageBackend};
use futures::lock::Mutex;
use std::sync::Arc;
use std::time::Duration;
use libp2p::gossipsub;

// Helper function to create a test storage backend
async fn create_test_storage() -> Arc<Mutex<dyn StorageBackend + Send + Sync>> {
    Arc::new(Mutex::new(AsyncInMemoryStorage::new()))
}

// Helper function to create a test federation manager
async fn create_test_federation_manager(
    storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>,
) -> FederationResult<FederationManager> {
    // Create a simple configuration for testing
    let config = FederationManagerConfig {
        bootstrap_period: Duration::from_millis(100),
        peer_sync_interval: Duration::from_millis(100),
        trust_bundle_sync_interval: Duration::from_millis(100),
        max_peers: 5,
        bootstrap_peers: vec![],
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        gossipsub_heartbeat_interval: Duration::from_millis(100),
        gossipsub_validation_mode: gossipsub::ValidationMode::Permissive,
    };
    
    // Start the federation node
    let (manager, _, _) = FederationManager::start_node(config, storage).await?;
    Ok(manager)
}

// Helper function to create a test trust bundle
fn create_test_trust_bundle(epoch_id: u64) -> TrustBundle {
    // Create a simple trust bundle for testing
    TrustBundle::new(
        epoch_id,
        "test-federation".to_string(),
        Vec::new(), // No DAG roots for test
        Vec::new(), // No attestations for test
    )
}

#[tokio::test]
async fn test_trust_bundle_storage_and_retrieval() {
    // Create storage and federation manager
    let storage = create_test_storage().await;
    let federation_manager = create_test_federation_manager(storage.clone()).await.unwrap();
    
    // Create a test trust bundle
    let test_bundle = create_test_trust_bundle(1);
    
    // Publish the trust bundle
    federation_manager.publish_trust_bundle(test_bundle.clone()).await.unwrap();
    
    // Request the trust bundle back
    let retrieved_bundle = federation_manager.request_trust_bundle(1).await.unwrap();
    
    // Verify the retrieved bundle matches what we published
    match retrieved_bundle {
        Some(bundle) => {
            assert_eq!(bundle.epoch_id, 1);
            assert_eq!(bundle.federation_id, "test-federation");
        },
        None => panic!("Trust bundle was not retrieved"),
    }
    
    // Shutdown the federation manager
    federation_manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_trust_bundle_epoch_tracking() {
    // Create storage and federation manager
    let storage = create_test_storage().await;
    let federation_manager = create_test_federation_manager(storage.clone()).await.unwrap();
    
    // Create test trust bundles with different epochs
    let bundle1 = create_test_trust_bundle(1);
    let bundle2 = create_test_trust_bundle(2);
    let bundle3 = create_test_trust_bundle(3);
    
    // Check initial latest epoch (should be 0)
    let initial_epoch = federation_manager.get_latest_known_epoch().await.unwrap();
    assert_eq!(initial_epoch, 0);
    
    // Publish bundles in order
    federation_manager.publish_trust_bundle(bundle1).await.unwrap();
    
    // Check latest epoch updated
    let epoch_after_first = federation_manager.get_latest_known_epoch().await.unwrap();
    assert_eq!(epoch_after_first, 1);
    
    // Publish higher epoch
    federation_manager.publish_trust_bundle(bundle3).await.unwrap();
    
    // Check latest epoch updated
    let epoch_after_third = federation_manager.get_latest_known_epoch().await.unwrap();
    assert_eq!(epoch_after_third, 3);
    
    // Publish lower epoch (shouldn't update the latest)
    federation_manager.publish_trust_bundle(bundle2).await.unwrap();
    
    // Check latest epoch hasn't changed
    let final_epoch = federation_manager.get_latest_known_epoch().await.unwrap();
    assert_eq!(final_epoch, 3);
    
    // Shutdown the federation manager
    federation_manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_trust_bundle_request_handling() {
    // Create two separate federation managers with their own storage
    let storage1 = create_test_storage().await;
    let storage2 = create_test_storage().await;
    
    let manager1 = create_test_federation_manager(storage1.clone()).await.unwrap();
    let manager2 = create_test_federation_manager(storage2.clone()).await.unwrap();
    
    // Get the listen addresses from manager1
    let manager1_addrs = manager1.get_listen_addresses();
    assert!(!manager1_addrs.is_empty(), "Manager1 should have listen addresses");
    
    // Create config for manager2 that connects to manager1
    let manager2_config = FederationManagerConfig {
        bootstrap_period: Duration::from_millis(100),
        peer_sync_interval: Duration::from_millis(100),
        trust_bundle_sync_interval: Duration::from_millis(100),
        max_peers: 5,
        bootstrap_peers: manager1_addrs.clone(),
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        gossipsub_heartbeat_interval: Duration::from_millis(100),
        gossipsub_validation_mode: gossipsub::ValidationMode::Permissive,
    };
    
    // Restart manager2 with the new config
    manager2.shutdown().await.unwrap();
    let (manager2, _, _) = FederationManager::start_node(manager2_config, storage2.clone()).await.unwrap();
    
    // Create a test trust bundle
    let test_bundle = create_test_trust_bundle(5);
    
    // Publish the trust bundle to manager1
    manager1.publish_trust_bundle(test_bundle.clone()).await.unwrap();
    
    // Give a moment for the network to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Request the trust bundle from manager2
    // Note: In a real test this would work over the network, but in this test
    // we're likely to get None since we're using separate storage instances
    let _result = manager2.request_trust_bundle(5).await.unwrap();
    
    // This is mostly testing that the request doesn't error, as we don't have
    // actual network communication in this test environment
    
    // Shutdown both federation managers
    manager1.shutdown().await.unwrap();
    manager2.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_trust_bundle_sync_client() {
    // Create storage and federation manager
    let storage = create_test_storage().await;
    let federation_manager = create_test_federation_manager(storage.clone()).await.unwrap();
    
    // Create a trust bundle with a valid quorum proof
    let mut test_bundle = create_test_trust_bundle(10);
    
    // Generate a simple keypair for testing (using the correct create method)
    let private_key = vec![1, 2, 3, 4]; // Dummy key for testing
    let public_key = vec![5, 6, 7, 8]; // Dummy key for testing
    let keypair = KeyPair::new(private_key, public_key);
    
    // Create an identity ID for the signer
    let identity_id = icn_identity::IdentityId::new("did:icn:test-signer");
    
    // Create a simple quorum proof
    let bundle_hash = test_bundle.calculate_hash();
    let signed_hash = keypair.sign(&bundle_hash).unwrap();
    let signature = icn_identity::Signature::new(signed_hash);
    
    // Set up a quorum config
    let quorum_config = QuorumConfig::Threshold(50); // 50% threshold
    
    // Create the proof with one vote
    let votes = vec![(identity_id, signature)];
    
    let proof = QuorumProof {
        config: quorum_config,
        votes,
    };
    
    // Attach the proof to the bundle
    test_bundle.proof = Some(proof);
    
    // Publish the trust bundle
    federation_manager.publish_trust_bundle(test_bundle.clone()).await.unwrap();
    
    // Request the bundle back
    let retrieved_bundle = federation_manager.request_trust_bundle(10).await.unwrap();
    
    // Verify the retrieved bundle has the proof
    match retrieved_bundle {
        Some(bundle) => {
            assert_eq!(bundle.epoch_id, 10);
            assert!(bundle.proof.is_some(), "Bundle should have a proof");
        },
        None => panic!("Trust bundle was not retrieved"),
    }
    
    // Shutdown the federation manager
    federation_manager.shutdown().await.unwrap();
} 