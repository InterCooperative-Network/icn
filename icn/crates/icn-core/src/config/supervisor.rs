//! Supervisor configuration for background tasks and timeouts

use icn_store::MaintenanceConfig;
use serde::{Deserialize, Serialize};

/// Supervisor configuration for background tasks and timeouts (A5 fix)
///
/// Centralizes previously hardcoded values from supervisor.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    /// Candidate cache cleanup interval in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_candidate_cleanup_interval_secs")]
    pub candidate_cleanup_interval_secs: u64,

    /// Delay before requesting peer exchange in milliseconds (default: 500)
    #[serde(default = "default_peer_exchange_delay_ms")]
    pub peer_exchange_delay_ms: u64,

    /// Maximum peers to request in peer exchange (default: 50)
    #[serde(default = "default_peer_exchange_max_peers")]
    pub peer_exchange_max_peers: usize,

    /// Metrics update interval in seconds (default: 10)
    #[serde(default = "default_metrics_update_interval_secs")]
    pub metrics_update_interval_secs: u64,

    /// Graceful shutdown timeout in seconds (default: 5)
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,

    /// Clock synchronization interval in seconds (default: 600 = 10 minutes)
    #[serde(default = "default_clock_sync_interval_secs")]
    pub clock_sync_interval_secs: u64,

    /// Actor restart policy configuration
    #[serde(default)]
    pub restart_policy: RestartPolicyConfig,

    /// Storage maintenance configuration (compaction, cleanup)
    #[serde(default)]
    pub storage_maintenance: MaintenanceConfig,

    /// Resource access enforcer configuration (idle revocation)
    #[serde(default)]
    pub resource_enforcer: crate::resource_enforcer_actor::ResourceEnforcerConfig,
}

/// Configuration for actor restart with exponential backoff.
///
/// When an actor task fails, the supervisor can restart it with configurable
/// backoff to prevent rapid restart loops that exhaust resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartPolicyConfig {
    /// Initial delay before first restart attempt in milliseconds (default: 100)
    #[serde(default = "default_restart_initial_delay_ms")]
    pub initial_delay_ms: u64,

    /// Maximum delay between restart attempts in milliseconds (default: 30000 = 30s)
    #[serde(default = "default_restart_max_delay_ms")]
    pub max_delay_ms: u64,

    /// Backoff multiplier applied to delay after each failure (default: 2.0)
    #[serde(default = "default_restart_backoff_multiplier")]
    pub backoff_multiplier: f64,

    /// Maximum restart attempts within the restart window (default: 5)
    #[serde(default = "default_restart_max_attempts")]
    pub max_attempts: u32,

    /// Time window for counting restart attempts in seconds (default: 60)
    /// If max_attempts is exceeded within this window, the actor is not restarted.
    #[serde(default = "default_restart_window_secs")]
    pub restart_window_secs: u64,
}

fn default_restart_initial_delay_ms() -> u64 {
    100
}

fn default_restart_max_delay_ms() -> u64 {
    30_000 // 30 seconds
}

fn default_restart_backoff_multiplier() -> f64 {
    2.0
}

fn default_restart_max_attempts() -> u32 {
    5
}

fn default_restart_window_secs() -> u64 {
    60
}

impl Default for RestartPolicyConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: default_restart_initial_delay_ms(),
            max_delay_ms: default_restart_max_delay_ms(),
            backoff_multiplier: default_restart_backoff_multiplier(),
            max_attempts: default_restart_max_attempts(),
            restart_window_secs: default_restart_window_secs(),
        }
    }
}

fn default_candidate_cleanup_interval_secs() -> u64 {
    300 // 5 minutes
}

fn default_peer_exchange_delay_ms() -> u64 {
    500
}

fn default_peer_exchange_max_peers() -> usize {
    50
}

fn default_metrics_update_interval_secs() -> u64 {
    10
}

fn default_shutdown_timeout_secs() -> u64 {
    5
}

fn default_clock_sync_interval_secs() -> u64 {
    600 // 10 minutes
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        SupervisorConfig {
            candidate_cleanup_interval_secs: default_candidate_cleanup_interval_secs(),
            peer_exchange_delay_ms: default_peer_exchange_delay_ms(),
            peer_exchange_max_peers: default_peer_exchange_max_peers(),
            metrics_update_interval_secs: default_metrics_update_interval_secs(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            clock_sync_interval_secs: default_clock_sync_interval_secs(),
            restart_policy: RestartPolicyConfig::default(),
            storage_maintenance: MaintenanceConfig::default(),
            resource_enforcer: crate::resource_enforcer_actor::ResourceEnforcerConfig::default(),
        }
    }
}
