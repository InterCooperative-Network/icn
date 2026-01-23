#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for trust-gated rate limiting
//!
//! Tests the TrustRateLimiter component which applies different rate limits
//! based on trust class (Isolated, Known, Partner, Federated).

use icn_gateway::{TrustRateLimitConfig, TrustRateLimiter};
use icn_identity::{Did, KeyPair};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Helper to create a test trust manager with a given trust graph
fn create_trust_manager(
    trust_graph: Arc<RwLock<TrustGraph>>,
) -> Arc<icn_gateway::trust_mgr::TrustManager> {
    Arc::new(icn_gateway::trust_mgr::TrustManager::with_handle(
        trust_graph,
    ))
}

/// Helper to create a DID for testing
fn create_test_did(_name: &str) -> (KeyPair, Did) {
    let kp = KeyPair::generate().expect("Failed to generate keypair");
    let did = kp.did().clone();
    (kp, did)
}

/// Helper to create a test trust graph with storage
async fn create_test_trust_graph(self_did: Did) -> TrustGraph {
    let store = Arc::new(SledStore::temporary().expect("Failed to create temporary store"));
    TrustGraph::new(store, self_did)
}

#[tokio::test]
async fn test_trust_rate_limiter_isolated_peer() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let trust_graph = Arc::new(RwLock::new(create_test_trust_graph(self_did.clone()).await));
    let trust_manager = create_trust_manager(trust_graph.clone());

    // Create trust-gated rate limiter with low isolated limits for testing
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0,
        known_requests_per_sec: 50.0,
        known_burst: 10.0,
        partner_requests_per_sec: 100.0,
        partner_burst: 20.0,
        federated_requests_per_sec: 200.0,
        federated_burst: 50.0,
    };
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Create an unknown peer (no trust edge)
    let (_peer_kp, peer_did) = create_test_did("unknown_peer");

    // First request should succeed (burst capacity = 2)
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_ok(),
        "First request should succeed"
    );

    // Second request should succeed (still within burst)
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_ok(),
        "Second request should succeed"
    );

    // Third request should fail (burst exhausted)
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_err(),
        "Third request should fail (isolated peer rate limited)"
    );

    // Wait for refill (500ms = 1 token at 2/sec)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Should now succeed after refill
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_ok(),
        "Request should succeed after refill"
    );
}

#[tokio::test]
async fn test_trust_rate_limiter_known_peer() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let mut trust_graph = create_test_trust_graph(self_did.clone()).await;

    // Create a known peer with trust score 0.2 (Known class: 0.1-0.4)
    let (_peer_kp, peer_did) = create_test_did("known_peer");
    trust_graph
        .add_edge(TrustEdge::new(self_did.clone(), peer_did.clone(), 0.2))
        .expect("Failed to add trust edge");

    let trust_graph = Arc::new(RwLock::new(trust_graph));
    let trust_manager = create_trust_manager(trust_graph.clone());

    // Create rate limiter with lower Known limits for testing
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0,
        known_requests_per_sec: 5.0,
        known_burst: 5.0,
        partner_requests_per_sec: 100.0,
        partner_burst: 20.0,
        federated_requests_per_sec: 200.0,
        federated_burst: 50.0,
    };
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Known peer should have higher limits (burst = 5)
    for i in 1..=5 {
        assert!(
            rate_limiter.check(&peer_did.to_string()).await.is_ok(),
            "Request {} should succeed for known peer",
            i
        );
    }

    // 6th request should fail
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_err(),
        "6th request should fail (known peer rate limited)"
    );
}

