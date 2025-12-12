//! Trust Manager for Gateway
//!
//! Manages trust graph operations and provides API access to trust scores and edges.

use dashmap::DashMap;
use icn_identity::Did;
use icn_trust::TrustEdge;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Trust edge for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEdgeResponse {
    pub from: String,
    pub to: String,
    pub score: f64,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

/// Trust network node for visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustNetworkNode {
    pub did: String,
    pub trust_score: f64,
    pub distance: u32, // Hops from origin
}

/// Trust network for visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustNetwork {
    pub nodes: Vec<TrustNetworkNode>,
    pub edges: Vec<TrustEdgeResponse>,
}

/// Trust Manager for gateway
///
/// Provides a simplified interface to the trust graph for API endpoints.
/// Uses in-memory storage for now (could be backed by Sled in production).
pub struct TrustManager {
    /// In-memory trust edges (source:target -> edge)
    edges: Arc<DashMap<String, TrustEdge>>,
    /// Own DID (for trust computation perspective)
    own_did: Option<Did>,
}

impl TrustManager {
    /// Create a new trust manager
    pub fn new() -> Self {
        Self {
            edges: Arc::new(DashMap::new()),
            own_did: None,
        }
    }

    /// Set the perspective DID for trust computation
    pub fn set_perspective(&mut self, did: Did) {
        self.own_did = Some(did);
    }

    /// Add or update a trust edge
    pub fn add_edge(&self, edge: TrustEdge) -> Result<(), String> {
        let key = format!("{}:{}", edge.source.as_str(), edge.target.as_str());
        self.edges.insert(key, edge);
        Ok(())
    }

    /// Get a trust edge
    pub fn get_edge(&self, from: &Did, to: &Did) -> Option<TrustEdge> {
        let key = format!("{}:{}", from.as_str(), to.as_str());
        self.edges.get(&key).map(|e| e.clone())
    }

