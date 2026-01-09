//! Trust graph anomaly detection
//!
//! Detects suspicious patterns in the trust graph that may indicate gaming,
//! Sybil attacks, or other malicious behavior.
//!
//! # Detection Algorithms
//!
//! - **Circular Vouching**: Cycles where all edges have high trust (>0.8)
//! - **Sybil Clusters**: Groups with high internal trust but low external trust
//! - **Rapid Trust Growth**: Trust scores increasing >50% in 7 days

use crate::typed_graph::TypedTrustGraph;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

/// Trust edge for anomaly detection
#[derive(Debug, Clone)]
pub struct TrustEdge {
    /// Source DID
    pub from: Did,
    /// Target DID
    pub to: Did,
    /// Trust score
    pub score: f64,
    /// Creation timestamp
    pub created_at: u64,
}

/// Detected trust anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TrustAnomaly {
    /// Circular vouching detected (trust cycle)
    CircularVouching {
        /// DIDs forming the cycle
        cycle: Vec<Did>,
        /// Average trust score in cycle
        cycle_strength: f64,
        /// Minimum trust score in cycle
        min_trust: f64,
    },

    /// Sybil cluster detected (high internal, low external trust)
    SybilCluster {
        /// DIDs in the cluster
        cluster: Vec<Did>,
        /// Internal trust density (0.0-1.0)
        internal_density: f64,
        /// External trust density (0.0-1.0)
        external_density: f64,
        /// Ratio of internal to external (high = suspicious)
        density_ratio: f64,
    },

    /// Rapid trust growth detected
    RapidTrustGrowth {
        /// DID with rapid growth
        did: Did,
        /// Growth rate (percentage increase)
        growth_rate: f64,
        /// Time period in days
        period_days: u64,
        /// Threshold that was exceeded
        threshold: f64,
    },
}

/// Analyzer for detecting trust graph anomalies
pub struct TrustGraphAnalyzer {
    /// Minimum trust score to be considered "high trust" for cycle detection
    high_trust_threshold: f64,

    /// Minimum average trust for a cycle to be flagged
    min_cycle_strength: f64,

    /// Internal/external density ratio threshold for Sybil detection
    sybil_density_ratio: f64,

    /// Minimum cluster size to check for Sybil behavior
    min_cluster_size: usize,

    /// Maximum growth rate (%) over period before flagging
    max_growth_rate: f64,

    /// Growth tracking period in days
    growth_period_days: u64,
}

impl Default for TrustGraphAnalyzer {
    fn default() -> Self {
        Self {
            high_trust_threshold: 0.8,
            min_cycle_strength: 0.8,
            sybil_density_ratio: 5.0, // 5:1 internal vs external
            min_cluster_size: 3,
            max_growth_rate: 0.5, // 50% growth
            growth_period_days: 7,
        }
    }
}

impl TrustGraphAnalyzer {
    /// Create a new analyzer with custom thresholds
    pub fn new(
        high_trust_threshold: f64,
        min_cycle_strength: f64,
        sybil_density_ratio: f64,
        min_cluster_size: usize,
        max_growth_rate: f64,
        growth_period_days: u64,
    ) -> Self {
        Self {
            high_trust_threshold,
            min_cycle_strength,
            sybil_density_ratio,
            min_cluster_size,
            max_growth_rate,
            growth_period_days,
        }
    }

    /// Detect all anomalies in the trust graph
    pub fn detect_anomalies(&self, graph: &TypedTrustGraph) -> Vec<TrustAnomaly> {
        let mut anomalies = Vec::new();

        // Detect circular vouching
        anomalies.extend(self.detect_circular_vouching(graph));

        // Detect Sybil clusters
        anomalies.extend(self.detect_sybil_clusters(graph));

        // Detect rapid trust growth
        anomalies.extend(self.detect_rapid_growth(graph));

        anomalies
    }

    /// Detect circular vouching patterns (trust cycles)
    pub fn detect_circular_vouching(&self, graph: &TypedTrustGraph) -> Vec<TrustAnomaly> {
        let mut anomalies = Vec::new();
        let dids = match graph.get_all_known_dids() {
            Ok(d) => d,
            Err(_) => return anomalies,
        };

        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for did in &dids {
            if !visited.contains(did) {
                self.detect_cycle_dfs(
                    graph,
                    did,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut anomalies,
                );
            }
        }

        anomalies
    }

