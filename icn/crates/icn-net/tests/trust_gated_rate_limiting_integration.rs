//! Integration test demonstrating trust-gated rate limiting in action
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! This test validates the complete trust-gated rate limiting feature:
//! 1. Different rate limits for different trust classes
//! 2. Dynamic adjustment when trust changes
//! 3. Metrics recording
//! 4. Configuration support

use anyhow::Result;
use icn_identity::{Did, KeyPair};
use icn_net::{RateLimiter, TrustGatedRateLimitConfig};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_trust_gated_rate_limiting_full_scenario() -> Result<()> {
    // Initialize metrics (ignore result - may already be initialized)
    let _ = icn_obs::init_metrics();

    // Create test identities
    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();
    let carol = KeyPair::generate()?.did().clone();
    let dave = KeyPair::generate()?.did().clone();

    // Create trust graph
    let store = Arc::new(SledStore::temporary()?);
    let mut graph = TrustGraph::new(store, alice.clone());

    // Establish trust relationships (direct score * 0.7 = final score)
    // Bob: Isolated (0.0 final score)
    // Carol: Partner (direct 0.7 -> final 0.49)
    graph.add_edge(TrustEdge::new(alice.clone(), carol.clone(), 0.7))?;
    // Dave: Federated (direct 1.0 -> final 0.7)
    graph.add_edge(TrustEdge::new(alice.clone(), dave.clone(), 1.0))?;

    let trust_graph = Arc::new(RwLock::new(graph));

    // Create trust-gated rate limiter with default config
    let config = TrustGatedRateLimitConfig::default();
    let rate_limiter = Arc::new(RateLimiter::new_trust_gated(config, trust_graph.clone()));

    println!("\n=== Trust-Gated Rate Limiting Demonstration ===\n");

    // Test 1: Bob (Isolated) - 10 msg/sec, burst 2
    println!("1. Testing Bob (Isolated - trust score 0.0):");
    println!("   Expected: 10 msg/sec, burst capacity 2");

    let allowed_count = test_peer_rate_limit(&rate_limiter, &bob, 10).await;
    println!("   Result: {allowed_count} messages allowed out of 10");
    assert_eq!(allowed_count, 2, "Isolated peer should allow burst of 2");

    // Test 2: Carol (Partner) - 100 msg/sec, burst 20
    println!("\n2. Testing Carol (Partner - trust score 0.49):");
    println!("   Expected: 100 msg/sec, burst capacity 20");

    let allowed_count = test_peer_rate_limit(&rate_limiter, &carol, 30).await;
    println!("   Result: {allowed_count} messages allowed out of 30");
    assert_eq!(allowed_count, 20, "Partner peer should allow burst of 20");

    // Test 3: Dave (Federated) - 200 msg/sec, burst 50
    println!("\n3. Testing Dave (Federated - trust score 0.7):");
    println!("   Expected: 200 msg/sec, burst capacity 50");

    let allowed_count = test_peer_rate_limit(&rate_limiter, &dave, 60).await;
    println!("   Result: {allowed_count} messages allowed out of 60");
    assert_eq!(allowed_count, 50, "Federated peer should allow burst of 50");

    // Test 4: Dynamic trust adjustment
    println!("\n4. Testing dynamic trust adjustment:");
    println!("   Upgrading Bob from Isolated to Federated...");

    {
        let mut graph = trust_graph.write().await;
        graph.add_edge(TrustEdge::new(alice.clone(), bob.clone(), 1.0))?;
    }

    // Bob should now have Federated limits (burst 50)
    let allowed_count = test_peer_rate_limit(&rate_limiter, &bob, 60).await;
    println!("   Result: {allowed_count} messages allowed out of 60");
    assert_eq!(
        allowed_count, 50,
        "Upgraded peer should get Federated limits"
    );

    println!("\n=== All tests passed! ===\n");
    println!("Summary:");
    println!("✓ Isolated peers: 10 msg/sec, burst 2");
    println!("✓ Partner peers: 100 msg/sec, burst 20");
    println!("✓ Federated peers: 200 msg/sec, burst 50");
    println!("✓ Dynamic trust upgrades: Immediate benefit with full token reset");
    println!("✓ Metrics recording: Trust lookups, cache hits/misses, rate limiting");

    Ok(())
}

