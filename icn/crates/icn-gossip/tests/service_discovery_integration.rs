//! Integration tests for service discovery gossip (Epic 3, Issue #937)
//!
//! Tests:
//! - Gossip message roundtrip (encode → decode → verify)
//! - Scope filtering on discovery
//! - Withdrawal propagation
//! - Sign/verify across keypair lifecycle
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::service_discovery::{
    sign_service_endpoint, verify_service_endpoint, ServiceDiscoveryMessage,
};
use icn_identity::KeyPair;
use icn_kernel_api::naming::{ServiceEndpoint, ServiceType};
use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::types::{Endpoint, Signature};

// ============================================================================
// Helpers
// ============================================================================

fn make_keypair() -> (ed25519_dalek::SigningKey, icn_identity::Did) {
    let kp = KeyPair::generate().unwrap();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&kp.to_signing_key_bytes());
    let did = kp.did().clone();
    (signing_key, did)
}

fn make_endpoint(
    id: &str,
    did: &icn_identity::Did,
    scope: ScopeLevel,
    caps: Vec<&str>,
) -> ServiceEndpoint {
    ServiceEndpoint {
        service_id: id.to_string(),
        provider: did.to_string(),
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        endpoints: vec![Endpoint::new("https", "node.example.com", 8443)],
        capabilities: caps.into_iter().map(String::from).collect(),
        trust_threshold: 0.1,
        scope_visibility: scope,
        ttl_secs: 3600,
        signature: Signature::new(vec![0; 64]),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    }
}

// ============================================================================
// Gossip roundtrip tests
// ============================================================================

#[test]
fn test_announce_message_gossip_roundtrip() {
    let (signing_key, did) = make_keypair();
    let mut ep = make_endpoint("svc-ledger-1", &did, ScopeLevel::Org, vec!["read", "write"]);

    // Sign the endpoint
    sign_service_endpoint(&mut ep, &signing_key);

    // Wrap in gossip message
    let msg = ServiceDiscoveryMessage::Announce {
        endpoint: ep.clone(),
    };

    // Encode → decode (simulates gossip transport)
    let encoded = icn_encoding::encode(&msg).expect("encode");
    let decoded: ServiceDiscoveryMessage = icn_encoding::decode(&encoded).expect("decode");

    // Verify decoded message matches
    match decoded {
        ServiceDiscoveryMessage::Announce { endpoint } => {
            assert_eq!(endpoint.service_id, "svc-ledger-1");
            assert_eq!(endpoint.provider, did.to_string());
            assert_eq!(endpoint.capabilities, vec!["read", "write"]);

            // Verify signature survives roundtrip
            verify_service_endpoint(&endpoint).expect("signature valid after roundtrip");
        }
        _ => panic!("Expected Announce message"),
    }
}

#[test]
fn test_withdraw_message_gossip_roundtrip() {
    let (_, did) = make_keypair();
    let msg = ServiceDiscoveryMessage::Withdraw {
        service_id: "svc-old".to_string(),
        provider: did.clone(),
        timestamp: 1700000000,
    };

    let encoded = icn_encoding::encode(&msg).expect("encode");
    let decoded: ServiceDiscoveryMessage = icn_encoding::decode(&encoded).expect("decode");

    match decoded {
        ServiceDiscoveryMessage::Withdraw {
            service_id,
            provider,
            timestamp,
        } => {
            assert_eq!(service_id, "svc-old");
            assert_eq!(provider.to_string(), did.to_string());
            assert_eq!(timestamp, 1700000000);
        }
        _ => panic!("Expected Withdraw message"),
    }
}

