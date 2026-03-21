//! Network layer configuration

use serde::{Deserialize, Serialize};

/// Network layer configuration
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

    /// Minimum trust score required for TLS connection acceptance (0.0 to 1.0)
    /// - 0.0 = Accept all authenticated DIDs (trust-gated TLS disabled)
    /// - 0.1 = Require at least "Known" trust class (one trust edge)
    /// - 0.4 = Require "Partner" trust class
    /// - 0.7 = Require "Federated" trust class
    ///
    /// Default: 0.1 (require at least one trust relationship)
    ///
    /// For controlled/trusted environments, set to 0.0 to disable trust gating
    #[serde(default = "default_min_trust_threshold")]
    pub min_trust_threshold: f64,

    /// TURN relay server for NAT traversal fallback (format: "IP:PORT")
    /// Used when direct connection fails (e.g., symmetric NAT)
    #[serde(default)]
    pub turn_server: Option<String>,

    /// TURN server username (optional, for authenticated TURN)
    #[serde(default)]
    pub turn_username: Option<String>,

    /// TURN server password (optional, for authenticated TURN)
    #[serde(default)]
    pub turn_password: Option<String>,

    /// Enable end-to-end encryption for all messages (when peer supports it)
    ///
    /// When enabled, messages sent to peers that advertise E2E_ENCRYPTION capability
    /// will be encrypted with the recipient's X25519 public key before signing.
    /// Messages from non-supporting peers remain signed-only (backward compatible).
    ///
    /// ## Fail-Closed Behavior
    ///
    /// If encryption fails (e.g., serialization error, missing key), the message
    /// is **dropped** rather than sent unencrypted. This ensures confidential data
    /// is never leaked over the network. Encryption failures are:
    /// - Logged at ERROR level for immediate visibility
    /// - Tracked via `icn_network_encryption_failed_total` metric
    ///
    /// Monitor this metric in production to detect systematic encryption issues
    /// that may be causing message loss.
    ///
    /// Default: true (recommended for pilot and production)
    #[serde(default = "default_true")]
    pub e2e_encryption_enabled: bool,

    /// Number of consecutive encryption sequence cleanup failures before escalating
    /// to ERROR level and tripping the circuit breaker.
    ///
    /// The cleanup task runs hourly to remove stale sequence entries. If cleanup
    /// fails repeatedly (e.g., storage issues), the circuit breaker trips to:
    /// - Escalate logging from WARN to ERROR
    /// - Increment `icn_network_encryption_circuit_breaker_trips_total` metric
    ///
    /// Lower values detect issues faster but may cause false alarms during
    /// transient storage hiccups. Higher values are more tolerant but delay
    /// detection of persistent issues.
    ///
    /// Default: 3 (failures detected within 3 hours)
    #[serde(default = "default_circuit_breaker_threshold")]
    pub encryption_cleanup_circuit_breaker_threshold: u32,

    /// NAT traversal dial strategy configuration
    #[serde(default)]
    pub nat_dial: NatDialConfig,

    /// Blob registry configuration for distributed data tracking
    #[serde(default)]
    pub blob_registry: BlobRegistryConfig,
}

/// Blob registry configuration for distributed data tracking
///
/// Controls size limits and quotas to prevent memory exhaustion attacks.
/// The blob registry tracks which peers have which data blobs for
/// locality-aware task placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRegistryConfig {
    /// Maximum size of a single blob in bytes (default: 10MB)
    ///
    /// Blobs larger than this are rejected. This prevents malicious peers
    /// from announcing extremely large blobs that could exhaust memory.
    #[serde(default = "default_max_blob_size")]
    pub max_blob_size: u64,

    /// Maximum total size of all blobs in the registry in bytes (default: 100MB)
    ///
    /// When this limit is reached, the oldest entries are evicted (LRU)
    /// to make room for new announcements.
    #[serde(default = "default_max_registry_size")]
    pub max_registry_size: u64,

    /// Maximum total blob size per peer in bytes (default: 10MB)
    ///
    /// This prevents a single peer from consuming too much of the registry
    /// capacity. Announcements from peers exceeding their quota are rejected.
    #[serde(default = "default_max_per_peer_size")]
    pub max_per_peer_size: u64,
}

impl Default for BlobRegistryConfig {
    fn default() -> Self {
        Self {
            max_blob_size: default_max_blob_size(),
            max_registry_size: default_max_registry_size(),
            max_per_peer_size: default_max_per_peer_size(),
        }
    }
}

fn default_max_blob_size() -> u64 {
    10 * 1024 * 1024 // 10MB
}

fn default_max_registry_size() -> u64 {
    100 * 1024 * 1024 // 100MB
}

fn default_max_per_peer_size() -> u64 {
    10 * 1024 * 1024 // 10MB
}

