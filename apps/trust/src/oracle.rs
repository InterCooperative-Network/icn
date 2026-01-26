//! Trust Policy Oracle
//!
//! Implements `PolicyOracle` to convert trust graph computations into
//! kernel-enforceable constraints. The kernel enforces these constraints
//! without understanding trust semantics (the "meaning firewall").
//!
//! # Trust-to-Constraint Mapping
//!
//! | Trust Score | Trust Class | Rate Limit | Credit Mult | Max Topics |
//! |-------------|-------------|------------|-------------|------------|
//! | 0.0 - 0.1   | Isolated    | 10 msg/s   | 0.1         | 5          |
//! | 0.1 - 0.4   | Known       | 50 msg/s   | 0.5         | 25         |
//! | 0.4 - 0.7   | Partner     | 100 msg/s  | 1.0         | 100        |
//! | 0.7 - 1.0   | Federated   | 200 msg/s  | 2.0         | 500        |

use icn_identity::Did as IdentityDid;
use icn_kernel_api::authz::{
    ConstraintSet, ConstraintValue, Domain, PolicyDecision, PolicyOracle, PolicyRequest, RateLimit,
};
use ordered_float::OrderedFloat;

// Re-export for tests that need it
#[cfg(test)]
use icn_kernel_api::authz::ActionKind;
use icn_trust::{MultiTrustGraph, TrustClass, TrustGraph};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, instrument};

/// Trust policy oracle for ICN.
///
/// Converts trust graph computations into kernel-enforceable constraints.
/// This oracle:
/// - Computes trust scores using the multi-graph architecture
/// - Maps scores to trust classes
/// - Returns constraints the kernel can enforce blindly
///
/// # Thread Safety
///
/// Uses `RwLock` for concurrent read access to trust graphs.
/// Most operations are reads (score computation), writes are infrequent
/// (edge updates via reducer).
pub struct TrustPolicyOracle {
    /// The multi-graph containing social, economic, and technical trust
    graph: Arc<RwLock<MultiTrustGraph>>,
    /// Cache TTL for policy decisions (default: 60 seconds)
    cache_ttl: Duration,
}

impl TrustPolicyOracle {
    /// Create a new trust policy oracle.
    pub fn new(graph: Arc<RwLock<MultiTrustGraph>>) -> Self {
        Self {
            graph,
            cache_ttl: Duration::from_secs(60),
        }
    }

    /// Create with custom cache TTL.
    pub fn with_cache_ttl(graph: Arc<RwLock<MultiTrustGraph>>, cache_ttl: Duration) -> Self {
        Self { graph, cache_ttl }
    }

    /// Compute combined trust score for an actor (async version).
    ///
    /// Uses weighted combination of social, economic, and technical scores.
    /// Converts from kernel's Did (String) to identity's Did for graph lookup.
    ///
    /// Note: Currently unused as PolicyOracle::evaluate is sync. Will be used
    /// when the oracle registry is made async-aware.
    #[allow(dead_code)]
    async fn compute_score(&self, actor: &str) -> f64 {
        // Convert kernel Did (String) to identity Did
        let identity_did = match IdentityDid::from_str(actor) {
            Ok(did) => did,
            Err(_) => return 0.0, // Invalid DID gets zero trust
        };

        let graph = self.graph.read().await;
        graph
            .compute_combined_trust_score(&identity_did)
            .unwrap_or_default()
    }

    /// Map trust score to rate limit constraints.
    fn rate_limit_for_score(score: f64) -> RateLimit {
        let trust_class = TrustClass::from_score(score);
        match trust_class {
            TrustClass::Isolated => RateLimit::new(10, 5),
            TrustClass::Known => RateLimit::new(50, 15),
            TrustClass::Partner => RateLimit::new(100, 25),
            TrustClass::Federated => RateLimit::new(200, 50),
        }
    }

    /// Map trust score to credit multiplier.
    fn credit_multiplier_for_score(score: f64) -> f64 {
        let trust_class = TrustClass::from_score(score);
        match trust_class {
            TrustClass::Isolated => 0.1,
            TrustClass::Known => 0.5,
            TrustClass::Partner => 1.0,
            TrustClass::Federated => 2.0,
        }
    }

    /// Map trust score to max topics.
    fn max_topics_for_score(score: f64) -> u32 {
        let trust_class = TrustClass::from_score(score);
        match trust_class {
            TrustClass::Isolated => 5,
            TrustClass::Known => 25,
            TrustClass::Partner => 100,
            TrustClass::Federated => 500,
        }
    }

    /// Map trust score to max connections.
    fn max_connections_for_score(score: f64) -> u32 {
        let trust_class = TrustClass::from_score(score);
        match trust_class {
            TrustClass::Isolated => 2,
            TrustClass::Known => 10,
            TrustClass::Partner => 50,
            TrustClass::Federated => 100,
        }
    }

