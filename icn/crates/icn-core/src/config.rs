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
}

fn default_rpc_port() -> u16 {
    5601
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
            },
            observability: ObservabilityConfig {
                metrics_port: 9100,
                health_port: 8080,
                log_level: "info".to_string(),
            },
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
