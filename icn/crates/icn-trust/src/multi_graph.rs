//! Multi-graph trust container
//!
//! Manages all three orthogonal trust graphs (Social, Economic, Technical)
//! and provides both typed access and backward-compatible combined scoring.

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use std::sync::Arc;
use std::time::Duration;

use crate::typed_graph::TypedTrustGraph;
use crate::types::TrustGraphType;
use crate::{TrustClass, TrustEdge};

/// Container managing all three trust graph types
///
/// This provides:
/// - **Typed access**: Get a specific graph by type for domain-specific operations
/// - **Combined scoring**: Backward-compatible weighted average for gradual migration
/// - **Unified storage**: All graphs share the same store but use different prefixes
///
/// # Example
/// ```ignore
/// use icn_trust::{MultiTrustGraph, TrustGraphType};
///
/// let multi = MultiTrustGraph::new(store, own_did);
///
/// // Typed access for domain-specific operations
/// let economic = multi.graph(TrustGraphType::EconomicReliability);
/// let credit_score = economic.compute_trust_score(&member)?;
///
/// // Combined score for backward compatibility
/// let combined = multi.compute_combined_trust_score(&peer)?;
/// ```
pub struct MultiTrustGraph {
    social: TypedTrustGraph,
    economic: TypedTrustGraph,
    technical: TypedTrustGraph,
    own_did: Did,
}

impl MultiTrustGraph {
    /// Create a new multi-graph with default cache settings
    pub fn new(store: Arc<dyn Store>, own_did: Did) -> Self {
        Self {
            social: TypedTrustGraph::new(store.clone(), own_did.clone(), TrustGraphType::Social),
            economic: TypedTrustGraph::new(
                store.clone(),
                own_did.clone(),
                TrustGraphType::EconomicReliability,
            ),
            technical: TypedTrustGraph::new(
                store,
                own_did.clone(),
                TrustGraphType::TechnicalReliability,
            ),
            own_did,
        }
    }

    /// Create a new multi-graph with custom cache configuration
    pub fn with_cache_config(
        store: Arc<dyn Store>,
        own_did: Did,
        cache_size: usize,
        cache_ttl: Duration,
    ) -> Self {
        Self {
            social: TypedTrustGraph::with_cache_config(
                store.clone(),
                own_did.clone(),
                TrustGraphType::Social,
                cache_size,
                cache_ttl,
            ),
            economic: TypedTrustGraph::with_cache_config(
                store.clone(),
                own_did.clone(),
                TrustGraphType::EconomicReliability,
                cache_size,
                cache_ttl,
            ),
            technical: TypedTrustGraph::with_cache_config(
                store,
                own_did.clone(),
                TrustGraphType::TechnicalReliability,
                cache_size,
                cache_ttl,
            ),
            own_did,
        }
    }

    /// Returns the DID of this node
    pub fn own_did(&self) -> &Did {
        &self.own_did
    }

    // ============================================================
    // Typed Access
    // ============================================================

    /// Get immutable access to a specific graph by type
    pub fn graph(&self, graph_type: TrustGraphType) -> &TypedTrustGraph {
        match graph_type {
            TrustGraphType::Social => &self.social,
            TrustGraphType::EconomicReliability => &self.economic,
            TrustGraphType::TechnicalReliability => &self.technical,
        }
    }

    /// Get mutable access to a specific graph by type
    pub fn graph_mut(&mut self, graph_type: TrustGraphType) -> &mut TypedTrustGraph {
        match graph_type {
            TrustGraphType::Social => &mut self.social,
            TrustGraphType::EconomicReliability => &mut self.economic,
            TrustGraphType::TechnicalReliability => &mut self.technical,
        }
    }

    /// Get the social trust graph
    pub fn social(&self) -> &TypedTrustGraph {
        &self.social
    }

    /// Get the social trust graph mutably
    pub fn social_mut(&mut self) -> &mut TypedTrustGraph {
        &mut self.social
    }

    /// Get the economic reliability graph
    pub fn economic(&self) -> &TypedTrustGraph {
        &self.economic
    }

    /// Get the economic reliability graph mutably
    pub fn economic_mut(&mut self) -> &mut TypedTrustGraph {
        &mut self.economic
    }

    /// Get the technical reliability graph
    pub fn technical(&self) -> &TypedTrustGraph {
        &self.technical
    }

