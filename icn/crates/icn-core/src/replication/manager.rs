//! ReplicationManager - Monitors and maintains data durability through replica coordination
//!
//! Phase 17 Week 3: ReplicationManager Actor
//!
//! Responsibilities:
//! - Monitor replica counts for all content hashes
//! - Detect under-replicated data (count < target)
//! - Request new replicas from trusted peers
//! - Track replica health status
//! - Enforce replication policies

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_store::{ContentHash, ReplicaHealth, Store};
use icn_trust::{TrustClass, TrustGraph};

/// Replication configuration
#[derive(Clone, Debug)]
pub struct ReplicationConfig {
    /// Target number of replicas per content hash
    pub target_replicas: usize,

    /// Minimum trust class required to serve as replica
    /// Default: Partner (0.4+)
    pub min_trust_class: TrustClass,

    /// Health check interval in seconds
    /// Default: 60 seconds
    pub health_check_interval_secs: u64,

    /// Stale threshold - replicas not seen in this duration are marked Stale
    /// Default: 5 minutes
    pub stale_threshold_secs: u64,

    /// Unreachable threshold - replicas not seen in this duration are marked Unreachable
    /// Default: 15 minutes
    pub unreachable_threshold_secs: u64,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            target_replicas: 3,
            min_trust_class: TrustClass::Partner,
            health_check_interval_secs: 60,
            stale_threshold_secs: 300,       // 5 minutes
            unreachable_threshold_secs: 900, // 15 minutes
        }
    }
}

/// Replication Manager Actor
pub struct ReplicationManager {
    /// Own DID
    own_did: Did,

    /// Replication configuration
    config: ReplicationConfig,

    /// Store for replica metadata
    store: Arc<dyn Store>,

    /// Trust graph for peer selection
    trust_graph: Arc<RwLock<TrustGraph>>,

    /// Gossip actor for sending replica requests
    gossip: Arc<RwLock<GossipActor>>,

    /// Cache of recent requests to avoid spam (content_hash -> timestamp)
    recent_requests: HashMap<ContentHash, SystemTime>,
}

impl ReplicationManager {
    /// Create a new ReplicationManager
    pub fn new(
        own_did: Did,
        config: ReplicationConfig,
        store: Arc<dyn Store>,
        trust_graph: Arc<RwLock<TrustGraph>>,
        gossip: Arc<RwLock<GossipActor>>,
    ) -> Self {
        Self {
            own_did,
            config,
            store,
            trust_graph,
            gossip,
            recent_requests: HashMap::new(),
        }
    }

    /// Spawn the replication manager with background health monitoring
    pub fn spawn(
        own_did: Did,
        config: ReplicationConfig,
        store: Arc<dyn Store>,
        trust_graph: Arc<RwLock<TrustGraph>>,
        gossip: Arc<RwLock<GossipActor>>,
    ) -> ReplicationHandle {
        let manager = Self::new(own_did, config.clone(), store, trust_graph, gossip);
        let handle = Arc::new(RwLock::new(manager));

        // Spawn background health monitoring loop
        let handle_clone = handle.clone();
        tokio::spawn(async move {
            Self::monitor_loop(handle_clone, config.health_check_interval_secs).await;
        });

        ReplicationHandle { inner: handle }
    }

    /// Background task: check replication health every N seconds
    async fn monitor_loop(manager: Arc<RwLock<ReplicationManager>>, interval_secs: u64) {
        let mut ticker = interval(Duration::from_secs(interval_secs));

        loop {
            ticker.tick().await;

            let mut mgr = manager.write().await;
            if let Err(e) = mgr.check_all_content().await {
                warn!("Replication health check failed: {}", e);
            }
        }
    }

