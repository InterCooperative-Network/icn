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
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

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
                total += edge.score.value();
                min = min.min(edge.score.value());
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

                // Confidence based on density ratio relative to the default detection
                // threshold (sybil_density_ratio = 5.0), capped at 1.0.
                // A cluster at the threshold gets 100% confidence since it was flagged.
                let confidence = (density_ratio / 5.0).min(1.0);

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

                // Confidence based on how much the threshold was exceeded.
                // Saturate at 1.0 when growth_rate is roughly 3x the threshold,
                // so a growth rate exactly at threshold gets ~33% confidence.
                let confidence = (growth_rate / (threshold * 3.0)).min(1.0);

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

// ============================================================================
// Centralization Metrics (Wave 2)
// ============================================================================

/// Centralization metrics for a scope
///
/// These metrics are **observational only**. They feed dashboards and governance
/// decisions, NOT automatic enforcement. The metrics never enter the kernel.
///
/// # Usage Pattern
///
/// ```text
/// Metrics ─► Governance ─► PolicyOracle ─► ConstraintSet ─► Kernel
///             │                              │
///        (human vote)                   (pure data)
/// ```
///
/// **Never**: `Metrics ─► Kernel` (direct)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CentralizationMetrics {
    /// Gini coefficient of inbound trust (0.0 = perfect equality, 1.0 = one node has all trust)
    pub gini_coefficient: f64,
    /// Number of nodes in scope
    pub node_count: usize,
    /// Total inbound trust across all nodes
    pub total_inbound_trust: f64,
    /// Top 10% of nodes by inbound trust (concentration measure)
    pub top_10_percent_share: f64,
    /// Betweenness centrality statistics
    pub betweenness: BetweennessStats,
}

/// Betweenness centrality statistics.
///
/// **Limitation**: Uses a simplified BFS that counts only the first shortest
/// path discovered between each node pair, not all shortest paths.  This may
/// undercount betweenness for nodes that lie on multiple equally-short paths.
/// A full Brandes-style all-paths algorithm would be more accurate but is
/// deferred until profiling shows the approximation is insufficient.
///
/// **Expected error bounds**:
/// - Sparse graphs (tree-like): Minimal error (<5%), since most node pairs
///   have a unique shortest path.
/// - Dense clusters: Could underestimate betweenness by 30-50% for hub nodes
///   that sit on multiple shortest paths.
/// - Practical impact: For centralization alerting (the primary use case),
///   underestimation means we may miss moderate centralization. This is
///   acceptable for Wave 2; upgrade to Brandes if false-negative rate is too high.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetweennessStats {
    /// Maximum betweenness centrality (most central node)
    pub max: f64,
    /// Mean betweenness centrality
    pub mean: f64,
    /// Standard deviation
    pub std_dev: f64,
}

impl TrustGraphAnalyzer {
    /// Compute centralization metrics for a specific scope
    ///
    /// This is an **observational metric** that feeds governance dashboards.
    /// It does NOT trigger automatic enforcement. Governance policies can use
    /// these metrics to decide on policy changes via PolicyOracle.
    ///
    /// # Firewall Compliance
    ///
    /// This method is in `icn-trust` (app layer). The kernel never calls this.
    /// Metrics are exposed via `icn-obs` but remain in the app layer.
    ///
    /// # Arguments
    ///
    /// * `graph` - The trust graph to analyze
    /// * `scope` - The scope to compute metrics for (uses `get_edges_in_scope`)
    ///
    /// # Returns
    ///
    /// Centralization metrics for the scope, or error if computation fails.
    pub fn compute_centralization_metrics(
        &self,
        graph: &TypedTrustGraph,
        scope: &crate::ScopeId,
    ) -> anyhow::Result<CentralizationMetrics> {
        // Get edges in scope
        let edges = graph.inner().get_edges_in_scope(scope)?;

        if edges.is_empty() {
            return Ok(CentralizationMetrics {
                gini_coefficient: 0.0,
                node_count: 0,
                total_inbound_trust: 0.0,
                top_10_percent_share: 0.0,
                betweenness: BetweennessStats {
                    max: 0.0,
                    mean: 0.0,
                    std_dev: 0.0,
                },
            });
        }

        // Compute inbound trust per node
        let mut inbound_trust: HashMap<Did, f64> = HashMap::new();
        let mut nodes: HashSet<Did> = HashSet::new();

        for edge in &edges {
            nodes.insert(edge.source.clone());
            nodes.insert(edge.target.clone());
            *inbound_trust.entry(edge.target.clone()).or_insert(0.0) += edge.score.value();
        }

        let node_count = nodes.len();
        let total_inbound_trust: f64 = inbound_trust.values().sum();

        // Compute Gini coefficient
        let gini = self.compute_gini_coefficient(&inbound_trust);

        // Compute top 10% share
        let top_10_percent_share = self.compute_top_concentration(&inbound_trust, 0.1);

        // Compute betweenness centrality
        let betweenness = self.compute_betweenness_stats(&edges);

        Ok(CentralizationMetrics {
            gini_coefficient: gini,
            node_count,
            total_inbound_trust,
            top_10_percent_share,
            betweenness,
        })
    }