    /// Get the technical reliability graph mutably
    pub fn technical_mut(&mut self) -> &mut TypedTrustGraph {
        &mut self.technical
    }

    // ============================================================
    // Combined Scoring (Backward Compatibility)
    // ============================================================

    /// Default weights for combined score calculation
    ///
    /// These weights prioritize social trust (50%) for backward compatibility,
    /// followed by economic (30%) and technical (20%).
    pub const COMBINED_WEIGHTS: (f64, f64, f64) = (0.5, 0.3, 0.2);

    /// Compute a combined trust score across all three graphs
    ///
    /// This provides backward compatibility for consumers that haven't
    /// migrated to typed access yet. The combined score is a weighted
    /// average: 50% social + 30% economic + 20% technical.
    ///
    /// **Note**: New code should prefer typed access via `graph()` for
    /// domain-appropriate scoring.
    pub fn compute_combined_trust_score(&self, target: &Did) -> Result<f64> {
        let social = self.social.compute_trust_score(target).unwrap_or(0.0);
        let economic = self.economic.compute_trust_score(target).unwrap_or(0.0);
        let technical = self.technical.compute_trust_score(target).unwrap_or(0.0);

        let (sw, ew, tw) = Self::COMBINED_WEIGHTS;
        Ok((social * sw + economic * ew + technical * tw).min(1.0))
    }

    /// Get the trust class based on combined score
    pub fn combined_trust_class(&self, did: &Did) -> Result<TrustClass> {
        let score = self.compute_combined_trust_score(did)?;
        Ok(TrustClass::from_score(score))
    }

    // ============================================================
    // Edge Routing
    // ============================================================

    /// Add an edge to the appropriate graph based on its type
    ///
    /// If the edge has a `graph_type` field, it's routed to that graph.
    /// Otherwise, it defaults to the Social graph for backward compatibility.
    pub fn add_edge(&mut self, edge: TrustEdge) -> Result<()> {
        // For now, edges without explicit type default to Social
        // This will be updated when we add graph_type to TrustEdge
        self.social.add_edge(edge)
    }

    /// Add an edge to a specific graph type
    pub fn add_edge_to(&mut self, graph_type: TrustGraphType, edge: TrustEdge) -> Result<()> {
        self.graph_mut(graph_type).add_edge(edge)
    }

    // ============================================================
    // Query Aggregation
    // ============================================================

    /// Get all known DIDs across all trust graphs
    pub fn get_all_known_dids(&self) -> Result<Vec<Did>> {
        use std::collections::HashSet;

        let mut all_dids: HashSet<Did> = HashSet::new();

        for did in self.social.get_all_known_dids()? {
            all_dids.insert(did);
        }
        for did in self.economic.get_all_known_dids()? {
            all_dids.insert(did);
        }
        for did in self.technical.get_all_known_dids()? {
            all_dids.insert(did);
        }

        Ok(all_dids.into_iter().collect())
    }

    // ============================================================
    // Cache Management
    // ============================================================

    /// Clear all trust score caches
    pub fn clear_all_caches(&self) {
        self.social.clear_cache();
        self.economic.clear_cache();
        self.technical.clear_cache();
    }

    // ============================================================
    // Recovery Support
    // ============================================================

    /// Map trust relationships from old_did to new_did across all graphs
    pub fn map_did_recovery(&mut self, old_did: &Did, new_did: &Did) -> Result<usize> {
        let mut total = 0;
        total += self.social.map_did_recovery(old_did, new_did)?;
        total += self.economic.map_did_recovery(old_did, new_did)?;
        total += self.technical.map_did_recovery(old_did, new_did)?;
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    #[test]
    fn test_multi_graph_creation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();

        let multi = MultiTrustGraph::new(store, alice.clone());

        assert_eq!(multi.own_did(), &alice);
        assert_eq!(multi.social().graph_type(), TrustGraphType::Social);
        assert_eq!(
            multi.economic().graph_type(),
            TrustGraphType::EconomicReliability
        );
        assert_eq!(
            multi.technical().graph_type(),
            TrustGraphType::TechnicalReliability
        );
    }

    #[test]
    fn test_multi_graph_typed_access() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();

        let multi = MultiTrustGraph::new(store, alice);

