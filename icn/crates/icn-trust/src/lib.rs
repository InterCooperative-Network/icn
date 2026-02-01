//! ICN Trust - Trust graph management and policy enforcement
//!
//! ## Cache Invalidation Strategy
//!
//! Trust scores are cached with a configurable TTL (default 5 minutes in the
//! LRU cache). When trust edges change, affected scores must be invalidated
//! immediately to prevent stale trust decisions (security issue #878).
//!
//! **Invalidation scope**: When edge A→B changes:
//! 1. **Direct**: Invalidate B's cached score
//! 2. **Transitive**: Invalidate all C where B→C exists
//!
//! This is bounded to one hop beyond the modified edge, which is sufficient
//! for our two-hop scoring algorithm (direct + one level of transitivity).
//! See [`TrustGraph::invalidate_affected`] for implementation details.
#![allow(missing_docs)]
// Prevent panics in production code paths
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! This crate provides trust graph infrastructure for ICN with three orthogonal
//! trust dimensions:
//! - **Social**: Peer endorsements and community participation
//! - **EconomicReliability**: Payment history and credit behavior
//! - **TechnicalReliability**: Node uptime and task success rates
//!
//! # Multi-Graph Architecture (Phase 21)
//!
//! ICN uses three separate trust graphs to prevent any single clique from
//! gaining cross-domain influence:
//!
//! - **Social Graph**: "I know you, we've worked together"
//!   - Used for: Connection priority, gossip bandwidth, topic access
//!   - Scoring: 60% direct, 40% transitive (reputation spreads)
//!
//! - **Economic Graph**: "You have a consistent record of clearing obligations"
//!   - Used for: Credit limits, dispute weighting, federation trade limits
//!   - Scoring: 80% direct, 20% transitive (payment history matters most)
//!
//! - **Technical Graph**: "Your node behaves correctly under load"
//!   - Used for: Compute scheduling, contract execution, storage selection
//!   - Scoring: 90% direct, 10% transitive (your node's performance is yours)
//!
//! # Quick Start
//!
//! ```ignore
//! use icn_trust::{MultiTrustGraph, TrustGraphType, TrustEdge};
//!
//! // Create multi-graph container
//! let mut multi = MultiTrustGraph::new(store, own_did);
//!
//! // Add edges to specific graphs
//! multi.economic_mut().add_edge(TrustEdge::new(alice, bob, 0.8))?;
//! multi.technical_mut().add_edge(TrustEdge::new(alice, bob, 0.6))?;
//!
//! // Typed access for domain-specific operations
//! let credit_score = multi.economic().compute_trust_score(&member)?;
//! let tech_score = multi.technical().compute_trust_score(&node)?;
//!
//! // Combined score for backward compatibility
//! let combined = multi.compute_combined_trust_score(&peer)?;
//! ```

pub mod anomaly;
pub mod attestation;
pub mod computation;
pub mod evidence;
pub mod evidence_validator;
pub mod facade;
pub mod multi_graph;
pub mod pathfinder;
pub mod precompute;
pub mod reachability;
pub mod sybil;
pub mod trust_cache;
pub mod typed_graph;
pub mod types;

pub use anomaly::{anomalies_to_sybil_flags, TrustAnomaly, TrustGraphAnalyzer};
pub use attestation::{TrustAttestation, TrustRevocation};
pub use computation::compute_trust_score;
pub use evidence::{
    EvidenceValidationError, EvidenceValidationResult, TechnicalMetricType, TrustEvidence,
};
pub use evidence_validator::{EvidenceValidator, EvidenceValidatorConfig};
pub use facade::TrustGraphFacade;
pub use multi_graph::MultiTrustGraph;
pub use pathfinder::{PathfinderConfig, TrustPathfinder};
pub use precompute::TrustPrecomputer;
pub use reachability::ReachabilityFilter;
pub use sybil::{
    SybilFlag, SybilFlagType, SybilResistance, SybilResistanceConfig, VerificationMethod,
    VerificationStatus,
};
pub use trust_cache::TrustCache;
pub use typed_graph::TypedTrustGraph;
pub use types::{ScopeId, ScoringWeights, TrustGraphType, TrustScore, TrustScoreError};

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

/// Trust classification for a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustClass {
    /// Not yet evaluated (0.0-0.1)
    Isolated = 0,
    /// Known but not trusted (0.1-0.4)
    Known = 1,
    /// Trusted partner (0.4-0.7)
    Partner = 2,
    /// Federated peer (0.7-1.0)
    Federated = 3,
}

impl TrustClass {
    /// Convert a trust score to a trust class
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s < 0.1 => TrustClass::Isolated,
            s if s < 0.4 => TrustClass::Known,
            s if s < 0.7 => TrustClass::Partner,
            _ => TrustClass::Federated,
        }
    }

    /// Get the minimum score for this class
    pub fn min_score(&self) -> f64 {
        match self {
            TrustClass::Isolated => 0.0,
            TrustClass::Known => 0.1,
            TrustClass::Partner => 0.4,
            TrustClass::Federated => 0.7,
        }
    }
}

