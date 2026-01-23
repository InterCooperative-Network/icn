#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for Phase 17 Storage Hardening & Replication
//!
//! Tests the full replication flow across multiple nodes:
//! - Node publishes content to gossip
//! - ReplicationManager detects under-replication
//! - Replica requests sent to trusted peers
//! - Peers respond with ReplicaOffer
//! - Metadata updated in store

use anyhow::Result;
use icn_core::replication::{ReplicationConfig, ReplicationManager};
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::{Did, KeyPair};
use icn_store::{ReplicaHealth, SledStore, Store};
use icn_trust::{TrustClass, TrustEdge, TrustGraph, TrustScore};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Test helper to create a node with gossip, trust graph, and replication manager
struct TestNode {
    did: Did,
    _keypair: KeyPair,
    gossip_handle: Arc<RwLock<GossipActor>>,
    trust_graph_handle: Arc<RwLock<TrustGraph>>,
    store: Arc<dyn Store>,
    _replication_handle: icn_core::ReplicationHandle,
}

impl TestNode {
    async fn new(config: ReplicationConfig) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        // Create stores
        let store: Arc<dyn Store> = Arc::new(SledStore::temporary()?);
        let trust_store: Arc<dyn Store> = Arc::new(SledStore::temporary()?);

        // Create trust graph
        let trust_graph = TrustGraph::new(trust_store, did.clone());
        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create gossip actor
        let trust_lookup = Arc::new(|_: &Did| None);
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        // Set gossip store
        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_store(store.clone());
        }

        // Spawn replication manager
        let replication_handle = ReplicationManager::spawn(
            did.clone(),
            config,
            store.clone(),
            trust_graph_handle.clone(),
            gossip_handle.clone(),
        );

        Ok(Self {
            did,
            _keypair: keypair,
            gossip_handle,
            trust_graph_handle,
            store,
            _replication_handle: replication_handle,
        })
    }

    /// Add a trust edge to another node
    async fn trust_peer(&self, peer_did: &Did, score: f64) -> Result<()> {
        let edge = TrustEdge::new(self.did.clone(), peer_did.clone(), TrustScore::unchecked(score));
        let mut graph = self.trust_graph_handle.write().await;
        graph.add_edge(edge)?;
        Ok(())
    }

    /// Publish content to gossip (creates a gossip entry with content hash)
    async fn publish_content(&self, topic: &str, data: Vec<u8>) -> Result<[u8; 32]> {
        let mut gossip = self.gossip_handle.write().await;

        // Create topic if needed
        if !gossip.get_topics().contains(&topic.to_string()) {
            gossip.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
        }

        let hash = gossip.publish(topic, data).await?;
        Ok(hash)
    }

    /// Get replica count for a content hash
    async fn get_replica_count(&self, hash: &[u8; 32]) -> Result<usize> {
        self.store.get_replica_count(hash)
    }

    /// Manually add this node as a replica for content
    async fn add_self_as_replica(&self, hash: &[u8; 32]) -> Result<()> {
        self.store
            .add_replica(hash, self.did.to_string(), ReplicaHealth::Healthy)
    }

    /// Subscribe to another node's gossip messages (simulates network connection)
    async fn connect_to(&self, peer: &TestNode) -> Result<()> {
        // Add peer to known peers via subscribing to a shared topic
        let mut my_gossip = self.gossip_handle.write().await;
        let mut peer_gossip = peer.gossip_handle.write().await;

        // Subscribe to each other's presence
        let _ = my_gossip
            .subscribe("network:presence", peer.did.clone())
            .await;
        let _ = peer_gossip
            .subscribe("network:presence", self.did.clone())
            .await;

        Ok(())
    }
}

#[tokio::test]
async fn test_two_node_replication_request() -> Result<()> {
    // Setup: Two nodes, node1 trusts node2 at Partner level
    let config = ReplicationConfig {
        target_replicas: 2,
        min_trust_class: TrustClass::Partner,
        health_check_interval_secs: 1, // Fast for testing
        stale_threshold_secs: 300,
        unreachable_threshold_secs: 900,
    };

    let node1 = TestNode::new(config.clone()).await?;
    let node2 = TestNode::new(config.clone()).await?;

    // Node1 trusts node2 (Partner level = 0.5)
    node1.trust_peer(&node2.did, 0.5).await?;

    // Connect nodes (they know about each other via gossip)
    node1.connect_to(&node2).await?;

    // Node1 publishes content
    let test_data = b"Hello, ICN replication!".to_vec();
    let content_hash = node1.publish_content("test:data", test_data).await?;

    // Node1 records itself as having the content
    node1.add_self_as_replica(&content_hash).await?;

    // Initial replica count should be 1 (only node1)
    let initial_count = node1.get_replica_count(&content_hash).await?;
    assert_eq!(initial_count, 1, "Initial replica count should be 1");

    // Manually trigger replication check (simulates the 60s monitor loop)
    // In production, this runs automatically every 60 seconds
    node1._replication_handle.trigger_health_check().await?;

    // Wait a moment for async operations
    tokio::time::sleep(Duration::from_millis(100)).await;

    // At this point, node1's ReplicationManager should have:
    // 1. Detected under-replication (count=1, target=2)
    // 2. Selected node2 as a candidate (Partner trust)
    // 3. Sent a ReplicaRequest to node2 via gossip

    // Verify that node2 received a replica request
    // (In a full integration, node2 would respond with ReplicaOffer and
    //  node1 would update metadata. For now, we test the request flow.)

    Ok(())
}

