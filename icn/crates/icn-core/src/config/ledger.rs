//! Ledger configuration
//!
//! Pure serde configuration structs for the mutual credit ledger, exchange rate
//! oracle, and witness signatures. No `icn-ledger` types are imported here.
//! Conversion to domain types happens in `apps/ledger/src/config.rs`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default suspicious rate threshold.
///
/// Must match the default suspicious rate threshold in the ledger oracle.
/// A CI/unit-test drift guard in `icn-ledger-app` (apps/ledger) ensures this.
const DEFAULT_SUSPICIOUS_RATE_THRESHOLD: f64 = 1000.0;

/// Configuration for the ledger subsystem
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerConfig {
    /// Oracle configuration for exchange rates
    #[serde(default)]
    pub oracle: OracleNodeConfig,

    /// Witness signature requirements for material transactions (Issue #676)
    #[serde(default)]
    pub witness: WitnessSettings,
}

/// Configuration for witness signatures on ledger entries
///
/// Witness signatures provide Byzantine fault tolerance by requiring
/// multiple parties to co-sign material transactions.
///
/// # Example Configuration
///
/// ```toml
/// [ledger.witness]
/// # Witness policy: "none", "counterparty", "quorum", "all_parties"
/// default_policy = "counterparty"
///
/// # Only require witnesses for transactions above this value
/// threshold = 1000
///
/// # Timeout for collecting signatures (seconds)
/// collection_timeout_secs = 300
///
/// # For quorum policy only:
/// # quorum_required = 2
/// # quorum_witnesses = ["did:icn:abc123", "did:icn:def456", "did:icn:ghi789"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessSettings {
    /// Default witness policy
    ///
    /// - "none": No witnesses required (default)
    /// - "counterparty": Other party must co-sign
    /// - "quorum": N-of-M designated witnesses
    /// - "all_parties": All transaction participants must sign
    #[serde(default = "default_witness_policy")]
    pub default_policy: String,

    /// Value threshold above which witnesses are required
    ///
    /// If None, the policy applies to all entries regardless of value.
    /// If set, entries below this threshold don't require witnesses.
    #[serde(default)]
    pub threshold: Option<u64>,

    /// Number of signatures required for quorum policy
    ///
    /// Only used when `default_policy` is "quorum".
    #[serde(default)]
    pub quorum_required: Option<u32>,

    /// List of authorized witness DIDs for quorum policy
    ///
    /// Only used when `default_policy` is "quorum".
    #[serde(default)]
    pub quorum_witnesses: Option<Vec<String>>,

    /// Grace period (seconds) to collect witness signatures
    ///
    /// After this time, unsigned entries may be rejected.
    #[serde(default = "default_collection_timeout")]
    pub collection_timeout_secs: u64,

    /// Minimum trust score required for witnesses (optional)
    ///
    /// When set, witnesses must have at least this trust score with
    /// all parties involved in the transaction. See `WitnessConfig` in
    /// icn-ledger for valid trust thresholds.
    #[serde(default)]
    pub min_witness_trust: Option<f64>,
}

fn default_witness_policy() -> String {
    "none".to_string()
}

fn default_collection_timeout() -> u64 {
    300 // 5 minutes
}

impl Default for WitnessSettings {
    fn default() -> Self {
        Self {
            default_policy: default_witness_policy(),
            threshold: None,
            quorum_required: None,
            quorum_witnesses: None,
            collection_timeout_secs: default_collection_timeout(),
            min_witness_trust: None,
        }
    }
}

/// Configuration for the exchange rate oracle
///
/// This configuration controls how exchange rates are validated and
/// what thresholds are used for suspicious rate detection.
///
/// # Example Configuration
///
/// ```toml
/// [ledger.oracle]
/// default_ttl_secs = 3600
/// min_sources_for_consensus = 1
/// outlier_threshold = 0.15
/// staleness_threshold_secs = 86400
/// default_suspicious_rate_threshold = 1000.0
///
/// # Per-currency-pair rate thresholds
/// [ledger.oracle.suspicious_rate_thresholds]
/// "USD:JPY" = 200.0      # JPY pairs have higher absolute rates
/// "BTC:USD" = 100000.0   # Crypto pairs can be very high
/// "hours:USD" = 500.0    # Cooperative time credits
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleNodeConfig {
    /// Default TTL for cached rates (seconds)
    #[serde(default = "default_ttl_secs")]
    pub default_ttl_secs: u64,

    /// Minimum number of sources for consensus
    #[serde(default = "default_min_sources")]
    pub min_sources_for_consensus: usize,

    /// Maximum deviation from median to be considered valid (e.g., 0.15 = 15%)
    #[serde(default = "default_outlier_threshold")]
    pub outlier_threshold: f64,

    /// Staleness threshold (seconds since last update)
    #[serde(default = "default_staleness_threshold")]
    pub staleness_threshold_secs: u64,

    /// Default suspicious rate threshold for pairs not in `suspicious_rate_thresholds`
    ///
    /// Exchange rates exceeding this value trigger additional validation.
    /// Most major currency pairs (EUR/USD, GBP/USD) range from 0.01 to ~150.
    /// Higher values may indicate oracle misconfiguration.
    #[serde(default = "default_suspicious_threshold")]
    pub default_suspicious_rate_threshold: f64,

    /// Per-currency-pair suspicious rate thresholds
    ///
    /// Key format: "FROM:TO" (e.g., "USD:JPY", "BTC:USD").
    /// Use this to configure appropriate thresholds for pairs that
    /// legitimately exceed the default threshold.
    ///
    /// Common examples:
    /// - "USD:JPY" = 200.0 (typically ~100-150)
    /// - "BTC:USD" = 100000.0 (can be very high)
    /// - "hours:USD" = 500.0 (cooperative time credits)
    #[serde(default)]
    pub suspicious_rate_thresholds: HashMap<String, f64>,
}

