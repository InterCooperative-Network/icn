//! Steward Configuration
//!
//! Runtime configuration for steward nodes including thresholds,
//! timing parameters, and operational settings.

use serde::{Deserialize, Serialize};

/// Configuration for a steward node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardConfig {
    /// Minimum threshold for VUI computation (t-of-n)
    pub vui_threshold: u32,

    /// Total number of stewards holding pepper shares
    pub vui_total_shares: u32,

    /// Token validity period in seconds (default: 7 days)
    pub token_validity_secs: u64,

    /// Bloom filter expected items for VUI registry
    pub bloom_expected_items: usize,

    /// Bloom filter false positive rate
    pub bloom_false_positive_rate: f64,

    /// Maximum concurrent enrollment ceremonies
    pub max_concurrent_enrollments: usize,

    /// Maximum concurrent recovery ceremonies
    pub max_concurrent_recoveries: usize,

    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,

    /// Stale heartbeat threshold in seconds (consider offline after this)
    pub stale_heartbeat_secs: u64,

    /// Minimum bond amount in credits
    pub min_bond_amount: i64,

    /// Rate limit: token requests per hour per VUI commitment
    pub rate_limit_per_vui_hour: u32,

    /// Enable VUI registry synchronization
    pub enable_vui_sync: bool,

    /// VUI sync interval in seconds
    pub vui_sync_interval_secs: u64,
}

impl Default for StewardConfig {
    fn default() -> Self {
        Self {
            // 3-of-5 threshold by default
            vui_threshold: 3,
            vui_total_shares: 5,

            // 7 days token validity
            token_validity_secs: 7 * 24 * 60 * 60,

            // Bloom filter sized for 1M entries with 0.01% false positive rate
            bloom_expected_items: 1_000_000,
            bloom_false_positive_rate: 0.0001,

            // Concurrent ceremony limits
            max_concurrent_enrollments: 100,
            max_concurrent_recoveries: 50,

            // Health monitoring
            heartbeat_interval_secs: 30,
            stale_heartbeat_secs: 120,

            // Economic parameters
            min_bond_amount: 1000,

            // Rate limiting (10 per hour per VUI)
            rate_limit_per_vui_hour: 10,

            // VUI sync enabled by default, every 5 minutes
            enable_vui_sync: true,
            vui_sync_interval_secs: 300,
        }
    }
}

impl StewardConfig {
    /// Create config with custom threshold parameters
    pub fn with_threshold(threshold: u32, total_shares: u32) -> Self {
        assert!(
            threshold > 0 && threshold <= total_shares,
            "Invalid threshold: {} of {}",
            threshold,
            total_shares
        );

        Self {
            vui_threshold: threshold,
            vui_total_shares: total_shares,
            ..Default::default()
        }
    }

    /// Check if threshold parameters are valid
    pub fn validate_threshold(&self) -> bool {
        self.vui_threshold > 0
            && self.vui_threshold <= self.vui_total_shares
            && self.vui_total_shares > 0
    }

    /// Check if a quorum is possible with given number of active stewards
    pub fn has_quorum(&self, active_stewards: u32) -> bool {
        active_stewards >= self.vui_threshold
    }

    /// Calculate the maximum number of faulty stewards tolerable
    pub fn fault_tolerance(&self) -> u32 {
        self.vui_total_shares - self.vui_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StewardConfig::default();

        assert_eq!(config.vui_threshold, 3);
        assert_eq!(config.vui_total_shares, 5);
        assert!(config.validate_threshold());
        assert_eq!(config.fault_tolerance(), 2);
    }

    #[test]
    fn test_custom_threshold() {
        let config = StewardConfig::with_threshold(5, 9);

        assert_eq!(config.vui_threshold, 5);
        assert_eq!(config.vui_total_shares, 9);
        assert!(config.validate_threshold());
        assert_eq!(config.fault_tolerance(), 4);
    }

    #[test]
    fn test_quorum_check() {
        let config = StewardConfig::with_threshold(3, 5);

        assert!(!config.has_quorum(2));
        assert!(config.has_quorum(3));
        assert!(config.has_quorum(5));
    }

    #[test]
    #[should_panic(expected = "Invalid threshold")]
    fn test_invalid_threshold_zero() {
        StewardConfig::with_threshold(0, 5);
    }

    #[test]
    #[should_panic(expected = "Invalid threshold")]
    fn test_invalid_threshold_exceeds_total() {
        StewardConfig::with_threshold(6, 5);
    }
}