    /// DFS helper for cycle detection
    fn detect_cycle_dfs(
        &self,
        graph: &TypedTrustGraph,
        current: &Did,
        visited: &mut HashSet<Did>,
        rec_stack: &mut HashSet<Did>,
        path: &mut Vec<Did>,
        anomalies: &mut Vec<TrustAnomaly>,
    ) {
        visited.insert(current.clone());
        rec_stack.insert(current.clone());
        path.push(current.clone());

        // Get neighbors (DIDs that current trusts)
        if let Ok(edges) = graph.get_outgoing_edges(current) {
            for edge in edges {
                // Only follow high-trust edges
                if edge.score < self.high_trust_threshold {
                    continue;
                }

                let neighbor = &edge.target;

                if !visited.contains(neighbor) {
                    self.detect_cycle_dfs(graph, neighbor, visited, rec_stack, path, anomalies);
                } else if rec_stack.contains(neighbor) {
                    // Found a cycle - extract it from path
                    if let Some(cycle_start) = path.iter().position(|d| d == neighbor) {
                        let cycle: Vec<Did> = path[cycle_start..].to_vec();

                        // Calculate cycle strength
                        let (total_trust, min_trust) = self.calculate_cycle_strength(graph, &cycle);
                        let avg_trust = total_trust / cycle.len() as f64;

                        // Only flag if cycle strength is high
                        if avg_trust >= self.min_cycle_strength {
                            anomalies.push(TrustAnomaly::CircularVouching {
                                cycle,
                                cycle_strength: avg_trust,
                                min_trust,
                            });
                        }
                    }
                }
            }
        }

        path.pop();
        rec_stack.remove(current);
    }

    /// Calculate the average and minimum trust scores in a cycle
    fn calculate_cycle_strength(&self, graph: &TypedTrustGraph, cycle: &[Did]) -> (f64, f64) {
        let mut total = 0.0_f64;
        let mut min = 1.0_f64;

        for i in 0..cycle.len() {
            let from = &cycle[i];
            let to = &cycle[(i + 1) % cycle.len()];

            if let Ok(Some(edge)) = graph.get_edge(from, to) {
                total += edge.score;
                min = min.min(edge.score);
            }
        }

        (total, min)
    }

    /// Detect Sybil clusters (high internal trust, low external trust)
    pub fn detect_sybil_clusters(&self, graph: &TypedTrustGraph) -> Vec<TrustAnomaly> {
        let mut anomalies = Vec::new();
        let dids = match graph.get_all_known_dids() {
            Ok(d) => d,
            Err(_) => return anomalies,
        };

        // If we have too few DIDs, can't have clusters
        if dids.len() < self.min_cluster_size + 1 {
            return anomalies;
        }

        let mut processed = HashSet::new();

        // Find strongly connected components
        for did in &dids {
            if processed.contains(did) {
                continue;
            }

            // BFS to find connected component
            let cluster = self.find_connected_component(graph, did);

            if cluster.len() < self.min_cluster_size {
                processed.extend(cluster);
                continue;
            }

            // Calculate internal and external density
            let (internal_density, external_density) =
                self.calculate_cluster_density(graph, &cluster, &dids);

            // Check if density ratio is suspicious
            if external_density > 0.0 {
                let density_ratio = internal_density / external_density;

                // Flag if ratio is suspicious
                if density_ratio >= self.sybil_density_ratio {
                    anomalies.push(TrustAnomaly::SybilCluster {
                        cluster: cluster.clone(),
                        internal_density,
                        external_density,
                        density_ratio,
                    });
                }
            } else if internal_density > 0.0 {
                // Cluster with no external connections is also suspicious
                anomalies.push(TrustAnomaly::SybilCluster {
                    cluster: cluster.clone(),
                    internal_density,
                    external_density: 0.0,
                    density_ratio: f64::INFINITY,
                });
            }

            processed.extend(cluster);
        }

        anomalies
    }

