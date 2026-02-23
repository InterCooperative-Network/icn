//! [`CapabilitySource`] trait and [`GraphBuilder`] for assembling capability graphs
//! from one or more domain-specific sources.
//!
//! Implementations of `CapabilitySource` will live in `icn-authz::adapters` (Phase B1).
//! The trait is defined here so B0 can write tests with mock sources.

use crate::model::edge::{CapabilityEdge, CapabilityGraph};
use crate::model::ids::SubjectId;

// ---------------------------------------------------------------------------
// CapabilitySource trait
// ---------------------------------------------------------------------------

/// An adapter that produces capability edges from a domain-specific source.
///
/// Implementations live in `icn-authz::adapters` (Phase B1).
/// The trait is defined here so B0 can write tests with mock sources.
pub trait CapabilitySource: Send + Sync {
    /// Return all capability edges this source knows about for the given subject.
    fn edges_for_subject(&self, subject: &SubjectId) -> Vec<CapabilityEdge>;

    /// Return all capability edges this source can produce.
    fn all_edges(&self) -> Vec<CapabilityEdge>;
}

// ---------------------------------------------------------------------------
// GraphBuilder
// ---------------------------------------------------------------------------

/// Assembles a [`CapabilityGraph`] from zero or more [`CapabilitySource`]s.
///
/// Builder pattern: create with `new()`, add sources, then `build()`.
/// The resulting graph is sorted and deduplicated (canonical).
pub struct GraphBuilder {
    sources: Vec<Box<dyn CapabilitySource>>,
}

impl GraphBuilder {
    /// Create a new builder with no sources.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Add a capability source. Builder pattern -- returns self.
    pub fn add_source(mut self, source: Box<dyn CapabilitySource>) -> Self {
        self.sources.push(source);
        self
    }

    /// Poll all sources via `all_edges()` and build a canonical graph.
    ///
    /// The resulting [`CapabilityGraph`] is sorted and deduplicated.
    pub fn build(&self) -> CapabilityGraph {
        let edges: Vec<CapabilityEdge> = self.sources.iter().flat_map(|s| s.all_edges()).collect();
        CapabilityGraph::from_edges(edges)
    }

    /// Poll all sources for edges matching `subject` and build a canonical graph.
    ///
    /// The resulting [`CapabilityGraph`] is sorted and deduplicated.
    pub fn build_for_subject(&self, subject: &SubjectId) -> CapabilityGraph {
        let edges: Vec<CapabilityEdge> = self
            .sources
            .iter()
            .flat_map(|s| s.edges_for_subject(subject))
            .collect();
        CapabilityGraph::from_edges(edges)
    }
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::ids::{Action, Constraint, EdgeSource, ResourceId, ResourceKind};

    // -- Mock source --------------------------------------------------------

    struct MockSource {
        edges: Vec<CapabilityEdge>,
    }

    impl CapabilitySource for MockSource {
        fn edges_for_subject(&self, subject: &SubjectId) -> Vec<CapabilityEdge> {
            self.edges
                .iter()
                .filter(|e| e.subject == *subject)
                .cloned()
                .collect()
        }

        fn all_edges(&self) -> Vec<CapabilityEdge> {
            self.edges.clone()
        }
    }

    // -- Test helpers -------------------------------------------------------

    fn alice() -> SubjectId {
        SubjectId::new("did:icn:alice").unwrap()
    }

    fn bob() -> SubjectId {
        SubjectId::new("did:icn:bob").unwrap()
    }

    fn propose() -> Action {
        Action::new("governance:propose").unwrap()
    }

    fn coop_resource() -> ResourceId {
        ResourceId::new(ResourceKind::Entity, "coop-1")
    }

    fn make_edge(subject: SubjectId, action: Action) -> CapabilityEdge {
        CapabilityEdge::new(
            subject,
            action,
            coop_resource(),
            vec![Constraint::RateLimit(10)],
            EdgeSource::Static("bootstrap".into()),
            None,
        )
    }

    // -- Tests --------------------------------------------------------------

    #[test]
    fn empty_builder_produces_empty_graph() {
        let graph = GraphBuilder::new().build();
        assert!(graph.edges().is_empty());
        let decision = graph.query(&alice(), &propose(), &coop_resource());
        assert!(!decision.allowed);
    }

    #[test]
    fn single_source_produces_edges() {
        let source = MockSource {
            edges: vec![make_edge(alice(), propose())],
        };
        let graph = GraphBuilder::new().add_source(Box::new(source)).build();
        assert_eq!(graph.edges().len(), 1);
        let decision = graph.query(&alice(), &propose(), &coop_resource());
        assert!(decision.allowed);
    }

    #[test]
    fn two_sources_with_overlapping_edges_deduped() {
        let edge = make_edge(alice(), propose());
        let source_a = MockSource {
            edges: vec![edge.clone()],
        };
        let source_b = MockSource { edges: vec![edge] };
        let graph = GraphBuilder::new()
            .add_source(Box::new(source_a))
            .add_source(Box::new(source_b))
            .build();
        assert_eq!(graph.edges().len(), 1);
    }

    #[test]
    fn two_sources_with_different_edges_merged() {
        let source_a = MockSource {
            edges: vec![make_edge(alice(), propose())],
        };
        let source_b = MockSource {
            edges: vec![make_edge(bob(), propose())],
        };
        let graph = GraphBuilder::new()
            .add_source(Box::new(source_a))
            .add_source(Box::new(source_b))
            .build();
        assert_eq!(graph.edges().len(), 2);
    }

    #[test]
    fn build_for_subject_returns_subset() {
        let source = MockSource {
            edges: vec![make_edge(alice(), propose()), make_edge(bob(), propose())],
        };
        let graph = GraphBuilder::new()
            .add_source(Box::new(source))
            .build_for_subject(&alice());
        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.edges()[0].subject, alice());
    }

    #[test]
    fn build_same_edges_different_order_same_hash() {
        let edge_a = make_edge(alice(), propose());
        let edge_b = make_edge(bob(), propose());

        let source_ab = MockSource {
            edges: vec![edge_a.clone(), edge_b.clone()],
        };
        let source_ba = MockSource {
            edges: vec![edge_b, edge_a],
        };

        let graph_ab = GraphBuilder::new().add_source(Box::new(source_ab)).build();
        let graph_ba = GraphBuilder::new().add_source(Box::new(source_ba)).build();

        assert_eq!(graph_ab.hash(), graph_ba.hash());
    }
}
