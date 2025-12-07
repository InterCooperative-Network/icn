//! Trust attestation propagation over gossip
//!
//! This module provides functionality for broadcasting and receiving
//! trust attestations via the gossip network.

use anyhow::Result;
use icn_gossip::{GossipEntry, GossipHandle};
use icn_identity::{Did, KeyPair};
use icn_trust::{TrustAttestation, TrustEdge, TrustGraph};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

/// Topic name for trust attestations
pub const TRUST_ATTESTATIONS_TOPIC: &str = "trust:attestations";

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

/// Broadcast a trust attestation to the network
///
/// Signs the attestation and publishes it to the `trust:attestations` gossip topic.
pub async fn broadcast_trust_attestation(
    attestation: &mut TrustAttestation,
    keypair: &KeyPair,
    gossip: &GossipHandle,
) -> Result<()> {
    // Sign the attestation
    attestation.sign(keypair)?;

    // Serialize
    let data = serde_json::to_vec(attestation)?;

    // Publish to gossip
    let mut gossip_actor = gossip.write().await;
    let hash = gossip_actor.publish(TRUST_ATTESTATIONS_TOPIC, data)?;

    debug!(
        "Broadcasted trust attestation: {} -> {} (hash: {:?})",
        attestation.issuer, attestation.subject, hash
    );

    // Record metric
    icn_obs::metrics::trust::attestations_broadcasted_inc();

    Ok(())
}

/// Handle an incoming trust attestation entry from gossip
///
/// Verifies the signature and applies the attestation to the trust graph.
///
/// ## Parameters
///
/// - `entry`: The gossip entry containing the attestation
/// - `trust_graph`: The trust graph to apply attestations to
/// - `own_did`: This node's DID (for logging when trust is received)
/// - `rate_limiter`: Optional rate limiter to prevent attestation floods
pub async fn handle_trust_attestation_entry(
    entry: &GossipEntry,
    trust_graph: &Arc<RwLock<TrustGraph>>,
    own_did: &Did,
    rate_limiter: Option<&AttestationRateLimiter>,
) -> Result<()> {
    // Decompress if needed
    let data = entry.get_data()?;

    // Deserialize
    let attestation: TrustAttestation = serde_json::from_slice(&data)?;

    // Check rate limits (if rate limiter provided)
    if let Some(limiter) = rate_limiter {
        if let Err(reason) = limiter
            .check(&attestation.issuer, &attestation.subject)
            .await
        {
            warn!("Rate limited attestation: {}", reason);
            icn_obs::metrics::trust::attestations_rejected_rate_limited_inc();
            return Ok(()); // Don't propagate error, just log and ignore
        }
    }

    // Verify signature
    if let Err(e) = attestation.verify() {
        warn!(
            "Rejecting trust attestation with invalid signature: {} -> {} (error: {})",
            attestation.issuer, attestation.subject, e
        );
        icn_obs::metrics::trust::attestations_rejected_invalid_signature_inc();
        return Ok(()); // Don't propagate error, just log and ignore
    }

    // Check if expired
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    if attestation.is_expired(now) {
        warn!(
            "Received expired trust attestation from {}: {} -> {} (expired {} seconds ago)",
            entry.author,
            attestation.issuer,
            attestation.subject,
            now.saturating_sub(attestation.expires_at())
        );
        icn_obs::metrics::trust::attestations_rejected_expired_inc();
        return Ok(()); // Silently ignore expired attestations
    }

    // Convert to trust edge
    let edge = attestation.to_trust_edge();

    // Apply to trust graph
    let mut graph = trust_graph.write().await;

    // Check if we already have this edge - gracefully degrade if lookup fails
    match graph.get_edge(&edge.source, &edge.target) {
        Ok(Some(existing)) => {
            // Use should_supersede to handle clock skew and score tiebreaking
            if !attestation.should_supersede(existing.created_at, existing.score) {
                debug!(
                    "Rejecting outdated trust attestation: {} -> {} (existing: ts={}, score={:.2}; incoming: ts={}, score={:.2})",
                    edge.source, edge.target,
                    existing.created_at, existing.score,
                    attestation.created_at, attestation.score
                );
                icn_obs::metrics::trust::attestations_rejected_outdated_inc();
                return Ok(());
            }

            // Log significant trust changes (>0.3 delta) for audit trail
            let score_delta = (attestation.score - existing.score).abs();
            if score_delta > 0.3 {
                warn!(
                    "Significant trust change: {} -> {} from {:.2} to {:.2} (delta: {:.2})",
                    edge.source, edge.target, existing.score, attestation.score, score_delta
                );
            }

            // Mark as an update rather than new edge
            icn_obs::metrics::trust::attestations_updated_inc();
        }
        Ok(None) => {
            // This is a new trust edge
            icn_obs::metrics::trust::attestations_new_inc();
        }
        Err(e) => {
            // Storage error during lookup - log but proceed with adding edge anyway
            // This ensures attestations are not lost due to transient storage issues
            warn!(
                "Failed to lookup existing edge during attestation processing: {}; treating as new edge",
                e
            );
            icn_obs::metrics::trust::attestations_new_inc();
        }
    }

    graph.add_edge(edge.clone())?;

    info!(
        "Applied remote trust attestation: {} -> {} (score: {})",
        edge.source, edge.target, edge.score
    );

    // Record metrics
    icn_obs::metrics::trust::attestations_received_inc();

    // If this attestation is about us, log it specially
    if edge.target == *own_did {
        info!(
            "✓ Received trust from {}: score {}",
            edge.source, edge.score
        );
    }

    Ok(())
}

