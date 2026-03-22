//! Observability subsystem configuration
//!
//! # Example TOML Configuration
//!
//! ```toml
//! [obs.contribution]
//! min_trust_to_attest = 0.3          # Minimum trust score required to attest
//! min_membership_age_secs = 7776000  # Minimum membership age (90 days)
//! max_attestations_per_period = 10   # Budget per attester per scoring period
//! org_attestation_threshold = 500    # Credit units above which org attestation required
//! contribution_threshold = 1.0       # Weighted score required to verify a claim
//! ```

use serde::{Deserialize, Serialize};

/// Observability subsystem configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObsConfig {
    /// Contribution attestation threshold configuration
    #[serde(default)]
    pub contribution: ContributionAttestationConfig,
}

/// Contribution attestation threshold configuration.
///
/// Controls the eligibility and scoring rules for the contribution attestation
/// system in `icn-obs`. These values were previously hardcoded as `pub const`
/// in `icn_obs::attestation`; they are now config-driven for cooperative
/// governance flexibility.
///
/// All defaults match the previously-hardcoded values for zero-impact upgrade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionAttestationConfig {
    /// Minimum trust score required to attest a contribution claim (0.0–1.0).
    /// Default: 0.3 — low bar that any established member can clear.
    #[serde(default = "default_min_trust_to_attest")]
    pub min_trust_to_attest: f64,

    /// Minimum membership age in seconds before a member may attest.
    /// Default: 7776000 (90 days).
    #[serde(default = "default_min_membership_age_secs")]
    pub min_membership_age_secs: u64,

    /// Maximum attestations per attester per scoring period.
    /// Default: 10. Prevents budget exhaustion and sybil rings.
    #[serde(default = "default_max_attestations_per_period")]
    pub max_attestations_per_period: u32,

    /// Credit unit threshold above which org-level attestation is required.
    /// Default: 500. High-value claims need an organizational co-signer.
    #[serde(default = "default_org_attestation_threshold")]
    pub org_attestation_threshold: u64,

    /// Minimum weighted score (sum of attester trust scores) to verify a claim.
    /// Default: 1.0 — requires at least one trusted attester with full score.
    #[serde(default = "default_contribution_threshold")]
    pub contribution_threshold: f64,
}

impl Default for ContributionAttestationConfig {
    fn default() -> Self {
        Self {
            min_trust_to_attest: default_min_trust_to_attest(),
            min_membership_age_secs: default_min_membership_age_secs(),
            max_attestations_per_period: default_max_attestations_per_period(),
            org_attestation_threshold: default_org_attestation_threshold(),
            contribution_threshold: default_contribution_threshold(),
        }
    }
}

fn default_min_trust_to_attest() -> f64 {
    0.3
}

fn default_min_membership_age_secs() -> u64 {
    90 * 24 * 60 * 60 // 90 days
}

fn default_max_attestations_per_period() -> u32 {
    10
}

fn default_org_attestation_threshold() -> u64 {
    500
}

fn default_contribution_threshold() -> f64 {
    1.0
}

impl ContributionAttestationConfig {
    /// Convert to `icn_obs::attestation::AttestationThresholds` for runtime use.
    pub fn to_attestation_thresholds(&self) -> icn_obs::attestation::AttestationThresholds {
        icn_obs::attestation::AttestationThresholds {
            min_trust_to_attest: self.min_trust_to_attest,
            min_membership_age_secs: self.min_membership_age_secs,
            max_attestations_per_period: self.max_attestations_per_period,
            org_attestation_threshold: self.org_attestation_threshold,
            contribution_threshold: self.contribution_threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contribution_attestation_config_defaults() {
        let config = ContributionAttestationConfig::default();
        assert!((config.min_trust_to_attest - 0.3).abs() < f64::EPSILON);
        assert_eq!(config.min_membership_age_secs, 90 * 24 * 60 * 60);
        assert_eq!(config.max_attestations_per_period, 10);
        assert_eq!(config.org_attestation_threshold, 500);
        assert!((config.contribution_threshold - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_obs_config_defaults() {
        let config = ObsConfig::default();
        assert!((config.contribution.min_trust_to_attest - 0.3).abs() < f64::EPSILON);
        assert_eq!(config.contribution.max_attestations_per_period, 10);
    }

    #[test]
    fn test_contribution_config_toml_roundtrip() {
        let toml_str = r#"
[contribution]
min_trust_to_attest = 0.5
max_attestations_per_period = 5
"#;
        let config: ObsConfig = toml::from_str(toml_str).unwrap();
        assert!((config.contribution.min_trust_to_attest - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.contribution.max_attestations_per_period, 5);
        // Unset fields use defaults
        assert_eq!(config.contribution.org_attestation_threshold, 500);
        assert!((config.contribution.contribution_threshold - 1.0).abs() < f64::EPSILON);
    }
}
