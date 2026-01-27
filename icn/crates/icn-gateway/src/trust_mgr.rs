//! Trust Manager for Gateway
//!
//! Manages trust graph operations and provides API access to trust scores and edges.
//!
//! ## Actor-Backed Mode
//!
//! When created with `with_handle()`, the TrustManager delegates all operations to
//! the daemon's TrustGraph actor, ensuring:
//! - Single source of truth for trust data
//! - Persistence across restarts
//! - Gossip synchronization of trust edges
//!
//! When created with `new()`, it uses in-memory storage (suitable for testing only).
//!
//! ## Sync vs Async Methods
//!
//! This module provides both sync and async variants of most methods:
//!
//! - **Async methods** (e.g., `add_edge_async`, `get_edge_async`): Preferred for use
//!   in async contexts. These use proper async RwLock operations and don't block
//!   any Tokio worker threads.
//!
//! - **Sync methods** (e.g., `add_edge`, `get_edge`): Provided for backward
//!   compatibility. These use `block_in_place` which tells Tokio to move other
//!   tasks off the current thread before blocking. While this prevents blocking
//!   the entire runtime, it still occupies a worker thread during Sled I/O.
//!
//! **Recommendation**: Use async methods whenever possible. The sync methods are
//! deprecated for use in async contexts and may be removed in a future version.

use dashmap::DashMap;
use icn_identity::Did;
use icn_kernel_api::{ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest};
use icn_trust::TrustScore;
use icn_trust::{TrustEdge, TrustGraph};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

// ============================================================================
// Trust Score Constants
// ============================================================================

/// Default trust score returned when trust cannot be computed.
///
/// This corresponds to the "Known" trust level in ICN's trust classification:
/// - Isolated: < 0.1 (10 msg/sec rate limit)
/// - Known: 0.1-0.4 (50 msg/sec rate limit)
/// - Partner: 0.4-0.7 (100 msg/sec rate limit)
/// - Federated: > 0.7 (200 msg/sec rate limit)
///
/// A score of 0.5 represents the middle of the "Known+" range, providing
/// moderate rate limits while not granting excessive privileges to unknown peers.
///
/// This default is used when:
/// 1. The TrustGraph returns an error during score computation
/// 2. Running in standalone mode without a configured perspective DID
/// 3. The target DID has no trust edges (neither direct nor transitive)
pub const DEFAULT_TRUST_SCORE: f64 = 0.5;
const DIRECT_TRUST_WEIGHT: f64 = 0.7;
const TRANSITIVE_TRUST_WEIGHT: f64 = 0.3;

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

/// Handle type for actor-backed trust graph
pub type TrustGraphHandle = Arc<RwLock<TrustGraph>>;

/// Trust Manager for gateway
///
/// Provides a simplified interface to the trust graph for API endpoints.
///
/// Supports two modes:
/// - **Standalone mode** (`new()`): In-memory storage, for testing only
/// - **Actor-backed mode** (`with_handle()`): Delegates to daemon's TrustGraph
pub struct TrustManager {
    /// In-memory trust edges (source:target -> edge) - used in standalone mode
    edges: Arc<DashMap<String, TrustEdge>>,
    /// Own DID (for trust computation perspective)
    own_did: Option<Did>,
    /// Optional handle to daemon's TrustGraph (actor-backed mode)
    trust_graph: Option<TrustGraphHandle>,
}

impl TrustManager {
    /// Create a new trust manager in standalone mode (in-memory only)
    ///
    /// **Warning**: This mode is for testing only. State is lost on restart
    /// and not synchronized via gossip.
    pub fn new() -> Self {
        debug!("TrustManager created in standalone mode (in-memory only)");
        Self {
            edges: Arc::new(DashMap::new()),
            own_did: None,
            trust_graph: None,
        }
    }

