//! Chaos Engineering Tests for ICN
//!
//! These tests verify system resilience under adverse conditions:
//! - Network partitions
//! - Node failures
//! - Storage corruption
//! - Message corruption
//!
//! See issue #226 for the full chaos engineering test plan.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use icn_gossip::AccessControl;
use icn_testkit::prelude::*;
use std::time::Duration;

/// Test that a cluster recovers from a network partition
///
/// Scenario:
/// 1. Create a 4-node cluster, fully connected
/// 2. Publish an entry on node 0
/// 3. Create partition: [0,1] | [2,3]
/// 4. Publish entries in each partition
/// 5. Heal partition
/// 6. Verify all nodes converge to same state
#[tokio::test]
#[ignore] // QUIC connection timing issues in CI - see issue #319
async fn test_network_partition_recovery() -> Result<()> {
    let cluster = TestCluster::new(4).await?;
    cluster.fully_connect().await?;

    // Create topic on all nodes
    let topic = "chaos:partition";
    cluster.create_topic_all(topic, AccessControl::Public).await;

    // Brief delay to ensure connections are stable
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Publish initial entry on node 0
    let hash_pre = cluster.nodes[0].publish(topic, b"pre-partition").await?;

    // Wait for initial convergence
    cluster
        .await_gossip_convergence(topic, 1, Duration::from_secs(5))
        .await?;

    // Create partition: [0,1] | [2,3]
    let partition_config = PartitionConfig::split_at(4, 2);
    cluster.partition(&partition_config).await?;

    // Publish in partition A (nodes 0,1)
    let hash_a = cluster.nodes[0].publish(topic, b"partition-a").await?;

    // Publish in partition B (nodes 2,3)
    let hash_b = cluster.nodes[2].publish(topic, b"partition-b").await?;

    // Wait a bit for intra-partition gossip
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify partition isolation: node 0 should have hash_a but not hash_b
    assert!(
        cluster.nodes[0].has_entry(topic, &hash_a).await,
        "Node 0 should have partition A entry"
    );
    assert!(
        !cluster.nodes[0].has_entry(topic, &hash_b).await,
        "Node 0 should NOT have partition B entry during partition"
    );

    // Verify partition isolation: node 2 should have hash_b but not hash_a
    assert!(
        cluster.nodes[2].has_entry(topic, &hash_b).await,
        "Node 2 should have partition B entry"
    );
    assert!(
        !cluster.nodes[2].has_entry(topic, &hash_a).await,
        "Node 2 should NOT have partition A entry during partition"
    );

    // Heal partition
    cluster.heal_partition().await?;

    // Wait for convergence after healing
    cluster
        .await_gossip_convergence(topic, 3, Duration::from_secs(10))
        .await?;

    // Verify all nodes have all entries
    for (i, node) in cluster.nodes.iter().enumerate() {
        assert!(
            node.has_entry(topic, &hash_pre).await,
            "Node {i} missing pre-partition entry"
        );
        assert!(
            node.has_entry(topic, &hash_a).await,
            "Node {i} missing partition A entry"
        );
        assert!(
            node.has_entry(topic, &hash_b).await,
            "Node {i} missing partition B entry"
        );
    }

    cluster.shutdown().await;
    Ok(())
}

