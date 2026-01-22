//! Distributed compute configuration
//!
//! # Example TOML Configuration
//!
//! ```toml
//! [compute]
//! max_concurrent_tasks = 10    # Maximum parallel task execution
//! actor_model_enabled = false  # Enable stateful compute actors
//! max_actors = 100             # Maximum hosted actors
//!
//! # Task result verification settings
//! [compute.verification]
//! low_value_threshold = 100    # Credits below this: single executor
//! medium_value_threshold = 1000 # Credits below this: 2 executors
//! high_value_threshold = 10000 # Credits above this: max quorum
//! high_value_quorum = 3        # Executors for high-value tasks
//! consensus_threshold = 0.67   # 2/3 majority required
//! collection_window_ms = 30000 # Time window to collect results
//! ```

use serde::{Deserialize, Serialize};

/// Distributed compute configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeConfig {
    /// Maximum concurrent tasks this node will execute
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,

    /// Task result verification settings
    #[serde(default)]
    pub verification: VerificationConfig,

    /// Enable actor model execution (stateful compute)
    #[serde(default = "default_false")]
    pub actor_model_enabled: bool,

    /// Maximum number of actors this node will host
    #[serde(default = "default_max_actors")]
    pub max_actors: usize,
}

impl Default for ComputeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: default_max_concurrent_tasks(),
            verification: VerificationConfig::default(),
            actor_model_enabled: default_false(),
            max_actors: default_max_actors(),
        }
    }
}

fn default_max_concurrent_tasks() -> usize {
    10
}

fn default_max_actors() -> usize {
    100
}

fn default_false() -> bool {
    false
}

/// Task result verification configuration
///
/// Controls quorum requirements based on task value to balance
/// security and performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Value threshold below which single executor is used (credits)
    #[serde(default = "default_low_value_threshold")]
    pub low_value_threshold: u64,

    /// Value threshold below which 2 executors are used (credits)
    #[serde(default = "default_medium_value_threshold")]
    pub medium_value_threshold: u64,

    /// Value threshold above which max executors are used (credits)
    #[serde(default = "default_high_value_threshold")]
    pub high_value_threshold: u64,

    /// Number of executors for high-value tasks
    #[serde(default = "default_high_value_quorum")]
    pub high_value_quorum: usize,

    /// Minimum consensus percentage (0.0-1.0) for accepting results
    #[serde(default = "default_consensus_threshold")]
    pub consensus_threshold: f64,

    /// Time window (ms) to collect results before evaluating quorum
    #[serde(default = "default_collection_window_ms")]
    pub collection_window_ms: u64,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            low_value_threshold: default_low_value_threshold(),
            medium_value_threshold: default_medium_value_threshold(),
            high_value_threshold: default_high_value_threshold(),
            high_value_quorum: default_high_value_quorum(),
            consensus_threshold: default_consensus_threshold(),
            collection_window_ms: default_collection_window_ms(),
        }
    }
}

fn default_low_value_threshold() -> u64 {
    100
}

fn default_medium_value_threshold() -> u64 {
    1000
}

fn default_high_value_threshold() -> u64 {
    10000
}

fn default_high_value_quorum() -> usize {
    3
}

fn default_consensus_threshold() -> f64 {
    0.67 // 2/3 majority
}

fn default_collection_window_ms() -> u64 {
    30_000 // 30 seconds
}

impl VerificationConfig {
    /// Convert to icn-compute VerificationConfig
    pub fn to_compute_config(&self) -> icn_compute::VerificationConfig {
        icn_compute::VerificationConfig {
            low_value_threshold: self.low_value_threshold,
            medium_value_threshold: self.medium_value_threshold,
            high_value_threshold: self.high_value_threshold,
            high_value_quorum: self.high_value_quorum,
            consensus_threshold: self.consensus_threshold,
            collection_window_ms: self.collection_window_ms,
        }
    }
}
