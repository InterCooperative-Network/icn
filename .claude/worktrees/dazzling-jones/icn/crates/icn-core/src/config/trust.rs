//! Trust graph configuration
//!
//! # Example TOML Configuration
//!
//! ```toml
//! [trust]
//! # Attestation settings
//! [trust.attestation]
//! min_attester_trust = 0.3      # Minimum trust to create attestations
//! max_attestations_per_day = 10 # Rate limit per attester
//! evidence_required = true      # Require proof for attestations
//! min_evidence_score = 0.5      # Minimum evidence quality
//!
//! # Trust propagation settings
//! [trust.propagation]
//! max_path_length = 3           # Maximum hops for transitive trust
//! decay_factor = 0.8            # 20% decay per hop
//! min_edge_trust = 0.1          # Minimum edge weight to include
//! cache_enabled = true          # Enable trust caching
//! cache_ttl_secs = 300          # Cache lifetime (5 minutes)
//!
//! # Sybil resistance settings
//! [trust.sybil_resistance]
//! enabled = true                # Enable sybil detection
//! max_trust_concentration = 0.3 # Max trust from single attester
//! sample_size = 100             # Network sampling size
//! min_diversity_ratio = 0.2     # Required attester diversity
//! ```

use serde::{Deserialize, Serialize};

/// Trust graph configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustConfig {
    /// Trust attestation settings
    #[serde(default)]
    pub attestation: AttestationConfig,

    /// Trust propagation settings
    #[serde(default)]
    pub propagation: PropagationConfig,

    /// Sybil resistance settings
    #[serde(default)]
    pub sybil_resistance: SybilResistanceConfig,
}

/// Trust attestation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationConfig {
    /// Minimum trust score required to create attestations (0.0-1.0)
    /// Default: 0.3 (basic trust relationship)
    #[serde(default = "default_min_attester_trust")]
    pub min_attester_trust: f64,

    /// Maximum attestations per day per attester
    #[serde(default = "default_max_attestations_per_day")]
    pub max_attestations_per_day: usize,

    /// Enable evidence-based attestations (requires proof)
    #[serde(default = "default_true")]
    pub evidence_required: bool,

    /// Minimum evidence score for attestations (0.0-1.0)
    #[serde(default = "default_min_evidence_score")]
    pub min_evidence_score: f64,
}

impl Default for AttestationConfig {
    fn default() -> Self {
        Self {
            min_attester_trust: default_min_attester_trust(),
            max_attestations_per_day: default_max_attestations_per_day(),
            evidence_required: default_true(),
            min_evidence_score: default_min_evidence_score(),
        }
    }
}

fn default_min_attester_trust() -> f64 {
    0.3 // Require moderate trust to attest
}

fn default_max_attestations_per_day() -> usize {
    10 // Prevent spam while allowing legitimate use
}

fn default_min_evidence_score() -> f64 {
    0.5 // Require reasonable evidence quality
}

/// Trust propagation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationConfig {
    /// Maximum path length for transitive trust (hops)
    #[serde(default = "default_max_path_length")]
    pub max_path_length: usize,

    /// Decay factor per hop (0.0-1.0)
    /// Trust score is multiplied by this factor for each hop
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f64,

    /// Minimum edge trust to include in propagation (0.0-1.0)
    #[serde(default = "default_min_edge_trust")]
    pub min_edge_trust: f64,

    /// Enable trust caching for performance
    #[serde(default = "default_true")]
    pub cache_enabled: bool,

    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,
}

impl Default for PropagationConfig {
    fn default() -> Self {
        Self {
            max_path_length: default_max_path_length(),
            decay_factor: default_decay_factor(),
            min_edge_trust: default_min_edge_trust(),
            cache_enabled: default_true(),
            cache_ttl_secs: default_cache_ttl_secs(),
        }
    }
}

fn default_max_path_length() -> usize {
    3 // Balance reach and computation cost
}

fn default_decay_factor() -> f64 {
    0.8 // 20% decay per hop
}

