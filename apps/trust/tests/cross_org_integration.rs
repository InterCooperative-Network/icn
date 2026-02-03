//! Cross-organization trust bridging integration tests.
//!
//! # Current Status
//!
//! These tests verify that the trust oracle **accepts** PolicyRequests with `org_id`
//! metadata without errors. The oracle implementation (`apps/trust/src/oracle.rs`)
//! currently **ignores** `org_id` and always uses global scope trust computation.
//!
//! The tests validate:
//! - `org_id` metadata can be passed through PolicyRequest without errors
//! - Trust score → constraint mapping works correctly at all thresholds
//! - Unknown actors receive minimal trust constraints (security boundary)
//!
//! # Future Work
//!
//! TODO(#1046): Implement scope-bounded evaluation by checking
//! `request.context.metadata.get("org_id")` and calling
//! `compute_trust_score_in_scope()` with the appropriate ScopeId.
//! Once implemented, these tests should be updated to verify that different
//! scopes produce different trust scores for the same actor.

use icn_identity::KeyPair;
use icn_kernel_api::authz::{
    ActionKind, Domain, PolicyContext, PolicyOracle, PolicyRequest, PolicyRequestCore,
};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustScore};
use parking_lot::RwLock;
use std::sync::Arc;

// Import the trust policy oracle from the crate
use icn_trust_app::TrustPolicyOracle;

/// Create a test trust graph with the given owner DID.
fn create_test_graph(owner_keypair: &KeyPair) -> Arc<RwLock<TrustGraph>> {
    let store = Arc::new(SledStore::temporary().unwrap());
    let graph = TrustGraph::new(store, owner_keypair.did().clone());
    Arc::new(RwLock::new(graph))
}

/// Helper to create a PolicyRequest with org_id metadata.
fn create_cross_org_request(actor_did: String, org_id: &str, action: ActionKind) -> PolicyRequest {
    let core = PolicyRequestCore::new(actor_did, action, Domain::trust());
    let context = PolicyContext::new().with_metadata("org_id", org_id);
    PolicyRequest::with_context(core, context)
}

// ============================================================================
// Cross-Org Request Tests
// ============================================================================

#[test]
fn test_cross_org_request_with_org_id_metadata() {
    // Setup
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add a trust edge (global scope for now)
    {
        let mut g = graph.write();
        g.add_edge(TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.8),
        ))
        .unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Create request with org_id metadata
    let request = create_cross_org_request(
        actor.did().as_str().to_string(),
        "regional-food-network",
        ActionKind::Read,
    );

    // Verify the oracle handles the request
    let decision = oracle.evaluate(&request);

    // Should be allowed (oracle handles cross-org)
    assert!(decision.is_allowed());
    assert!(oracle.handles_cross_org());

    // Verify metadata was set correctly in the request.
    // Note: This tests the request builder, not the oracle's handling of metadata.
    // The oracle currently ignores org_id (see TODO #1046 for scope-bounded implementation).
    assert_eq!(
        request.context.metadata.get("org_id"),
        Some(&"regional-food-network".to_string())
    );
}

#[test]
fn test_trust_score_computation_across_federation_boundaries() {
    // Setup: Two nodes with high global trust edge
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add high trust edge - note that trust computation uses weighted scoring.
    // Direct trust of 1.0 with legacy weights (70% direct, 30% transitive) gives:
    // score = 1.0 * 0.7 + 0.0 * 0.3 = 0.7 (exactly at unlimited rate threshold).
    // We need to ensure the computed score is high enough to reach unlimited rate (>= 0.7).
    {
        let mut g = graph.write();
        // Add a higher direct trust edge that will result in >= 0.7 after weighting
        // With 70% weighting, we need at least 1.0 direct to get 0.7 computed
        g.add_edge(TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(1.0),
        ))
        .unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Create cross-org request
    let request = create_cross_org_request(
        actor.did().as_str().to_string(),
        "cooperative-alliance",
        ActionKind::Write,
    );

    let decision = oracle.evaluate(&request);

    // High trust should grant elevated privileges
    assert!(decision.is_allowed());

    let constraints = decision.constraints().unwrap();
    let rate = constraints.rate_limit.as_ref().unwrap();

    // With 1.0 direct trust and 70% weighting: 1.0 * 0.7 = 0.7 (exactly at threshold)
    assert_eq!(
        rate.messages_per_second,
        u32::MAX,
        "High trust should grant unlimited rate"
    );
    assert_eq!(constraints.max_topics, Some(500));
    assert_eq!(constraints.max_connections, Some(100));
}

#[test]
fn test_constraint_generation_for_cross_org_requests() {
    // Setup
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add medium trust edge (global scope for now)
    // Trust computation: 0.7 direct * 0.7 weight = 0.49 (standard rate range: 0.4-0.7)
    {
        let mut g = graph.write();
        g.add_edge(TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.7),
        ))
        .unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Create cross-org request with org_id metadata
    let request = create_cross_org_request(
        actor.did().as_str().to_string(),
        "mutual-credit-network",
        ActionKind::Execute,
    );

    let decision = oracle.evaluate(&request);
    assert!(decision.is_allowed());

    let constraints = decision.constraints().unwrap();

    // Verify constraint generation
    // Computed score ~0.49 maps to standard rate (0.4-0.7 range)
    let rate = constraints.rate_limit.as_ref().unwrap();
    assert_eq!(
        rate.messages_per_second, 100,
        "Medium trust should get standard rate"
    );
    assert_eq!(constraints.max_topics, Some(100));
    assert_eq!(constraints.max_connections, Some(50));

    // Verify custom constraints
    assert!(constraints.custom.contains_key("credit_multiplier"));
    assert!(constraints.custom.contains_key("voting_weight"));
    assert!(constraints.custom.contains_key("trust_score"));
}