    /// Compute Gini coefficient of trust distribution
    ///
    /// The Gini coefficient measures inequality in trust distribution:
    /// - 0.0 = perfect equality (all nodes have equal inbound trust)
    /// - 1.0 = perfect inequality (one node has all trust)
    ///
    /// Formula: G = (2 * Σ(i * x_i)) / (n * Σ(x_i)) - (n + 1) / n
    fn compute_gini_coefficient(&self, inbound_trust: &HashMap<Did, f64>) -> f64 {
        if inbound_trust.is_empty() {
            return 0.0;
        }

        let mut values: Vec<f64> = inbound_trust.values().copied().collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let n = values.len() as f64;
        let sum: f64 = values.iter().sum();

        if sum == 0.0 {
            return 0.0;
        }

        let weighted_sum: f64 = values
            .iter()
            .enumerate()
            .map(|(i, &x)| (i as f64 + 1.0) * x)
            .sum();

        let gini = (2.0 * weighted_sum) / (n * sum) - (n + 1.0) / n;
        if !(0.0..=1.0).contains(&gini) {
            tracing::warn!(
                "Gini coefficient {gini:.4} outside [0,1] range (n={}, sum={sum:.4}); clamping",
                inbound_trust.len()
            );
        }
        gini.clamp(0.0, 1.0)
    }