fn default_min_edge_trust() -> f64 {
    0.1 // Include weak edges but not zero trust
}

fn default_cache_ttl_secs() -> u64 {
    300 // 5 minutes
}

/// Sybil resistance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SybilResistanceConfig {
    /// Enable sybil detection
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum trust concentration per attester (0.0-1.0)
    /// If an attester vouches for entities with combined trust exceeding this,
    /// additional attestations have reduced weight
    #[serde(default = "default_max_trust_concentration")]
    pub max_trust_concentration: f64,

    /// Network sampling size for sybil detection
    #[serde(default = "default_sample_size")]
    pub sample_size: usize,

    /// Minimum network diversity ratio (0.0-1.0)
    /// Entities with low diversity (few unique attesters) have reduced trust
    #[serde(default = "default_min_diversity_ratio")]
    pub min_diversity_ratio: f64,
}

impl Default for SybilResistanceConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            max_trust_concentration: default_max_trust_concentration(),
            sample_size: default_sample_size(),
            min_diversity_ratio: default_min_diversity_ratio(),
        }
    }
}

fn default_max_trust_concentration() -> f64 {
    0.3 // Prevent single attester from dominating
}

fn default_sample_size() -> usize {
    100 // Sample size for network analysis
}

fn default_min_diversity_ratio() -> f64 {
    0.2 // Require 20% unique attesters
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_config_defaults() {
        let config = TrustConfig::default();

        // Attestation defaults
        assert_eq!(config.attestation.min_attester_trust, 0.3);
        assert_eq!(config.attestation.max_attestations_per_day, 10);
        assert!(config.attestation.evidence_required);
        assert_eq!(config.attestation.min_evidence_score, 0.5);

        // Propagation defaults
        assert_eq!(config.propagation.max_path_length, 3);
        assert_eq!(config.propagation.decay_factor, 0.8);
        assert_eq!(config.propagation.min_edge_trust, 0.1);
        assert!(config.propagation.cache_enabled);
        assert_eq!(config.propagation.cache_ttl_secs, 300);

        // Sybil resistance defaults
        assert!(config.sybil_resistance.enabled);
        assert_eq!(config.sybil_resistance.max_trust_concentration, 0.3);
        assert_eq!(config.sybil_resistance.sample_size, 100);
        assert_eq!(config.sybil_resistance.min_diversity_ratio, 0.2);
    }

    #[test]
    fn test_attestation_config_defaults() {
        let config = AttestationConfig::default();
        assert_eq!(config.min_attester_trust, 0.3);
        assert_eq!(config.max_attestations_per_day, 10);
        assert!(config.evidence_required);
        assert_eq!(config.min_evidence_score, 0.5);
    }

    #[test]
    fn test_propagation_config_defaults() {
        let config = PropagationConfig::default();
        assert_eq!(config.max_path_length, 3);
        assert_eq!(config.decay_factor, 0.8);
        assert_eq!(config.min_edge_trust, 0.1);
        assert!(config.cache_enabled);
        assert_eq!(config.cache_ttl_secs, 300);
    }

    #[test]
    fn test_sybil_resistance_config_defaults() {
        let config = SybilResistanceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_trust_concentration, 0.3);
        assert_eq!(config.sample_size, 100);
        assert_eq!(config.min_diversity_ratio, 0.2);
    }

    #[test]
    fn test_trust_config_toml_serialization() {
        let config = TrustConfig {
            attestation: AttestationConfig {
                min_attester_trust: 0.5,
                max_attestations_per_day: 20,
                evidence_required: false,
                min_evidence_score: 0.6,
            },
            propagation: PropagationConfig {
                max_path_length: 5,
                decay_factor: 0.9,
                min_edge_trust: 0.2,
                cache_enabled: false,
                cache_ttl_secs: 600,
            },
            sybil_resistance: SybilResistanceConfig {
                enabled: false,
                max_trust_concentration: 0.4,
                sample_size: 200,
                min_diversity_ratio: 0.3,
            },
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: TrustConfig = toml::from_str(&toml_str).unwrap();

        // Attestation
        assert_eq!(deserialized.attestation.min_attester_trust, 0.5);
        assert_eq!(deserialized.attestation.max_attestations_per_day, 20);
        assert!(!deserialized.attestation.evidence_required);
        assert_eq!(deserialized.attestation.min_evidence_score, 0.6);

        // Propagation
        assert_eq!(deserialized.propagation.max_path_length, 5);
        assert_eq!(deserialized.propagation.decay_factor, 0.9);
        assert_eq!(deserialized.propagation.min_edge_trust, 0.2);
        assert!(!deserialized.propagation.cache_enabled);
        assert_eq!(deserialized.propagation.cache_ttl_secs, 600);

        // Sybil resistance
        assert!(!deserialized.sybil_resistance.enabled);
        assert_eq!(deserialized.sybil_resistance.max_trust_concentration, 0.4);
        assert_eq!(deserialized.sybil_resistance.sample_size, 200);
        assert_eq!(deserialized.sybil_resistance.min_diversity_ratio, 0.3);
    }

    #[test]
    fn test_trust_config_toml_deserialization_with_defaults() {
        let toml_str = r#"
[attestation]
min_attester_trust = 0.4

[propagation]
max_path_length = 4

[sybil_resistance]
sample_size = 150
"#;

        let config: TrustConfig = toml::from_str(toml_str).unwrap();

        // Explicitly set values
        assert_eq!(config.attestation.min_attester_trust, 0.4);
        assert_eq!(config.propagation.max_path_length, 4);
        assert_eq!(config.sybil_resistance.sample_size, 150);

        // Default values
        assert_eq!(config.attestation.max_attestations_per_day, 10);
        assert_eq!(config.propagation.decay_factor, 0.8);
        assert!(config.sybil_resistance.enabled);
    }

    #[test]
    fn test_attestation_config_trust_scores_valid_range() {
        let config = AttestationConfig::default();

        // Trust scores should be between 0 and 1
        assert!(config.min_attester_trust >= 0.0);
        assert!(config.min_attester_trust <= 1.0);
        assert!(config.min_evidence_score >= 0.0);
        assert!(config.min_evidence_score <= 1.0);
    }

    #[test]
    fn test_propagation_config_trust_scores_valid_range() {
        let config = PropagationConfig::default();

        // Trust scores and decay factor should be between 0 and 1
        assert!(config.decay_factor >= 0.0);
        assert!(config.decay_factor <= 1.0);
        assert!(config.min_edge_trust >= 0.0);
        assert!(config.min_edge_trust <= 1.0);
    }

    #[test]
    fn test_sybil_resistance_config_trust_scores_valid_range() {
        let config = SybilResistanceConfig::default();

        // Trust scores should be between 0 and 1
        assert!(config.max_trust_concentration >= 0.0);
        assert!(config.max_trust_concentration <= 1.0);
        assert!(config.min_diversity_ratio >= 0.0);
        assert!(config.min_diversity_ratio <= 1.0);
    }

    #[test]
    fn test_propagation_config_decay_factor_reasonable() {
        let config = PropagationConfig::default();

        // Decay factor of 0.8 means 20% decay per hop, which is reasonable
        // Should be > 0 (otherwise trust would disappear immediately)
        // Should be < 1 (otherwise trust wouldn't decay)
        assert!(config.decay_factor > 0.0);
        assert!(config.decay_factor < 1.0);
    }

    #[test]
    fn test_attestation_config_daily_limit_reasonable() {
        let config = AttestationConfig::default();

        // Daily limit should be positive and reasonable
        assert!(config.max_attestations_per_day > 0);
        assert!(config.max_attestations_per_day <= 1000); // Sanity check
    }

    #[test]
    fn test_sybil_resistance_sample_size_reasonable() {
        let config = SybilResistanceConfig::default();

        // Sample size should be large enough for statistical significance
        // but not so large as to be computationally expensive
        assert!(config.sample_size >= 10);
        assert!(config.sample_size <= 10000);
    }
}
