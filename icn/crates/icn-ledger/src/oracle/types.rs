//! Core types for the exchange rate oracle
//!
//! Provides types for currency pairs, exchange rates, rate observations,
//! and oracle configuration.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A currency pair for exchange rate lookup
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CurrencyPair {
    /// Source currency (e.g., "hours")
    pub from: String,
    /// Target currency (e.g., "USD")
    pub to: String,
}

impl CurrencyPair {
    /// Create a new currency pair
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }

    /// Storage key format: "from:to"
    pub fn key(&self) -> String {
        format!("{}:{}", self.from, self.to)
    }

    /// Inverse pair for bidirectional lookups
    pub fn inverse(&self) -> Self {
        Self {
            from: self.to.clone(),
            to: self.from.clone(),
        }
    }

    /// Check if this is a same-currency pair (no conversion needed)
    pub fn is_identity(&self) -> bool {
        self.from == self.to
    }
}

impl fmt::Display for CurrencyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.from, self.to)
    }
}

/// A single exchange rate observation from a source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateObservation {
    /// The exchange rate (how many `to` units per 1 `from` unit)
    pub rate: f64,
    /// Source identifier (e.g., "manual", "federation:coop-a")
    pub source: String,
    /// Timestamp when observed (Unix seconds)
    pub observed_at: u64,
    /// Optional confidence score (0.0 - 1.0)
    pub confidence: Option<f64>,
}

impl RateObservation {
    /// Create a new rate observation
    pub fn new(rate: f64, source: impl Into<String>) -> Self {
        Self {
            rate,
            source: source.into(),
            observed_at: crate::current_timestamp_secs(),
            confidence: None,
        }
    }

    /// Create with confidence score
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    /// Create with specific timestamp
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.observed_at = timestamp;
        self
    }
}

/// Aggregated exchange rate with multiple source consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    /// Currency pair
    pub pair: CurrencyPair,
    /// Aggregated rate (median of valid sources)
    pub rate: f64,
    /// Individual observations used
    pub observations: Vec<RateObservation>,
    /// Timestamp of aggregation
    pub aggregated_at: u64,
    /// Time-to-live in seconds
    pub ttl_secs: u64,
    /// Whether this rate is considered stale
    pub is_stale: bool,
}

impl ExchangeRate {
    /// Check if the rate has expired
    pub fn is_expired(&self) -> bool {
        let now = crate::current_timestamp_secs();
        now > self.aggregated_at.saturating_add(self.ttl_secs)
    }

    /// Remaining valid time in seconds
    pub fn remaining_ttl(&self) -> u64 {
        let now = crate::current_timestamp_secs();
        let expires_at = self.aggregated_at.saturating_add(self.ttl_secs);
        expires_at.saturating_sub(now)
    }

    /// Convert an amount from source to target currency
    ///
    /// Returns `None` if the conversion would overflow i64 bounds.
    /// Uses a 1% safety margin to account for floating-point precision issues.
    pub fn convert(&self, amount: i64) -> Option<i64> {
        let result = (amount as f64) * self.rate;
        // Check for non-finite results (NaN, infinity)
        if !result.is_finite() {
            return None;
        }
        // Use 99% of i64 bounds as safety margin for floating-point precision
        const SAFETY_MARGIN: f64 = 0.99;
        let max_safe = i64::MAX as f64 * SAFETY_MARGIN;
        let min_safe = i64::MIN as f64 * SAFETY_MARGIN;
        if result > max_safe || result < min_safe {
            return None;
        }
        Some(result.round() as i64)
    }

    /// Convert an amount, saturating at i64 bounds on overflow
    ///
    /// Returns i64::MAX or i64::MIN for values that overflow or are non-finite.
    pub fn convert_saturating(&self, amount: i64) -> i64 {
        let result = (amount as f64) * self.rate;
        if !result.is_finite() || result > i64::MAX as f64 {
            i64::MAX
        } else if result < i64::MIN as f64 {
            i64::MIN
        } else {
            result.round() as i64
        }
    }

    /// Get the inverse rate
    pub fn inverse_rate(&self) -> f64 {
        if self.rate == 0.0 {
            0.0
        } else {
            1.0 / self.rate
        }
    }

    /// Get the number of sources used
    pub fn source_count(&self) -> usize {
        self.observations.len()
    }

    /// Get the list of source names
    pub fn sources(&self) -> Vec<String> {
        self.observations.iter().map(|o| o.source.clone()).collect()
    }
}

