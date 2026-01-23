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

use icn_trust::TrustScore;
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
    pub min_replica_trust: TrustScore,

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

fn default_min_replica_trust() -> TrustScore {
    TrustScore::unchecked(0.4) // Partner trust class
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
            min_trust_class: self.min_replica_trust.to_class(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_config_defaults() {
        let config = GossipConfig::default();
        assert_eq!(config.replication.target_replicas, 3);
        assert_eq!(config.replication.min_replica_trust.value(), 0.4);
        assert_eq!(config.replication.health_check_interval_secs, 60);
        assert_eq!(config.replication.stale_threshold_secs, 300);
        assert_eq!(config.replication.unreachable_threshold_secs, 900);

        assert_eq!(config.partition.silence_threshold_secs, 300);
        assert_eq!(config.partition.check_interval_secs, 30);
        assert!(config.partition.auto_heal_enabled);
        assert_eq!(config.partition.heal_interval_secs, 60);
    }

    #[test]
    fn test_replication_config_defaults() {
        let config = ReplicationConfig::default();
        assert_eq!(config.target_replicas, 3);
        assert_eq!(config.min_replica_trust.value(), 0.4);
        assert_eq!(config.health_check_interval_secs, 60);
        assert_eq!(config.stale_threshold_secs, 300);
        assert_eq!(config.unreachable_threshold_secs, 900);
    }

    #[test]
    fn test_partition_config_defaults() {
        let config = PartitionConfig::default();
        assert_eq!(config.silence_threshold_secs, 300);
        assert_eq!(config.check_interval_secs, 30);
        assert!(config.auto_heal_enabled);
        assert_eq!(config.heal_interval_secs, 60);
    }

    #[test]
    fn test_gossip_config_toml_serialization() {
        let config = GossipConfig {
            replication: ReplicationConfig {
                target_replicas: 5,
                min_replica_trust: TrustScore::unchecked(0.5),
                health_check_interval_secs: 120,
                stale_threshold_secs: 600,
                unreachable_threshold_secs: 1800,
            },
            partition: PartitionConfig {
                silence_threshold_secs: 600,
                check_interval_secs: 60,
                auto_heal_enabled: false,
                heal_interval_secs: 120,
            },
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: GossipConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(deserialized.replication.target_replicas, 5);
        assert_eq!(deserialized.replication.min_replica_trust.value(), 0.5);
        assert_eq!(deserialized.replication.health_check_interval_secs, 120);
        assert_eq!(deserialized.replication.stale_threshold_secs, 600);
        assert_eq!(deserialized.replication.unreachable_threshold_secs, 1800);

        assert_eq!(deserialized.partition.silence_threshold_secs, 600);
        assert_eq!(deserialized.partition.check_interval_secs, 60);
        assert!(!deserialized.partition.auto_heal_enabled);
        assert_eq!(deserialized.partition.heal_interval_secs, 120);
    }

    #[test]
    fn test_gossip_config_toml_deserialization_with_defaults() {
        let toml_str = r#"
[replication]
target_replicas = 4

[partition]
silence_threshold_secs = 400
"#;

        let config: GossipConfig = toml::from_str(toml_str).unwrap();

        // Explicitly set values
        assert_eq!(config.replication.target_replicas, 4);
        assert_eq!(config.partition.silence_threshold_secs, 400);

        // Default values
        assert_eq!(config.replication.min_replica_trust.value(), 0.4);
        assert_eq!(config.replication.health_check_interval_secs, 60);
        assert_eq!(config.partition.check_interval_secs, 30);
        assert!(config.partition.auto_heal_enabled);
    }

    #[test]
    fn test_replication_config_to_manager_config() {
        let config = ReplicationConfig {
            target_replicas: 5,
            min_replica_trust: TrustScore::unchecked(0.7),
            health_check_interval_secs: 90,
            stale_threshold_secs: 450,
            unreachable_threshold_secs: 1200,
        };

        let manager_config = config.to_manager_config();

        assert_eq!(manager_config.target_replicas, 5);
        assert_eq!(
            manager_config.min_trust_class,
            icn_trust::TrustClass::Federated
        );
        assert_eq!(manager_config.health_check_interval_secs, 90);
        assert_eq!(manager_config.stale_threshold_secs, 450);
        assert_eq!(manager_config.unreachable_threshold_secs, 1200);
    }

    #[test]
    fn test_partition_config_to_gossip_config() {
        let config = PartitionConfig {
            silence_threshold_secs: 600,
            check_interval_secs: 45,
            auto_heal_enabled: true,
            heal_interval_secs: 90,
        };

        let gossip_config = config.to_gossip_config();

        assert_eq!(gossip_config.partition_threshold.as_secs(), 600);
        assert_eq!(gossip_config.check_interval.as_secs(), 45);
    }
}