        // Access by type
        assert_eq!(
            multi.graph(TrustGraphType::Social).storage_prefix(),
            "trust/social"
        );
        assert_eq!(
            multi
                .graph(TrustGraphType::EconomicReliability)
                .storage_prefix(),
            "trust/economic"
        );
        assert_eq!(
            multi
                .graph(TrustGraphType::TechnicalReliability)
                .storage_prefix(),
            "trust/technical"
        );
    }

    #[test]
    fn test_multi_graph_isolation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut multi = MultiTrustGraph::new(store, alice.clone());

        // Add different scores to different graphs
        multi
            .social_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.9))
            .unwrap();
        multi
            .economic_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.5))
            .unwrap();
        multi
            .technical_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.3))
            .unwrap();

        // Verify isolation
        let social_edge = multi.social().get_edge(&alice, &bob).unwrap().unwrap();
        let economic_edge = multi.economic().get_edge(&alice, &bob).unwrap().unwrap();
        let technical_edge = multi.technical().get_edge(&alice, &bob).unwrap().unwrap();

        assert!((social_edge.score - 0.9).abs() < 0.001);
        assert!((economic_edge.score - 0.5).abs() < 0.001);
        assert!((technical_edge.score - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_multi_graph_combined_score() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut multi = MultiTrustGraph::new(store, alice.clone());

        // Add edges to all three graphs
        // Social: 0.8 direct → 0.8 * 0.6 = 0.48 score
        // Economic: 0.6 direct → 0.6 * 0.8 = 0.48 score
        // Technical: 0.4 direct → 0.4 * 0.9 = 0.36 score
        multi
            .social_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();
        multi
            .economic_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.6))
            .unwrap();
        multi
            .technical_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.4))
            .unwrap();

        // Combined: 0.48 * 0.5 + 0.48 * 0.3 + 0.36 * 0.2
        //         = 0.24 + 0.144 + 0.072 = 0.456
        let combined = multi.compute_combined_trust_score(&bob).unwrap();
        assert!((combined - 0.456).abs() < 0.01);
    }

    #[test]
    fn test_multi_graph_add_edge_to() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut multi = MultiTrustGraph::new(store, alice.clone());

        // Add edge to specific graph type
        multi
            .add_edge_to(
                TrustGraphType::EconomicReliability,
                TrustEdge::new(alice.clone(), bob.clone(), 0.7),
            )
            .unwrap();

        // Verify it's only in economic
        assert!(multi.social().get_edge(&alice, &bob).unwrap().is_none());
        assert!(multi.economic().get_edge(&alice, &bob).unwrap().is_some());
        assert!(multi.technical().get_edge(&alice, &bob).unwrap().is_none());
    }

    #[test]
    fn test_multi_graph_get_all_known_dids() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();
        let dave = KeyPair::generate().unwrap().did().clone();

        let mut multi = MultiTrustGraph::new(store, alice.clone());

        // Add different DIDs to different graphs
        multi
            .social_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.5))
            .unwrap();
        multi
            .economic_mut()
            .add_edge(TrustEdge::new(alice.clone(), carol.clone(), 0.5))
            .unwrap();
        multi
            .technical_mut()
            .add_edge(TrustEdge::new(alice.clone(), dave.clone(), 0.5))
            .unwrap();

        // Get all DIDs
        let all_dids = multi.get_all_known_dids().unwrap();

        // Should have alice, bob, carol, dave (4 unique DIDs)
        assert_eq!(all_dids.len(), 4);
        assert!(all_dids.contains(&alice));
        assert!(all_dids.contains(&bob));
        assert!(all_dids.contains(&carol));
        assert!(all_dids.contains(&dave));
    }

    #[test]
    fn test_multi_graph_recovery() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let new_alice = KeyPair::generate().unwrap().did().clone();

        let mut multi = MultiTrustGraph::new(store, alice.clone());

        // Add edges from alice in all graphs
        multi
            .social_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();
        multi
            .economic_mut()
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.6))
            .unwrap();

        // Perform recovery
        let migrated = multi.map_did_recovery(&alice, &new_alice).unwrap();
        assert_eq!(migrated, 2);

        // Verify edges are now from new_alice
        assert!(multi.social().get_edge(&alice, &bob).unwrap().is_none());
        assert!(multi.social().get_edge(&new_alice, &bob).unwrap().is_some());
        assert!(multi.economic().get_edge(&alice, &bob).unwrap().is_none());
        assert!(multi
            .economic()
            .get_edge(&new_alice, &bob)
            .unwrap()
            .is_some());
    }
}
