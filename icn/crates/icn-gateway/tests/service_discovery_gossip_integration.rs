//! Integration tests for service discovery with gossip propagation (Issue #935)
//!
//! Tests:
//! - Service announcement publishes to gossip topic
//! - Service withdrawal publishes to gossip topic
//! - Incoming gossip messages are processed
//! - Signature verification
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gateway::service_discovery_mgr::ServiceDiscoveryManager;
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_kernel_api::naming::{ServiceEndpoint, ServiceType};
use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::types::{Endpoint, Signature};
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Helpers
// ============================================================================

fn make_endpoint(id: &str, did: &str, signing_key: &ed25519_dalek::SigningKey) -> ServiceEndpoint {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut ep = ServiceEndpoint {
        service_id: id.to_string(),
        provider: did.to_string(),
        endpoint_type: icn_kernel_api::naming::EndpointType::Http,
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        endpoints: vec![Endpoint::new("https", "example.com", 8443)],
        addresses: vec![],
        capabilities: vec!["read".to_string()],
        trust_threshold: 0.1,
        scope_visibility: ScopeLevel::Org,
        cell_id: None,
        ttl_secs: 3600,
        signature: Signature::new(vec![0; 64]),
        created_at: now,
        updated_at: now,
    };

    // Sign the endpoint
    icn_gossip::sign_service_endpoint(&mut ep, signing_key);
    ep
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_service_manager_publishes_to_gossip() {
    // Create a manager with gossip enabled
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&keypair.to_signing_key_bytes());

    let gossip = Arc::new(RwLock::new(GossipActor::new(did.clone(), None)));
    let mgr = ServiceDiscoveryManager::with_gossip(gossip.clone(), did.clone())
        .await
        .expect("Failed to create manager");

    // Announce a service
    let endpoint = make_endpoint("svc-test", did.as_str(), &signing_key);
    mgr.announce(endpoint.clone())
        .await
        .expect("Failed to announce");

    // Check that the service was published to gossip
    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
    let g = gossip.read().await;
    let entries = g.get_entries(topic);
    assert!(!entries.is_empty(), "Gossip topic should have entries");

    // Decode and verify it's our announcement
    let last_entry = entries.last().unwrap();
    let msg: icn_gossip::ServiceDiscoveryMessage =
        icn_encoding::decode(&last_entry.data).expect("decode");

    match msg {
        icn_gossip::ServiceDiscoveryMessage::Announce { endpoint: ep } => {
            assert_eq!(ep.service_id, "svc-test");
            assert_eq!(ep.provider, did.as_str());
        }
        _ => panic!("Expected Announce message"),
    }
}

#[tokio::test]
async fn test_service_manager_publishes_withdrawal() {
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&keypair.to_signing_key_bytes());

    let gossip = Arc::new(RwLock::new(GossipActor::new(did.clone(), None)));
    let mgr = ServiceDiscoveryManager::with_gossip(gossip.clone(), did.clone())
        .await
        .expect("Failed to create manager");

    // Announce then withdraw
    let endpoint = make_endpoint("svc-temp", did.as_str(), &signing_key);
    mgr.announce(endpoint).await.expect("Failed to announce");
    mgr.withdraw("svc-temp", did.as_str())
        .await
        .expect("Failed to withdraw");

    // Check that withdrawal was published
    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
    let g = gossip.read().await;
    let entries = g.get_entries(topic);
    assert!(
        entries.len() >= 2,
        "Should have announce and withdraw messages"
    );

    // Check that there's a withdrawal message somewhere
    let has_withdrawal = entries.iter().any(|entry| {
        if let Ok(data) = entry.get_data() {
            if let Ok(msg) = icn_encoding::decode::<icn_gossip::ServiceDiscoveryMessage>(&data) {
                matches!(msg, icn_gossip::ServiceDiscoveryMessage::Withdraw { .. })
            } else {
                false
            }
        } else {
            false
        }
    });

    assert!(has_withdrawal, "Should have a Withdraw message in gossip");
}

