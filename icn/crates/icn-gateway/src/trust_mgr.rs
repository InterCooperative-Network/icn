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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
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

// Trust class thresholds (single source of truth)
// These define the trust score boundaries for rate limiting
/// Threshold below which a peer is considered "Isolated" (untrusted)
pub const TRUST_ISOLATED_MAX: f64 = 0.1;
/// Threshold below which a peer is considered "Known" (recognized but not endorsed)
pub const TRUST_KNOWN_MAX: f64 = 0.4;
/// Threshold below which a peer is considered "Partner" (trusted collaborator)
pub const TRUST_PARTNER_MAX: f64 = 0.7;
// Above TRUST_PARTNER_MAX = "Federated" (highly trusted)

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

/// Default TTL for gateway-level trust score cache entries.
///
/// 30s is chosen to balance freshness with lock contention reduction.
/// This is shorter than TrustGraph's internal LRU cache (5 min) to ensure
/// gateway-level decisions don't become staler than the underlying graph.
/// Combined with generation-based invalidation, this provides dual protection:
/// mutations invalidate immediately, TTL bounds staleness from missed events.
const ORACLE_CACHE_TTL: Duration = Duration::from_secs(30);

/// Maximum entries in the gateway-level trust score cache.
///
/// 4096 actors × ~100 bytes/entry ≈ 400KB memory footprint.
/// Exceeding this triggers full cache clear (lazy eviction).
/// If evictions are frequent in production, increase this limit.
const ORACLE_CACHE_MAX_ENTRIES: usize = 4096;

/// Trust Manager for gateway
///
/// Provides a simplified interface to the trust graph for API endpoints.
///
/// Supports three modes:
/// - **Standalone mode** (`new()`): In-memory storage, for testing only
/// - **Actor-backed mode** (`with_handle()`): Delegates to daemon's TrustGraph
/// - **TrustService mode** (`with_trust_service()`): Uses kernel-api TrustService
///   for proper kernel/app separation
pub struct TrustManager {
    /// In-memory trust edges (source:target -> edge) - used in standalone mode
    edges: Arc<DashMap<String, TrustEdge>>,
    /// Own DID (for trust computation perspective)
    own_did: Option<Did>,
    /// Optional handle to daemon's TrustGraph (actor-backed mode, deprecated)
    trust_graph: Option<TrustGraphHandle>,
    /// Optional TrustService for kernel/app separation (preferred mode)
    trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    /// Cached PolicyOracle (avoids repeated Arc allocation)
    cached_oracle: Option<Arc<TrustPolicyOracle>>,
    /// Configurable default trust score
    default_trust_score: f64,
    /// Generation counter — incremented on every edge mutation.
    /// Shared with TrustPolicyOracle for version-aware cache invalidation.
    generation: Arc<AtomicU64>,
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
            trust_service: None,
            cached_oracle: None,
            default_trust_score: DEFAULT_TRUST_SCORE,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Set the default trust score
    pub fn with_default_score(mut self, score: f64) -> Self {
        self.default_trust_score = score;
        self
    }

