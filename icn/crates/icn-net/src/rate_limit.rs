//! Rate limiting for network messages
//!
//! Implements a token bucket algorithm for per-peer rate limiting to prevent DoS attacks.

use icn_identity::Did;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

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
            burst_capacity: 20,            // Allow bursts of 20 messages
            refill_interval: Duration::from_millis(100), // Refill every 100ms
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
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64, refill_interval: Duration) -> Self {
        TokenBucket {
            tokens: capacity, // Start with full bucket
            capacity,
            refill_rate,
            last_refill: Instant::now(),
            refill_interval,
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
    /// Configuration
    config: RateLimitConfig,

    /// Token buckets per peer DID
    buckets: Arc<RwLock<HashMap<Did, TokenBucket>>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration
    pub fn new(config: RateLimitConfig) -> Self {
        RateLimiter {
            config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a message from the given peer should be allowed.
    /// Returns true if allowed, false if rate limited.
    pub async fn check_rate_limit(&self, peer: &Did) -> bool {
        let mut buckets = self.buckets.write().await;

        // Get or create bucket for this peer
        let bucket = buckets.entry(peer.clone()).or_insert_with(|| {
            // Calculate refill rate: tokens per interval
            let refill_rate = (self.config.max_messages_per_second as f64
                * self.config.refill_interval.as_secs_f64())
            .max(1.0);

            TokenBucket::new(
                self.config.burst_capacity as f64,
                refill_rate,
                self.config.refill_interval,
            )
        });

        bucket.try_consume()
    }

    /// Clean up old buckets for peers that haven't sent messages recently
    /// (Call this periodically to prevent unbounded memory growth)
    pub async fn cleanup_old_buckets(&self, max_age: Duration) {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();

        buckets.retain(|_, bucket| {
            now.duration_since(bucket.last_refill) < max_age
        });
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
}
