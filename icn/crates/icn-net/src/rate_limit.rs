//! Rate limiting for network messages
//!
//! Implements a token bucket algorithm for per-peer rate limiting to prevent DoS attacks.
//! Supports trust-gated rate limiting where different limits apply based on peer trust level.

use icn_identity::Did;
use icn_trust::TrustClass;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// Helper to convert TrustClass to string for metrics
fn trust_class_to_str(class: TrustClass) -> &'static str {
    match class {
        TrustClass::Isolated => "isolated",
        TrustClass::Known => "known",
        TrustClass::Partner => "partner",
        TrustClass::Federated => "federated",
    }
}

/// Configuration for rate limiting
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum messages per second per peer
    pub max_messages_per_second: u32,

    /// Bucket capacity (allows bursts up to this many messages)
    pub burst_capacity: u32,

    /// How often to refill tokens (default: 100ms)
    pub refill_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            max_messages_per_second: 100, // 100 msgs/sec = reasonable for gossip
            burst_capacity: 20,           // Allow bursts of 20 messages
            refill_interval: Duration::from_millis(100), // Refill every 100ms
        }
    }
}

/// Trust-gated rate limiting configuration
/// Different limits apply based on peer trust classification
#[derive(Clone, Debug)]
pub struct TrustGatedRateLimitConfig {
    /// Limits for isolated peers (untrusted, score < 0.1)
    pub isolated: RateLimitConfig,

    /// Limits for known peers (limited trust, score 0.1-0.4)
    pub known: RateLimitConfig,

    /// Limits for partner peers (trusted, score 0.4-0.7)
    pub partner: RateLimitConfig,

    /// Limits for federated peers (highly trusted, score 0.7+)
    pub federated: RateLimitConfig,

    /// Refill interval (shared across all trust levels)
    pub refill_interval: Duration,

    /// Minimum trust score required for TLS connections (default: 0.0 = allow all authenticated DIDs)
    /// Peers with trust scores below this threshold will be rejected during TLS handshake
    pub min_trust_threshold: f64,
}

impl Default for TrustGatedRateLimitConfig {
    fn default() -> Self {
        let refill_interval = Duration::from_millis(100);

        TrustGatedRateLimitConfig {
            isolated: RateLimitConfig {
                max_messages_per_second: 10,
                burst_capacity: 2,
                refill_interval,
            },
            known: RateLimitConfig {
                max_messages_per_second: 50,
                burst_capacity: 10,
                refill_interval,
            },
            partner: RateLimitConfig {
                max_messages_per_second: 100,
                burst_capacity: 20,
                refill_interval,
            },
            federated: RateLimitConfig {
                max_messages_per_second: 200,
                burst_capacity: 50,
                refill_interval,
            },
            refill_interval,
            min_trust_threshold: 0.0, // Default: allow all authenticated DIDs
        }
    }
}

impl TrustGatedRateLimitConfig {
    /// Get the rate limit config for a specific trust class
    pub fn for_class(&self, class: TrustClass) -> &RateLimitConfig {
        match class {
            TrustClass::Isolated => &self.isolated,
            TrustClass::Known => &self.known,
            TrustClass::Partner => &self.partner,
            TrustClass::Federated => &self.federated,
        }
    }
}

/// Token bucket for a single peer
#[derive(Debug)]
struct TokenBucket {
    /// Current number of tokens
    tokens: f64,

    /// Maximum tokens (burst capacity)
    capacity: f64,

    /// Tokens added per refill
    refill_rate: f64,

    /// Last refill timestamp
    last_refill: Instant,

    /// Refill interval
    refill_interval: Duration,

    /// Current trust class (for detecting changes in trust-gated mode)
    trust_class: Option<TrustClass>,
}

impl TokenBucket {
    fn new(
        capacity: f64,
        refill_rate: f64,
        refill_interval: Duration,
        trust_class: Option<TrustClass>,
    ) -> Self {
        TokenBucket {
            tokens: capacity, // Start with full bucket
            capacity,
            refill_rate,
            last_refill: Instant::now(),
            refill_interval,
            trust_class,
        }
    }

