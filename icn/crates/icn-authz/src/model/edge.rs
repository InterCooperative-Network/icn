//! Capability edges and the computed capability graph.
//!
//! A [`CapabilityEdge`] is the fundamental unit: subject S may perform action A
//! on resource R, subject to constraints C, granted by source E, valid at block H.
//!
//! A [`CapabilityGraph`] is a sorted, deduplicated collection of edges with
//! a simple query interface.

use serde::{Deserialize, Serialize};

use super::ids::{Action, BlockHeight, Constraint, EdgeSource, ResourceId, SubjectId};

// ---------------------------------------------------------------------------
// CapabilityEdge
// ---------------------------------------------------------------------------

/// A single capability grant: subject may perform action on resource.
///
/// Constraints are always sorted and deduplicated (canonicalized in constructor).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CapabilityEdge {
    pub subject: SubjectId,
    pub action: Action,
    pub resource: ResourceId,
    /// INVARIANT: always sorted and deduplicated.
    pub constraints: Vec<Constraint>,
    pub source: EdgeSource,
    pub valid_at: Option<BlockHeight>,
}

impl CapabilityEdge {
    /// Create a new edge, sorting and deduplicating constraints.
    pub fn new(
        subject: SubjectId,
        action: Action,
        resource: ResourceId,
        mut constraints: Vec<Constraint>,
        source: EdgeSource,
        valid_at: Option<BlockHeight>,
    ) -> Self {
        constraints.sort();
        constraints.dedup();
        Self {
            subject,
            action,
            resource,
            constraints,
            source,
            valid_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Decision
// ---------------------------------------------------------------------------

/// The result of a capability graph query (B0 minimal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    /// Whether the action is allowed.
    pub allowed: bool,
    /// Indices into `CapabilityGraph::edges()` that matched the query.
    pub matching_edges: Vec<usize>,
}

// ---------------------------------------------------------------------------
// CapabilityGraph
// ---------------------------------------------------------------------------

/// A computed, canonical capability graph.
///
/// Edges are sorted and deduplicated. Not serializable -- use edges directly
/// if you need to persist.
#[derive(Clone, Debug)]
pub struct CapabilityGraph {
    edges: Vec<CapabilityEdge>,
}

impl CapabilityGraph {
    /// Build a graph from a collection of edges, sorting and deduplicating.
    pub fn from_edges(mut edges: Vec<CapabilityEdge>) -> Self {
        edges.sort();
        edges.dedup();
        Self { edges }
    }

    /// Borrow the canonical edge list.
    pub fn edges(&self) -> &[CapabilityEdge] {
        &self.edges
    }

    /// Query whether `subject` may perform `action` on `resource`.
    ///
    /// Returns a [`Decision`] with matching edge indices. In B0, a match
    /// requires exact equality on subject, action, and resource.
    pub fn query(&self, subject: &SubjectId, action: &Action, resource: &ResourceId) -> Decision {
        let matching_edges: Vec<usize> = self
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.subject == *subject && e.action == *action && e.resource == *resource
            })
            .map(|(i, _)| i)
            .collect();

        Decision {
            allowed: !matching_edges.is_empty(),
            matching_edges,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::ids::{EdgeSource, ResourceKind};

    fn make_subject() -> SubjectId {
        SubjectId::new("did:icn:alice").unwrap()
    }

    fn make_action() -> Action {
        Action::new("ledger:transfer").unwrap()
    }

    fn make_resource() -> ResourceId {
        ResourceId::new(ResourceKind::Asset, "credit-line-1")
    }

    fn make_edge(constraints: Vec<Constraint>) -> CapabilityEdge {
        CapabilityEdge::new(
            make_subject(),
            make_action(),
            make_resource(),
            constraints,
            EdgeSource::Static("test".into()),
            None,
        )
    }

    // -- CapabilityEdge -----------------------------------------------------

    #[test]
    fn constructor_sorts_constraints() {
        let edge = make_edge(vec![Constraint::MaxTopics(5), Constraint::RateLimit(100)]);
        assert_eq!(
            edge.constraints,
            vec![Constraint::RateLimit(100), Constraint::MaxTopics(5)]
        );
    }

    #[test]
    fn constructor_dedupes_constraints() {
        let edge = make_edge(vec![
            Constraint::RateLimit(100),
            Constraint::MaxTopics(5),
            Constraint::RateLimit(100),
        ]);
        assert_eq!(
            edge.constraints,
            vec![Constraint::RateLimit(100), Constraint::MaxTopics(5)]
        );
    }

    // -- CapabilityGraph query ----------------------------------------------

    #[test]
    fn query_allowed_when_edge_exists() {
        let graph = CapabilityGraph::from_edges(vec![make_edge(vec![])]);
        let decision = graph.query(&make_subject(), &make_action(), &make_resource());
        assert!(decision.allowed);
        assert_eq!(decision.matching_edges, vec![0]);
    }

    #[test]
    fn query_denied_when_no_matching_edge() {
        let graph = CapabilityGraph::from_edges(vec![make_edge(vec![])]);
        let other_subject = SubjectId::new("did:icn:bob").unwrap();
        let decision = graph.query(&other_subject, &make_action(), &make_resource());
        assert!(!decision.allowed);
        assert!(decision.matching_edges.is_empty());
    }

    #[test]
    fn query_denied_on_empty_graph() {
        let graph = CapabilityGraph::from_edges(vec![]);
        let decision = graph.query(&make_subject(), &make_action(), &make_resource());
        assert!(!decision.allowed);
        assert!(decision.matching_edges.is_empty());
    }

    // -- Serde roundtrip ----------------------------------------------------

    #[test]
    fn capability_edge_serde_roundtrip() {
        let edge = make_edge(vec![Constraint::RateLimit(50), Constraint::MaxTopics(3)]);
        let json = serde_json::to_string(&edge).unwrap();
        let back: CapabilityEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(edge, back);
    }
}