/// Test that isolating a single node and healing works correctly
///
/// Scenario:
/// 1. Create a 4-node cluster
/// 2. Isolate node 2
/// 3. Publish entries on both sides
/// 4. Heal and verify convergence
#[tokio::test]
#[ignore] // QUIC connection timing issues in CI - see issue #319
async fn test_single_node_isolation() -> Result<()> {
    let cluster = TestCluster::new(4).await?;
    cluster.fully_connect().await?;

    let topic = "chaos:isolation";
    cluster.create_topic_all(topic, AccessControl::Public).await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Isolate node 2
    let partition_config = PartitionConfig::isolate_node(4, 2);
    cluster.partition(&partition_config).await?;

    // Publish on the majority (nodes 0,1,3)
    let hash_majority = cluster.nodes[0].publish(topic, b"majority").await?;

    // Publish on isolated node
    let hash_isolated = cluster.nodes[2].publish(topic, b"isolated").await?;

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Majority nodes should have majority entry
    assert!(cluster.nodes[0].has_entry(topic, &hash_majority).await);
    assert!(cluster.nodes[1].has_entry(topic, &hash_majority).await);
    assert!(cluster.nodes[3].has_entry(topic, &hash_majority).await);

    // Isolated node should not have majority entry
    assert!(!cluster.nodes[2].has_entry(topic, &hash_majority).await);

    // Heal partition
    cluster.heal_partition().await?;

    // Wait for convergence
    cluster
        .await_gossip_convergence(topic, 2, Duration::from_secs(10))
        .await?;

    // All nodes should have both entries
    for (i, node) in cluster.nodes.iter().enumerate() {
        assert!(
            node.has_entry(topic, &hash_majority).await,
            "Node {i} missing majority entry"
        );
        assert!(
            node.has_entry(topic, &hash_isolated).await,
            "Node {i} missing isolated entry"
        );
    }

    cluster.shutdown().await;
    Ok(())
}