/// A trust edge in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEdge {
    /// Source DID (truster)
    pub source: Did,
    /// Target DID (trustee)
    pub target: Did,
    /// Labels describing the trust relationship
    pub labels: Vec<String>,
    /// Trust score (0.0 to 1.0)
    pub score: TrustScore,
    /// Typed evidence supporting this trust relationship
    ///
    /// Each evidence item is a verifiable reference that can be validated
    /// against actual records in the system. See [`TrustEvidence`] for types.
    ///
    /// # Migration Note
    ///
    /// When deserializing edges from storage, this field defaults to an empty
    /// vector if not present. Callers should call [`TrustEdge::migrate_legacy_evidence`]
    /// after deserialization to convert `legacy_evidence` to typed evidence.
    /// Otherwise, legacy evidence will be silently ignored.
    #[serde(default)]
    pub evidence: Vec<TrustEvidence>,
    /// Legacy string evidence (for backward compatibility during migration)
    ///
    /// **Deprecated**: New edges should not use this field. Existing edges
    /// will have their string evidence migrated to [`TrustEvidence::Legacy`]
    /// in the `evidence` field when [`TrustEdge::migrate_legacy_evidence`] is called.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legacy_evidence: Vec<String>,
    /// Optional expiration timestamp
    pub expires_at: Option<u64>,
    /// Creation timestamp
    pub created_at: u64,
    /// The type of trust graph this edge belongs to
    ///
    /// Defaults to `Social` for backward compatibility with edges
    /// created before multi-graph support was added.
    #[serde(default)]
    pub graph_type: TrustGraphType,
    /// Optional scope identifier for scope-bounded trust (Wave 2)
    ///
    /// When set, this edge is scoped to a specific organizational boundary
    /// (cooperative, federation, domain, or topic class). Trust computation
    /// can be performed within scope boundaries to prevent centralization.
    ///
    /// Defaults to `None` (global scope) for backward compatibility.
    ///
    /// # Firewall Compliance
    ///
    /// This field lives ONLY in `icn-trust`. It never crosses into kernel
    /// crates. PolicyOracles use scope-bounded trust to compute constraint
    /// values, but the kernel receives only opaque `ConstraintSet` data.
    #[serde(default)]
    pub scope_id: Option<ScopeId>,
}

impl TrustEdge {
    /// Create a new trust edge (defaults to Social graph type, global scope)
    pub fn new(source: Did, target: Did, score: TrustScore) -> Self {
        Self::new_typed(source, target, score, TrustGraphType::Social)
    }

    /// Create a new trust edge with explicit graph type (global scope)
    pub fn new_typed(
        source: Did,
        target: Did,
        score: TrustScore,
        graph_type: TrustGraphType,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            source,
            target,
            labels: Vec::new(),
            score,
            evidence: Vec::new(),
            legacy_evidence: Vec::new(),
            expires_at: None,
            created_at: now,
            graph_type,
            scope_id: None, // Global scope by default
        }
    }

    /// Create a new trust edge with explicit graph type and scope
    pub fn new_scoped(
        source: Did,
        target: Did,
        score: TrustScore,
        graph_type: TrustGraphType,
        scope_id: ScopeId,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            source,
            target,
            labels: Vec::new(),
            score,
            evidence: Vec::new(),
            legacy_evidence: Vec::new(),
            expires_at: None,
            created_at: now,
            graph_type,
            scope_id: Some(scope_id),
        }
    }

    /// Check if this edge is expired
    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|exp| now > exp)
    }

    /// Set the scope for this edge
    pub fn with_scope(mut self, scope_id: ScopeId) -> Self {
        self.scope_id = Some(scope_id);
        self
    }

    /// Add a label to this edge
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }

    /// Add typed evidence to this edge
    pub fn with_evidence(mut self, evidence: TrustEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }

    /// Add multiple evidence items to this edge
    pub fn with_evidence_list(mut self, evidence: Vec<TrustEvidence>) -> Self {
        self.evidence.extend(evidence);
        self
    }

    /// Migrate legacy string evidence to typed evidence
    ///
    /// This converts all `legacy_evidence` strings to `TrustEvidence::Legacy`
    /// variants and moves them to the `evidence` field. Use during migration
    /// from the old string-based evidence format.
    pub fn migrate_legacy_evidence(&mut self) {
        if self.legacy_evidence.is_empty() {
            return;
        }

        let migrated: Vec<TrustEvidence> = self
            .legacy_evidence
            .drain(..)
            .map(TrustEvidence::from_legacy_string)
            .collect();

        self.evidence.extend(migrated);
    }

    /// Check if this edge has any legacy evidence that needs migration
    pub fn has_legacy_evidence(&self) -> bool {
        !self.legacy_evidence.is_empty()
            || self
                .evidence
                .iter()
                .any(|e| matches!(e, TrustEvidence::Legacy { .. }))
    }

    /// Get all typed evidence items.
    ///
    /// Note: Legacy evidence must be migrated via [`migrate_legacy_evidence`]
    /// before it will be included in this iterator.
    pub fn evidence(&self) -> impl Iterator<Item = &TrustEvidence> {
        self.evidence.iter()
    }

    /// Set expiry time
    pub fn with_expiry(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set the graph type
    pub fn with_graph_type(mut self, graph_type: TrustGraphType) -> Self {
        self.graph_type = graph_type;
        self
    }
}

