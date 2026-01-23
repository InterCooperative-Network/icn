//! Backward-compatible facade for gradual migration
//!
//! This module provides a facade that wraps `MultiTrustGraph` while exposing
//! the original `TrustGraph` API. This allows existing code to continue working
//! unchanged while new code can opt into typed access.
//!
//! # Migration Strategy
//!
//! 1. **Phase 1**: Replace `TrustGraph` with `TrustGraphFacade` in supervisor
//!    - All existing code continues to work unchanged
//!    - Combined scores used for backward compatibility
//!
//! 2. **Phase 2**: Update consumers one by one to use typed access
//!    - `icn-ledger` → `facade.economic()` for credit limits
//!    - `icn-compute` → `facade.technical()` for task scheduling
//!    - `icn-net` → `facade.social()` for rate limiting
//!
//! 3. **Phase 3**: Remove facade, use `MultiTrustGraph` directly
//!
//! # Example
//!
//! ```ignore
//! use icn_trust::{TrustGraphFacade, TrustGraphType};
//!
//! // Create facade (drop-in replacement for TrustGraph)
//! let mut facade = TrustGraphFacade::new(store, own_did);
//!
//! // Old API: works unchanged
//! facade.add_edge(edge)?;
//! let score = facade.compute_trust_score(&target)?;
//! let class = facade.trust_class(&target)?;
//!
//! // New API: typed access for domain-specific operations
//! let economic = facade.economic();
//! let credit_score = economic.compute_trust_score(&member)?;
//!
//! let technical = facade.technical();
//! let node_reliability = technical.compute_trust_score(&executor)?;
//! ```

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use std::sync::Arc;
use std::time::Duration;

use crate::multi_graph::MultiTrustGraph;
use crate::typed_graph::TypedTrustGraph;
use crate::types::TrustGraphType;
use crate::{TrustClass, TrustEdge};

/// Backward-compatible facade wrapping MultiTrustGraph
///
/// This provides the original `TrustGraph` API while internally managing
/// three separate trust graphs. Edges are routed to the appropriate graph
/// based on their `graph_type` field (defaulting to Social).
///
/// # Backward Compatibility
///
/// - `add_edge()` routes edges based on `edge.graph_type`
/// - `compute_trust_score()` returns combined score (50% social + 30% economic + 20% technical)
/// - `trust_class()` uses combined score
///
/// # Typed Access
///
/// New code should use typed access for domain-appropriate scoring:
/// - `facade.social()` for connection priority, gossip bandwidth
/// - `facade.economic()` for credit limits, dispute weighting
/// - `facade.technical()` for compute scheduling, storage selection
pub struct TrustGraphFacade {
    multi: MultiTrustGraph,
}

impl TrustGraphFacade {
    /// Create a new facade with default cache settings
    pub fn new(store: Arc<dyn Store>, own_did: Did) -> Self {
        Self {
            multi: MultiTrustGraph::new(store, own_did),
        }
    }

    /// Create a new facade with custom cache configuration
    pub fn with_cache_config(
        store: Arc<dyn Store>,
        own_did: Did,
        cache_size: usize,
        cache_ttl: Duration,
    ) -> Self {
        Self {
            multi: MultiTrustGraph::with_cache_config(store, own_did, cache_size, cache_ttl),
        }
    }

    /// Returns the DID of this node
    pub fn own_did(&self) -> &Did {
        self.multi.own_did()
    }

    // ============================================================
    // Old API (Backward Compatibility)
    // ============================================================

    /// Add or update a trust edge
    ///
    /// Routes the edge to the appropriate graph based on `edge.graph_type`.
    /// Defaults to Social graph for edges without explicit type.
    pub fn add_edge(&mut self, edge: TrustEdge) -> Result<()> {
        self.multi.add_edge_to(edge.graph_type, edge)
    }

    /// Get a trust edge from a specific graph
    ///
    /// Checks the graph specified by `graph_type` parameter.
    /// For backward compatibility, defaults to Social graph.
    pub fn get_edge(&self, source: &Did, target: &Did) -> Result<Option<TrustEdge>> {
        // Default to Social for backward compatibility
        self.multi.social().get_edge(source, target)
    }

    /// Get a trust edge from a specific graph type
    pub fn get_edge_from(
        &self,
        graph_type: TrustGraphType,
        source: &Did,
        target: &Did,
    ) -> Result<Option<TrustEdge>> {
        self.multi.graph(graph_type).get_edge(source, target)
    }

    /// Get all outgoing edges from a DID (from Social graph)
    ///
    /// For backward compatibility, returns edges from Social graph only.
    /// Use `get_outgoing_edges_from()` for other graph types.
    pub fn get_outgoing_edges(&self, source: &Did) -> Result<Vec<TrustEdge>> {
        self.multi.social().get_outgoing_edges(source)
    }

