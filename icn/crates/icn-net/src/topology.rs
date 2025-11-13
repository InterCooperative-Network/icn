//! Topology-aware neighbor management
//!
//! This module provides categorized neighbor sets for scale-aware routing.
//! Neighbors are organized into sets based on geographic proximity and trust:
//!
//! - **LocalCluster**: Same region + same cluster_id (high locality)
//! - **Regional**: Same region, different cluster (medium locality)
//! - **Backbone**: Different region, high trust (federation links)
//! - **Trusted**: Cross-region high-trust peers (special relationships)
//!
//! Each set has configurable size limits with LRU eviction based on trust scores.

use icn_gossip::Scope;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::time::Instant;

/// Categorized neighbor sets for topology-aware routing
#[derive(Debug)]
pub struct NeighborSets {
    /// Local cluster neighbors (same region + cluster_id)
    pub local_cluster: BTreeSet<PeerId>,

    /// Regional neighbors (same region, different cluster)
    pub regional: BTreeSet<PeerId>,

    /// Backbone neighbors (different region, high trust)
    pub backbone: BTreeSet<PeerId>,

    /// Trusted neighbors (cross-region high-trust)
    pub trusted: BTreeSet<PeerId>,

    /// Metadata per peer
    metadata: HashMap<PeerId, PeerMetadata>,

    /// Our own topology info for comparison
    own_topology: TopologyInfo,
}

/// Peer identifier wrapping a DID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerId(pub Did);

impl PartialOrd for PeerId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PeerId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Order by DID string representation
        self.0.to_string().cmp(&other.0.to_string())
    }
}

/// Metadata tracked for each neighbor
#[derive(Debug, Clone)]
struct PeerMetadata {
    topology_info: TopologyInfo,
    rtt_ms: Option<u64>,
    trust_score: f32,
    connected_at: Instant,
}

/// Geographic and organizational topology information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyInfo {
    pub region: String,
    pub cluster_id: String,
    pub role: NodeRole,
}

/// Node role in the network topology
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    /// Edge node (standard peer)
    Edge,
    /// Rendezvous server (bootstrap/discovery)
    Rendezvous,
    /// Archive node (long-term storage)
    Archive,
}

impl Default for NodeRole {
    fn default() -> Self {
        NodeRole::Edge
    }
}

/// Neighbor set size limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborLimitsConfig {
    pub max_local_cluster: usize,
    pub max_regional: usize,
    pub max_backbone: usize,
    pub max_trusted: usize,
}

impl Default for NeighborLimitsConfig {
    fn default() -> Self {
        Self {
            max_local_cluster: 50,
            max_regional: 30,
            max_backbone: 20,
            max_trusted: 10,
        }
    }
}

/// Internal enum for neighbor set classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NeighborSet {
    LocalCluster,
    Regional,
    Backbone,
    Trusted,
}

/// Metrics for observability
#[derive(Debug, Clone)]
pub struct NeighborMetrics {
    pub local_cluster_count: usize,
    pub regional_count: usize,
    pub backbone_count: usize,
    pub trusted_count: usize,
}

impl NeighborSets {
    /// Create new empty neighbor sets with our topology info
    pub fn new(own_topology: TopologyInfo) -> Self {
        Self {
            local_cluster: BTreeSet::new(),
            regional: BTreeSet::new(),
            backbone: BTreeSet::new(),
            trusted: BTreeSet::new(),
            metadata: HashMap::new(),
            own_topology,
        }
    }

    /// Add or update a neighbor in the appropriate set
    ///
    /// This method:
    /// 1. Removes peer from old sets if already present
    /// 2. Determines placement based on topology + trust
    /// 3. Enforces limits with LRU eviction (score-based)
    /// 4. Inserts into appropriate set
    /// 5. Stores metadata
    pub fn add_neighbor(
        &mut self,
        peer: PeerId,
        topology_info: TopologyInfo,
        rtt_ms: Option<u64>,
        trust_score: f32,
        limits: &NeighborLimitsConfig,
    ) {
        // 1. Remove from old sets if already present
        self.remove_neighbor(&peer);

        // 2. Create metadata
        let metadata = PeerMetadata {
            topology_info: topology_info.clone(),
            rtt_ms,
            trust_score,
            connected_at: Instant::now(),
        };

        // 3. Determine placement based on topology_info + trust
        let target_set = self.determine_set(&metadata);

        // 4. Enforce limit with LRU eviction (score-based)
        self.enforce_limit(target_set, limits);

        // 5. Insert into appropriate set
        match target_set {
            NeighborSet::LocalCluster => {
                self.local_cluster.insert(peer.clone());
            }
            NeighborSet::Regional => {
                self.regional.insert(peer.clone());
            }
            NeighborSet::Backbone => {
                self.backbone.insert(peer.clone());
            }
            NeighborSet::Trusted => {
                self.trusted.insert(peer.clone());
            }
        }

        // 6. Store metadata
        self.metadata.insert(peer, metadata);
    }