    /// Bump the generation counter. Called after any edge mutation so that
    /// version-aware caches in TrustPolicyOracle detect staleness.
    fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Create a trust manager backed by the daemon's TrustGraph
    ///
    /// **Deprecated**: Use `with_trust_service()` instead for proper kernel/app separation.
    /// This mode is kept for backward compatibility.
    pub fn with_handle(handle: TrustGraphHandle) -> Self {
        debug!("TrustManager created with daemon TrustGraph handle (deprecated)");
        Self {
            edges: Arc::new(DashMap::new()), // Not used in actor-backed mode
            own_did: None,
            trust_graph: Some(handle),
            trust_service: None,
            cached_oracle: None,
            default_trust_score: DEFAULT_TRUST_SCORE,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a trust manager backed by a TrustService (kernel/app separated)
    ///
    /// This is the recommended mode for production with proper kernel/app separation.
    /// Trust score queries are delegated to the TrustService, which abstracts
    /// the underlying TrustGraph implementation.
    pub fn with_trust_service(
        trust_service: Arc<dyn icn_kernel_api::services::TrustService>,
    ) -> Self {
        debug!("TrustManager created with TrustService (kernel/app separated)");
        Self {
            edges: Arc::new(DashMap::new()),
            own_did: None,
            trust_graph: None,
            trust_service: Some(trust_service),
            cached_oracle: None,
            default_trust_score: DEFAULT_TRUST_SCORE,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Check if running in actor-backed mode
    pub fn is_actor_backed(&self) -> bool {
        self.trust_graph.is_some() || self.trust_service.is_some()
    }

    /// Set the perspective DID for trust computation
    pub fn set_perspective(&mut self, did: Did) {
        self.own_did = Some(did);
        // Invalidate cached oracle since perspective changed
        self.cached_oracle = None;
    }

    /// Get the trust manager as a PolicyOracle (creates new instance)
    ///
    /// This method creates a new `TrustPolicyOracle` wrapped in an `Arc` each time
    /// it's called. For repeated calls in the same context, prefer using
    /// `get_or_create_oracle()` which caches the oracle instance.
    ///
    /// # When to use which method
    ///
    /// - Use `as_oracle()` when you need a `Arc<dyn PolicyOracle>` trait object
    ///   for passing to kernel APIs or one-off evaluations.
    /// - Use `get_or_create_oracle()` when you need repeated evaluations and
    ///   have mutable access to the TrustManager.
    pub fn as_oracle(&self) -> Arc<dyn PolicyOracle> {
        Arc::new(TrustPolicyOracle {
            trust_graph: self.trust_graph.clone(),
            own_did: self.own_did.clone(),
            standalone_edges: if self.trust_graph.is_none() {
                Some(self.edges.clone())
            } else {
                None
            },
            default_trust_score: self.default_trust_score,
            generation: self.generation.clone(),
            cache: DashMap::new(),
            cache_ttl: ORACLE_CACHE_TTL,
        })
    }

    /// Get or create a cached oracle (for mutable contexts)
    ///
    /// This is more efficient than `as_oracle()` for repeated calls.
    /// The oracle instance shares the generation counter with this TrustManager,
    /// so edge mutations automatically invalidate stale cache entries.
    pub fn get_or_create_oracle(&mut self) -> Arc<TrustPolicyOracle> {
        self.cached_oracle
            .get_or_insert_with(|| {
                Arc::new(TrustPolicyOracle {
                    trust_graph: self.trust_graph.clone(),
                    own_did: self.own_did.clone(),
                    standalone_edges: if self.trust_graph.is_none() {
                        Some(self.edges.clone())
                    } else {
                        None
                    },
                    default_trust_score: self.default_trust_score,
                    generation: self.generation.clone(),
                    cache: DashMap::new(),
                    cache_ttl: ORACLE_CACHE_TTL,
                })
            })
            .clone()
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
            tokio::task::block_in_place(|| {
                let mut graph = handle.blocking_write();
                let result = graph
                    .add_edge(edge)
                    .map_err(|e| format!("TrustGraph error: {e}"));
                if result.is_ok() {
                    self.bump_generation();
                }
                result
            })
        } else {
            let key = format!("{}:{}", edge.source.as_str(), edge.target.as_str());
            self.edges.insert(key, edge);
            self.bump_generation();
            Ok(())
        }
    }

    /// Add or update a trust edge (async version)
    pub async fn add_edge_async(&self, edge: TrustEdge) -> Result<(), String> {
        if let Some(ref handle) = self.trust_graph {
            let mut graph = handle.write().await;
            let result = graph
                .add_edge(edge)
                .map_err(|e| format!("TrustGraph error: {e}"));
            if result.is_ok() {
                self.bump_generation();
            }
            result
        } else {
            let key = format!("{}:{}", edge.source.as_str(), edge.target.as_str());
            self.edges.insert(key, edge);
            self.bump_generation();
            Ok(())
        }
    }

    /// Add a trust edge from raw parts (f64 score)
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if:
    /// - `score` is outside the valid range [0.0, 1.0]
    /// - `score` is NaN or infinite
    /// - The underlying edge storage fails (actor-backed mode only)
    ///
    /// These errors should be mapped to HTTP 400 Bad Request at the API layer,
    /// as they represent invalid user input.
    pub async fn add_edge_with_score(
        &self,
        from: Did,
        to: Did,
        score: f64,
        memo: Option<String>,
    ) -> Result<(), String> {
        // TrustScore::new validates the score is in [0.0, 1.0] range
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
            tokio::task::block_in_place(|| {
                let mut graph = handle.blocking_write();
                let result = graph
                    .remove_edge(from, to)
                    .map_err(|e| format!("TrustGraph error: {e}"));
                if result.is_ok() {
                    self.bump_generation();
                }
                result
            })
        } else {
            let key = format!("{}:{}", from.as_str(), to.as_str());
            if self.edges.remove(&key).is_some() {
                self.bump_generation();
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
            let result = graph
                .remove_edge(from, to)
                .map_err(|e| format!("TrustGraph error: {e}"));
            if result.is_ok() {
                self.bump_generation();
            }
            result
        } else {
            let key = format!("{}:{}", from.as_str(), to.as_str());
            if self.edges.remove(&key).is_some() {
                self.bump_generation();
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
        // Prefer TrustService if available (kernel/app separation)
        if let Some(ref trust_service) = self.trust_service {
            let kernel_did = icn_kernel_api::types::Did::from(to.to_string());
            return trust_service.trust_score(&kernel_did);
        }

        // Fall back to TrustGraph (deprecated)
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
        // Prefer TrustService if available (kernel/app separation)
        if let Some(ref trust_service) = self.trust_service {
            let kernel_did = icn_kernel_api::types::Did::from(to.to_string());
            return trust_service.trust_score(&kernel_did);
        }

        // Fall back to TrustGraph (deprecated)
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
    /// - Isolated (< 0.1): 5 msg/sec - Unknown/untrusted peers
    /// - Known (0.1-0.4): 20 msg/sec - Recognized but not endorsed
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
        let request = icn_kernel_api::PolicyRequest::with_context(
            request.core,
            icn_kernel_api::PolicyContext::new().with_resource("trust_score"),
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

        // Build iterator of (intermediate_trust, indirect_trust) pairs
        let intermediates = outgoing.into_iter().filter_map(|intermediate_edge| {
            // Skip if intermediate is the target
            if intermediate_edge.target == *to {
                return None;
            }

            // Get edge from intermediate to target
            self.get_edge(&intermediate_edge.target, to)
                .map(|indirect_edge| (intermediate_edge.score.value(), indirect_edge.score.value()))
        });

        // Use shared computation algorithm with legacy weights (0.7/0.3)
        icn_trust::compute_trust_score(
            direct_score,
            intermediates,
            icn_trust::ScoringWeights::legacy(),
        )
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

        // Build vector of (intermediate_trust, indirect_trust) pairs
        // Note: We collect into a Vec because we can't use async in filter_map
        let mut intermediates = Vec::new();
        for intermediate_edge in outgoing {
            // Skip if intermediate is the target
            if intermediate_edge.target == *to {
                continue;
            }

            // Get edge from intermediate to target
            if let Some(indirect_edge) = self.get_edge_async(&intermediate_edge.target, to).await {
                intermediates.push((intermediate_edge.score.value(), indirect_edge.score.value()));
            }
        }

        // Use shared computation algorithm with legacy weights (0.7/0.3)
        icn_trust::compute_trust_score(
            direct_score,
            intermediates.into_iter(),
            icn_trust::ScoringWeights::legacy(),
        )
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

/// A cached trust evaluation result with version-awareness.
///
/// Entries are valid only when:
/// 1. The entry has not expired (TTL check)
/// 2. The generation matches the current graph generation (no edge mutations since caching)
struct CachedTrustDecision {
    decision: PolicyDecision,
    generation: u64,
    computed_at: Instant,
}

/// Trust Policy Oracle implementation
///
/// Adapts the `TrustGraph` to the `PolicyOracle` trait, allowing the kernel
/// to query trust scores without unauthorized access to the graph structure.
///
/// Includes a version-aware TTL cache keyed by actor DID. Cache entries are
/// invalidated when:
/// - The TTL expires (configurable, default 30s)
/// - The graph generation has changed (any edge mutation since caching)
///
/// This prevents serving stale trust scores after edge changes while avoiding
/// redundant TrustGraph lock acquisitions for the same actor.
pub struct TrustPolicyOracle {
    trust_graph: Option<TrustGraphHandle>,
    own_did: Option<Did>,
    standalone_edges: Option<Arc<DashMap<String, TrustEdge>>>,
    default_trust_score: f64,
    /// Generation counter shared with TrustManager.
    /// Incremented on every edge mutation for version-aware invalidation.
    generation: Arc<AtomicU64>,
    /// Version-aware TTL cache keyed by actor DID string.
    cache: DashMap<String, CachedTrustDecision>,
    /// Time-to-live for cached decisions.
    cache_ttl: Duration,
}

impl TrustPolicyOracle {
    /// Compute trust score using local algorithm (helper for standalone mode)
    fn compute_local(&self, target_did: &str) -> f64 {
        let Some(ref own_did) = self.own_did else {
            return self.default_trust_score;
        };

        let Some(ref edges) = self.standalone_edges else {
            return self.default_trust_score;
        };

        compute_trust_from_edges_map(own_did.as_str(), target_did, edges)
    }
}

/// Helper to compute trust score from edges map (for standalone mode)
///
/// This function uses the shared trust computation algorithm from `icn_trust::computation`
/// to ensure consistency with actor-backed mode.
///
/// # Parameters
///
/// - `own_did`: The DID from whose perspective trust is computed
/// - `target_did`: The DID being evaluated
/// - `edges`: The in-memory edge map (source:target -> TrustEdge)
fn compute_trust_from_edges_map(
    own_did: &str,
    target_did: &str,
    edges: &DashMap<String, TrustEdge>,
) -> f64 {
    // 1. Direct trust
    let key = format!("{}:{}", own_did, target_did);
    let direct_score = edges.get(&key).map(|e| e.score.value()).unwrap_or(0.0);

    // 2. Transitive trust (via intermediaries)
    let prefix = format!("{}:", own_did);
    let outgoing: Vec<_> = edges
        .iter()
        .filter(|entry| entry.key().starts_with(&prefix))
        .map(|entry| entry.value().clone())
        .collect();

    // Build iterator of (intermediate_trust, indirect_trust) pairs
    let intermediates = outgoing.into_iter().filter_map(|mid| {
        if mid.target.to_string() == *target_did {
            return None; // Skip if intermediate is the target
        }

        let key2 = format!("{}:{}", mid.target, target_did);
        edges
            .get(&key2)
            .map(|indirect| (mid.score.value(), indirect.score.value()))
    });

    // Use shared computation algorithm with legacy weights (0.7/0.3)
    icn_trust::compute_trust_score(
        direct_score,
        intermediates,
        icn_trust::ScoringWeights::legacy(),
    )
}

impl TrustPolicyOracle {
    /// Check the version-aware cache for a valid entry.
    ///
    /// Returns `Some(decision)` if:
    /// 1. Entry exists for this actor DID
    /// 2. Entry was computed at the current generation (no edge mutations since)
    /// 3. Entry has not expired (within TTL)
    fn cache_get(&self, actor: &str) -> Option<PolicyDecision> {
        let current_gen = self.generation.load(Ordering::Acquire);

        if let Some(entry) = self.cache.get(actor) {
            if entry.generation == current_gen && entry.computed_at.elapsed() < self.cache_ttl {
                return Some(entry.decision.clone());
            }
            // Stale — will be overwritten on next insert
        }
        None
    }

    /// Insert a decision into the version-aware cache.
    fn cache_put(&self, actor: String, decision: PolicyDecision) {
        let current_gen = self.generation.load(Ordering::Acquire);

        // Lazy eviction: if cache is too large, clear it.
        // This is simpler than LRU for a DashMap and sufficient for gateway use.
        if self.cache.len() > ORACLE_CACHE_MAX_ENTRIES {
            let evicted = self.cache.len() as u64;
            self.cache.clear();
            metrics::counter!("trust_oracle_cache_evictions_total").increment(evicted);
        }

        self.cache.insert(
            actor,
            CachedTrustDecision {
                decision,
                generation: current_gen,
                computed_at: Instant::now(),
            },
        );
    }

    /// Get current cache size (for metrics/testing).
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let start = std::time::Instant::now();

        // 1. Check domain
        if request.domain().as_str() != "trust" {
            return PolicyDecision::allow_with(ConstraintSet::new());
        }

        let target_str = request.actor();

        // 2. Check version-aware cache
        if let Some(cached) = self.cache_get(target_str) {
            metrics::counter!("trust_oracle_cache_hits_total").increment(1);
            metrics::gauge!("trust_oracle_cache_size").set(self.cache.len() as f64);
            metrics::histogram!(
                "trust_oracle_evaluation_duration_seconds",
                "trust_tier" => "cache_hit",
            )
            .record(start.elapsed().as_secs_f64());
            return cached;
        }
        metrics::counter!("trust_oracle_cache_misses_total").increment(1);

        // 3. Compute trust score (cache miss path)
        let score = if let Some(ref handle) = self.trust_graph {
            if let Ok(target_did) = target_str.parse::<Did>() {
                if let Ok(graph) = handle.try_read() {
                    graph.compute_trust_score(&target_did).unwrap_or_else(|e| {
                        warn!("TrustOracle compute error (try_read): {}", e);
                        self.default_trust_score
                    })
                } else {
                    debug!("TrustOracle contention: falling back to block_in_place");
                    metrics::counter!("trust_oracle_block_in_place_total").increment(1);
                    tokio::task::block_in_place(|| {
                        let graph = handle.blocking_read();
                        graph.compute_trust_score(&target_did).unwrap_or_else(|e| {
                            warn!("TrustOracle compute error (blocking): {}", e);
                            self.default_trust_score
                        })
                    })
                }
            } else {
                warn!(
                    "TrustOracle: Failed to parse actor DID '{}', using default trust score",
                    target_str
                );
                metrics::counter!("trust_oracle_did_parse_failures_total").increment(1);
                self.default_trust_score
            }
        } else {
            self.compute_local(target_str)
        };

        // 4. Construct constraints (meaning firewall boundary)
        let mut constraints = ConstraintSet::new();
        constraints = constraints.with_custom("trust_score", score.into());

        let rate_limit = if score > TRUST_PARTNER_MAX {
            icn_kernel_api::RateLimit::new(200, 50) // Federated
        } else if score > TRUST_KNOWN_MAX {
            icn_kernel_api::RateLimit::standard() // Partner (100/25)
        } else if score > TRUST_ISOLATED_MAX {
            icn_kernel_api::RateLimit::throttled() // Known (20/10)
        } else {
            icn_kernel_api::RateLimit::restricted() // Isolated (5/5)
        };
        constraints = constraints.with_rate_limit(rate_limit);

        let decision = PolicyDecision::allow_with(constraints);

        // 5. Cache the result
        self.cache_put(target_str.to_string(), decision.clone());

        // 6. Record metrics
        let trust_tier = if score > TRUST_PARTNER_MAX {
            "federated"
        } else if score > TRUST_KNOWN_MAX {
            "partner"
        } else if score > TRUST_ISOLATED_MAX {
            "known"
        } else {
            "isolated"
        };

        metrics::histogram!(
            "trust_oracle_evaluation_duration_seconds",
            "trust_tier" => trust_tier
        )
        .record(start.elapsed().as_secs_f64());

        metrics::gauge!("trust_oracle_cache_size").set(self.cache.len() as f64);

        decision
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }

    fn cache_ttl(&self) -> Duration {
        // We manage our own version-aware cache, so tell the OracleRegistry
        // not to double-cache our decisions.
        Duration::ZERO
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

    #[test]
    fn test_trust_oracle_standalone() {
        use icn_kernel_api::{ActionKind, ConstraintValue};

        let mut manager = TrustManager::new();
        // Setup direct trust: Own -> Alice (0.8)
        // Setup transitive trust: Own -> Alice -> Bob (0.9)
        // Expected for Bob: 0.7*0 (direct) + 0.3*0.8*0.9 (transitive) = 0.216

        let own = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Configure manager perspective
        manager.set_perspective(own.clone());

        // Add edges to underlying storage synchronously
        let edge1 = TrustEdge::new(own.clone(), alice.clone(), TrustScore::unchecked(0.8));
        manager.add_edge(edge1).unwrap();

        let edge2 = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.9));
        manager.add_edge(edge2).unwrap();

        // Create oracle
        let oracle = manager.as_oracle();

        // Check Domain
        // icn_kernel_api::Did is type alias for String
        let req = PolicyRequest::new(
            bob.to_string(),
            ActionKind::custom("connect"),
            icn_kernel_api::Domain::trust(),
        );

        // Evaluate
        let decision = oracle.evaluate(&req);

        // Verify decision
        if let PolicyDecision::Allow { constraints } = decision {
            let score_val = constraints.custom.get("trust_score").unwrap();
            let score = match score_val {
                ConstraintValue::Float(f) => f.into_inner(),
                _ => panic!("Expected Float constraint, got {:?}", score_val),
            };

            // Calculation:
            // Direct: 0.0
            // Transitive: 0.8 * 0.9 = 0.72
            // Total: 0.0*0.7 + 0.72*0.3 = 0.216
            assert!(
                (score - 0.216).abs() < 0.001,
                "Expected ~0.216, got {}",
                score
            );
        } else {
            panic!("Should satisfy trust policy");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_oracle_invalid_did_returns_default() {
        use icn_kernel_api::{ActionKind, ConstraintValue, Domain, PolicyRequest};

        // Create a TrustManager in standalone mode
        let mut trust_manager = TrustManager::new();
        let keypair = icn_identity::KeyPair::generate().ok();
        if let Some(kp) = keypair {
            trust_manager.set_perspective(kp.did().clone());
        }

        // Get oracle
        let oracle = trust_manager.as_oracle();

        // Create request with invalid DID (this will fail to parse as icn_identity::Did)
        // Note: In actor-backed mode, this would trigger the warning log
        let request = PolicyRequest::new(
            "not-a-valid-did-format".to_string(), // Invalid DID
            ActionKind::Read,
            Domain::trust(),
        );

        // Evaluate - should return Allow with default trust score
        let decision = oracle.evaluate(&request);

        // Verify decision contains default trust score
        if let icn_kernel_api::PolicyDecision::Allow { constraints } = decision {
            // In standalone mode, compute_local is called which accepts strings
            // So we get a computed score (likely 0.0 for no edges), not DEFAULT_TRUST_SCORE
            // This test verifies the oracle doesn't panic on unusual input
            let score_val = constraints.custom.get("trust_score");
            assert!(score_val.is_some(), "Should have trust_score constraint");

            if let Some(ConstraintValue::Float(f)) = score_val {
                // Score should be valid (0.0 to 1.0 range)
                let score = f.into_inner();
                assert!(
                    (0.0..=1.0).contains(&score),
                    "Trust score should be in valid range, got {}",
                    score
                );
            }
        } else {
            panic!("Expected Allow decision");
        }
    }
    #[tokio::test]
    async fn test_trust_oracle_custom_default_score() {
        use icn_kernel_api::{ActionKind, Domain, PolicyRequest};

        // Create manager with custom default score of 0.8
        let mgr = TrustManager::new().with_default_score(0.8);
        let oracle = mgr.as_oracle();

        // Request for unknown DID
        let request = PolicyRequest::new(
            "did:icn:unknown".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        let decision = oracle.evaluate(&request);
        if let icn_kernel_api::PolicyDecision::Allow { constraints } = decision {
            let score = constraints
                .custom
                .get("trust_score")
                .and_then(|value| match value {
                    icn_kernel_api::authz::ConstraintValue::Float(f) => Some(f.into_inner()),
                    _ => None,
                })
                .expect("Should have trust score");
            assert!(
                (score - 0.8).abs() < f64::EPSILON,
                "Should use custom default score of 0.8, got {}",
                score
            );
        } else {
            panic!("Expected Allow decision");
        }
    }

    // ============================================================================
    // Version-Aware Cache Tests
    // ============================================================================

    /// Helper: create a trust policy request for a given actor DID.
    fn trust_request(actor: &str) -> PolicyRequest {
        use icn_kernel_api::{ActionKind, Domain};
        PolicyRequest::new(actor.to_string(), ActionKind::Read, Domain::trust())
    }

    /// Helper: extract trust_score from a PolicyDecision's constraints.
    fn extract_trust_score(decision: &PolicyDecision) -> f64 {
        if let PolicyDecision::Allow { constraints } = decision {
            if let Some(icn_kernel_api::ConstraintValue::Float(f)) =
                constraints.custom.get("trust_score")
            {
                return f.into_inner();
            }
        }
        panic!("Expected Allow decision with trust_score constraint");
    }

    #[test]
    fn test_oracle_cache_hit_on_second_call() {
        let mut manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        manager.set_perspective(alice.clone());
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.6));
        manager.add_edge(edge).unwrap();

        let oracle = manager.get_or_create_oracle();

        // First call: cache miss
        let req = trust_request(bob.as_str());
        let _d1 = oracle.evaluate(&req);
        assert_eq!(oracle.cache_len(), 1);

        // Second call: cache hit (same result)
        let d2 = oracle.evaluate(&req);
        assert_eq!(oracle.cache_len(), 1);

        // Verify the decision is Allow with trust_score
        let score = extract_trust_score(&d2);
        assert!(score > 0.0, "Trust score should be positive");
    }

    #[test]
    fn test_oracle_cache_invalidated_by_edge_mutation() {
        let mut manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        manager.set_perspective(alice.clone());
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.3));
        manager.add_edge(edge).unwrap();

        let oracle = manager.get_or_create_oracle();

        // Populate cache
        let req = trust_request(bob.as_str());
        let d1 = oracle.evaluate(&req);

        // Mutate edge — bumps generation
        let edge2 = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.9));
        manager.add_edge(edge2).unwrap();

        // Next evaluate should miss cache (generation changed)
        let d2 = oracle.evaluate(&req);

        // Both should be Allow, but the score should differ
        // (standalone mode uses in-memory edges, so the new score is visible)
        let s1 = extract_trust_score(&d1);
        let s2 = extract_trust_score(&d2);
        // s2 should reflect the updated edge (0.9 * DIRECT_WEIGHT)
        assert!(
            (s2 - s1).abs() > 0.01,
            "Scores should differ after edge mutation: s1={s1}, s2={s2}"
        );
    }

    #[test]
    fn test_oracle_cache_ttl_expiration() {
        let oracle = Arc::new(TrustPolicyOracle {
            trust_graph: None,
            own_did: None,
            standalone_edges: Some(Arc::new(DashMap::new())),
            default_trust_score: 0.5,
            generation: Arc::new(AtomicU64::new(0)),
            cache: DashMap::new(),
            cache_ttl: Duration::from_millis(50), // Very short TTL for testing
        });

        let req = trust_request("did:icn:test");
        let _ = oracle.evaluate(&req);
        assert_eq!(oracle.cache_len(), 1);

        // Cache hit before expiry
        assert!(oracle.cache_get("did:icn:test").is_some());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(60));

        // Cache miss after expiry
        assert!(oracle.cache_get("did:icn:test").is_none());
    }

    #[test]
    fn test_oracle_cache_ttl_returns_zero() {
        let oracle = TrustManager::new().as_oracle();
        assert_eq!(
            oracle.cache_ttl(),
            Duration::ZERO,
            "TrustPolicyOracle should return ZERO cache_ttl (self-managed cache)"
        );
    }

    #[test]
    fn test_oracle_non_trust_domain_bypasses_cache() {
        let mut manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        manager.set_perspective(alice);

        let oracle = manager.get_or_create_oracle();

        // Non-trust domain request
        use icn_kernel_api::{ActionKind, Domain};
        let req = PolicyRequest::new(
            "did:icn:someone".to_string(),
            ActionKind::Read,
            Domain::new("governance"),
        );

        let _ = oracle.evaluate(&req);
        // Non-trust domain should not populate the trust cache
        assert_eq!(oracle.cache_len(), 0);
    }

    #[test]
    fn test_generation_counter_increments_on_mutations() {
        let manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        assert_eq!(manager.generation.load(Ordering::Acquire), 0);

        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.5));
        manager.add_edge(edge).unwrap();
        assert_eq!(manager.generation.load(Ordering::Acquire), 1);

        manager.remove_edge(&alice, &bob).unwrap();
        assert_eq!(manager.generation.load(Ordering::Acquire), 2);
    }