    /// Create a trust manager backed by the daemon's TrustGraph
    ///
    /// This is the recommended mode for production. All operations delegate
    /// to the daemon's TrustGraph actor, ensuring:
    /// - Persistence across restarts
    /// - Gossip synchronization
    /// - Single source of truth
    pub fn with_handle(handle: TrustGraphHandle) -> Self {
        debug!("TrustManager created with daemon TrustGraph handle");
        Self {
            edges: Arc::new(DashMap::new()), // Not used in actor-backed mode
            own_did: None,
            trust_graph: Some(handle),
        }
    }

    /// Check if running in actor-backed mode
    pub fn is_actor_backed(&self) -> bool {
        self.trust_graph.is_some()
    }

    /// Set the perspective DID for trust computation
    pub fn set_perspective(&mut self, did: Did) {
        self.own_did = Some(did);
    }

    /// Get the trust manager as a PolicyOracle
    pub fn as_oracle(&self) -> Arc<dyn PolicyOracle> {
        Arc::new(TrustPolicyOracle {
            trust_graph: self.trust_graph.clone(),
            own_did: self.own_did.clone(),
            // In standalone mode, we share the edges map
            // Note: This is an imperfect clone for standalone mode, but sufficient for visual testing
            // Since standalone mode is dev-only, we accept the limitation that
            // checking edges directly updates the map but oracle check needs a new reference
            standalone_edges: if self.trust_graph.is_none() {
                Some(self.edges.clone())
            } else {
                None
            },
        })
    }

