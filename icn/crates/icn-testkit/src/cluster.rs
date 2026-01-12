//! Multi-node test cluster management

use anyhow::{Context, Result};
use std::time::Duration;
use tracing::info;

use crate::node::{NodeConfig, TestNode};
use crate::simulation::PartitionConfig;

/// A cluster of test nodes for multi-node integration tests
///
/// TestCluster manages the lifecycle of multiple TestNodes and provides
/// convenience methods for common multi-node test patterns.
///
/// # Example
///
/// ```rust,ignore
/// let cluster = TestCluster::new(3).await?;
/// cluster.fully_connect().await?;
///
/// // All nodes can now communicate
/// for node in &cluster.nodes {
///     assert!(node.peer_count().await >= 2);
/// }
/// ```
#[derive(Debug)]
pub struct TestCluster {
    /// The nodes in this cluster
    pub nodes: Vec<TestNode>,
}

impl TestCluster {
    /// Create a new cluster with the specified number of nodes
    pub async fn new(size: usize) -> Result<Self> {
        Self::with_config(size, NodeConfig::default()).await
    }

    /// Create a new cluster with custom node configuration
    pub async fn with_config(size: usize, config: NodeConfig) -> Result<Self> {
        info!("Creating test cluster with {} nodes", size);

        let mut nodes = Vec::with_capacity(size);
        for i in 0..size {
            let node = TestNode::spawn_with_index(config.clone(), i)
                .await
                .with_context(|| format!("Failed to spawn node {i}"))?;
            nodes.push(node);
        }

        Ok(TestCluster { nodes })
    }

    /// Get the number of nodes in the cluster
    pub fn size(&self) -> usize {
        self.nodes.len()
    }

    /// Get a node by index
    pub fn node(&self, index: usize) -> Option<&TestNode> {
        self.nodes.get(index)
    }

    /// Connect all nodes in a full mesh topology
    ///
    /// After this call, every node is directly connected to every other node.
    pub async fn fully_connect(&self) -> Result<()> {
        info!("Fully connecting {} nodes", self.nodes.len());

        for i in 0..self.nodes.len() {
            for j in (i + 1)..self.nodes.len() {
                // Add delay between connection attempts to avoid overwhelming TLS handshakes
                if i > 0 || j > 1 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                self.nodes[i]
                    .connect(&self.nodes[j])
                    .await
                    .with_context(|| format!("Failed to connect node {i} to node {j}"))?;
            }
        }

        // Allow connections to stabilize
        tokio::time::sleep(Duration::from_millis(200)).await;

        Ok(())
    }