    /// Remove neighbor from all sets
    pub fn remove_neighbor(&mut self, peer: &PeerId) {
        self.local_cluster.remove(peer);
        self.regional.remove(peer);
        self.backbone.remove(peer);
        self.trusted.remove(peer);
        self.metadata.remove(peer);
    }

    /// Sample peers from a specific scope for gossip fanout
    ///
    /// Returns random selection without replacement up to `count` peers.
    pub fn sample(&self, scope: Scope, count: usize) -> Vec<PeerId> {
        use rand::seq::SliceRandom;

        let peers: Vec<PeerId> = match scope {
            Scope::LocalCluster => self.local_cluster.iter().cloned().collect(),
            Scope::Regional => {
                // Regional includes local_cluster + regional
                let mut peers = self.local_cluster.iter().cloned().collect::<Vec<_>>();
                peers.extend(self.regional.iter().cloned());
                peers
            }
            Scope::Global => {
                // Global includes all sets
                let mut peers = self.local_cluster.iter().cloned().collect::<Vec<_>>();
                peers.extend(self.regional.iter().cloned());
                peers.extend(self.backbone.iter().cloned());
                peers.extend(self.trusted.iter().cloned());
                peers
            }
        };

        let mut rng = rand::thread_rng();
        peers
            .choose_multiple(&mut rng, count.min(peers.len()))
            .cloned()
            .collect()
    }

    /// Get count of local cluster neighbors
    pub fn local_cluster_count(&self) -> usize {
        self.local_cluster.len()
    }

    /// Get count of regional neighbors
    pub fn regional_count(&self) -> usize {
        self.regional.len()
    }

    /// Get count of backbone neighbors
    pub fn backbone_count(&self) -> usize {
        self.backbone.len()
    }

    /// Get count of trusted neighbors
    pub fn trusted_count(&self) -> usize {
        self.trusted.len()
    }

    /// Get metrics for observability
    pub fn metrics(&self) -> NeighborMetrics {
        NeighborMetrics {
            local_cluster_count: self.local_cluster.len(),
            regional_count: self.regional.len(),
            backbone_count: self.backbone.len(),
            trusted_count: self.trusted.len(),
        }
    }

    /// Get total neighbor count across all sets
    pub fn total_count(&self) -> usize {
        self.local_cluster.len()
            + self.regional.len()
            + self.backbone.len()
            + self.trusted.len()
    }

    /// Determine which set a peer belongs to based on topology and trust
    ///
    /// Priority: Trust > Region > Cluster
    fn determine_set(&self, metadata: &PeerMetadata) -> NeighborSet {
        // High trust (0.7+) goes to trusted set regardless of location
        if metadata.trust_score >= 0.7 {
            return NeighborSet::Trusted;
        }

        // Compare regions
        let same_region = metadata.topology_info.region == self.own_topology.region;
        let same_cluster = metadata.topology_info.cluster_id == self.own_topology.cluster_id;

        if same_region && same_cluster {
            NeighborSet::LocalCluster
        } else if same_region {
            NeighborSet::Regional
        } else {
            // Different region goes to backbone
            NeighborSet::Backbone
        }
    }

    /// Enforce size limits with LRU eviction based on trust score
    fn enforce_limit(&mut self, target: NeighborSet, limits: &NeighborLimitsConfig) {
        // Get the appropriate set and limit
        let (max, set_ref) = match target {
            NeighborSet::LocalCluster => (limits.max_local_cluster, &self.local_cluster),
            NeighborSet::Regional => (limits.max_regional, &self.regional),
            NeighborSet::Backbone => (limits.max_backbone, &self.backbone),
            NeighborSet::Trusted => (limits.max_trusted, &self.trusted),
        };

        if set_ref.len() < max {
            return;
        }

        // Collect all peers with their scores
        let mut peers_with_scores: Vec<(PeerId, f32)> = set_ref
            .iter()
            .map(|peer| {
                let score = self.metadata.get(peer).map(|m| m.trust_score).unwrap_or(0.0);
                (peer.clone(), score)
            })
            .collect();

        // Sort by score (ascending)
        peers_with_scores.sort_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Calculate how many to evict
        let to_evict_count = set_ref.len() - max + 1; // +1 to make room for new peer

        // Evict lowest-scoring peers
        for i in 0..to_evict_count.min(peers_with_scores.len()) {
            let peer = &peers_with_scores[i].0;
            match target {
                NeighborSet::LocalCluster => { self.local_cluster.remove(peer); }
                NeighborSet::Regional => { self.regional.remove(peer); }
                NeighborSet::Backbone => { self.backbone.remove(peer); }
                NeighborSet::Trusted => { self.trusted.remove(peer); }
            }
            self.metadata.remove(peer);
        }
    }

}

