//! Configuration management for ICNd

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// ICNd configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Data directory for ICN state
    pub data_dir: PathBuf,

    /// Network configuration
    pub network: NetworkConfig,

    /// Observability configuration
    pub observability: ObservabilityConfig,

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limiting: RateLimitingConfig,

    /// Topology configuration for regional/cluster-based networking
    #[serde(default)]
    pub topology: TopologyConfig,

    /// Gateway API configuration
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Federation configuration for cross-network connectivity
    #[serde(default)]
    pub federation: FederationConfig,

    /// Privacy configuration for metadata protection and onion routing
    #[serde(default)]
    pub privacy: PrivacyConfig,

    /// Cooperative configuration for cooperative-specific settings
    #[serde(default)]
    pub cooperative: CooperativeConfig,

    /// Steward configuration for SDIS steward network participation
    #[serde(default)]
    pub steward: StewardNodeConfig,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Enable mDNS discovery
    pub mdns_enabled: bool,

    /// QUIC listen address (format: "IP:PORT")
    pub listen_addr: String,

    /// RPC (JSON-RPC over HTTP) listen port
    #[serde(default = "default_rpc_port")]
    pub rpc_port: u16,

    /// Bootstrap rendezvous endpoints
    pub bootstrap_peers: Vec<String>,

    /// STUN servers for NAT traversal (format: "IP:PORT")
    /// Multiple servers enable majority vote consensus for public endpoint discovery
    #[serde(default = "default_stun_servers")]
    pub stun_servers: Vec<String>,
}

fn default_rpc_port() -> u16 {
    5601
}

fn default_stun_servers() -> Vec<String> {
    // Use Google's public STUN servers by default
    // Multiple servers enable majority vote for robust NAT discovery
    vec![
        "stun.l.google.com:19302".to_string(),
        "stun1.l.google.com:19302".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Metrics server port
    pub metrics_port: u16,

    /// Health check port
    pub health_port: u16,

    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,
}

/// Rate limiting configuration for trust-gated message throttling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Enable trust-gated rate limiting (requires trust graph)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Refill interval in milliseconds (how often to add tokens)
    #[serde(default = "default_refill_interval_ms")]
    pub refill_interval_ms: u64,

    /// Rate limits for isolated peers (trust score < 0.1)
    #[serde(default = "default_isolated")]
    pub isolated: TrustClassRateLimitConfig,

    /// Rate limits for known peers (trust score 0.1-0.4)
    #[serde(default = "default_known")]
    pub known: TrustClassRateLimitConfig,

    /// Rate limits for partner peers (trust score 0.4-0.7)
    #[serde(default = "default_partner")]
    pub partner: TrustClassRateLimitConfig,

    /// Rate limits for federated peers (trust score 0.7+)
    #[serde(default = "default_federated")]
    pub federated: TrustClassRateLimitConfig,

    /// Fallback rate limit when trust graph is unavailable
    #[serde(default = "default_fallback")]
    pub fallback: TrustClassRateLimitConfig,
}

/// Rate limit configuration for a specific trust class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustClassRateLimitConfig {
    /// Maximum messages per second
    pub max_messages_per_second: u32,

    /// Burst capacity (maximum tokens in bucket)
    pub burst_capacity: u32,
}

fn default_true() -> bool {
    true
}

fn default_refill_interval_ms() -> u64 {
    100
}

fn default_isolated() -> TrustClassRateLimitConfig {
    TrustClassRateLimitConfig {
        max_messages_per_second: 10,
        burst_capacity: 2,
    }
}

fn default_known() -> TrustClassRateLimitConfig {
    TrustClassRateLimitConfig {
        max_messages_per_second: 50,
        burst_capacity: 10,
    }
}

fn default_partner() -> TrustClassRateLimitConfig {
    TrustClassRateLimitConfig {
        max_messages_per_second: 100,
        burst_capacity: 20,
    }
}

fn default_federated() -> TrustClassRateLimitConfig {
    TrustClassRateLimitConfig {
        max_messages_per_second: 200,
        burst_capacity: 50,
    }
}

fn default_fallback() -> TrustClassRateLimitConfig {
    TrustClassRateLimitConfig {
        max_messages_per_second: 100,
        burst_capacity: 20,
    }
}

impl Default for RateLimitingConfig {
    fn default() -> Self {
        RateLimitingConfig {
            enabled: default_true(),
            refill_interval_ms: default_refill_interval_ms(),
            isolated: default_isolated(),
            known: default_known(),
            partner: default_partner(),
            federated: default_federated(),
            fallback: default_fallback(),
        }
    }
}