    /// Check replication health for all content hashes
    async fn check_all_content(&mut self) -> Result<()> {
        use std::time::Instant;

        debug!("Running replication health check");
        let start = Instant::now();

        // Get all content hashes with replica metadata
        let content_hashes = self.store.list_replica_hashes()?;

        let mut under_replicated = 0;
        let mut healthy = 0;
        let mut total_replicas = 0;
        let mut healthy_replicas = 0;
        let mut stale_replicas = 0;
        let mut unreachable_replicas = 0;
        let mut unhealthy_replicas = 0;

        for hash in &content_hashes {
            match self.check_content_replication(hash).await {
                Ok(true) => healthy += 1,
                Ok(false) => under_replicated += 1,
                Err(e) => {
                    warn!(
                        "Failed to check replication for {:?}: {}",
                        hex::encode(hash),
                        e
                    );
                }
            }

            // Count replicas by health status
            if let Ok(Some(metadata)) = self.store.get_replica_metadata(hash) {
                total_replicas += metadata.replicas.len();
                for replica in &metadata.replicas {
                    match replica.health {
                        ReplicaHealth::Healthy => healthy_replicas += 1,
                        ReplicaHealth::Stale => stale_replicas += 1,
                        ReplicaHealth::Unreachable => unreachable_replicas += 1,
                        ReplicaHealth::Unhealthy(_) => unhealthy_replicas += 1,
                    }
                }
            }
        }

        // Emit metrics
        icn_obs::metrics::replication::content_total_set(content_hashes.len());
        icn_obs::metrics::replication::content_healthy_set(healthy);
        icn_obs::metrics::replication::content_under_replicated_set(under_replicated);
        icn_obs::metrics::replication::replicas_total_set(total_replicas);
        icn_obs::metrics::replication::replicas_healthy_set(healthy_replicas);
        icn_obs::metrics::replication::replicas_stale_set(stale_replicas);
        icn_obs::metrics::replication::replicas_unreachable_set(unreachable_replicas);
        icn_obs::metrics::replication::replicas_unhealthy_set(unhealthy_replicas);
        icn_obs::metrics::replication::health_checks_inc();
        icn_obs::metrics::replication::health_check_duration_record(start.elapsed().as_secs_f64());

        info!(
            "Replication health check complete: {} healthy, {} under-replicated (duration: {:?})",
            healthy,
            under_replicated,
            start.elapsed()
        );

        Ok(())
    }

    /// Check replication for a single content hash
    /// Returns true if properly replicated, false if under-replicated
    async fn check_content_replication(&mut self, hash: &ContentHash) -> Result<bool> {
        // Load replica metadata
        let mut metadata = match self.store.get_replica_metadata(hash)? {
            Some(m) => m,
            None => {
                // No metadata - this shouldn't happen, but handle gracefully
                debug!("No replica metadata for {:?}", hex::encode(hash));
                return Ok(true); // Skip this entry
            }
        };

        // Update replica health based on timestamps
        self.update_replica_health(&mut metadata);

        // Save updated metadata
        self.store.put_replica_metadata(&metadata)?;

        // Count healthy replicas
        let healthy_count = metadata.healthy_count();

        // Check if under-replicated
        if healthy_count < self.config.target_replicas {
            debug!(
                "Content {:?} under-replicated: {} / {} replicas",
                hex::encode(hash),
                healthy_count,
                self.config.target_replicas
            );

            // Request additional replicas
            self.request_additional_replicas(hash, &metadata).await?;
            return Ok(false);
        }

        Ok(true)
    }

    /// Update replica health status based on last_seen timestamps
    fn update_replica_health(&self, metadata: &mut icn_store::ReplicaMetadata) {
        let now = SystemTime::now();

        for replica in &mut metadata.replicas {
            if let Ok(elapsed) = now.duration_since(replica.last_seen) {
                let elapsed_secs = elapsed.as_secs();

                // Update health based on elapsed time
                let new_health = if elapsed_secs > self.config.unreachable_threshold_secs {
                    ReplicaHealth::Unreachable
                } else if elapsed_secs > self.config.stale_threshold_secs {
                    ReplicaHealth::Stale
                } else {
                    ReplicaHealth::Healthy
                };

                // Only update if health changed
                if replica.health != new_health {
                    debug!(
                        "Replica {} health changed: {:?} -> {:?} (last seen {}s ago)",
                        replica.peer_did, replica.health, new_health, elapsed_secs
                    );
                    replica.health = new_health;
                }
            }
        }

        metadata.updated_at = now;
    }