#[tokio::test]
async fn test_invalid_signature_rejected() {
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&keypair.to_signing_key_bytes());

    let gossip = Arc::new(RwLock::new(GossipActor::new(did.clone(), None)));
    let _mgr = ServiceDiscoveryManager::with_gossip(gossip.clone(), did.clone())
        .await
        .expect("Failed to create manager");

    // Create a signed endpoint then tamper with it
    let mut endpoint = make_endpoint("svc-tampered", did.as_str(), &signing_key);
    endpoint.trust_threshold = 0.9; // Tamper after signing

    // Create the announcement message
    let msg = icn_gossip::ServiceDiscoveryMessage::Announce { endpoint };
    let encoded = icn_encoding::encode(&msg).expect("encode");

    // Publish directly to gossip (bypassing manager's signature check)
    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
    let mut g = gossip.write().await;
    let _ = g.publish(topic, encoded).await;

    // Get the gossip entry that was just published
    let entries = g.get_entries(topic);
    let last_entry = entries.last().expect("Should have entry");
    drop(g);

    // Manually invoke the handler (simulating what the supervisor would do)
    let registry = Arc::new(RwLock::new(std::collections::HashMap::new()));
    let result =
        ServiceDiscoveryManager::handle_gossip_entry(registry.clone(), last_entry.clone()).await;

    // The handler should return an error for tampered signature
    assert!(result.is_err(), "Should reject tampered signature");

    // Verify the endpoint was NOT stored
    let reg = registry.read().await;
    assert!(
        reg.get("svc-tampered").is_none(),
        "Tampered endpoint should be rejected"
    );
}

#[tokio::test]
async fn test_deduplication_by_service_id() {
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&keypair.to_signing_key_bytes());

    let gossip = Arc::new(RwLock::new(GossipActor::new(did.clone(), None)));
    let mgr = ServiceDiscoveryManager::with_gossip(gossip.clone(), did.clone())
        .await
        .expect("Failed to create manager");

    // Announce same service_id twice with different capabilities
    let mut ep1 = make_endpoint("svc-dup", did.as_str(), &signing_key);
    ep1.capabilities = vec!["read".to_string()];
    mgr.announce(ep1).await.expect("Failed to announce ep1");

    let mut ep2 = make_endpoint("svc-dup", did.as_str(), &signing_key);
    ep2.capabilities = vec!["read".to_string(), "write".to_string()];
    mgr.announce(ep2).await.expect("Failed to announce ep2");

    // Get the endpoint - should be the latest one
    let stored = mgr.get("svc-dup").await.expect("Service should exist");
    assert_eq!(
        stored.capabilities.len(),
        2,
        "Should have latest capabilities"
    );
}

#[tokio::test]
async fn test_incoming_announcement_stored() {
    // Create manager that will receive announcements
    let keypair_b = KeyPair::generate().unwrap();
    let did_b = keypair_b.did().clone();

    let gossip_b = Arc::new(RwLock::new(GossipActor::new(did_b.clone(), None)));
    let _mgr_b = ServiceDiscoveryManager::with_gossip(gossip_b.clone(), did_b.clone())
        .await
        .expect("Failed to create manager B");

    // Create a valid signed endpoint from another node
    let keypair_a = KeyPair::generate().unwrap();
    let did_a = keypair_a.did().clone();
    let signing_key_a = ed25519_dalek::SigningKey::from_bytes(&keypair_a.to_signing_key_bytes());
    let endpoint = make_endpoint("svc-remote", did_a.as_str(), &signing_key_a);

    // Publish it directly to node B's gossip
    let msg = icn_gossip::ServiceDiscoveryMessage::Announce {
        endpoint: endpoint.clone(),
    };
    let encoded = icn_encoding::encode(&msg).expect("encode");

    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
    let mut g = gossip_b.write().await;
    let _ = g.publish(topic, encoded).await;

    // Get the gossip entry that was just published
    let entries = g.get_entries(topic);
    let last_entry = entries.last().expect("Should have entry");
    drop(g);

    // Manually invoke the handler (simulating what the supervisor would do)
    let registry = Arc::new(RwLock::new(std::collections::HashMap::new()));
    let _ =
        ServiceDiscoveryManager::handle_gossip_entry(registry.clone(), last_entry.clone()).await;

    // Verify the endpoint was stored
    let reg = registry.read().await;
    let discovered = reg.get("svc-remote");
    assert!(discovered.is_some(), "Remote service should be discovered");

    let ep = discovered.unwrap();
    assert_eq!(ep.service_id, "svc-remote");
    assert_eq!(ep.provider, did_a.as_str());
}

