//! Rate limiting for network messages
//!
//! Implements a token bucket algorithm for per-peer rate limiting to prevent DoS attacks.
//! Supports trust-gated rate limiting where different limits apply based on peer trust level.
//!
//! ## Sybil Resistance (Issue #675)
//!
//! In addition to per-DID rate limiting, this module supports per-anchor (per-person)
//! rate limiting to prevent Sybil attacks where an attacker creates multiple DIDs
//! to bypass aggregate rate limits. When enabled, all DIDs belonging to the same
//! PersonhoodAnchor share a single rate limit bucket.

use icn_identity::{Did, PersonhoodStoreTrait};
use icn_trust::TrustClass;
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
///
/// Supports three layers of rate limiting:
/// 1. Per-DID: Basic rate limiting per identity
/// 2. Trust-gated: Different limits based on peer trust class
/// 3. Per-anchor (Sybil resistance): Aggregate limits across DIDs sharing same PersonhoodAnchor
pub struct RateLimiter {
    /// Trust-gated configuration (if enabled)
    trust_gated_config: Option<TrustGatedRateLimitConfig>,

    /// Fallback configuration (used when trust-gated is disabled)
    fallback_config: RateLimitConfig,

    /// Token buckets per peer DID
    buckets: Arc<RwLock<HashMap<Did, TokenBucket>>>,

    /// Trust graph for looking up peer trust classes (optional)
    trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,

    // --- Sybil resistance (Issue #675) ---
    /// Per-anchor (per-person) token buckets for Sybil resistance
    anchor_buckets: Arc<RwLock<HashMap<[u8; 32], TokenBucket>>>,

    /// Personhood store for DID-to-anchor lookups
    personhood_store: Option<Arc<dyn PersonhoodStoreTrait>>,

    /// Per-anchor rate limit configuration
    anchor_rate_config: Option<AnchorRateLimitConfig>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration (no trust-gating)
    pub fn new(config: RateLimitConfig) -> Self {
        RateLimiter {
            trust_gated_config: None,
            fallback_config: config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            trust_graph: None,
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: None,
            anchor_rate_config: None,
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
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: None,
            anchor_rate_config: None,
        }
    }

