//! Security subsystem configuration
//!
//! # Example TOML Configuration
//!
//! ```toml
//! [security.reputation]
//! penalty_rate = 0.05            # Fraction of score lost per severity point
//! critical_severity = 10         # Severity weight for critical violations
//! major_severity = 5             # Severity weight for major violations
//! minor_severity = 1             # Severity weight for minor violations
//! storage_invalid_severity = 8   # Severity for invalid Merkle/signature proofs
//! storage_suspicious_severity = 5 # Severity for data/challenge mismatches
//! storage_missing_severity = 3   # Severity for content-not-found responses
//! storage_timeout_severity = 1   # Severity for timeouts/expired challenges
//! max_violations_per_hour = 10   # Violations per peer per hour before auto-quarantine
//! violation_retention_secs = 604800  # How long to keep violation history (7 days)
//! ```

use serde::{Deserialize, Serialize};

/// Security subsystem configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    /// Reputation scoring policy configuration
    #[serde(default)]
    pub reputation: ReputationPolicyConfig,
}

/// Reputation scoring policy configuration
///
/// Controls how severely violations affect peer reputation scores
/// and which severity weights map to each violation category.
/// All defaults match the previously-hardcoded values for zero-impact upgrade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationPolicyConfig {
    /// Fraction of reputation score lost per severity point (0.0–1.0).
    /// Default: 0.05 — a critical violation (severity 10) costs 50% score.
    /// Governance decision: adjust for cooperative risk tolerance.
    #[serde(default = "default_penalty_rate")]
    pub penalty_rate: f64,

    /// Severity weight for critical violations
    /// (ConflictingLedgerEntries, ConflictingSignedStatements, ReplayAttack).
    #[serde(default = "default_critical_severity")]
    pub critical_severity: u32,

    /// Severity weight for major violations
    /// (FailedComputeVerification, InvalidSignature).
    #[serde(default = "default_major_severity")]
    pub major_severity: u32,

    /// Severity weight for minor violations
    /// (ExcessiveResourceUse, TrustGraphSpam).
    #[serde(default = "default_minor_severity")]
    pub minor_severity: u32,

    /// Severity weight for storage violations with invalid proofs
    /// (InvalidMerkleProof, InvalidSignature on challenge response).
    #[serde(default = "default_storage_invalid_severity")]
    pub storage_invalid_severity: u32,

    /// Severity weight for suspicious storage responses
    /// (DataMismatch, ChallengeMismatch).
    #[serde(default = "default_storage_suspicious_severity")]
    pub storage_suspicious_severity: u32,

    /// Severity weight for missing content responses.
    #[serde(default = "default_storage_missing_severity")]
    pub storage_missing_severity: u32,

    /// Severity weight for storage timeouts and expired challenges.
    #[serde(default = "default_storage_timeout_severity")]
    pub storage_timeout_severity: u32,

    /// Violations per peer per hour before auto-quarantine.
    /// Governance decision: lower = stricter, higher = more lenient.
    #[serde(default = "default_max_violations_per_hour")]
    pub max_violations_per_hour: usize,

    /// How long to keep violation history in seconds.
    /// Default: 604800 (7 days). Longer retention uses more memory.
    #[serde(default = "default_violation_retention_secs")]
    pub violation_retention_secs: u64,
}

impl Default for ReputationPolicyConfig {
    fn default() -> Self {
        Self {
            penalty_rate: default_penalty_rate(),
            critical_severity: default_critical_severity(),
            major_severity: default_major_severity(),
            minor_severity: default_minor_severity(),
            storage_invalid_severity: default_storage_invalid_severity(),
            storage_suspicious_severity: default_storage_suspicious_severity(),
            storage_missing_severity: default_storage_missing_severity(),
            storage_timeout_severity: default_storage_timeout_severity(),
            max_violations_per_hour: default_max_violations_per_hour(),
            violation_retention_secs: default_violation_retention_secs(),
        }
    }
}

fn default_penalty_rate() -> f64 {
    0.05
}

fn default_critical_severity() -> u32 {
    10
}

fn default_major_severity() -> u32 {
    5
}

fn default_minor_severity() -> u32 {
    1
}

fn default_storage_invalid_severity() -> u32 {
    8
}

fn default_storage_suspicious_severity() -> u32 {
    5
}

fn default_storage_missing_severity() -> u32 {
    3
}

fn default_storage_timeout_severity() -> u32 {
    1
}

fn default_max_violations_per_hour() -> usize {
    10
}