    /// Request additional replicas for under-replicated content
    async fn request_additional_replicas(
        &mut self,
        hash: &ContentHash,
        metadata: &icn_store::ReplicaMetadata,
    ) -> Result<()> {
        // Check if we've recently requested replicas for this hash
        if let Some(last_request) = self.recent_requests.get(hash) {
            if let Ok(elapsed) = SystemTime::now().duration_since(*last_request) {
                // Avoid spamming requests - wait at least 5 minutes between requests
                if elapsed.as_secs() < 300 {
                    return Ok(());
                }
            }
        }

        // Get trusted peers who could serve as replicas
        let candidate_peers = self.select_replica_candidates(metadata).await?;
        icn_obs::metrics::replication::candidates_evaluated_inc(candidate_peers.len() as u64);

        if candidate_peers.is_empty() {
            warn!(
                "No suitable replica candidates found for {:?}",
                hex::encode(hash)
            );
            return Ok(());
        }

        // Send ReplicaRequest to candidate peers (limit to 5)
        let peers_to_request: Vec<Did> = candidate_peers.into_iter().take(5).collect();

        if !peers_to_request.is_empty() {
            let gossip = self.gossip.read().await;
            gossip.request_replicas(hash, &peers_to_request);

            icn_obs::metrics::replication::requests_sent_inc(peers_to_request.len() as u64);

            for peer_did in &peers_to_request {
                debug!(
                    "Sent ReplicaRequest for {:?} to {}",
                    hex::encode(hash),
                    peer_did
                );
            }
        }

        // Record this request
        self.recent_requests.insert(*hash, SystemTime::now());

        Ok(())
    }

    /// Select candidate peers for replication using trust-weighted selection
    ///
    /// Strategy:
    /// 1. Filter peers by minimum trust class
    /// 2. Exclude peers already serving as replicas
    /// 3. Sort by trust score (higher is better)
    /// 4. Return top N candidates
    async fn select_replica_candidates(
        &self,
        metadata: &icn_store::ReplicaMetadata,
    ) -> Result<Vec<Did>> {
        let trust_graph = self.trust_graph.read().await;
        let gossip = self.gossip.read().await;

        // Get all peers we know about through gossip (subscribers, vector clock)
        let all_peers = gossip.get_known_peers();

        // Filter to peers meeting minimum trust requirements
        let mut candidates: Vec<(Did, f64)> = Vec::new();

        // Get DIDs of existing replicas
        let existing_replicas: HashSet<String> = metadata
            .replicas
            .iter()
            .map(|r| r.peer_did.clone())
            .collect();

        for peer_did in all_peers {
            // Skip if already a replica
            if existing_replicas.contains(&peer_did.to_string()) {
                continue;
            }

            // Skip self
            if peer_did == self.own_did {
                continue;
            }

            // Check trust class - trust_class returns Result<TrustClass>, not Option
            if let Ok(trust_class) = trust_graph.trust_class(&peer_did) {
                if trust_class >= self.config.min_trust_class {
                    // Get trust score for sorting (compute_trust_score, not trust_score)
                    if let Ok(score) = trust_graph.compute_trust_score(&peer_did) {
                        candidates.push((peer_did, score));
                    }
                }
            }
        }

        // Sort by trust score (descending - higher trust first)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return just the DIDs
        Ok(candidates.into_iter().map(|(did, _)| did).collect())
    }

    /// Manually trigger a health check (for testing)
    pub async fn trigger_health_check(&mut self) -> Result<()> {
        self.check_all_content().await
    }
}

/// Handle for interacting with the replication manager
#[derive(Clone)]
pub struct ReplicationHandle {
    inner: Arc<RwLock<ReplicationManager>>,
}

impl ReplicationHandle {
    /// Trigger an immediate health check
    pub async fn trigger_health_check(&self) -> Result<()> {
        self.inner.write().await.trigger_health_check().await
    }