#[tokio::test]
async fn test_trust_rate_limiter_partner_peer() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let mut trust_graph = create_test_trust_graph(self_did.clone()).await;

    // Create a partner peer with trust score 0.5 (Partner class: 0.4-0.7)
    let (_peer_kp, peer_did) = create_test_did("partner_peer");
    trust_graph
        .add_edge(TrustEdge::new(self_did.clone(), peer_did.clone(), 0.5))
        .expect("Failed to add trust edge");

    let trust_graph = Arc::new(RwLock::new(trust_graph));
    let trust_manager = create_trust_manager(trust_graph.clone());

    // Create rate limiter with moderate Partner limits for testing
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0,
        known_requests_per_sec: 5.0,
        known_burst: 5.0,
        partner_requests_per_sec: 10.0,
        partner_burst: 10.0,
        federated_requests_per_sec: 200.0,
        federated_burst: 50.0,
    };
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Partner peer should have even higher limits (burst = 10)
    for i in 1..=10 {
        assert!(
            rate_limiter.check(&peer_did.to_string()).await.is_ok(),
            "Request {} should succeed for partner peer",
            i
        );
    }

    // 11th request should fail
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_err(),
        "11th request should fail (partner peer rate limited)"
    );
}

#[tokio::test]
async fn test_trust_rate_limiter_federated_peer() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let mut trust_graph = create_test_trust_graph(self_did.clone()).await;

    // Create a federated peer with trust score 0.8 (Federated class: >= 0.7)
    let (_peer_kp, peer_did) = create_test_did("federated_peer");
    trust_graph
        .add_edge(TrustEdge::new(self_did.clone(), peer_did.clone(), 0.8))
        .expect("Failed to add trust edge");

    let trust_graph = Arc::new(RwLock::new(trust_graph));
    let trust_manager = create_trust_manager(trust_graph.clone());

    // Create rate limiter with high Federated limits for testing
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0,
        known_requests_per_sec: 5.0,
        known_burst: 5.0,
        partner_requests_per_sec: 10.0,
        partner_burst: 10.0,
        federated_requests_per_sec: 20.0,
        federated_burst: 20.0,
    };
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Federated peer should have highest limits (burst = 20)
    for i in 1..=20 {
        assert!(
            rate_limiter.check(&peer_did.to_string()).await.is_ok(),
            "Request {} should succeed for federated peer",
            i
        );
    }

    // 21st request should fail
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_err(),
        "21st request should fail (federated peer rate limited)"
    );
}

#[tokio::test]
async fn test_trust_rate_limiter_trust_upgrade() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let mut trust_graph = create_test_trust_graph(self_did.clone()).await;

    // Create a peer starting as Isolated (no trust edge)
    let (_peer_kp, peer_did) = create_test_did("upgrading_peer");

    let trust_graph = Arc::new(RwLock::new(trust_graph));
    let trust_manager = create_trust_manager(trust_graph.clone());

    // Create rate limiter
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0,
        known_requests_per_sec: 5.0,
        known_burst: 5.0,
        partner_requests_per_sec: 10.0,
        partner_burst: 10.0,
        federated_requests_per_sec: 20.0,
        federated_burst: 20.0,
    };
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Use isolated limits (burst = 2)
    assert!(rate_limiter.check(&peer_did.to_string()).await.is_ok());
    assert!(rate_limiter.check(&peer_did.to_string()).await.is_ok());
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_err(),
        "Should be rate limited as Isolated"
    );

    // Now upgrade the peer to Partner (0.5)
    {
        let mut graph = trust_graph.write().await;
        graph
            .add_edge(TrustEdge::new(self_did.clone(), peer_did.clone(), 0.5))
            .expect("Failed to add trust edge");
    }

    // Wait a bit for the trust class change to be detected
    tokio::time::sleep(Duration::from_millis(100)).await;

    // After trust upgrade, should get new bucket with Partner limits (burst = 10)
    // The peer should be able to make more requests now
    for i in 1..=10 {
        assert!(
            rate_limiter.check(&peer_did.to_string()).await.is_ok(),
            "Request {} should succeed after trust upgrade",
            i
        );
    }

    // 11th request should fail with Partner limits
    assert!(
        rate_limiter.check(&peer_did.to_string()).await.is_err(),
        "Should be rate limited with Partner limits"
    );
}