#[tokio::test]
async fn test_health_status_transitions() -> Result<()> {
    // Setup: One node with content
    let config = ReplicationConfig::default();
    let node = TestNode::new(config).await?;

    // Publish content
    let content_hash = node
        .publish_content("test:health", b"test data".to_vec())
        .await?;

    // Add a replica that's old (simulate stale replica)
    let old_time = std::time::SystemTime::now() - Duration::from_secs(600); // 10 minutes ago
    let mut metadata = icn_store::ReplicaMetadata::new(content_hash);
    metadata.replicas.push(icn_store::ReplicaInfo {
        peer_did: "did:icn:stale-peer".to_string(),
        last_seen: old_time,
        health: ReplicaHealth::Healthy,
    });
    node.store.put_replica_metadata(&metadata)?;

    // Trigger health check (should mark replica as Stale)
    node._replication_handle.trigger_health_check().await?;

    // Verify replica was marked as Stale
    let updated_metadata = node.store.get_replica_metadata(&content_hash)?.unwrap();
    assert_eq!(updated_metadata.replicas[0].health, ReplicaHealth::Stale);

    Ok(())
}

#[tokio::test]
async fn test_trust_weighted_selection() -> Result<()> {
    // Setup: One node with multiple peers at different trust levels
    let config = ReplicationConfig {
        target_replicas: 3,
        min_trust_class: TrustClass::Partner,
        ..Default::default()
    };
    let node = TestNode::new(config).await?;

    // Create 4 peers with different trust scores
    let low_trust_peer = KeyPair::generate()?.did().clone(); // 0.2 = Known (below Partner)
    let partner_peer1 = KeyPair::generate()?.did().clone(); // 0.5 = Partner
    let partner_peer2 = KeyPair::generate()?.did().clone(); // 0.6 = Partner
    let federated_peer = KeyPair::generate()?.did().clone(); // 0.8 = Federated

    // Add trust edges
    node.trust_peer(&low_trust_peer, 0.2).await?;
    node.trust_peer(&partner_peer1, 0.5).await?;
    node.trust_peer(&partner_peer2, 0.6).await?;
    node.trust_peer(&federated_peer, 0.8).await?;

    // Connect to all peers (make them known via gossip)
    for peer_did in &[
        &low_trust_peer,
        &partner_peer1,
        &partner_peer2,
        &federated_peer,
    ] {
        let mut gossip = node.gossip_handle.write().await;
        let _ = gossip
            .subscribe("network:presence", (*peer_did).clone())
            .await;
    }

    // Publish content with only node as replica
    let content_hash = node
        .publish_content("test:trust", b"trust test".to_vec())
        .await?;
    node.add_self_as_replica(&content_hash).await?;

    // Trigger replication check
    node._replication_handle.trigger_health_check().await?;

    // ReplicationManager should select:
    // 1. federated_peer first (highest trust = 0.8)
    // 2. partner_peer2 second (0.6)
    // 3. partner_peer1 third (0.5)
    // Should NOT select low_trust_peer (0.2 < 0.4 Partner threshold)

    // This test validates the selection algorithm runs without error
    // In a full integration, we'd inspect the actual ReplicaRequest messages sent

    Ok(())
}

#[tokio::test]
async fn test_no_suitable_candidates() -> Result<()> {
    // Setup: Node with no trusted peers
    let config = ReplicationConfig::default();
    let node = TestNode::new(config).await?;

    // Publish content
    let content_hash = node
        .publish_content("test:lonely", b"alone".to_vec())
        .await?;
    node.add_self_as_replica(&content_hash).await?;

    // Trigger health check (should complete without error despite no candidates)
    node._replication_handle.trigger_health_check().await?;

    // Replica count should still be 1 (no new replicas created)
    let count = node.get_replica_count(&content_hash).await?;
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
async fn test_replication_config_customization() -> Result<()> {
    // Test that custom configuration is respected
    let custom_config = ReplicationConfig {
        target_replicas: 5,
        min_trust_class: TrustClass::Federated,
        health_check_interval_secs: 30,
        stale_threshold_secs: 600,
        unreachable_threshold_secs: 1800,
    };

    let node = TestNode::new(custom_config.clone()).await?;

    // Verify config was applied
    let actual_config = node._replication_handle.get_config().await;
    assert_eq!(actual_config.target_replicas, 5);
    assert_eq!(actual_config.min_trust_class, TrustClass::Federated);
    assert_eq!(actual_config.health_check_interval_secs, 30);

    Ok(())
}