    /// Get all edges from a DID
    pub fn get_outgoing_edges(&self, from: &Did) -> Vec<TrustEdge> {
        let prefix = format!("{}:", from.as_str());
        self.edges
            .iter()
            .filter(|entry| entry.key().starts_with(&prefix))
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get all edges to a DID
    pub fn get_incoming_edges(&self, to: &Did) -> Vec<TrustEdge> {
        let to_str = to.as_str();
        self.edges
            .iter()
            .filter(|entry| entry.value().target.as_str() == to_str)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Compute trust score for a DID
    ///
    /// Uses a simplified PageRank-like algorithm:
    /// - 70% direct trust
    /// - 30% transitive trust (average of weighted paths)
    pub fn compute_trust_score(&self, from: &Did, to: &Did) -> f64 {
        // Direct trust
        let direct_score = self
            .get_edge(from, to)
            .map(|e| e.score)
            .unwrap_or(0.0);

        // Transitive trust (via intermediates)
        let outgoing = self.get_outgoing_edges(from);
        let mut transitive_sum = 0.0;
        let mut transitive_count = 0;

        for intermediate_edge in outgoing {
            // Skip if intermediate is the target
            if intermediate_edge.target == *to {
                continue;
            }

            // Get edge from intermediate to target
            if let Some(indirect_edge) = self.get_edge(&intermediate_edge.target, to) {
                // Weight: trust in intermediate * trust from intermediate to target
                let weight = intermediate_edge.score * indirect_edge.score;
                transitive_sum += weight;
                transitive_count += 1;
            }
        }

        let transitive_score = if transitive_count > 0 {
            transitive_sum / transitive_count as f64
        } else {
            0.0
        };

        // Combine (70% direct, 30% transitive)
        (direct_score * 0.7 + transitive_score * 0.3).min(1.0)
    }

    /// Get trust network around a DID (for visualization)
    ///
    /// Returns nodes and edges within `max_distance` hops
    pub fn get_trust_network(&self, center: &Did, max_distance: u32) -> TrustNetwork {
        use std::collections::{HashMap, HashSet, VecDeque};

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Start with center node
        queue.push_back((center.clone(), 0u32));
        nodes.insert(center.clone(), 1.0); // Self-trust = 1.0

        while let Some((current, distance)) = queue.pop_front() {
            if distance >= max_distance {
                continue;
            }

            if !visited.insert(current.clone()) {
                continue;
            }

            // Get outgoing edges
            for edge in self.get_outgoing_edges(&current) {
                // Add edge to result
                edges.push(TrustEdgeResponse {
                    from: edge.source.to_string(),
                    to: edge.target.to_string(),
                    score: edge.score,
                    created_at: edge.created_at,
                    labels: if edge.labels.is_empty() {
                        None
                    } else {
                        Some(edge.labels.clone())
                    },
                });

                // Add target node if not visited
                if !nodes.contains_key(&edge.target) {
                    // Compute trust score from center
                    let trust_score = self.compute_trust_score(center, &edge.target);
                    nodes.insert(edge.target.clone(), trust_score);

                    // Add to queue for further exploration
                    queue.push_back((edge.target.clone(), distance + 1));
                }
            }
        }

        // Convert nodes to response format
        let mut node_list: Vec<TrustNetworkNode> = nodes
            .into_iter()
            .map(|(did, trust_score)| TrustNetworkNode {
                did: did.to_string(),
                trust_score,
                distance: 0, // Will be computed below
            })
            .collect();

        // Compute distances using BFS
        let mut distances = HashMap::new();
        let mut queue = VecDeque::new();
        queue.push_back((center.clone(), 0u32));
        distances.insert(center.clone(), 0u32);

        while let Some((current, distance)) = queue.pop_front() {
            for edge in self.get_outgoing_edges(&current) {
                if !distances.contains_key(&edge.target) {
                    distances.insert(edge.target.clone(), distance + 1);
                    queue.push_back((edge.target.clone(), distance + 1));
                }
            }
        }

        // Update distances in nodes
        for node in &mut node_list {
            let did: Did = node.did.parse().unwrap_or_else(|_| center.clone());
            node.distance = *distances.get(&did).unwrap_or(&0);
        }

        TrustNetwork {
            nodes: node_list,
            edges,
        }
    }

    /// Get count of stored edges
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

impl Default for TrustManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_add_and_get_edge() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.8);
        manager.add_edge(edge.clone()).unwrap();

        let retrieved = manager.get_edge(&alice, &bob).unwrap();
        assert_eq!(retrieved.score, 0.8);
    }

    #[test]
    fn test_compute_direct_trust() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.6);
        manager.add_edge(edge).unwrap();

        let score = manager.compute_trust_score(&alice, &bob);
        // 0.6 * 0.7 (direct only) = 0.42
        assert!((0.41..=0.43).contains(&score));
    }

    #[test]
    fn test_compute_transitive_trust() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        // Alice trusts Bob
        manager
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();

        // Bob trusts Carol
        manager
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.6))
            .unwrap();

        let score = manager.compute_trust_score(&alice, &carol);
        // 0 * 0.7 (no direct) + (0.8 * 0.6) * 0.3 (transitive) = 0.144
        assert!((0.14..=0.15).contains(&score));
    }

    #[test]
    fn test_get_trust_network() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        manager
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();
        manager
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.6))
            .unwrap();

        let network = manager.get_trust_network(&alice, 2);

        // Should have 3 nodes (alice, bob, carol) and 2 edges
        assert_eq!(network.nodes.len(), 3);
        assert_eq!(network.edges.len(), 2);

        // Find Alice's node (should have trust_score = 1.0 for self)
        let alice_node = network
            .nodes
            .iter()
            .find(|n| n.did == alice.to_string())
            .unwrap();
        assert_eq!(alice_node.trust_score, 1.0);
        assert_eq!(alice_node.distance, 0);
    }
}