/// Trust graph manager
///
/// Manages trust edges and computes trust scores from this node's perspective.
/// Each graph can optionally use a storage prefix for namespace isolation
/// when running multiple trust graphs (Social, Economic, Technical).
///
/// # Performance Optimizations (Phase 22)
///
/// - **LRU Cache**: O(1) lookups for recently computed scores (5 min TTL)
/// - **Reachability Filter**: Bloom filter for O(1) "definitely not reachable" queries
/// - **Priority Queue**: Optional pathfinder for early termination when threshold met
pub struct TrustGraph {
    store: Arc<dyn Store>,
    own_did: Did,
    /// LRU cache for trust scores with TTL-based invalidation (Phase 19)
    cache: TrustCache,
    /// Storage key prefix for namespace isolation (e.g., "trust/social")
    storage_prefix: String,
    /// Bloom filter for fast negative lookups (Phase 22)
    reachability: ReachabilityFilter,
}

impl TrustGraph {
    /// Default storage prefix for legacy single-graph mode
    const DEFAULT_PREFIX: &'static str = "trust";

    /// Create a new trust graph with default storage prefix
    pub fn new(store: Arc<dyn Store>, own_did: Did) -> Self {
        Self {
            store,
            own_did,
            cache: TrustCache::new(),
            storage_prefix: Self::DEFAULT_PREFIX.to_string(),
            reachability: ReachabilityFilter::new(),
        }
    }

    /// Create a new trust graph with a custom storage prefix
    ///
    /// This is used by `TypedTrustGraph` to create isolated namespaces for
    /// each trust graph type (social, economic, technical).
    pub fn new_with_prefix(store: Arc<dyn Store>, own_did: Did, prefix: &str) -> Self {
        Self {
            store,
            own_did,
            cache: TrustCache::new(),
            storage_prefix: prefix.to_string(),
            reachability: ReachabilityFilter::new(),
        }
    }

    /// Create a new trust graph with custom cache configuration
    pub fn with_cache_config(
        store: Arc<dyn Store>,
        own_did: Did,
        cache_size: usize,
        cache_ttl: std::time::Duration,
    ) -> Self {
        Self {
            store,
            own_did,
            cache: TrustCache::with_config(cache_size, cache_ttl),
            storage_prefix: Self::DEFAULT_PREFIX.to_string(),
            reachability: ReachabilityFilter::new(),
        }
    }

    /// Create a new trust graph with custom prefix and cache configuration
    pub fn with_prefix_and_cache(
        store: Arc<dyn Store>,
        own_did: Did,
        prefix: &str,
        cache_size: usize,
        cache_ttl: std::time::Duration,
    ) -> Self {
        Self {
            store,
            own_did,
            cache: TrustCache::with_config(cache_size, cache_ttl),
            storage_prefix: prefix.to_string(),
            reachability: ReachabilityFilter::new(),
        }
    }

    /// Returns the storage prefix used by this graph
    pub fn storage_prefix(&self) -> &str {
        &self.storage_prefix
    }

    /// Get the DID of this node
    pub fn own_did(&self) -> &Did {
        &self.own_did
    }

    /// Add or update a trust edge
    pub fn add_edge(&mut self, edge: TrustEdge) -> Result<()> {
        info!(
            "Adding trust edge: {} -> {} (score: {}, prefix: {})",
            edge.source, edge.target, edge.score, self.storage_prefix
        );

        let key = self.edge_key(&edge.source, &edge.target);
        let value = serde_json::to_vec(&edge)?;

        self.store.put(key.as_bytes(), &value)?;

        // Invalidate cache for target and transitively affected DIDs.
        //
        // When edge A→B changes, B's score changes (direct invalidation).
        // Additionally, any DID C where B→C exists may have a stale cached
        // score because the transitive path through B changed. We invalidate
        // those too to prevent serving stale scores.
        self.invalidate_affected(&edge.target);

        // Update reachability filter for both direct and transitive paths
        if edge.source == self.own_did {
            // Direct edge from us: target is reachable
            self.reachability.add_reachable(&edge.target);
        } else {
            // Check if source is someone we directly trust (transitive reachability)
            // If we trust the source, then target is also reachable (2-hop)
            if self.get_edge(&self.own_did, &edge.source)?.is_some() {
                self.reachability.add_reachable(&edge.target);
            }
        }

        Ok(())
    }

    /// Add or update a trust edge with evidence validation
    ///
    /// This method validates all evidence attached to the edge before adding it
    /// to the graph. If any evidence fails validation, the edge is rejected.
    ///
    /// Returns the validation result including any score adjustments suggested
    /// by the evidence validation.
    ///
    /// # Parameters
    ///
    /// - `edge`: The trust edge to add
    /// - `validator`: The evidence validator to use for validation
    ///
    /// # Errors
    ///
    /// Returns an error if evidence validation fails or if storage fails.
    pub fn add_edge_validated(
        &mut self,
        edge: TrustEdge,
        validator: &EvidenceValidator,
    ) -> Result<EvidenceValidationResult> {
        // Validate evidence
        let validation_result =
            validator.validate_all_evidence(&edge.evidence, &edge.source, &edge.target, Some(self));

        if !validation_result.valid {
            debug!(
                "Rejecting edge {} -> {}: evidence validation failed ({} errors)",
                edge.source,
                edge.target,
                validation_result.errors.len()
            );
            return Ok(validation_result);
        }

        // Evidence is valid - add the edge
        self.add_edge(edge)?;

        Ok(validation_result)
    }

    /// Generate storage key for an edge
    fn edge_key(&self, source: &Did, target: &Did) -> String {
        format!(
            "{}/edges/{}:{}",
            self.storage_prefix,
            source.as_str(),
            target.as_str()
        )
    }

