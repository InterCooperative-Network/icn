//! Cross-module integration tests for icn-authz capability graph.
//!
//! These tests exercise the full pipeline: construct edges from multiple
//! `CapabilitySource` implementations, build graphs via `GraphBuilder`,
//! query decisions, and verify deterministic hashing across configurations.

#![allow(clippy::unwrap_used)]

use icn_authz::*;

// ---------------------------------------------------------------------------
// FixedSource -- a trivial CapabilitySource for integration testing
// ---------------------------------------------------------------------------

struct FixedSource(Vec<CapabilityEdge>);

impl CapabilitySource for FixedSource {
    fn edges_for_subject(&self, subject: &SubjectId) -> Vec<CapabilityEdge> {
        self.0
            .iter()
            .filter(|e| e.subject == *subject)
            .cloned()
            .collect()
    }

    fn all_edges(&self) -> Vec<CapabilityEdge> {
        self.0.clone()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn alice() -> SubjectId {
    SubjectId::new("did:icn:alice").unwrap()
}
fn bob() -> SubjectId {
    SubjectId::new("did:icn:bob").unwrap()
}
fn carol() -> SubjectId {
    SubjectId::new("did:icn:carol").unwrap()
}
fn propose() -> Action {
    Action::new("governance:propose").unwrap()
}
fn vote() -> Action {
    Action::new("governance:vote").unwrap()
}
fn transfer() -> Action {
    Action::new("ledger:transfer").unwrap()
}
fn coop() -> ResourceId {
    ResourceId::new(ResourceKind::Entity, "coop-1")
}

fn make_edge(
    subject: SubjectId,
    action: Action,
    resource: ResourceId,
    source: EdgeSource,
) -> CapabilityEdge {
    CapabilityEdge::new(
        subject,
        action,
        resource,
        vec![Constraint::RateLimit(10)],
        source,
        None,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Build a graph, serialize its edges to JSON, deserialize, rebuild.
/// The two graphs must have identical hashes (canonical ordering survives serde).
#[test]
fn edges_sorted_after_serde_roundtrip() {
    // Intentionally insert in non-sorted order: carol, alice, bob
    let edges = vec![
        make_edge(carol(), vote(), coop(), EdgeSource::Static("c".into())),
        make_edge(
            alice(),
            propose(),
            coop(),
            EdgeSource::CclContract("contract-1".into()),
        ),
        make_edge(
            bob(),
            transfer(),
            coop(),
            EdgeSource::TrustScore("high".into()),
        ),
    ];

    let graph_a = CapabilityGraph::from_edges(edges.clone());
    let hash_a = graph_a.hash();

    // Round-trip through JSON
    let json = serde_json::to_string(graph_a.edges()).unwrap();
    let deserialized: Vec<CapabilityEdge> = serde_json::from_str(&json).unwrap();
    let graph_b = CapabilityGraph::from_edges(deserialized);

    assert_eq!(hash_a, graph_b.hash());
    assert_eq!(graph_a.edges(), graph_b.edges());
}

/// Two independent sources feed edges into a single graph via GraphBuilder.
/// Verify edge count, and query results for each subject.
#[test]
fn full_pipeline_two_sources() {
    let source_1 = FixedSource(vec![
        make_edge(
            alice(),
            propose(),
            coop(),
            EdgeSource::CclContract("contract-1".into()),
        ),
        make_edge(
            bob(),
            vote(),
            coop(),
            EdgeSource::TrustScore("medium".into()),
        ),
    ]);

    let treasury = ResourceId::new(ResourceKind::Asset, "treasury");
    let source_2 = FixedSource(vec![make_edge(
        carol(),
        transfer(),
        treasury.clone(),
        EdgeSource::GovernanceVote("proposal-42".into()),
    )]);

    let graph = GraphBuilder::new()
        .add_source(Box::new(source_1))
        .add_source(Box::new(source_2))
        .build();

    assert_eq!(graph.edges().len(), 3);

    // Alice can propose on coop
    let d = graph.query(&alice(), &propose(), &coop());
    assert!(d.allowed);

    // Bob can vote on coop
    let d = graph.query(&bob(), &vote(), &coop());
    assert!(d.allowed);

    // Carol can transfer on treasury
    let d = graph.query(&carol(), &transfer(), &treasury);
    assert!(d.allowed);

    // Alice cannot vote on coop (no such edge)
    let d = graph.query(&alice(), &vote(), &coop());
    assert!(!d.allowed);
    assert!(d.matching_edges.is_empty());
}

/// Same set of edges, different source configurations (one source vs two).
/// The resulting graph hash must be identical.
#[test]
fn hash_stable_across_source_configurations() {
    let edge_a = make_edge(
        alice(),
        propose(),
        coop(),
        EdgeSource::CclContract("c-1".into()),
    );
    let edge_b = make_edge(bob(), vote(), coop(), EdgeSource::TrustScore("high".into()));

    // Config A: one source with both edges
    let source_both = FixedSource(vec![edge_a.clone(), edge_b.clone()]);
    let graph_a = GraphBuilder::new()
        .add_source(Box::new(source_both))
        .build();

    // Config B: two sources, one edge each
    let source_a = FixedSource(vec![edge_a]);
    let source_b = FixedSource(vec![edge_b]);
    let graph_b = GraphBuilder::new()
        .add_source(Box::new(source_a))
        .add_source(Box::new(source_b))
        .build();

    assert_eq!(graph_a.hash(), graph_b.hash());
    assert_eq!(graph_a.edges(), graph_b.edges());
}

/// `build_for_subject` returns only edges matching the requested subject.
#[test]
fn build_for_subject_correct_subset() {
    let source = FixedSource(vec![
        make_edge(
            alice(),
            propose(),
            coop(),
            EdgeSource::CclContract("c-1".into()),
        ),
        make_edge(
            alice(),
            vote(),
            coop(),
            EdgeSource::TrustScore("high".into()),
        ),
        make_edge(
            bob(),
            propose(),
            coop(),
            EdgeSource::Static("bootstrap".into()),
        ),
    ]);

    let graph = GraphBuilder::new()
        .add_source(Box::new(source))
        .build_for_subject(&alice());

    assert_eq!(graph.edges().len(), 2);
    for edge in graph.edges() {
        assert_eq!(edge.subject, alice());
    }
}

/// When two edges match the same (subject, action, resource) but differ in
/// constraints or source, query returns both matching edge indices.
#[test]
fn query_returns_multiple_matching_edges() {
    let edge_a = CapabilityEdge::new(
        alice(),
        propose(),
        coop(),
        vec![Constraint::RateLimit(10)],
        EdgeSource::CclContract("contract-1".into()),
        None,
    );
    let edge_b = CapabilityEdge::new(
        alice(),
        propose(),
        coop(),
        vec![Constraint::MaxTopics(5)],
        EdgeSource::GovernanceVote("proposal-7".into()),
        None,
    );

    let graph = CapabilityGraph::from_edges(vec![edge_a, edge_b]);

    let d = graph.query(&alice(), &propose(), &coop());
    assert!(d.allowed);
    assert_eq!(d.matching_edges.len(), 2);
}

/// An empty graph denies every query.
#[test]
fn query_denied_on_empty_graph() {
    let graph = CapabilityGraph::from_edges(vec![]);
    let d = graph.query(&alice(), &propose(), &coop());
    assert!(!d.allowed);
    assert!(d.matching_edges.is_empty());
}
