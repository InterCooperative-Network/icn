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
            rate_limiting: RateLimitingConfig::default(),
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
        }
    }

    /// Convert to fallback rate limit config for icn-net
    pub fn to_fallback_config(&self) -> icn_net::RateLimitConfig {
        use std::time::Duration;
        self.fallback.to_rate_limit_config(Duration::from_millis(self.refill_interval_ms))
    }
}

impl TrustClassRateLimitConfig {
    /// Convert to rate limit config for icn-net
    pub fn to_rate_limit_config(&self, refill_interval: std::time::Duration) -> icn_net::RateLimitConfig {
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
        assert_eq!(deserialized.rate_limiting.isolated.max_messages_per_second, 10);
        assert_eq!(deserialized.rate_limiting.isolated.burst_capacity, 2);
        assert_eq!(deserialized.rate_limiting.known.max_messages_per_second, 50);
        assert_eq!(deserialized.rate_limiting.known.burst_capacity, 10);
        assert_eq!(deserialized.rate_limiting.partner.max_messages_per_second, 100);
        assert_eq!(deserialized.rate_limiting.partner.burst_capacity, 20);
        assert_eq!(deserialized.rate_limiting.federated.max_messages_per_second, 200);
        assert_eq!(deserialized.rate_limiting.federated.burst_capacity, 50);
        assert_eq!(deserialized.rate_limiting.fallback.max_messages_per_second, 100);
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
        let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
            .map(|dir| std::path::PathBuf::from(dir).parent().unwrap().parent().unwrap().parent().unwrap().to_path_buf())
            .unwrap_or_else(|_| std::path::PathBuf::from("/home/matt/projects/icn"));

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
}