#[tokio::test]
async fn test_malicious_withdrawal_rejected() {
    // Test that a node cannot forge a withdrawal for another node's service
    //
    // Scenario:
    // 1. Node A announces a service
    // 2. Node B receives the announcement and stores it
    // 3. Node B tries to forge a withdrawal for Node A's service
    // 4. The forged withdrawal should be rejected (service should remain in registry)

    // Node A creates and announces a service
    let keypair_a = KeyPair::generate().unwrap();
    let did_a = keypair_a.did().clone();
    let signing_key_a = ed25519_dalek::SigningKey::from_bytes(&keypair_a.to_signing_key_bytes());

    let endpoint_a = make_endpoint("svc-owned-by-a", did_a.as_str(), &signing_key_a);

    // Node B's setup - it will receive Node A's announcement
    let keypair_b = KeyPair::generate().unwrap();
    let did_b = keypair_b.did().clone();

    let gossip_b = Arc::new(RwLock::new(GossipActor::new(did_b.clone(), None)));
    let _mgr_b = ServiceDiscoveryManager::with_gossip(gossip_b.clone(), did_b.clone())
        .await
        .expect("Failed to create manager B");

    // Create a shared registry for Node B that we can inspect
    let registry_b = Arc::new(RwLock::new(std::collections::HashMap::new()));

    // Step 1: Propagate Node A's announcement to Node B's registry
    // Simulate Node A's announcement arriving via gossip
    let announce_msg = icn_gossip::ServiceDiscoveryMessage::Announce {
        endpoint: endpoint_a.clone(),
    };
    let encoded_announce = icn_encoding::encode(&announce_msg).expect("encode");

    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
    let mut g = gossip_b.write().await;
    let _ = g.publish(topic, encoded_announce).await;
    let entries = g.get_entries(topic);
    let announce_entry = entries.last().expect("Should have announce entry");
    drop(g);

    // Process the announcement in Node B's registry
    ServiceDiscoveryManager::handle_gossip_entry(registry_b.clone(), announce_entry.clone())
        .await
        .expect("Should accept valid announcement");

    // Verify the service is now in Node B's registry
    {
        let reg = registry_b.read().await;
        assert!(
            reg.get("svc-owned-by-a").is_some(),
            "Service should be in Node B's registry after announcement"
        );
    }

    // Step 2: Node B tries to forge a withdrawal for Node A's service
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let forged_msg = icn_gossip::ServiceDiscoveryMessage::Withdraw {
        service_id: "svc-owned-by-a".to_string(),
        provider: did_a.clone(), // Claims to be Node A
        timestamp,
    };
    let encoded_forged = icn_encoding::encode(&forged_msg).expect("encode");

    // Node B publishes the forged withdrawal (entry.author will be did_b, not did_a)
    let mut g = gossip_b.write().await;
    let forged_hash = g
        .publish(topic, encoded_forged)
        .await
        .expect("publish should succeed");
    
    // Get the specific entry we just published by its hash
    let forged_entry = g
        .get_entry(topic, &forged_hash)
        .expect("Should be able to retrieve published entry");
    
    // Verify this is a withdrawal message
    let forged_data = forged_entry.get_data().expect("get data");
    let decoded: icn_gossip::ServiceDiscoveryMessage =
        icn_encoding::decode(&forged_data).expect("decode");
    assert!(
        matches!(decoded, icn_gossip::ServiceDiscoveryMessage::Withdraw { .. }),
        "Expected Withdraw message"
    );
    
    drop(g);

    // Step 3: Process the forged withdrawal - should fail
    let result =
        ServiceDiscoveryManager::handle_gossip_entry(registry_b.clone(), forged_entry.clone())
            .await;

    // The handler should return an error because entry.author (did_b) != provider (did_a)
    assert!(
        result.is_err(),
        "Forged withdrawal should be rejected with author mismatch error"
    );

    // Step 4: Verify the service is STILL in the registry (withdrawal was rejected)
    let reg = registry_b.read().await;
    assert!(
        reg.get("svc-owned-by-a").is_some(),
        "Service should still exist after forged withdrawal is rejected"
    );
}