    /// Get all outgoing edges from a specific graph type
    pub fn get_outgoing_edges_from(
        &self,
        graph_type: TrustGraphType,
        source: &Did,
    ) -> Result<Vec<TrustEdge>> {
        self.multi.graph(graph_type).get_outgoing_edges(source)
    }

    /// Get all incoming edges to a DID (from Social graph)
    ///
    /// Returns edges where target == did ("who trusts me?").
    /// For backward compatibility, returns edges from Social graph only.
    ///
    /// Note: This operation scans all edges and is O(n). Use sparingly.
    pub fn get_incoming_edges(&self, target: &Did) -> Result<Vec<TrustEdge>> {
        self.multi.social().get_incoming_edges(target)
    }

    /// Get all incoming edges from a specific graph type
    pub fn get_incoming_edges_from(
        &self,
        graph_type: TrustGraphType,
        target: &Did,
    ) -> Result<Vec<TrustEdge>> {
        self.multi.graph(graph_type).get_incoming_edges(target)
    }

    /// Compute combined trust score (backward compatible)
    ///
    /// Returns a weighted average: 50% social + 30% economic + 20% technical
    ///
    /// **Note**: New code should use typed access via `social()`, `economic()`,
    /// or `technical()` for domain-appropriate scoring.
    pub fn compute_trust_score(&self, target: &Did) -> Result<f64> {
        self.multi.compute_combined_trust_score(target)
    }

    /// Get trust class based on combined score (backward compatible)
    pub fn trust_class(&self, did: &Did) -> Result<TrustClass> {
        self.multi.combined_trust_class(did)
    }

    /// Remove an edge from Social graph (backward compatible)
    pub fn remove_edge(&mut self, source: &Did, target: &Did) -> Result<()> {
        self.multi.social_mut().remove_edge(source, target)
    }

    /// Remove an edge from a specific graph type
    pub fn remove_edge_from(
        &mut self,
        graph_type: TrustGraphType,
        source: &Did,
        target: &Did,
    ) -> Result<()> {
        self.multi.graph_mut(graph_type).remove_edge(source, target)
    }

    /// Get all known DIDs across all trust graphs
    pub fn get_all_known_dids(&self) -> Result<Vec<Did>> {
        self.multi.get_all_known_dids()
    }

    /// Get DIDs above a threshold using combined scoring
    pub fn get_dids_above_threshold(&self, threshold: f64) -> Result<Vec<Did>> {
        let all_dids = self.get_all_known_dids()?;
        let mut trusted_dids = Vec::new();

        for did in all_dids {
            if did == *self.own_did() {
                continue;
            }

            let score = self.compute_trust_score(&did)?;
            if score >= threshold {
                trusted_dids.push(did);
            }
        }

        Ok(trusted_dids)
    }

    /// Clear all caches across all graphs
    pub fn clear_cache(&self) {
        self.multi.clear_all_caches();
    }

    /// Map trust relationships during DID recovery
    pub fn map_did_recovery(&mut self, old_did: &Did, new_did: &Did) -> Result<usize> {
        self.multi.map_did_recovery(old_did, new_did)
    }

    // ============================================================
    // New API (Typed Access)
    // ============================================================

    /// Get immutable access to the social trust graph
    ///
    /// Use for: Connection priority, gossip bandwidth, topic access, org membership
    pub fn social(&self) -> &TypedTrustGraph {
        self.multi.social()
    }

    /// Get mutable access to the social trust graph
    pub fn social_mut(&mut self) -> &mut TypedTrustGraph {
        self.multi.social_mut()
    }

    /// Get immutable access to the economic reliability graph
    ///
    /// Use for: Credit limits, dispute weighting, federation trade limits
    pub fn economic(&self) -> &TypedTrustGraph {
        self.multi.economic()
    }

    /// Get mutable access to the economic reliability graph
    pub fn economic_mut(&mut self) -> &mut TypedTrustGraph {
        self.multi.economic_mut()
    }

    /// Get immutable access to the technical reliability graph
    ///
    /// Use for: Compute scheduling, contract execution priority, storage selection
    pub fn technical(&self) -> &TypedTrustGraph {
        self.multi.technical()
    }

    /// Get mutable access to the technical reliability graph
    pub fn technical_mut(&mut self) -> &mut TypedTrustGraph {
        self.multi.technical_mut()
    }

    /// Get a specific graph by type
    pub fn graph(&self, graph_type: TrustGraphType) -> &TypedTrustGraph {
        self.multi.graph(graph_type)
    }

    /// Get mutable access to a specific graph by type
    pub fn graph_mut(&mut self, graph_type: TrustGraphType) -> &mut TypedTrustGraph {
        self.multi.graph_mut(graph_type)
    }

    /// Get the underlying MultiTrustGraph
    ///
    /// For advanced use cases that need direct access to all graphs.
    pub fn inner(&self) -> &MultiTrustGraph {
        &self.multi
    }