    /// Update bucket configuration if trust class has changed
    fn update_config(
        &mut self,
        new_capacity: f64,
        new_refill_rate: f64,
        new_trust_class: Option<TrustClass>,
    ) -> bool {
        // Only update if trust class actually changed
        if self.trust_class != new_trust_class {
            self.capacity = new_capacity;
            self.refill_rate = new_refill_rate;
            self.trust_class = new_trust_class;
            // Reset to full capacity when trust class changes
            // This gives immediate benefit for trust upgrades
            self.tokens = new_capacity;
            self.last_refill = Instant::now();
            true // Changed
        } else {
            false // No change
        }
    }

    /// Try to consume a token. Returns true if allowed, false if rate limited.
    fn try_consume(&mut self) -> bool {
        // Refill tokens based on elapsed time
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false // Rate limited
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        // Calculate how many refill intervals have passed
        let intervals = elapsed.as_secs_f64() / self.refill_interval.as_secs_f64();

        if intervals >= 1.0 {
            // Add tokens proportional to intervals passed
            let tokens_to_add = intervals * self.refill_rate;
            self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
            self.last_refill = now;
        }
    }
}

/// Per-peer rate limiter using token bucket algorithm
pub struct RateLimiter {
    /// Trust-gated configuration (if enabled)
    trust_gated_config: Option<TrustGatedRateLimitConfig>,

    /// Fallback configuration (used when trust-gated is disabled)
    fallback_config: RateLimitConfig,

    /// Token buckets per peer DID
    buckets: Arc<RwLock<HashMap<Did, TokenBucket>>>,

    /// Trust graph for looking up peer trust classes (optional)
    trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration (no trust-gating)
    pub fn new(config: RateLimitConfig) -> Self {
        RateLimiter {
            trust_gated_config: None,
            fallback_config: config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            trust_graph: None,
        }
    }

    /// Create a new trust-gated rate limiter
    pub fn new_trust_gated(
        config: TrustGatedRateLimitConfig,
        trust_graph: Arc<RwLock<icn_trust::TrustGraph>>,
    ) -> Self {
        RateLimiter {
            trust_gated_config: Some(config.clone()),
            fallback_config: config.isolated.clone(), // Use most restrictive as fallback
            buckets: Arc::new(RwLock::new(HashMap::new())),
            trust_graph: Some(trust_graph),
        }
    }

    /// Check if a message from the given peer should be allowed.
    /// Returns true if allowed, false if rate limited.
    ///
    /// Note: There is a minor TOCTOU window where trust class could change between
    /// lookup and bucket update. This is acceptable - worst case is one message gets
    /// slightly wrong rate limit. Avoiding nested locks prevents deadlock risk.
    pub async fn check_rate_limit(&self, peer: &Did) -> bool {
        // Determine trust class FIRST (before acquiring buckets lock) to avoid
        // nested lock acquisition which could cause deadlock if other code
        // acquires trust_graph write lock while holding buckets lock.
        //
        // Minor TOCTOU: trust class could change between lookup and bucket update.
        // This is acceptable - at worst one message gets slightly wrong rate limit,
        // and update_config() will correct it on the next check.
        let (config, trust_class) = if let (Some(trust_gated_config), Some(trust_graph)) =
            (&self.trust_gated_config, &self.trust_graph)
        {
            // Trust-gated mode: look up peer's trust class
            let trust_class = {
                let graph = trust_graph.read().await;
                graph.trust_class(peer).unwrap_or(TrustClass::Isolated)
            };

            (trust_gated_config.for_class(trust_class), Some(trust_class))
        } else {
            // Fallback mode: use single config for all peers
            (&self.fallback_config, None)
        };

        // Now acquire write lock on buckets (no other locks held)
        let mut buckets = self.buckets.write().await;

        // Calculate refill rate: tokens per interval
        let refill_rate =
            (config.max_messages_per_second as f64 * config.refill_interval.as_secs_f64()).max(1.0);

        let capacity = config.burst_capacity as f64;
        let refill_interval = config.refill_interval;

        // Get or create bucket for this peer
        let bucket = buckets.entry(peer.clone()).or_insert_with(|| {
            TokenBucket::new(capacity, refill_rate, refill_interval, trust_class)
        });

        // Update bucket config if trust class has changed
        let changed = bucket.update_config(capacity, refill_rate, trust_class);
        if changed {
            icn_obs::metrics::network::trust_class_changes_inc();
        }

        let allowed = bucket.try_consume();

        // Record per-class rate limiting metric (general counter recorded in actor.rs)
        if !allowed {
            if let Some(class) = trust_class {
                icn_obs::metrics::network::messages_rate_limited_by_class_inc(trust_class_to_str(
                    class,
                ));
            }
        }

        allowed
    }

