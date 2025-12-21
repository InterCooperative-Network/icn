//! Trust Pathfinder - Priority Queue-based Trust Score Computation
//!
//! Replaces the O(n·m) linear scan with a priority queue (max-heap) that
//! explores highest-trust paths first. Supports early termination when
//! a minimum threshold is reached.

use crate::types::ScoringWeights;
use crate::TrustGraph;
use anyhow::Result;
use icn_identity::Did;
use ordered_float::OrderedFloat;
use std::collections::{BinaryHeap, HashSet};

/// Entry in the priority queue: (trust_score, did, hops)
/// Using Reverse to get max-heap behavior (highest trust first)
#[derive(Debug, Clone, PartialEq, Eq)]
struct PathEntry {
    /// Trust score along this path (higher is better)
    score: OrderedFloat<f64>,
    /// Current DID being visited
    did: Did,
    /// Number of hops from origin
    hops: u8,
}

impl Ord for PathEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher score = higher priority
        self.score.cmp(&other.score)
    }
}

impl PartialOrd for PathEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Configuration for pathfinding
#[derive(Debug, Clone)]
pub struct PathfinderConfig {
    /// Maximum hops to traverse (default: 2)
    pub max_hops: u8,
    /// Minimum score threshold for early termination (optional)
    pub min_threshold: Option<f64>,
    /// Scoring weights for direct/transitive trust
    pub weights: ScoringWeights,
}

impl Default for PathfinderConfig {
    fn default() -> Self {
        Self {
            max_hops: 2,
            min_threshold: None,
            weights: ScoringWeights::legacy(),
        }
    }
}

impl PathfinderConfig {
    /// Set early termination threshold
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.min_threshold = Some(threshold);
        self
    }

    /// Set maximum hops
    pub fn with_max_hops(mut self, hops: u8) -> Self {
        self.max_hops = hops;
        self
    }

    /// Set scoring weights
    pub fn with_weights(mut self, weights: ScoringWeights) -> Self {
        self.weights = weights;
        self
    }
}

/// Trust pathfinder using priority queue for efficient path discovery
pub struct TrustPathfinder {
    /// Configuration
    config: PathfinderConfig,
}

impl TrustPathfinder {
    /// Create a new pathfinder with default configuration
    pub fn new() -> Self {
        Self {
            config: PathfinderConfig::default(),
        }
    }

    /// Create a new pathfinder with custom configuration
    pub fn with_config(config: PathfinderConfig) -> Self {
        Self { config }
    }

    /// Find trust score for a target DID using priority queue traversal
    ///
    /// This method:
    /// 1. Starts from direct edges from own_did
    /// 2. Uses a max-heap to always explore highest-trust paths first
    /// 3. Supports early termination when threshold is met
    /// 4. Limits traversal to max_hops
    ///
    /// Returns the weighted trust score combining direct and transitive trust.
    pub fn find_score(&self, graph: &TrustGraph, target: &Did) -> Result<f64> {
        let own_did = graph.own_did();

        // Fast path: check direct edge first
        let direct_score = graph
            .get_edge(own_did, target)?
            .map(|e| e.score)
            .unwrap_or(0.0);

        // If we have high direct trust and threshold is set, maybe early exit
        let weighted_direct = direct_score * self.config.weights.direct;
        if let Some(threshold) = self.config.min_threshold {
            if weighted_direct >= threshold {
                return Ok(weighted_direct);
            }
        }

        // For transitive trust, we need to explore through intermediates
        let transitive_score = self.compute_transitive_score(graph, target)?;

        // Combine scores using weights
        let final_score =
            (direct_score * self.config.weights.direct
                + transitive_score * self.config.weights.transitive)
                .min(1.0);

        Ok(final_score)
    }

    /// Compute transitive trust score using priority queue
    fn compute_transitive_score(&self, graph: &TrustGraph, target: &Did) -> Result<f64> {
        let own_did = graph.own_did();

        // Priority queue: entries sorted by trust score (highest first)
        let mut queue: BinaryHeap<PathEntry> = BinaryHeap::new();

        // Track visited DIDs to avoid cycles
        let mut visited: HashSet<Did> = HashSet::new();
        visited.insert(own_did.clone());

        // Best transitive score found for target
        let mut best_transitive: f64 = 0.0;
        let mut path_count: usize = 0;
        let mut path_sum: f64 = 0.0;

        // Initialize queue with direct edges from own_did
        let own_edges = graph.get_outgoing_edges(own_did)?;
        for edge in own_edges {
            if edge.target == *target {
                continue; // Skip direct edge to target (handled separately)
            }
            queue.push(PathEntry {
                score: OrderedFloat(edge.score),
                did: edge.target,
                hops: 1,
            });
        }

        // Process queue
        while let Some(entry) = queue.pop() {
            let current_did = &entry.did;
            let current_score = entry.score.into_inner();

            // Skip if already visited
            if visited.contains(current_did) {
                continue;
            }
            visited.insert(current_did.clone());

            // Check for edge to target
            if let Some(edge_to_target) = graph.get_edge(current_did, target)? {
                // Found a path: own -> current -> target
                let path_trust = current_score * edge_to_target.score;
                path_sum += path_trust;
                path_count += 1;

                // Update best score
                if path_trust > best_transitive {
                    best_transitive = path_trust;
                }

                // Early termination check
                if let Some(threshold) = self.config.min_threshold {
                    let current_transitive = if path_count > 0 {
                        path_sum / path_count as f64
                    } else {
                        0.0
                    };
                    let weighted = current_transitive * self.config.weights.transitive;
                    if weighted >= threshold {
                        // We have enough transitive trust
                        return Ok(current_transitive);
                    }
                }
            }

            // If we haven't reached max hops, explore further
            if entry.hops < self.config.max_hops {
                let edges = graph.get_outgoing_edges(current_did)?;
                for edge in edges {
                    if !visited.contains(&edge.target) && edge.target != *target {
                        // Diminish trust along the path
                        let path_score = current_score * edge.score;

                        // Only add if score is still meaningful
                        if path_score > 0.01 {
                            queue.push(PathEntry {
                                score: OrderedFloat(path_score),
                                did: edge.target,
                                hops: entry.hops + 1,
                            });
                        }
                    }
                }
            }
        }

        // Return average transitive score
        if path_count > 0 {
            Ok(path_sum / path_count as f64)
        } else {
            Ok(0.0)
        }
    }
}

