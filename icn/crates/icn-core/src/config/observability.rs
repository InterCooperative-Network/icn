//! Observability and monitoring configuration

use serde::{Deserialize, Serialize};

use super::network::default_true;

/// Observability and monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Metrics server port
    pub metrics_port: u16,

    /// Health check port
    pub health_port: u16,

    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// Distributed tracing configuration
    #[serde(default)]
    pub tracing: TracingConfig,
}

/// Distributed tracing configuration with OpenTelemetry
///
/// # Priority Sampling
///
/// ICN uses priority-based sampling to ensure important traces are always captured
/// while reducing overhead for routine operations:
///
/// - **Errors**: Always sampled (set `always_sample_errors = true`)
/// - **Slow requests**: Always sampled if slower than `slow_threshold_ms`
/// - **Security events**: Always sampled (spans with "security" in name)
/// - **Normal operations**: Sampled at `sampling_rate`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable distributed tracing export
    #[serde(default)]
    pub enabled: bool,

    /// OTLP endpoint for trace export (e.g., "http://localhost:4317")
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Sampling rate (0.0 to 1.0). Default is 0.1 (10%) for production.
    /// Note: Error and slow traces are sampled regardless of this rate
    /// when `always_sample_errors` and `always_sample_slow` are enabled.
    #[serde(default = "default_sampling_rate")]
    pub sampling_rate: f64,

    /// Always sample traces that contain errors.
    /// This ensures debugging information is captured for all failures.
    #[serde(default = "default_always_sample_errors")]
    pub always_sample_errors: bool,

    /// Always sample traces that exceed `slow_threshold_ms`.
    /// This ensures performance issues are captured.
    #[serde(default = "default_always_sample_slow")]
    pub always_sample_slow: bool,

    /// Threshold (in milliseconds) for considering a request "slow".
    /// Traces exceeding this duration are always sampled if `always_sample_slow` is true.
    #[serde(default = "default_slow_threshold_ms")]
    pub slow_threshold_ms: u64,

    /// Service name for traces
    #[serde(default = "default_service_name")]
    pub service_name: String,
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_sampling_rate() -> f64 {
    0.1 // 10% sampling for production
}

fn default_always_sample_errors() -> bool {
    true // Always capture errors by default
}

fn default_always_sample_slow() -> bool {
    true // Always capture slow requests by default
}

fn default_slow_threshold_ms() -> u64 {
    1000 // 1 second
}

fn default_service_name() -> String {
    "icnd".to_string()
}

impl Default for TracingConfig {
    fn default() -> Self {
        TracingConfig {
            enabled: false,
            otlp_endpoint: default_otlp_endpoint(),
            sampling_rate: default_sampling_rate(),
            always_sample_errors: default_always_sample_errors(),
            always_sample_slow: default_always_sample_slow(),
            slow_threshold_ms: default_slow_threshold_ms(),
            service_name: default_service_name(),
        }
    }
}

impl TracingConfig {
    /// Convert to icn_obs TracingConfig
    pub fn to_obs_config(&self, node_did: Option<String>) -> icn_obs::TracingConfig {
        icn_obs::TracingConfig {
            enabled: self.enabled,
            otlp_endpoint: self.otlp_endpoint.clone(),
            sampling_rate: self.sampling_rate,
            always_sample_errors: self.always_sample_errors,
            always_sample_slow: self.always_sample_slow,
            slow_threshold_ms: self.slow_threshold_ms,
            service_name: self.service_name.clone(),
            node_did,
        }
    }
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

impl RateLimitingConfig {
    /// Convert to trust-gated rate limit config for icn-net
    ///
    /// # Arguments
    /// * `min_trust_threshold` - Minimum trust score required for TLS connections (from network config)
    pub fn to_trust_gated_config(
        &self,
        min_trust_threshold: f64,
    ) -> icn_net::TrustGatedRateLimitConfig {
        use std::time::Duration;

        let refill_interval = Duration::from_millis(self.refill_interval_ms);

        icn_net::TrustGatedRateLimitConfig {
            isolated: self.isolated.to_rate_limit_config(refill_interval),
            known: self.known.to_rate_limit_config(refill_interval),
            partner: self.partner.to_rate_limit_config(refill_interval),
            federated: self.federated.to_rate_limit_config(refill_interval),
            refill_interval,
            // Use the configurable trust threshold from network config
            min_trust_threshold,
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

// Audit retention default value functions
fn default_retention_days() -> u64 {
    365
}
fn default_max_records_per_entity() -> usize {
    10000
}
fn default_prune_interval_secs() -> u64 {
    3600
}
fn default_prune_batch_size() -> usize {
    1000
}

/// Audit retention configuration for entity audit records
///
/// Controls how long audit records are kept and how pruning is performed.
/// This prevents unbounded storage growth in high-activity cooperatives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRetentionConfig {
    /// Maximum age of audit records in days (default: 365)
    /// Records older than this are eligible for pruning
    #[serde(default = "default_retention_days")]
    pub retention_days: u64,

    /// Maximum number of audit records per entity (default: 10000)
    /// Oldest records beyond this limit are eligible for pruning
    #[serde(default = "default_max_records_per_entity")]
    pub max_records_per_entity: usize,

    /// Interval between automatic pruning runs in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_prune_interval_secs")]
    pub prune_interval_secs: u64,

    /// Enable automatic background pruning (default: true)
    #[serde(default = "default_true")]
    pub auto_prune_enabled: bool,

    /// Batch size for pruning operations (default: 1000)
    /// Larger batches are more efficient but may cause longer lock times
    #[serde(default = "default_prune_batch_size")]
    pub prune_batch_size: usize,
}

impl Default for AuditRetentionConfig {
    fn default() -> Self {
        Self {
            retention_days: default_retention_days(),
            max_records_per_entity: default_max_records_per_entity(),
            prune_interval_secs: default_prune_interval_secs(),
            auto_prune_enabled: true,
            prune_batch_size: default_prune_batch_size(),
        }
    }
}