#[test]
fn test_multiple_federation_scopes_with_different_trust_levels() {
    // Setup: Two actors with different global trust levels
    // This demonstrates how different actors get different constraints based on trust
    let owner = KeyPair::generate().unwrap();
    let actor1 = KeyPair::generate().unwrap();
    let actor2 = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    {
        let mut g = graph.write();

        // High trust actor - use 1.0 to ensure >= 0.7 after 70% weighting
        g.add_edge(TrustEdge::new(
            owner.did().clone(),
            actor1.did().clone(),
            TrustScore::unchecked(1.0),
        ))
        .unwrap();

        // Low trust actor - 0.2 * 0.7 = 0.14 (throttled rate range: 0.1-0.4)
        g.add_edge(TrustEdge::new(
            owner.did().clone(),
            actor2.did().clone(),
            TrustScore::unchecked(0.2),
        ))
        .unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Test high-trust actor request
    let request1 = create_cross_org_request(
        actor1.did().as_str().to_string(),
        "high-trust-federation",
        ActionKind::Read,
    );
    let decision1 = oracle.evaluate(&request1);
    assert!(decision1.is_allowed());

    let constraints1 = decision1.constraints().unwrap();
    let rate1 = constraints1.rate_limit.as_ref().unwrap();
    assert_eq!(
        rate1.messages_per_second,
        u32::MAX,
        "High trust actor should get unlimited rate"
    );

    // Test low-trust actor request
    let request2 = create_cross_org_request(
        actor2.did().as_str().to_string(),
        "low-trust-federation",
        ActionKind::Read,
    );
    let decision2 = oracle.evaluate(&request2);
    assert!(decision2.is_allowed());

    let constraints2 = decision2.constraints().unwrap();
    let rate2 = constraints2.rate_limit.as_ref().unwrap();
    assert_eq!(
        rate2.messages_per_second, 20,
        "Low trust actor should get throttled rate"
    );
}

#[test]
fn test_cross_org_request_without_org_id_uses_global_scope() {
    // Setup
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add global-scope trust edge
    {
        let mut g = graph.write();
        g.add_edge(TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.6),
        ))
        .unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Create request WITHOUT org_id metadata (should use global scope).
    // NOTE: Currently the oracle always uses global scope even when org_id IS provided.
    // This test will differentiate behavior once scope-bounded trust is implemented.
    let request = PolicyRequest::new(
        actor.did().as_str().to_string(),
        ActionKind::Write,
        Domain::trust(),
    );

    let decision = oracle.evaluate(&request);
    assert!(decision.is_allowed());

    let constraints = decision.constraints().unwrap();
    let rate = constraints.rate_limit.as_ref().unwrap();

    // Computed trust score 0.42 (0.6 direct * 0.7 weight) maps to standard rate (0.4-0.7 range)
    assert_eq!(rate.messages_per_second, 100);
}

#[test]
fn test_unknown_actor_in_cross_org_request_gets_minimal_trust() {
    // Setup
    let owner = KeyPair::generate().unwrap();
    let unknown_actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);
    // Don't add any edges for unknown_actor

    let oracle = TrustPolicyOracle::new(graph);

    // Create cross-org request for unknown actor
    let request = create_cross_org_request(
        unknown_actor.did().as_str().to_string(),
        "some-federation",
        ActionKind::Read,
    );

    let decision = oracle.evaluate(&request);

    // Unknown actors should still be allowed but with minimal constraints
    assert!(decision.is_allowed());

    let constraints = decision.constraints().unwrap();
    let rate = constraints.rate_limit.as_ref().unwrap();

    // Unknown actor gets 0.0 trust -> restricted rate (< 0.1)
    assert_eq!(
        rate.messages_per_second, 5,
        "Unknown actor should get restricted rate"
    );
    assert_eq!(constraints.max_topics, Some(5));
    assert_eq!(constraints.max_connections, Some(5));
}

#[test]
fn test_cross_org_request_accepts_cooperative_org_id() {
    // Test that cooperative org_id metadata is accepted without errors.
    // NOTE: Currently the oracle ignores org_id and uses global scope.
    // This test verifies metadata acceptance, not scope-bounded computation.
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    {
        let mut g = graph.write();
        // Use 1.0 to ensure >= 0.7 after 70% weighting
        g.add_edge(TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(1.0),
        ))
        .unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Create request with cooperative org_id
    let request = create_cross_org_request(
        actor.did().as_str().to_string(),
        "local-food-coop",
        ActionKind::Publish,
    );

    let decision = oracle.evaluate(&request);
    assert!(decision.is_allowed());

    let constraints = decision.constraints().unwrap();
    let rate = constraints.rate_limit.as_ref().unwrap();

    // Trust score 1.0 * 0.7 = 0.7 should grant unlimited rate (>= 0.7)
    assert_eq!(rate.messages_per_second, u32::MAX);
}