    /// Create a new trust-gated rate limiter with Sybil resistance
    ///
    /// This constructor enables per-anchor (per-person) rate limiting in addition
    /// to per-DID rate limiting, preventing Sybil attacks where an attacker
    /// creates multiple DIDs to bypass aggregate rate limits.
    pub fn new_with_sybil_resistance(
        trust_gated_config: TrustGatedRateLimitConfig,
        trust_graph: Arc<RwLock<icn_trust::TrustGraph>>,
        personhood_store: Arc<dyn PersonhoodStoreTrait>,
        anchor_config: AnchorRateLimitConfig,
    ) -> Self {
        RateLimiter {
            trust_gated_config: Some(trust_gated_config.clone()),
            fallback_config: trust_gated_config.isolated.clone(),
            buckets: Arc::new(RwLock::new(HashMap::new())),
            trust_graph: Some(trust_graph),
            anchor_buckets: Arc::new(RwLock::new(HashMap::new())),
            personhood_store: Some(personhood_store),
            anchor_rate_config: Some(anchor_config),
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
    /// This implementation uses an atomic update pattern to eliminate the TOCTOU
    /// window between trust class lookup and bucket update (Issue #426).
    pub async fn check_rate_limit(&self, peer: &Did) -> bool {
        const MAX_RETRIES: usize = 3;

        for attempt in 0..MAX_RETRIES {
            // Phase 1: Get current trust class (releases lock immediately)
            let (config, trust_class) = if let (Some(trust_gated_config), Some(trust_graph)) =
                (&self.trust_gated_config, &self.trust_graph)
            {
                let trust_class = {
                    let graph = trust_graph.read().await;
                    graph.trust_class(peer).unwrap_or(TrustClass::Isolated)
                };
                (trust_gated_config.for_class(trust_class), Some(trust_class))
            } else {
                // Fallback mode: no TOCTOU possible
                (&self.fallback_config, None)
            };

            // Phase 2: Acquire buckets lock
            let mut buckets = self.buckets.write().await;

            // Phase 3: Verify trust class hasn't changed (eliminates TOCTOU)
            //
            // SAFETY: This nested lock acquisition (trust_graph.read() while holding
            // buckets.write()) is safe because no code path in the codebase acquires
            // buckets.write() while holding trust_graph.write(). See issue #426.
            if let (Some(trust_gated_config), Some(trust_graph)) =
                (&self.trust_gated_config, &self.trust_graph)
            {
                let current_trust_class = {
                    let graph = trust_graph.read().await;
                    graph.trust_class(peer).unwrap_or(TrustClass::Isolated)
                };

                if Some(current_trust_class) != trust_class {
                    // TOCTOU detected - trust class changed between phases 1 and 2
                    icn_obs::metrics::network::trust_class_toctou_mismatches_inc();

                    if attempt < MAX_RETRIES - 1 {
                        // Release locks and retry with fresh trust class
                        drop(buckets);
                        // Brief yield to prevent tight spinning if trust class is thrashing
                        tokio::task::yield_now().await;
                        continue;
                    }

                    // Final attempt - use the verified current trust class
                    let verified_config = trust_gated_config.for_class(current_trust_class);
                    return self.do_rate_limit_check(
                        &mut buckets,
                        peer,
                        verified_config,
                        Some(current_trust_class),
                    );
                }
            }

            // Trust class is consistent - proceed with rate limit check
            return self.do_rate_limit_check(&mut buckets, peer, config, trust_class);
        }

        // Should never reach here, but if we do, use most restrictive limit
        false
    }

    /// Internal helper to perform the actual rate limit check
    fn do_rate_limit_check(
        &self,
        buckets: &mut HashMap<Did, TokenBucket>,
        peer: &Did,
        config: &RateLimitConfig,
        trust_class: Option<TrustClass>,
    ) -> bool {
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
                warn!(
                    did = %peer,
                    error = %e,
                    "Failed to look up personhood anchor, falling back to per-DID limiting"
                );
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
            TokenBucket::new(
                capacity,
                refill_rate,
                anchor_config.refill_interval,
                None, // Anchor buckets don't track trust class
            )
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
        use icn_trust::{TrustEdge, TrustGraph, TrustScore};

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
                TrustScore::unchecked(0.3),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                own_keypair.did().clone(),
                partner_peer.clone(),
                TrustScore::unchecked(0.7),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                own_keypair.did().clone(),
                federated_peer.clone(),
                TrustScore::unchecked(1.0),
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
        use icn_trust::{TrustEdge, TrustGraph, TrustScore};

        // Create temporary store and trust graph
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let own_did = own_keypair.did().clone();
        let mut graph = TrustGraph::new(store, own_did);

        let peer = KeyPair::generate().unwrap().did().clone();

        // Start with low trust (Known = burst 10)
        // Direct score 0.3 -> final 0.21 = Known class
        graph
            .add_edge(TrustEdge::new(own_keypair.did().clone(), peer.clone(), TrustScore::unchecked(0.3)))
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
                .add_edge(TrustEdge::new(own_keypair.did().clone(), peer.clone(), TrustScore::unchecked(1.0)))
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
        use icn_store::SledStore;
        use icn_trust::TrustGraph;

        // Create components
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let graph = TrustGraph::new(store, own_keypair.did().clone());
        let graph_handle = Arc::new(tokio::sync::RwLock::new(graph));

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
        let trust_config = TrustGatedRateLimitConfig {
            isolated: RateLimitConfig {
                max_messages_per_second: 100,
                burst_capacity: 20, // High per-DID limit
                refill_interval: Duration::from_millis(100),
            },
            ..Default::default()
        };

        let anchor_config = AnchorRateLimitConfig {
            max_messages_per_person_per_second: 100,
            person_burst_capacity: 5, // Low limit per person - this is the bottleneck
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 1.0,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new_with_sybil_resistance(
            trust_config,
            graph_handle,
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
        use icn_store::SledStore;
        use icn_trust::TrustGraph;

        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let graph = TrustGraph::new(store, own_keypair.did().clone());
        let graph_handle = Arc::new(tokio::sync::RwLock::new(graph));

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
        let trust_config = TrustGatedRateLimitConfig {
            isolated: RateLimitConfig {
                max_messages_per_second: 100,
                burst_capacity: 20, // High per-DID limit (would allow 200 total without anchor limiting)
                refill_interval: Duration::from_millis(100),
            },
            ..Default::default()
        };

        let anchor_config = AnchorRateLimitConfig {
            max_messages_per_person_per_second: 100,
            person_burst_capacity: 10, // Per-person limit: only 10 messages total
            enforcement_mode: EnforcementMode::Enforce,
            verified_multiplier: 1.0,
            refill_interval: Duration::from_millis(100),
        };

        let limiter = RateLimiter::new_with_sybil_resistance(
            trust_config,
            graph_handle,
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
        use icn_store::SledStore;
        use icn_trust::TrustGraph;

        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let graph = TrustGraph::new(store, own_keypair.did().clone());
        let graph_handle = Arc::new(tokio::sync::RwLock::new(graph));

        let personhood_store = create_test_personhood_store();

        // Create a peer without any personhood anchor
        let peer_without_anchor = KeyPair::generate().unwrap().did().clone();

        // Create rate limiter with Sybil resistance in Enforce mode (not RequirePersonhood)
        let anchor_config = AnchorRateLimitConfig {
            enforcement_mode: EnforcementMode::Enforce,
            ..Default::default()
        };

        let limiter = RateLimiter::new_with_sybil_resistance(
            TrustGatedRateLimitConfig::default(),
            graph_handle,
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
        use icn_store::SledStore;
        use icn_trust::TrustGraph;

        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let graph = TrustGraph::new(store, own_keypair.did().clone());
        let graph_handle = Arc::new(tokio::sync::RwLock::new(graph));

        let personhood_store = create_test_personhood_store();

        // Create a peer without any personhood anchor
        let peer_without_anchor = KeyPair::generate().unwrap().did().clone();

        // Create rate limiter with RequirePersonhood mode
        let anchor_config = AnchorRateLimitConfig {
            enforcement_mode: EnforcementMode::RequirePersonhood,
            ..Default::default()
        };

        let limiter = RateLimiter::new_with_sybil_resistance(
            TrustGatedRateLimitConfig::default(),
            graph_handle,
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
        use icn_store::SledStore;
        use icn_trust::TrustGraph;

        let store = Arc::new(SledStore::temporary().unwrap());
        let own_keypair = KeyPair::generate().unwrap();
        let graph = TrustGraph::new(store, own_keypair.did().clone());
        let graph_handle = Arc::new(tokio::sync::RwLock::new(graph));

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

        let limiter = RateLimiter::new_with_sybil_resistance(
            TrustGatedRateLimitConfig::default(),
            graph_handle,
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
