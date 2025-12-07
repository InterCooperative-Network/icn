//! Typed trust graph wrapper
//!
//! Provides a type-safe wrapper around `TrustGraph` that enforces
//! consistent use of scoring weights for a specific trust dimension.

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use std::sync::Arc;
use std::time::Duration;

use crate::types::{ScoringWeights, TrustGraphType};
use crate::{TrustClass, TrustEdge, TrustGraph};

/// A trust graph with explicit type designation
///
/// This wrapper ensures that:
/// - All edges are stored in a type-specific namespace
/// - Trust scores are computed with consistent weights
/// - Metrics can be attributed to the correct graph type
///
/// # Example
/// ```ignore
/// use icn_trust::{TypedTrustGraph, TrustGraphType};
///
/// // Create an economic reliability graph
/// let economic = TypedTrustGraph::new(store, own_did, TrustGraphType::EconomicReliability);
///
/// // Add an edge (will be stored in trust/economic/edges/...)
/// economic.add_edge(edge)?;
///
/// // Compute score using 80/20 direct/transitive weighting
/// let score = economic.compute_trust_score(&target)?;
/// ```
pub struct TypedTrustGraph {
    graph_type: TrustGraphType,
    inner: TrustGraph,
    weights: ScoringWeights,
}

impl TypedTrustGraph {
    /// Create a new typed trust graph with default cache settings
    pub fn new(store: Arc<dyn Store>, own_did: Did, graph_type: TrustGraphType) -> Self {
        let weights = graph_type.default_weights();
        Self {
            graph_type,
            inner: TrustGraph::new_with_prefix(store, own_did, graph_type.storage_prefix()),
            weights,
        }
    }

    /// Create a new typed trust graph with custom cache configuration
    pub fn with_cache_config(
        store: Arc<dyn Store>,
        own_did: Did,
        graph_type: TrustGraphType,
        cache_size: usize,
        cache_ttl: Duration,
    ) -> Self {
        let weights = graph_type.default_weights();
        Self {
            graph_type,
            inner: TrustGraph::with_prefix_and_cache(
                store,
                own_did,
                graph_type.storage_prefix(),
                cache_size,
                cache_ttl,
            ),
            weights,
        }
    }

    /// Returns the type of this trust graph
    pub fn graph_type(&self) -> TrustGraphType {
        self.graph_type
    }

    /// Returns the scoring weights used by this graph
    pub fn weights(&self) -> ScoringWeights {
        self.weights
    }

    /// Returns the DID of this node
    pub fn own_did(&self) -> &Did {
        self.inner.own_did()
    }

    /// Returns the storage prefix used by this graph
    pub fn storage_prefix(&self) -> &str {
        self.inner.storage_prefix()
    }

    // ============================================================
    // Edge Management (delegated to inner graph)
    // ============================================================

    /// Add or update a trust edge
    pub fn add_edge(&mut self, edge: TrustEdge) -> Result<()> {
        self.inner.add_edge(edge)
    }

    /// Get a trust edge
    pub fn get_edge(&self, source: &Did, target: &Did) -> Result<Option<TrustEdge>> {
        self.inner.get_edge(source, target)
    }

    /// Get all outgoing edges from a DID
    pub fn get_outgoing_edges(&self, source: &Did) -> Result<Vec<TrustEdge>> {
        self.inner.get_outgoing_edges(source)
    }

    /// Remove a trust edge
    pub fn remove_edge(&mut self, source: &Did, target: &Did) -> Result<()> {
        self.inner.remove_edge(source, target)
    }

    // ============================================================
    // Trust Computation (using type-specific weights)
    // ============================================================

    /// Compute trust score using this graph type's weights
    ///
    /// Different graph types use different weighting:
    /// - **Social**: 60/40 (direct/transitive) - Reputation spreads
    /// - **Economic**: 80/20 - Your payment history matters most
    /// - **Technical**: 90/10 - Your node's performance is yours
    pub fn compute_trust_score(&self, target: &Did) -> Result<f64> {
        self.inner
            .compute_trust_score_weighted(target, self.weights)
    }

    /// Get the trust class for a DID
    pub fn trust_class(&self, did: &Did) -> Result<TrustClass> {
        let score = self.compute_trust_score(did)?;
        Ok(TrustClass::from_score(score))
    }

    // ============================================================
    // Query Methods
    // ============================================================

    /// Get all known DIDs from this trust graph
    pub fn get_all_known_dids(&self) -> Result<Vec<Did>> {
        self.inner.get_all_known_dids()
    }

