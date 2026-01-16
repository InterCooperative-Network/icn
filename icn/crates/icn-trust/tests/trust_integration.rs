//! Integration tests for the ICN trust layer.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! These tests verify the trust graph functionality including:
//! - Multi-graph trust management (Social, Economic, Technical)
//! - Trust attestation signing and verification
//! - Transitive trust computation
//! - Cache behavior
//! - DID recovery flows

use icn_identity::KeyPair;
use icn_store::SledStore;
use icn_trust::{
    EvidenceValidator, EvidenceValidatorConfig, MultiTrustGraph, ScoringWeights, TrustAttestation,
    TrustCache, TrustClass, TrustEdge, TrustEvidence, TrustGraph, TrustGraphType,
};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Multi-Graph Integration Tests
// ============================================================================

#[test]
fn test_multi_graph_complete_trust_network() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();
    let dave = KeyPair::generate().unwrap();

    let mut multi = MultiTrustGraph::new(store, alice.did().clone());

    // Build a realistic multi-dimensional trust network
    // Social connections: Alice knows Bob well, Bob knows Carol
    multi
        .social_mut()
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.8))
        .unwrap();
    multi
        .social_mut()
        .add_edge(TrustEdge::new(bob.did().clone(), carol.did().clone(), 0.7))
        .unwrap();

    // Economic reliability: Alice has done business with Bob, Bob with Dave
    multi
        .economic_mut()
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.9))
        .unwrap();
    multi
        .economic_mut()
        .add_edge(TrustEdge::new(bob.did().clone(), dave.did().clone(), 0.6))
        .unwrap();

    // Technical reliability: Alice trusts Carol's node, Carol trusts Dave's node
    multi
        .technical_mut()
        .add_edge(TrustEdge::new(
            alice.did().clone(),
            carol.did().clone(),
            0.7,
        ))
        .unwrap();
    multi
        .technical_mut()
        .add_edge(TrustEdge::new(carol.did().clone(), dave.did().clone(), 0.8))
        .unwrap();

    // Verify graph isolation - each graph has different trust profiles
    // Bob: high in social and economic (direct from Alice)
    let bob_social = multi.social().compute_trust_score(bob.did()).unwrap();
    let bob_economic = multi.economic().compute_trust_score(bob.did()).unwrap();
    let bob_technical = multi.technical().compute_trust_score(bob.did()).unwrap();

    assert!(bob_social > 0.4, "Bob should be socially trusted");
    assert!(bob_economic > 0.7, "Bob should be economically trusted");
    assert!(
        bob_technical < 0.1,
        "Bob has no technical trust: {bob_technical}"
    );

    // Carol: transitive social via Bob, direct technical
    let carol_social = multi.social().compute_trust_score(carol.did()).unwrap();
    let carol_technical = multi.technical().compute_trust_score(carol.did()).unwrap();

    assert!(
        carol_social > 0.0 && carol_social < 0.4,
        "Carol should have transitive social trust: {carol_social}"
    );
    assert!(
        carol_technical > 0.5,
        "Carol should have direct technical trust"
    );

    // Combined score considers all three dimensions
    let bob_combined = multi.compute_combined_trust_score(bob.did()).unwrap();
    // Expected: social(0.48) * 0.5 + economic(0.72) * 0.3 + technical(0) * 0.2
    assert!(
        bob_combined > 0.3,
        "Bob should have good combined trust: {bob_combined}"
    );
}

