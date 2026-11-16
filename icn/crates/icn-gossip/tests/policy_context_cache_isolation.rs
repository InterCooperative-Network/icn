//! Regression for core-only policy cache keys reused across trust-gated topics.
//!
//! This exercises the assembled GossipActor subscription path with the *actual*
//! OracleRegistry. The oracle intentionally has a fixed score to isolate cache
//! behavior from trust-graph calculation and transport identity binding.
//! Authentication of network claimed DIDs is a distinct, unresolved contract.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::{Did, KeyPair};
use icn_kernel_api::authz::{Domain, PolicyDecision, PolicyOracle, PolicyRequest};
use icn_kernel_api::bootstrap::{BootstrapPhase, OracleRegistry};

/// Mirrors the relevant decision of the trust oracle for a 0.5-scored peer.
struct FixedTrustOracle;

impl PolicyOracle for FixedTrustOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let threshold = ["min_trust_threshold", "acl_min_trust_score"]
            .iter()
            .filter_map(|key| request.context.metadata.get(*key))
            .filter_map(|value| value.parse::<f64>().ok())
            .fold(0.0_f64, f64::max);

        if threshold > 0.5 {
            PolicyDecision::deny("trust score below required threshold")
        } else {
            PolicyDecision::allow()
        }
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }
}

fn setup_topics() -> (GossipActor, Did) {
    let node = KeyPair::generate().expect("node test key");
    let peer = KeyPair::generate().expect("peer test key");

    let registry = Arc::new(OracleRegistry::new());
    registry.register(Domain::trust(), Arc::new(FixedTrustOracle));
    registry.set_phase(BootstrapPhase::Running);

    let oracle: Arc<dyn PolicyOracle> = registry;
    let mut gossip = GossipActor::new(node.did().clone(), Some(oracle));
    gossip.create_topic(Topic::new(
        "community:open".to_string(),
        AccessControl::MinTrustScore(0.2),
    ));
    gossip.create_topic(Topic::new(
        "community:restricted".to_string(),
        AccessControl::MinTrustScore(0.8),
    ));
    (gossip, peer.did().clone())
}

#[tokio::test]
async fn permissive_topic_does_not_authorize_restricted_topic() {
    let (mut gossip, peer) = setup_topics();

    gossip
        .subscribe_from_network("community:open", peer.clone())
        .await
        .expect("peer with score 0.5 meets threshold 0.2");
    let denied = gossip
        .subscribe_from_network("community:restricted", peer.clone())
        .await;

    assert!(denied.is_err(), "higher threshold must still be checked");
    assert!(gossip.is_subscribed("community:open", &peer));
    assert!(!gossip.is_subscribed("community:restricted", &peer));
}

#[tokio::test]
async fn rejected_topic_does_not_poison_permissive_topic() {
    let (mut gossip, peer) = setup_topics();

    assert!(gossip
        .subscribe_from_network("community:restricted", peer.clone())
        .await
        .is_err());
    gossip
        .subscribe_from_network("community:open", peer.clone())
        .await
        .expect("a previous deny must not leak to an allowed topic");

    assert!(!gossip.is_subscribed("community:restricted", &peer));
    assert!(gossip.is_subscribed("community:open", &peer));
}
