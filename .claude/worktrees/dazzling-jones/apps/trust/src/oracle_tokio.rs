//! Tokio-compatible Trust Policy Oracle
//!
//! This module provides a PolicyOracle implementation that wraps a
//! `tokio::sync::RwLock<TrustGraph>` instead of `parking_lot::RwLock`.
//!
//! Use this when integrating with icn-core which uses tokio locks.
//!
//! # Note
//!
//! Since `PolicyOracle::evaluate()` is synchronous, this implementation
//! uses `try_read()` for non-blocking access to the tokio lock. If the
//! lock is contended, it fails closed with a score of 0.0 (most
//! restrictive constraints) rather than blocking.

use icn_kernel_api::authz::{
    ActionKind, ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest, RateLimit,
};
use icn_trust::TrustGraph;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Trust app's PolicyOracle implementation for tokio locks.
///
/// This wraps TrustGraph with `tokio::sync::RwLock` for compatibility
/// with icn-core's async lock usage.
pub struct TrustPolicyOracleTokio {
    graph: Arc<RwLock<TrustGraph>>,
}

impl TrustPolicyOracleTokio {
    /// Create a new TrustPolicyOracleTokio wrapping a TrustGraph.
    pub fn new(graph: Arc<RwLock<TrustGraph>>) -> Self {
        Self { graph }
    }

    /// Convert trust score to kernel-enforceable constraints.
    ///
    /// # THIS IS THE MEANING FIREWALL BOUNDARY
    ///
    /// Above this function: trust semantics (scores, classes, graph operations)
    /// Below this function: generic kernel constraints (rate limits, multipliers)
    fn score_to_constraints(&self, score: f64, _action: &ActionKind) -> ConstraintSet {
        // Rate limiting based on trust score
        let rate_limit = match score {
            s if s >= 0.7 => RateLimit::unlimited(), // Federated class
            s if s >= 0.4 => RateLimit::standard(),  // Partner class
            s if s >= 0.1 => RateLimit::throttled(), // Known class
            _ => RateLimit::restricted(),            // Isolated class
        };

        // Max topics aligned with rate limits
        let max_topics = match score {
            s if s >= 0.7 => 500,
            s if s >= 0.4 => 100,
            s if s >= 0.1 => 25,
            _ => 5,
        };

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
}

impl PolicyOracle for TrustPolicyOracleTokio {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Use try_read() for non-blocking access. If lock is contended,
        // fail closed with minimal trust. This avoids block_in_place
        // which requires a multi-threaded runtime (actix workers are single-threaded).
        let score = match self.graph.try_read() {
            Ok(graph) => compute_score(&graph, &request.core.actor),
            Err(_) => {
                // Lock contention — fail closed with minimal trust so unknown/low-trust
                // actors cannot gain elevated privileges by inducing contention.
                tracing::debug!(
                    actor = %request.core.actor,
                    "Trust oracle lock contention, using minimal score 0.0"
                );
                0.0 // Minimal trust under contention: most restrictive constraints
            }
        };

        // Enforce any policy threshold hints from the kernel
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

        tracing::debug!(
            actor = %request.core.actor,
            score = %score,
            domain = %request.core.domain,
            action = ?request.core.action,
            "Trust oracle (tokio) evaluated request"
        );

        let constraints = self.score_to_constraints(score, &request.core.action);
        PolicyDecision::allow_with(constraints)
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }

    fn cache_ttl(&self) -> Duration {
        Duration::from_secs(30)
    }

    fn handles_cross_org(&self) -> bool {
        true
    }
}

/// Compute trust score for an actor DID.
fn compute_score(graph: &TrustGraph, actor: &str) -> f64 {
    // Convert kernel-api DID (String) to icn-identity Did
    let actor_did = match icn_identity::Did::from_str(actor) {
        Ok(did) => did,
        Err(e) => {
            tracing::warn!(
                actor = %actor,
                error = %e,
                "Invalid DID format, returning minimal trust"
            );
            return 0.0;
        }
    };

    match graph.compute_trust_score(&actor_did) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                actor = %actor,
                error = %e,
                "Failed to compute trust score, returning minimal trust"
            );
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_store::SledStore;
    use tempfile::tempdir;

    fn create_test_oracle() -> TrustPolicyOracleTokio {
        let temp_dir = tempdir().unwrap();
        let store = SledStore::open(temp_dir.path()).unwrap();

        let keypair = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let own_did = icn_identity::Did::from_public_key(&keypair.verifying_key());

        let graph = TrustGraph::new(std::sync::Arc::new(store), own_did);
        TrustPolicyOracleTokio::new(Arc::new(RwLock::new(graph)))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_evaluate_unknown_actor() {
        let oracle = create_test_oracle();

        let actor_keypair = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let actor_did = icn_identity::Did::from_public_key(&actor_keypair.verifying_key());

        let request = PolicyRequest::new(
            actor_did.as_str().to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        let decision = oracle.evaluate(&request);

        assert!(decision.is_allowed());
        let constraints = decision.constraints().unwrap();
        // Unknown actor gets restricted rate (5 msg/s)
        let rate = constraints.rate_limit.as_ref().unwrap();
        assert_eq!(rate.messages_per_second, 5);
    }

    #[test]
    fn test_score_to_constraints() {
        let oracle = create_test_oracle();

        // High trust
        let constraints = oracle.score_to_constraints(0.8, &ActionKind::Write);
        assert_eq!(
            constraints.rate_limit.as_ref().unwrap().messages_per_second,
            u32::MAX
        );
        assert_eq!(constraints.max_topics, Some(500));

        // Low trust
        let constraints = oracle.score_to_constraints(0.05, &ActionKind::Write);
        assert_eq!(
            constraints.rate_limit.as_ref().unwrap().messages_per_second,
            5
        );
        assert_eq!(constraints.max_topics, Some(5));
    }
}
