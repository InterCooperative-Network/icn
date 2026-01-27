#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for trust-gated rate limiting
//!
//! Tests the TrustRateLimiter component which applies different rate limits
//! based on trust class (Isolated, Known, Partner, Federated).

use icn_gateway::{TrustRateLimitConfig, TrustRateLimiter};
use icn_identity::{Did, KeyPair};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustScore};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_trust_rate_limiter_known_peer() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let mut trust_graph = create_test_trust_graph(self_did.clone()).await;

    // Create a known peer with trust score 0.2 (Known class: 0.1-0.4)
    let (_peer_kp, peer_did) = create_test_did("known_peer");
    trust_graph
        .add_edge(TrustEdge::new(
            self_did.clone(),
            peer_did.clone(),
            TrustScore::unchecked(0.2),
        ))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_trust_rate_limiter_partner_peer() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let mut trust_graph = create_test_trust_graph(self_did.clone()).await;

    // Create a partner peer with trust score in Partner class (0.4-0.7)
    // Note: compute_trust_score uses weights: direct * 0.7 + transitive * 0.3
    // So edge score 0.7 → actual score 0.7 * 0.7 = 0.49 (Partner class)
    let (_peer_kp, peer_did) = create_test_did("partner_peer");
    trust_graph
        .add_edge(TrustEdge::new(
            self_did.clone(),
            peer_did.clone(),
            TrustScore::unchecked(0.7),
        ))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_trust_rate_limiter_federated_peer() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let mut trust_graph = create_test_trust_graph(self_did.clone()).await;

    // Create a federated peer with trust score in Federated class (>= 0.7)
    // Note: compute_trust_score uses weights: direct * 0.7 + transitive * 0.3
    // So edge score 1.0 → actual score 1.0 * 0.7 = 0.7 (Federated class)
    let (_peer_kp, peer_did) = create_test_did("federated_peer");
    trust_graph
        .add_edge(TrustEdge::new(
            self_did.clone(),
            peer_did.clone(),
            TrustScore::unchecked(1.0),
        ))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_trust_rate_limiter_trust_upgrade() {
    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let trust_graph = create_test_trust_graph(self_did.clone()).await;

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

    // Now upgrade the peer to Partner class
    // Note: compute_trust_score uses weights: direct * 0.7 + transitive * 0.3
    // So edge score 0.7 → actual score 0.7 * 0.7 = 0.49 (Partner class)
    {
        let mut graph = trust_graph.write().await;
        graph
            .add_edge(TrustEdge::new(
                self_did.clone(),
                peer_did.clone(),
                TrustScore::unchecked(0.7),
            ))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
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

    // Use synthetic anonymous DID format (matches middleware behavior)
    // In the middleware, anonymous users get did:icn:anonymous:{ip}
    let anonymous_did = "did:icn:anonymous:127.0.0.1".to_string();

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
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

// ============================================================================
// HTTP Integration Tests
// ============================================================================
// These tests verify the middleware is properly wired up and responds via HTTP.

/// Test that the trust_rate_limit_middleware correctly rate limits HTTP requests
/// when the TrustRateLimiter is configured.
///
/// This test verifies:
/// 1. The middleware is correctly wired up via wrap()
/// 2. Anonymous requests get Isolated class limits
/// 3. Rate limiting triggers 429 Too Many Requests
#[actix_web::test]
async fn test_http_trust_rate_limit_middleware_integration() {
    use actix_web::{middleware, web, App, HttpResponse};

    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let trust_graph = Arc::new(RwLock::new(create_test_trust_graph(self_did.clone()).await));
    let trust_manager = create_trust_manager(trust_graph);

    // Create rate limiter with very low limits for testing (burst=2)
    let config = TrustRateLimitConfig {
        isolated_requests_per_sec: 2.0,
        isolated_burst: 2.0, // Only 2 requests allowed in burst
        known_requests_per_sec: 5.0,
        known_burst: 5.0,
        partner_requests_per_sec: 10.0,
        partner_burst: 10.0,
        federated_requests_per_sec: 20.0,
        federated_burst: 20.0,
    };
    let rate_limiter = Arc::new(TrustRateLimiter::new(trust_manager, config));

    // Create test app with trust rate limit middleware
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(rate_limiter.clone()))
            .route(
                "/test",
                web::get()
                    .to(|| async { HttpResponse::Ok().body("OK") })
                    .wrap(middleware::from_fn(
                        icn_gateway::rate_limit::trust_rate_limit_middleware,
                    )),
            ),
    )
    .await;

    // First request should succeed (burst capacity = 2)
    // Note: All anonymous requests share the same bucket (did:icn:anonymous:{ip})
    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .to_request();
    let resp = actix_web::test::try_call_service(&app, req).await;
    assert!(
        resp.is_ok(),
        "First request should succeed: {:?}",
        resp.err()
    );
    let resp = resp.unwrap();
    assert!(
        resp.status().is_success(),
        "First request status should be 2xx: {}",
        resp.status()
    );

    // Second request should succeed (still within burst)
    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .to_request();
    let resp = actix_web::test::try_call_service(&app, req).await;
    assert!(
        resp.is_ok(),
        "Second request should succeed: {:?}",
        resp.err()
    );
    let resp = resp.unwrap();
    assert!(
        resp.status().is_success(),
        "Second request status should be 2xx: {}",
        resp.status()
    );

    // Third request should fail (burst exhausted)
    // The middleware returns an error which try_call_service will capture
    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .to_request();
    let resp = actix_web::test::try_call_service(&app, req).await;

    // Rate limiting should cause an error (429 response)
    // Note: actix-web middleware errors are returned as Err, not as a response
    assert!(
        resp.is_err(),
        "Third request should be rate limited (got: {:?})",
        resp.ok().map(|r| r.status())
    );
}