/// Gateway API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Enable the gateway API server
    #[serde(default)]
    pub enabled: bool,

    /// Gateway HTTP bind address (format: "IP:PORT")
    #[serde(default = "default_gateway_bind_addr")]
    pub bind_addr: String,

    /// JWT token expiration time in hours
    #[serde(default = "default_token_expiry_hours")]
    pub token_expiry_hours: u64,

    /// Challenge TTL in minutes
    #[serde(default = "default_challenge_ttl_minutes")]
    pub challenge_ttl_minutes: u64,

    /// JWT secret key (should be loaded from environment variable or secure file)
    /// If empty, gateway will fail to start
    #[serde(default)]
    pub jwt_secret: String,
}

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

/// Cooperative configuration for cooperative-specific settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CooperativeConfig {
    /// Treasury DID for the cooperative
    /// This DID is used as the source for budget payouts and other financial operations.
    /// If not set, the node's own DID will be used as a fallback.
    /// Format: "did:icn:<base58-pubkey>"
    #[serde(default)]
    pub treasury_did: Option<String>,

    /// Cooperative display name (human-readable)
    #[serde(default)]
    pub name: Option<String>,

    /// Cooperative description
    #[serde(default)]
    pub description: Option<String>,
}

fn default_gateway_bind_addr() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_token_expiry_hours() -> u64 {
    24
}

fn default_challenge_ttl_minutes() -> u64 {
    5
}

impl Default for GatewayConfig {
    fn default() -> Self {
        GatewayConfig {
            enabled: false,
            bind_addr: default_gateway_bind_addr(),
            token_expiry_hours: default_token_expiry_hours(),
            challenge_ttl_minutes: default_challenge_ttl_minutes(),
            jwt_secret: String::new(),
        }
    }
}

// Re-export topology types from icn-net to avoid circular dependencies
pub use icn_net::{FanoutConfig, NeighborLimitsConfig, NodeRole, TopologyConfig};

impl Default for Config {
    fn default() -> Self {
        Config {
            data_dir: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("icn"),
            network: NetworkConfig {
                mdns_enabled: true,
                listen_addr: "0.0.0.0:7777".to_string(),
                rpc_port: 5601,
                bootstrap_peers: vec![],
                stun_servers: default_stun_servers(),
            },
            observability: ObservabilityConfig {
                metrics_port: 9100,
                health_port: 8080,
                log_level: "info".to_string(),
            },
            rate_limiting: RateLimitingConfig::default(),
            topology: TopologyConfig::default(),
            gateway: GatewayConfig::default(),
            federation: FederationConfig::default(),
            privacy: PrivacyConfig::default(),
            cooperative: CooperativeConfig::default(),
            steward: StewardNodeConfig::default(),
        }
    }
}

impl RateLimitingConfig {
    /// Convert to trust-gated rate limit config for icn-net
    pub fn to_trust_gated_config(&self) -> icn_net::TrustGatedRateLimitConfig {
        use std::time::Duration;

        let refill_interval = Duration::from_millis(self.refill_interval_ms);

        icn_net::TrustGatedRateLimitConfig {
            isolated: self.isolated.to_rate_limit_config(refill_interval),
            known: self.known.to_rate_limit_config(refill_interval),
            partner: self.partner.to_rate_limit_config(refill_interval),
            federated: self.federated.to_rate_limit_config(refill_interval),
            refill_interval,
            // Default minimum trust threshold for rate limiting (H6 fix)
            // 0.1 requires "Known" trust class (at least one explicit trust edge)
            // This prevents completely unknown DIDs from bypassing rate limits
            min_trust_threshold: 0.1,
        }
    }

    /// Convert to fallback rate limit config for icn-net
    pub fn to_fallback_config(&self) -> icn_net::RateLimitConfig {
        use std::time::Duration;
        self.fallback
            .to_rate_limit_config(Duration::from_millis(self.refill_interval_ms))
    }
}

