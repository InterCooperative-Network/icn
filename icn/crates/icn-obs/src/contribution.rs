//! Contribution metering types and metrics (Phase 21.1)
//!
//! This module provides types and metrics for tracking infrastructure contributions
//! from nodes in the ICN network. Contributions are measured across four resource types:
//!
//! - **Compute**: CPU-hours contributed via job execution
//! - **Storage**: GB-months of data stored and replicated
//! - **Bandwidth**: GB of data transferred for the network
//! - **Uptime**: Node-hours of availability

use serde::{Deserialize, Serialize};

/// Timestamp in Unix epoch seconds
pub type Timestamp = u64;

/// Resource contribution metrics for a single node over a time period
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceMetrics {
    /// The DID string of the contributing node
    pub did: String,
    /// Time period covered (start, end) in Unix epoch seconds
    pub period: (Timestamp, Timestamp),
    /// Compute contribution in CPU-seconds (tracks job completion)
    pub compute_cpu_seconds: u64,
    /// Storage contribution in byte-seconds (GB-months = bytes * seconds / (1e9 * 2592000))
    pub storage_byte_seconds: u64,
    /// Bandwidth contribution in bytes transferred
    pub bandwidth_bytes: u64,
    /// Uptime contribution in seconds
    pub uptime_seconds: u64,
}

impl ResourceMetrics {
    /// Create a new empty metrics instance for a DID and time period
    pub fn new(did: impl Into<String>, start: Timestamp, end: Timestamp) -> Self {
        Self {
            did: did.into(),
            period: (start, end),
            compute_cpu_seconds: 0,
            storage_byte_seconds: 0,
            bandwidth_bytes: 0,
            uptime_seconds: 0,
        }
    }

    /// Convert compute CPU-seconds to CPU-hours
    pub fn compute_hours(&self) -> f64 {
        self.compute_cpu_seconds as f64 / 3600.0
    }

    /// Convert storage byte-seconds to GB-months
    /// (1 GB-month = 1e9 bytes * 30 days * 24 hours * 3600 seconds)
    pub fn storage_gb_months(&self) -> f64 {
        self.storage_byte_seconds as f64 / (1e9 * 2_592_000.0)
    }

    /// Convert bandwidth bytes to GB
    pub fn bandwidth_gb(&self) -> f64 {
        self.bandwidth_bytes as f64 / 1e9
    }

    /// Convert uptime seconds to hours
    pub fn uptime_hours(&self) -> f64 {
        self.uptime_seconds as f64 / 3600.0
    }

    /// Add compute contribution
    pub fn add_compute(&mut self, cpu_seconds: u64) {
        self.compute_cpu_seconds = self.compute_cpu_seconds.saturating_add(cpu_seconds);
    }

    /// Add storage contribution
    pub fn add_storage(&mut self, byte_seconds: u64) {
        self.storage_byte_seconds = self.storage_byte_seconds.saturating_add(byte_seconds);
    }

    /// Add bandwidth contribution
    pub fn add_bandwidth(&mut self, bytes: u64) {
        self.bandwidth_bytes = self.bandwidth_bytes.saturating_add(bytes);
    }

    /// Add uptime contribution
    pub fn add_uptime(&mut self, seconds: u64) {
        self.uptime_seconds = self.uptime_seconds.saturating_add(seconds);
    }

    /// Merge another metrics instance into this one
    /// Periods must match for a valid merge
    pub fn merge(&mut self, other: &ResourceMetrics) -> Result<(), &'static str> {
        if self.did != other.did {
            return Err("Cannot merge metrics from different DIDs");
        }
        if self.period != other.period {
            return Err("Cannot merge metrics from different time periods");
        }
        self.compute_cpu_seconds = self
            .compute_cpu_seconds
            .saturating_add(other.compute_cpu_seconds);
        self.storage_byte_seconds = self
            .storage_byte_seconds
            .saturating_add(other.storage_byte_seconds);
        self.bandwidth_bytes = self.bandwidth_bytes.saturating_add(other.bandwidth_bytes);
        self.uptime_seconds = self.uptime_seconds.saturating_add(other.uptime_seconds);
        Ok(())
    }
}

/// Aggregated contribution metrics across multiple nodes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregatedMetrics {
    /// Total compute CPU-seconds across all nodes
    pub total_compute_cpu_seconds: u64,
    /// Total storage byte-seconds across all nodes
    pub total_storage_byte_seconds: u64,
    /// Total bandwidth bytes across all nodes
    pub total_bandwidth_bytes: u64,
    /// Total uptime seconds across all nodes
    pub total_uptime_seconds: u64,
    /// Number of nodes contributing
    pub node_count: u64,
}