/// NAT traversal dial strategy configuration
///
/// Controls how ICN attempts to connect to peers with multiple candidate addresses.
/// The strategy tries addresses in order: local -> public -> relay, with configurable
/// timeouts and parallel attempts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatDialConfig {
    /// Enable parallel dial attempts for local and public addresses
    ///
    /// When true, local and public addresses are dialed simultaneously and the
    /// first successful connection wins. When false, addresses are tried sequentially.
    ///
    /// Default: true (faster connection establishment)
    #[serde(default = "default_true")]
    pub parallel_dial: bool,

    /// Timeout for local address dial attempts in milliseconds
    ///
    /// Local addresses (same LAN) should connect quickly. A short timeout
    /// avoids waiting too long when peers are not on the same network.
    ///
    /// Default: 2000 (2 seconds)
    #[serde(default = "default_local_dial_timeout_ms")]
    pub local_dial_timeout_ms: u64,

    /// Timeout for public address dial attempts in milliseconds
    ///
    /// Public addresses require NAT hole punching which may take longer.
    /// This timeout should allow for packet loss and retransmission.
    ///
    /// Default: 10000 (10 seconds)
    #[serde(default = "default_public_dial_timeout_ms")]
    pub public_dial_timeout_ms: u64,

    /// Timeout for relay address dial attempts in milliseconds
    ///
    /// Relay connections go through a TURN server, adding latency.
    /// This should be the longest timeout as it's the fallback option.
    ///
    /// Default: 30000 (30 seconds)
    #[serde(default = "default_relay_dial_timeout_ms")]
    pub relay_dial_timeout_ms: u64,

    /// Interval for re-announcing connection candidates in seconds
    ///
    /// Candidates are periodically re-published to gossip to account for
    /// IP changes and keep TTL fresh. Should be less than candidate TTL (5 min).
    ///
    /// Default: 150 (2.5 minutes)
    #[serde(default = "default_candidate_announce_interval_secs")]
    pub candidate_announce_interval_secs: u64,

    /// Delay before attempting the IPv4 fallback in Happy Eyeballs dialing (RFC 8305), in milliseconds
    ///
    /// When multiple addresses of different IP versions are available within one endpoint
    /// category (Local or Public), ICN dials IPv6 first and waits this long before also
    /// spawning an IPv4 dial task. The first successful connection wins.
    ///
    /// 250ms matches RFC 8305 §5 and the value used by all major browsers.
    /// Set to 0 to start the IPv4 task immediately alongside IPv6 (no stagger delay).
    /// IPv6 is still preferred — this only removes the delay before IPv4 is spawned.
    ///
    /// Default: 250 (milliseconds)
    #[serde(default = "default_happy_eyeballs_delay_ms")]
    pub happy_eyeballs_delay_ms: u64,
}

impl Default for NatDialConfig {
    fn default() -> Self {
        Self {
            parallel_dial: true,
            local_dial_timeout_ms: default_local_dial_timeout_ms(),
            public_dial_timeout_ms: default_public_dial_timeout_ms(),
            relay_dial_timeout_ms: default_relay_dial_timeout_ms(),
            candidate_announce_interval_secs: default_candidate_announce_interval_secs(),
            happy_eyeballs_delay_ms: default_happy_eyeballs_delay_ms(),
        }
    }
}

fn default_local_dial_timeout_ms() -> u64 {
    2000
}

fn default_public_dial_timeout_ms() -> u64 {
    10000
}

fn default_relay_dial_timeout_ms() -> u64 {
    30000
}

fn default_candidate_announce_interval_secs() -> u64 {
    150
}

fn default_happy_eyeballs_delay_ms() -> u64 {
    250
}

fn default_circuit_breaker_threshold() -> u32 {
    3
}

impl NetworkConfig {
    /// Convert to TurnConfig if TURN server is configured
    pub fn turn_config(&self) -> Option<icn_net::TurnConfig> {
        let server = self.turn_server.as_ref()?;
        let server_addr = server.parse().ok()?;
        Some(
            icn_net::TurnConfig::new(server_addr)
                .with_username(self.turn_username.clone())
                .with_password(self.turn_password.clone()),
        )
    }
}

fn default_rpc_port() -> u16 {
    5601
}

pub(crate) fn default_stun_servers() -> Vec<String> {
    // Use Google's public STUN servers by default
    // Multiple servers enable majority vote for robust NAT discovery
    vec![
        "stun.l.google.com:19302".to_string(),
        "stun1.l.google.com:19302".to_string(),
    ]
}

pub(crate) fn default_min_trust_threshold() -> f64 {
    0.1 // Require at least "Known" trust class by default
}

pub(crate) fn default_true() -> bool {
    true
}