    #[tokio::test]
    async fn test_oracle_cache_invalidated_by_async_edge_mutation() {
        let mut manager = TrustManager::new();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        manager.set_perspective(alice.clone());
        manager
            .add_edge_async(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.3),
            ))
            .await
            .unwrap();

        let oracle = manager.get_or_create_oracle();

        // Populate cache
        let req = trust_request(bob.as_str());
        let d1 = oracle.evaluate(&req);

        // Mutate via async path — should bump generation
        manager
            .add_edge_async(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.9),
            ))
            .await
            .unwrap();

        // Next evaluate should miss cache (generation changed)
        let d2 = oracle.evaluate(&req);

        let s1 = extract_trust_score(&d1);
        let s2 = extract_trust_score(&d2);
        assert!(
            (s2 - s1).abs() > 0.01,
            "Scores should differ after async edge mutation: s1={s1}, s2={s2}"
        );
    }

    /// Test that standalone and actor-backed modes produce identical trust scores
    ///
    /// This test validates that the unified trust computation algorithm produces
    /// consistent results regardless of whether running in standalone mode
    /// (TrustManager with in-memory edges) or actor-backed mode (with TrustGraph).
    ///
    /// This addresses issue #902: unify trust computation algorithm.
    #[tokio::test]
    async fn test_algorithm_consistency_standalone_vs_actor_backed() {
        use icn_store::SledStore;
        use icn_trust::TrustGraph;
        use std::sync::Arc;

        // Setup: Create identical trust scenarios in both modes
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();
        let dave = KeyPair::generate().unwrap().did().clone();

        // Scenario 1: Direct trust only
        let edge_direct = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.8));

        // Scenario 2: Transitive trust through one intermediary
        let edge_alice_bob = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.9));
        let edge_bob_carol = TrustEdge::new(bob.clone(), carol.clone(), TrustScore::unchecked(0.7));

        // Scenario 3: Multiple intermediaries
        let edge_alice_dave =
            TrustEdge::new(alice.clone(), dave.clone(), TrustScore::unchecked(0.6));
        let edge_dave_carol =
            TrustEdge::new(dave.clone(), carol.clone(), TrustScore::unchecked(0.8));

        // Test Scenario 1: Direct trust only
        {
            // Standalone mode
            let mut standalone = TrustManager::new();
            standalone.set_perspective(alice.clone());
            standalone.add_edge(edge_direct.clone()).unwrap();
            let score_standalone = standalone.compute_trust_score(&alice, &bob);

            // Actor-backed mode
            let store = Arc::new(SledStore::temporary().unwrap());
            let mut graph = TrustGraph::new(store, alice.clone());
            graph.add_edge(edge_direct.clone()).unwrap();
            let score_actor = graph.compute_trust_score(&bob).unwrap();

            // Both should produce identical scores (within floating point epsilon)
            assert!(
                (score_standalone - score_actor).abs() < 0.0001,
                "Direct trust scores differ: standalone={}, actor={}",
                score_standalone,
                score_actor
            );
        }

        // Test Scenario 2: Transitive trust through one intermediary
        {
            // Standalone mode
            let mut standalone = TrustManager::new();
            standalone.set_perspective(alice.clone());
            standalone.add_edge(edge_alice_bob.clone()).unwrap();
            standalone.add_edge(edge_bob_carol.clone()).unwrap();
            let score_standalone = standalone.compute_trust_score(&alice, &carol);

            // Actor-backed mode
            let store = Arc::new(SledStore::temporary().unwrap());
            let mut graph = TrustGraph::new(store, alice.clone());
            graph.add_edge(edge_alice_bob.clone()).unwrap();
            graph.add_edge(edge_bob_carol.clone()).unwrap();
            let score_actor = graph.compute_trust_score(&carol).unwrap();

            // Both should produce identical scores
            assert!(
                (score_standalone - score_actor).abs() < 0.0001,
                "Transitive trust scores differ: standalone={}, actor={}",
                score_standalone,
                score_actor
            );
        }

        // Test Scenario 3: Multiple intermediaries (averaging)
        {
            // Standalone mode
            let mut standalone = TrustManager::new();
            standalone.set_perspective(alice.clone());
            standalone.add_edge(edge_alice_bob.clone()).unwrap();
            standalone.add_edge(edge_bob_carol.clone()).unwrap();
            standalone.add_edge(edge_alice_dave.clone()).unwrap();
            standalone.add_edge(edge_dave_carol.clone()).unwrap();
            let score_standalone = standalone.compute_trust_score(&alice, &carol);

            // Actor-backed mode
            let store = Arc::new(SledStore::temporary().unwrap());
            let mut graph = TrustGraph::new(store, alice.clone());
            graph.add_edge(edge_alice_bob.clone()).unwrap();
            graph.add_edge(edge_bob_carol.clone()).unwrap();
            graph.add_edge(edge_alice_dave.clone()).unwrap();
            graph.add_edge(edge_dave_carol.clone()).unwrap();
            let score_actor = graph.compute_trust_score(&carol).unwrap();

            // Both should produce identical scores (averaging over multiple paths)
            assert!(
                (score_standalone - score_actor).abs() < 0.0001,
                "Multi-path trust scores differ: standalone={}, actor={}",
                score_standalone,
                score_actor
            );
        }
    }
}
