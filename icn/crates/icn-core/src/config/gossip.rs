//! Gossip protocol configuration
//!
//! # Example TOML Configuration
//!
//! ```toml
//! [gossip]
//! # Replication settings for data durability
//! [gossip.replication]
//! target_replicas = 3              # Number of replicas per content hash
//! min_replica_trust = 0.4          # Partner trust class required
//! health_check_interval_secs = 60  # How often to check replica health
//! stale_threshold_secs = 300       # Mark stale after 5 minutes
//! unreachable_threshold_secs = 900 # Mark unreachable after 15 minutes
//!
//! # Partition detection for split-brain prevention
//! [gossip.partition]
//! silence_threshold_secs = 300     # Suspect partition after 5 min silence
//! check_interval_secs = 30         # How often to check for partitions
//! auto_heal_enabled = true         # Automatically attempt healing
//! heal_interval_secs = 60          # How often to attempt healing
//! ```

use serde::{Deserialize, Serialize};

/// Gossip protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GossipConfig {
    /// Replication settings for data durability
    #[serde(default)]
    pub replication: ReplicationConfig,

    /// Partition detection settings for split-brain prevention
    #[serde(default)]
    pub partition: PartitionConfig,
}

/// Replication configuration for data durability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// Target number of replicas per content hash
    #[serde(default = "default_target_replicas")]
    pub target_replicas: usize,

    /// Minimum trust class required to serve as replica (Known = 0.1, Partner = 0.4, Federated = 0.7)
    /// Default: 0.4 (Partner)
    #[serde(default = "default_min_replica_trust")]
    pub min_replica_trust: f64,

    /// Health check interval in seconds
    #[serde(default = "default_health_check_interval_secs")]
    pub health_check_interval_secs: u64,

    /// Stale threshold - replicas not seen in this duration (seconds) are marked Stale
    #[serde(default = "default_stale_threshold_secs")]
    pub stale_threshold_secs: u64,

    /// Unreachable threshold - replicas not seen in this duration (seconds) are marked Unreachable
    #[serde(default = "default_unreachable_threshold_secs")]
    pub unreachable_threshold_secs: u64,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            target_replicas: default_target_replicas(),
            min_replica_trust: default_min_replica_trust(),
            health_check_interval_secs: default_health_check_interval_secs(),
            stale_threshold_secs: default_stale_threshold_secs(),
            unreachable_threshold_secs: default_unreachable_threshold_secs(),
        }
    }
}

fn default_target_replicas() -> usize {
    3
}

fn default_min_replica_trust() -> f64 {
    0.4 // Partner trust class
}

fn default_health_check_interval_secs() -> u64 {
    60
}

fn default_stale_threshold_secs() -> u64 {
    300 // 5 minutes
}

fn default_unreachable_threshold_secs() -> u64 {
    900 // 15 minutes
}

/// Partition detection configuration for split-brain prevention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionConfig {
    /// Silence threshold in seconds - if no messages received for this duration, suspect partition
    #[serde(default = "default_silence_threshold_secs")]
    pub silence_threshold_secs: u64,

    /// Check interval in seconds - how often to run partition detection
    #[serde(default = "default_check_interval_secs")]
    pub check_interval_secs: u64,

    /// Enable automatic partition healing
    #[serde(default = "default_true")]
    pub auto_heal_enabled: bool,

    /// Heal interval in seconds - how often to attempt partition healing
    #[serde(default = "default_heal_interval_secs")]
    pub heal_interval_secs: u64,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            silence_threshold_secs: default_silence_threshold_secs(),
            check_interval_secs: default_check_interval_secs(),
            auto_heal_enabled: default_true(),
            heal_interval_secs: default_heal_interval_secs(),
        }
    }
}

fn default_silence_threshold_secs() -> u64 {
    300 // 5 minutes
}

fn default_check_interval_secs() -> u64 {
    30 // 30 seconds
}

fn default_heal_interval_secs() -> u64 {
    60 // 1 minute
}

fn default_true() -> bool {
    true
}

impl ReplicationConfig {
    /// Convert to icn-core ReplicationConfig used by ReplicationManager
    pub fn to_manager_config(&self) -> crate::replication::ReplicationConfig {
        crate::replication::ReplicationConfig {
            target_replicas: self.target_replicas,
            min_trust_class: icn_trust::TrustClass::from_score(self.min_replica_trust),
            health_check_interval_secs: self.health_check_interval_secs,
            stale_threshold_secs: self.stale_threshold_secs,
            unreachable_threshold_secs: self.unreachable_threshold_secs,
        }
    }
}

impl PartitionConfig {
    /// Convert to icn-gossip PartitionConfig
    pub fn to_gossip_config(&self) -> icn_gossip::PartitionConfig {
        use std::time::Duration;
        icn_gossip::PartitionConfig {
            partition_threshold: Duration::from_secs(self.silence_threshold_secs),
            check_interval: Duration::from_secs(self.check_interval_secs),
        }
    }
}