fn default_ttl_secs() -> u64 {
    3600 // 1 hour
}

fn default_min_sources() -> usize {
    1
}

fn default_outlier_threshold() -> f64 {
    0.15 // 15%
}

fn default_staleness_threshold() -> u64 {
    86400 // 24 hours
}

fn default_suspicious_threshold() -> f64 {
    DEFAULT_SUSPICIOUS_RATE_THRESHOLD
}

impl Default for OracleNodeConfig {
    fn default() -> Self {
        Self {
            default_ttl_secs: default_ttl_secs(),
            min_sources_for_consensus: default_min_sources(),
            outlier_threshold: default_outlier_threshold(),
            staleness_threshold_secs: default_staleness_threshold(),
            default_suspicious_rate_threshold: default_suspicious_threshold(),
            suspicious_rate_thresholds: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ledger_config_defaults() {
        let config = LedgerConfig::default();

        assert_eq!(config.oracle.default_ttl_secs, 3600);
        assert_eq!(config.oracle.min_sources_for_consensus, 1);
        assert!((config.oracle.outlier_threshold - 0.15).abs() < f64::EPSILON);
        assert_eq!(config.oracle.staleness_threshold_secs, 86400);
        assert!((config.oracle.default_suspicious_rate_threshold - 1000.0).abs() < f64::EPSILON);
        assert!(config.oracle.suspicious_rate_thresholds.is_empty());
    }

    #[test]
    fn test_oracle_config_serialization() {
        let toml_str = r#"
default_ttl_secs = 7200
min_sources_for_consensus = 2
outlier_threshold = 0.10
staleness_threshold_secs = 43200
default_suspicious_rate_threshold = 500.0

[suspicious_rate_thresholds]
"USD:JPY" = 200.0
"BTC:USD" = 100000.0
"hours:USD" = 500.0
"#;

        let config: OracleNodeConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(config.default_ttl_secs, 7200);
        assert_eq!(config.min_sources_for_consensus, 2);
        assert!((config.outlier_threshold - 0.10).abs() < f64::EPSILON);
        assert_eq!(config.staleness_threshold_secs, 43200);
        assert!((config.default_suspicious_rate_threshold - 500.0).abs() < f64::EPSILON);
        assert_eq!(
            config.suspicious_rate_thresholds.get("USD:JPY"),
            Some(&200.0)
        );
        assert_eq!(
            config.suspicious_rate_thresholds.get("BTC:USD"),
            Some(&100000.0)
        );
        assert_eq!(
            config.suspicious_rate_thresholds.get("hours:USD"),
            Some(&500.0)
        );
    }

    #[test]
    fn test_partial_config() {
        let toml_str = r#"
default_suspicious_rate_threshold = 2000.0

[suspicious_rate_thresholds]
"BTC:USD" = 150000.0
"#;

        let config: OracleNodeConfig = toml::from_str(toml_str).unwrap();

        assert!((config.default_suspicious_rate_threshold - 2000.0).abs() < f64::EPSILON);
        assert_eq!(
            config.suspicious_rate_thresholds.get("BTC:USD"),
            Some(&150000.0)
        );

        assert_eq!(config.default_ttl_secs, 3600);
        assert_eq!(config.min_sources_for_consensus, 1);
    }

    #[test]
    fn test_witness_settings_defaults() {
        let settings = WitnessSettings::default();

        assert_eq!(settings.default_policy, "none");
        assert!(settings.threshold.is_none());
        assert!(settings.quorum_required.is_none());
        assert!(settings.quorum_witnesses.is_none());
        assert_eq!(settings.collection_timeout_secs, 300);
    }

    #[test]
    fn test_witness_settings_serialization() {
        let toml_str = r#"
default_policy = "counterparty"
threshold = 5000
collection_timeout_secs = 600
"#;

        let settings: WitnessSettings = toml::from_str(toml_str).unwrap();

        assert_eq!(settings.default_policy, "counterparty");
        assert_eq!(settings.threshold, Some(5000));
        assert_eq!(settings.collection_timeout_secs, 600);
    }

    #[test]
    fn test_ledger_config_with_witness_defaults() {
        let config = LedgerConfig::default();

        assert_eq!(config.oracle.default_ttl_secs, 3600);
        assert_eq!(config.witness.default_policy, "none");
        assert!(config.witness.threshold.is_none());
    }
}