    /// Get current replication config
    pub async fn get_config(&self) -> ReplicationConfig {
        self.inner.read().await.config.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    #[tokio::test]
    async fn test_replication_config_default() {
        let config = ReplicationConfig::default();
        assert_eq!(config.target_replicas, 3);
        assert_eq!(config.min_trust_class, TrustClass::Partner);
        assert_eq!(config.health_check_interval_secs, 60);
    }

    #[tokio::test]
    async fn test_replica_health_update() -> Result<()> {
        // Setup
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();
        let config = ReplicationConfig::default();

        let store = Arc::new(SledStore::temporary()?) as Arc<dyn Store>;
        let trust_store = Arc::new(SledStore::temporary()?) as Arc<dyn Store>;
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(trust_store, did.clone())));

        let trust_lookup = Arc::new(|_: &Did| None);
        let gossip = GossipActor::spawn(did.clone(), trust_lookup);

        let manager =
            ReplicationManager::new(did.clone(), config, store.clone(), trust_graph, gossip);

        // Create test metadata with old timestamp
        let hash = [0u8; 32];
        let old_time = SystemTime::now() - Duration::from_secs(600); // 10 minutes ago

        let mut metadata = icn_store::ReplicaMetadata::new(hash);
        metadata.replicas.push(icn_store::ReplicaInfo {
            peer_did: "did:icn:test".to_string(),
            last_seen: old_time,
            health: ReplicaHealth::Healthy,
        });

        // Update health
        manager.update_replica_health(&mut metadata);

        // Should be marked as Stale (>5 minutes old)
        assert_eq!(metadata.replicas[0].health, ReplicaHealth::Stale);

        Ok(())
    }

    #[tokio::test]
    async fn test_select_replica_candidates_filters_existing() -> Result<()> {
        // Setup
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();
        let config = ReplicationConfig::default();

        let store = Arc::new(SledStore::temporary()?) as Arc<dyn Store>;
        let trust_store = Arc::new(SledStore::temporary()?) as Arc<dyn Store>;
        let mut trust_graph = TrustGraph::new(trust_store.clone(), did.clone());

        // Add a peer with Partner trust (0.4-0.7 range)
        let peer_keypair = KeyPair::generate()?;
        let peer_did = peer_keypair.did().clone();
        let edge = icn_trust::TrustEdge::new(did.clone(), peer_did.clone(), icn_trust::TrustScore::unchecked(0.5));
        trust_graph.add_edge(edge)?;

        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        let trust_lookup = Arc::new(|_: &Did| None);
        let gossip = GossipActor::spawn(did.clone(), trust_lookup);

        let manager =
            ReplicationManager::new(did.clone(), config, store, trust_graph_handle, gossip);

        // Create metadata with peer already as replica
        let hash = [0u8; 32];
        let mut metadata = icn_store::ReplicaMetadata::new(hash);
        metadata.replicas.push(icn_store::ReplicaInfo {
            peer_did: peer_did.to_string(),
            last_seen: SystemTime::now(),
            health: ReplicaHealth::Healthy,
        });

        // Select candidates - should exclude the existing replica
        let candidates = manager.select_replica_candidates(&metadata).await?;
        assert_eq!(candidates.len(), 0); // No candidates because peer is already a replica

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_creation() -> Result<()> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();
        let config = ReplicationConfig::default();

        let store = Arc::new(SledStore::temporary()?) as Arc<dyn Store>;
        let trust_store = Arc::new(SledStore::temporary()?) as Arc<dyn Store>;
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(trust_store, did.clone())));

        let trust_lookup = Arc::new(|_: &Did| None);
        let gossip = GossipActor::spawn(did.clone(), trust_lookup);

        let manager = ReplicationManager::new(did, config.clone(), store, trust_graph, gossip);

        assert_eq!(manager.config.target_replicas, 3);
        assert_eq!(manager.config.min_trust_class, TrustClass::Partner);

        Ok(())
    }
}