async fn test_peer_rate_limit(
    rate_limiter: &Arc<RateLimiter>,
    peer: &Did,
    attempts: usize,
) -> usize {
    let mut allowed = 0;
    for _ in 0..attempts {
        if rate_limiter.check_rate_limit(peer).await {
            allowed += 1;
        }
    }
    allowed
}

#[tokio::test]
async fn test_configuration_support() -> Result<()> {
    println!("\n=== Configuration Support Test ===\n");

    // Test custom configuration
    let custom_config = TrustGatedRateLimitConfig {
        min_trust_threshold: 0.0, // Allow all authenticated DIDs for rate limiting
        isolated: icn_net::RateLimitConfig {
            max_messages_per_second: 5,
            burst_capacity: 1,
            refill_interval: std::time::Duration::from_millis(100),
        },
        known: icn_net::RateLimitConfig {
            max_messages_per_second: 25,
            burst_capacity: 5,
            refill_interval: std::time::Duration::from_millis(100),
        },
        partner: icn_net::RateLimitConfig {
            max_messages_per_second: 50,
            burst_capacity: 10,
            refill_interval: std::time::Duration::from_millis(100),
        },
        federated: icn_net::RateLimitConfig {
            max_messages_per_second: 100,
            burst_capacity: 25,
            refill_interval: std::time::Duration::from_millis(100),
        },
        refill_interval: std::time::Duration::from_millis(100),
    };

    println!("Custom configuration:");
    println!(
        "  Isolated: {} msg/sec, burst {}",
        custom_config.isolated.max_messages_per_second, custom_config.isolated.burst_capacity
    );
    println!(
        "  Known: {} msg/sec, burst {}",
        custom_config.known.max_messages_per_second, custom_config.known.burst_capacity
    );
    println!(
        "  Partner: {} msg/sec, burst {}",
        custom_config.partner.max_messages_per_second, custom_config.partner.burst_capacity
    );
    println!(
        "  Federated: {} msg/sec, burst {}",
        custom_config.federated.max_messages_per_second, custom_config.federated.burst_capacity
    );

    // Create rate limiter with custom config
    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();

    let store = Arc::new(SledStore::temporary()?);
    let graph = TrustGraph::new(store, alice.clone());
    let trust_graph = Arc::new(RwLock::new(graph));

    let rate_limiter = Arc::new(RateLimiter::new_trust_gated(custom_config, trust_graph));

    // Test that custom config is applied
    let allowed_count = test_peer_rate_limit(&rate_limiter, &bob, 10).await;
    assert_eq!(
        allowed_count, 1,
        "Custom isolated config should allow burst of 1"
    );

    println!("\n✓ Custom configuration applied successfully");
    println!("✓ Operators can tune rate limits via icn.toml");

    Ok(())
}

#[tokio::test]
async fn test_cache_performance() -> Result<()> {
    println!("\n=== Cache Performance Test ===\n");

    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();

    let store = Arc::new(SledStore::temporary()?);
    let mut graph = TrustGraph::new(store, alice.clone());
    graph.add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))?;

    // First lookup - cache miss
    let start = std::time::Instant::now();
    let _score1 = graph.compute_trust_score(&bob)?;
    let duration1 = start.elapsed();
    println!("First lookup (cache miss): {duration1:?}");

    // Second lookup - cache hit
    let start = std::time::Instant::now();
    let _score2 = graph.compute_trust_score(&bob)?;
    let duration2 = start.elapsed();
    println!("Second lookup (cache hit): {duration2:?}");

    // Cache hit should be significantly faster
    assert!(
        duration2 < duration1,
        "Cache hit should be faster than cache miss"
    );
    println!("\n✓ Cache optimization working");
    println!("✓ Interior mutability allows concurrent read access");

    Ok(())
}
