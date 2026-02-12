//! Steward node configuration for SDIS steward network participation

use serde::{Deserialize, Serialize};

/// Steward node configuration for SDIS steward network participation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardNodeConfig {
    /// Enable steward functionality on this node
    #[serde(default)]
    pub enabled: bool,

    /// VUI threshold - minimum stewards required for VUI computation (t-of-n)
    #[serde(default = "default_vui_threshold")]
    pub vui_threshold: u32,

    /// Total number of stewards holding pepper shares (n in t-of-n)
    #[serde(default = "default_vui_total_shares")]
    pub vui_total_shares: u32,

    /// Maximum concurrent enrollment ceremonies
    #[serde(default = "default_max_concurrent_enrollments")]
    pub max_concurrent_enrollments: usize,

    /// Maximum concurrent recovery ceremonies
    #[serde(default = "default_max_concurrent_recoveries")]
    pub max_concurrent_recoveries: usize,

    /// Token validity period in seconds
    #[serde(default = "default_token_validity_secs")]
    pub token_validity_secs: u64,
}

fn default_vui_threshold() -> u32 {
    3
}

fn default_vui_total_shares() -> u32 {
    5
}

fn default_max_concurrent_enrollments() -> usize {
    100
}

fn default_max_concurrent_recoveries() -> usize {
    50
}

fn default_token_validity_secs() -> u64 {
    7 * 24 * 60 * 60 // 7 days
}

impl Default for StewardNodeConfig {
    fn default() -> Self {
        StewardNodeConfig {
            enabled: false,
            vui_threshold: default_vui_threshold(),
            vui_total_shares: default_vui_total_shares(),
            max_concurrent_enrollments: default_max_concurrent_enrollments(),
            max_concurrent_recoveries: default_max_concurrent_recoveries(),
            token_validity_secs: default_token_validity_secs(),
        }
    }
}

impl StewardNodeConfig {
    /// Convert to icn-steward StewardConfig
    pub fn to_steward_config(&self) -> icn_steward::StewardConfig {
        icn_steward::StewardConfig {
            vui_threshold: self.vui_threshold,
            vui_total_shares: self.vui_total_shares,
            max_concurrent_enrollments: self.max_concurrent_enrollments,
            max_concurrent_recoveries: self.max_concurrent_recoveries,
            token_validity_secs: self.token_validity_secs,
            ..Default::default()
        }
    }
}