#[test]
fn test_multi_graph_dimension_specific_operations() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut multi = MultiTrustGraph::new(store, alice.did().clone());

    // Use add_edge_to for explicit graph targeting
    multi
        .add_edge_to(
            TrustGraphType::Social,
            TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.9),
        )
        .unwrap();

    multi
        .add_edge_to(
            TrustGraphType::EconomicReliability,
            TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.3),
        )
        .unwrap();

    multi
        .add_edge_to(
            TrustGraphType::TechnicalReliability,
            TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.5),
        )
        .unwrap();

    // Verify each graph got the correct edge
    let social_edge = multi
        .social()
        .get_edge(alice.did(), bob.did())
        .unwrap()
        .unwrap();
    let economic_edge = multi
        .economic()
        .get_edge(alice.did(), bob.did())
        .unwrap()
        .unwrap();
    let technical_edge = multi
        .technical()
        .get_edge(alice.did(), bob.did())
        .unwrap()
        .unwrap();

    assert!((social_edge.score - 0.9).abs() < 0.001);
    assert!((economic_edge.score - 0.3).abs() < 0.001);
    assert!((technical_edge.score - 0.5).abs() < 0.001);

    // Verify type-specific scoring weights
    // Social: 60% direct = 0.9 * 0.6 = 0.54
    let social_score = multi.social().compute_trust_score(bob.did()).unwrap();
    assert!(
        (social_score - 0.54).abs() < 0.01,
        "Social score: {social_score}"
    );

    // Economic: 80% direct = 0.3 * 0.8 = 0.24
    let economic_score = multi.economic().compute_trust_score(bob.did()).unwrap();
    assert!(
        (economic_score - 0.24).abs() < 0.01,
        "Economic score: {economic_score}"
    );

    // Technical: 90% direct = 0.5 * 0.9 = 0.45
    let technical_score = multi.technical().compute_trust_score(bob.did()).unwrap();
    assert!(
        (technical_score - 0.45).abs() < 0.01,
        "Technical score: {technical_score}"
    );
}

// ============================================================================
// Trust Attestation Integration Tests
// ============================================================================

#[test]
fn test_attestation_sign_verify_store_cycle() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice.did().clone());

    // Create and sign attestation
    let mut attestation = TrustAttestation::new(alice.did().clone(), bob.did().clone(), 0.75)
        .with_label("partner")
        .with_evidence("contract_123")
        .with_ttl(86400); // 1 day

    attestation.sign(&alice).unwrap();

    // Verify signature
    attestation.verify().unwrap();

    // Convert to edge and store
    let edge = attestation.to_trust_edge();
    graph.add_edge(edge).unwrap();

    // Retrieve and verify
    let stored_edge = graph
        .get_edge(alice.did(), bob.did())
        .unwrap()
        .expect("Edge should exist");

    assert!((stored_edge.score - 0.75).abs() < 0.001);
    assert_eq!(stored_edge.labels, vec!["partner"]);
    // Evidence from attestation is converted to TrustEvidence::Legacy
    assert_eq!(stored_edge.evidence.len(), 1);
    match &stored_edge.evidence[0] {
        icn_trust::TrustEvidence::Legacy { reference, .. } => {
            assert_eq!(reference, "contract_123");
        }
        other => panic!("Expected Legacy evidence, got {other:?}"),
    }
}

#[test]
fn test_attestation_typed_graph_routing() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut multi = MultiTrustGraph::new(store, alice.did().clone());

    // Create attestations for different graph types
    let types_and_scores = [
        (TrustGraphType::Social, 0.9),
        (TrustGraphType::EconomicReliability, 0.6),
        (TrustGraphType::TechnicalReliability, 0.4),
    ];

    for (graph_type, score) in types_and_scores {
        let mut attestation =
            TrustAttestation::new_typed(alice.did().clone(), bob.did().clone(), score, graph_type);
        attestation.sign(&alice).unwrap();
        attestation.verify().unwrap();

        let edge = attestation.to_trust_edge();
        multi.add_edge_to(graph_type, edge).unwrap();
    }

    // Verify each graph got its attestation
    assert!(multi
        .social()
        .get_edge(alice.did(), bob.did())
        .unwrap()
        .is_some());
    assert!(multi
        .economic()
        .get_edge(alice.did(), bob.did())
        .unwrap()
        .is_some());
    assert!(multi
        .technical()
        .get_edge(alice.did(), bob.did())
        .unwrap()
        .is_some());
}

