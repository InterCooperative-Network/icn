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
//! # Governance policy thresholds (defaults are cooperative-neutral starting points)
//! [compute.policy]
//! min_standing = 0.3           # Min trust standing to submit to commons pool
//! min_trust_score = 0.1        # Min trust score for sybil admission
//! fuel_cost_divisor = 1000     # Fuel units per credit (1 credit per N fuel)
//! credit_ceiling = 5000        # Optional: max credits a submitter may spend (omit to disable)
//! preemptable_priorities = ["ubs_first", "emergency_first"]  # Modes that allow preemption
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

/// Governance policy thresholds for the compute subsystem.
///
/// These values encode cooperative-level governance decisions. Different cooperatives
/// running different risk profiles should be able to set them via config without
/// recompiling. Default values preserve the original hardcoded behavior.
///
/// Config section: `[compute.policy]`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputePolicyConfig {
    /// Minimum trust standing required to submit tasks to the commons pool (0.0–1.0).
    /// Members below this threshold are rejected at the gate.
    /// Original hardcoded value: 0.3
    #[serde(default = "default_min_standing")]
    pub min_standing: f64,

    /// Minimum trust score required for sybil-resistance admission to the commons pool (0.0–1.0).
    /// Nodes below this threshold cannot join as commons participants.
    /// Original hardcoded value: 0.1
    #[serde(default = "default_min_trust_score")]
    pub min_trust_score: f64,

    /// Fuel units per commons credit for cost estimation.
    /// A divisor of 1000 means 1 credit per 1000 fuel units consumed.
    /// Original hardcoded value: 1000
    #[serde(default = "default_fuel_cost_divisor")]
    pub fuel_cost_divisor: u64,

    /// Credit ceiling for cost validation (in credit units).
    /// Tasks whose estimated cost exceeds available credits are rejected.
    /// `None` (default) disables ceiling enforcement.
    #[serde(default)]
    pub credit_ceiling: Option<i64>,

    /// Charter priority modes that allow task preemption.
    ///
    /// When preemption is enabled and the active charter priority is in this list,
    /// higher-priority tasks may interrupt running lower-priority tasks.
    /// Default: `[ubs_first, emergency_first]` (previous hardcoded behavior).
    #[serde(default = "default_preemptable_priorities")]
    pub preemptable_priorities: Vec<icn_compute::CharterPriority>,
}

fn default_min_standing() -> f64 {
    0.3
}

fn default_min_trust_score() -> f64 {
    0.1
}

fn default_fuel_cost_divisor() -> u64 {
    1000
}

fn default_preemptable_priorities() -> Vec<icn_compute::CharterPriority> {
    vec![
        icn_compute::CharterPriority::UbsFirst,
        icn_compute::CharterPriority::EmergencyFirst,
    ]
}

impl Default for ComputePolicyConfig {
    fn default() -> Self {
        Self {
            min_standing: default_min_standing(),
            min_trust_score: default_min_trust_score(),
            fuel_cost_divisor: default_fuel_cost_divisor(),
            credit_ceiling: None,
            preemptable_priorities: default_preemptable_priorities(),
        }
    }
}

