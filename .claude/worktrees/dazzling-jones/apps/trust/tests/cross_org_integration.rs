//! Cross-organization trust bridging integration tests.
//!
//! # Current Status
//!
//! These tests verify that the trust oracle correctly evaluates trust within specific
//! organizational scopes (cooperatives and federations) when `org_id` metadata is provided.
//!
//! The tests validate:
//! - `org_id` metadata can be passed through PolicyRequest
//! - Scope-bounded trust computation works correctly
//! - Trust scores differ based on scope (global vs. cooperative vs. federation)
//! - Unknown actors receive minimal trust constraints (security boundary)
//!
//! # Scope-Bounded Trust
//!
//! When `org_id` is provided in metadata:
//! - `did:icn:fed:<id>` → Federation scope
//! - `did:icn:coop:<id>` → Cooperative scope
//! - Plain name (e.g., `regional-food-network`) → Cooperative scope (fallback)
//!
//! When `org_id` is absent, global scope trust computation is used (backward compatible).

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

/// Verifies that the oracle accepts PolicyRequests with org_id metadata
/// and uses scope-bounded trust computation.
#[test]
fn test_cross_org_request_accepts_org_id_metadata() {
    // Setup
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add a trust edge in the "regional-food-network" cooperative scope
    {
        let mut g = graph.write();
        let mut edge = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.8),
        );
        edge.scope_id = Some(icn_trust::ScopeId::cooperative("regional-food-network"));
        g.add_edge(edge).unwrap();
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
    assert_eq!(
        request.context.metadata.get("org_id"),
        Some(&"regional-food-network".to_string())
    );

    // Verify that scope-bounded trust was computed correctly
    let constraints = decision.constraints().unwrap();
    let rate = constraints.rate_limit.as_ref().unwrap();
    // 0.8 direct * 0.7 weight = 0.56 -> standard rate (0.4-0.7 range)
    assert_eq!(
        rate.messages_per_second, 100,
        "Should use scope-bounded trust"
    );
}