#[test]
fn test_attestation_supersede_logic() {
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let base_time = 1_700_000_000u64;

    // Old attestation
    let old = TrustAttestation {
        issuer: alice.did().clone(),
        subject: bob.did().clone(),
        score: 0.5,
        labels: vec![],
        evidence: vec![],
        ttl_seconds: 86400,
        created_at: base_time,
        signature: vec![],
        graph_type: TrustGraphType::Social,
    };

    // Clearly newer (6 minutes later, outside tolerance)
    let newer = TrustAttestation {
        issuer: alice.did().clone(),
        subject: bob.did().clone(),
        score: 0.5,
        labels: vec![],
        evidence: vec![],
        ttl_seconds: 86400,
        created_at: base_time + 400,
        signature: vec![],
        graph_type: TrustGraphType::Social,
    };

    assert!(newer.should_supersede(old.created_at, old.score));
    assert!(!old.should_supersede(newer.created_at, newer.score));

    // Within tolerance, higher score wins
    let within_tolerance_higher = TrustAttestation {
        issuer: alice.did().clone(),
        subject: bob.did().clone(),
        score: 0.8, // Higher score
        labels: vec![],
        evidence: vec![],
        ttl_seconds: 86400,
        created_at: base_time + 60, // Only 1 minute newer
        signature: vec![],
        graph_type: TrustGraphType::Social,
    };

    assert!(within_tolerance_higher.should_supersede(old.created_at, old.score));
}

// ============================================================================
// Transitive Trust Computation Tests
// ============================================================================

#[test]
fn test_complex_transitive_trust_network() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();
    let dave = KeyPair::generate().unwrap();
    let eve = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice.did().clone());

    // Build a trust web:
    // Alice -> Bob (0.8)
    // Alice -> Carol (0.6)
    // Bob -> Dave (0.7)
    // Carol -> Dave (0.9)
    // Bob -> Eve (0.5)

    graph
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.8))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(
            alice.did().clone(),
            carol.did().clone(),
            0.6,
        ))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(bob.did().clone(), dave.did().clone(), 0.7))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(carol.did().clone(), dave.did().clone(), 0.9))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(bob.did().clone(), eve.did().clone(), 0.5))
        .unwrap();

    // Direct trust scores (70% direct, 30% transitive)
    let bob_score = graph.compute_trust_score(bob.did()).unwrap();
    assert!((bob_score - 0.56).abs() < 0.01, "Bob score: {bob_score}"); // 0.8 * 0.7

    let carol_score = graph.compute_trust_score(carol.did()).unwrap();
    assert!(
        (carol_score - 0.42).abs() < 0.01,
        "Carol score: {carol_score}"
    ); // 0.6 * 0.7

    // Dave has no direct trust from Alice, only transitive via Bob and Carol
    // Transitive = avg(0.8*0.7, 0.6*0.9) * 0.3 = avg(0.56, 0.54) * 0.3 = 0.55 * 0.3 = 0.165
    let dave_score = graph.compute_trust_score(dave.did()).unwrap();
    assert!(
        dave_score > 0.1 && dave_score < 0.25,
        "Dave score: {dave_score}"
    );

    // Eve has transitive only via Bob
    // Transitive = 0.8 * 0.5 * 0.3 = 0.12
    let eve_score = graph.compute_trust_score(eve.did()).unwrap();
    assert!(
        eve_score > 0.1 && eve_score < 0.15,
        "Eve score: {eve_score}"
    );
}

#[test]
fn test_custom_scoring_weights() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice.did().clone());

    // Alice trusts Bob directly, Bob trusts Carol
    graph
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 1.0))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(bob.did().clone(), carol.did().clone(), 1.0))
        .unwrap();

    // With 100% direct weight, Carol gets 0 (no direct edge)
    let direct_only = ScoringWeights::new(1.0, 0.0);
    let carol_direct_only = graph
        .compute_trust_score_weighted(carol.did(), direct_only)
        .unwrap();
    assert!(
        carol_direct_only < 0.01,
        "Carol direct-only: {carol_direct_only}"
    );

    // Clear cache before testing different weights
    // (cache doesn't distinguish by weight parameters)
    graph.clear_cache();

    // With 100% transitive weight, Carol gets full transitive
    let transitive_only = ScoringWeights::new(0.0, 1.0);
    let carol_transitive_only = graph
        .compute_trust_score_weighted(carol.did(), transitive_only)
        .unwrap();
    assert!(
        carol_transitive_only > 0.9,
        "Carol transitive-only: {carol_transitive_only}"
    );
}

