//! Trust Policy Oracle
//!
//! Implements the PolicyOracle trait, converting trust semantics into
//! kernel-enforceable constraints.
//!
//! # The Meaning Firewall
//!
//! Everything above `score_to_constraints()` is trust semantics.
//! Everything below is generic kernel constraints.
//!
//! The kernel never knows:
//! - What a "trust score" is
//! - What "Isolated", "Known", "Partner", "Federated" mean
//! - How trust is computed
//!
//! It only knows:
//! - Rate limits to enforce
//! - Credit multipliers to apply
//! - Topic limits to check

use icn_kernel_api::authz::{
    ActionKind, ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest, RateLimit,
};
use icn_trust::{ScopeId, TrustGraph};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;

/// Trust app's PolicyOracle implementation.
///
/// This wraps TrustGraph internally but exposes only ConstraintSet to the kernel.
/// The kernel never sees TrustGraph, TrustClass, or trust scores directly.
///
/// # Thread Safety
///
/// Uses `parking_lot::RwLock` instead of `tokio::sync::RwLock` because
/// `PolicyOracle::evaluate()` is synchronous. This is intentional - see
/// the tech debt note in the `PolicyOracle` trait documentation.
pub struct TrustPolicyOracle {
    graph: Arc<RwLock<TrustGraph>>,
}

impl TrustPolicyOracle {
    /// Create a new TrustPolicyOracle wrapping a TrustGraph.
    pub fn new(graph: Arc<RwLock<TrustGraph>>) -> Self {
        Self { graph }
    }

    /// Convert trust score to kernel-enforceable constraints.
    ///
    /// # THIS IS THE MEANING FIREWALL BOUNDARY
    ///
    /// Above this function: trust semantics (scores, classes, graph operations)
    /// Below this function: generic kernel constraints (rate limits, multipliers)
    ///
    /// The kernel cannot determine WHY these constraint values were chosen.
    ///
    /// # Parameters
    ///
    /// - `score`: Trust score in range [0.0, 1.0]
    /// - `_action`: Reserved for future per-action constraints (e.g., read-only peers
    ///   could have different limits than writers). Currently unused but kept for
    ///   forward compatibility.
    fn score_to_constraints(&self, score: f64, _action: &ActionKind) -> ConstraintSet {
        // Rate limiting based on trust score
        // The kernel sees rate limits, not trust classes
        //
        // Rate limit tiers (from authz.rs):
        //   unlimited: u32::MAX msg/s (Federated)
        //   standard:  100 msg/s      (Partner)
        //   throttled: 20 msg/s       (Known)
        //   restricted: 5 msg/s       (Isolated)
        let rate_limit = match score {
            s if s >= 0.7 => RateLimit::unlimited(), // Federated class
            s if s >= 0.4 => RateLimit::standard(),  // Partner class
            s if s >= 0.1 => RateLimit::throttled(), // Known class
            _ => RateLimit::restricted(),            // Isolated class
        };

        // Max topics aligned with rate limits
        // Higher trust = more topic subscriptions allowed
        // Rationale: topic count should scale with message capacity
        let max_topics = match score {
            s if s >= 0.7 => 500, // Federated: unlimited rate, many topics
            s if s >= 0.4 => 100, // Partner: standard rate
            s if s >= 0.1 => 25,  // Known: throttled rate
            _ => 5,               // Isolated: restricted rate, minimal topics
        };

        // TODO(#868): max_connections will be enforced by icn-net after
        // rate_limit.rs migration to PolicyOracle (Phase 2.2). Currently set but not enforced.
        let max_connections = match score {
            s if s >= 0.7 => 100,
            s if s >= 0.4 => 50,
            s if s >= 0.1 => 20,
            _ => 5,
        };

        ConstraintSet::new()
            .with_rate_limit(rate_limit)
            .with_max_topics(max_topics)
            .with_max_connections(max_connections)
            // App-specific values are opaque to the kernel.
            .with_custom("credit_multiplier", score.into())
            .with_custom("voting_weight", score.into())
            .with_custom("trust_score", score.into())
    }

