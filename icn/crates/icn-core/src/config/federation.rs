//! Federation configuration for cross-network connectivity

use serde::{Deserialize, Serialize};

use super::network::default_true;

/// Federation configuration for cross-network connectivity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Enable federation features
    #[serde(default)]
    pub enabled: bool,

    /// Cooperative ID (unique identifier for this cooperative in the federation)
    /// If empty, derived from the network_name
    #[serde(default)]
    pub coop_id: String,

    /// Cooperative name (human-readable name for this cooperative)
    /// If empty, uses the network_name
    #[serde(default)]
    pub coop_name: String,

    /// Federation network name (used for network identification)
    /// Nodes with the same network name will preferentially connect
    #[serde(default = "default_network_name")]
    pub network_name: String,

    /// Initial trust score to assign to bootstrap peers
    /// Range: 0.0 to 1.0 (default: 0.3 = trusted enough to relay gossip)
    #[serde(default = "default_bootstrap_trust")]
    pub bootstrap_peer_trust: f64,

    /// Automatically accept federation invites from trusted peers
    /// If false, requires manual approval via CLI
    #[serde(default)]
    pub auto_accept_invites: bool,

    /// Minimum trust score required to accept federation invite
    /// Only used when auto_accept_invites is true
    #[serde(default = "default_min_invite_trust")]
    pub min_invite_trust: f64,

    /// Maximum number of federated networks to join
    #[serde(default = "default_max_federations")]
    pub max_federations: usize,

    /// Connection retry settings
    #[serde(default)]
    pub retry: FederationRetryConfig,

    /// Announce this node's public address to federation peers
    /// Set to false if behind NAT without proper port forwarding
    #[serde(default = "default_true")]
    pub announce_public_addr: bool,
}

/// Retry settings for federation connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationRetryConfig {
    /// Maximum number of connection retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Initial retry delay in seconds
    #[serde(default = "default_initial_delay_secs")]
    pub initial_delay_secs: u64,

    /// Maximum retry delay in seconds (exponential backoff caps here)
    #[serde(default = "default_max_delay_secs")]
    pub max_delay_secs: u64,

    /// Reconnect interval for established peers that disconnect (seconds)
    #[serde(default = "default_reconnect_interval_secs")]
    pub reconnect_interval_secs: u64,
}

fn default_network_name() -> String {
    "icn-mainnet".to_string()
}

fn default_bootstrap_trust() -> f64 {
    0.3
}

fn default_min_invite_trust() -> f64 {
    0.5
}

fn default_max_federations() -> usize {
    10
}

fn default_max_retries() -> u32 {
    5
}

fn default_initial_delay_secs() -> u64 {
    1
}

fn default_max_delay_secs() -> u64 {
    60
}

fn default_reconnect_interval_secs() -> u64 {
    30
}

impl Default for FederationConfig {
    fn default() -> Self {
        FederationConfig {
            enabled: false,
            coop_id: String::new(),
            coop_name: String::new(),
            network_name: default_network_name(),
            bootstrap_peer_trust: default_bootstrap_trust(),
            auto_accept_invites: false,
            min_invite_trust: default_min_invite_trust(),
            max_federations: default_max_federations(),
            retry: FederationRetryConfig::default(),
            announce_public_addr: true,
        }
    }
}

impl Default for FederationRetryConfig {
    fn default() -> Self {
        FederationRetryConfig {
            max_retries: default_max_retries(),
            initial_delay_secs: default_initial_delay_secs(),
            max_delay_secs: default_max_delay_secs(),
            reconnect_interval_secs: default_reconnect_interval_secs(),
        }
    }
}
