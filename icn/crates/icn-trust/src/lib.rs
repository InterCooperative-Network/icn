//! ICN Trust - Trust graph management and policy enforcement

pub mod attestation;
pub mod trust_cache;

pub use attestation::TrustAttestation;
pub use trust_cache::TrustCache;

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

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
    pub source: Did,
    pub target: Did,
    pub labels: Vec<String>,
    pub score: f64,
    pub evidence: Vec<String>, // Evidence references (content hashes)
    pub expires_at: Option<u64>,
    pub created_at: u64,
}

impl TrustEdge {
    /// Create a new trust edge
    pub fn new(source: Did, target: Did, score: f64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            source,
            target,
            labels: Vec::new(),
            score,
            evidence: Vec::new(),
            expires_at: None,
            created_at: now,
        }
    }

    /// Check if this edge is expired
    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|exp| now > exp)
    }

    /// Add a label to this edge
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }

    /// Add evidence to this edge
    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence.push(evidence.into());
        self
    }

    /// Set expiry time
    pub fn with_expiry(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
}

/// Trust graph manager
pub struct TrustGraph {
    store: Arc<dyn Store>,
    own_did: Did,
    /// LRU cache for trust scores with TTL-based invalidation (Phase 19)
    cache: TrustCache,
}

impl TrustGraph {
    /// Create a new trust graph
    pub fn new(store: Arc<dyn Store>, own_did: Did) -> Self {
        Self {
            store,
            own_did,
            cache: TrustCache::new(),
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
        }
    }

    /// Get the DID of this node
    pub fn own_did(&self) -> &Did {
        &self.own_did
    }

    /// Add or update a trust edge
    pub fn add_edge(&mut self, edge: TrustEdge) -> Result<()> {
        info!(
            "Adding trust edge: {} -> {} (score: {})",
            edge.source, edge.target, edge.score
        );

        let key = format!(
            "trust/edges/{}:{}",
            edge.source.as_str(),
            edge.target.as_str()
        );
        let value = serde_json::to_vec(&edge)?;

        self.store.put(key.as_bytes(), &value)?;

        // Invalidate cache for target (LRU cache with TTL)
        self.cache.invalidate(&edge.target);

        Ok(())
    }

    /// Get a trust edge
    pub fn get_edge(&self, source: &Did, target: &Did) -> Result<Option<TrustEdge>> {
        let key = format!("trust/edges/{}:{}", source.as_str(), target.as_str());

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
        let prefix = format!("trust/edges/{}:", source.as_str());
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

    /// Compute trust score for a DID (from own perspective)
    ///
    /// Uses a simplified PageRank-like algorithm:
    /// TrustScore(own -> target) =
    ///     DirectTrust(own -> target) * 0.7 +
    ///     TransitiveTrust(own -> intermediate -> target) * 0.3
    pub fn compute_trust_score(&self, target: &Did) -> Result<f64> {
        // Record lookup
        icn_obs::metrics::trust::lookups_inc();

        // Check LRU cache first (with TTL validation)
        if let Some(score) = self.cache.get(target) {
            icn_obs::metrics::trust::cache_hits_inc();
            return Ok(score);
        }

        icn_obs::metrics::trust::cache_misses_inc();
        debug!("Computing trust score for {}", target);

        // Get direct trust edge
        let direct_score = self
            .get_edge(&self.own_did, target)?
            .map(|e| e.score)
            .unwrap_or(0.0);

        // Get transitive trust (via intermediates we trust)
        let own_edges = self.get_outgoing_edges(&self.own_did)?;

        let mut transitive_sum = 0.0;
        let mut transitive_count = 0;

        for intermediate_edge in own_edges {
            // Skip if intermediate is the target
            if intermediate_edge.target == *target {
                continue;
            }

            // Get edge from intermediate to target
            if let Some(indirect_edge) = self.get_edge(&intermediate_edge.target, target)? {
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

        // Combine: 70% direct, 30% transitive
        let final_score = (direct_score * 0.7 + transitive_score * 0.3).min(1.0);

        debug!(
            "Trust score for {}: direct={}, transitive={}, final={}",
            target, direct_score, transitive_score, final_score
        );

        // Cache result (LRU cache with automatic eviction)
        self.cache.put(target.clone(), final_score);

        // Record score distribution
        icn_obs::metrics::trust::score_distribution_record(final_score);

        Ok(final_score)
    }

    /// Get the trust class for a DID
    pub fn trust_class(&self, did: &Did) -> Result<TrustClass> {
        let score = self.compute_trust_score(did)?;
        Ok(TrustClass::from_score(score))
    }

    /// Remove a trust edge
    pub fn remove_edge(&mut self, source: &Did, target: &Did) -> Result<()> {
        let key = format!("trust/edges/{}:{}", source.as_str(), target.as_str());
        self.store.delete(key.as_bytes())?;

        // Invalidate cache for target
        self.cache.invalidate(target);

        Ok(())
    }

    /// Clear the trust score cache
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
                expires_at: edge.expires_at,
                created_at: edge.created_at,
            };

            self.add_edge(new_edge)?;
            migrated_count += 1;

            // Remove old edge
            self.remove_edge(&edge.source, &edge.target)?;
        }

        // 2. Get all incoming edges to old_did (old_did as target)
        // We need to scan all edges and find ones where target == old_did
        let all_edges_results = self.store.scan(b"trust/edges/")?;

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
                    expires_at: edge.expires_at,
                    created_at: edge.created_at,
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

        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.5)
            .with_label("partner")
            .with_evidence("contract_abc123");

        assert_eq!(edge.source, alice);
        assert_eq!(edge.target, bob);
        assert_eq!(edge.score, 0.5);
        assert_eq!(edge.labels, vec!["partner"]);
        assert_eq!(edge.evidence, vec!["contract_abc123"]);
    }

    #[test]
    fn test_trust_graph_direct() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut graph = TrustGraph::new(store, alice.clone());

        // Alice trusts Bob directly
        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.6);
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
            .add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.8))
            .unwrap();

        // Bob trusts Carol
        graph
            .add_edge(TrustEdge::new(bob.clone(), carol.clone(), 0.6))
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

        let edge = TrustEdge::new(alice, bob, 0.5).with_expiry(now - 100);

        assert!(edge.is_expired(now));
    }
}
