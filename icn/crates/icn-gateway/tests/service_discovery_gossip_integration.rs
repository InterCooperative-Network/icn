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

fn make_endpoint(
    id: &str,
    did: &str,
    signing_key: &ed25519_dalek::SigningKey,
) -> ServiceEndpoint {
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
    mgr.announce(endpoint.clone()).await.expect("Failed to announce");
    
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
    mgr.withdraw("svc-temp", did.as_str()).await.expect("Failed to withdraw");
    
    // Check that withdrawal was published
    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
    let g = gossip.read().await;
    let entries = g.get_entries(topic);
    assert!(entries.len() >= 2, "Should have announce and withdraw messages");
    
    // Check that there's a withdrawal message somewhere
    let has_withdrawal = entries.iter().any(|entry| {
        if let Ok(msg) = icn_encoding::decode::<icn_gossip::ServiceDiscoveryMessage>(&entry.data) {
            matches!(msg, icn_gossip::ServiceDiscoveryMessage::Withdraw { .. })
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
    let mgr = ServiceDiscoveryManager::with_gossip(gossip.clone(), did.clone())
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
    drop(g);
    
    // Give the callback time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // The tampered endpoint should not be in the registry
    assert!(
        mgr.get("svc-tampered").await.is_none(),
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
    assert_eq!(stored.capabilities.len(), 2, "Should have latest capabilities");
}

#[tokio::test]
async fn test_incoming_announcement_stored() {
    // Create manager that will receive announcements
    let keypair_b = KeyPair::generate().unwrap();
    let did_b = keypair_b.did().clone();
    
    let gossip_b = Arc::new(RwLock::new(GossipActor::new(did_b.clone(), None)));
    let mgr_b = ServiceDiscoveryManager::with_gossip(gossip_b.clone(), did_b.clone())
        .await
        .expect("Failed to create manager B");
    
    // Create a valid signed endpoint from another node
    let keypair_a = KeyPair::generate().unwrap();
    let did_a = keypair_a.did().clone();
    let signing_key_a = ed25519_dalek::SigningKey::from_bytes(&keypair_a.to_signing_key_bytes());
    let endpoint = make_endpoint("svc-remote", did_a.as_str(), &signing_key_a);
    
    // Publish it directly to node B's gossip
    let msg = icn_gossip::ServiceDiscoveryMessage::Announce { endpoint };
    let encoded = icn_encoding::encode(&msg).expect("encode");
    
    let topic = icn_gossip::service_discovery_topics::SERVICES_ANNOUNCE;
    let mut g = gossip_b.write().await;
    let _ = g.publish(topic, encoded).await;
    drop(g);
    
    // Give the callback time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Node B should have stored the endpoint
    let discovered = mgr_b.get("svc-remote").await;
    assert!(discovered.is_some(), "Remote service should be discovered");
    
    let ep = discovered.unwrap();
    assert_eq!(ep.service_id, "svc-remote");
    assert_eq!(ep.provider, did_a.as_str());
}