// ============================================================================
// Trust Class Tests
// ============================================================================

#[test]
fn test_trust_class_boundaries() {
    assert_eq!(TrustClass::from_score(0.0), TrustClass::Isolated);
    assert_eq!(TrustClass::from_score(0.09), TrustClass::Isolated);
    assert_eq!(TrustClass::from_score(0.1), TrustClass::Known);
    assert_eq!(TrustClass::from_score(0.39), TrustClass::Known);
    assert_eq!(TrustClass::from_score(0.4), TrustClass::Partner);
    assert_eq!(TrustClass::from_score(0.69), TrustClass::Partner);
    assert_eq!(TrustClass::from_score(0.7), TrustClass::Federated);
    assert_eq!(TrustClass::from_score(1.0), TrustClass::Federated);
}

#[test]
fn test_trust_class_from_graph() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();
    let dave = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice.did().clone());

    // Different trust levels
    graph
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.95))
        .unwrap(); // → Federated
    graph
        .add_edge(TrustEdge::new(
            alice.did().clone(),
            carol.did().clone(),
            0.5,
        ))
        .unwrap(); // → Known
    graph
        .add_edge(TrustEdge::new(alice.did().clone(), dave.did().clone(), 0.1))
        .unwrap(); // → Isolated

    assert_eq!(graph.trust_class(bob.did()).unwrap(), TrustClass::Partner); // 0.95 * 0.7 = 0.665
    assert_eq!(graph.trust_class(carol.did()).unwrap(), TrustClass::Known); // 0.5 * 0.7 = 0.35
    assert_eq!(graph.trust_class(dave.did()).unwrap(), TrustClass::Isolated); // 0.1 * 0.7 = 0.07
}

// ============================================================================
// Cache Integration Tests
// ============================================================================

#[test]
fn test_cache_ttl_expiration() {
    let cache = TrustCache::with_config(100, Duration::from_millis(50));
    let alice = KeyPair::generate().unwrap();

    // Put and verify
    cache.put(alice.did().clone(), 0.75);
    assert_eq!(cache.get(alice.did()), Some(0.75));

    // Wait for expiration
    std::thread::sleep(Duration::from_millis(60));

    // Should be expired
    assert_eq!(cache.get(alice.did()), None);
}

#[test]
fn test_cache_lru_eviction() {
    let cache = TrustCache::with_config(3, Duration::from_secs(300));

    let dids: Vec<_> = (0..5).map(|_| KeyPair::generate().unwrap()).collect();

    // Fill cache
    for (i, did) in dids.iter().enumerate().take(3) {
        cache.put(did.did().clone(), 0.1 * i as f64);
    }

    assert_eq!(cache.len(), 3);

    // Access first to make it recently used
    cache.get(dids[0].did());

    // Add two more - should evict dids[1] and dids[2]
    cache.put(dids[3].did().clone(), 0.3);
    cache.put(dids[4].did().clone(), 0.4);

    // did[0] should still be present (recently accessed)
    assert!(cache.get(dids[0].did()).is_some());
    // did[1] should be evicted
    assert!(cache.get(dids[1].did()).is_none());
}

#[test]
fn test_cache_warm() {
    let cache = TrustCache::new();

    let dids: Vec<_> = (0..10).map(|_| KeyPair::generate().unwrap()).collect();

    let warm_data: Vec<_> = dids
        .iter()
        .enumerate()
        .map(|(i, kp)| (kp.did().clone(), 0.1 * i as f64))
        .collect();

    cache.warm(warm_data);

    assert_eq!(cache.len(), 10);
    for (i, kp) in dids.iter().enumerate() {
        let expected = 0.1 * i as f64;
        let actual = cache.get(kp.did()).unwrap();
        assert!((actual - expected).abs() < 0.001);
    }
}

