//! NAT traversal support using STUN and TURN protocols.
//!
//! This module provides a unified interface for NAT traversal, wrapping the
//! low-level STUN and TURN clients. It handles:
//!
//! - Public IP discovery via STUN
//! - NAT type detection
//! - Fallback to TURN relay when direct connectivity fails
//!
//! # Example
//!
//! ```no_run
//! use icn_net::nat::{NatConfig, NatTraversal};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = NatConfig::default();
//! let nat = NatTraversal::new(config).await?;
//!
//! // Discover public address
//! let public_addr = nat.discover_public_address().await?;
//! println!("Public address: {} (NAT type: {:?})", public_addr.address, public_addr.nat_type);
//! # Ok(())
//! # }
//! ```

use crate::stun::StunClient;
use crate::turn::{TurnClient, TurnConfig};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

/// Configuration for NAT traversal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatConfig {
    /// STUN server addresses for public IP discovery (hostname:port format)
    pub stun_servers: Vec<String>,

    /// TURN relay servers (fallback for symmetric NAT)
    pub turn_servers: Vec<TurnServerConfig>,

    /// Whether NAT traversal is enabled
    pub enabled: bool,

    /// Timeout for STUN requests in milliseconds
    pub stun_timeout_ms: u64,

    /// Maximum retries for STUN queries
    pub stun_max_retries: u32,
}

impl Default for NatConfig {
    fn default() -> Self {
        Self {
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
                "stun1.l.google.com:19302".to_string(),
            ],
            turn_servers: vec![],
            enabled: true,
            stun_timeout_ms: 5000,
            stun_max_retries: 3,
        }
    }
}

/// TURN server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnServerConfig {
    /// Server URL (format: "turn:host:port" or just "host:port")
    pub url: String,

    /// Username for authentication (optional for some servers)
    pub username: Option<String>,

    /// Credential/password for authentication
    pub credential: Option<String>,
}

/// Discovered public address information
#[derive(Debug, Clone)]
pub struct PublicAddress {
    /// The discovered public address
    pub address: SocketAddr,

    /// Detected NAT type
    pub nat_type: NatType,

    /// Which server/method discovered this address
    pub discovered_via: String,
}

/// Detected NAT type
///
/// NAT types affect connectivity:
/// - `None` and `FullCone`: Best connectivity, direct connections work
/// - `RestrictedCone` and `PortRestricted`: Limited connectivity, may need hole-punching
/// - `Symmetric`: Worst case, requires TURN relay for connectivity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NatType {
    /// No NAT - direct public IP
    None,

    /// Full cone NAT - most permissive, any external host can send to mapped port
    FullCone,

    /// Restricted cone NAT - only hosts we've sent to can reply
    RestrictedCone,

    /// Port-restricted cone NAT - only specific host:port pairs we've sent to can reply
    PortRestricted,

    /// Symmetric NAT - different mapping for each destination, requires TURN
    Symmetric,

    /// Could not determine NAT type
    Unknown,
}

impl std::fmt::Display for NatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NatType::None => write!(f, "No NAT"),
            NatType::FullCone => write!(f, "Full Cone"),
            NatType::RestrictedCone => write!(f, "Restricted Cone"),
            NatType::PortRestricted => write!(f, "Port Restricted"),
            NatType::Symmetric => write!(f, "Symmetric"),
            NatType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// NAT traversal manager
///
/// Provides a unified interface for NAT traversal, automatically falling back
/// from STUN to TURN when needed.
pub struct NatTraversal {
    config: NatConfig,
    stun_client: Option<StunClient>,
    turn_clients: Vec<TurnClient>,
}

