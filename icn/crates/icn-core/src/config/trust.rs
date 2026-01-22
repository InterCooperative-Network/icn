//! Trust graph configuration

use serde::{Deserialize, Serialize};

/// Trust graph configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            attestation: AttestationConfig::default(),
            propagation: PropagationConfig::default(),
            sybil_resistance: SybilResistanceConfig::default(),
        }
    }
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