// ============================================================================
// DID Recovery Integration Tests
// ============================================================================

#[test]
fn test_did_recovery_single_graph() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice_old = KeyPair::generate().unwrap();
    let alice_new = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice_old.did().clone());

    // Build relationships with old DID
    graph
        .add_edge(TrustEdge::new(
            alice_old.did().clone(),
            bob.did().clone(),
            0.8,
        ))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(
            bob.did().clone(),
            alice_old.did().clone(),
            0.7,
        ))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(
            carol.did().clone(),
            alice_old.did().clone(),
            0.9,
        ))
        .unwrap();

    // Perform recovery
    let migrated = graph
        .map_did_recovery(alice_old.did(), alice_new.did())
        .unwrap();
    assert_eq!(migrated, 3);

    // Old edges should be gone
    assert!(graph
        .get_edge(alice_old.did(), bob.did())
        .unwrap()
        .is_none());
    assert!(graph
        .get_edge(bob.did(), alice_old.did())
        .unwrap()
        .is_none());

    // New edges should exist
    let new_outgoing = graph.get_edge(alice_new.did(), bob.did()).unwrap().unwrap();
    assert!((new_outgoing.score - 0.8).abs() < 0.001);

    let new_incoming_bob = graph.get_edge(bob.did(), alice_new.did()).unwrap().unwrap();
    assert!((new_incoming_bob.score - 0.7).abs() < 0.001);

    let new_incoming_carol = graph
        .get_edge(carol.did(), alice_new.did())
        .unwrap()
        .unwrap();
    assert!((new_incoming_carol.score - 0.9).abs() < 0.001);
}

#[test]
fn test_did_recovery_multi_graph() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice_old = KeyPair::generate().unwrap();
    let alice_new = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut multi = MultiTrustGraph::new(store, alice_old.did().clone());

    // Build relationships across all graphs
    multi
        .social_mut()
        .add_edge(TrustEdge::new(
            alice_old.did().clone(),
            bob.did().clone(),
            0.8,
        ))
        .unwrap();
    multi
        .economic_mut()
        .add_edge(TrustEdge::new(
            alice_old.did().clone(),
            bob.did().clone(),
            0.7,
        ))
        .unwrap();
    multi
        .technical_mut()
        .add_edge(TrustEdge::new(
            alice_old.did().clone(),
            bob.did().clone(),
            0.6,
        ))
        .unwrap();

    // Perform recovery
    let migrated = multi
        .map_did_recovery(alice_old.did(), alice_new.did())
        .unwrap();
    assert_eq!(migrated, 3);

    // Verify migration across all graphs
    assert!(multi
        .social()
        .get_edge(alice_new.did(), bob.did())
        .unwrap()
        .is_some());
    assert!(multi
        .economic()
        .get_edge(alice_new.did(), bob.did())
        .unwrap()
        .is_some());
    assert!(multi
        .technical()
        .get_edge(alice_new.did(), bob.did())
        .unwrap()
        .is_some());

    // Old edges should be gone
    assert!(multi
        .social()
        .get_edge(alice_old.did(), bob.did())
        .unwrap()
        .is_none());
}

// ============================================================================
// Edge Expiry Integration Tests
// ============================================================================

#[test]
fn test_edge_expiry_filtering() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice.did().clone());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Add expired edge
    let expired_edge =
        TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.8).with_expiry(now - 100); // Expired 100 seconds ago

    graph.add_edge(expired_edge).unwrap();

    // Should not be retrievable
    let retrieved = graph.get_edge(alice.did(), bob.did()).unwrap();
    assert!(retrieved.is_none(), "Expired edge should not be returned");

    // Trust score should be 0
    let score = graph.compute_trust_score(bob.did()).unwrap();
    assert!(score < 0.01, "Expired edge should not contribute to score");
}