    /// Connect nodes in a ring topology
    ///
    /// Each node connects to the next node, forming a ring.
    /// Node N connects to Node (N+1) % size.
    pub async fn connect_ring(&self) -> Result<()> {
        info!("Connecting {} nodes in a ring", self.nodes.len());

        for i in 0..self.nodes.len() {
            let j = (i + 1) % self.nodes.len();
            self.nodes[i]
                .connect(&self.nodes[j])
                .await
                .with_context(|| format!("Failed to connect node {i} to node {j}"))?;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    /// Connect nodes in a line topology
    ///
    /// Node 0 → Node 1 → Node 2 → ... → Node N
    pub async fn connect_line(&self) -> Result<()> {
        info!("Connecting {} nodes in a line", self.nodes.len());

        for i in 0..(self.nodes.len() - 1) {
            self.nodes[i]
                .connect(&self.nodes[i + 1])
                .await
                .with_context(|| format!("Failed to connect node {} to node {}", i, i + 1))?;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    /// Create a gossip topic on all nodes
    pub async fn create_topic_all(&self, name: &str, access: icn_gossip::AccessControl) {
        for node in &self.nodes {
            node.create_topic(name, access.clone()).await;
        }
    }

    /// Wait for gossip to converge on a specific topic
    ///
    /// Waits until all nodes have the same number of entries for the topic,
    /// or the timeout expires.
    pub async fn await_gossip_convergence(
        &self,
        topic: &str,
        expected_count: usize,
        timeout: Duration,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            let mut all_converged = true;
            for node in &self.nodes {
                if node.entry_count(topic).await != expected_count {
                    all_converged = false;
                    break;
                }
            }

            if all_converged {
                info!(
                    "Gossip converged: all {} nodes have {} entries for topic '{}'",
                    self.nodes.len(),
                    expected_count,
                    topic
                );
                return Ok(());
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Build detailed error message
        let mut counts = Vec::new();
        for node in &self.nodes {
            counts.push(format!(
                "node {}: {}",
                node.index,
                node.entry_count(topic).await
            ));
        }

        anyhow::bail!(
            "Gossip did not converge within {:?}. Expected {} entries. Counts: {}",
            timeout,
            expected_count,
            counts.join(", ")
        )
    }

    /// Create a network partition by disconnecting nodes across groups
    ///
    /// Nodes within each group remain connected, but connections between
    /// groups are severed. This simulates a network partition scenario.
    ///
    /// # Arguments
    ///
    /// * `config` - The partition configuration specifying groups
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the partition was successfully created.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cluster = TestCluster::new(4).await?;
    /// cluster.fully_connect().await?;
    ///
    /// // Create partition: [0,1] | [2,3]
    /// cluster.partition(&PartitionConfig::split_at(4, 2)).await?;
    /// ```
    pub async fn partition(&self, config: &PartitionConfig) -> Result<()> {
        info!("Creating network partition: {:?}", config);

        // Validate partition config
        let total_nodes: usize = config.groups.iter().map(|g| g.len()).sum();
        if total_nodes != self.nodes.len() {
            anyhow::bail!(
                "Partition groups must include all nodes. Expected {}, got {}",
                self.nodes.len(),
                total_nodes
            );
        }

        // Build a map of node index -> group index
        let mut node_group: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for (group_idx, group) in config.groups.iter().enumerate() {
            for &node_idx in group {
                if node_idx >= self.nodes.len() {
                    anyhow::bail!("Invalid node index {node_idx} in partition config");
                }
                node_group.insert(node_idx, group_idx);
            }
        }

        // Disconnect nodes that are in different groups
        for i in 0..self.nodes.len() {
            for j in (i + 1)..self.nodes.len() {
                let group_i = node_group.get(&i).copied().unwrap_or(0);
                let group_j = node_group.get(&j).copied().unwrap_or(0);

                if group_i != group_j {
                    // Disconnect in both directions
                    self.nodes[i].disconnect(&self.nodes[j]).await;
                    self.nodes[j].disconnect(&self.nodes[i]).await;
                    info!(
                        "Partitioned: node {} (group {}) <-> node {} (group {})",
                        i, group_i, j, group_j
                    );
                }
            }
        }

        // Allow time for disconnections to take effect
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Ok(())
    }

    /// Check if two nodes are in the same partition group
    pub fn same_partition_group(config: &PartitionConfig, node_a: usize, node_b: usize) -> bool {
        for group in &config.groups {
            let has_a = group.contains(&node_a);
            let has_b = group.contains(&node_b);
            if has_a && has_b {
                return true;
            }
            if has_a || has_b {
                return false;
            }
        }
        false
    }

    /// Heal a network partition
    ///
    /// Reconnects all nodes that were previously partitioned.
    pub async fn heal_partition(&self) -> Result<()> {
        info!("Healing partition, reconnecting all nodes");
        self.fully_connect().await
    }

    /// Shutdown all nodes in the cluster
    pub async fn shutdown(mut self) {
        info!("Shutting down cluster with {} nodes", self.nodes.len());
        for node in self.nodes.drain(..) {
            node.shutdown().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_cluster() -> Result<()> {
        let cluster = TestCluster::new(3).await?;
        assert_eq!(cluster.size(), 3);
        cluster.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    #[ignore] // QUIC connection timing issues in CI - see issue #319
    async fn test_fully_connect() -> Result<()> {
        let cluster = TestCluster::new(3).await?;
        cluster.fully_connect().await?;

        // Each node should have at least 2 peers
        tokio::time::sleep(Duration::from_millis(500)).await;
        for node in &cluster.nodes {
            assert!(
                node.peer_count().await >= 2,
                "Node {} has {} peers, expected >= 2",
                node.index,
                node.peer_count().await
            );
        }

        cluster.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    #[ignore] // QUIC connection timing issues in CI - see issue #319
    async fn test_gossip_propagation() -> Result<()> {
        let cluster = TestCluster::new(3).await?;
        cluster.fully_connect().await?;

        // Create topic on all nodes
        cluster
            .create_topic_all("test:events", icn_gossip::AccessControl::Public)
            .await;

        // Publish on node 0
        let hash = cluster.nodes[0]
            .publish("test:events", b"hello world")
            .await?;

        // Node 0 should have the entry immediately
        assert!(cluster.nodes[0].has_entry("test:events", &hash).await);

        // Wait for convergence (with a timeout)
        cluster
            .await_gossip_convergence("test:events", 1, Duration::from_secs(5))
            .await?;

        // All nodes should now have the entry
        for node in &cluster.nodes {
            assert!(
                node.has_entry("test:events", &hash).await,
                "Node {} missing entry",
                node.index
            );
        }

        cluster.shutdown().await;
        Ok(())
    }
}