    /// Generate storage key prefix for all edges from a source
    fn edge_prefix(&self, source: &Did) -> String {
        format!("{}/edges/{}:", self.storage_prefix, source.as_str())
    }

    /// Generate storage key prefix for scanning all edges
    fn all_edges_prefix(&self) -> String {
        format!("{}/edges/", self.storage_prefix)
    }

    /// Get a trust edge
    pub fn get_edge(&self, source: &Did, target: &Did) -> Result<Option<TrustEdge>> {
        let key = self.edge_key(source, target);

        match self.store.get(key.as_bytes())? {
            Some(value) => {
                let edge: TrustEdge = serde_json::from_slice(&value)?;

                // Check if expired
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();

                if edge.is_expired(now) {
                    Ok(None)
                } else {
                    Ok(Some(edge))
                }
            }
            None => Ok(None),
        }
    }

    /// Get all outgoing edges from a DID
    pub fn get_outgoing_edges(&self, source: &Did) -> Result<Vec<TrustEdge>> {
        let prefix = self.edge_prefix(source);
        let mut edges = Vec::new();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let results = self.store.scan(prefix.as_bytes())?;
        for (_key, value) in results.into_iter() {
            let edge: TrustEdge = serde_json::from_slice(&value)?;
            if !edge.is_expired(now) {
                edges.push(edge);
            }
        }

        Ok(edges)
    }

    /// Get all incoming edges to a DID (edges where target == did)
    ///
    /// Note: This requires scanning all edges in the graph, which is O(n).
    /// Use sparingly for operations like "who trusts me?" queries.
    /// For high-frequency operations, consider caching the results.
    pub fn get_incoming_edges(&self, target: &Did) -> Result<Vec<TrustEdge>> {
        let prefix = self.all_edges_prefix();
        let mut edges = Vec::new();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let results = self.store.scan(prefix.as_bytes())?;
        for (_key, value) in results.into_iter() {
            let edge: TrustEdge = serde_json::from_slice(&value)?;
            if !edge.is_expired(now) && edge.target == *target {
                edges.push(edge);
            }
        }

        Ok(edges)
    }

    /// Compute trust score for a DID using default weights (70% direct, 30% transitive)
    ///
    /// Uses a simplified PageRank-like algorithm:
    /// TrustScore(own -> target) =
    ///     DirectTrust(own -> target) * 0.7 +
    ///     TransitiveTrust(own -> intermediate -> target) * 0.3
    #[instrument(skip(self), fields(target = %target))]
    pub fn compute_trust_score(&self, target: &Did) -> Result<f64> {
        self.compute_trust_score_weighted(target, ScoringWeights::legacy())
    }

    /// Compute trust score for a DID using custom weights
    ///
    /// This allows different trust graph types to use different weighting:
    /// - **Social**: 60/40 - Reputation spreads through networks
    /// - **Economic**: 80/20 - Your payment history matters most
    /// - **Technical**: 90/10 - Your node's performance is yours
    ///
    /// # Performance Optimizations (Phase 22)
    /// 1. LRU cache check (O(1))
    /// 2. Bloom filter for negative lookups (O(1))
    /// 3. Two-hop transitive computation
    ///
    /// # Arguments
    /// * `target` - The DID to compute trust for
    /// * `weights` - Custom direct/transitive weighting
    ///
    /// # Returns
    /// Trust score in range [0.0, 1.0]
    pub fn compute_trust_score_weighted(
        &self,
        target: &Did,
        weights: ScoringWeights,
    ) -> Result<f64> {
        // Record lookup
        icn_obs::metrics::trust::lookups_inc();

        // Check LRU cache first (with TTL validation)
        // Note: Cache doesn't distinguish by weights, so this is only valid
        // when consistently using the same weights (TypedTrustGraph ensures this)
        if let Some(score) = self.cache.get(target) {
            icn_obs::metrics::trust::cache_hits_inc();
            return Ok(score);
        }

        // Fast path: bloom filter check for unreachable DIDs (Phase 22)
        // If the filter says the DID is definitely NOT reachable, return 0 immediately
        if !self.reachability.is_empty() && !self.reachability.may_be_reachable(target) {
            debug!("Bloom filter: {} is definitely not reachable", target);
            self.cache.put(target.clone(), 0.0);
            return Ok(0.0);
        }

        icn_obs::metrics::trust::cache_misses_inc();
        debug!(
            "Computing trust score for {} (weights: {:?})",
            target, weights
        );

        // Get direct trust edge
        let direct_score = self
            .get_edge(&self.own_did, target)?
            .map(|e| e.score.value())
            .unwrap_or(0.0);

        // Get transitive trust (via intermediates we trust)
        let own_edges = self.get_outgoing_edges(&self.own_did)?;

        // Build iterator of (intermediate_trust, indirect_trust) pairs
        let intermediates: Vec<_> = own_edges
            .into_iter()
            .filter(|edge| edge.target != *target) // Skip if intermediate is the target
            .filter_map(|intermediate_edge| {
                // Get edge from intermediate to target
                self.get_edge(&intermediate_edge.target, target)
                    .ok()
                    .flatten()
                    .map(|indirect_edge| {
                        (intermediate_edge.score.value(), indirect_edge.score.value())
                    })
            })
            .collect();

        // Use shared computation algorithm
        let final_score = crate::computation::compute_trust_score(
            direct_score,
            intermediates.iter().copied(),
            weights,
        );

        // Compute transitive score for debug logging
        let transitive_score = if !intermediates.is_empty() {
            intermediates.iter().map(|(a, b)| a * b).sum::<f64>() / intermediates.len() as f64
        } else {
            0.0
        };

        debug!(
            "Trust score for {}: direct={}, transitive={}, final={} (weights: {}/{})",
            target, direct_score, transitive_score, final_score, weights.direct, weights.transitive
        );

        // Cache result (LRU cache with automatic eviction)
        self.cache.put(target.clone(), final_score);

        // Record score distribution
        icn_obs::metrics::trust::score_distribution_record(final_score);

        Ok(final_score)
    }