/// Verifies high trust actors get unlimited rate in cross-org requests with scope-bounded trust.
#[test]
fn test_high_trust_actor_gets_unlimited_rate_in_cross_org_request() {
    // Setup: Two nodes with high trust edge in cooperative scope
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add high trust edge in the "cooperative-alliance" scope
    {
        let mut g = graph.write();
        let mut edge = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(1.0),
        );
        edge.scope_id = Some(icn_trust::ScopeId::cooperative("cooperative-alliance"));
        g.add_edge(edge).unwrap();
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

/// Verifies constraint generation for medium-trust actors in cross-org requests with scope-bounded trust.
#[test]
fn test_constraint_generation_for_cross_org_requests() {
    // Setup
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add medium trust edge in "mutual-credit-network" scope
    {
        let mut g = graph.write();
        let mut edge = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.7),
        );
        edge.scope_id = Some(icn_trust::ScopeId::cooperative("mutual-credit-network"));
        g.add_edge(edge).unwrap();
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

/// Verifies different actors get different constraints based on their trust levels.
/// This test demonstrates scope-bounded trust for multiple actors within the same scope.
#[test]
fn test_multiple_actors_with_different_trust_levels() {
    // Setup: Two actors with different trust levels in the same scope
    let owner = KeyPair::generate().unwrap();
    let actor1 = KeyPair::generate().unwrap();
    let actor2 = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    {
        let mut g = graph.write();

        // High trust actor in scope
        let mut edge1 = TrustEdge::new(
            owner.did().clone(),
            actor1.did().clone(),
            TrustScore::unchecked(1.0),
        );
        edge1.scope_id = Some(icn_trust::ScopeId::federation("high-trust-federation"));
        g.add_edge(edge1).unwrap();

        // Low trust actor in different scope
        let mut edge2 = TrustEdge::new(
            owner.did().clone(),
            actor2.did().clone(),
            TrustScore::unchecked(0.2),
        );
        edge2.scope_id = Some(icn_trust::ScopeId::federation("low-trust-federation"));
        g.add_edge(edge2).unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Test high-trust actor request
    let request1 = create_cross_org_request(
        actor1.did().as_str().to_string(),
        "did:icn:fed:high-trust-federation",
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
        "did:icn:fed:low-trust-federation",
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

/// Verifies requests without org_id use global scope trust computation (backward compatible).
#[test]
fn test_request_without_org_id_uses_global_scope() {
    // Setup
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    // Add global-scope trust edge (no scope_id)
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

    // Create request WITHOUT org_id metadata (should use global scope)
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
    assert_eq!(
        rate.messages_per_second, 100,
        "Should use global scope when org_id is absent"
    );
}

/// Verifies unknown actors receive minimal trust constraints (security boundary).
/// This is critical: zero-trust principle ensures unknown actors are restricted.
#[test]
fn test_unknown_actor_gets_minimal_trust_constraints() {
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
    // Test that cooperative org_id metadata is accepted and used for scope-bounded evaluation.
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    {
        let mut g = graph.write();
        let mut edge = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(1.0),
        );
        edge.scope_id = Some(icn_trust::ScopeId::cooperative("local-food-coop"));
        g.add_edge(edge).unwrap();
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
    assert_eq!(
        rate.messages_per_second,
        u32::MAX,
        "Should use cooperative-scoped trust"
    );
}

/// Verifies that the same actor can have different trust scores in different scopes.
///
/// **Test Validates**: Same actor with high trust in "coop-a" and low trust in "coop-b"
/// should get different rate limits when requesting with each org_id.
///
/// **Current Limitation**: TrustGraph storage uses `source:target` keys without scope,
/// so the second edge overwrites the first. This test documents the intended behavior
/// once icn-trust supports multi-scope edges (requires storage schema update).
///
/// **TODO**: Update TrustGraph storage to include scope_id in edge_key to support
/// multiple edges between the same DIDs with different scopes (separate follow-up issue).
#[test]
#[ignore = "Requires TrustGraph storage schema update to support multiple edges per DID pair with different scopes"]
fn test_same_actor_different_trust_in_different_scopes() {
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    {
        let mut g = graph.write();

        // High trust in "coop-a" scope
        let mut edge1 = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(1.0),
        );
        edge1.scope_id = Some(icn_trust::ScopeId::cooperative("coop-a"));
        g.add_edge(edge1).unwrap();

        // Low trust in "coop-b" scope
        let mut edge2 = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.2),
        );
        edge2.scope_id = Some(icn_trust::ScopeId::cooperative("coop-b"));
        g.add_edge(edge2).unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Test high-trust scope
    let request_a =
        create_cross_org_request(actor.did().as_str().to_string(), "coop-a", ActionKind::Read);
    let decision_a = oracle.evaluate(&request_a);
    assert!(decision_a.is_allowed());

    let constraints_a = decision_a.constraints().unwrap();
    let rate_a = constraints_a.rate_limit.as_ref().unwrap();
    // 1.0 * 0.7 = 0.7 -> unlimited rate
    assert_eq!(
        rate_a.messages_per_second,
        u32::MAX,
        "Should get unlimited rate in high-trust scope"
    );

    // Test low-trust scope
    let request_b =
        create_cross_org_request(actor.did().as_str().to_string(), "coop-b", ActionKind::Read);
    let decision_b = oracle.evaluate(&request_b);
    assert!(decision_b.is_allowed());

    let constraints_b = decision_b.constraints().unwrap();
    let rate_b = constraints_b.rate_limit.as_ref().unwrap();
    // 0.2 * 0.7 = 0.14 -> throttled rate (20 msg/s)
    assert_eq!(
        rate_b.messages_per_second, 20,
        "Should get throttled rate in low-trust scope"
    );
}

/// Verifies federation scope (did:icn:fed:*) is parsed and used correctly.
#[test]
fn test_federation_scope_parsing_and_evaluation() {
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    {
        let mut g = graph.write();
        let mut edge = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.8),
        );
        edge.scope_id = Some(icn_trust::ScopeId::federation("regional-alliance"));
        g.add_edge(edge).unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Use DID-style federation org_id
    let request = create_cross_org_request(
        actor.did().as_str().to_string(),
        "did:icn:fed:regional-alliance",
        ActionKind::Publish,
    );

    let decision = oracle.evaluate(&request);
    assert!(decision.is_allowed());

    let constraints = decision.constraints().unwrap();
    let rate = constraints.rate_limit.as_ref().unwrap();

    // 0.8 * 0.7 = 0.56 -> standard rate
    assert_eq!(
        rate.messages_per_second, 100,
        "Should use federation-scoped trust"
    );
}

/// Verifies cooperative scope (did:icn:coop:*) is parsed and used correctly.
#[test]
fn test_cooperative_scope_parsing_and_evaluation() {
    let owner = KeyPair::generate().unwrap();
    let actor = KeyPair::generate().unwrap();
    let graph = create_test_graph(&owner);

    {
        let mut g = graph.write();
        let mut edge = TrustEdge::new(
            owner.did().clone(),
            actor.did().clone(),
            TrustScore::unchecked(0.6),
        );
        edge.scope_id = Some(icn_trust::ScopeId::cooperative("food-coop"));
        g.add_edge(edge).unwrap();
    }

    let oracle = TrustPolicyOracle::new(graph);

    // Use DID-style cooperative org_id
    let request = create_cross_org_request(
        actor.did().as_str().to_string(),
        "did:icn:coop:food-coop",
        ActionKind::Write,
    );

    let decision = oracle.evaluate(&request);
    assert!(decision.is_allowed());

    let constraints = decision.constraints().unwrap();
    let rate = constraints.rate_limit.as_ref().unwrap();

    // 0.6 * 0.7 = 0.42 -> standard rate
    assert_eq!(
        rate.messages_per_second, 100,
        "Should use cooperative-scoped trust"
    );
}