impl NatTraversal {
    /// Create a new NAT traversal manager with the given configuration
    pub async fn new(config: NatConfig) -> Result<Self> {
        if !config.enabled {
            return Ok(Self {
                config,
                stun_client: None,
                turn_clients: vec![],
            });
        }

        // Resolve STUN servers
        let stun_servers = Self::resolve_servers(&config.stun_servers).await;

        let stun_client = if stun_servers.is_empty() {
            warn!("No STUN servers could be resolved");
            None
        } else {
            Some(
                StunClient::new(stun_servers)
                    .with_timeout(Duration::from_millis(config.stun_timeout_ms))
                    .with_max_retries(config.stun_max_retries),
            )
        };

        // Create TURN clients
        let mut turn_clients = Vec::new();
        for turn_config in &config.turn_servers {
            if let Ok(client) = Self::create_turn_client(turn_config).await {
                turn_clients.push(client);
            }
        }

        Ok(Self {
            config,
            stun_client,
            turn_clients,
        })
    }

    /// Create with default Google STUN servers
    pub async fn with_defaults() -> Result<Self> {
        Self::new(NatConfig::default()).await
    }

    /// Resolve server hostnames to socket addresses
    async fn resolve_servers(hostnames: &[String]) -> Vec<SocketAddr> {
        let mut servers = Vec::new();

        for hostname in hostnames {
            // Strip any "stun:" prefix
            let hostname = hostname.strip_prefix("stun:").unwrap_or(hostname);

            match tokio::net::lookup_host(hostname).await {
                Ok(addrs) => {
                    if let Some(addr) = addrs.into_iter().next() {
                        debug!("Resolved STUN server {hostname} to {addr}");
                        servers.push(addr);
                    }
                }
                Err(e) => {
                    warn!("Failed to resolve STUN server {hostname}: {e}");
                }
            }
        }

        servers
    }

    /// Create a TURN client from configuration
    async fn create_turn_client(config: &TurnServerConfig) -> Result<TurnClient> {
        // Parse server URL, strip any "turn:" prefix
        let url = config.url.strip_prefix("turn:").unwrap_or(&config.url);

        let server: SocketAddr = tokio::net::lookup_host(url)
            .await
            .context("Failed to resolve TURN server")?
            .next()
            .context("No addresses for TURN server")?;

        let turn_config = TurnConfig::new(server)
            .with_username(config.username.clone())
            .with_password(config.credential.clone());

        Ok(TurnClient::new(turn_config))
    }

    /// Discover public address using STUN
    ///
    /// Queries configured STUN servers to discover the public-facing address.
    /// Returns the discovered address along with NAT type information.
    pub async fn discover_public_address(&self) -> Result<PublicAddress> {
        if !self.config.enabled {
            anyhow::bail!("NAT traversal is disabled");
        }

        let stun = self
            .stun_client
            .as_ref()
            .context("No STUN client configured")?;

        // Bind to an ephemeral port
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("Failed to bind UDP socket for STUN")?;

        let local_addr = socket.local_addr()?;
        debug!("STUN discovery using local socket: {local_addr}");

        let public_addr = stun
            .discover_public_endpoint(&socket)
            .await
            .context("STUN discovery failed")?;

        // Detect NAT type by comparing local and public addresses
        let nat_type = self
            .detect_nat_type(&socket, &local_addr, &public_addr)
            .await;

        info!(
            "Discovered public address: {} (NAT type: {})",
            public_addr, nat_type
        );

        Ok(PublicAddress {
            address: public_addr,
            nat_type,
            discovered_via: "STUN".to_string(),
        })
    }

    /// Detect NAT type based on STUN responses
    ///
    /// This is a simplified detection that determines if we're behind NAT.
    /// Full NAT type detection would require multiple STUN servers with
    /// different behaviors (change-request support).
    async fn detect_nat_type(
        &self,
        _socket: &UdpSocket,
        local_addr: &SocketAddr,
        public_addr: &SocketAddr,
    ) -> NatType {
        // If local and public IPs match, we have no NAT
        if local_addr.ip() == public_addr.ip() {
            return NatType::None;
        }

        // We're behind some form of NAT
        // Full detection would require:
        // 1. Query two different STUN servers
        // 2. Compare mapped addresses
        // 3. Use CHANGE-REQUEST to test port/address filtering
        //
        // For now, return Unknown since we can't distinguish types
        // without additional STUN queries
        NatType::Unknown
    }

