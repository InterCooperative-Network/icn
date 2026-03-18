//! Network simulation utilities for testing
//!
//! This module provides configuration types for network simulation.
//! Currently, these types define simulation parameters but are not yet
//! integrated into the actual network layer. They serve as:
//!
//! 1. Documentation of intended simulation capabilities
//! 2. Configuration types for future implementation
//! 3. Test fixtures for partition configuration validation
//!
//! # Future Work
//!
//! Full network simulation would require integration with NetworkActor
//! to intercept and modify message delivery based on these conditions.

use std::time::Duration;

/// Network conditions to simulate
#[derive(Debug, Clone)]
pub struct NetworkCondition {
    /// Latency to add to all messages (one-way)
    pub latency: Duration,
    /// Packet loss percentage (0.0 - 1.0)
    pub packet_loss: f64,
    /// Bandwidth limit in bytes/sec (None = unlimited)
    pub bandwidth_limit: Option<u64>,
    /// Jitter as percentage of latency (0.0 - 1.0)
    pub jitter: f64,
}

impl Default for NetworkCondition {
    fn default() -> Self {
        Self {
            latency: Duration::ZERO,
            packet_loss: 0.0,
            bandwidth_limit: None,
            jitter: 0.0,
        }
    }
}

impl NetworkCondition {
    /// Create network conditions simulating a LAN
    pub fn lan() -> Self {
        Self {
            latency: Duration::from_micros(100),
            packet_loss: 0.0,
            bandwidth_limit: None,
            jitter: 0.1,
        }
    }

    /// Create network conditions simulating a good internet connection
    pub fn good_internet() -> Self {
        Self {
            latency: Duration::from_millis(20),
            packet_loss: 0.001,
            bandwidth_limit: Some(100 * 1024 * 1024), // 100 MB/s
            jitter: 0.2,
        }
    }

    /// Create network conditions simulating a poor internet connection
    pub fn poor_internet() -> Self {
        Self {
            latency: Duration::from_millis(200),
            packet_loss: 0.05,
            bandwidth_limit: Some(1024 * 1024), // 1 MB/s
            jitter: 0.5,
        }
    }

    /// Create network conditions simulating a very unreliable connection
    pub fn unreliable() -> Self {
        Self {
            latency: Duration::from_millis(500),
            packet_loss: 0.2,
            bandwidth_limit: Some(100 * 1024), // 100 KB/s
            jitter: 1.0,
        }
    }

    /// Set latency
    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.latency = latency;
        self
    }

    /// Set packet loss rate (0.0 - 1.0)
    pub fn with_packet_loss(mut self, rate: f64) -> Self {
        self.packet_loss = rate.clamp(0.0, 1.0);
        self
    }

    /// Set bandwidth limit in bytes/sec
    pub fn with_bandwidth_limit(mut self, limit: u64) -> Self {
        self.bandwidth_limit = Some(limit);
        self
    }

    /// Set jitter as percentage of latency
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter.clamp(0.0, 2.0);
        self
    }
}

/// Configuration for network partitions
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Groups of node indices that can communicate with each other
    pub groups: Vec<Vec<usize>>,
}

impl PartitionConfig {
    /// Create a new partition configuration
    pub fn new(groups: Vec<Vec<usize>>) -> Self {
        Self { groups }
    }

    /// Create a simple two-way split
    ///
    /// Nodes 0..mid go in group A, nodes mid.. go in group B
    pub fn split_at(total_nodes: usize, mid: usize) -> Self {
        let group_a: Vec<usize> = (0..mid).collect();
        let group_b: Vec<usize> = (mid..total_nodes).collect();
        Self::new(vec![group_a, group_b])
    }

    /// Create an even split (as close to 50/50 as possible)
    pub fn even_split(total_nodes: usize) -> Self {
        Self::split_at(total_nodes, total_nodes / 2)
    }

    /// Isolate a single node from the rest
    pub fn isolate_node(total_nodes: usize, isolated: usize) -> Self {
        let others: Vec<usize> = (0..total_nodes).filter(|&i| i != isolated).collect();
        Self::new(vec![vec![isolated], others])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_conditions() {
        let cond = NetworkCondition::default();
        assert_eq!(cond.latency, Duration::ZERO);
        assert_eq!(cond.packet_loss, 0.0);

        let lan = NetworkCondition::lan();
        assert!(lan.latency < Duration::from_millis(1));

        let poor = NetworkCondition::poor_internet();
        assert!(poor.packet_loss > 0.0);
        assert!(poor.bandwidth_limit.is_some());
    }

    #[test]
    fn test_partition_config() {
        let split = PartitionConfig::split_at(6, 3);
        assert_eq!(split.groups.len(), 2);
        assert_eq!(split.groups[0], vec![0, 1, 2]);
        assert_eq!(split.groups[1], vec![3, 4, 5]);

        let even = PartitionConfig::even_split(5);
        assert_eq!(even.groups[0], vec![0, 1]);
        assert_eq!(even.groups[1], vec![2, 3, 4]);

        let isolated = PartitionConfig::isolate_node(4, 2);
        assert_eq!(isolated.groups[0], vec![2]);
        assert_eq!(isolated.groups[1], vec![0, 1, 3]);
    }
}