    /// Build constraint set from trust score.
    fn constraints_for_score(score: f64) -> ConstraintSet {
        ConstraintSet::new()
            .with_rate_limit(Self::rate_limit_for_score(score))
            .with_credit_multiplier(Self::credit_multiplier_for_score(score))
            .with_max_topics(Self::max_topics_for_score(score))
            .with_max_connections(Self::max_connections_for_score(score))
            .with_voting_weight(score) // Voting weight equals trust score
            .with_custom("trust_score", ConstraintValue::Float(OrderedFloat(score)))
            .with_custom(
                "trust_class",
                ConstraintValue::String(format!("{:?}", TrustClass::from_score(score))),
            )
    }

    /// Evaluate a request synchronously (blocking).
    ///
    /// This is used by the PolicyOracle trait which requires sync evaluation.
    /// Uses `block_in_place` to safely block within a tokio runtime.
    #[instrument(skip(self), fields(actor = %request.actor()))]
    fn evaluate_sync(&self, request: &PolicyRequest) -> PolicyDecision {
        // Convert kernel Did (String) to identity Did
        let identity_did = match IdentityDid::from_str(request.actor()) {
            Ok(did) => did,
            Err(_) => {
                // Invalid DID gets Isolated constraints (lowest trust)
                return PolicyDecision::allow_with(Self::constraints_for_score(0.0));
            }
        };

        // Use block_in_place to safely block within a tokio runtime
        let score = tokio::task::block_in_place(|| {
            let graph = self.graph.blocking_read();
            graph
                .compute_combined_trust_score(&identity_did)
                .unwrap_or_default()
        });

        debug!(
            "Trust score for {}: {} (class: {:?})",
            request.actor(),
            score,
            TrustClass::from_score(score)
        );

        // Handle specific resource queries
        if let Some(resource) = &request.ext.resource {
            match resource.as_str() {
                "score" => {
                    // Caller wants just the score
                    let constraints = ConstraintSet::new()
                        .with_custom("trust_score", ConstraintValue::Float(OrderedFloat(score)));
                    return PolicyDecision::allow_with(constraints);
                }
                "class" => {
                    // Caller wants the trust class
                    let constraints = ConstraintSet::new().with_custom(
                        "trust_class",
                        ConstraintValue::String(format!("{:?}", TrustClass::from_score(score))),
                    );
                    return PolicyDecision::allow_with(constraints);
                }
                _ => {}
            }
        }

        // Default: return full constraints
        let constraints = Self::constraints_for_score(score);
        PolicyDecision::allow_with(constraints)
    }
}

impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        self.evaluate_sync(request)
    }

    fn cache_ttl(&self) -> Duration {
        self.cache_ttl
    }

    fn handles_cross_org(&self) -> bool {
        // Trust oracle can handle cross-org requests via federation
        true
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }
}

/// Simplified trust oracle for single-graph deployments.
///
/// Use this when you don't need the multi-graph architecture.
pub struct SimpleTrustOracle {
    graph: Arc<RwLock<TrustGraph>>,
    cache_ttl: Duration,
}

impl SimpleTrustOracle {
    /// Create a new simple trust oracle.
    pub fn new(graph: Arc<RwLock<TrustGraph>>) -> Self {
        Self {
            graph,
            cache_ttl: Duration::from_secs(60),
        }
    }
}