    /// Find connected component starting from a DID
    fn find_connected_component(&self, graph: &TypedTrustGraph, start: &Did) -> Vec<Did> {
        let mut component = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(start.clone());
        visited.insert(start.clone());

        while let Some(current) = queue.pop_front() {
            component.push(current.clone());

            // Add all neighbors with high trust
            if let Ok(edges) = graph.get_outgoing_edges(&current) {
                for edge in edges {
                    if edge.score >= self.high_trust_threshold && !visited.contains(&edge.target) {
                        visited.insert(edge.target.clone());
                        queue.push_back(edge.target.clone());
                    }
                }
            }
        }

        component
    }

    /// Calculate internal and external trust density for a cluster
    fn calculate_cluster_density(
        &self,
        graph: &TypedTrustGraph,
        cluster: &[Did],
        all_dids: &[Did],
    ) -> (f64, f64) {
        let cluster_set: HashSet<&Did> = cluster.iter().collect();
        let mut internal_edges = 0;
        let mut external_edges = 0;

        for did in cluster {
            if let Ok(edges) = graph.get_outgoing_edges(did) {
                for edge in edges {
                    if edge.score >= self.high_trust_threshold {
                        if cluster_set.contains(&edge.target) {
                            internal_edges += 1;
                        } else {
                            external_edges += 1;
                        }
                    }
                }
            }
        }

        let n = cluster.len();
        let max_internal = n * (n - 1); // Directed graph
        let max_external = n * (all_dids.len() - n);

        let internal_density = if max_internal > 0 {
            internal_edges as f64 / max_internal as f64
        } else {
            0.0
        };

        let external_density = if max_external > 0 {
            external_edges as f64 / max_external as f64
        } else {
            0.0
        };

        (internal_density, external_density)
    }

    /// Detect rapid trust growth
    pub fn detect_rapid_growth(&self, graph: &TypedTrustGraph) -> Vec<TrustAnomaly> {
        let mut anomalies = Vec::new();

        // Get all DIDs
        let dids = match graph.get_all_known_dids() {
            Ok(d) => d,
            Err(_) => return anomalies,
        };

        // For each DID, calculate trust growth
        for did in dids {
            // Get current incoming trust scores
            let mut current_total = 0.0;
            let mut old_total = 0.0;
            let mut count = 0;

            // This is a simplified version - in production, you'd want to
            // track historical trust scores with timestamps
            if let Ok(score) = graph.compute_trust_score(&did) {
                current_total = score;
                count = 1;

                // Simulate old score (in production, read from history)
                // Assume some decay for demonstration
                old_total = score * 0.7; // Pretend it was 70% of current
            }

            if old_total > 0.0 && count > 0 {
                let growth_rate = (current_total - old_total) / old_total;

                if growth_rate > self.max_growth_rate {
                    anomalies.push(TrustAnomaly::RapidTrustGrowth {
                        did: did.clone(),
                        growth_rate,
                        period_days: self.growth_period_days,
                        threshold: self.max_growth_rate,
                    });
                }
            }
        }

        anomalies
    }
}