#[test]
fn test_outgoing_edges_expiry_filtering() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice.did().clone());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Add valid and expired edges
    graph
        .add_edge(
            TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.8).with_expiry(now + 3600), // Valid for 1 hour
        )
        .unwrap();

    graph
        .add_edge(
            TrustEdge::new(alice.did().clone(), carol.did().clone(), 0.7).with_expiry(now - 100), // Already expired
        )
        .unwrap();

    // Only valid edge should be returned
    let edges = graph.get_outgoing_edges(alice.did()).unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target, *bob.did());
}

// ============================================================================
// Query Aggregation Tests
// ============================================================================

#[test]
fn test_get_dids_above_threshold() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();
    let dave = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store, alice.did().clone());

    // Different trust levels
    // Bob: 0.9 * 0.7 = 0.63 (Partner)
    // Carol: 0.5 * 0.7 = 0.35 (Known)
    // Dave: 0.15 * 0.7 = 0.105 (Known, barely)
    graph
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.9))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(
            alice.did().clone(),
            carol.did().clone(),
            0.5,
        ))
        .unwrap();
    graph
        .add_edge(TrustEdge::new(
            alice.did().clone(),
            dave.did().clone(),
            0.15,
        ))
        .unwrap();

    // Threshold 0.5: only Bob
    let high_trust = graph.get_dids_above_threshold(0.5).unwrap();
    assert_eq!(high_trust.len(), 1);
    assert!(high_trust.contains(bob.did()));

    // Threshold 0.2: Bob and Carol
    let medium_trust = graph.get_dids_above_threshold(0.2).unwrap();
    assert_eq!(medium_trust.len(), 2);
    assert!(medium_trust.contains(bob.did()));
    assert!(medium_trust.contains(carol.did()));

    // Threshold 0.1: all three
    let low_trust = graph.get_dids_above_threshold(0.1).unwrap();
    assert_eq!(low_trust.len(), 3);
}

#[test]
fn test_get_all_known_dids_multi_graph() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();
    let dave = KeyPair::generate().unwrap();

    let mut multi = MultiTrustGraph::new(store, alice.did().clone());

    // Add different DIDs to different graphs (with some overlap)
    multi
        .social_mut()
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.5))
        .unwrap();
    multi
        .economic_mut()
        .add_edge(TrustEdge::new(
            alice.did().clone(),
            carol.did().clone(),
            0.5,
        ))
        .unwrap();
    multi
        .technical_mut()
        .add_edge(TrustEdge::new(alice.did().clone(), dave.did().clone(), 0.5))
        .unwrap();

    // Bob also appears in economic
    multi
        .economic_mut()
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.3))
        .unwrap();

    // Get all unique DIDs across all graphs
    let all_dids = multi.get_all_known_dids().unwrap();

    // Should have alice, bob, carol, dave (4 unique)
    assert_eq!(all_dids.len(), 4);
    assert!(all_dids.contains(alice.did()));
    assert!(all_dids.contains(bob.did()));
    assert!(all_dids.contains(carol.did()));
    assert!(all_dids.contains(dave.did()));
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

#[test]
fn test_cache_concurrent_access() {
    use std::thread;

    let cache = Arc::new(TrustCache::with_config(1000, Duration::from_secs(300)));

    let mut handles = vec![];

    // Spawn multiple threads doing cache operations
    for i in 0..10 {
        let cache = cache.clone();
        let handle = thread::spawn(move || {
            let kp = KeyPair::generate().unwrap();
            let score = 0.1 * i as f64;

            cache.put(kp.did().clone(), score);
            let retrieved = cache.get(kp.did());

            assert_eq!(retrieved, Some(score));
            kp.did().clone()
        });
        handles.push(handle);
    }

    let mut dids = vec![];
    for handle in handles {
        dids.push(handle.join().unwrap());
    }

    // All DIDs should still be in cache
    for did in dids {
        assert!(cache.get(&did).is_some());
    }
}

// ============================================================================
// Trust Graph Type Tests
// ============================================================================