/// Test that the middleware falls back gracefully when TrustRateLimiter is not configured.
///
/// When TrustRateLimiter is not in app_data, the middleware falls back to
/// regular rate_limit_middleware behavior. Since regular rate limiting
/// allows unauthenticated requests through, we verify requests succeed.
#[actix_web::test]
async fn test_http_trust_middleware_fallback_allows_requests() {
    use actix_web::{middleware, web, App, HttpResponse};
    use icn_gateway::rate_limit::{RateLimitConfig, RateLimiter};

    // Create regular rate limiter (used by fallback)
    // Note: Regular rate limiting skips unauthenticated requests
    let config = RateLimitConfig {
        capacity: 2.0,
        refill_rate: 1.0,
        cost_per_request: 1.0,
    };
    let rate_limiter = Arc::new(RateLimiter::new(config));

    // Create test app WITHOUT TrustRateLimiter but WITH regular RateLimiter
    // The trust_rate_limit_middleware should fall back to regular rate limiting
    // which allows unauthenticated requests through
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(rate_limiter.clone()))
            .route(
                "/test",
                web::get()
                    .to(|| async { HttpResponse::Ok().body("OK") })
                    .wrap(middleware::from_fn(
                        icn_gateway::rate_limit::trust_rate_limit_middleware,
                    )),
            ),
    )
    .await;

    // All requests should succeed since regular rate limiting
    // allows unauthenticated requests through (no TokenClaims)
    for i in 1..=5 {
        let req = actix_web::test::TestRequest::get()
            .uri("/test")
            .to_request();
        let resp = actix_web::test::try_call_service(&app, req).await;
        assert!(
            resp.is_ok(),
            "Request {} should succeed via fallback: {:?}",
            i,
            resp.err()
        );
        let resp = resp.unwrap();
        assert!(
            resp.status().is_success(),
            "Request {} status should be 2xx (fallback allows unauthenticated): {}",
            i,
            resp.status()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_trust_oracle_abstains_on_wrong_domain() {
    use icn_kernel_api::{ActionKind, Domain, PolicyDecision, PolicyRequest};

    // Create trust graph with self-DID
    let (_self_kp, self_did) = create_test_did("self");
    let trust_graph = Arc::new(RwLock::new(create_test_trust_graph(self_did.clone()).await));
    let trust_manager = create_trust_manager(trust_graph);
    let oracle = trust_manager.as_oracle();

    // Create request for "ledger" domain (not "trust")
    let request = PolicyRequest::new(
        "did:icn:test".to_string(),
        ActionKind::Read,
        Domain::new("ledger"),
    );

    let decision = oracle.evaluate(&request);

    // Should return Allow with empty constraints (Abstain)
    match decision {
        PolicyDecision::Allow { constraints } => {
            assert!(constraints.custom.is_empty(), "Custom constraints should be empty");
            assert!(constraints.rate_limit.is_none(), "Rate limit should be None");
        }
        _ => panic!("Should allow non-trust domains (abstain), got {:?}", decision),
    }
}
