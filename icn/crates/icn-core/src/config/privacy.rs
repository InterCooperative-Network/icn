//! Privacy configuration for metadata protection and onion routing

use serde::{Deserialize, Serialize};

/// Privacy configuration for metadata protection and onion routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// Enable privacy features (onion routing, topic encryption)
    #[serde(default)]
    pub enabled: bool,

    /// Enable onion routing for anonymous message delivery
    /// Messages will be routed through multiple relay nodes to hide sender/receiver
    #[serde(default)]
    pub onion_routing_enabled: bool,

    /// Number of relay hops for onion routing (2-5 recommended)
    /// More hops = more privacy but higher latency
    #[serde(default = "default_onion_hops")]
    pub onion_hops: usize,

    /// Minimum trust score for relay selection (0.0 to 1.0)
    /// Higher values = fewer relays but more trusted
    #[serde(default = "default_min_relay_trust")]
    pub min_relay_trust: f64,

    /// Enable topic name encryption
    /// Topic names will be encrypted so observers can't see what topics are subscribed
    #[serde(default)]
    pub topic_encryption_enabled: bool,

    /// Enable traffic obfuscation (padding, delays, cover traffic)
    #[serde(default)]
    pub traffic_obfuscation_enabled: bool,

    /// Message padding target size (bytes, 0 = disabled)
    /// Messages will be padded to hide their actual size
    #[serde(default = "default_padding_target")]
    pub padding_target: usize,

    /// Maximum random delay for messages (milliseconds, 0 = disabled)
    /// Adds random delay to hide timing patterns
    #[serde(default)]
    pub max_delay_ms: u64,

    /// Cover traffic rate (messages per minute, 0 = disabled)
    /// Generates decoy traffic to hide real activity patterns
    #[serde(default)]
    pub cover_traffic_rate: u32,
}

fn default_onion_hops() -> usize {
    2 // Default to 2 relay hops
}

fn default_min_relay_trust() -> f64 {
    0.3 // Minimum trust score for relay selection
}

fn default_padding_target() -> usize {
    0 // Disabled by default
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        PrivacyConfig {
            enabled: false,
            onion_routing_enabled: false,
            onion_hops: default_onion_hops(),
            min_relay_trust: default_min_relay_trust(),
            topic_encryption_enabled: false,
            traffic_obfuscation_enabled: false,
            padding_target: default_padding_target(),
            max_delay_ms: 0,
            cover_traffic_rate: 0,
        }
    }
}