    /// Add or update a trust edge (sync version)
    ///
    /// In actor-backed mode, this requires acquiring a write lock on the TrustGraph.
    ///
    /// # Warning: Blocking I/O
    ///
    /// This method uses `block_in_place` which allows other Tokio tasks to be moved
    /// off the current thread, but still blocks a worker thread during Sled I/O
    /// (which can take milliseconds during compaction).
    ///
    /// **For async contexts, use [`add_edge_async`] instead.**
    ///
    /// # When to use this method
    ///
    /// - Non-async contexts (e.g., synchronous HTTP handlers, tests)
    /// - Performance-insensitive code paths
    /// - Quick prototyping (prefer async for production)
    #[deprecated(
        since = "0.2.0",
        note = "Use add_edge_async for async contexts to avoid blocking worker threads"
    )]
    pub fn add_edge(&self, edge: TrustEdge) -> Result<(), String> {
        if let Some(ref handle) = self.trust_graph {
            // Actor-backed mode: delegate to TrustGraph
            // Use block_in_place to properly isolate blocking I/O
            tokio::task::block_in_place(|| {
                let mut graph = handle.blocking_write();
                graph
                    .add_edge(edge)
                    .map_err(|e| format!("TrustGraph error: {e}"))
            })
        } else {
            // Standalone mode: in-memory storage
            let key = format!("{}:{}", edge.source.as_str(), edge.target.as_str());
            self.edges.insert(key, edge);
            Ok(())
        }
    }

    /// Add or update a trust edge (async version)
    pub async fn add_edge_async(&self, edge: TrustEdge) -> Result<(), String> {
        if let Some(ref handle) = self.trust_graph {
            let mut graph = handle.write().await;
            graph
                .add_edge(edge)
                .map_err(|e| format!("TrustGraph error: {e}"))
        } else {
            let key = format!("{}:{}", edge.source.as_str(), edge.target.as_str());
            self.edges.insert(key, edge);
            Ok(())
        }
    }

    /// Add a trust edge from raw parts (f64 score)
    pub async fn add_edge_with_score(
        &self,
        from: Did,
        to: Did,
        score: f64,
        memo: Option<String>,
    ) -> Result<(), String> {
        // Validate score
        crate::validation::validate_trust_score(score).map_err(|e| e.to_string())?;

        let trust_score = TrustScore::new(score).map_err(|e| e.to_string())?;
        let mut edge = TrustEdge::new(from, to, trust_score);

        if let Some(memo_str) = memo {
            edge = edge.with_label(memo_str);
        }

        self.add_edge_async(edge).await
    }

    /// Get a trust edge (sync version)
    ///
    /// # Warning: Blocking I/O
    ///
    /// This method uses `block_in_place` which still blocks a worker thread.
    /// **For async contexts, use [`get_edge_async`] instead.**
    #[deprecated(
        since = "0.2.0",
        note = "Use get_edge_async for async contexts to avoid blocking worker threads"
    )]
    pub fn get_edge(&self, from: &Did, to: &Did) -> Option<TrustEdge> {
        if let Some(ref handle) = self.trust_graph {
            // Actor-backed mode: delegate to TrustGraph
            tokio::task::block_in_place(|| {
                let graph = handle.blocking_read();
                graph.get_edge(from, to).ok().flatten()
            })
        } else {
            // Standalone mode: in-memory storage
            let key = format!("{}:{}", from.as_str(), to.as_str());
            self.edges.get(&key).map(|e| e.clone())
        }
    }

    /// Get a trust edge (async version)
    pub async fn get_edge_async(&self, from: &Did, to: &Did) -> Option<TrustEdge> {
        if let Some(ref handle) = self.trust_graph {
            let graph = handle.read().await;
            graph.get_edge(from, to).ok().flatten()
        } else {
            let key = format!("{}:{}", from.as_str(), to.as_str());
            self.edges.get(&key).map(|e| e.clone())
        }
    }

    /// Remove a trust edge (sync version)
    ///
    /// # Warning: Blocking I/O
    ///
    /// This method uses `block_in_place` which still blocks a worker thread.
    /// **For async contexts, use [`remove_edge_async`] instead.**
    #[deprecated(
        since = "0.2.0",
        note = "Use remove_edge_async for async contexts to avoid blocking worker threads"
    )]
    pub fn remove_edge(&self, from: &Did, to: &Did) -> Result<(), String> {
        if let Some(ref handle) = self.trust_graph {
            // Actor-backed mode: delegate to TrustGraph
            tokio::task::block_in_place(|| {
                let mut graph = handle.blocking_write();
                graph
                    .remove_edge(from, to)
                    .map_err(|e| format!("TrustGraph error: {e}"))
            })
        } else {
            // Standalone mode: in-memory storage
            let key = format!("{}:{}", from.as_str(), to.as_str());
            if self.edges.remove(&key).is_some() {
                Ok(())
            } else {
                Err("Trust edge not found".to_string())
            }
        }
    }

    /// Remove a trust edge (async version)
    pub async fn remove_edge_async(&self, from: &Did, to: &Did) -> Result<(), String> {
        if let Some(ref handle) = self.trust_graph {
            let mut graph = handle.write().await;
            graph
                .remove_edge(from, to)
                .map_err(|e| format!("TrustGraph error: {e}"))
        } else {
            let key = format!("{}:{}", from.as_str(), to.as_str());
            if self.edges.remove(&key).is_some() {
                Ok(())
            } else {
                Err("Trust edge not found".to_string())
            }
        }
    }

    /// Get all edges from a DID (sync version)
    ///
    /// # Warning: Blocking I/O
    ///
    /// This method uses `block_in_place` which still blocks a worker thread.
    /// **For async contexts, use [`get_outgoing_edges_async`] instead.**
    #[deprecated(
        since = "0.2.0",
        note = "Use get_outgoing_edges_async for async contexts to avoid blocking worker threads"
    )]
    pub fn get_outgoing_edges(&self, from: &Did) -> Vec<TrustEdge> {
        if let Some(ref handle) = self.trust_graph {
            // Actor-backed mode: delegate to TrustGraph
            // Use block_in_place to properly isolate blocking I/O
            tokio::task::block_in_place(|| {
                let graph = handle.blocking_read();
                graph.get_outgoing_edges(from).unwrap_or_else(|e| {
                    warn!("Failed to get outgoing edges for {}: {e}", from);
                    Vec::new()
                })
            })
        } else {
            // Standalone mode: in-memory storage
            let prefix = format!("{}:", from.as_str());
            self.edges
                .iter()
                .filter(|entry| entry.key().starts_with(&prefix))
                .map(|entry| entry.value().clone())
                .collect()
        }
    }

    /// Get all edges from a DID (async version)
    pub async fn get_outgoing_edges_async(&self, from: &Did) -> Vec<TrustEdge> {
        if let Some(ref handle) = self.trust_graph {
            let graph = handle.read().await;
            graph.get_outgoing_edges(from).unwrap_or_else(|e| {
                warn!("Failed to get outgoing edges for {}: {e}", from);
                Vec::new()
            })
        } else {
            let prefix = format!("{}:", from.as_str());
            self.edges
                .iter()
                .filter(|entry| entry.key().starts_with(&prefix))
                .map(|entry| entry.value().clone())
                .collect()
        }
    }

    /// Get all edges to a DID ("who trusts me?") (sync version)
    ///
    /// # Performance Warning
    ///
    /// This operation scans all edges and is **O(n)** where n is the total number
    /// of edges in the graph. Use sparingly for operations like user profile display.
    /// For high-frequency operations, consider caching the results.
    ///
    /// # Warning: Blocking I/O
    ///
    /// This method uses `block_in_place` which still blocks a worker thread.
    /// **For async contexts, use [`get_incoming_edges_async`] instead.**
    #[deprecated(
        since = "0.2.0",
        note = "Use get_incoming_edges_async for async contexts to avoid blocking worker threads"
    )]
    pub fn get_incoming_edges(&self, to: &Did) -> Vec<TrustEdge> {
        if let Some(ref handle) = self.trust_graph {
            // Actor-backed mode: delegate to TrustGraph
            // Use block_in_place to properly isolate blocking I/O
            tokio::task::block_in_place(|| {
                let graph = handle.blocking_read();
                graph.get_incoming_edges(to).unwrap_or_else(|e| {
                    warn!("Failed to get incoming edges for {}: {e}", to);
                    Vec::new()
                })
            })
        } else {
            // Standalone mode: in-memory storage
            let to_str = to.as_str();
            self.edges
                .iter()
                .filter(|entry| entry.value().target.as_str() == to_str)
                .map(|entry| entry.value().clone())
                .collect()
        }
    }

    /// Get all edges to a DID (async version)
    ///
    /// Note: This operation scans all edges and is O(n). Use sparingly.
    pub async fn get_incoming_edges_async(&self, to: &Did) -> Vec<TrustEdge> {
        if let Some(ref handle) = self.trust_graph {
            let graph = handle.read().await;
            graph.get_incoming_edges(to).unwrap_or_else(|e| {
                warn!("Failed to get incoming edges for {}: {e}", to);
                Vec::new()
            })
        } else {
            let to_str = to.as_str();
            self.edges
                .iter()
                .filter(|entry| entry.value().target.as_str() == to_str)
                .map(|entry| entry.value().clone())
                .collect()
        }
    }

    /// Compute trust score for a DID (sync version)
    ///
    /// In actor-backed mode, delegates to TrustGraph's optimized computation
    /// which includes caching, bloom filters, and priority queue pathfinding.
    ///
    /// In standalone mode, uses a simplified PageRank-like algorithm:
    /// - 70% direct trust
    /// - 30% transitive trust (average of weighted paths)
    ///
    /// # Warning: Blocking I/O
    ///
    /// This method uses `block_in_place` which still blocks a worker thread.
    /// **For async contexts, use [`compute_trust_score_async`] instead.**
    #[deprecated(
        since = "0.2.0",
        note = "Use compute_trust_score_async for async contexts to avoid blocking worker threads"
    )]
    pub fn compute_trust_score(&self, from: &Did, to: &Did) -> f64 {
        if let Some(ref handle) = self.trust_graph {
            // Actor-backed mode: delegate to TrustGraph
            // Use block_in_place to properly isolate blocking I/O
            tokio::task::block_in_place(|| {
                let graph = handle.blocking_read();
                // Note: TrustGraph computes from its own_did perspective
                // We need to check if from matches own_did
                if graph.own_did() == from {
                    graph.compute_trust_score(to).unwrap_or_else(|e| {
                        warn!("Failed to compute trust score for {}: {e}", to);
                        0.0
                    })
                } else {
                    // For non-own perspective, fall back to local computation
                    // Note: this still uses the underlying graph for edges
                    drop(graph); // Release lock before local computation
                    self.compute_trust_score_local(from, to)
                }
            })
        } else {
            // Standalone mode: local computation
            self.compute_trust_score_local(from, to)
        }
    }

    /// Compute trust score for a DID (async version)
    ///
    /// Prefer this over the sync version when calling from async context
    /// to avoid blocking the Tokio runtime.
    pub async fn compute_trust_score_async(&self, from: &Did, to: &Did) -> f64 {
        if let Some(ref handle) = self.trust_graph {
            let graph = handle.read().await;
            if graph.own_did() == from {
                graph.compute_trust_score(to).unwrap_or_else(|e| {
                    warn!("Failed to compute trust score for {}: {e}", to);
                    0.0
                })
            } else {
                // For non-own perspective, fall back to local computation
                drop(graph); // Release lock before local computation
                self.compute_trust_score_local_async(from, to).await
            }
        } else {
            self.compute_trust_score_local_async(from, to).await
        }
    }

    /// Compute trust score for velocity limiting (async version)
    ///
    /// This computes the trust score from the node's perspective to the given DID.
    /// Used for rate limiting based on trust level.
    ///
    /// # Trust-Based Rate Limiting
    ///
    /// The returned score maps to velocity limits as follows:
    /// - Isolated (< 0.1): 10 msg/sec - Unknown/untrusted peers
    /// - Known (0.1-0.4): 50 msg/sec - Recognized but not endorsed
    /// - Partner (0.4-0.7): 100 msg/sec - Trusted collaborators
    /// - Federated (> 0.7): 200 msg/sec - Highly trusted federation members
    ///
    /// # Default Behavior
    ///
    /// Returns [`DEFAULT_TRUST_SCORE`] (0.5) when trust cannot be computed:
    /// - TrustGraph lookup fails or returns None
    /// - Running in standalone mode without a configured perspective DID
    /// - Target has no trust edges in the graph
    ///
    /// The 0.5 default places unknown targets at the Partner threshold,
    /// which is intentionally permissive to avoid blocking legitimate traffic
    /// from new participants while still providing some rate protection.
    pub async fn compute_trust_score_for_velocity(&self, target: &Did) -> f64 {
        // Create an oracle request for trust score
        // We use the "trust" domain and "read" action
        let request = PolicyRequest::new(
            target.to_string(),
            icn_kernel_api::ActionKind::Read,
            Domain::trust(),
        );

        // Add metadata to request specifically the trust score
        let request = icn_kernel_api::PolicyRequest::with_ext(
            request.core,
            icn_kernel_api::PolicyRequestExt::new().with_resource("trust_score"),
        );

        // Evaluate using our internal oracle implementation
        let oracle = self.as_oracle();
        let decision = oracle.evaluate(&request);

        match decision {
            PolicyDecision::Allow { constraints } => {
                // Check for custom "trust_score" constraint first
                if let Some(icn_kernel_api::ConstraintValue::Float(score)) =
                    constraints.custom.get("trust_score")
                {
                    return score.into_inner();
                }

                // Fallback: check other constraint types that might map to trust
                // This is relevant if the policy model evolves
                DEFAULT_TRUST_SCORE
            }
            PolicyDecision::Deny { .. } => DEFAULT_TRUST_SCORE,
        }
    }

    /// Compute trust score using local (in-memory or fallback) algorithm
    ///
    /// Note: This is a sync method that uses sync edge getters. In async contexts,
    /// prefer `compute_trust_score_local_async` for better performance.
    #[allow(deprecated)] // Uses sync edge methods intentionally
    fn compute_trust_score_local(&self, from: &Did, to: &Did) -> f64 {
        // Direct trust
        let direct_score = self
            .get_edge(from, to)
            .map(|e| e.score.value())
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

    /// Compute trust score using local (in-memory or fallback) algorithm (async version)
    ///
    /// Uses async edge getters to avoid blocking the Tokio runtime.
    async fn compute_trust_score_local_async(&self, from: &Did, to: &Did) -> f64 {
        // Direct trust
        let direct_score = self
            .get_edge_async(from, to)
            .await
            .map(|e| e.score.value())
            .unwrap_or(0.0);

        // Transitive trust (via intermediates)
        let outgoing = self.get_outgoing_edges_async(from).await;
        let mut transitive_sum = 0.0;
        let mut transitive_count = 0;

        for intermediate_edge in outgoing {
            // Skip if intermediate is the target
            if intermediate_edge.target == *to {
                continue;
            }

            // Get edge from intermediate to target
            if let Some(indirect_edge) = self.get_edge_async(&intermediate_edge.target, to).await {
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

    /// Get trust network around a DID (for visualization) - async version
    ///
    /// Returns nodes and edges within `max_distance` hops.
    /// Prefer this over the sync version when calling from async context.
    pub async fn get_trust_network_async(&self, center: &Did, max_distance: u32) -> TrustNetwork {
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
            for edge in self.get_outgoing_edges_async(&current).await {
                // Add edge to result
                edges.push(TrustEdgeResponse {
                    from: edge.source.to_string(),
                    to: edge.target.to_string(),
                    score: edge.score.value(),
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
                    let trust_score = self.compute_trust_score_async(center, &edge.target).await;
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
            for edge in self.get_outgoing_edges_async(&current).await {
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

    /// Get trust network around a DID (for visualization) - sync version
    ///
    /// Returns nodes and edges within `max_distance` hops.
    ///
    /// # Warning: Blocking I/O
    ///
    /// This method uses sync APIs internally which block worker threads.
    /// **For async contexts, use [`get_trust_network_async`] instead.**
    #[deprecated(
        since = "0.2.0",
        note = "Use get_trust_network_async for async contexts to avoid blocking worker threads"
    )]
    #[allow(deprecated)] // Uses sync APIs intentionally
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
                    score: edge.score.value(),
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

/// Trust Policy Oracle implementation
///
/// Adapts the `TrustGraph` to the `PolicyOracle` trait, allowing the kernel
/// to query trust scores without unauthorized access to the graph structure.
pub struct TrustPolicyOracle {
    trust_graph: Option<TrustGraphHandle>,
    own_did: Option<Did>,
    standalone_edges: Option<Arc<DashMap<String, TrustEdge>>>,
}

impl TrustPolicyOracle {
    /// Compute trust score using local algorithm (helper for standalone mode)
    fn compute_local(&self, target_did: &String) -> f64 {
        let Some(ref own_did) = self.own_did else {
            return DEFAULT_TRUST_SCORE;
        };
    
        let Some(ref edges) = self.standalone_edges else {
            return DEFAULT_TRUST_SCORE;
        };

        compute_trust_from_edges_map(own_did.as_str(), target_did, edges)
    }
}



/// Helper to compute trust score from edges map (for standalone mode)
fn compute_trust_from_edges_map(own_did: &str, target_did: &str, edges: &DashMap<String, TrustEdge>) -> f64 {
    // 1. Direct trust
    let key = format!("{}:{}", own_did, target_did);
    let direct_score = edges.get(&key).map(|e| e.score.value()).unwrap_or(0.0);

    // 2. Transitive trust (1 hop only for simplicity in oracle)
    let prefix = format!("{}:", own_did);
    let outgoing: Vec<_> = edges
        .iter()
        .filter(|entry| entry.key().starts_with(&prefix))
        .map(|entry| entry.value().clone())
        .collect();

    let mut transitive_sum = 0.0;
    let mut transitive_count = 0;

    for mid in outgoing {
        if mid.target.to_string() == *target_did {
            continue;
        }

        let key2 = format!("{}:{}", mid.target, target_did);
        if let Some(indirect) = edges.get(&key2) {
            let weight = mid.score.value() * indirect.score.value();
            transitive_sum += weight;
            transitive_count += 1;
        }
    }

    let transitive_score = if transitive_count > 0 {
        transitive_sum / (transitive_count as f64)
    } else {
        0.0
    };

    (direct_score * DIRECT_TRUST_WEIGHT + transitive_score * TRANSITIVE_TRUST_WEIGHT).min(1.0)
}

impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // 1. Check domain
        if request.domain().as_str() != "trust" {
            // We don't handle other domains. In a chain-of-responsibility pattern,
            // we should Allow (abstain) rather than Deny, to let other oracles handle it.
            return PolicyDecision::allow_with(ConstraintSet::new());
        }

        // 2. Compute trust score
        let target_str = request.actor(); // This is a String (icn_kernel_api::Did)

        let score = if let Some(ref handle) = self.trust_graph {
            // Convert String to icn_identity::Did
            // If parse fails, treat as untrusted (default score)
            if let Ok(target_did) = target_str.parse::<Did>() {
                // Actor-backed mode: usage of block_in_place is required because
                // PolicyOracle::evaluate is synchronous but TrustGraph requires locking.
                // We try try_read first to avoid block_in_place overhead and support single-threaded runtimes.
                if let Ok(graph) = handle.try_read() {
                    graph.compute_trust_score(&target_did).unwrap_or_else(|e| {
                        warn!("TrustOracle compute error (try_read): {}", e);
                        DEFAULT_TRUST_SCORE
                    })
                } else {
                    debug!("TrustOracle contention: falling back to block_in_place");
                    tokio::task::block_in_place(|| {
                        let graph = handle.blocking_read();
                        graph.compute_trust_score(&target_did).unwrap_or_else(|e| {
                            warn!("TrustOracle compute error (blocking): {}", e);
                            DEFAULT_TRUST_SCORE
                        })
                    })
                }
            } else {
                DEFAULT_TRUST_SCORE
            }
        } else {
            // Standalone mode - works with Strings mostly
            self.compute_local(target_str)
        };

        // 3. Construct constraints
        let mut constraints = ConstraintSet::new();

        // Add trust score as custom constraint
        constraints = constraints.with_custom("trust_score", score.into());

        // Add rate limiting based on score
        // - Isolated: < 0.1
        // - Known: 0.1-0.4
        // - Partner: 0.4-0.7
        // - Federated: > 0.7
        let rate_limit = if score > 0.7 {
            icn_kernel_api::RateLimit::new(200, 50) // Federated
        } else if score > 0.4 {
            icn_kernel_api::RateLimit::standard() // Partner (100/25)
        } else if score > 0.1 {
            icn_kernel_api::RateLimit::throttled() // Known (20/10)
        } else {
            icn_kernel_api::RateLimit::restricted() // Isolated (5/5)
        };
        constraints = constraints.with_rate_limit(rate_limit);

        PolicyDecision::allow_with(constraints)
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }
}

impl Default for TrustManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(deprecated)] // Tests exercise both sync and async APIs
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    // ============================================================================
    // Sync API Tests (deprecated but still need coverage)
    // ============================================================================

    #[test]
    fn test_add_and_get_edge() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.8));
        manager.add_edge(edge.clone()).unwrap();

        let retrieved = manager.get_edge(&alice, &bob).unwrap();
        assert_eq!(retrieved.score, 0.8);
    }

    #[test]
    fn test_compute_direct_trust() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.6));
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
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();

        // Bob trusts Carol
        manager
            .add_edge(TrustEdge::new(
                bob.clone(),
                carol.clone(),
                TrustScore::unchecked(0.6),
            ))
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
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();
        manager
            .add_edge(TrustEdge::new(
                bob.clone(),
                carol.clone(),
                TrustScore::unchecked(0.6),
            ))
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

    #[test]
    fn test_incoming_edges() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        // Both Alice and Carol trust Bob
        manager
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();
        manager
            .add_edge(TrustEdge::new(
                carol.clone(),
                bob.clone(),
                TrustScore::unchecked(0.6),
            ))
            .unwrap();

        // Get incoming edges to Bob ("who trusts Bob?")
        let incoming = manager.get_incoming_edges(&bob);
        assert_eq!(incoming.len(), 2);

        // Verify both edges are present
        let sources: Vec<String> = incoming.iter().map(|e| e.source.to_string()).collect();
        assert!(sources.contains(&alice.to_string()));
        assert!(sources.contains(&carol.to_string()));
    }

    // ============================================================================
    // Async Tests
    // ============================================================================

    #[tokio::test]
    async fn test_async_add_and_get_edge() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.8));
        manager.add_edge_async(edge).await.unwrap();

        let retrieved = manager.get_edge_async(&alice, &bob).await.unwrap();
        assert_eq!(retrieved.score, 0.8);
    }

    #[tokio::test]
    async fn test_async_compute_trust_score() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        // Alice trusts Bob, Bob trusts Carol
        manager
            .add_edge_async(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .await
            .unwrap();
        manager
            .add_edge_async(TrustEdge::new(
                bob.clone(),
                carol.clone(),
                TrustScore::unchecked(0.6),
            ))
            .await
            .unwrap();

        // Test direct trust
        let direct_score = manager.compute_trust_score_async(&alice, &bob).await;
        // 0.8 * 0.7 (direct) = 0.56
        assert!((0.55..=0.57).contains(&direct_score));

        // Test transitive trust
        let transitive_score = manager.compute_trust_score_async(&alice, &carol).await;
        // 0 * 0.7 (no direct) + (0.8 * 0.6) * 0.3 (transitive) = 0.144
        assert!((0.14..=0.15).contains(&transitive_score));
    }

    #[tokio::test]
    async fn test_async_incoming_and_outgoing_edges() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        // Alice trusts Bob, Carol trusts Bob
        manager
            .add_edge_async(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .await
            .unwrap();
        manager
            .add_edge_async(TrustEdge::new(
                carol.clone(),
                bob.clone(),
                TrustScore::unchecked(0.6),
            ))
            .await
            .unwrap();

        // Test outgoing edges
        let outgoing = manager.get_outgoing_edges_async(&alice).await;
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].target, bob);

        // Test incoming edges
        let incoming = manager.get_incoming_edges_async(&bob).await;
        assert_eq!(incoming.len(), 2);
    }

    #[tokio::test]
    async fn test_async_concurrent_access() {
        use std::sync::Arc;

        let manager = Arc::new(TrustManager::new());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Seed with an edge
        manager
            .add_edge_async(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.5),
            ))
            .await
            .unwrap();

        // Spawn multiple concurrent reads
        let mut handles = Vec::new();
        for _ in 0..10 {
            let mgr = Arc::clone(&manager);
            let a = alice.clone();
            let b = bob.clone();
            handles.push(tokio::spawn(async move {
                let edge = mgr.get_edge_async(&a, &b).await;
                assert!(edge.is_some());
                let score = mgr.compute_trust_score_async(&a, &b).await;
                assert!(score > 0.0);
            }));
        }

        // Wait for all to complete
        for handle in handles {
            handle.await.unwrap();
        }
    }
}