impl TrustClassRateLimitConfig {
    /// Convert to rate limit config for icn-net
    pub fn to_rate_limit_config(
        &self,
        refill_interval: std::time::Duration,
    ) -> icn_net::RateLimitConfig {
        icn_net::RateLimitConfig {
            max_messages_per_second: self.max_messages_per_second,
            burst_capacity: self.burst_capacity,
            refill_interval,
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to a TOML file
    pub fn to_file(&self, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Get the keystore path
    pub fn keystore_path(&self) -> PathBuf {
        self.data_dir.join("identity.age")
    }

    /// Get the store path
    pub fn store_path(&self) -> PathBuf {
        self.data_dir.join("store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = Config::default();

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Should contain rate limiting section
        assert!(toml_str.contains("[rate_limiting]"));
        assert!(toml_str.contains("enabled"));
        assert!(toml_str.contains("[rate_limiting.isolated]"));
        assert!(toml_str.contains("[rate_limiting.known]"));
        assert!(toml_str.contains("[rate_limiting.partner]"));
        assert!(toml_str.contains("[rate_limiting.federated]"));
        assert!(toml_str.contains("[rate_limiting.fallback]"));

        // Deserialize back
        let deserialized: Config = toml::from_str(&toml_str).unwrap();

        // Verify rate limiting config
        assert!(deserialized.rate_limiting.enabled);
        assert_eq!(deserialized.rate_limiting.refill_interval_ms, 100);
        assert_eq!(
            deserialized.rate_limiting.isolated.max_messages_per_second,
            10
        );
        assert_eq!(deserialized.rate_limiting.isolated.burst_capacity, 2);
        assert_eq!(deserialized.rate_limiting.known.max_messages_per_second, 50);
        assert_eq!(deserialized.rate_limiting.known.burst_capacity, 10);
        assert_eq!(
            deserialized.rate_limiting.partner.max_messages_per_second,
            100
        );
        assert_eq!(deserialized.rate_limiting.partner.burst_capacity, 20);
        assert_eq!(
            deserialized.rate_limiting.federated.max_messages_per_second,
            200
        );
        assert_eq!(deserialized.rate_limiting.federated.burst_capacity, 50);
        assert_eq!(
            deserialized.rate_limiting.fallback.max_messages_per_second,
            100
        );
        assert_eq!(deserialized.rate_limiting.fallback.burst_capacity, 20);
    }

    #[test]
    fn test_rate_limiting_config_conversion() {
        let config = RateLimitingConfig::default();

        // Convert to icn-net types
        let trust_gated = config.to_trust_gated_config();
        let fallback = config.to_fallback_config();

        // Verify trust-gated config
        assert_eq!(trust_gated.isolated.max_messages_per_second, 10);
        assert_eq!(trust_gated.isolated.burst_capacity, 2);
        assert_eq!(trust_gated.known.max_messages_per_second, 50);
        assert_eq!(trust_gated.known.burst_capacity, 10);
        assert_eq!(trust_gated.partner.max_messages_per_second, 100);
        assert_eq!(trust_gated.partner.burst_capacity, 20);
        assert_eq!(trust_gated.federated.max_messages_per_second, 200);
        assert_eq!(trust_gated.federated.burst_capacity, 50);
        assert_eq!(trust_gated.refill_interval.as_millis(), 100);

        // Verify fallback config
        assert_eq!(fallback.max_messages_per_second, 100);
        assert_eq!(fallback.burst_capacity, 20);
        assert_eq!(fallback.refill_interval.as_millis(), 100);
    }

    #[test]
    fn test_repository_config_files() {
        // Test that the actual config files in the repository parse correctly
        // CARGO_MANIFEST_DIR is always set during cargo test and points to the crate directory
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR should be set during cargo test");

        // From icn-core crate: icn/crates/icn-core -> icn (workspace root) -> project root
        let workspace_root = std::path::PathBuf::from(manifest_dir)
            .parent() // -> crates/
            .expect("manifest dir should have parent")
            .parent() // -> icn/
            .expect("crates dir should have parent")
            .parent() // -> project root
            .expect("workspace should have parent")
            .to_path_buf();

        let config_dir = workspace_root.join("config");

        // Test icn-alpha.toml
        let alpha_path = config_dir.join("icn-alpha.toml");
        if alpha_path.exists() {
            let alpha = Config::from_file(&alpha_path).unwrap();
            assert_eq!(alpha.network.listen_addr, "0.0.0.0:7777");
            assert_eq!(alpha.observability.metrics_port, 9100);
            assert!(alpha.rate_limiting.enabled);
            assert_eq!(alpha.rate_limiting.isolated.max_messages_per_second, 10);
            assert_eq!(alpha.rate_limiting.federated.max_messages_per_second, 200);
        }

        // Test icn-beta.toml
        let beta_path = config_dir.join("icn-beta.toml");
        if beta_path.exists() {
            let beta = Config::from_file(&beta_path).unwrap();
            assert_eq!(beta.network.listen_addr, "0.0.0.0:7778");
            assert_eq!(beta.observability.metrics_port, 9101);
            assert!(beta.rate_limiting.enabled);
        }

        // Test icn.toml.example
        let example_path = config_dir.join("icn.toml.example");
        if example_path.exists() {
            let example = Config::from_file(&example_path).unwrap();
            assert!(example.rate_limiting.enabled);
            assert_eq!(example.rate_limiting.partner.max_messages_per_second, 100);
        }
    }

    #[test]
    fn test_partial_rate_limiting_config() {
        // Test that we can override individual rate limiting settings
        let toml_str = r#"
data_dir = "/tmp/icn"

[network]
mdns_enabled = true
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
bootstrap_peers = []

[observability]
metrics_port = 9100
health_port = 8080
log_level = "info"

[rate_limiting]
enabled = false
refill_interval_ms = 200

[rate_limiting.isolated]
max_messages_per_second = 5
burst_capacity = 3
"#;

        let config: Config = toml::from_str(toml_str).unwrap();

        assert!(!config.rate_limiting.enabled);
        assert_eq!(config.rate_limiting.refill_interval_ms, 200); // custom
        assert_eq!(config.rate_limiting.isolated.max_messages_per_second, 5); // custom
        assert_eq!(config.rate_limiting.isolated.burst_capacity, 3); // custom
                                                                     // Other fields should use defaults
        assert_eq!(config.rate_limiting.known.max_messages_per_second, 50);
        assert_eq!(config.rate_limiting.known.burst_capacity, 10);
    }

    #[test]
    fn test_gateway_config_defaults() {
        let config = GatewayConfig::default();

        assert!(!config.enabled); // Disabled by default
        assert_eq!(config.bind_addr, "127.0.0.1:8080");
        assert_eq!(config.token_expiry_hours, 24);
        assert_eq!(config.challenge_ttl_minutes, 5);
        assert_eq!(config.jwt_secret, "");
    }

    #[test]
    fn test_gateway_config_serialization() {
        let toml_str = r#"
enabled = true
bind_addr = "0.0.0.0:8080"
token_expiry_hours = 48
challenge_ttl_minutes = 10
jwt_secret = "test-secret"
"#;

        let config: GatewayConfig = toml::from_str(toml_str).unwrap();

        assert!(config.enabled);
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert_eq!(config.token_expiry_hours, 48);
        assert_eq!(config.challenge_ttl_minutes, 10);
        assert_eq!(config.jwt_secret, "test-secret");
    }

    #[test]
    fn test_config_with_gateway() {
        let config = Config::default();

        // Verify gateway config is present and has defaults
        assert!(!config.gateway.enabled);
        assert_eq!(config.gateway.bind_addr, "127.0.0.1:8080");

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[gateway]"));

        // Deserialize back
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert!(!deserialized.gateway.enabled);
        assert_eq!(deserialized.gateway.bind_addr, "127.0.0.1:8080");
    }

    #[test]
    fn test_federation_config_defaults() {
        let config = FederationConfig::default();

        assert!(!config.enabled); // Disabled by default
        assert_eq!(config.network_name, "icn-mainnet");
        assert!((config.bootstrap_peer_trust - 0.3).abs() < f64::EPSILON);
        assert!(!config.auto_accept_invites);
        assert!((config.min_invite_trust - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.max_federations, 10);
        assert!(config.announce_public_addr);

        // Retry defaults
        assert_eq!(config.retry.max_retries, 5);
        assert_eq!(config.retry.initial_delay_secs, 1);
        assert_eq!(config.retry.max_delay_secs, 60);
        assert_eq!(config.retry.reconnect_interval_secs, 30);
    }

    #[test]
    fn test_federation_config_serialization() {
        let toml_str = r#"
enabled = true
network_name = "my-coop-network"
bootstrap_peer_trust = 0.5
auto_accept_invites = true
min_invite_trust = 0.7
max_federations = 5
announce_public_addr = false

[retry]
max_retries = 10
initial_delay_secs = 2
max_delay_secs = 120
reconnect_interval_secs = 60
"#;

        let config: FederationConfig = toml::from_str(toml_str).unwrap();

        assert!(config.enabled);
        assert_eq!(config.network_name, "my-coop-network");
        assert!((config.bootstrap_peer_trust - 0.5).abs() < f64::EPSILON);
        assert!(config.auto_accept_invites);
        assert!((config.min_invite_trust - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.max_federations, 5);
        assert!(!config.announce_public_addr);

        // Retry settings
        assert_eq!(config.retry.max_retries, 10);
        assert_eq!(config.retry.initial_delay_secs, 2);
        assert_eq!(config.retry.max_delay_secs, 120);
        assert_eq!(config.retry.reconnect_interval_secs, 60);
    }

    #[test]
    fn test_config_with_federation() {
        let config = Config::default();

        // Verify federation config is present and has defaults
        assert!(!config.federation.enabled);
        assert_eq!(config.federation.network_name, "icn-mainnet");

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[federation]"));
        assert!(toml_str.contains("[federation.retry]"));

        // Deserialize back
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert!(!deserialized.federation.enabled);
        assert_eq!(deserialized.federation.network_name, "icn-mainnet");
    }

    #[test]
    fn test_partial_federation_config() {
        // Test that we can override individual federation settings
        let toml_str = r#"
data_dir = "/tmp/icn"

[network]
mdns_enabled = true
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
bootstrap_peers = ["icn://did:icn:test@192.168.1.100:7777"]

[observability]
metrics_port = 9100
health_port = 8080
log_level = "info"

[federation]
enabled = true
network_name = "test-network"
bootstrap_peer_trust = 0.4
"#;

        let config: Config = toml::from_str(toml_str).unwrap();

        // Custom federation settings
        assert!(config.federation.enabled);
        assert_eq!(config.federation.network_name, "test-network");
        assert!((config.federation.bootstrap_peer_trust - 0.4).abs() < f64::EPSILON);

        // Default federation settings
        assert!(!config.federation.auto_accept_invites);
        assert_eq!(config.federation.max_federations, 10);
        assert_eq!(config.federation.retry.max_retries, 5);

        // Verify bootstrap peers in network config
        assert_eq!(config.network.bootstrap_peers.len(), 1);
        assert!(config.network.bootstrap_peers[0].contains("did:icn:test"));
    }

    #[test]
    fn test_steward_config_defaults() {
        let config = StewardNodeConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.vui_threshold, 3);
        assert_eq!(config.vui_total_shares, 5);
        assert_eq!(config.max_concurrent_enrollments, 100);
        assert_eq!(config.max_concurrent_recoveries, 50);
        assert_eq!(config.token_validity_secs, 7 * 24 * 60 * 60);
    }

    #[test]
    fn test_steward_config_serialization() {
        let toml_str = r#"
enabled = true
vui_threshold = 5
vui_total_shares = 9
max_concurrent_enrollments = 200
max_concurrent_recoveries = 100
token_validity_secs = 86400
"#;

        let config: StewardNodeConfig = toml::from_str(toml_str).unwrap();

        assert!(config.enabled);
        assert_eq!(config.vui_threshold, 5);
        assert_eq!(config.vui_total_shares, 9);
        assert_eq!(config.max_concurrent_enrollments, 200);
        assert_eq!(config.max_concurrent_recoveries, 100);
        assert_eq!(config.token_validity_secs, 86400);
    }

    #[test]
    fn test_steward_config_conversion() {
        let node_config = StewardNodeConfig {
            enabled: true,
            vui_threshold: 5,
            vui_total_shares: 9,
            max_concurrent_enrollments: 200,
            max_concurrent_recoveries: 100,
            token_validity_secs: 86400,
        };

        let steward_config = node_config.to_steward_config();

        assert_eq!(steward_config.vui_threshold, 5);
        assert_eq!(steward_config.vui_total_shares, 9);
        assert_eq!(steward_config.max_concurrent_enrollments, 200);
        assert_eq!(steward_config.max_concurrent_recoveries, 100);
        assert_eq!(steward_config.token_validity_secs, 86400);
    }

    #[test]
    fn test_config_with_steward() {
        let config = Config::default();

        // Verify steward config is present and has defaults
        assert!(!config.steward.enabled);
        assert_eq!(config.steward.vui_threshold, 3);
        assert_eq!(config.steward.vui_total_shares, 5);

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[steward]"));

        // Deserialize back
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert!(!deserialized.steward.enabled);
        assert_eq!(deserialized.steward.vui_threshold, 3);
    }
}