    /// Compute trust score with early termination when threshold is met
    ///
    /// This is an optimized version that uses a priority queue for path finding
    /// and can terminate early when the minimum threshold is reached.
    ///
    /// # Arguments
    /// * `target` - The DID to compute trust for
    /// * `min_threshold` - Optional minimum score threshold for early termination
    ///
    /// # Returns
    /// Trust score in range [0.0, 1.0]
    pub fn compute_trust_score_with_threshold(
        &self,
        target: &Did,
        min_threshold: Option<f64>,
    ) -> Result<f64> {
        // Record lookup
        icn_obs::metrics::trust::lookups_inc();

        // Check cache first
        if let Some(score) = self.cache.get(target) {
            icn_obs::metrics::trust::cache_hits_inc();
            return Ok(score);
        }

        // Fast path: bloom filter check
        if !self.reachability.is_empty() && !self.reachability.may_be_reachable(target) {
            self.cache.put(target.clone(), 0.0);
            return Ok(0.0);
        }

        icn_obs::metrics::trust::cache_misses_inc();

        // Use pathfinder for optimized computation
        let config = if let Some(threshold) = min_threshold {
            PathfinderConfig::default().with_threshold(threshold)
        } else {
            PathfinderConfig::default()
        };

        let pathfinder = TrustPathfinder::with_config(config);
        let score = pathfinder.find_score(self, target)?;

        // Cache result
        self.cache.put(target.clone(), score);

        Ok(score)
    }

    /// Rebuild the reachability filter from current trust graph
    ///
    /// This should be called periodically or after bulk edge operations
    /// to ensure the bloom filter accurately reflects reachable DIDs.
    pub fn rebuild_reachability_filter(&self) -> Result<()> {
        // Get all DIDs reachable from our node within 2 hops
        let mut reachable = Vec::new();

        // Direct edges (1 hop)
        let direct_edges = self.get_outgoing_edges(&self.own_did)?;
        for edge in &direct_edges {
            reachable.push(edge.target.clone());

            // Transitive edges (2 hops)
            if let Ok(indirect_edges) = self.get_outgoing_edges(&edge.target) {
                for indirect in indirect_edges {
                    reachable.push(indirect.target.clone());
                }
            }
        }

        self.reachability.rebuild(reachable);
        Ok(())
    }

    /// Get a reference to the reachability filter
    pub fn reachability_filter(&self) -> &ReachabilityFilter {
        &self.reachability
    }

    /// Get the trust class for a DID
    pub fn trust_class(&self, did: &Did) -> Result<TrustClass> {
        let score = self.compute_trust_score(did)?;
        Ok(TrustClass::from_score(score))
    }

    /// Remove a trust edge
    pub fn remove_edge(&mut self, source: &Did, target: &Did) -> Result<()> {
        // Invalidate caches before removing the edge from storage. This ordering
        // is defensive: `get_outgoing_edges(target)` inside `invalidate_affected`
        // can still read target's edges, ensuring transitive invalidations are
        // complete even under concurrent edge deletion scenarios.
        self.invalidate_affected(target);

        let key = self.edge_key(source, target);
        self.store.delete(key.as_bytes())?;

        Ok(())
    }

    /// Get all known DIDs from the trust graph
    ///
    /// This scans all edges and extracts unique DIDs (both sources and targets).
    /// Useful for trust-based membership resolution in governance.
    pub fn get_all_known_dids(&self) -> Result<Vec<Did>> {
        use std::collections::HashSet;

        let mut dids: HashSet<Did> = HashSet::new();

        // Scan all edges
        let prefix = self.all_edges_prefix();
        let results = self.store.scan(prefix.as_bytes())?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        for (_key, value) in results {
            let edge: TrustEdge = serde_json::from_slice(&value)?;

            // Skip expired edges
            if edge.is_expired(now) {
                continue;
            }

            dids.insert(edge.source);
            dids.insert(edge.target);
        }

        Ok(dids.into_iter().collect())
    }

