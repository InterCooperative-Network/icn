//! Example integration tests demonstrating icn-testkit usage
//!
//! This file shows how to use the testkit for common testing scenarios.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use icn_gossip::AccessControl;
use icn_testkit::prelude::*;

/// Example: Spawning a single test node
#[tokio::test]
async fn example_single_node() -> Result<()> {
    // Spawn a node with default config
    let node = TestNode::spawn(NodeConfig::default()).await?;

    // Check the node has a valid DID
    assert!(!node.did.to_string().is_empty());

    // Clean up
    node.shutdown().await;
    Ok(())
}

/// Example: Publishing and retrieving gossip entries
#[tokio::test]
async fn example_gossip_publish() -> Result<()> {
    let node = TestNode::spawn(NodeConfig::default()).await?;

    // Create a topic
    node.create_topic("events:test", AccessControl::Public).await;

    // Publish some data
    let hash1 = node.publish("events:test", b"first event").await?;
    let hash2 = node.publish("events:test", b"second event").await?;

    // Verify entries exist
    assert!(node.has_entry("events:test", &hash1).await);
    assert!(node.has_entry("events:test", &hash2).await);
    assert_eq!(node.entry_count("events:test").await, 2);

    // Get all entries
    let entries = node.get_entries("events:test").await;
    assert_eq!(entries.len(), 2);

    node.shutdown().await;
    Ok(())
}

/// Example: Two nodes connecting
#[tokio::test]
async fn example_two_node_connection() -> Result<()> {
    let node_a = TestNode::spawn(NodeConfig::default()).await?;
    let node_b = TestNode::spawn(NodeConfig::default()).await?;

    // Connect A to B
    node_a.connect(&node_b).await?;

    // Wait for connection to establish
    node_a
        .wait_connected(&node_b.did, Duration::from_secs(5))
        .await?;

    // Verify connection
    assert!(node_a.is_connected_to(&node_b.did).await);

    node_a.shutdown().await;
    node_b.shutdown().await;
    Ok(())
}

/// Example: Creating a test cluster
#[tokio::test]
async fn example_cluster_creation() -> Result<()> {
    // Create a 3-node cluster
    let cluster = TestCluster::new(3).await?;

    // Verify all nodes exist
    assert_eq!(cluster.size(), 3);
    assert!(cluster.node(0).is_some());
    assert!(cluster.node(1).is_some());
    assert!(cluster.node(2).is_some());
    assert!(cluster.node(3).is_none());

    // Each node has a unique DID
    let did0 = &cluster.nodes[0].did;
    let did1 = &cluster.nodes[1].did;
    let did2 = &cluster.nodes[2].did;
    assert_ne!(did0, did1);
    assert_ne!(did1, did2);
    assert_ne!(did0, did2);

    cluster.shutdown().await;
    Ok(())
}

/// Example: Using network simulation presets
#[test]
fn example_network_conditions() {
    // LAN conditions - very fast
    let lan = NetworkCondition::lan();
    assert!(lan.latency < Duration::from_millis(1));

    // Good internet
    let good = NetworkCondition::good_internet();
    assert!(good.latency >= Duration::from_millis(10));

    // Poor internet
    let poor = NetworkCondition::poor_internet();
    assert!(poor.packet_loss > 0.0);

    // Custom conditions
    let custom = NetworkCondition::default()
        .with_latency(Duration::from_millis(50))
        .with_packet_loss(0.01)
        .with_jitter(0.3);
    assert_eq!(custom.latency, Duration::from_millis(50));
}

/// Example: Partition configuration
#[test]
fn example_partition_config() {
    // Split 6 nodes into two groups
    let split = PartitionConfig::split_at(6, 3);
    assert_eq!(split.groups.len(), 2);
    assert_eq!(split.groups[0].len(), 3); // nodes 0, 1, 2
    assert_eq!(split.groups[1].len(), 3); // nodes 3, 4, 5

    // Isolate a single node
    let isolated = PartitionConfig::isolate_node(5, 2);
    assert_eq!(isolated.groups[0], vec![2]); // isolated node
    assert_eq!(isolated.groups[1].len(), 4); // other nodes
}
