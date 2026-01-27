//! Rate limiting for network messages
//!
//! Implements a token bucket algorithm for per-peer rate limiting to prevent DoS attacks.
//! Supports policy-based rate limiting where limits are determined by PolicyOracle.
//!
//! ## Sybil Resistance (Issue #675)
//!
//! In addition to per-DID rate limiting, this module supports per-anchor (per-person)
//! rate limiting to prevent Sybil attacks where an attacker creates multiple DIDs
//! to bypass aggregate rate limits. When enabled, all DIDs belonging to the same
//! PersonhoodAnchor share a single rate limit bucket.

use icn_identity::{Did, PersonhoodStoreTrait};
use icn_kernel_api::authz::{
    ActionKind, ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest,
};

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Wrapper for lazy hex encoding - only encodes when Display is called.
///
/// This avoids computing hex strings when log levels would filter out the message.
struct HexDisplay<'a>(&'a [u8; 32]);

impl fmt::Display for HexDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

const NETWORK_MESSAGE_ACTION: &str = "network_message";
const NETWORK_DOMAIN: &str = "net";

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

/// Enforcement mode for per-person (per-anchor) rate limiting
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum EnforcementMode {
    /// Log violations but allow messages through (for gradual rollout)
    LogOnly,
    /// Reject messages that exceed the per-person limit
    #[default]
    Enforce,
    /// Reject ALL messages from DIDs without personhood anchors
    RequirePersonhood,
}

impl EnforcementMode {
    /// Parse from string (for config file)
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "log_only" | "logonly" => EnforcementMode::LogOnly,
            "require_personhood" | "requirepersonhood" => EnforcementMode::RequirePersonhood,
            _ => EnforcementMode::Enforce,
        }
    }
}

/// Configuration for per-anchor (per-person) rate limiting
///
/// This provides Sybil resistance by aggregating rate limits across all DIDs
/// belonging to the same PersonhoodAnchor.
#[derive(Clone, Debug)]
pub struct AnchorRateLimitConfig {
    /// Maximum messages per second per person (across all their DIDs)
    pub max_messages_per_person_per_second: u32,

    /// Bucket capacity for per-person rate limiting (allows bursts)
    pub person_burst_capacity: u32,

    /// How to handle rate limit violations
    pub enforcement_mode: EnforcementMode,

    /// Multiplier for verified personhood (e.g., 2.0 = verified persons get 2x limit)
    /// Applied when the anchor has POPLevel::Verified or higher
    pub verified_multiplier: f64,

    /// Refill interval (shared with per-DID)
    pub refill_interval: Duration,
}