    /// Get all DIDs that meet a minimum trust threshold
    ///
    /// Returns DIDs whose computed trust score (from this node's perspective)
    /// is greater than or equal to the threshold.
    pub fn get_dids_above_threshold(&self, threshold: f64) -> Result<Vec<Did>> {
        let all_dids = self.get_all_known_dids()?;
        let mut trusted_dids = Vec::new();

        for did in all_dids {
            // Skip self
            if did == self.own_did {
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
    // Scope-Aware Trust Computation (Wave 2)
    // ============================================================

    /// Compute trust score within a specific scope
    ///
    /// This filters edges to only those matching the given scope before
    /// computing trust. This provides scope-bounded trust computation to
    /// prevent centralization across scope boundaries.
    ///
    /// # Arguments
    ///
    /// * `target` - The DID to compute trust for
    /// * `scope` - The scope to filter edges by
    ///
    /// # Firewall Compliance
    ///
    /// This method is in `icn-trust` (app layer). The kernel never calls this.
    /// PolicyOracles use this to compute scope-aware constraint values.
    pub fn compute_trust_score_in_scope(
        &self,
        target: &Did,
        scope: &crate::ScopeId,
    ) -> Result<f64> {
        let outgoing = self.get_outgoing_edges(&self.own_did)?;
        let now = icn_time::current_timestamp_secs();

        // Filter to scope-matching edges only
        let scoped_edges: Vec<_> = outgoing
            .into_iter()
            .filter(|e| !e.is_expired(now))
            .filter(|e| match &e.scope_id {
                Some(edge_scope) => edge_scope == scope,
                None => scope.is_global(), // Global edges match global scope queries
            })
            .collect();

        // Use pathfinder with scope-filtered edges
        // (Future: could create a scoped pathfinder)
        
        // Compute direct score
        let direct_score = scoped_edges
            .iter()
            .find(|e| e.target == *target)
            .map(|e| e.score.value())
            .unwrap_or(0.0);

        // Compute transitive scores through intermediaries
        let intermediates = scoped_edges
            .iter()
            .filter(|e| e.target != *target) // Don't count direct edge as transitive
            .filter_map(|e| {
                // Look for edges from intermediate to target
                scoped_edges
                    .iter()
                    .find(|indirect| indirect.source == e.target && indirect.target == *target)
                    .map(|indirect| (e.score.value(), indirect.score.value()))
            });

        let weights = crate::ScoringWeights::legacy();
        let score = crate::computation::compute_trust_score(direct_score, intermediates, weights);
        Ok(score)
    }

    /// Get all edges within a specific scope
    ///
    /// Returns all outgoing edges from this node that belong to the given scope.
    pub fn get_edges_in_scope(&self, scope: &crate::ScopeId) -> Result<Vec<TrustEdge>> {
        let outgoing = self.get_outgoing_edges(&self.own_did)?;
        let now = icn_time::current_timestamp_secs();

        Ok(outgoing
            .into_iter()
            .filter(|e| !e.is_expired(now))
            .filter(|e| match &e.scope_id {
                Some(edge_scope) => edge_scope == scope,
                None => scope.is_global(),
            })
            .collect())
    }

    // ============================================================
    // End of TrustGraph Implementation
    // ============================================================

    /// Invalidate cached scores for a DID and all transitively affected DIDs.
    ///
    /// When any edge with `target` as the target node changes (regardless of
    /// source), `target`'s score is directly affected. Additionally, any DID C
    /// where target→C exists may have a stale score because the transitive path
    /// (source→target→C) changed. We invalidate those one-hop-out DIDs as well.
    ///
    /// This is bounded: we only go one hop out from `target`, not the
    /// full graph. The scoring algorithm is two-hop (direct + transitive),
    /// so one additional hop of invalidation is sufficient.
    ///
    /// # Thread safety
    ///
    /// This method must be called while the caller holds a write lock on the
    /// `TrustGraph` (or equivalent exclusive access). This prevents a TOCTOU
    /// race where a concurrent reader could compute and cache a score using
    /// stale edges between the store mutation and cache invalidation. The
    /// actor model in `icn-core` guarantees serial message processing, so
    /// this is satisfied in production. Tests must ensure exclusive access.
    fn invalidate_affected(&self, target: &Did) {
        // Always invalidate the direct target
        self.cache.invalidate(target);

        // Invalidate transitive targets: DIDs that target has outgoing edges to.
        // These scores may depend on trust flowing through `target`.
        match self.get_outgoing_edges(target) {
            Ok(outgoing) => {
                let mut count = 0u64;
                for edge in &outgoing {
                    // Skip self-loops (A→A) — already invalidated above
                    if edge.target == *target {
                        continue;
                    }
                    self.cache.invalidate(&edge.target);
                    count += 1;
                }
                if count > 0 {
                    icn_obs::metrics::scalability::trust_cache_transitive_invalidations_inc(count);
                    debug!(
                        target = %target,
                        downstream_count = count,
                        "Transitive cache invalidation for {target}: {count} downstream DIDs",
                    );
                }
            }
            Err(e) => {
                icn_obs::metrics::scalability::trust_cache_invalidation_errors_inc();
                warn!(
                    "Failed to get outgoing edges during cache invalidation for {}: {}",
                    target, e
                );
            }
        }
    }

    /// Clear the entire trust score cache.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Map trust relationships from old_did to new_did during recovery
    ///
    /// This is called when a social recovery is finalized. All trust relationships
    /// involving the old DID are transferred to the new DID.
    ///
    /// Returns the number of edges migrated.
    pub fn map_did_recovery(&mut self, old_did: &Did, new_did: &Did) -> Result<usize> {
        info!("Mapping trust relationships: {} → {}", old_did, new_did);

        let mut migrated_count = 0;

        // 1. Get all outgoing edges from old_did (old_did as source)
        let outgoing = self.get_outgoing_edges(old_did)?;
        for edge in outgoing {
            // Create new edge with new_did as source
            let new_edge = TrustEdge {
                source: new_did.clone(),
                target: edge.target.clone(),
                labels: edge.labels,
                score: edge.score,
                evidence: edge.evidence,
                legacy_evidence: edge.legacy_evidence,
                expires_at: edge.expires_at,
                created_at: edge.created_at,
                graph_type: edge.graph_type,
                scope_id: edge.scope_id, // Preserve scope during migration
            };

            self.add_edge(new_edge)?;
            migrated_count += 1;

            // Remove old edge
            self.remove_edge(&edge.source, &edge.target)?;
        }

        // 2. Get all incoming edges to old_did (old_did as target)
        // We need to scan all edges and find ones where target == old_did
        let prefix = self.all_edges_prefix();
        let all_edges_results = self.store.scan(prefix.as_bytes())?;

        for (_key, value) in all_edges_results {
            let edge: TrustEdge = serde_json::from_slice(&value)?;

            // Check if this edge points to the old_did
            if edge.target.as_str() == old_did.as_str() {
                // Create new edge with new_did as target
                let new_edge = TrustEdge {
                    source: edge.source.clone(),
                    target: new_did.clone(),
                    labels: edge.labels,
                    score: edge.score,
                    evidence: edge.evidence,
                    legacy_evidence: edge.legacy_evidence,
                    expires_at: edge.expires_at,
                    created_at: edge.created_at,
                    graph_type: edge.graph_type,
                    scope_id: edge.scope_id, // Preserve scope during migration
                };

                self.add_edge(new_edge)?;
                migrated_count += 1;

                // Remove old edge
                self.remove_edge(&edge.source, &edge.target)?;
            }
        }

        // 3. Clear cache since trust scores may have changed
        self.clear_cache();

        info!(
            "Migrated {} trust edges from {} to {}",
            migrated_count, old_did, new_did
        );

        Ok(migrated_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    #[test]
    fn test_trust_class_from_score() {
        assert_eq!(TrustClass::from_score(0.0), TrustClass::Isolated);
        assert_eq!(TrustClass::from_score(0.05), TrustClass::Isolated);
        assert_eq!(TrustClass::from_score(0.2), TrustClass::Known);
        assert_eq!(TrustClass::from_score(0.5), TrustClass::Partner);
        assert_eq!(TrustClass::from_score(0.9), TrustClass::Federated);
    }

    #[test]
    fn test_trust_edge_creation() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let evidence = TrustEvidence::ContractExecution {
            contract_id: [0u8; 32],
            agreement_id: Some("contract_abc123".to_string()),
            executed_at: 12345,
        };

        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.5))
            .with_label("partner")
            .with_evidence(evidence.clone());

        assert_eq!(edge.source, alice);
        assert_eq!(edge.target, bob);
        assert_eq!(edge.score.value(), 0.5);
        assert_eq!(edge.labels, vec!["partner"]);
        assert_eq!(edge.evidence.len(), 1);
        assert_eq!(edge.evidence[0], evidence);
    }

    #[test]
    fn test_trust_graph_direct() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Alice trusts Bob directly
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.6));
        graph.add_edge(edge).unwrap();