/// Configuration for the oracle manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleConfig {
    /// Default TTL for cached rates (seconds)
    pub default_ttl_secs: u64,
    /// Minimum number of sources for consensus
    pub min_sources_for_consensus: usize,
    /// Maximum deviation from median to be considered valid (e.g., 0.15 = 15%)
    pub outlier_threshold: f64,
    /// Staleness threshold (seconds since last update)
    pub staleness_threshold_secs: u64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            default_ttl_secs: 3600,          // 1 hour
            min_sources_for_consensus: 1,    // At least 1 source
            outlier_threshold: 0.15,         // 15% deviation
            staleness_threshold_secs: 86400, // 24 hours
        }
    }
}

impl OracleConfig {
    /// Create config for testing with shorter timeouts
    #[cfg(test)]
    pub fn for_testing() -> Self {
        Self {
            default_ttl_secs: 60,
            min_sources_for_consensus: 1,
            outlier_threshold: 0.15,
            staleness_threshold_secs: 300,
        }
    }
}

/// Info about a registered rate source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Unique identifier
    pub source_id: String,
    /// Human-readable name
    pub name: String,
    /// Priority (lower = higher priority)
    pub priority: u8,
    /// Whether source is currently healthy
    pub is_healthy: bool,
    /// Last successful update timestamp
    pub last_update: Option<u64>,
}

/// Record for manually set rates (stored in sled)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualRateRecord {
    /// The exchange rate
    pub rate: f64,
    /// Who set this rate (DID)
    pub set_by: String,
    /// When the rate was set
    pub set_at: u64,
    /// Optional note/reason
    pub note: Option<String>,
}

/// Audit trail entry for rate changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateAuditEntry {
    /// Currency pair
    pub pair: CurrencyPair,
    /// Previous rate (None if first entry)
    pub previous_rate: Option<f64>,
    /// New rate
    pub new_rate: f64,
    /// Who made the change (DID)
    pub changed_by: String,
    /// When the change was made (Unix timestamp)
    pub changed_at: u64,
    /// Source of the change (e.g., "manual", "federation:agreement-123")
    pub source: String,
    /// Optional note/reason for the change
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_currency_pair_key() {
        let pair = CurrencyPair::new("hours", "USD");
        assert_eq!(pair.key(), "hours:USD");
        assert_eq!(pair.to_string(), "hours/USD");
    }

    #[test]
    fn test_currency_pair_inverse() {
        let pair = CurrencyPair::new("hours", "USD");
        let inverse = pair.inverse();
        assert_eq!(inverse.from, "USD");
        assert_eq!(inverse.to, "hours");
    }

    #[test]
    fn test_currency_pair_identity() {
        let pair = CurrencyPair::new("USD", "USD");
        assert!(pair.is_identity());

        let pair2 = CurrencyPair::new("hours", "USD");
        assert!(!pair2.is_identity());
    }

    #[test]
    fn test_exchange_rate_convert() {
        let rate = ExchangeRate {
            pair: CurrencyPair::new("hours", "USD"),
            rate: 25.0,
            observations: vec![],
            aggregated_at: 0,
            ttl_secs: 3600,
            is_stale: false,
        };

        assert_eq!(rate.convert(10), Some(250));
        assert_eq!(rate.convert(1), Some(25));
    }

    #[test]
    fn test_exchange_rate_convert_overflow() {
        let rate = ExchangeRate {
            pair: CurrencyPair::new("hours", "USD"),
            rate: 1e20,
            observations: vec![],
            aggregated_at: 0,
            ttl_secs: 3600,
            is_stale: false,
        };

        // Should return None on overflow
        assert_eq!(rate.convert(i64::MAX), None);
        assert_eq!(rate.convert(i64::MAX / 2), None);

        // Saturating version should return i64::MAX
        assert_eq!(rate.convert_saturating(i64::MAX), i64::MAX);
    }

    #[test]
    fn test_exchange_rate_inverse() {
        let rate = ExchangeRate {
            pair: CurrencyPair::new("hours", "USD"),
            rate: 25.0,
            observations: vec![],
            aggregated_at: 0,
            ttl_secs: 3600,
            is_stale: false,
        };

        assert!((rate.inverse_rate() - 0.04).abs() < 0.0001);
    }

    #[test]
    fn test_rate_observation_with_confidence() {
        let obs = RateObservation::new(25.0, "manual").with_confidence(0.95);
        assert_eq!(obs.confidence, Some(0.95));
    }

    #[test]
    fn test_default_config() {
        let config = OracleConfig::default();
        assert_eq!(config.default_ttl_secs, 3600);
        assert_eq!(config.outlier_threshold, 0.15);
    }
}