/// Topology configuration for regional/cluster-based networking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyConfig {
    /// Geographic region identifier (e.g., "na-east", "eu-west", "ap-south")
    pub region: String,

    /// Cluster identifier within region (e.g., "coop-mesh-1", "timebank-cluster")
    pub cluster_id: String,

    /// Node role in the network
    pub role: NodeRole,

    /// Maximum neighbors per set
    pub neighbor_limits: NeighborLimitsConfig,

    /// Gossip fanout per scope
    pub fanout: FanoutConfig,
}

/// Gossip fanout configuration per scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanoutConfig {
    /// Fanout for local cluster scope
    pub local_cluster: usize,

    /// Fanout for regional scope
    pub regional: usize,

    /// Fanout for global scope
    pub global: usize,
}

impl Default for FanoutConfig {
    fn default() -> Self {
        FanoutConfig {
            local_cluster: 8,
            regional: 6,
            global: 4,
        }
    }
}

impl Default for TopologyConfig {
    fn default() -> Self {
        TopologyConfig {
            region: "default".to_string(),
            cluster_id: "default".to_string(),
            role: NodeRole::default(),
            neighbor_limits: NeighborLimitsConfig::default(),
            fanout: FanoutConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn create_test_topology(region: &str, cluster: &str) -> TopologyInfo {
        TopologyInfo {
            region: region.to_string(),
            cluster_id: cluster.to_string(),
            role: NodeRole::Edge,
        }
    }

    fn create_test_peer_id() -> PeerId {
        let kp = KeyPair::generate().unwrap();
        PeerId(kp.did().clone())
    }

    #[test]
    fn test_neighbor_placement_local_cluster() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        let peer = create_test_peer_id();
        let peer_topo = create_test_topology("na-east", "coop-1");

        sets.add_neighbor(peer.clone(), peer_topo, Some(10), 0.5, &limits);

        assert!(sets.local_cluster.contains(&peer));
        assert_eq!(sets.local_cluster.len(), 1);
        assert_eq!(sets.regional.len(), 0);
        assert_eq!(sets.backbone.len(), 0);
    }

    #[test]
    fn test_neighbor_placement_regional() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        let peer = create_test_peer_id();
        let peer_topo = create_test_topology("na-east", "coop-2"); // Different cluster

        sets.add_neighbor(peer.clone(), peer_topo, Some(15), 0.5, &limits);

        assert!(sets.regional.contains(&peer));
        assert_eq!(sets.regional.len(), 1);
        assert_eq!(sets.local_cluster.len(), 0);
    }

    #[test]
    fn test_neighbor_placement_backbone() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        let peer = create_test_peer_id();
        let peer_topo = create_test_topology("eu-west", "coop-1"); // Different region

        sets.add_neighbor(peer.clone(), peer_topo, Some(50), 0.5, &limits);