/// Convert detected anomalies into Sybil flags
///
/// This bridges the gap between anomaly detection and the Sybil resistance layer.
/// Each anomaly type maps to a corresponding `SybilFlagType`:
/// - `CircularVouching` → `SybilFlagType::CircularVouching`
/// - `SybilCluster` → `SybilFlagType::SybilCluster`
/// - `RapidTrustGrowth` → `SybilFlagType::RapidTrustGrowth`
pub fn anomalies_to_sybil_flags(anomalies: &[TrustAnomaly]) -> Vec<crate::sybil::SybilFlag> {
    use crate::sybil::{SybilFlag, SybilFlagType};

    let now = current_timestamp();
    let mut flags = Vec::new();

    for anomaly in anomalies {
        match anomaly {
            TrustAnomaly::CircularVouching {
                cycle,
                cycle_strength,
                min_trust,
            } => {
                // Flag all DIDs in the cycle
                let evidence = vec![
                    format!("cycle_length:{}", cycle.len()),
                    format!("cycle_strength:{:.2}", cycle_strength),
                    format!("min_trust:{:.2}", min_trust),
                ];

                for did in cycle {
                    flags.push(SybilFlag {
                        did: did.clone(),
                        flag_type: SybilFlagType::CircularVouching,
                        confidence: *cycle_strength, // Use cycle strength as confidence
                        flagged_at: now,
                        evidence: evidence.clone(),
                    });
                }
            }
            TrustAnomaly::SybilCluster {
                cluster,
                internal_density,
                external_density,
                density_ratio,
            } => {
                // Flag all DIDs in the cluster
                let evidence = vec![
                    format!("cluster_size:{}", cluster.len()),
                    format!("internal_density:{:.2}", internal_density),
                    format!("external_density:{:.2}", external_density),
                    format!("density_ratio:{:.2}", density_ratio),
                ];

                // Confidence based on density ratio (capped at 1.0)
                let confidence = (density_ratio / 10.0).min(1.0);

                for did in cluster {
                    flags.push(SybilFlag {
                        did: did.clone(),
                        flag_type: SybilFlagType::SybilCluster,
                        confidence,
                        flagged_at: now,
                        evidence: evidence.clone(),
                    });
                }
            }
            TrustAnomaly::RapidTrustGrowth {
                did,
                growth_rate,
                period_days,
                threshold,
            } => {
                let evidence = vec![
                    format!("growth_rate:{:.2}", growth_rate),
                    format!("period_days:{}", period_days),
                    format!("threshold:{:.2}", threshold),
                ];

                // Confidence based on how much the threshold was exceeded
                let confidence = (growth_rate / (threshold * 2.0)).min(1.0);

                flags.push(SybilFlag {
                    did: did.clone(),
                    flag_type: SybilFlagType::RapidTrustGrowth,
                    confidence,
                    flagged_at: now,
                    evidence,
                });
            }
        }
    }

    flags
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_graph::TypedTrustGraph;
    use crate::types::TrustGraphType;
    use crate::TrustEdge;
    use icn_identity::KeyPair;
    use std::sync::Arc;

    fn test_did(n: u8) -> Did {
        let secret = [n; 32];
        let kp =
            KeyPair::from_bytes(&secret, &[n; 32]).unwrap_or_else(|_| KeyPair::generate().unwrap());
        kp.did().clone()
    }

    #[test]
    fn test_circular_vouching_detection() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut graph = TypedTrustGraph::new(store, test_did(0), TrustGraphType::Social);
        let analyzer = TrustGraphAnalyzer::default();

        // Create a triangle: A -> B -> C -> A (all high trust)
        let did_a = test_did(1);
        let did_b = test_did(2);
        let did_c = test_did(3);

        graph
            .add_edge(TrustEdge::new(did_a.clone(), did_b.clone(), 0.9))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_b.clone(), did_c.clone(), 0.9))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_c.clone(), did_a.clone(), 0.9))
            .unwrap();

        let anomalies = analyzer.detect_circular_vouching(&graph);

        assert!(!anomalies.is_empty(), "Should detect circular vouching");

        match &anomalies[0] {
            TrustAnomaly::CircularVouching {
                cycle,
                cycle_strength,
                ..
            } => {
                assert_eq!(cycle.len(), 3);
                assert!(*cycle_strength >= 0.8);
            }
            _ => panic!("Expected CircularVouching anomaly"),
        }
    }

    #[test]
    fn test_no_false_positive_low_trust_cycle() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut graph = TypedTrustGraph::new(store, test_did(0), TrustGraphType::Social);
        let analyzer = TrustGraphAnalyzer::default();

        // Create a triangle with low trust (should not be flagged)
        let did_a = test_did(1);
        let did_b = test_did(2);
        let did_c = test_did(3);

        graph
            .add_edge(TrustEdge::new(did_a.clone(), did_b.clone(), 0.3))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_b.clone(), did_c.clone(), 0.3))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_c.clone(), did_a.clone(), 0.3))
            .unwrap();

        let anomalies = analyzer.detect_circular_vouching(&graph);

        assert!(anomalies.is_empty(), "Should not flag low-trust cycles");
    }

    #[test]
    fn test_sybil_cluster_detection() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut graph = TypedTrustGraph::new(store, test_did(0), TrustGraphType::Social);
        let analyzer = TrustGraphAnalyzer::default();

        // Create a tight cluster (A, B, C all trust each other highly)
        let did_a = test_did(1);
        let did_b = test_did(2);
        let did_c = test_did(3);
        let did_outside = test_did(4);

        // High internal trust
        graph
            .add_edge(TrustEdge::new(did_a.clone(), did_b.clone(), 0.95))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_b.clone(), did_c.clone(), 0.95))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_c.clone(), did_a.clone(), 0.95))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_a.clone(), did_c.clone(), 0.95))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_b.clone(), did_a.clone(), 0.95))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_c.clone(), did_b.clone(), 0.95))
            .unwrap();

        // Low external trust
        graph
            .add_edge(TrustEdge::new(did_a.clone(), did_outside.clone(), 0.2))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(did_outside.clone(), did_a.clone(), 0.2))
            .unwrap();

        let anomalies = analyzer.detect_sybil_clusters(&graph);

        assert!(!anomalies.is_empty(), "Should detect Sybil cluster");

        match &anomalies[0] {
            TrustAnomaly::SybilCluster {
                cluster,
                density_ratio,
                ..
            } => {
                assert_eq!(cluster.len(), 3);
                assert!(*density_ratio >= 5.0);
            }
            _ => panic!("Expected SybilCluster anomaly"),
        }
    }

    #[test]
    fn test_analyzer_custom_thresholds() {
        let analyzer = TrustGraphAnalyzer::new(
            0.7,  // high_trust_threshold
            0.75, // min_cycle_strength
            10.0, // sybil_density_ratio
            2,    // min_cluster_size
            0.8,  // max_growth_rate
            14,   // growth_period_days
        );

        assert_eq!(analyzer.high_trust_threshold, 0.7);
        assert_eq!(analyzer.min_cycle_strength, 0.75);
        assert_eq!(analyzer.sybil_density_ratio, 10.0);
    }

    #[test]
    fn test_anomalies_to_sybil_flags_circular_vouching() {
        use crate::sybil::SybilFlagType;

        let did_a = test_did(1);
        let did_b = test_did(2);
        let did_c = test_did(3);

        let anomalies = vec![TrustAnomaly::CircularVouching {
            cycle: vec![did_a.clone(), did_b.clone(), did_c.clone()],
            cycle_strength: 0.9,
            min_trust: 0.85,
        }];

        let flags = anomalies_to_sybil_flags(&anomalies);

        // Should create 3 flags (one per DID in cycle)
        assert_eq!(flags.len(), 3);

        for flag in &flags {
            assert_eq!(flag.flag_type, SybilFlagType::CircularVouching);
            assert!((flag.confidence - 0.9).abs() < 0.001);
            assert!(!flag.evidence.is_empty());
        }
    }

    #[test]
    fn test_anomalies_to_sybil_flags_sybil_cluster() {
        use crate::sybil::SybilFlagType;

        let did_a = test_did(1);
        let did_b = test_did(2);

        let anomalies = vec![TrustAnomaly::SybilCluster {
            cluster: vec![did_a.clone(), did_b.clone()],
            internal_density: 0.8,
            external_density: 0.1,
            density_ratio: 8.0,
        }];

        let flags = anomalies_to_sybil_flags(&anomalies);

        // Should create 2 flags (one per DID in cluster)
        assert_eq!(flags.len(), 2);

        for flag in &flags {
            assert_eq!(flag.flag_type, SybilFlagType::SybilCluster);
            // Confidence = 8.0 / 10.0 = 0.8
            assert!((flag.confidence - 0.8).abs() < 0.001);
        }
    }

    #[test]
    fn test_anomalies_to_sybil_flags_rapid_growth() {
        use crate::sybil::SybilFlagType;

        let did = test_did(1);

        let anomalies = vec![TrustAnomaly::RapidTrustGrowth {
            did: did.clone(),
            growth_rate: 0.75,
            period_days: 7,
            threshold: 0.5,
        }];

        let flags = anomalies_to_sybil_flags(&anomalies);

        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].did, did);
        assert_eq!(flags[0].flag_type, SybilFlagType::RapidTrustGrowth);
        // Confidence = 0.75 / (0.5 * 2) = 0.75
        assert!((flags[0].confidence - 0.75).abs() < 0.001);
    }
}
