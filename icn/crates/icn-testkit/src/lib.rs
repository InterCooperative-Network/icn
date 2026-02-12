//! ICN Testkit - Testing utilities and simulation harness
//!
//! This crate provides a comprehensive testing framework for ICN integration tests.
//! It enables multi-node test scenarios with proper isolation, lifecycle management,
//! and helper utilities for common test patterns.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use icn_testkit::{TestNode, TestCluster};
//! use std::time::Duration;
//!
//! #[tokio::test]
//! async fn test_gossip_propagation() -> anyhow::Result<()> {
//!     // Create a 3-node cluster
//!     let cluster = TestCluster::new(3).await?;
//!     cluster.fully_connect().await?;
//!
//!     // Publish on node 0, verify on all nodes
//!     let hash = cluster.nodes[0].publish("test:topic", b"hello").await?;
//!     cluster.await_gossip_convergence("test:topic", 1, Duration::from_secs(5)).await?;
//!
//!     for node in &cluster.nodes {
//!         assert!(node.has_entry("test:topic", &hash).await);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Features
//!
//! - **TestNode**: Individual test node with isolated state
//! - **TestCluster**: Multi-node cluster with automatic lifecycle management
//! - **Port Allocation**: Automatic unique port assignment
//! - **Convergence Helpers**: Wait for gossip/ledger sync
//! - **Network Simulation**: Latency injection, partition simulation
//! - **State Inspection**: Query node state for assertions

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow in tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod cluster;
mod node;
mod simulation;
mod util;

pub use cluster::TestCluster;
pub use node::{NodeConfig, TestNode};
pub use simulation::{NetworkCondition, PartitionConfig};
pub use util::{install_crypto_provider, pick_port, poll_with_backoff, BackoffConfig, TestResult};

/// Re-export common types for convenience
pub mod prelude {
    pub use crate::{
        install_crypto_provider, pick_port, poll_with_backoff, BackoffConfig, NetworkCondition,
        NodeConfig, PartitionConfig, TestCluster, TestNode, TestResult,
    };
    pub use anyhow::Result;
    pub use std::time::Duration;
}