        // Compute trust
        let score = graph.compute_trust_score(&bob).unwrap();
        assert!((0.42..=0.43).contains(&score)); // 0.6 * 0.7 (direct only)

        let class = graph.trust_class(&bob).unwrap();
        assert_eq!(class, TrustClass::Partner);
    }

    #[test]
    fn test_trust_graph_transitive() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Alice trusts Bob
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();

        // Bob trusts Carol
        graph
            .add_edge(TrustEdge::new(
                bob.clone(),
                carol.clone(),
                TrustScore::unchecked(0.6),
            ))
            .unwrap();

        // Compute Alice's trust in Carol (transitive through Bob)
        let score = graph.compute_trust_score(&carol).unwrap();

        // Expected: 0 * 0.7 (no direct) + (0.8 * 0.6) * 0.3 (transitive)
        // = 0 + 0.144 = 0.144
        assert!((0.14..=0.15).contains(&score));

        let class = graph.trust_class(&carol).unwrap();
        assert_eq!(class, TrustClass::Known);
    }

    #[test]
    fn test_trust_edge_expiry() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let edge = TrustEdge::new(alice, bob, TrustScore::unchecked(0.5)).with_expiry(now - 100);

        assert!(edge.is_expired(now));
    }

    #[test]
    fn test_get_all_known_dids() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Alice trusts Bob
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();

        // Bob trusts Carol
        graph
            .add_edge(TrustEdge::new(
                bob.clone(),
                carol.clone(),
                TrustScore::unchecked(0.6),
            ))
            .unwrap();

        let all_dids = graph.get_all_known_dids().unwrap();

        // Should have Alice, Bob, and Carol
        assert_eq!(all_dids.len(), 3);
        assert!(all_dids.contains(&alice));
        assert!(all_dids.contains(&bob));
        assert!(all_dids.contains(&carol));
    }

    #[test]
    fn test_get_dids_above_threshold() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();
        let dave = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Alice trusts Bob highly (0.8 direct = 0.56 score)
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();

        // Alice trusts Carol moderately (0.5 direct = 0.35 score)
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                carol.clone(),
                TrustScore::unchecked(0.5),
            ))
            .unwrap();

        // Alice has low trust in Dave (0.2 direct = 0.14 score)
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                dave.clone(),
                TrustScore::unchecked(0.2),
            ))
            .unwrap();

        // Threshold 0.3: should include Bob (0.56) and Carol (0.35), exclude Dave (0.14)
        let trusted = graph.get_dids_above_threshold(0.3).unwrap();
        assert_eq!(trusted.len(), 2);
        assert!(trusted.contains(&bob));
        assert!(trusted.contains(&carol));
        assert!(!trusted.contains(&dave));

        // Threshold 0.5: should only include Bob (0.56)
        let highly_trusted = graph.get_dids_above_threshold(0.5).unwrap();
        assert_eq!(highly_trusted.len(), 1);
        assert!(highly_trusted.contains(&bob));
    }

    #[test]
    fn test_get_incoming_edges() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();
        let dave = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Alice trusts Bob
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();

        // Carol trusts Bob
        graph
            .add_edge(TrustEdge::new(
                carol.clone(),
                bob.clone(),
                TrustScore::unchecked(0.7),
            ))
            .unwrap();

        // Dave trusts Bob
        graph
            .add_edge(TrustEdge::new(
                dave.clone(),
                bob.clone(),
                TrustScore::unchecked(0.6),
            ))
            .unwrap();

        // Alice also trusts Carol
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                carol.clone(),
                TrustScore::unchecked(0.5),
            ))
            .unwrap();

        // Get incoming edges to Bob (who trusts Bob?)
        let incoming_to_bob = graph.get_incoming_edges(&bob).unwrap();
        assert_eq!(incoming_to_bob.len(), 3);

        // Verify sources
        let sources: Vec<_> = incoming_to_bob.iter().map(|e| e.source.clone()).collect();
        assert!(sources.contains(&alice));
        assert!(sources.contains(&carol));
        assert!(sources.contains(&dave));

        // Get incoming edges to Carol (only Alice trusts Carol)
        let incoming_to_carol = graph.get_incoming_edges(&carol).unwrap();
        assert_eq!(incoming_to_carol.len(), 1);
        assert_eq!(incoming_to_carol[0].source, alice);

        // Get incoming edges to Alice (no one trusts Alice in this test)
        let incoming_to_alice = graph.get_incoming_edges(&alice).unwrap();
        assert_eq!(incoming_to_alice.len(), 0);
    }

    #[test]
    fn test_get_incoming_edges_filters_expired() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Alice trusts Bob (not expired)
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();

        // Carol trusts Bob (expired - set expiry to 100 seconds ago)
        graph
            .add_edge(
                TrustEdge::new(carol.clone(), bob.clone(), TrustScore::unchecked(0.7))
                    .with_expiry(now - 100),
            )
            .unwrap();

        // Get incoming edges to Bob - should only include non-expired edge from Alice
        let incoming = graph.get_incoming_edges(&bob).unwrap();
        assert_eq!(incoming.len(), 1, "Expired edges should be filtered out");
        assert_eq!(incoming[0].source, alice);
    }

    // ================================================================
    // Cache invalidation tests (#878)
    // ================================================================

    #[test]
    fn cache_hit_after_score_computation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.6));
        graph.add_edge(edge).unwrap();

        // First call computes and caches
        let score1 = graph.compute_trust_score(&bob).unwrap();
        assert!((score1 - 0.42).abs() < 0.01); // 0.6 * 0.7 direct weight

        // Second call should return cached value
        let score2 = graph.compute_trust_score(&bob).unwrap();
        assert_eq!(score1, score2);
    }

    #[test]
    fn cache_invalidated_on_edge_mutation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Add edge and compute score (populates cache)
        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.6));
        graph.add_edge(edge).unwrap();
        let score_before = graph.compute_trust_score(&bob).unwrap();

        // Mutate the edge (update score)
        let updated = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.9));
        graph.add_edge(updated).unwrap();

        // Recompute — should NOT return stale cached value
        let score_after = graph.compute_trust_score(&bob).unwrap();
        assert!(
            score_after > score_before,
            "Score should increase after edge update: {score_before} -> {score_after}"
        );
    }

    #[test]
    fn cache_invalidated_on_edge_removal() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        let edge = TrustEdge::new(alice.clone(), bob.clone(), TrustScore::unchecked(0.8));
        graph.add_edge(edge).unwrap();

        // Compute score (populates cache)
        let score = graph.compute_trust_score(&bob).unwrap();
        assert!(score > 0.0);

        // Remove the edge
        graph.remove_edge(&alice, &bob).unwrap();

        // Recompute — should be 0 now
        let score_after = graph.compute_trust_score(&bob).unwrap();
        assert_eq!(score_after, 0.0, "Score should be 0 after edge removal");
    }

    #[test]
    fn transitive_cache_invalidation() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let carol = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Alice trusts Bob
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.8),
            ))
            .unwrap();

        // Bob trusts Carol (transitive: Alice → Bob → Carol)
        graph
            .add_edge(TrustEdge::new(
                bob.clone(),
                carol.clone(),
                TrustScore::unchecked(0.7),
            ))
            .unwrap();

        // Compute Carol's transitive score (populates cache)
        let carol_score_before = graph.compute_trust_score(&carol).unwrap();
        assert!(
            carol_score_before > 0.0,
            "Carol should have transitive trust"
        );

        // Mutate Alice→Bob edge (lower trust in Bob)
        // This should invalidate Carol's cache because the transitive path changed
        graph
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(0.2),
            ))
            .unwrap();

        // Recompute Carol's score — should reflect lowered trust through Bob
        let carol_score_after = graph.compute_trust_score(&carol).unwrap();
        assert!(
            carol_score_after < carol_score_before,
            "Carol's score should decrease when Alice's trust in Bob decreases: {} -> {}",
            carol_score_before,
            carol_score_after,
        );
    }
}
