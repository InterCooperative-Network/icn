//! Configuration management for ICNd
//!
//! This module provides configuration types for all ICN subsystems,
//! organized by domain into sub-modules.

mod compute;
mod cooperative;
mod federation;
mod gateway;
mod gossip;
mod identity;
mod ledger;
mod network;
mod observability;
mod privacy;
mod steward;
mod supervisor;
mod trust;

// Re-export all types for backwards compatibility
pub use compute::*;
pub use cooperative::*;
pub use federation::*;
pub use gateway::*;
pub use gossip::*;
pub use identity::*;
pub use ledger::*;
pub use network::*;
pub use observability::*;
pub use privacy::*;
pub use steward::*;
pub use supervisor::*;
pub use trust::*;

// Re-export topology types from icn-net to avoid circular dependencies
pub use icn_net::{FanoutConfig, NeighborLimitsConfig, NodeRole, TopologyConfig};

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

    /// Supervisor configuration for background tasks and timeouts
    #[serde(default)]
    pub supervisor: SupervisorConfig,

    /// Ledger configuration for mutual credit and exchange rate oracle
    #[serde(default)]
    pub ledger: LedgerConfig,

    /// Identity backend configuration (keystore selection)
    #[serde(default)]
    pub identity: IdentityConfig,

    /// Gossip protocol configuration
    #[serde(default)]
    pub gossip: GossipConfig,

    /// Distributed compute configuration
    #[serde(default)]
    pub compute: ComputeConfig,

    /// Trust graph configuration
    #[serde(default)]
    pub trust: TrustConfig,
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
                stun_servers: default_stun_servers(),
                min_trust_threshold: default_min_trust_threshold(),
                turn_server: None,
                turn_username: None,
                turn_password: None,
                e2e_encryption_enabled: true,
                encryption_cleanup_circuit_breaker_threshold: 3,
                nat_dial: NatDialConfig::default(),
                blob_registry: BlobRegistryConfig::default(),
            },
            observability: ObservabilityConfig {
                metrics_port: 9100,
                health_port: 8080,
                log_level: "info".to_string(),
                tracing: TracingConfig::default(),
            },
            rate_limiting: RateLimitingConfig::default(),
            topology: TopologyConfig::default(),
            gateway: GatewayConfig::default(),
            federation: FederationConfig::default(),
            privacy: PrivacyConfig::default(),
            cooperative: CooperativeConfig::default(),
            steward: StewardNodeConfig::default(),
            supervisor: SupervisorConfig::default(),
            ledger: LedgerConfig::default(),
            identity: IdentityConfig::default(),
            gossip: GossipConfig::default(),
            compute: ComputeConfig::default(),
            trust: TrustConfig::default(),
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

    /// Validate configuration and return a list of warnings/errors
    ///
    /// Returns Ok(warnings) if config is valid (warnings are non-fatal),
    /// Returns Err(errors) if config has fatal issues.
    pub fn validate(&self) -> Result<Vec<String>, Vec<String>> {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // Gateway validation
        if self.gateway.enabled {
            if self.gateway.jwt_secret.is_empty() {
                errors.push(
                    "Gateway is enabled but jwt_secret is empty. Set via:\n  \
                     - Config: gateway.jwt_secret = \"your-secret\"\n  \
                     - Environment: ICN_GATEWAY_JWT_SECRET\n  \
                     - CLI: --gateway-jwt-secret"
                        .to_string(),
                );
            } else if self.gateway.jwt_secret.len() < 32 {
                warnings.push(format!(
                    "Gateway jwt_secret is {} bytes, recommend at least 32 bytes for security",
                    self.gateway.jwt_secret.len()
                ));
            }

            if self.gateway.bind_addr.starts_with("0.0.0.0") {
                warnings.push(
                    "Gateway binding to 0.0.0.0 - ensure proper firewall/reverse proxy in production"
                        .to_string(),
                );
            }
        }

        // Network validation
        if self.network.listen_addr.is_empty() {
            errors.push("network.listen_addr cannot be empty".to_string());
        }

        // TrustScore already validates range [0.0, 1.0] at construction time

        // Trust threshold warnings
        if self.network.min_trust_threshold.value() == 0.0 {
            warnings.push(
                "network.min_trust_threshold is 0.0 - trust-gated TLS is disabled, \
                 all authenticated DIDs will be accepted"
                    .to_string(),
            );
        }

        // Federation validation
        if self.federation.enabled {
            if self.federation.coop_id.is_empty() {
                warnings.push(
                    "Federation enabled but coop_id is empty - will be derived from network_name"
                        .to_string(),
                );
            }

            if self.federation.bootstrap_peer_trust < 0.0
                || self.federation.bootstrap_peer_trust > 1.0
            {
                errors.push(format!(
                    "federation.bootstrap_peer_trust must be 0.0-1.0, got {}",
                    self.federation.bootstrap_peer_trust
                ));
            }
        }

        // Privacy validation
        if self.privacy.enabled && self.privacy.onion_routing_enabled {
            if self.privacy.onion_hops < 2 {
                warnings.push(format!(
                    "privacy.onion_hops is {} - recommend at least 2 for meaningful anonymity",
                    self.privacy.onion_hops
                ));
            }
            if self.privacy.onion_hops > 5 {
                warnings.push(format!(
                    "privacy.onion_hops is {} - high latency expected, consider reducing",
                    self.privacy.onion_hops
                ));
            }
        }

        // Steward validation
        if self.steward.enabled {
            if self.steward.vui_threshold > self.steward.vui_total_shares {
                errors.push(format!(
                    "steward.vui_threshold ({}) cannot exceed vui_total_shares ({})",
                    self.steward.vui_threshold, self.steward.vui_total_shares
                ));
            }
            if self.steward.vui_threshold < 2 {
                warnings
                    .push("steward.vui_threshold < 2 provides no threshold security".to_string());
            }
        }

        // Rate limiting validation
        if self.rate_limiting.enabled && self.rate_limiting.refill_interval_ms == 0 {
            errors.push("rate_limiting.refill_interval_ms cannot be 0".to_string());
        }

        // Topology validation
        if self.topology.fanout.local_cluster == 0 {
            errors.push("topology.fanout.local_cluster cannot be 0".to_string());
        }

        // Trust validation
        // Note: TrustScore validates range [0.0, 1.0] at construction time
        if self.trust.propagation.max_path_length == 0 {
            errors.push("trust.propagation.max_path_length cannot be 0".to_string());
        }

        // Gossip validation
        if self.gossip.replication.target_replicas == 0 {
            errors.push("gossip.replication.target_replicas cannot be 0".to_string());
        }
        // Note: TrustScore validates range [0.0, 1.0] at construction time

        // Compute validation
        if self.compute.verification.consensus_threshold < 0.0
            || self.compute.verification.consensus_threshold > 1.0
        {
            errors.push(format!(
                "compute.verification.consensus_threshold must be 0.0-1.0, got {}",
                self.compute.verification.consensus_threshold
            ));
        }
        if self.compute.verification.high_value_quorum == 0 {
            errors.push("compute.verification.high_value_quorum cannot be 0".to_string());
        }
        if self.compute.max_concurrent_tasks == 0 {
            errors.push("compute.max_concurrent_tasks cannot be 0".to_string());
        }

        // Identity backend validation
        match self.identity.validate() {
            Ok(id_warnings) => warnings.extend(id_warnings),
            Err(id_errors) => errors.extend(id_errors),
        }

        if errors.is_empty() {
            Ok(warnings)
        } else {
            Err(errors)
        }
    }

    /// Print validation results to stderr with formatting
    pub fn print_validation_results(&self) {
        match self.validate() {
            Ok(warnings) => {
                for warning in warnings {
                    eprintln!("\x1b[33mWarning:\x1b[0m {warning}");
                }
            }
            Err(errors) => {
                for error in &errors {
                    eprintln!("\x1b[31mError:\x1b[0m {error}");
                }
                eprintln!(
                    "\n\x1b[31mConfiguration has {} error(s). Fix them before starting.\x1b[0m",
                    errors.len()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_trust::TrustScore;

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
        let fallback = config.to_fallback_config();

        // Verify fallback config
        assert_eq!(fallback.max_messages_per_second, 100);
        assert_eq!(fallback.burst_capacity, 20);
        assert_eq!(fallback.refill_interval.as_millis(), 100);
    }

    #[test]
    fn test_repository_config_files() {
        // Golden integration test: validates that actual config files in the repository
        // parse correctly AND pass validation checks
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

            // Validate configuration - should have zero fatal errors
            let validation = alpha.validate();
            match validation {
                Ok(warnings) => {
                    // Warnings are acceptable for example configs (e.g., missing JWT secret)
                    // but we want to ensure there are no fatal errors
                    eprintln!("icn-alpha.toml validation warnings: {:?}", warnings);
                }
                Err(errors) => {
                    panic!("icn-alpha.toml has fatal validation errors: {:?}", errors);
                }
            }
        }

        // Test icn-beta.toml
        let beta_path = config_dir.join("icn-beta.toml");
        if beta_path.exists() {
            let beta = Config::from_file(&beta_path).unwrap();
            assert_eq!(beta.network.listen_addr, "0.0.0.0:7778");
            assert_eq!(beta.observability.metrics_port, 9101);
            assert!(beta.rate_limiting.enabled);

            // Validate configuration
            let validation = beta.validate();
            match validation {
                Ok(warnings) => {
                    eprintln!("icn-beta.toml validation warnings: {:?}", warnings);
                }
                Err(errors) => {
                    panic!("icn-beta.toml has fatal validation errors: {:?}", errors);
                }
            }
        }

        // Test icn.toml.example
        let example_path = config_dir.join("icn.toml.example");
        if example_path.exists() {
            let example = Config::from_file(&example_path).unwrap();
            assert!(example.rate_limiting.enabled);
            assert_eq!(example.rate_limiting.partner.max_messages_per_second, 100);

            // Validate configuration - example file should be production-ready
            let validation = example.validate();
            match validation {
                Ok(warnings) => {
                    // Document expected warnings for the example config
                    // Example configs may have warnings (e.g., missing JWT secret, 0.0.0.0 binding)
                    // but should have zero fatal errors
                    eprintln!("icn.toml.example validation warnings: {:?}", warnings);
                }
                Err(errors) => {
                    panic!("icn.toml.example has fatal validation errors: {:?}", errors);
                }
            }
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

    #[test]
    fn test_min_trust_threshold_config() {
        // Test default value
        let config = Config::default();
        assert!((config.network.min_trust_threshold - 0.1).abs() < f64::EPSILON);

        // Test serialization with default
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("min_trust_threshold"));

        // Test custom value parsing
        let custom_toml = r#"
data_dir = "/tmp/icn"

[network]
mdns_enabled = true
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
bootstrap_peers = []
min_trust_threshold = 0.0

[observability]
metrics_port = 9100
health_port = 8080
log_level = "info"
"#;

        let config: Config = toml::from_str(custom_toml).unwrap();
        assert!((config.network.min_trust_threshold.value() - 0.0).abs() < f64::EPSILON);

        // Test that fallback config can be generated
        let _fallback = config.rate_limiting.to_fallback_config();
    }

    #[test]
    fn test_config_validation_default_passes() {
        let config = Config::default();
        let result = config.validate();
        // Default config should pass validation (may have warnings)
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_validation_gateway_jwt_required() {
        let mut config = Config::default();
        config.gateway.enabled = true;
        config.gateway.jwt_secret = String::new();

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("jwt_secret")));
    }

    #[test]
    fn test_config_validation_gateway_jwt_short_warning() {
        let mut config = Config::default();
        config.gateway.enabled = true;
        config.gateway.jwt_secret = "short".to_string();

        let result = config.validate();
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(warnings.iter().any(|w| w.contains("32 bytes")));
    }

    #[test]
    fn test_config_validation_steward_threshold() {
        let mut config = Config::default();
        config.steward.enabled = true;
        config.steward.vui_threshold = 10;
        config.steward.vui_total_shares = 5;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("vui_threshold")));
    }

    #[test]
    fn test_config_validation_trust_threshold_range() {
        // TrustScore validates range during deserialization
        let invalid_toml = r#"
[network]
min_trust_threshold = 1.5
"#;
        let result: Result<Config, _> = toml::from_str(invalid_toml);
        // Should fail during TOML parsing since TrustScore rejects invalid values
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_zero_trust_warning() {
        let mut config = Config::default();
        config.network.min_trust_threshold = TrustScore::unchecked(0.0);

        let result = config.validate();
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(warnings
            .iter()
            .any(|w| w.contains("trust-gated TLS is disabled")));
    }

    #[test]
    fn test_config_validation_trust_decay_factor_range() {
        // TrustScore validates range during deserialization
        let invalid_toml = r#"
[trust.propagation]
decay_factor = 1.5
"#;
        let result: Result<Config, _> = toml::from_str(invalid_toml);
        // Should fail during TOML parsing since TrustScore rejects invalid values
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_gossip_replica_trust_range() {
        // TrustScore validates range during deserialization
        let invalid_toml = r#"
[gossip.replication]
min_replica_trust = -0.5
"#;
        let result: Result<Config, _> = toml::from_str(invalid_toml);
        // Should fail during TOML parsing since TrustScore rejects invalid values
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_compute_consensus_threshold_range() {
        let mut config = Config::default();
        config.compute.verification.consensus_threshold = 2.0;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("consensus_threshold")));
    }

    #[test]
    fn test_config_validation_zero_values() {
        let mut config = Config::default();
        config.trust.propagation.max_path_length = 0;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("max_path_length")));

        let mut config = Config::default();
        config.gossip.replication.target_replicas = 0;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("target_replicas")));

        let mut config = Config::default();
        config.compute.verification.high_value_quorum = 0;

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("high_value_quorum")));
    }

    #[test]
    fn test_gossip_config_defaults() {
        let config = GossipConfig::default();

        // Replication defaults
        assert_eq!(config.replication.target_replicas, 3);
        assert!((config.replication.min_replica_trust - 0.4).abs() < f64::EPSILON);
        assert_eq!(config.replication.health_check_interval_secs, 60);
        assert_eq!(config.replication.stale_threshold_secs, 300);
        assert_eq!(config.replication.unreachable_threshold_secs, 900);

        // Partition defaults
        assert_eq!(config.partition.silence_threshold_secs, 300);
        assert_eq!(config.partition.check_interval_secs, 30);
        assert!(config.partition.auto_heal_enabled);
        assert_eq!(config.partition.heal_interval_secs, 60);
    }

    #[test]
    fn test_gossip_config_serialization() {
        let toml_str = r#"
[replication]
target_replicas = 5
min_replica_trust = 0.5
health_check_interval_secs = 120

[partition]
silence_threshold_secs = 600
check_interval_secs = 60
auto_heal_enabled = false
"#;

        let config: GossipConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(config.replication.target_replicas, 5);
        assert!((config.replication.min_replica_trust - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.replication.health_check_interval_secs, 120);
        assert_eq!(config.partition.silence_threshold_secs, 600);
        assert_eq!(config.partition.check_interval_secs, 60);
        assert!(!config.partition.auto_heal_enabled);
    }

    #[test]
    fn test_compute_config_defaults() {
        let config = ComputeConfig::default();

        assert_eq!(config.max_concurrent_tasks, 10);
        assert!(!config.actor_model_enabled);
        assert_eq!(config.max_actors, 100);

        // Verification defaults
        assert_eq!(config.verification.low_value_threshold, 100);
        assert_eq!(config.verification.medium_value_threshold, 1000);
        assert_eq!(config.verification.high_value_threshold, 10000);
        assert_eq!(config.verification.high_value_quorum, 3);
        assert!((config.verification.consensus_threshold - 0.67).abs() < f64::EPSILON);
        assert_eq!(config.verification.collection_window_ms, 30_000);
    }

    #[test]
    fn test_compute_config_serialization() {
        let toml_str = r#"
max_concurrent_tasks = 20
actor_model_enabled = true
max_actors = 200

[verification]
low_value_threshold = 50
high_value_quorum = 5
"#;

        let config: ComputeConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(config.max_concurrent_tasks, 20);
        assert!(config.actor_model_enabled);
        assert_eq!(config.max_actors, 200);
        assert_eq!(config.verification.low_value_threshold, 50);
        assert_eq!(config.verification.high_value_quorum, 5);
        // Other fields should use defaults
        assert_eq!(config.verification.medium_value_threshold, 1000);
    }

    #[test]
    fn test_trust_config_defaults() {
        let config = TrustConfig::default();

        // Attestation defaults
        assert!((config.attestation.min_attester_trust - 0.3).abs() < f64::EPSILON);
        assert_eq!(config.attestation.max_attestations_per_day, 10);
        assert!(config.attestation.evidence_required);
        assert!((config.attestation.min_evidence_score - 0.5).abs() < f64::EPSILON);

        // Propagation defaults
        assert_eq!(config.propagation.max_path_length, 3);
        assert!((config.propagation.decay_factor - 0.8).abs() < f64::EPSILON);
        assert!((config.propagation.min_edge_trust - 0.1).abs() < f64::EPSILON);
        assert!(config.propagation.cache_enabled);
        assert_eq!(config.propagation.cache_ttl_secs, 300);

        // Sybil resistance defaults
        assert!(config.sybil_resistance.enabled);
        assert!((config.sybil_resistance.max_trust_concentration - 0.3).abs() < f64::EPSILON);
        assert_eq!(config.sybil_resistance.sample_size, 100);
        assert!((config.sybil_resistance.min_diversity_ratio - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_trust_config_serialization() {
        let toml_str = r#"
[attestation]
min_attester_trust = 0.5
max_attestations_per_day = 20
evidence_required = false

[propagation]
max_path_length = 5
decay_factor = 0.9

[sybil_resistance]
enabled = false
"#;

        let config: TrustConfig = toml::from_str(toml_str).unwrap();

        assert!((config.attestation.min_attester_trust - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.attestation.max_attestations_per_day, 20);
        assert!(!config.attestation.evidence_required);
        assert_eq!(config.propagation.max_path_length, 5);
        assert!((config.propagation.decay_factor - 0.9).abs() < f64::EPSILON);
        assert!(!config.sybil_resistance.enabled);
        // Other fields should use defaults
        assert_eq!(config.propagation.cache_ttl_secs, 300);
    }

    #[test]
    fn test_config_with_new_subsystems() {
        let config = Config::default();

        // Verify new configs are present and have defaults
        assert_eq!(config.gossip.replication.target_replicas, 3);
        assert_eq!(config.compute.max_concurrent_tasks, 10);
        assert!((config.trust.attestation.min_attester_trust - 0.3).abs() < f64::EPSILON);

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Note: Default values may not appear in serialized TOML due to #[serde(default)]
        // So we test deserialization to ensure defaults work correctly
        let deserialized: Config = toml::from_str(&toml_str).unwrap();

        // Verify defaults are properly restored
        assert_eq!(deserialized.gossip.replication.target_replicas, 3);
        assert_eq!(deserialized.compute.max_concurrent_tasks, 10);
        assert!(
            (deserialized.trust.attestation.min_attester_trust.value() - 0.3).abs() < f64::EPSILON
        );

        // Test with explicit values to ensure they serialize
        let mut custom_config = Config::default();
        custom_config.gossip.replication.target_replicas = 5;
        custom_config.compute.max_concurrent_tasks = 20;
        custom_config.trust.attestation.min_attester_trust = TrustScore::unchecked(0.5);

        let custom_toml = toml::to_string_pretty(&custom_config).unwrap();
        let custom_deserialized: Config = toml::from_str(&custom_toml).unwrap();

        assert_eq!(custom_deserialized.gossip.replication.target_replicas, 5);
        assert_eq!(custom_deserialized.compute.max_concurrent_tasks, 20);
        assert!(
            (custom_deserialized
                .trust
                .attestation
                .min_attester_trust
                .value()
                - 0.5)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn test_gossip_config_conversion() {
        let config = GossipConfig::default();

        // Test ReplicationConfig conversion
        let manager_config = config.replication.to_manager_config();
        assert_eq!(manager_config.target_replicas, 3);
        assert_eq!(manager_config.health_check_interval_secs, 60);
        assert_eq!(manager_config.stale_threshold_secs, 300);
        assert_eq!(manager_config.unreachable_threshold_secs, 900);

        // Test PartitionConfig conversion
        let partition_config = config.partition.to_gossip_config();
        assert_eq!(partition_config.partition_threshold.as_secs(), 300);
        assert_eq!(partition_config.check_interval.as_secs(), 30);
    }

    #[test]
    fn test_compute_config_conversion() {
        let config = ComputeConfig::default();

        // Test VerificationConfig conversion
        let verification = config.verification.to_compute_config();
        assert_eq!(verification.low_value_threshold, 100);
        assert_eq!(verification.medium_value_threshold, 1000);
        assert_eq!(verification.high_value_threshold, 10000);
        assert_eq!(verification.high_value_quorum, 3);
        assert!((verification.consensus_threshold - 0.67).abs() < f64::EPSILON);
        assert_eq!(verification.collection_window_ms, 30_000);
    }

    #[test]
    fn test_gossip_config_conversion_trust_class_semantic() {
        // Verify that default min_replica_trust (0.4) converts to the correct score
        let config = GossipConfig::default();
        let manager_config = config.replication.to_manager_config();

        // Default should be 0.4 (Partner threshold)
        assert_eq!(manager_config.min_trust_score, 0.4);

        // Test boundary cases
        let mut config_known = GossipConfig::default();
        config_known.replication.min_replica_trust = TrustScore::unchecked(0.3);
        let manager_known = config_known.replication.to_manager_config();
        assert_eq!(manager_known.min_trust_score, 0.3);

        let mut config_federated = GossipConfig::default();
        config_federated.replication.min_replica_trust = TrustScore::unchecked(0.8);
        let manager_federated = config_federated.replication.to_manager_config();
        assert_eq!(manager_federated.min_trust_score, 0.8);
    }
}