    /// Get mutable access to the underlying MultiTrustGraph
    pub fn inner_mut(&mut self) -> &mut MultiTrustGraph {
        &mut self.multi
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrustScore;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    #[test]
    fn test_facade_backward_compatible_api() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut facade = TrustGraphFacade::new(store, alice.clone());

        // Old API: add_edge without explicit graph type (defaults to Social)
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.7));
        facade.add_edge(edge).unwrap();

        // Old API: get_edge returns from Social graph
        let retrieved = facade.get_edge(&alice, &bob).unwrap().unwrap();
        assert!((retrieved.score.value() - 0.7).abs() < 0.001);

        // Old API: compute_trust_score returns combined score
        // With only Social graph populated: 0.7 * 0.6 * 0.5 = 0.21 (Social weight 50%, direct 60%)
        let score = facade.compute_trust_score(&bob).unwrap();
        assert!(score > 0.0);
    }

    #[test]
    fn test_facade_edge_routing() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut facade = TrustGraphFacade::new(store, alice.clone());

        // Add edge to Economic graph explicitly
        let economic_edge = TrustEdge::new_typed(
            alice.clone(),
            bob.clone(),
            TrustScore::unchecked(0.8),
            TrustGraphType::EconomicReliability,
        );
        facade.add_edge(economic_edge).unwrap();

        // Should NOT be in Social graph
        assert!(facade.get_edge(&alice, &bob).unwrap().is_none());

        // Should be in Economic graph
        let from_economic = facade
            .get_edge_from(TrustGraphType::EconomicReliability, &alice, &bob)
            .unwrap();
        assert!(from_economic.is_some());
        assert!((from_economic.unwrap().score.value() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_facade_typed_access() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut facade = TrustGraphFacade::new(store, alice.clone());

        // Use typed access to add edges
        facade
            .social_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.9),
            ))
            .unwrap();
        facade
            .economic_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.5),
            ))
            .unwrap();
        facade
            .technical_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.3),
            ))
            .unwrap();

        // Verify scores through typed access
        let social_score = facade.social().compute_trust_score(&bob).unwrap();
        let economic_score = facade.economic().compute_trust_score(&bob).unwrap();
        let technical_score = facade.technical().compute_trust_score(&bob).unwrap();

        // Social: 0.9 * 0.6 = 0.54
        assert!((social_score - 0.54).abs() < 0.01);
        // Economic: 0.5 * 0.8 = 0.40
        assert!((economic_score - 0.40).abs() < 0.01);
        // Technical: 0.3 * 0.9 = 0.27
        assert!((technical_score - 0.27).abs() < 0.01);
    }

    #[test]
    fn test_facade_combined_score() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut facade = TrustGraphFacade::new(store, alice.clone());

        // Add edges to all graphs
        facade
            .social_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();
        facade
            .economic_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.6),
            ))
            .unwrap();
        facade
            .technical_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.4),
            ))
            .unwrap();

        // Combined score calculation:
        // Social: 0.8 * 0.6 = 0.48
        // Economic: 0.6 * 0.8 = 0.48
        // Technical: 0.4 * 0.9 = 0.36
        // Combined: 0.48 * 0.5 + 0.48 * 0.3 + 0.36 * 0.2 = 0.24 + 0.144 + 0.072 = 0.456
        let combined = facade.compute_trust_score(&bob).unwrap();
        assert!((combined - 0.456).abs() < 0.01);
    }

    #[test]
    fn test_facade_trust_class() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut facade = TrustGraphFacade::new(store, alice.clone());

        // Add high trust in all graphs
        facade
            .social_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(1.0),
            ))
            .unwrap();
        facade
            .economic_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(1.0),
            ))
            .unwrap();
        facade
            .technical_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(1.0),
            ))
            .unwrap();

        // Combined: 0.6*0.5 + 0.8*0.3 + 0.9*0.2 = 0.3 + 0.24 + 0.18 = 0.72
        let class = facade.trust_class(&bob).unwrap();
        assert_eq!(class, TrustClass::Federated);
    }

    #[test]
    fn test_facade_recovery() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let new_alice = KeyPair::generate().unwrap().did().clone();

        let mut facade = TrustGraphFacade::new(store, alice.clone());

        // Add edges in multiple graphs
        facade
            .social_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();
        facade
            .economic_mut()
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.6),
            ))
            .unwrap();

        // Perform recovery
        let migrated = facade.map_did_recovery(&alice, &new_alice).unwrap();
        assert_eq!(migrated, 2); // 2 edges migrated

        // Verify edges migrated
        assert!(facade.social().get_edge(&alice, &bob).unwrap().is_none());
        assert!(facade
            .social()
            .get_edge(&new_alice, &bob)
            .unwrap()
            .is_some());
        assert!(facade.economic().get_edge(&alice, &bob).unwrap().is_none());
        assert!(facade
            .economic()
            .get_edge(&new_alice, &bob)
            .unwrap()
            .is_some());
    }
}