impl AggregatedMetrics {
    /// Add a node's metrics to the aggregate
    pub fn add(&mut self, metrics: &ResourceMetrics) {
        self.total_compute_cpu_seconds = self
            .total_compute_cpu_seconds
            .saturating_add(metrics.compute_cpu_seconds);
        self.total_storage_byte_seconds = self
            .total_storage_byte_seconds
            .saturating_add(metrics.storage_byte_seconds);
        self.total_bandwidth_bytes = self
            .total_bandwidth_bytes
            .saturating_add(metrics.bandwidth_bytes);
        self.total_uptime_seconds = self
            .total_uptime_seconds
            .saturating_add(metrics.uptime_seconds);
        self.node_count += 1;
    }
}

/// Resource type for tracking specific contributions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    /// Compute resources (CPU time)
    Compute,
    /// Storage resources (data replication)
    Storage,
    /// Bandwidth resources (data transfer)
    Bandwidth,
    /// Uptime resources (node availability)
    Uptime,
}

impl ResourceType {
    /// Get the unit for this resource type
    pub fn unit(&self) -> &'static str {
        match self {
            ResourceType::Compute => "cpu-hours",
            ResourceType::Storage => "gb-months",
            ResourceType::Bandwidth => "gb",
            ResourceType::Uptime => "hours",
        }
    }

    /// Get the Prometheus metric name suffix for this resource type
    pub fn metric_suffix(&self) -> &'static str {
        match self {
            ResourceType::Compute => "compute_cpu_seconds",
            ResourceType::Storage => "storage_byte_seconds",
            ResourceType::Bandwidth => "bandwidth_bytes",
            ResourceType::Uptime => "uptime_seconds",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DID: &str = "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    const TEST_DID_2: &str = "did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

    #[test]
    fn test_resource_metrics_creation() {
        let metrics = ResourceMetrics::new(TEST_DID, 1000, 2000);

        assert_eq!(metrics.did, TEST_DID);
        assert_eq!(metrics.period, (1000, 2000));
        assert_eq!(metrics.compute_cpu_seconds, 0);
        assert_eq!(metrics.storage_byte_seconds, 0);
        assert_eq!(metrics.bandwidth_bytes, 0);
        assert_eq!(metrics.uptime_seconds, 0);
    }

    #[test]
    fn test_add_contributions() {
        let mut metrics = ResourceMetrics::new(TEST_DID, 1000, 2000);

        metrics.add_compute(3600); // 1 hour
        metrics.add_storage(1_000_000_000 * 2_592_000); // 1 GB-month
        metrics.add_bandwidth(1_000_000_000); // 1 GB
        metrics.add_uptime(3600); // 1 hour

        assert_eq!(metrics.compute_hours(), 1.0);
        assert!((metrics.storage_gb_months() - 1.0).abs() < 0.001);
        assert_eq!(metrics.bandwidth_gb(), 1.0);
        assert_eq!(metrics.uptime_hours(), 1.0);
    }

    #[test]
    fn test_merge_metrics() {
        let mut m1 = ResourceMetrics::new(TEST_DID, 1000, 2000);
        m1.add_compute(100);
        m1.add_bandwidth(500);

        let mut m2 = ResourceMetrics::new(TEST_DID, 1000, 2000);
        m2.add_compute(200);
        m2.add_storage(300);

        m1.merge(&m2).unwrap();

        assert_eq!(m1.compute_cpu_seconds, 300);
        assert_eq!(m1.storage_byte_seconds, 300);
        assert_eq!(m1.bandwidth_bytes, 500);
    }

    #[test]
    fn test_merge_different_dids_fails() {
        let mut m1 = ResourceMetrics::new(TEST_DID, 1000, 2000);
        let m2 = ResourceMetrics::new(TEST_DID_2, 1000, 2000);

        assert!(m1.merge(&m2).is_err());
    }

    #[test]
    fn test_aggregated_metrics() {
        let mut agg = AggregatedMetrics::default();

        let mut m1 = ResourceMetrics::new(TEST_DID, 1000, 2000);
        m1.add_compute(100);
        agg.add(&m1);

        let mut m2 = ResourceMetrics::new(TEST_DID, 1000, 2000);
        m2.add_compute(200);
        agg.add(&m2);

        assert_eq!(agg.total_compute_cpu_seconds, 300);
        assert_eq!(agg.node_count, 2);
    }

    #[test]
    fn test_resource_type_units() {
        assert_eq!(ResourceType::Compute.unit(), "cpu-hours");
        assert_eq!(ResourceType::Storage.unit(), "gb-months");
        assert_eq!(ResourceType::Bandwidth.unit(), "gb");
        assert_eq!(ResourceType::Uptime.unit(), "hours");
    }
}