    /// Get all DIDs that meet a minimum trust threshold
    pub fn get_dids_above_threshold(&self, threshold: f64) -> Result<Vec<Did>> {
        // Note: This uses our weighted scoring
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

    // ============================================================
    // Cache Management
    // ============================================================

    /// Clear the trust score cache
    pub fn clear_cache(&self) {
        self.inner.clear_cache()
    }

    // ============================================================
    // Recovery Support
    // ============================================================

    /// Map trust relationships from old_did to new_did during recovery
    pub fn map_did_recovery(&mut self, old_did: &Did, new_did: &Did) -> Result<usize> {
        self.inner.map_did_recovery(old_did, new_did)
    }

    // ============================================================
    // Advanced Access (for TrustGraphFacade)
    // ============================================================

    /// Get mutable access to the underlying TrustGraph
    ///
    /// This is primarily used by `TrustGraphFacade` for operations
    /// that need direct access to the inner graph.
    pub fn inner_mut(&mut self) -> &mut TrustGraph {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    #[test]
    fn test_typed_graph_creation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();

        let social = TypedTrustGraph::new(store.clone(), alice.clone(), TrustGraphType::Social);
        assert_eq!(social.graph_type(), TrustGraphType::Social);
        assert_eq!(social.storage_prefix(), "trust/social");
        assert!((social.weights().direct - 0.6).abs() < 0.001);
        assert!((social.weights().transitive - 0.4).abs() < 0.001);

        let economic = TypedTrustGraph::new(
            store.clone(),
            alice.clone(),
            TrustGraphType::EconomicReliability,
        );
        assert_eq!(economic.graph_type(), TrustGraphType::EconomicReliability);
        assert_eq!(economic.storage_prefix(), "trust/economic");
        assert!((economic.weights().direct - 0.8).abs() < 0.001);
        assert!((economic.weights().transitive - 0.2).abs() < 0.001);

        let technical = TypedTrustGraph::new(store, alice, TrustGraphType::TechnicalReliability);
        assert_eq!(technical.graph_type(), TrustGraphType::TechnicalReliability);
        assert_eq!(technical.storage_prefix(), "trust/technical");
        assert!((technical.weights().direct - 0.9).abs() < 0.001);
        assert!((technical.weights().transitive - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_typed_graph_isolation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create two different graph types
        let mut social = TypedTrustGraph::new(store.clone(), alice.clone(), TrustGraphType::Social);
        let mut economic = TypedTrustGraph::new(
            store.clone(),
            alice.clone(),
            TrustGraphType::EconomicReliability,
        );

        // Add edge to social graph
        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.8);
        social.add_edge(edge).unwrap();

        // Verify edge exists in social graph
        assert!(social.get_edge(&alice, &bob).unwrap().is_some());

        // Verify edge does NOT exist in economic graph (different namespace)
        assert!(economic.get_edge(&alice, &bob).unwrap().is_none());

        // Add different edge to economic graph
        let economic_edge = TrustEdge::new(alice.clone(), bob.clone(), 0.5);
        economic.add_edge(economic_edge).unwrap();

        // Both graphs now have edges, but with different scores
        assert_eq!(social.get_edge(&alice, &bob).unwrap().unwrap().score, 0.8);
        assert_eq!(economic.get_edge(&alice, &bob).unwrap().unwrap().score, 0.5);
    }

    #[test]
    fn test_typed_graph_weighted_scoring() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create social graph (60% direct, 40% transitive)
        let mut social = TypedTrustGraph::new(store.clone(), alice.clone(), TrustGraphType::Social);

        // Add direct edge with score 0.5
        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.5);
        social.add_edge(edge).unwrap();

        // Compute score: 0.5 * 0.6 (direct) + 0 * 0.4 (no transitive) = 0.3
        let score = social.compute_trust_score(&bob).unwrap();
        assert!((score - 0.3).abs() < 0.001);

        // Create technical graph (90% direct, 10% transitive)
        let mut technical = TypedTrustGraph::new(
            store.clone(),
            alice.clone(),
            TrustGraphType::TechnicalReliability,
        );

        // Add same edge
        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.5);
        technical.add_edge(edge).unwrap();

        // Compute score: 0.5 * 0.9 (direct) + 0 * 0.1 (no transitive) = 0.45
        let score = technical.compute_trust_score(&bob).unwrap();
        assert!((score - 0.45).abs() < 0.001);
    }
}