        assert!(sets.backbone.contains(&peer));
        assert_eq!(sets.backbone.len(), 1);
    }

    #[test]
    fn test_neighbor_placement_trusted() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        let peer = create_test_peer_id();
        let peer_topo = create_test_topology("eu-west", "coop-1");

        // High trust score (0.8) overrides geographic placement
        sets.add_neighbor(peer.clone(), peer_topo, Some(50), 0.8, &limits);

        assert!(sets.trusted.contains(&peer));
        assert_eq!(sets.trusted.len(), 1);
        assert_eq!(sets.backbone.len(), 0);
    }

    #[test]
    fn test_lru_eviction() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);

        // Set tight limit
        let limits = NeighborLimitsConfig {
            max_local_cluster: 3,
            max_regional: 30,
            max_backbone: 20,
            max_trusted: 10,
        };

        // Add 4 local cluster peers with different trust scores
        let peer1 = create_test_peer_id();
        let peer2 = create_test_peer_id();
        let peer3 = create_test_peer_id();
        let peer4 = create_test_peer_id();

        let local_topo = create_test_topology("na-east", "coop-1");

        sets.add_neighbor(peer1.clone(), local_topo.clone(), Some(10), 0.3, &limits);
        sets.add_neighbor(peer2.clone(), local_topo.clone(), Some(10), 0.5, &limits);
        sets.add_neighbor(peer3.clone(), local_topo.clone(), Some(10), 0.4, &limits);

        // All 3 should be present
        assert_eq!(sets.local_cluster.len(), 3);

        // Add 4th with higher score - should evict lowest (peer1 with 0.3)
        sets.add_neighbor(peer4.clone(), local_topo.clone(), Some(10), 0.6, &limits);

        assert_eq!(sets.local_cluster.len(), 3);
        assert!(!sets.local_cluster.contains(&peer1)); // Evicted
        assert!(sets.local_cluster.contains(&peer2));
        assert!(sets.local_cluster.contains(&peer3));
        assert!(sets.local_cluster.contains(&peer4));
    }

    #[test]
    fn test_remove_neighbor() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        let peer = create_test_peer_id();
        let peer_topo = create_test_topology("na-east", "coop-1");

        sets.add_neighbor(peer.clone(), peer_topo, Some(10), 0.5, &limits);
        assert_eq!(sets.local_cluster.len(), 1);

        sets.remove_neighbor(&peer);
        assert_eq!(sets.local_cluster.len(), 0);
        assert_eq!(sets.metadata.len(), 0);
    }

    #[test]
    fn test_sampling_local_cluster() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        let local_topo = create_test_topology("na-east", "coop-1");

        // Add 10 local cluster peers
        let mut all_peers = Vec::new();
        for _ in 0..10 {
            let peer = create_test_peer_id();
            sets.add_neighbor(peer.clone(), local_topo.clone(), Some(10), 0.5, &limits);
            all_peers.push(peer);
        }

        // Sample 5 from local cluster scope
        let sampled = sets.sample(Scope::LocalCluster, 5);
        assert_eq!(sampled.len(), 5);

        // All sampled peers should be in local_cluster
        for peer in &sampled {
            assert!(sets.local_cluster.contains(peer));
        }
    }

    #[test]
    fn test_sampling_regional() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        // Add 5 local + 5 regional
        let local_topo = create_test_topology("na-east", "coop-1");
        let regional_topo = create_test_topology("na-east", "coop-2");

        for _ in 0..5 {
            let peer = create_test_peer_id();
            sets.add_neighbor(peer, local_topo.clone(), Some(10), 0.5, &limits);
        }

        for _ in 0..5 {
            let peer = create_test_peer_id();
            sets.add_neighbor(peer, regional_topo.clone(), Some(15), 0.5, &limits);
        }

        // Regional scope should include both sets
        let sampled = sets.sample(Scope::Regional, 8);
        assert_eq!(sampled.len(), 8);
    }

    #[test]
    fn test_sampling_global() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        // Add peers to all sets
        let local_topo = create_test_topology("na-east", "coop-1");
        let regional_topo = create_test_topology("na-east", "coop-2");
        let backbone_topo = create_test_topology("eu-west", "coop-1");

        sets.add_neighbor(create_test_peer_id(), local_topo, Some(10), 0.5, &limits);
        sets.add_neighbor(create_test_peer_id(), regional_topo, Some(15), 0.5, &limits);
        sets.add_neighbor(create_test_peer_id(), backbone_topo, Some(50), 0.5, &limits);
        sets.add_neighbor(create_test_peer_id(), create_test_topology("ap-south", "coop-1"), Some(60), 0.8, &limits);

        // Global scope includes all sets
        let sampled = sets.sample(Scope::Global, 4);
        assert_eq!(sampled.len(), 4);
    }

    #[test]
    fn test_metrics() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        sets.add_neighbor(
            create_test_peer_id(),
            create_test_topology("na-east", "coop-1"),
            Some(10),
            0.5,
            &limits,
        );
        sets.add_neighbor(
            create_test_peer_id(),
            create_test_topology("na-east", "coop-2"),
            Some(15),
            0.5,
            &limits,
        );
        sets.add_neighbor(
            create_test_peer_id(),
            create_test_topology("eu-west", "coop-1"),
            Some(50),
            0.5,
            &limits,
        );
        sets.add_neighbor(
            create_test_peer_id(),
            create_test_topology("ap-south", "coop-1"),
            Some(60),
            0.8,
            &limits,
        );

        let metrics = sets.metrics();
        assert_eq!(metrics.local_cluster_count, 1);
        assert_eq!(metrics.regional_count, 1);
        assert_eq!(metrics.backbone_count, 1);
        assert_eq!(metrics.trusted_count, 1);
    }

    #[test]
    fn test_sampling_respects_count() {
        let own_topo = create_test_topology("na-east", "coop-1");
        let mut sets = NeighborSets::new(own_topo);
        let limits = NeighborLimitsConfig::default();

        let local_topo = create_test_topology("na-east", "coop-1");

        // Add only 3 peers
        for _ in 0..3 {
            let peer = create_test_peer_id();
            sets.add_neighbor(peer, local_topo.clone(), Some(10), 0.5, &limits);
        }

        // Request 10 but should only get 3
        let sampled = sets.sample(Scope::LocalCluster, 10);
        assert_eq!(sampled.len(), 3);
    }
}