#[test]
fn test_trust_graph_type_weights_sum_to_one() {
    for graph_type in TrustGraphType::all() {
        let weights = graph_type.default_weights();
        let sum = weights.direct + weights.transitive;
        assert!(
            (sum - 1.0).abs() < 0.001,
            "{graph_type:?} weights don't sum to 1.0: {sum}"
        );
    }
}

#[test]
fn test_trust_graph_type_storage_isolation() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    // Create separate graphs for each type
    let mut social = TrustGraph::new_with_prefix(
        store.clone(),
        alice.did().clone(),
        TrustGraphType::Social.storage_prefix(),
    );
    let mut economic = TrustGraph::new_with_prefix(
        store.clone(),
        alice.did().clone(),
        TrustGraphType::EconomicReliability.storage_prefix(),
    );
    let mut technical = TrustGraph::new_with_prefix(
        store,
        alice.did().clone(),
        TrustGraphType::TechnicalReliability.storage_prefix(),
    );

    // Add different scores to each
    social
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.9))
        .unwrap();
    economic
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.5))
        .unwrap();
    technical
        .add_edge(TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.3))
        .unwrap();

    // Verify isolation
    let social_edge = social.get_edge(alice.did(), bob.did()).unwrap().unwrap();
    let economic_edge = economic.get_edge(alice.did(), bob.did()).unwrap().unwrap();
    let technical_edge = technical.get_edge(alice.did(), bob.did()).unwrap().unwrap();

    assert!((social_edge.score - 0.9).abs() < 0.001);
    assert!((economic_edge.score - 0.5).abs() < 0.001);
    assert!((technical_edge.score - 0.3).abs() < 0.001);
}

// ============================================================================
// Evidence Validation Tests
// ============================================================================

#[test]
fn test_add_edge_validated_with_legacy_evidence() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store.clone(), alice.did().clone());

    // Create validator that accepts legacy evidence
    let config = EvidenceValidatorConfig {
        accept_legacy: true,
        ..Default::default()
    };
    let validator = EvidenceValidator::with_config(store, config);

    // Create edge with legacy evidence
    let edge = TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.7).with_evidence(
        TrustEvidence::from_legacy_string("test-evidence".to_string()),
    );

    // Add with validation
    let result = graph.add_edge_validated(edge, &validator).unwrap();

    // Should succeed since we accept legacy evidence
    assert!(result.valid);

    // Verify edge was added
    let retrieved = graph.get_edge(alice.did(), bob.did()).unwrap();
    assert!(retrieved.is_some());
    assert!((retrieved.unwrap().score - 0.7).abs() < 0.001);
}

#[test]
fn test_add_edge_validated_reject_legacy_evidence() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store.clone(), alice.did().clone());

    // Create validator that rejects legacy evidence
    let config = EvidenceValidatorConfig {
        accept_legacy: false,
        ..Default::default()
    };
    let validator = EvidenceValidator::with_config(store, config);

    // Create edge with legacy evidence
    let edge = TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.7).with_evidence(
        TrustEvidence::from_legacy_string("test-evidence".to_string()),
    );

    // Add with validation
    let result = graph.add_edge_validated(edge, &validator).unwrap();

    // Should fail since we don't accept legacy evidence
    assert!(!result.valid);
    assert_eq!(result.errors.len(), 1);

    // Verify edge was NOT added
    let retrieved = graph.get_edge(alice.did(), bob.did()).unwrap();
    assert!(retrieved.is_none());
}

#[test]
fn test_add_edge_validated_empty_evidence() {
    let store = Arc::new(SledStore::temporary().unwrap());
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut graph = TrustGraph::new(store.clone(), alice.did().clone());

    // Create validator with default config
    let validator = EvidenceValidator::new(store);

    // Create edge without evidence
    let edge = TrustEdge::new(alice.did().clone(), bob.did().clone(), 0.5);

    // Add with validation
    let result = graph.add_edge_validated(edge, &validator).unwrap();

    // Should succeed - empty evidence is valid
    assert!(result.valid);

    // Verify edge was added
    let retrieved = graph.get_edge(alice.did(), bob.did()).unwrap();
    assert!(retrieved.is_some());
}