    /// Clean up old buckets for peers that haven't sent messages recently
    /// (Call this periodically to prevent unbounded memory growth)
    pub async fn cleanup_old_buckets(&self, max_age: Duration) {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();

        buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);
    }

    /// Get the number of tracked peers
    pub async fn tracked_peers(&self) -> usize {
        self.buckets.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let config = RateLimitConfig {
            max_messages_per_second: 10,
            burst_capacity: 5,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new(config);
        let peer = KeyPair::generate().unwrap().did().clone();

        // Should allow burst_capacity messages immediately
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(&peer).await);
        }

        // Next message should be rate limited
        assert!(!limiter.check_rate_limit(&peer).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_refills() {
        let config = RateLimitConfig {
            max_messages_per_second: 10,
            burst_capacity: 2,
            refill_interval: Duration::from_millis(50),
        };

        let limiter = RateLimiter::new(config);
        let peer = KeyPair::generate().unwrap().did().clone();

        // Consume all tokens
        assert!(limiter.check_rate_limit(&peer).await);
        assert!(limiter.check_rate_limit(&peer).await);
        assert!(!limiter.check_rate_limit(&peer).await);

        // Wait for refill
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be allowed again
        assert!(limiter.check_rate_limit(&peer).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_per_peer() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        let peer1 = KeyPair::generate().unwrap().did().clone();
        let peer2 = KeyPair::generate().unwrap().did().clone();

        // Consume all tokens for peer1
        for _ in 0..20 {
            limiter.check_rate_limit(&peer1).await;
        }
        assert!(!limiter.check_rate_limit(&peer1).await);

        // peer2 should still be allowed
        assert!(limiter.check_rate_limit(&peer2).await);
    }

    #[tokio::test]
    async fn test_cleanup_old_buckets() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        let peer1 = KeyPair::generate().unwrap().did().clone();
        let peer2 = KeyPair::generate().unwrap().did().clone();

        limiter.check_rate_limit(&peer1).await;
        limiter.check_rate_limit(&peer2).await;

        assert_eq!(limiter.tracked_peers().await, 2);

        // Clean up buckets older than 1ms (should clean peer1 and peer2)
        tokio::time::sleep(Duration::from_millis(10)).await;
        limiter.cleanup_old_buckets(Duration::from_millis(5)).await;

        assert_eq!(limiter.tracked_peers().await, 0);
    }

    #[tokio::test]
    async fn test_trust_gated_rate_limiting_different_classes() {
        use icn_store::SledStore;
        use icn_trust::{TrustEdge, TrustGraph};

        // Create temporary store and trust graph
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let own_did = own_keypair.did().clone();
        let mut graph = TrustGraph::new(store, own_did);

        // Create peers with different trust levels
        // Note: TrustGraph computes final score as 70% direct + 30% transitive
        // So we need to adjust direct scores to achieve desired final trust classes:
        let isolated_peer = KeyPair::generate().unwrap().did().clone(); // No trust edge = Isolated (final < 0.1)
        let known_peer = KeyPair::generate().unwrap().did().clone(); // Direct 0.3 -> final 0.21 = Known
        let partner_peer = KeyPair::generate().unwrap().did().clone(); // Direct 0.7 -> final 0.49 = Partner
        let federated_peer = KeyPair::generate().unwrap().did().clone(); // Direct 1.0 -> final 0.7 = Federated

        // Add trust edges with adjusted scores
        graph
            .add_edge(TrustEdge::new(
                own_keypair.did().clone(),
                known_peer.clone(),
                0.3,
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                own_keypair.did().clone(),
                partner_peer.clone(),
                0.7,
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                own_keypair.did().clone(),
                federated_peer.clone(),
                1.0,
            ))
            .unwrap();

        // Create trust-gated rate limiter
        let graph_handle = Arc::new(tokio::sync::RwLock::new(graph));
        let limiter =
            RateLimiter::new_trust_gated(TrustGatedRateLimitConfig::default(), graph_handle);

        // Isolated peer (burst 2) - should be rate limited after 2 messages
        assert!(limiter.check_rate_limit(&isolated_peer).await);
        assert!(limiter.check_rate_limit(&isolated_peer).await);
        assert!(!limiter.check_rate_limit(&isolated_peer).await); // Rate limited

        // Known peer (burst 10) - should be rate limited after 10 messages
        for _ in 0..10 {
            assert!(limiter.check_rate_limit(&known_peer).await);
        }
        assert!(!limiter.check_rate_limit(&known_peer).await); // Rate limited

        // Partner peer (burst 20) - should be rate limited after 20 messages
        for _ in 0..20 {
            assert!(limiter.check_rate_limit(&partner_peer).await);
        }
        assert!(!limiter.check_rate_limit(&partner_peer).await); // Rate limited

        // Federated peer (burst 50) - should be rate limited after 50 messages
        for _ in 0..50 {
            assert!(limiter.check_rate_limit(&federated_peer).await);
        }
        assert!(!limiter.check_rate_limit(&federated_peer).await); // Rate limited
    }

    #[tokio::test]
    async fn test_trust_gated_rate_limiting_trust_class_change() {
        use icn_store::SledStore;
        use icn_trust::{TrustEdge, TrustGraph};

        // Create temporary store and trust graph
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let own_did = own_keypair.did().clone();
        let mut graph = TrustGraph::new(store, own_did);

        let peer = KeyPair::generate().unwrap().did().clone();

        // Start with low trust (Known = burst 10)
        // Direct score 0.3 -> final 0.21 = Known class
        graph
            .add_edge(TrustEdge::new(own_keypair.did().clone(), peer.clone(), 0.3))
            .unwrap();

        // Create trust-gated rate limiter
        let graph_handle = Arc::new(tokio::sync::RwLock::new(graph));
        let limiter = RateLimiter::new_trust_gated(
            TrustGatedRateLimitConfig::default(),
            graph_handle.clone(),
        );

        // Consume all tokens for Known class (10)
        for _ in 0..10 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await); // Rate limited

        // Upgrade trust to Federated (burst 50)
        // Direct score 1.0 -> final 0.7 = Federated class
        {
            let mut graph = graph_handle.write().await;
            graph
                .add_edge(TrustEdge::new(own_keypair.did().clone(), peer.clone(), 1.0))
                .unwrap();
        }

        // After trust upgrade, should get more capacity
        // (Note: bucket is recreated with new capacity, starting fresh at 50 tokens)
        for _ in 0..50 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await); // Rate limited at new threshold
    }

    #[tokio::test]
    async fn test_trust_gated_config_for_class() {
        let config = TrustGatedRateLimitConfig::default();

        // Verify each trust class gets the right config
        assert_eq!(config.for_class(TrustClass::Isolated).burst_capacity, 2);
        assert_eq!(config.for_class(TrustClass::Known).burst_capacity, 10);
        assert_eq!(config.for_class(TrustClass::Partner).burst_capacity, 20);
        assert_eq!(config.for_class(TrustClass::Federated).burst_capacity, 50);

        assert_eq!(
            config
                .for_class(TrustClass::Isolated)
                .max_messages_per_second,
            10
        );
        assert_eq!(
            config.for_class(TrustClass::Known).max_messages_per_second,
            50
        );
        assert_eq!(
            config
                .for_class(TrustClass::Partner)
                .max_messages_per_second,
            100
        );
        assert_eq!(
            config
                .for_class(TrustClass::Federated)
                .max_messages_per_second,
            200
        );
    }
}