/// Test rapid connect/disconnect cycles (flapping connection)
///
/// Scenario:
/// 1. Create two nodes
/// 2. Connect, publish, disconnect, repeat
/// 3. Verify entries eventually converge
#[tokio::test]
#[ignore] // QUIC connection timing issues in CI - see issue #319
async fn test_connection_flapping() -> Result<()> {
    let cluster = TestCluster::new(2).await?;

    let topic = "chaos:flapping";
    cluster.create_topic_all(topic, AccessControl::Public).await;

    let mut hashes = Vec::new();

    // Flap connection 3 times
    for i in 0..3 {
        // Connect
        cluster.nodes[0].connect(&cluster.nodes[1]).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Publish from each side
        let h0 = cluster.nodes[0]
            .publish(topic, format!("node0-round{i}").as_bytes())
            .await?;
        let h1 = cluster.nodes[1]
            .publish(topic, format!("node1-round{i}").as_bytes())
            .await?;
        hashes.push(h0);
        hashes.push(h1);

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Disconnect
        cluster.nodes[0].disconnect(&cluster.nodes[1]).await;
        cluster.nodes[1].disconnect(&cluster.nodes[0]).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Final connect and wait for sync
    cluster.nodes[0].connect(&cluster.nodes[1]).await?;

    // Wait for convergence (6 entries total)
    cluster
        .await_gossip_convergence(topic, 6, Duration::from_secs(10))
        .await?;

    // Verify all entries on both nodes
    for hash in &hashes {
        assert!(
            cluster.nodes[0].has_entry(topic, hash).await,
            "Node 0 missing entry"
        );
        assert!(
            cluster.nodes[1].has_entry(topic, hash).await,
            "Node 1 missing entry"
        );
    }

    cluster.shutdown().await;
    Ok(())
}

/// Test that TestNode uses temporary storage (no persistence)
///
/// This test verifies that TestNode creates fresh state on each spawn,
/// which is the expected behavior for isolated test environments.
/// A "restarted" node gets a new identity and empty storage.
#[tokio::test]
async fn test_node_temporary_storage() -> Result<()> {
    // Create initial node
    let node1 = TestNode::spawn(NodeConfig::default()).await?;
    node1
        .create_topic("chaos:restart", AccessControl::Public)
        .await;

    // Publish some data
    let hash = node1.publish("chaos:restart", b"before-restart").await?;
    assert!(node1.has_entry("chaos:restart", &hash).await);

    // Get the DID before shutdown
    let did1 = node1.did.clone();

    // Shutdown (simulates node going down)
    node1.shutdown().await;

    // Create new node (simulates restart - note: fresh identity)
    let node2 = TestNode::spawn(NodeConfig::default()).await?;
    node2
        .create_topic("chaos:restart", AccessControl::Public)
        .await;

    // New node has different DID (fresh identity)
    assert_ne!(
        did1, node2.did,
        "Restarted node should have new DID with temp storage"
    );

    // New node starts fresh (no persistence with temp storage)
    assert!(!node2.has_entry("chaos:restart", &hash).await);

    node2.shutdown().await;
    Ok(())
}

/// Test cluster behavior under high message volume
///
/// This is a stress test to ensure gossip handles burst traffic.
#[tokio::test]
#[ignore] // Long-running stress test
async fn test_message_burst() -> Result<()> {
    let cluster = TestCluster::new(3).await?;
    cluster.fully_connect().await?;

    let topic = "chaos:burst";
    cluster.create_topic_all(topic, AccessControl::Public).await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Publish many messages in rapid succession
    let mut hashes = Vec::new();
    for i in 0..50 {
        let node_idx = i % 3;
        let hash = cluster.nodes[node_idx]
            .publish(topic, format!("burst-{i}").as_bytes())
            .await?;
        hashes.push(hash);
    }

    // Wait for convergence (with longer timeout for burst)
    cluster
        .await_gossip_convergence(topic, 50, Duration::from_secs(30))
        .await?;

    // Verify random sample of entries
    for hash in hashes.iter().step_by(10) {
        for node in &cluster.nodes {
            assert!(node.has_entry(topic, hash).await, "Missing entry in burst");
        }
    }

    cluster.shutdown().await;
    Ok(())
}

/// Test that partition helper correctly identifies same-group nodes
#[test]
fn test_same_partition_group() {
    let config = PartitionConfig::split_at(6, 3);

    // Same group: 0,1,2 and 3,4,5
    assert!(TestCluster::same_partition_group(&config, 0, 1));
    assert!(TestCluster::same_partition_group(&config, 0, 2));
    assert!(TestCluster::same_partition_group(&config, 3, 4));
    assert!(TestCluster::same_partition_group(&config, 3, 5));

    // Different groups
    assert!(!TestCluster::same_partition_group(&config, 0, 3));
    assert!(!TestCluster::same_partition_group(&config, 2, 4));
    assert!(!TestCluster::same_partition_group(&config, 1, 5));
}

/// Test even partition split
#[test]
fn test_even_split_partition() {
    let config = PartitionConfig::even_split(5);
    assert_eq!(config.groups.len(), 2);
    assert_eq!(config.groups[0], vec![0, 1]);
    assert_eq!(config.groups[1], vec![2, 3, 4]);
}

/// Test node isolation partition
#[test]
fn test_isolate_node_partition() {
    let config = PartitionConfig::isolate_node(4, 2);
    assert_eq!(config.groups.len(), 2);
    assert_eq!(config.groups[0], vec![2]); // Isolated
    assert_eq!(config.groups[1], vec![0, 1, 3]); // Others
}

#[cfg(test)]
mod disconnect_tests {
    use super::*;

    /// Test basic disconnect functionality
    ///
    /// Verifies that disconnect_peer correctly closes connections and
    /// updates tracking state.
    #[tokio::test]
    async fn test_disconnect_basic() -> Result<()> {
        let node1 = TestNode::spawn(NodeConfig::default()).await?;
        let node2 = TestNode::spawn(NodeConfig::default()).await?;

        // Connect
        node1.connect(&node2).await?;
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify connected
        assert!(
            node1.is_connected_to(&node2.did).await,
            "node1 should be connected to node2 after connect()"
        );

        // Disconnect and verify return value
        let disconnected = node1.disconnect(&node2).await;
        assert!(
            disconnected,
            "disconnect should return true for connected peer"
        );

        // Wait for disconnection to complete with timeout
        node1
            .wait_disconnected(&node2.did, Duration::from_secs(2))
            .await?;

        // Verify actually disconnected
        assert!(
            !node1.is_connected_to(&node2.did).await,
            "node1 should not be connected after disconnect"
        );

        node1.shutdown().await;
        node2.shutdown().await;
        Ok(())
    }

    /// Test disconnect of non-existent peer returns false
    #[tokio::test]
    async fn test_disconnect_nonexistent() -> Result<()> {
        let node1 = TestNode::spawn(NodeConfig::default()).await?;
        let node2 = TestNode::spawn(NodeConfig::default()).await?;

        // Never connected, so disconnect should return false
        let disconnected = node1.disconnect(&node2).await;
        assert!(
            !disconnected,
            "disconnect of non-connected peer should return false"
        );

        node1.shutdown().await;
        node2.shutdown().await;
        Ok(())
    }
}