fn default_violation_retention_secs() -> u64 {
    7 * 24 * 3600 // 7 days
}

impl ReputationPolicyConfig {
    /// Convert to `icn_security::SeverityWeights` for use in `MisbehaviorDetector`.
    pub fn to_severity_weights(&self) -> icn_security::SeverityWeights {
        icn_security::SeverityWeights {
            critical: self.critical_severity,
            major: self.major_severity,
            minor: self.minor_severity,
            storage_invalid: self.storage_invalid_severity,
            storage_suspicious: self.storage_suspicious_severity,
            storage_missing: self.storage_missing_severity,
            storage_timeout: self.storage_timeout_severity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reputation_policy_config_defaults() {
        let config = ReputationPolicyConfig::default();
        assert!((config.penalty_rate - 0.05).abs() < f64::EPSILON);
        assert_eq!(config.critical_severity, 10);
        assert_eq!(config.major_severity, 5);
        assert_eq!(config.minor_severity, 1);
        assert_eq!(config.storage_invalid_severity, 8);
        assert_eq!(config.storage_suspicious_severity, 5);
        assert_eq!(config.storage_missing_severity, 3);
        assert_eq!(config.storage_timeout_severity, 1);
        assert_eq!(config.max_violations_per_hour, 10);
        assert_eq!(config.violation_retention_secs, 7 * 24 * 3600);
    }

    #[test]
    fn test_security_config_defaults() {
        let config = SecurityConfig::default();
        let rep = config.reputation;
        assert!((rep.penalty_rate - 0.05).abs() < f64::EPSILON);
        assert_eq!(rep.critical_severity, 10);
    }

    #[test]
    fn test_security_config_toml_serialization() {
        let config = SecurityConfig {
            reputation: ReputationPolicyConfig {
                penalty_rate: 0.10,
                critical_severity: 20,
                major_severity: 10,
                minor_severity: 2,
                storage_invalid_severity: 16,
                storage_suspicious_severity: 8,
                storage_missing_severity: 4,
                storage_timeout_severity: 2,
                ..Default::default()
            },
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: SecurityConfig = toml::from_str(&toml_str).unwrap();

        assert!((deserialized.reputation.penalty_rate - 0.10).abs() < f64::EPSILON);
        assert_eq!(deserialized.reputation.critical_severity, 20);
        assert_eq!(deserialized.reputation.major_severity, 10);
        assert_eq!(deserialized.reputation.minor_severity, 2);
    }

    #[test]
    fn test_security_config_partial_toml_with_defaults() {
        let toml_str = r#"
[reputation]
penalty_rate = 0.10
"#;
        let config: SecurityConfig = toml::from_str(toml_str).unwrap();
        assert!((config.reputation.penalty_rate - 0.10).abs() < f64::EPSILON);
        // Remaining fields use defaults
        assert_eq!(config.reputation.critical_severity, 10);
        assert_eq!(config.reputation.minor_severity, 1);
        assert_eq!(config.reputation.max_violations_per_hour, 10);
        assert_eq!(config.reputation.violation_retention_secs, 7 * 24 * 3600);
    }

    #[test]
    fn test_security_config_threshold_fields_roundtrip() {
        let toml_str = r#"
[reputation]
max_violations_per_hour = 5
violation_retention_secs = 86400
"#;
        let config: SecurityConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.reputation.max_violations_per_hour, 5);
        assert_eq!(config.reputation.violation_retention_secs, 86400);
    }

    #[test]
    fn test_to_severity_weights_matches_defaults() {
        let config = ReputationPolicyConfig::default();
        let weights = config.to_severity_weights();
        assert_eq!(weights.critical, 10);
        assert_eq!(weights.major, 5);
        assert_eq!(weights.minor, 1);
        assert_eq!(weights.storage_invalid, 8);
        assert_eq!(weights.storage_suspicious, 5);
        assert_eq!(weights.storage_missing, 3);
        assert_eq!(weights.storage_timeout, 1);
    }

    #[test]
    fn test_to_severity_weights_custom_values() {
        let config = ReputationPolicyConfig {
            penalty_rate: 0.10,
            critical_severity: 20,
            major_severity: 10,
            minor_severity: 2,
            storage_invalid_severity: 16,
            storage_suspicious_severity: 8,
            storage_missing_severity: 4,
            storage_timeout_severity: 2,
            ..Default::default()
        };
        let weights = config.to_severity_weights();
        assert_eq!(weights.critical, 20);
        assert_eq!(weights.major, 10);
        assert_eq!(weights.minor, 2);
    }
}
