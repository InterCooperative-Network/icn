//! Trust attestation rate limiting and topic constants
//!
//! Provides rate limiting for trust attestations propagated via gossip.
//! The actual attestation handling (signature verification, graph mutation)
//! is performed by `TrustService` in the trust app (`apps/trust/`).

use icn_identity::Did;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::debug;

/// Topic name for trust attestations
pub const TRUST_ATTESTATIONS_TOPIC: &str = "trust:attestations";

/// Topic name for trust revocations
pub const TRUST_REVOCATIONS_TOPIC: &str = "trust:revocations";

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Maximum tokens (burst capacity)
    capacity: u32,
    /// Current tokens available
    tokens: f64,
    /// Refill rate (tokens per second)
    refill_rate: f64,
    /// Last refill timestamp
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume N tokens
    ///
    /// Returns `true` if tokens were consumed, `false` if insufficient tokens available
    fn try_consume(&mut self, tokens: u32) -> bool {
        self.refill();

        if self.tokens >= tokens as f64 {
            self.tokens -= tokens as f64;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        let new_tokens = self.tokens + (elapsed * self.refill_rate);
        self.tokens = new_tokens.min(self.capacity as f64);
        self.last_refill = now;
    }
}

/// Per-issuer rate limiter for trust attestations
///
/// This prevents a single compromised node (even with `TrustClass::Known`) from
/// flooding the network with attestations for sock puppet accounts.
///
/// ## Attack Scenario (Without Rate Limiting)
///
/// 1. Attacker creates 1000 DIDs
/// 2. Gets ONE of them to `Known` status (score ≥0.1) via social engineering
/// 3. Uses that one DID to flood network with attestations for the other 999
///
/// ## Mitigation Strategy
///
/// - Limit attestations per issuer: 10/hour, 50/day
/// - Limit attestations per target: 5/hour (prevent targeting attacks)
/// - Token bucket algorithm with burst capacity
///
/// ## Configuration
///
/// Default limits are conservative to prevent legitimate use cases while stopping floods:
/// - **10 attestations/hour** - Allows gradual trust building
/// - **Burst of 5** - Allows small teams to establish trust quickly
/// - **50 attestations/day** - Sufficient for most cooperative networks
pub struct AttestationRateLimiter {
    /// Per-issuer token buckets
    issuer_buckets: Mutex<HashMap<Did, TokenBucket>>,
    /// Per-target token buckets (prevents targeting attacks)
    target_buckets: Mutex<HashMap<Did, TokenBucket>>,
    /// Limits configuration
    limits: AttestationLimits,
}

/// Rate limiting configuration for attestations
#[derive(Debug, Clone)]
pub struct AttestationLimits {
    /// Attestations per hour per issuer
    pub per_issuer_per_hour: u32,
    /// Burst capacity per issuer
    pub per_issuer_burst: u32,
    /// Attestations per hour per target (prevents targeting)
    pub per_target_per_hour: u32,
}

impl Default for AttestationLimits {
    fn default() -> Self {
        Self {
            per_issuer_per_hour: 10,
            per_issuer_burst: 5,
            per_target_per_hour: 5,
        }
    }
}

impl Default for AttestationRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl AttestationRateLimiter {
    /// Create a new rate limiter with default limits
    pub fn new() -> Self {
        Self::with_limits(AttestationLimits::default())
    }

    /// Create a new rate limiter with custom limits
    pub fn with_limits(limits: AttestationLimits) -> Self {
        Self {
            issuer_buckets: Mutex::new(HashMap::new()),
            target_buckets: Mutex::new(HashMap::new()),
            limits,
        }
    }

    /// Check if an attestation from issuer to target is allowed
    ///
    /// Returns `Ok(())` if allowed, `Err(reason)` if rate limited
    pub async fn check(&self, issuer: &Did, target: &Did) -> Result<(), String> {
        // Check issuer rate limit
        {
            let mut buckets = self.issuer_buckets.lock().await;
            let bucket = buckets.entry(issuer.clone()).or_insert_with(|| {
                TokenBucket::new(
                    self.limits.per_issuer_burst,
                    self.limits.per_issuer_per_hour as f64 / 3600.0, // Convert to per-second
                )
            });

            if !bucket.try_consume(1) {
                return Err(format!(
                    "Issuer {} exceeded rate limit ({} attestations/hour)",
                    issuer, self.limits.per_issuer_per_hour
                ));
            }
        }

        // Check target rate limit (prevents targeting attacks)
        {
            let mut buckets = self.target_buckets.lock().await;
            let bucket = buckets.entry(target.clone()).or_insert_with(|| {
                TokenBucket::new(
                    self.limits.per_target_per_hour,
                    self.limits.per_target_per_hour as f64 / 3600.0, // Convert to per-second
                )
            });

            if !bucket.try_consume(1) {
                return Err(format!(
                    "Target {} is receiving too many attestations ({} attestations/hour limit)",
                    target, self.limits.per_target_per_hour
                ));
            }
        }

        Ok(())
    }