/// Distributed compute configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeConfig {
    /// Maximum concurrent tasks this node will execute
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,

    /// Governance policy thresholds for commons pool and cost estimation
    #[serde(default)]
    pub policy: ComputePolicyConfig,

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
            policy: ComputePolicyConfig::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_config_defaults() {
        let config = ComputeConfig::default();
        assert_eq!(config.max_concurrent_tasks, 10);
        assert!(!config.actor_model_enabled);
        assert_eq!(config.max_actors, 100);

        assert_eq!(config.verification.low_value_threshold, 100);
        assert_eq!(config.verification.medium_value_threshold, 1000);
        assert_eq!(config.verification.high_value_threshold, 10000);
        assert_eq!(config.verification.high_value_quorum, 3);
        assert!((config.verification.consensus_threshold - 0.67).abs() < f64::EPSILON);
        assert_eq!(config.verification.collection_window_ms, 30_000);
    }

    #[test]
    fn test_verification_config_defaults() {
        let config = VerificationConfig::default();
        assert_eq!(config.low_value_threshold, 100);
        assert_eq!(config.medium_value_threshold, 1000);
        assert_eq!(config.high_value_threshold, 10000);
        assert_eq!(config.high_value_quorum, 3);
        assert!((config.consensus_threshold - 0.67).abs() < f64::EPSILON);
        assert_eq!(config.collection_window_ms, 30_000);
    }

    #[test]
    fn test_compute_config_toml_serialization() {
        let config = ComputeConfig {
            max_concurrent_tasks: 20,
            policy: ComputePolicyConfig::default(),
            verification: VerificationConfig {
                low_value_threshold: 200,
                medium_value_threshold: 2000,
                high_value_threshold: 20000,
                high_value_quorum: 5,
                consensus_threshold: 0.75,
                collection_window_ms: 60_000,
            },
            actor_model_enabled: true,
            max_actors: 200,
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: ComputeConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(deserialized.max_concurrent_tasks, 20);
        assert!(deserialized.actor_model_enabled);
        assert_eq!(deserialized.max_actors, 200);

        assert_eq!(deserialized.verification.low_value_threshold, 200);
        assert_eq!(deserialized.verification.medium_value_threshold, 2000);
        assert_eq!(deserialized.verification.high_value_threshold, 20000);
        assert_eq!(deserialized.verification.high_value_quorum, 5);
        assert!((deserialized.verification.consensus_threshold - 0.75).abs() < f64::EPSILON);
        assert_eq!(deserialized.verification.collection_window_ms, 60_000);
    }

    #[test]
    fn test_compute_config_toml_deserialization_with_defaults() {
        let toml_str = r#"
max_concurrent_tasks = 15

[verification]
low_value_threshold = 150
"#;

        let config: ComputeConfig = toml::from_str(toml_str).unwrap();

        // Explicitly set values
        assert_eq!(config.max_concurrent_tasks, 15);
        assert_eq!(config.verification.low_value_threshold, 150);

        // Default values
        assert!(!config.actor_model_enabled);
        assert_eq!(config.max_actors, 100);
        assert_eq!(config.verification.medium_value_threshold, 1000);
        assert_eq!(config.verification.high_value_quorum, 3);
    }

    #[test]
    fn test_verification_config_to_compute_config() {
        let config = VerificationConfig {
            low_value_threshold: 250,
            medium_value_threshold: 2500,
            high_value_threshold: 25000,
            high_value_quorum: 7,
            consensus_threshold: 0.8,
            collection_window_ms: 45_000,
        };

        let compute_config = config.to_compute_config();

        assert_eq!(compute_config.low_value_threshold, 250);
        assert_eq!(compute_config.medium_value_threshold, 2500);
        assert_eq!(compute_config.high_value_threshold, 25000);
        assert_eq!(compute_config.high_value_quorum, 7);
        assert!((compute_config.consensus_threshold - 0.8).abs() < f64::EPSILON);
        assert_eq!(compute_config.collection_window_ms, 45_000);
    }

    #[test]
    fn test_verification_config_value_thresholds() {
        let config = VerificationConfig::default();

        // Verify thresholds are in ascending order
        assert!(config.low_value_threshold < config.medium_value_threshold);
        assert!(config.medium_value_threshold < config.high_value_threshold);
    }

    #[test]
    fn test_verification_config_consensus_threshold_valid_range() {
        let config = VerificationConfig::default();

        // Consensus threshold should be between 0 and 1
        assert!(config.consensus_threshold > 0.0);
        assert!(config.consensus_threshold <= 1.0);
    }

    #[test]
    fn test_compute_policy_config_defaults() {
        let config = ComputePolicyConfig::default();
        assert!((config.min_standing - 0.3).abs() < f64::EPSILON);
        assert!((config.min_trust_score - 0.1).abs() < f64::EPSILON);
        assert_eq!(config.fuel_cost_divisor, 1000);
        assert!(config.credit_ceiling.is_none());
        assert_eq!(config.preemptable_priorities.len(), 2);
        assert!(config
            .preemptable_priorities
            .contains(&icn_compute::CharterPriority::UbsFirst));
        assert!(config
            .preemptable_priorities
            .contains(&icn_compute::CharterPriority::EmergencyFirst));
    }

    #[test]
    fn test_compute_policy_config_credit_ceiling_toml() {
        let toml_str = r#"
[policy]
credit_ceiling = 5000
"#;
        let config: ComputeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.policy.credit_ceiling, Some(5000));
        // Other fields use defaults
        assert!((config.policy.min_standing - 0.3).abs() < f64::EPSILON);
        assert_eq!(config.policy.fuel_cost_divisor, 1000);
    }

    #[test]
    fn test_compute_policy_config_preemptable_priorities_toml() {
        let toml_str = r#"
[policy]
preemptable_priorities = ["emergency_first"]
"#;
        let config: ComputeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.policy.preemptable_priorities.len(), 1);
        assert!(config
            .policy
            .preemptable_priorities
            .contains(&icn_compute::CharterPriority::EmergencyFirst));
        assert!(!config
            .policy
            .preemptable_priorities
            .contains(&icn_compute::CharterPriority::UbsFirst));
    }
}
