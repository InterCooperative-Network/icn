//! Gateway API configuration

use serde::{Deserialize, Serialize};

use super::observability::AuditRetentionConfig;

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

    /// Audit retention policy configuration
    #[serde(default)]
    pub audit_retention: AuditRetentionConfig,

    /// Default trust score for unknown peers (0.0 to 1.0)
    #[serde(default)]
    pub default_trust_score: Option<f64>,
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
            audit_retention: AuditRetentionConfig::default(),
            default_trust_score: None,
        }
    }
}