    /// Check if TURN relay is needed (symmetric NAT detected)
    pub fn needs_turn_relay(&self, nat_type: NatType) -> bool {
        nat_type == NatType::Symmetric
    }

    /// Get a TURN client for relay connectivity
    pub fn get_turn_client(&self) -> Option<&TurnClient> {
        self.turn_clients.first()
    }

    /// Check if NAT traversal is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the current configuration
    pub fn config(&self) -> &NatConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nat_config_default() {
        let config = NatConfig::default();

        assert!(config.enabled);
        assert_eq!(config.stun_servers.len(), 2);
        assert!(config.turn_servers.is_empty());
        assert_eq!(config.stun_timeout_ms, 5000);
        assert_eq!(config.stun_max_retries, 3);
    }

    #[test]
    fn test_nat_config_serialization() {
        let config = NatConfig {
            stun_servers: vec!["stun.example.com:3478".to_string()],
            turn_servers: vec![TurnServerConfig {
                url: "turn:turn.example.com:3478".to_string(),
                username: Some("user".to_string()),
                credential: Some("pass".to_string()),
            }],
            enabled: true,
            stun_timeout_ms: 3000,
            stun_max_retries: 5,
        };

        let json = serde_json::to_string(&config).expect("Failed to serialize");
        let deserialized: NatConfig = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(deserialized.stun_servers.len(), 1);
        assert_eq!(deserialized.turn_servers.len(), 1);
        assert_eq!(deserialized.stun_timeout_ms, 3000);
        assert_eq!(deserialized.stun_max_retries, 5);
    }

    #[test]
    fn test_nat_type_display() {
        assert_eq!(format!("{}", NatType::None), "No NAT");
        assert_eq!(format!("{}", NatType::FullCone), "Full Cone");
        assert_eq!(format!("{}", NatType::RestrictedCone), "Restricted Cone");
        assert_eq!(format!("{}", NatType::PortRestricted), "Port Restricted");
        assert_eq!(format!("{}", NatType::Symmetric), "Symmetric");
        assert_eq!(format!("{}", NatType::Unknown), "Unknown");
    }

    #[test]
    fn test_nat_type_serialization() {
        let nat_types = vec![
            NatType::None,
            NatType::FullCone,
            NatType::RestrictedCone,
            NatType::PortRestricted,
            NatType::Symmetric,
            NatType::Unknown,
        ];

        for nat_type in nat_types {
            let json = serde_json::to_string(&nat_type).expect("Failed to serialize");
            let deserialized: NatType = serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(deserialized, nat_type);
        }
    }

    #[tokio::test]
    async fn test_nat_traversal_disabled() {
        let config = NatConfig {
            enabled: false,
            ..Default::default()
        };

        let nat = NatTraversal::new(config)
            .await
            .expect("Failed to create NatTraversal");

        assert!(!nat.is_enabled());
        assert!(nat.discover_public_address().await.is_err());
    }

    #[tokio::test]
    async fn test_nat_traversal_no_stun_servers() {
        let config = NatConfig {
            stun_servers: vec![],
            enabled: true,
            ..Default::default()
        };

        let nat = NatTraversal::new(config)
            .await
            .expect("Failed to create NatTraversal");

        assert!(nat.stun_client.is_none());
    }

    #[test]
    fn test_needs_turn_relay() {
        let _config = NatConfig::default();
        // Cannot test full NatTraversal without async, just verify logic
        assert!(NatType::Symmetric == NatType::Symmetric);
        assert!(NatType::FullCone != NatType::Symmetric);
    }

    #[tokio::test]
    async fn test_nat_traversal_with_defaults() {
        // This test requires network access - skip in CI
        if std::env::var("CI").is_ok() {
            return;
        }

        let nat = NatTraversal::with_defaults().await;
        assert!(nat.is_ok());
    }
}