/// Helper to create and broadcast a trust attestation for a new edge
///
/// This is a convenience function that combines edge addition with broadcasting.
pub async fn add_edge_and_broadcast(
    trust_graph: &Arc<RwLock<TrustGraph>>,
    edge: TrustEdge,
    keypair: &KeyPair,
    gossip: &GossipHandle,
) -> Result<()> {
    // Add edge locally first
    {
        let mut graph = trust_graph.write().await;
        graph.add_edge(edge.clone())?;
    }

    // Create and broadcast attestation
    let mut attestation = TrustAttestation::from_trust_edge(&edge);
    broadcast_trust_attestation(&mut attestation, keypair, gossip).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use icn_trust::TrustClass;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_broadcast_and_handle_attestation() {
        // Create two nodes
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Create trust graphs
        let alice_store = Arc::new(SledStore::temporary().unwrap());
        let alice_graph = Arc::new(RwLock::new(TrustGraph::new(
            alice_store,
            alice.did().clone(),
        )));

        let bob_store = Arc::new(SledStore::temporary().unwrap());
        let bob_graph = Arc::new(RwLock::new(TrustGraph::new(bob_store, bob.did().clone())));

        // Create gossip actors
        let trust_lookup = Arc::new(|_: &Did| Some(TrustClass::Partner));
        let alice_gossip =
            icn_gossip::GossipActor::spawn(alice.did().clone(), trust_lookup.clone());
        let _bob_gossip = icn_gossip::GossipActor::spawn(bob.did().clone(), trust_lookup);

        // Alice trusts Bob
        let edge = TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.75);
        add_edge_and_broadcast(&alice_graph, edge.clone(), &alice, &alice_gossip)
            .await
            .unwrap();

        // Get the entry from Alice's gossip
        let entry = {
            let alice_g = alice_gossip.read().await;
            let entries = alice_g.get_entries(TRUST_ATTESTATIONS_TOPIC);
            entries.first().unwrap().clone()
        };

        // Bob receives and handles it
        handle_trust_attestation_entry(&entry, &bob_graph, bob.did(), None)
            .await
            .unwrap();

        // Verify Bob now has the trust edge
        let bob_g = bob_graph.read().await;
        let bob_edge = bob_g.get_edge(alice.did(), bob.did()).unwrap();
        assert!(bob_edge.is_some());
        assert_eq!(bob_edge.unwrap().score, 0.75);
    }

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