    /// Compute concentration in top percentile
    ///
    /// Returns the fraction of total trust held by the top `percentile` of nodes.
    /// E.g., `percentile=0.1` returns the share held by the top 10% of nodes.
    fn compute_top_concentration(&self, inbound_trust: &HashMap<Did, f64>, percentile: f64) -> f64 {
        if inbound_trust.is_empty() {
            return 0.0;
        }

        let mut values: Vec<f64> = inbound_trust.values().copied().collect();
        values.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)); // Sort descending

        let top_n = ((values.len() as f64 * percentile).ceil() as usize).max(1);
        let top_sum: f64 = values.iter().take(top_n).sum();
        let total_sum: f64 = values.iter().sum();

        if total_sum == 0.0 {
            0.0
        } else {
            top_sum / total_sum
        }
    }

    /// Compute betweenness centrality statistics
    ///
    /// Betweenness centrality measures how often a node appears on shortest paths
    /// between other nodes. High betweenness indicates bridge/bottleneck nodes.
    ///
    /// This uses a simplified all-pairs shortest paths approach.
    fn compute_betweenness_stats(&self, edges: &[crate::TrustEdge]) -> BetweennessStats {
        // Extract nodes from edges
        let mut nodes = HashSet::new();
        for edge in edges {
            nodes.insert(edge.source.clone());
            nodes.insert(edge.target.clone());
        }

        if nodes.len() <= 1 {
            return BetweennessStats {
                max: 0.0,
                mean: 0.0,
                std_dev: 0.0,
            };
        }

        // Build adjacency list
        let mut adj: HashMap<Did, Vec<Did>> = HashMap::new();
        for edge in edges {
            adj.entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
        }

        // Compute betweenness for each node
        let mut betweenness: HashMap<Did, f64> = HashMap::new();
        for node in &nodes {
            betweenness.insert(node.clone(), 0.0);
        }

        // For each pair of nodes, find shortest paths and count intermediate nodes.
        // O(n² * (V+E)) — refuse computation above hard limit to prevent DoS.
        const BETWEENNESS_HARD_LIMIT: usize = 5000;
        if nodes.len() > BETWEENNESS_HARD_LIMIT {
            tracing::error!(
                "Betweenness computation refused: {} nodes exceeds hard limit of {}",
                nodes.len(),
                BETWEENNESS_HARD_LIMIT
            );
            return BetweennessStats {
                max: 0.0,
                mean: 0.0,
                std_dev: 0.0,
            };
        }
        if nodes.len() > 1000 {
            tracing::warn!(
                "Betweenness computation for {} nodes may be slow; consider sampling",
                nodes.len()
            );
        }
        for start in &nodes {
            let shortest_paths = self.compute_shortest_paths(start, &adj);

            for (end, path) in shortest_paths {
                if start == &end {
                    continue;
                }

                // Count intermediate nodes (exclude start and end)
                for node in path.iter().skip(1).take(path.len().saturating_sub(2)) {
                    if let Some(score) = betweenness.get_mut(node) {
                        *score += 1.0;
                    }
                }
            }
        }

        // Normalize by (n-1)(n-2) for directed graphs
        let n = nodes.len() as f64;
        let normalization = if n > 2.0 { (n - 1.0) * (n - 2.0) } else { 1.0 };

        let normalized: Vec<f64> = betweenness.values().map(|&bc| bc / normalization).collect();

        let max = normalized.iter().copied().fold(0.0, f64::max);
        let mean = if normalized.is_empty() {
            0.0
        } else {
            normalized.iter().sum::<f64>() / normalized.len() as f64
        };

        let variance = if normalized.is_empty() {
            0.0
        } else {
            normalized
                .iter()
                .map(|&bc| (bc - mean).powi(2))
                .sum::<f64>()
                / normalized.len() as f64
        };

        let std_dev = variance.sqrt();

        BetweennessStats { max, mean, std_dev }
    }

    /// Compute shortest paths from start node using BFS
    fn compute_shortest_paths(
        &self,
        start: &Did,
        adj: &HashMap<Did, Vec<Did>>,
    ) -> HashMap<Did, Vec<Did>> {
        let mut paths: HashMap<Did, Vec<Did>> = HashMap::new();
        let mut visited: HashSet<Did> = HashSet::new();
        let mut queue: VecDeque<(Did, Vec<Did>)> = VecDeque::new();

        queue.push_back((start.clone(), vec![start.clone()]));
        visited.insert(start.clone());

        while let Some((node, path)) = queue.pop_front() {
            paths.insert(node.clone(), path.clone());

            if let Some(neighbors) = adj.get(&node) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        let mut new_path = path.clone();
                        new_path.push(neighbor.clone());
                        queue.push_back((neighbor.clone(), new_path));
                    }
                }
            }
        }

        paths
    }
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
    use crate::{TrustEdge, TrustScore};
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
            .add_edge(TrustEdge::new(
                did_a.clone(),
                did_b.clone(),
                TrustScore::unchecked(0.9),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_b.clone(),
                did_c.clone(),
                TrustScore::unchecked(0.9),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_c.clone(),
                did_a.clone(),
                TrustScore::unchecked(0.9),
            ))
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
            .add_edge(TrustEdge::new(
                did_a.clone(),
                did_b.clone(),
                TrustScore::unchecked(0.3),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_b.clone(),
                did_c.clone(),
                TrustScore::unchecked(0.3),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_c.clone(),
                did_a.clone(),
                TrustScore::unchecked(0.3),
            ))
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
            .add_edge(TrustEdge::new(
                did_a.clone(),
                did_b.clone(),
                TrustScore::unchecked(0.95),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_b.clone(),
                did_c.clone(),
                TrustScore::unchecked(0.95),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_c.clone(),
                did_a.clone(),
                TrustScore::unchecked(0.95),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_a.clone(),
                did_c.clone(),
                TrustScore::unchecked(0.95),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_b.clone(),
                did_a.clone(),
                TrustScore::unchecked(0.95),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_c.clone(),
                did_b.clone(),
                TrustScore::unchecked(0.95),
            ))
            .unwrap();

        // Low external trust
        graph
            .add_edge(TrustEdge::new(
                did_a.clone(),
                did_outside.clone(),
                TrustScore::unchecked(0.2),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                did_outside.clone(),
                did_a.clone(),
                TrustScore::unchecked(0.2),
            ))
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
            // Confidence = 8.0 / 5.0 = 1.6, capped at 1.0
            assert!((flag.confidence - 1.0).abs() < 0.001);
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
        // Confidence = 0.75 / (0.5 * 3) = 0.5
        assert!((flags[0].confidence - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_gini_coefficient_all_zero_trust() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut graph =
            TypedTrustGraph::new(store, test_did(0), TrustGraphType::Social);

        // Add edges with zero trust scores
        graph
            .add_edge(TrustEdge::new(
                test_did(0),
                test_did(1),
                TrustScore::unchecked(0.0),
            ))
            .unwrap();
        graph
            .add_edge(TrustEdge::new(
                test_did(0),
                test_did(2),
                TrustScore::unchecked(0.0),
            ))
            .unwrap();

        let analyzer = TrustGraphAnalyzer::default();
        let scope = crate::ScopeId::Global;
        let metrics = analyzer
            .compute_centralization_metrics(&graph, &scope)
            .unwrap();
        // All-zero trust should produce Gini of 0 (perfect equality)
        assert!(
            metrics.gini_coefficient >= 0.0 && metrics.gini_coefficient <= 1.0,
            "Gini should be in [0,1] even with all-zero trust: {}",
            metrics.gini_coefficient
        );
    }

    #[test]
    fn test_gini_coefficient_single_edge() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut graph =
            TypedTrustGraph::new(store, test_did(0), TrustGraphType::Social);

        graph
            .add_edge(TrustEdge::new(
                test_did(0),
                test_did(1),
                TrustScore::unchecked(0.5),
            ))
            .unwrap();

        let analyzer = TrustGraphAnalyzer::default();
        let scope = crate::ScopeId::Global;
        let metrics = analyzer
            .compute_centralization_metrics(&graph, &scope)
            .unwrap();
        assert!(
            metrics.gini_coefficient >= 0.0 && metrics.gini_coefficient <= 1.0,
            "Gini should be valid for single-edge graph: {}",
            metrics.gini_coefficient
        );
    }
}
