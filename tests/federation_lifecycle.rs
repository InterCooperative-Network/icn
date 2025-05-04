use icn_federation::genesis::{bootstrap, trustbundle, FederationMetadata};
use icn_federation::guardian::{initialization, QuorumType};
use icn_federation::dag_anchor::anchor;
use icn_federation::recovery::recovery;
use icn_federation::dag_client::{DagClient, InMemoryDagClient, FederationDagEvent, FederationReplayEngine};
use icn_identity::{KeyPair, Signature};
use chrono::Utc;

#[tokio::test]
async fn test_federation_lifecycle() {
    // 1. Set up an in-memory DAG client for testing
    let dag_client = InMemoryDagClient::default();
    
    // 2. Initialize guardians with majority quorum
    let (guardians, quorum_config) = initialization::initialize_guardian_set(
        3, // 3 guardians
        QuorumType::Majority, // Majority voting (2 out of 3)
    ).await.unwrap();
    
    println!("✅ Initialized {} guardians with majority quorum", guardians.len());
    
    // 3. Create guardian credentials
    let federation_did = "did:key:z6MkTestFederation".to_string();
    let mut guardians_with_credentials = guardians.clone();
    let guardian_credentials = initialization::create_guardian_credentials(
        &mut guardians_with_credentials,
        &federation_did,
    ).await.unwrap();
    
    println!("✅ Created guardian credentials for federation {}", federation_did);
    
    // 4. Initialize federation
    let (metadata, establishment_credential, _) = bootstrap::initialize_federation(
        "Test Federation".to_string(),
        Some("A federation for lifecycle testing".to_string()),
        &guardians_with_credentials,
        quorum_config.clone(),
        Vec::new(), // No initial policies
        Vec::new(), // No initial members
    ).await.unwrap();
    
    println!("✅ Initialized federation: {}", metadata.name);
    
    // 5. Create genesis trust bundle
    let guardian_credentials_vec = guardian_credentials.iter()
        .map(|gc| gc.credential.clone())
        .collect();
    
    let trust_bundle = trustbundle::create_trust_bundle(
        &metadata,
        establishment_credential.clone(),
        guardian_credentials_vec,
        &guardians_with_credentials,
    ).await.unwrap();
    
    println!("✅ Created genesis trust bundle with CID: {}", trust_bundle.federation_metadata_cid);
    
    // 6. Create federation keypair for signing
    let federation_keypair = KeyPair::new(vec![1, 2, 3, 4], vec![5, 6, 7, 8, 9]);
    
    // 7. Create genesis anchor
    let genesis_anchor = anchor::create_genesis_anchor(
        &trust_bundle,
        &federation_keypair,
        &federation_did,
    ).await.unwrap();
    
    println!("✅ Created genesis anchor with DAG root: {}", genesis_anchor.dag_root_cid);
    
    // 8. Store genesis event in DAG
    let genesis_event = FederationDagEvent::Genesis(genesis_anchor.clone());
    let genesis_cid = dag_client.store_event(genesis_event).await.unwrap();
    
    println!("✅ Stored genesis event in DAG with CID: {}", genesis_cid);
    
    // 9. Create a key rotation event
    let new_federation_keypair = KeyPair::new(vec![9, 8, 7, 6], vec![5, 4, 3, 2, 1]);
    let key_rotation = recovery::create_key_rotation_event(
        &federation_did,
        &new_federation_keypair,
        1, // First event after genesis
        Some(genesis_cid.clone()),
        &guardians_with_credentials,
        &quorum_config,
    ).await.unwrap();
    
    println!("✅ Created key rotation event");
    
    // 10. Store key rotation event in DAG
    let key_rotation_event = FederationDagEvent::KeyRotation(key_rotation.clone());
    let rotation_cid = dag_client.store_event(key_rotation_event).await.unwrap();
    
    println!("✅ Stored key rotation event in DAG with CID: {}", rotation_cid);
    
    // 11. Create a metadata update event
    let updated_metadata = FederationMetadata {
        federation_did: federation_did.clone(),
        name: "Updated Test Federation".to_string(),
        description: Some("This federation has been updated".to_string()),
        created_at: metadata.created_at, // Keep original creation time
        initial_policies: vec![],
        initial_members: vec![],
    };
    
    let metadata_update = recovery::create_metadata_update_event(
        &federation_did,
        2, // Second event after genesis
        Some(rotation_cid.clone()),
        updated_metadata,
        &guardians_with_credentials,
        &quorum_config,
    ).await.unwrap();
    
    println!("✅ Created metadata update event");
    
    // 12. Store metadata update event in DAG
    let metadata_update_event = FederationDagEvent::MetadataUpdate(metadata_update.clone());
    let metadata_cid = dag_client.store_event(metadata_update_event).await.unwrap();
    
    println!("✅ Stored metadata update event in DAG with CID: {}", metadata_cid);
    
    // 13. Validate the event chain
    let valid = dag_client.verify_event_chain(&metadata_cid).await.unwrap();
    assert!(valid, "Event chain validation should succeed");
    println!("✅ Validated event chain from metadata update to genesis");
    
    // 14. Create the replay engine and replay all events
    let replay_engine = FederationReplayEngine::new(&dag_client);
    let events = replay_engine.replay_federation(&federation_did).await.unwrap();
    
    // 15. Verify the federation state after replay
    assert_eq!(events.len(), 3, "Should have 3 events: genesis, key rotation, and metadata update");
    assert_eq!(events[0].event_type(), "genesis");
    assert_eq!(events[1].event_type(), "key_rotation");
    assert_eq!(events[2].event_type(), "metadata_update");
    
    println!("✅ Successfully replayed all federation events");
    
    // 16. In a real implementation, we would now create a federation state by applying 
    // these events in sequence
    println!("✅ End-to-end federation lifecycle test completed successfully");
} 