    fn required_trust_threshold(request: &PolicyRequest) -> Option<f64> {
        let mut threshold: Option<f64> = None;
        for key in ["min_trust_threshold", "acl_min_trust_score"] {
            if let Some(value) = request.context.metadata.get(key) {
                if let Ok(parsed) = value.parse::<f64>() {
                    threshold = Some(threshold.map_or(parsed, |current: f64| current.max(parsed)));
                }
            }
        }
        threshold
    }

    /// Parse org_id from metadata and convert to ScopeId.
    ///
    /// Supports federation and cooperative scope identifiers:
    /// - `did:icn:fed:<id>` -> `ScopeId::Federation(id)`
    /// - `did:icn:coop:<id>` -> `ScopeId::Cooperative(id)`
    /// - Other formats (e.g., plain names) -> `ScopeId::Cooperative(org_id)`
    ///
    /// Returns None if org_id is not present or invalid.
    fn parse_scope_from_org_id(request: &PolicyRequest) -> Option<ScopeId> {
        let org_id = request.context.metadata.get("org_id")?;
        
        // Check for DID-style org_id patterns
        if let Some(stripped) = org_id.strip_prefix("did:icn:fed:") {
            return Some(ScopeId::federation(stripped));
        }
        if let Some(stripped) = org_id.strip_prefix("did:icn:coop:") {
            return Some(ScopeId::cooperative(stripped));
        }
        
        // Fallback: treat as cooperative scope for plain names
        // This supports legacy org_id formats like "regional-food-network"
        if !org_id.is_empty() {
            return Some(ScopeId::cooperative(org_id));
        }
        
        None
    }
}

impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let graph = self.graph.read();

        // Convert kernel-api Did (String) to icn-identity Did
        // This is a type boundary - kernel uses String DIDs, trust uses validated DIDs
        let actor_did = match icn_identity::Did::from_str(&request.core.actor) {
            Ok(did) => did,
            Err(e) => {
                // Invalid DID format - return minimal trust constraints
                tracing::warn!(
                    actor = %request.core.actor,
                    error = %e,
                    "Invalid DID format, returning minimal trust"
                );
                return PolicyDecision::allow_with(
                    self.score_to_constraints(0.0, &request.core.action),
                );
            }
        };

        // Check for scope-bounded evaluation via org_id metadata
        // If org_id is present, compute trust within that scope
        // Otherwise, use global scope trust computation (backward compatible)
        let score = if let Some(scope) = Self::parse_scope_from_org_id(request) {
            // Scope-bounded trust computation
            tracing::debug!(
                actor = %request.core.actor,
                scope = %scope,
                "Computing trust score in scope"
            );
            match graph.compute_trust_score_in_scope(&actor_did, &scope) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        actor = %request.core.actor,
                        scope = %scope,
                        error = %e,
                        "Failed to compute scope-bounded trust score, returning minimal trust"
                    );
                    0.0
                }
            }
        } else {
            // Global scope trust computation (default)
            match graph.compute_trust_score(&actor_did) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        actor = %request.core.actor,
                        error = %e,
                        "Failed to compute trust score, returning minimal trust"
                    );
                    0.0
                }
            }
        };

        // Enforce any policy threshold hints from the kernel (topic ACL, etc.).
        if let Some(required) = Self::required_trust_threshold(request) {
            if score < required {
                tracing::warn!(
                    actor = %request.core.actor,
                    score = %score,
                    required = %required,
                    "Trust oracle denied request due to minimum trust threshold"
                );
                return PolicyDecision::deny("trust score below required threshold");
            }
        }

        // Log for debugging (semantic info stays in app)
        tracing::debug!(
            actor = %request.core.actor,
            score = %score,
            domain = %request.core.domain,
            action = ?request.core.action,
            "Trust oracle evaluated request"
        );

        // Convert to constraints (kernel only sees this)
        // THIS IS WHERE THE MEANING FIREWALL IS ENFORCED
        let constraints = self.score_to_constraints(score, &request.core.action);

        PolicyDecision::allow_with(constraints)
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }

    fn cache_ttl(&self) -> Duration {
        // Trust scores don't change frequently, cache for 30 seconds.
        //
        // Trade-off:
        // - Shorter TTL (10-15s): Tighter security, faster response to trust changes
        // - Longer TTL (60s+): Lower computation load, less responsive to changes
        //
        // 30s balances security and performance. See #878 for cache invalidation
        // when trust edges change (tighter security for immediate response).
        Duration::from_secs(30)
    }

    fn handles_cross_org(&self) -> bool {
        // Trust app handles federation trust bridging
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::authz::PolicyRequestCore;
    use icn_store::SledStore;
    use tempfile::tempdir;

    /// Create a minimal TrustPolicyOracle for testing.
    ///
    /// Note: The actual trust score for test subjects will be 0.0 (unknown)
    /// since we're not adding edges. For constraint mapping tests, we test
    /// `score_to_constraints` directly.
    fn create_test_oracle() -> TrustPolicyOracle {
        let temp_dir = tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();

        // Create a valid test DID using ed25519 keygen
        let keypair = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let own_did = icn_identity::Did::from_public_key(&keypair.verifying_key());

        let graph = TrustGraph::new(std::sync::Arc::new(store), own_did);

        TrustPolicyOracle::new(Arc::new(RwLock::new(graph)))
    }

    // ================================================================
    // Direct score_to_constraints tests (unit tests for meaning firewall)
    // ================================================================

    #[test]
    fn test_high_trust_score_gets_unlimited_rate() {
        let oracle = create_test_oracle();
        let constraints = oracle.score_to_constraints(0.8, &ActionKind::Write);

        // High trust (>= 0.7) should get unlimited rate
        assert!(constraints.rate_limit.is_some());
        let rate = constraints.rate_limit.as_ref().unwrap();
        assert_eq!(rate.messages_per_second, u32::MAX);
        assert_eq!(constraints.max_topics, Some(500)); // Aligned with unlimited rate
        assert_eq!(constraints.max_connections, Some(100));
    }

    #[test]
    fn test_medium_trust_score_gets_standard_rate() {
        let oracle = create_test_oracle();
        let constraints = oracle.score_to_constraints(0.5, &ActionKind::Write);

        // Medium trust (0.4-0.7) should get standard rate
        assert!(constraints.rate_limit.is_some());
        let rate = constraints.rate_limit.as_ref().unwrap();
        assert_eq!(rate.messages_per_second, 100);
        assert_eq!(constraints.max_topics, Some(100)); // Aligned with standard rate
        assert_eq!(constraints.max_connections, Some(50));
    }

    #[test]
    fn test_low_trust_score_gets_throttled_rate() {
        let oracle = create_test_oracle();
        let constraints = oracle.score_to_constraints(0.2, &ActionKind::Write);

        // Low trust (0.1-0.4) should get throttled rate
        assert!(constraints.rate_limit.is_some());
        let rate = constraints.rate_limit.as_ref().unwrap();
        assert_eq!(rate.messages_per_second, 20);
        assert_eq!(constraints.max_topics, Some(25)); // Aligned with throttled rate
        assert_eq!(constraints.max_connections, Some(20));
    }

    #[test]
    fn test_zero_trust_score_gets_restricted_rate() {
        let oracle = create_test_oracle();
        let constraints = oracle.score_to_constraints(0.05, &ActionKind::Write);

        // Very low trust (< 0.1) should get restricted rate
        assert!(constraints.rate_limit.is_some());
        let rate = constraints.rate_limit.as_ref().unwrap();
        assert_eq!(rate.messages_per_second, 5);
        assert_eq!(constraints.max_topics, Some(5)); // Aligned with restricted rate
        assert_eq!(constraints.max_connections, Some(5)); // Reduced for isolated peers
    }

    #[test]
    fn test_credit_multiplier_equals_score() {
        let oracle = create_test_oracle();

        for score in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let constraints = oracle.score_to_constraints(score, &ActionKind::Write);
            assert_eq!(
                constraints.custom.get("credit_multiplier"),
                Some(&score.into())
            );
            assert_eq!(constraints.custom.get("voting_weight"), Some(&score.into()));
        }
    }

    #[test]
    fn test_meaning_firewall_constraint_set_is_generic() {
        let oracle = create_test_oracle();
        let constraints = oracle.score_to_constraints(0.5, &ActionKind::Read);

        // Verify response contains only generic constraints
        // No TrustClass, no trust score explanation, no semantic meaning

        // These are generic kernel constraints
        assert!(constraints.rate_limit.is_some());
        assert!(constraints.max_topics.is_some());
        assert!(constraints.max_connections.is_some());
        assert!(constraints.custom.contains_key("credit_multiplier"));
        assert!(constraints.custom.contains_key("voting_weight"));

        // The ConstraintSet struct has no field for "trust class" or "trust score"
        // The kernel cannot tell WHY these values were chosen
    }

    // ================================================================
    // Oracle trait method tests
    // ================================================================

    #[test]
    fn test_oracle_domain_is_trust() {
        let oracle = create_test_oracle();
        assert_eq!(oracle.domain().as_str(), "trust");
    }

    #[test]
    fn test_oracle_handles_cross_org() {
        let oracle = create_test_oracle();
        assert!(oracle.handles_cross_org());
    }

    #[test]
    fn test_cache_ttl_is_30_seconds() {
        let oracle = create_test_oracle();
        assert_eq!(oracle.cache_ttl(), Duration::from_secs(30));
    }

    // ================================================================
    // Scope parsing tests
    // ================================================================

    #[test]
    fn test_parse_scope_from_org_id_cooperative_plain() {
        let core = PolicyRequestCore::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );
        let context = icn_kernel_api::authz::PolicyContext::new().with_metadata("org_id", "coop-a");
        let request = PolicyRequest::with_context(core, context);

        let scope = TrustPolicyOracle::parse_scope_from_org_id(&request);
        assert!(scope.is_some());
        assert_eq!(scope.unwrap(), icn_trust::ScopeId::cooperative("coop-a"));
    }

    #[test]
    fn test_parse_scope_from_org_id_federation_did_style() {
        let core = PolicyRequestCore::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );
        let context = icn_kernel_api::authz::PolicyContext::new()
            .with_metadata("org_id", "did:icn:fed:regional-alliance");
        let request = PolicyRequest::with_context(core, context);

        let scope = TrustPolicyOracle::parse_scope_from_org_id(&request);
        assert!(scope.is_some());
        assert_eq!(
            scope.unwrap(),
            icn_trust::ScopeId::federation("regional-alliance")
        );
    }

    #[test]
    fn test_parse_scope_from_org_id_cooperative_did_style() {
        let core = PolicyRequestCore::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );
        let context = icn_kernel_api::authz::PolicyContext::new()
            .with_metadata("org_id", "did:icn:coop:food-coop");
        let request = PolicyRequest::with_context(core, context);

        let scope = TrustPolicyOracle::parse_scope_from_org_id(&request);
        assert!(scope.is_some());
        assert_eq!(
            scope.unwrap(),
            icn_trust::ScopeId::cooperative("food-coop")
        );
    }

    #[test]
    fn test_parse_scope_from_org_id_no_metadata() {
        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        let scope = TrustPolicyOracle::parse_scope_from_org_id(&request);
        assert!(scope.is_none());
    }

    // ================================================================
    // Full evaluate() flow tests
    // ================================================================

    #[test]
    fn test_evaluate_unknown_actor_gets_zero_trust() {
        let oracle = create_test_oracle();

        // Create a valid test DID for the actor
        let actor_keypair = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let actor_did = icn_identity::Did::from_public_key(&actor_keypair.verifying_key());

        let request = PolicyRequest::new(
            actor_did.as_str().to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        let decision = oracle.evaluate(&request);

        // Unknown actor should still be allowed, but with minimal constraints
        assert!(decision.is_allowed());

        let constraints = decision.constraints().unwrap();
        // Unknown actors get 0.0 trust score -> restricted rate
        let rate = constraints.rate_limit.as_ref().unwrap();
        assert_eq!(rate.messages_per_second, 5);
    }

    #[test]
    fn test_evaluate_invalid_did_format_gets_zero_trust() {
        let oracle = create_test_oracle();

        // Invalid DID format (not a proper did:icn: format)
        let request = PolicyRequest::new(
            "not-a-valid-did".to_string(),
            ActionKind::Write,
            Domain::trust(),
        );

        let decision = oracle.evaluate(&request);

        // Invalid DID should still be allowed, but with minimal constraints
        assert!(decision.is_allowed());

        let constraints = decision.constraints().unwrap();
        // Invalid DID -> 0.0 trust -> restricted rate
        let rate = constraints.rate_limit.as_ref().unwrap();
        assert_eq!(rate.messages_per_second, 5);
    }
}