impl Default for AnchorRateLimitConfig {
    fn default() -> Self {
        AnchorRateLimitConfig {
            max_messages_per_person_per_second: 500,
            person_burst_capacity: 100,
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 2.0,
            refill_interval: Duration::from_millis(100),
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

    /// Tolerance for floating point comparison (1e-6 is appropriate for user-configured values)
    const CONFIG_EPSILON: f64 = 1e-6;

    /// Update bucket configuration with proportional refill
    fn update_config(&mut self, new_capacity: f64, new_refill_rate: f64) -> bool {
        // Check if config changed using appropriate tolerance
        let changed = (self.capacity - new_capacity).abs() > Self::CONFIG_EPSILON
            || (self.refill_rate - new_refill_rate).abs() > Self::CONFIG_EPSILON;

        if changed {
            // Proportional refill: scale tokens by the capacity ratio to prevent
            // exploitation via rapid trust cycling (repeatedly getting full buckets)
            let ratio = new_capacity / self.capacity.max(1.0);
            self.tokens = (self.tokens * ratio).min(new_capacity);
            self.capacity = new_capacity;
            self.refill_rate = new_refill_rate;
            self.last_refill = Instant::now();
        }
        changed
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
///
/// Supports two layers of rate limiting:
/// 1. Per-DID: Policy-based rate limiting via PolicyOracle
/// 2. Per-anchor (Sybil resistance): Aggregate limits across DIDs sharing same PersonhoodAnchor
pub struct RateLimiter {
    /// Policy oracle for determining rate limits (optional)
    oracle: Option<Arc<dyn PolicyOracle>>,

    /// Fallback configuration (used when oracle is not provided)
    fallback_config: RateLimitConfig,

    /// Token buckets per peer DID
    buckets: Arc<RwLock<HashMap<Did, TokenBucket>>>,

    // --- Sybil resistance (Issue #675) ---
    /// Per-anchor (per-person) token buckets for Sybil resistance
    anchor_buckets: Arc<RwLock<HashMap<[u8; 32], TokenBucket>>>,

    /// Personhood store for DID-to-anchor lookups
    personhood_store: Option<Arc<dyn PersonhoodStoreTrait>>,

    /// Per-anchor rate limit configuration
    anchor_rate_config: Option<AnchorRateLimitConfig>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given fallback configuration
    pub fn new(config: RateLimitConfig) -> Self {
        RateLimiter {
            oracle: None,
            fallback_config: config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: None,
            anchor_rate_config: None,
        }
    }

    /// Create a new rate limiter with PolicyOracle
    ///
    /// The oracle is consulted on each message to determine the appropriate rate limit.
    /// If the oracle returns `PolicyDecision::Allow`, rate limits from the constraints
    /// are applied. If it returns `PolicyDecision::Deny`, messages are immediately rejected.
    ///
    /// The `fallback_config` is used when constraint extraction fails or when constraints
    /// don't specify a rate limit. This provides sensible defaults while still allowing
    /// the oracle to make authorization decisions.
    pub fn new_with_oracle(
        oracle: Arc<dyn PolicyOracle>,
        fallback_config: RateLimitConfig,
    ) -> Self {
        RateLimiter {
            oracle: Some(oracle),
            fallback_config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: None,
            anchor_rate_config: None,
        }
    }

    /// Create a new rate limiter with PolicyOracle and Sybil resistance
    ///
    /// This constructor enables per-anchor (per-person) rate limiting in addition
    /// to per-DID rate limiting, preventing Sybil attacks where an attacker
    /// creates multiple DIDs to bypass aggregate rate limits.
    pub fn new_with_oracle_and_sybil_resistance(
        oracle: Arc<dyn PolicyOracle>,
        fallback_config: RateLimitConfig,
        personhood_store: Arc<dyn PersonhoodStoreTrait>,
        anchor_config: AnchorRateLimitConfig,
    ) -> Self {
        RateLimiter {
            oracle: Some(oracle),
            fallback_config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: Some(personhood_store),
            anchor_rate_config: Some(anchor_config),
        }
    }

    /// Convert an oracle decision into a rate limit config (or None if denied).
    fn rate_limit_from_decision(
        fallback_config: &RateLimitConfig,
        decision: PolicyDecision,
    ) -> Option<RateLimitConfig> {
        match decision {
            PolicyDecision::Allow { constraints } => Some(Self::rate_limit_from_constraints(
                fallback_config,
                &constraints,
            )),
            PolicyDecision::Deny { .. } => None,
        }
    }

    fn rate_limit_from_constraints(
        fallback_config: &RateLimitConfig,
        constraints: &ConstraintSet,
    ) -> RateLimitConfig {
        match &constraints.rate_limit {
            Some(rate_limit) => RateLimitConfig {
                max_messages_per_second: rate_limit.messages_per_second,
                burst_capacity: rate_limit.burst_size,
                refill_interval: fallback_config.refill_interval,
            },
            None => fallback_config.clone(),
        }
    }

    /// Enable Sybil resistance on an existing rate limiter
    pub fn with_sybil_resistance(
        mut self,
        personhood_store: Arc<dyn PersonhoodStoreTrait>,
        anchor_config: AnchorRateLimitConfig,
    ) -> Self {
        self.personhood_store = Some(personhood_store);
        self.anchor_rate_config = Some(anchor_config);
        self
    }

    /// Check if a message from the given peer should be allowed.
    /// Returns true if allowed, false if rate limited.
    ///
    /// Uses PolicyOracle to determine rate limits based on trust/policy.
    ///
    /// ## Oracle Evaluation
    ///
    /// The oracle is consulted on each message to get the current policy decision:
    /// - `PolicyDecision::Allow { constraints }`: Extract rate limit from constraints
    /// - `PolicyDecision::Deny { .. }`: Return false immediately (message rejected)
    ///
    /// ## Deny Decision Semantics
    ///
    /// When the oracle returns `Deny`, no token bucket is created for the peer.
    /// This means subsequent messages from denied peers will re-query the oracle
    /// each time. This is intentional:
    /// - Allows oracle to change its decision if circumstances change
    /// - Avoids stale cached decisions for peers whose trust status improves
    /// - Negligible overhead since denied peers should be rare in healthy networks
    ///
    /// If oracle call overhead becomes a concern, consider adding a TTL cache
    /// for Deny decisions in the future.
    pub async fn check_rate_limit(&self, peer: &Did) -> bool {
        // Get rate limit config from oracle or fallback
        let config = if let Some(oracle) = &self.oracle {
            let request = PolicyRequest::new(
                peer.to_string(),
                ActionKind::Custom(NETWORK_MESSAGE_ACTION.to_string()),
                Domain::new(NETWORK_DOMAIN),
            );

            match Self::rate_limit_from_decision(&self.fallback_config, oracle.evaluate(&request)) {
                Some(config) => config,
                None => return false,
            }
        } else {
            self.fallback_config.clone()
        };

        // LOCK ORDER INVARIANT: Always acquire `buckets` before `anchor_buckets`
        // to prevent deadlocks. Any code path that needs both locks must follow
        // this order. See also line 485 where anchor_buckets is acquired after
        // check_rate_limit completes (never holding buckets simultaneously).
        let mut buckets = self.buckets.write().await;

        // Perform rate limit check
        self.do_rate_limit_check(&mut buckets, peer, &config)
    }

    /// Internal helper to perform the actual rate limit check
    fn do_rate_limit_check(
        &self,
        buckets: &mut HashMap<Did, TokenBucket>,
        peer: &Did,
        config: &RateLimitConfig,
    ) -> bool {
        // Calculate refill rate: tokens per interval
        let refill_rate =
            (config.max_messages_per_second as f64 * config.refill_interval.as_secs_f64()).max(1.0);

        let capacity = config.burst_capacity as f64;
        let refill_interval = config.refill_interval;

        // Get or create bucket for this peer
        let bucket = buckets
            .entry(peer.clone())
            .or_insert_with(|| TokenBucket::new(capacity, refill_rate, refill_interval));

        // Update bucket config if it has changed
        let changed = bucket.update_config(capacity, refill_rate);
        if changed {
            icn_obs::metrics::network::rate_limit_config_changes_inc();
        }

        let allowed = bucket.try_consume();

        // Record rate limiting metric by rate value
        if !allowed {
            icn_obs::metrics::network::messages_rate_limited_by_rate_inc(
                config.max_messages_per_second,
            );
        }

        allowed
    }

    /// Check rate limit with Sybil resistance (per-person limiting)
    ///
    /// This method performs dual-path rate limiting:
    /// 1. Per-DID rate limit check (existing behavior)
    /// 2. Per-anchor (per-person) rate limit check (Sybil resistance)
    ///
    /// Both checks must pass for the message to be allowed.
    ///
    /// Returns `(did_allowed, anchor_allowed)`:
    /// - `(true, true)` = message allowed
    /// - `(false, _)` = rate limited by per-DID limit
    /// - `(_, false)` = rate limited by per-person limit (Sybil mitigation)
    pub async fn check_rate_limit_with_personhood(&self, peer: &Did) -> (bool, bool) {
        // Phase 1: Per-DID rate limit check (existing logic)
        let did_allowed = self.check_rate_limit(peer).await;

        // Phase 2: Per-anchor (per-person) rate limit check
        let anchor_allowed = self.check_anchor_rate_limit(peer).await;

        (did_allowed, anchor_allowed)
    }

    /// Internal helper to check per-anchor rate limit
    async fn check_anchor_rate_limit(&self, peer: &Did) -> bool {
        // If Sybil resistance is not enabled, allow everything
        let (personhood_store, anchor_config) =
            match (&self.personhood_store, &self.anchor_rate_config) {
                (Some(store), Some(config)) => (store, config),
                _ => return true, // Sybil resistance not enabled
            };

        // Look up the anchor ID for this DID
        let anchor_id = match personhood_store.get_anchor_id_for_did(peer) {
            Ok(Some(id)) => id,
            Ok(None) => {
                // No personhood anchor for this DID
                return match anchor_config.enforcement_mode {
                    EnforcementMode::RequirePersonhood => {
                        warn!(
                            did = %peer,
                            "Rejecting message: DID has no personhood anchor (RequirePersonhood mode)"
                        );
                        icn_obs::metrics::network::personhood_required_rejections_inc();
                        icn_obs::metrics::network::sybil_detection_inc(
                            icn_obs::metrics::network::SybilDetectionType::NoPersonhoodRequired,
                        );
                        false
                    }
                    EnforcementMode::LogOnly | EnforcementMode::Enforce => {
                        // DID has no anchor - fall back to per-DID limiting only
                        debug!(
                            did = %peer,
                            "DID has no personhood anchor, skipping per-person rate limit"
                        );
                        true
                    }
                };
            }
            Err(e) => {
                // Store lookup failed - graceful degradation
                // But increment metric so operators know Sybil protection is degraded
                warn!(
                    did = %peer,
                    error = %e,
                    "Failed to look up personhood anchor, falling back to per-DID limiting"
                );
                icn_obs::metrics::network::personhood_store_failures_inc();
                return true;
            }
        };

        // Check if we need to apply verified multiplier
        let (capacity, refill_rate) =
            self.get_anchor_bucket_params(peer, &anchor_id, anchor_config);

        // Acquire the anchor buckets lock
        let mut anchor_buckets = self.anchor_buckets.write().await;

        // Get or create bucket for this anchor
        let bucket = anchor_buckets.entry(anchor_id).or_insert_with(|| {
            TokenBucket::new(capacity, refill_rate, anchor_config.refill_interval)
        });

        let allowed = bucket.try_consume();

        // Update metrics
        icn_obs::metrics::network::personhood_anchors_active_set(anchor_buckets.len());

        if !allowed {
            // Per-person rate limit exceeded - this is Sybil mitigation
            icn_obs::metrics::network::messages_rate_limited_by_anchor_inc();
            icn_obs::metrics::network::sybil_detection_inc(
                icn_obs::metrics::network::SybilDetectionType::PerPersonLimit,
            );

            match anchor_config.enforcement_mode {
                EnforcementMode::LogOnly => {
                    warn!(
                        did = %peer,
                        anchor_id = %HexDisplay(&anchor_id),
                        "Per-person rate limit exceeded (LogOnly mode - allowing)"
                    );
                    return true; // Allow in LogOnly mode
                }
                EnforcementMode::Enforce | EnforcementMode::RequirePersonhood => {
                    debug!(
                        did = %peer,
                        anchor_id = %HexDisplay(&anchor_id),
                        "Per-person rate limit exceeded"
                    );
                    return false;
                }
            }
        }

        true
    }

    /// Calculate bucket parameters, applying verified multiplier if applicable
    fn get_anchor_bucket_params(
        &self,
        peer: &Did,
        anchor_id: &[u8; 32],
        config: &AnchorRateLimitConfig,
    ) -> (f64, f64) {
        let mut multiplier = 1.0;

        // Check if the anchor has verified personhood (for multiplier)
        if config.verified_multiplier > 1.0 {
            if let Some(store) = &self.personhood_store {
                if let Ok(Some(anchor)) = store.get_anchor(anchor_id) {
                    // Check if anchor meets verified POP level
                    if anchor.meets_pop_level(icn_identity::POPLevel::Verified) {
                        multiplier = config.verified_multiplier;
                        debug!(
                            did = %peer,
                            anchor_id = %HexDisplay(anchor_id),
                            multiplier = multiplier,
                            "Applying verified personhood multiplier"
                        );
                    }
                }
            }
        }

        let capacity = config.person_burst_capacity as f64 * multiplier;
        let refill_rate = (config.max_messages_per_person_per_second as f64
            * config.refill_interval.as_secs_f64()
            * multiplier)
            .max(1.0);

        (capacity, refill_rate)
    }

    /// Clean up old buckets for peers that haven't sent messages recently
    /// (Call this periodically to prevent unbounded memory growth)
    pub async fn cleanup_old_buckets(&self, max_age: Duration) {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();

        buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);

        // Also clean up anchor buckets
        let mut anchor_buckets = self.anchor_buckets.write().await;
        anchor_buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);

        // Update metrics
        icn_obs::metrics::network::personhood_anchors_active_set(anchor_buckets.len());
    }

    /// Get the number of tracked peers
    pub async fn tracked_peers(&self) -> usize {
        self.buckets.read().await.len()
    }

    /// Get the number of tracked personhood anchors (for Sybil resistance)
    pub async fn tracked_anchors(&self) -> usize {
        self.anchor_buckets.read().await.len()
    }

    /// Check if Sybil resistance is enabled
    pub fn is_sybil_resistance_enabled(&self) -> bool {
        self.personhood_store.is_some() && self.anchor_rate_config.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_kernel_api::authz::{AllowAllOracle, ConstraintSet, DenyAllOracle, RateLimit};

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
    async fn test_rate_limiter_oracle_deny_blocks() {
        let oracle = Arc::new(DenyAllOracle::new(Domain::new(NETWORK_DOMAIN), "denied"));
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();
        assert!(!limiter.check_rate_limit(&peer).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_oracle_rate_limit() {
        struct TestOracle;
        impl PolicyOracle for TestOracle {
            fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
                PolicyDecision::allow_with(
                    ConstraintSet::new().with_rate_limit(RateLimit::new(5, 2)),
                )
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        let oracle = Arc::new(TestOracle);
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();

        for _ in 0..2 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await);
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
        struct PerPeerOracle {
            isolated_peer: Did,
            known_peer: Did,
            partner_peer: Did,
            federated_peer: Did,
        }

        impl PolicyOracle for PerPeerOracle {
            fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
                let limits = match request.actor().as_str() {
                    actor if actor == self.isolated_peer.as_str() => RateLimit::new(10, 2),
                    actor if actor == self.known_peer.as_str() => RateLimit::new(50, 10),
                    actor if actor == self.partner_peer.as_str() => RateLimit::new(100, 20),
                    actor if actor == self.federated_peer.as_str() => RateLimit::new(200, 50),
                    _ => RateLimit::new(10, 2),
                };
                PolicyDecision::allow_with(ConstraintSet::new().with_rate_limit(limits))
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        let isolated_peer = KeyPair::generate().unwrap().did().clone();
        let known_peer = KeyPair::generate().unwrap().did().clone();
        let partner_peer = KeyPair::generate().unwrap().did().clone();
        let federated_peer = KeyPair::generate().unwrap().did().clone();

        let oracle = Arc::new(PerPeerOracle {
            isolated_peer: isolated_peer.clone(),
            known_peer: known_peer.clone(),
            partner_peer: partner_peer.clone(),
            federated_peer: federated_peer.clone(),
        });
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());

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
        struct MutableOracle {
            first_limit: RateLimit,
            upgraded_limit: RateLimit,
            upgraded: std::sync::atomic::AtomicBool,
        }

        impl PolicyOracle for MutableOracle {
            fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
                let limit = if self.upgraded.load(std::sync::atomic::Ordering::SeqCst) {
                    self.upgraded_limit.clone()
                } else {
                    self.first_limit.clone()
                };
                PolicyDecision::allow_with(ConstraintSet::new().with_rate_limit(limit))
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        // RateLimit::new(messages_per_second, burst_size)
        // burst_size is used as the bucket capacity
        let oracle = Arc::new(MutableOracle {
            first_limit: RateLimit::new(10, 10),    // capacity=10
            upgraded_limit: RateLimit::new(50, 50), // capacity=50
            upgraded: std::sync::atomic::AtomicBool::new(false),
        });
        let limiter = RateLimiter::new_with_oracle(oracle.clone(), RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();

        // Consume 5 out of 10 tokens (leaving 5)
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(&peer).await);
        }

        // Upgrade trust - proportional refill scales tokens: 5 * (50/10) = 25
        oracle
            .upgraded
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // After trust upgrade with proportional refill, we should have 25 tokens
        // (proportional: 5 remaining * (new_capacity / old_capacity) = 5 * 5 = 25)
        for _ in 0..25 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await); // Rate limited at new threshold
    }

    #[tokio::test]
    async fn test_trust_gated_config_for_class() {
        struct SimpleOracle;

        impl PolicyOracle for SimpleOracle {
            fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
                PolicyDecision::allow_with(
                    ConstraintSet::new().with_rate_limit(RateLimit::new(25, 5)),
                )
            }

            fn domain(&self) -> Domain {
                Domain::new(NETWORK_DOMAIN)
            }
        }

        let oracle = Arc::new(SimpleOracle);
        let limiter = RateLimiter::new_with_oracle(oracle, RateLimitConfig::default());
        let peer = KeyPair::generate().unwrap().did().clone();

        for _ in 0..5 {
            assert!(limiter.check_rate_limit(&peer).await);
        }
        assert!(!limiter.check_rate_limit(&peer).await);
    }

    // ========================================================================
    // Sybil Resistance Tests (Issue #675)
    // ========================================================================

    use icn_identity::{
        InMemoryPersonhoodStore, PersonhoodAnchor, PersonhoodAnchorStore, PersonhoodStoreTrait,
    };

    /// Create a test personhood store with anchors
    fn create_test_personhood_store() -> Arc<PersonhoodAnchorStore<InMemoryPersonhoodStore>> {
        let store = Arc::new(InMemoryPersonhoodStore::new());
        Arc::new(PersonhoodAnchorStore::new(store))
    }

    /// Test that per-person rate limiting works correctly for different people.
    ///
    /// This test creates 10 different people (10 anchors), each with their own DID.
    /// Each person should get their own per-anchor bucket, so 10 people × 5 burst
    /// = 50 total messages allowed.
    ///
    /// Note: This is NOT a Sybil attack test. See `test_sybil_attack_multiple_dids_one_anchor`
    /// for the actual Sybil resistance test.
    #[tokio::test]
    async fn test_per_person_rate_limiting() {
        let personhood_store = create_test_personhood_store();

        // Create 10 different people, each with their own anchor and DID
        let mut person_dids = Vec::new();

        for i in 0..10 {
            let anchor_key = [i as u8; 32];
            let anchor = PersonhoodAnchor::genesis(&format!("person_{i}"), anchor_key);
            personhood_store.store(&anchor).unwrap();
            // Each person uses their anchor's DID
            person_dids.push(anchor.to_did());
        }

        // Create rate limiter with Sybil resistance
        // Use high per-DID limits so per-anchor limits are the bottleneck
        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let fallback_config = RateLimitConfig {
            max_messages_per_second: 100,
            burst_capacity: 20, // High per-DID limit
            refill_interval: Duration::from_millis(100),
        };

        let anchor_config = AnchorRateLimitConfig {
            max_messages_per_person_per_second: 100,
            person_burst_capacity: 5, // Low limit per person - this is the bottleneck
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 1.0,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            fallback_config,
            personhood_store.clone() as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Each person (anchor) should be able to send up to their burst capacity
        // 10 people × 5 burst each = 50 total messages
        let mut total_allowed = 0;

        for did in &person_dids {
            // Each person tries to send 10 messages
            for _ in 0..10 {
                let (did_allowed, anchor_allowed) =
                    limiter.check_rate_limit_with_personhood(did).await;
                if did_allowed && anchor_allowed {
                    total_allowed += 1;
                }
            }
        }

        // With per-person limits (burst=5) and 10 people: ~50 messages allowed
        // Each person gets 5 messages, rest are blocked by per-anchor limit
        assert!(
            (45..=55).contains(&total_allowed),
            "Expected ~50 messages allowed (10 people × 5 burst), got {total_allowed}"
        );

        // Verify we're tracking all 10 anchors
        assert_eq!(
            limiter.tracked_anchors().await,
            10,
            "Should track 10 anchors (one per person)"
        );
    }

    /// Test actual Sybil attack mitigation: multiple DIDs belonging to one person.
    ///
    /// A Sybil attacker creates multiple DIDs but they all resolve to the same
    /// PersonhoodAnchor. Without per-anchor limiting, they'd get N × burst capacity.
    /// With per-anchor limiting, all their DIDs share ONE bucket.
    #[tokio::test]
    async fn test_sybil_attack_multiple_dids_one_anchor() {
        let personhood_store = create_test_personhood_store();

        // Create ONE person (one anchor)
        let anchor_key = [42u8; 32];
        let anchor = PersonhoodAnchor::genesis("sybil_attacker", anchor_key);
        let anchor_id = anchor.anchor.id;
        personhood_store.store(&anchor).unwrap();

        // The attacker creates 10 different DIDs (simulating key rotation or multi-device)
        // but they all point to the same anchor (same person)
        let mut attacker_dids = vec![anchor.to_did()]; // Primary DID

        for _ in 0..9 {
            // Generate additional keypairs (simulating Sybil attack)
            let extra_keypair = KeyPair::generate().unwrap();
            let extra_did = extra_keypair.did().clone();
            // Link this DID to the same anchor
            personhood_store.link_did(&anchor_id, &extra_did).unwrap();
            attacker_dids.push(extra_did);
        }

        assert_eq!(
            attacker_dids.len(),
            10,
            "Should have 10 DIDs for the attacker"
        );

        // Create rate limiter with Sybil resistance
        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let fallback_config = RateLimitConfig {
            max_messages_per_second: 100,
            burst_capacity: 20, // High per-DID limit (would allow 200 total without anchor limiting)
            refill_interval: Duration::from_millis(100),
        };

        let anchor_config = AnchorRateLimitConfig {
            max_messages_per_person_per_second: 100,
            person_burst_capacity: 10, // Per-person limit: only 10 messages total
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 1.0,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            fallback_config,
            personhood_store.clone() as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Attacker tries to send messages from all their DIDs
        // Without Sybil protection: 10 DIDs × 20 burst = 200 messages
        // With Sybil protection: all DIDs share ONE anchor bucket = 10 messages total
        let mut total_allowed = 0;
        let mut anchor_rejections = 0;

        for did in &attacker_dids {
            for _ in 0..5 {
                let (did_allowed, anchor_allowed) =
                    limiter.check_rate_limit_with_personhood(did).await;
                if did_allowed && anchor_allowed {
                    total_allowed += 1;
                } else if did_allowed && !anchor_allowed {
                    // DID limit passed but anchor limit blocked - this is Sybil mitigation
                    anchor_rejections += 1;
                }
            }
        }

        // Key assertion: despite 10 DIDs, only ~10 messages should be allowed
        // (the per-person burst capacity), not 200 (10 × 20)
        assert!(
            (8..=12).contains(&total_allowed),
            "Expected ~10 messages allowed (per-person limit), got {total_allowed}. \
             Sybil resistance should limit aggregate throughput."
        );

        // Verify significant rejections due to anchor limit (Sybil mitigation)
        assert!(
            anchor_rejections > 30,
            "Expected many anchor-based rejections (Sybil mitigation), got {anchor_rejections}"
        );

        // Verify only ONE anchor is tracked (the attacker is one person)
        assert_eq!(
            limiter.tracked_anchors().await,
            1,
            "Should track only 1 anchor (all DIDs belong to same person)"
        );
    }

    #[tokio::test]
    async fn test_no_personhood_anchor_graceful_fallback() {
        let personhood_store = create_test_personhood_store();

        // Create a peer without any personhood anchor
        let peer_without_anchor = KeyPair::generate().unwrap().did().clone();

        // Create rate limiter with Sybil resistance in Enforce mode (not RequirePersonhood)
        let anchor_config = AnchorRateLimitConfig {
            enforcement_mode: EnforcementMode::Enforce,
            ..Default::default()
        };

        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            RateLimitConfig::default(),
            personhood_store as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Peer without anchor should still be allowed (falls back to per-DID limiting)
        let (did_allowed, anchor_allowed) = limiter
            .check_rate_limit_with_personhood(&peer_without_anchor)
            .await;

        assert!(did_allowed, "DID should be allowed by per-DID limit");
        assert!(
            anchor_allowed,
            "Anchor check should pass for DIDs without anchor (graceful fallback)"
        );
    }

    #[tokio::test]
    async fn test_require_personhood_mode() {
        let personhood_store = create_test_personhood_store();

        // Create a peer without any personhood anchor
        let peer_without_anchor = KeyPair::generate().unwrap().did().clone();

        // Create rate limiter with RequirePersonhood mode
        let anchor_config = AnchorRateLimitConfig {
            enforcement_mode: EnforcementMode::RequirePersonhood,
            ..Default::default()
        };

        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            RateLimitConfig::default(),
            personhood_store as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Peer without anchor should be rejected in RequirePersonhood mode
        let (did_allowed, anchor_allowed) = limiter
            .check_rate_limit_with_personhood(&peer_without_anchor)
            .await;

        assert!(did_allowed, "DID should be allowed by per-DID limit");
        assert!(
            !anchor_allowed,
            "Anchor check should fail for DIDs without anchor in RequirePersonhood mode"
        );
    }

    #[tokio::test]
    async fn test_log_only_mode_allows_excess() {
        let personhood_store = create_test_personhood_store();

        // Create anchor and use its DID
        let anchor_key = [1u8; 32];
        let anchor = PersonhoodAnchor::genesis("test", anchor_key);
        personhood_store.store(&anchor).unwrap();

        // Use the anchor's own DID (which is automatically indexed)
        let peer = anchor.to_did();

        // Create rate limiter with LogOnly mode and very low limit
        let anchor_config = AnchorRateLimitConfig {
            person_burst_capacity: 5,
            enforcement_mode: EnforcementMode::LogOnly,
            ..Default::default()
        };

        let oracle = Arc::new(AllowAllOracle::new(Domain::new(NETWORK_DOMAIN)));
        let limiter = RateLimiter::new_with_oracle_and_sybil_resistance(
            oracle,
            RateLimitConfig::default(),
            personhood_store as Arc<dyn PersonhoodStoreTrait>,
            anchor_config,
        );

        // Exhaust the per-person limit
        for _ in 0..5 {
            let (_, anchor_allowed) = limiter.check_rate_limit_with_personhood(&peer).await;
            assert!(anchor_allowed);
        }

        // Subsequent messages should still be allowed in LogOnly mode
        for _ in 0..5 {
            let (_, anchor_allowed) = limiter.check_rate_limit_with_personhood(&peer).await;
            assert!(
                anchor_allowed,
                "LogOnly mode should allow messages even after limit exceeded"
            );
        }
    }

    #[tokio::test]
    async fn test_sybil_resistance_not_enabled_passthrough() {
        let config = RateLimitConfig {
            max_messages_per_second: 10,
            burst_capacity: 5,
            refill_interval: Duration::from_millis(100),
        };

        // Create limiter WITHOUT Sybil resistance
        let limiter = RateLimiter::new(config);
        let peer = KeyPair::generate().unwrap().did().clone();

        assert!(
            !limiter.is_sybil_resistance_enabled(),
            "Sybil resistance should not be enabled"
        );

        // check_rate_limit_with_personhood should work (anchor check passes)
        let (did_allowed, anchor_allowed) = limiter.check_rate_limit_with_personhood(&peer).await;
        assert!(did_allowed);
        assert!(
            anchor_allowed,
            "Anchor check should pass when Sybil resistance is disabled"
        );
    }

    #[test]
    fn test_enforcement_mode_parsing() {
        assert_eq!(EnforcementMode::parse("log_only"), EnforcementMode::LogOnly);
        assert_eq!(EnforcementMode::parse("LogOnly"), EnforcementMode::LogOnly);
        assert_eq!(EnforcementMode::parse("enforce"), EnforcementMode::Enforce);
        assert_eq!(EnforcementMode::parse("ENFORCE"), EnforcementMode::Enforce);
        assert_eq!(
            EnforcementMode::parse("require_personhood"),
            EnforcementMode::RequirePersonhood
        );
        assert_eq!(
            EnforcementMode::parse("requirepersonhood"),
            EnforcementMode::RequirePersonhood
        );
        assert_eq!(EnforcementMode::parse("unknown"), EnforcementMode::Enforce);
        // Default
    }
}