#[test]
fn test_query_response_roundtrip() {
    let (signing_key, did) = make_keypair();
    let (_, requester_did) = make_keypair();

    // Create and sign an endpoint for the response
    let mut ep = make_endpoint("svc-1", &did, ScopeLevel::Org, vec!["read"]);
    sign_service_endpoint(&mut ep, &signing_key);

    let query = ServiceDiscoveryMessage::Query {
        requester: requester_did.clone(),
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        max_scope: ScopeLevel::Federation,
        required_capabilities: vec!["read".to_string()],
        query_id: "q-001".to_string(),
    };

    let encoded = icn_encoding::encode(&query).expect("encode query");
    let decoded: ServiceDiscoveryMessage = icn_encoding::decode(&encoded).expect("decode query");
    assert_eq!(query, decoded);

    let response = ServiceDiscoveryMessage::Response {
        query_id: "q-001".to_string(),
        endpoints: vec![ep.clone()],
        responder: did.clone(),
    };

    let encoded = icn_encoding::encode(&response).expect("encode response");
    let decoded: ServiceDiscoveryMessage = icn_encoding::decode(&encoded).expect("decode response");

    match decoded {
        ServiceDiscoveryMessage::Response {
            query_id,
            endpoints,
            responder,
        } => {
            assert_eq!(query_id, "q-001");
            assert_eq!(endpoints.len(), 1);
            assert_eq!(responder.to_string(), did.to_string());
            verify_service_endpoint(&endpoints[0]).expect("signature valid in response");
        }
        _ => panic!("Expected Response message"),
    }
}

// ============================================================================
// Scope filtering tests
// ============================================================================

#[test]
fn test_scope_visibility_hierarchy() {
    // Endpoints at different scopes should respect the ScopeLevel ordering
    let (signing_key, did) = make_keypair();

    let scopes = [
        ScopeLevel::Local,
        ScopeLevel::Cell,
        ScopeLevel::Org,
        ScopeLevel::Federation,
        ScopeLevel::Commons,
    ];

    let mut endpoints: Vec<ServiceEndpoint> = Vec::new();
    for (i, &scope) in scopes.iter().enumerate() {
        let mut ep = make_endpoint(&format!("svc-{i}"), &did, scope, vec![]);
        sign_service_endpoint(&mut ep, &signing_key);
        endpoints.push(ep);
    }

    // Org scope should include Local, Cell, and Org endpoints
    let org_visible: Vec<_> = endpoints
        .iter()
        .filter(|ep| ScopeLevel::Org.includes(ep.scope_visibility))
        .collect();
    assert_eq!(org_visible.len(), 3); // Local, Cell, Org

    // Federation scope should include all except Commons
    let fed_visible: Vec<_> = endpoints
        .iter()
        .filter(|ep| ScopeLevel::Federation.includes(ep.scope_visibility))
        .collect();
    assert_eq!(fed_visible.len(), 4); // Local, Cell, Org, Federation

    // Commons includes everything
    let commons_visible: Vec<_> = endpoints
        .iter()
        .filter(|ep| ScopeLevel::Commons.includes(ep.scope_visibility))
        .collect();
    assert_eq!(commons_visible.len(), 5);
}

// ============================================================================
// Signing lifecycle tests
// ============================================================================

#[test]
fn test_multiple_endpoints_from_same_provider() {
    let (signing_key, did) = make_keypair();

    let mut ep1 = make_endpoint("svc-1", &did, ScopeLevel::Org, vec!["read"]);
    let mut ep2 = make_endpoint("svc-2", &did, ScopeLevel::Federation, vec!["write"]);

    sign_service_endpoint(&mut ep1, &signing_key);
    sign_service_endpoint(&mut ep2, &signing_key);

    // Both signatures should verify independently
    verify_service_endpoint(&ep1).expect("ep1 valid");
    verify_service_endpoint(&ep2).expect("ep2 valid");

    // Signatures should be different (different payloads)
    assert_ne!(ep1.signature.as_bytes(), ep2.signature.as_bytes());
}

#[test]
fn test_different_providers_independent_signing() {
    let (key_a, did_a) = make_keypair();
    let (key_b, did_b) = make_keypair();

    let mut ep_a = make_endpoint("svc-a", &did_a, ScopeLevel::Org, vec![]);
    let mut ep_b = make_endpoint("svc-b", &did_b, ScopeLevel::Org, vec![]);

    sign_service_endpoint(&mut ep_a, &key_a);
    sign_service_endpoint(&mut ep_b, &key_b);

    verify_service_endpoint(&ep_a).expect("ep_a valid");
    verify_service_endpoint(&ep_b).expect("ep_b valid");

    // Cross-verify should fail (swap signatures)
    let mut ep_a_wrong_sig = ep_a.clone();
    ep_a_wrong_sig.signature = ep_b.signature.clone();
    assert!(verify_service_endpoint(&ep_a_wrong_sig).is_err());
}
