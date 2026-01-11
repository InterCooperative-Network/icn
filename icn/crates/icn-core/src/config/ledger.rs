//! Ledger configuration
//!
//! Configuration for the mutual credit ledger and exchange rate oracle.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default suspicious rate threshold for unconfigured pairs
pub const DEFAULT_SUSPICIOUS_RATE_THRESHOLD: f64 = 1000.0;

/// Configuration for the ledger subsystem
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerConfig {
    /// Oracle configuration for exchange rates
    #[serde(default)]
    pub oracle: OracleNodeConfig,
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

impl OracleNodeConfig {
    /// Convert to the icn-ledger OracleConfig type
    pub fn to_oracle_config(&self) -> icn_ledger::oracle::OracleConfig {
        let mut config = icn_ledger::oracle::OracleConfig {
            default_ttl_secs: self.default_ttl_secs,
            min_sources_for_consensus: self.min_sources_for_consensus,
            outlier_threshold: self.outlier_threshold,
            staleness_threshold_secs: self.staleness_threshold_secs,
            default_suspicious_rate_threshold: self.default_suspicious_rate_threshold,
            suspicious_rate_thresholds: self.suspicious_rate_thresholds.clone(),
        };

        // Ensure per-pair thresholds are set
        for (pair, threshold) in &self.suspicious_rate_thresholds {
            config
                .suspicious_rate_thresholds
                .insert(pair.clone(), *threshold);
        }

        config
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
        assert!(
            (config.oracle.default_suspicious_rate_threshold - DEFAULT_SUSPICIOUS_RATE_THRESHOLD)
                .abs()
                < f64::EPSILON
        );
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
    fn test_to_oracle_config() {
        let mut node_config = OracleNodeConfig::default();
        node_config.default_suspicious_rate_threshold = 500.0;
        node_config
            .suspicious_rate_thresholds
            .insert("USD:JPY".to_string(), 200.0);

        let oracle_config = node_config.to_oracle_config();

        assert_eq!(oracle_config.default_ttl_secs, 3600);
        assert!((oracle_config.default_suspicious_rate_threshold - 500.0).abs() < f64::EPSILON);
        assert_eq!(
            oracle_config.suspicious_rate_thresholds.get("USD:JPY"),
            Some(&200.0)
        );
    }

    #[test]
    fn test_partial_config() {
        // Test that we can provide only some fields and get defaults for others
        let toml_str = r#"
default_suspicious_rate_threshold = 2000.0

[suspicious_rate_thresholds]
"BTC:USD" = 150000.0
"#;

        let config: OracleNodeConfig = toml::from_str(toml_str).unwrap();

        // Custom values
        assert!((config.default_suspicious_rate_threshold - 2000.0).abs() < f64::EPSILON);
        assert_eq!(
            config.suspicious_rate_thresholds.get("BTC:USD"),
            Some(&150000.0)
        );

        // Default values
        assert_eq!(config.default_ttl_secs, 3600);
        assert_eq!(config.min_sources_for_consensus, 1);
    }
}