impl PolicyOracle for SimpleTrustOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Convert kernel Did (String) to identity Did
        let identity_did = match IdentityDid::from_str(request.actor()) {
            Ok(did) => did,
            Err(_) => {
                // Invalid DID gets Isolated constraints
                return PolicyDecision::allow_with(TrustPolicyOracle::constraints_for_score(0.0));
            }
        };

        // Use block_in_place to safely block within a tokio runtime
        let score = tokio::task::block_in_place(|| {
            let graph = self.graph.blocking_read();
            graph
                .compute_trust_score(&identity_did)
                .unwrap_or_default()
        });

        let constraints = TrustPolicyOracle::constraints_for_score(score);
        PolicyDecision::allow_with(constraints)
    }

    fn cache_ttl(&self) -> Duration {
        self.cache_ttl
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use icn_trust::TrustEdge;

    fn create_test_graph() -> Arc<RwLock<MultiTrustGraph>> {
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_did = KeyPair::generate().unwrap().did().clone();
        Arc::new(RwLock::new(MultiTrustGraph::new(store, own_did)))
    }

    #[test]
    fn test_rate_limit_mapping() {
        // Isolated: < 0.1
        let rl = TrustPolicyOracle::rate_limit_for_score(0.05);
        assert_eq!(rl.messages_per_second, 10);

        // Known: 0.1 - 0.4
        let rl = TrustPolicyOracle::rate_limit_for_score(0.25);
        assert_eq!(rl.messages_per_second, 50);

        // Partner: 0.4 - 0.7
        let rl = TrustPolicyOracle::rate_limit_for_score(0.55);
        assert_eq!(rl.messages_per_second, 100);

        // Federated: >= 0.7
        let rl = TrustPolicyOracle::rate_limit_for_score(0.85);
        assert_eq!(rl.messages_per_second, 200);
    }

    #[test]
    fn test_credit_multiplier_mapping() {
        assert_eq!(TrustPolicyOracle::credit_multiplier_for_score(0.05), 0.1);
        assert_eq!(TrustPolicyOracle::credit_multiplier_for_score(0.25), 0.5);
        assert_eq!(TrustPolicyOracle::credit_multiplier_for_score(0.55), 1.0);
        assert_eq!(TrustPolicyOracle::credit_multiplier_for_score(0.85), 2.0);
    }

    #[test]
    fn test_constraints_for_score() {
        let constraints = TrustPolicyOracle::constraints_for_score(0.55);

        assert!(constraints.rate_limit.is_some());
        assert_eq!(constraints.rate_limit.as_ref().unwrap().messages_per_second, 100);
        assert_eq!(constraints.credit_multiplier, Some(1.0));
        assert_eq!(constraints.max_topics, Some(100));
        assert_eq!(constraints.max_connections, Some(50));
        assert_eq!(constraints.voting_weight, Some(0.55));

        // Check custom constraints
        assert!(matches!(
            constraints.custom.get("trust_score"),
            Some(ConstraintValue::Float(s)) if (s.into_inner() - 0.55).abs() < 0.001
        ));
        assert!(matches!(
            constraints.custom.get("trust_class"),
            Some(ConstraintValue::String(s)) if s == "Partner"
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_oracle_evaluate_unknown_actor() {
        let graph = create_test_graph();
        let oracle = TrustPolicyOracle::new(graph);

        let unknown_did = KeyPair::generate().unwrap().did().clone();
        // Convert identity Did to kernel Did (String)
        let request = PolicyRequest::new(unknown_did.to_string(), ActionKind::Read, Domain::trust());

        let decision = oracle.evaluate(&request);

        // Unknown actor should get Isolated constraints
        assert!(decision.is_allowed());
        let constraints = decision.constraints().unwrap();
        assert_eq!(constraints.rate_limit.as_ref().unwrap().messages_per_second, 10);
        assert_eq!(constraints.credit_multiplier, Some(0.1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_oracle_evaluate_with_trust() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let own_did = KeyPair::generate().unwrap().did().clone();
        let peer_did = KeyPair::generate().unwrap().did().clone();

        let graph = Arc::new(RwLock::new(MultiTrustGraph::new(store, own_did.clone())));

        // Add trust edge
        {
            let mut g = graph.write().await;
            g.social_mut()
                .add_edge(TrustEdge::new(
                    own_did.clone(),
                    peer_did.clone(),
                    icn_trust::TrustScore::unchecked(0.8),
                ))
                .unwrap();
        }

        let oracle = TrustPolicyOracle::new(graph);
        // Convert identity Did to kernel Did (String)
        let request = PolicyRequest::new(peer_did.to_string(), ActionKind::Read, Domain::trust());

        let decision = oracle.evaluate(&request);

        // Peer should get constraints based on their trust score
        assert!(decision.is_allowed());
        let constraints = decision.constraints().unwrap();

        // Combined score calculation with only social edge at 0.8:
        // - Social: 0.8 * 0.6 (60/40 weighting) = 0.48, then * 0.5 (combined weight) = 0.24
        // - Economic: 0.05 fallback * 0.3 = 0.015
        // - Technical: 0.05 fallback * 0.2 = 0.01
        // Total: ~0.265 -> Known class (0.1-0.4) -> 50 msg/s rate limit
        assert_eq!(constraints.rate_limit.as_ref().unwrap().messages_per_second, 50);
    }

    #[test]
    fn test_oracle_domain() {
        let graph = create_test_graph();
        let oracle = TrustPolicyOracle::new(graph);

        assert_eq!(oracle.domain(), Domain::trust());
    }

    #[test]
    fn test_oracle_cache_ttl() {
        let graph = create_test_graph();
        let oracle = TrustPolicyOracle::new(graph);

        assert_eq!(oracle.cache_ttl(), Duration::from_secs(60));

        let oracle_custom = TrustPolicyOracle::with_cache_ttl(
            create_test_graph(),
            Duration::from_secs(120),
        );
        assert_eq!(oracle_custom.cache_ttl(), Duration::from_secs(120));
    }

    #[test]
    fn test_oracle_handles_cross_org() {
        let graph = create_test_graph();
        let oracle = TrustPolicyOracle::new(graph);

        assert!(oracle.handles_cross_org());
    }
}