#[tokio::test]
async fn test_trust_rate_limiter_anonymous_request() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let trust_graph = Arc::new(RwLock::new(create_test_trust_graph(self_did.clone()).await));
    let trust_manager = create_trust_manager(trust_graph);

    // Create rate limiter
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0,
        known_requests_per_sec: 50.0,
        known_burst: 10.0,
        partner_requests_per_sec: 100.0,
        partner_burst: 20.0,
        federated_requests_per_sec: 200.0,
        federated_burst: 50.0,
    };
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Use a valid DID format for anonymous (synthetic keypair with all zeros)
    let anonymous_kp =
        KeyPair::from_bytes(&[0u8; 32], &[0u8; 32]).expect("Failed to create anonymous keypair");
    let anonymous_did = anonymous_kp.did().to_string();

    // Should use Isolated limits (burst = 2)
    let result1 = rate_limiter.check(&anonymous_did).await;
    assert!(
        result1.is_ok(),
        "First anonymous request should succeed: {:?}",
        result1.err()
    );

    let result2 = rate_limiter.check(&anonymous_did).await;
    assert!(
        result2.is_ok(),
        "Second anonymous request should succeed: {:?}",
        result2.err()
    );

    let result3 = rate_limiter.check(&anonymous_did).await;
    assert!(
        result3.is_err(),
        "Third anonymous request should fail (Isolated limits)"
    );
}

#[tokio::test]
async fn test_trust_rate_limiter_cleanup() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let trust_graph = Arc::new(RwLock::new(create_test_trust_graph(self_did.clone()).await));
    let trust_manager = create_trust_manager(trust_graph);

    // Create rate limiter
    let config = TrustRateLimitConfig::default();
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Make requests from multiple DIDs
    for i in 0..5 {
        let (_kp, did) = create_test_did(&format!("peer{}", i));
        rate_limiter.check(&did.to_string()).await.ok();
    }

    // Clean up buckets inactive for > 0 seconds (should remove all)
    let removed = rate_limiter.cleanup_inactive_buckets(Duration::from_secs(0));
    assert_eq!(removed, 5, "Should clean up all inactive buckets");

    // Make a request and verify bucket is still there after short cleanup window
    let (_kp, did) = create_test_did("active_peer");
    rate_limiter.check(&did.to_string()).await.ok();

    let removed = rate_limiter.cleanup_inactive_buckets(Duration::from_secs(3600));
    assert_eq!(removed, 0, "Should not clean up recently active buckets");
}

#[tokio::test]
async fn test_trust_rate_limiter_config_defaults() {
    let config = TrustRateLimitConfig::default();

    // Verify default values match spec
    assert_eq!(config.isolated_requests_per_sec, 10.0);
    assert_eq!(config.isolated_burst, 2.0);
    assert_eq!(config.known_requests_per_sec, 50.0);
    assert_eq!(config.known_burst, 10.0);
    assert_eq!(config.partner_requests_per_sec, 100.0);
    assert_eq!(config.partner_burst, 20.0);
    assert_eq!(config.federated_requests_per_sec, 200.0);
    assert_eq!(config.federated_burst, 50.0);
}

#[tokio::test]
async fn test_trust_rate_limiter_per_did_isolation() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let trust_graph = Arc::new(RwLock::new(create_test_trust_graph(self_did.clone()).await));
    let trust_manager = create_trust_manager(trust_graph);

    // Create rate limiter with low limits
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0,
        known_requests_per_sec: 5.0,
        known_burst: 5.0,
        partner_requests_per_sec: 10.0,
        partner_burst: 10.0,
        federated_requests_per_sec: 20.0,
        federated_burst: 20.0,
    };
    let rate_limiter = TrustRateLimiter::new(trust_manager, config);

    // Create two peers
    let (_peer1_kp, peer1_did) = create_test_did("peer1");
    let (_peer2_kp, peer2_did) = create_test_did("peer2");

    // Exhaust peer1's limits
    assert!(rate_limiter.check(&peer1_did.to_string()).await.is_ok());
    assert!(rate_limiter.check(&peer1_did.to_string()).await.is_ok());
    assert!(
        rate_limiter.check(&peer1_did.to_string()).await.is_err(),
        "Peer1 should be rate limited"
    );

    // Peer2 should still have their own independent limits
    assert!(
        rate_limiter.check(&peer2_did.to_string()).await.is_ok(),
        "Peer2 should not be affected by peer1's rate limit"
    );
    assert!(
        rate_limiter.check(&peer2_did.to_string()).await.is_ok(),
        "Peer2 should have full burst capacity"
    );
}