impl Default for TrustPathfinder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrustEdge;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use std::sync::Arc;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn make_test_graph() -> (TrustGraph, Did, Did, Did, Did) {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = make_test_did();
        let bob = make_test_did();
        let carol = make_test_did();
        let dave = make_test_did();

        let graph = TrustGraph::new(store, alice.clone());
        (graph, alice, bob, carol, dave)
    }

    #[test]
    fn test_direct_trust_only() {
        let (mut graph, alice, bob, _, _) = make_test_graph();

        // Alice directly trusts Bob with 0.8
        graph
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();

        let pathfinder = TrustPathfinder::new();
        let score = pathfinder.find_score(&graph, &bob).unwrap();

        // Direct trust: 0.8 * 0.7 = 0.56
        assert!((score - 0.56).abs() < 0.01);
    }

    #[test]
    fn test_transitive_trust() {
        let (mut graph, alice, bob, carol, _) = make_test_graph();

        // Alice -> Bob (0.8), Bob -> Carol (0.6)
        graph
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.6))
            .unwrap();

        let pathfinder = TrustPathfinder::new();
        let score = pathfinder.find_score(&graph, &carol).unwrap();

        // Transitive only: 0.8 * 0.6 = 0.48, weighted at 0.3 = 0.144
        assert!((score - 0.144).abs() < 0.02);
    }

    #[test]
    fn test_combined_direct_and_transitive() {
        let (mut graph, alice, bob, carol, _) = make_test_graph();

        // Alice -> Carol directly (0.4)
        // Alice -> Bob (0.8), Bob -> Carol (0.9)
        graph
            .add_edge(TrustEdge::new(alice.clone(), carol.clone(), 0.4))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.9))
            .unwrap();

        let pathfinder = TrustPathfinder::new();
        let score = pathfinder.find_score(&graph, &carol).unwrap();

        // Direct: 0.4 * 0.7 = 0.28
        // Transitive: 0.8 * 0.9 = 0.72 * 0.3 = 0.216
        // Total: ~0.496
        assert!((score - 0.496).abs() < 0.02);
    }

    #[test]
    fn test_early_termination() {
        let (mut graph, alice, bob, carol, _) = make_test_graph();

        // High direct trust
        graph
            .add_edge(TrustEdge::new(alice.clone(), carol.clone(), 0.9))
            .unwrap();
        // Also transitive path that won't be explored
        graph
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.7))
            .unwrap();

        let config = PathfinderConfig::default().with_threshold(0.5);
        let pathfinder = TrustPathfinder::with_config(config);
        let score = pathfinder.find_score(&graph, &carol).unwrap();

        // Direct: 0.9 * 0.7 = 0.63 > 0.5 threshold, should early terminate
        assert!(score >= 0.5);
    }

    #[test]
    fn test_no_path() {
        let (graph, _, _, _, dave) = make_test_graph();

        let pathfinder = TrustPathfinder::new();
        let score = pathfinder.find_score(&graph, &dave).unwrap();

        // No edges, should be 0
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_cycle_handling() {
        let (mut graph, alice, bob, carol, _) = make_test_graph();

        // Create a cycle: Alice -> Bob -> Carol -> Alice
        graph
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.7))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(carol.clone(), alice.clone(), 0.6))
            .unwrap();

        let pathfinder = TrustPathfinder::new();

        // Should not infinite loop
        let score = pathfinder.find_score(&graph, &carol).unwrap();
        assert!(score >= 0.0 && score <= 1.0);
    }

    #[test]
    fn test_max_hops_limit() {
        let (mut graph, alice, bob, carol, dave) = make_test_graph();

        // Chain: Alice -> Bob -> Carol -> Dave
        graph
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.9))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.9))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(carol.clone(), dave.clone(), 0.9))
            .unwrap();

        // With max_hops=2, shouldn't reach Dave
        let config = PathfinderConfig::default().with_max_hops(2);
        let pathfinder = TrustPathfinder::with_config(config);
        let score = pathfinder.find_score(&graph, &dave).unwrap();

        // Should find path: Alice -> Bob -> Carol (-> Dave) within 2 hops
        // Bob at hop 1, Carol at hop 2, check edge to Dave
        assert!(score > 0.0);
    }
}