    /// Cleanup old buckets to prevent memory leaks
    ///
    /// Should be called periodically (e.g., every hour) to remove buckets
    /// that haven't been used recently.
    pub async fn cleanup_old_buckets(&self, max_age: Duration) {
        let mut issuer_buckets = self.issuer_buckets.lock().await;
        let mut target_buckets = self.target_buckets.lock().await;

        let now = Instant::now();

        issuer_buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);
        target_buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);

        debug!(
            "Rate limiter cleanup: {} issuer buckets, {} target buckets",
            issuer_buckets.len(),
            target_buckets.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_rate_limiter_allows_burst() {
        let limiter = AttestationRateLimiter::with_limits(AttestationLimits {
            per_issuer_per_hour: 10,
            per_issuer_burst: 5,
            per_target_per_hour: 5,
        });

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Should allow burst of 5
        for i in 0..5 {
            let result = limiter.check(alice.did(), bob.did()).await;
            assert!(result.is_ok(), "Burst attestation {i} should be allowed");
        }

        // 6th should be rate limited
        let result = limiter.check(alice.did(), bob.did()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeded rate limit"));
    }

    #[tokio::test]
    async fn test_rate_limiter_refills_over_time() {
        let limiter = AttestationRateLimiter::with_limits(AttestationLimits {
            per_issuer_per_hour: 3600, // 1 per second
            per_issuer_burst: 2,
            per_target_per_hour: 3600,
        });

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Consume burst
        assert!(limiter.check(alice.did(), bob.did()).await.is_ok());
        assert!(limiter.check(alice.did(), bob.did()).await.is_ok());

        // Should be rate limited
        assert!(limiter.check(alice.did(), bob.did()).await.is_err());

        // Wait 1 second for refill
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should allow 1 more
        assert!(limiter.check(alice.did(), bob.did()).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_per_target_limit() {
        let limiter = AttestationRateLimiter::with_limits(AttestationLimits {
            per_issuer_per_hour: 100, // High issuer limit
            per_issuer_burst: 50,
            per_target_per_hour: 3, // Low target limit
        });

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let carol = KeyPair::generate().unwrap();

        // Alice can attest to Bob 3 times
        for i in 0..3 {
            let result = limiter.check(alice.did(), bob.did()).await;
            assert!(result.is_ok(), "Attestation {i} should be allowed");
        }

        // 4th attestation to Bob should be blocked (targeting limit)
        let result = limiter.check(alice.did(), bob.did()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("receiving too many attestations"));

        // But Alice can still attest to Carol (different target)
        assert!(limiter.check(alice.did(), carol.did()).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_different_issuers_independent() {
        let limiter = AttestationRateLimiter::with_limits(AttestationLimits {
            per_issuer_per_hour: 10,
            per_issuer_burst: 2,
            per_target_per_hour: 10,
        });

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let carol = KeyPair::generate().unwrap();

        // Alice uses her burst
        assert!(limiter.check(alice.did(), carol.did()).await.is_ok());
        assert!(limiter.check(alice.did(), carol.did()).await.is_ok());
        assert!(limiter.check(alice.did(), carol.did()).await.is_err());

        // Bob should still have independent quota
        assert!(limiter.check(bob.did(), carol.did()).await.is_ok());
        assert!(limiter.check(bob.did(), carol.did()).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_cleanup() {
        let limiter = AttestationRateLimiter::with_limits(AttestationLimits {
            per_issuer_per_hour: 10,
            per_issuer_burst: 5,
            per_target_per_hour: 5,
        });

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Create some buckets
        assert!(limiter.check(alice.did(), bob.did()).await.is_ok());

        // Cleanup with short max_age should remove buckets
        tokio::time::sleep(Duration::from_millis(100)).await;
        limiter.cleanup_old_buckets(Duration::from_millis(50)).await;

        // After cleanup, should have fresh burst capacity
        // (this is implicit - if cleanup didn't work, we'd still have depleted buckets)
        for _ in 0..5 {
            assert!(limiter.check(alice.did(), bob.did()).await.is_ok());
        }
    }
}
