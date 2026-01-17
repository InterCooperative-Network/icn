//! Storage maintenance and monitoring for Sled databases
//!
//! Provides periodic maintenance tasks to optimize storage performance:
//! - Periodic flush to ensure durability
//! - Space amplification monitoring
//! - Configurable maintenance schedule
//!
//! Note: Sled uses a lock-free B+ tree with automatic garbage collection.
//! This module provides additional maintenance operations (flush, monitoring)
//! but does not perform manual compaction as Sled handles this internally.

use crate::SledStore;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::{debug, info, warn};

/// Configuration for storage maintenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceConfig {
    /// Enable periodic maintenance
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Interval between maintenance runs in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,

    /// Space amplification threshold to trigger warning (default: 2.0)
    #[serde(default = "default_amplification_threshold")]
    pub amplification_threshold: f64,

    /// Size threshold in bytes to trigger maintenance (default: 1GB)
    #[serde(default = "default_size_threshold_bytes")]
    pub size_threshold_bytes: u64,

    /// Flush to disk during maintenance (default: true)
    #[serde(default = "default_flush_on_maintenance")]
    pub flush_on_maintenance: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_interval_secs() -> u64 {
    3600 // 1 hour
}

fn default_amplification_threshold() -> f64 {
    2.0
}

fn default_size_threshold_bytes() -> u64 {
    1024 * 1024 * 1024 // 1 GB
}

fn default_flush_on_maintenance() -> bool {
    true
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            interval_secs: default_interval_secs(),
            amplification_threshold: default_amplification_threshold(),
            size_threshold_bytes: default_size_threshold_bytes(),
            flush_on_maintenance: default_flush_on_maintenance(),
        }
    }
}

/// Result of a maintenance run
#[derive(Debug, Clone)]
pub struct MaintenanceResult {
    /// Duration of the maintenance operation
    pub duration: Duration,
    /// Size on disk before maintenance
    pub size_before: u64,
    /// Size on disk after maintenance
    pub size_after: u64,
    /// Space amplification before maintenance
    pub amplification_before: f64,
    /// Space amplification after maintenance
    pub amplification_after: f64,
    /// Bytes flushed to disk (if flush was performed)
    pub bytes_flushed: Option<usize>,
    /// Whether any warnings were generated
    pub warnings: Vec<String>,
}

impl MaintenanceResult {
    /// Calculate space reclaimed (can be negative if database grew)
    pub fn space_change(&self) -> i64 {
        self.size_before as i64 - self.size_after as i64
    }
}

/// Handle for controlling the maintenance task
pub struct MaintenanceHandle {
    shutdown: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl MaintenanceHandle {
    /// Trigger an immediate maintenance run
    pub fn trigger(&self) {
        self.notify.notify_one();
    }

    /// Stop the maintenance task
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }
}

/// Storage maintenance manager
pub struct MaintenanceManager {
    store: Arc<SledStore>,
    config: MaintenanceConfig,
}

impl MaintenanceManager {
    /// Create a new maintenance manager
    pub fn new(store: Arc<SledStore>, config: MaintenanceConfig) -> Self {
        Self { store, config }
    }

    /// Run a single maintenance cycle
    pub fn run_maintenance(&self) -> Result<MaintenanceResult> {
        let start = Instant::now();
        let mut warnings = Vec::new();

        // Get stats before maintenance
        let size_before = self.store.size_on_disk()?;
        let amplification_before = self.store.db().space_amplification().unwrap_or(1.0);

        // Check thresholds and generate warnings
        if amplification_before > self.config.amplification_threshold {
            let msg = format!(
                "Space amplification ({:.2}) exceeds threshold ({:.2})",
                amplification_before, self.config.amplification_threshold
            );
            warn!("{}", msg);
            warnings.push(msg);
        }

        if size_before > self.config.size_threshold_bytes {
            let msg = format!(
                "Database size ({} MB) exceeds threshold ({} MB)",
                size_before / (1024 * 1024),
                self.config.size_threshold_bytes / (1024 * 1024)
            );
            warn!("{}", msg);
            warnings.push(msg);
        }

        // Perform flush if configured
        let bytes_flushed = if self.config.flush_on_maintenance {
            Some(self.store.flush()?)
        } else {
            None
        };

        // Get stats after maintenance
        let size_after = self.store.size_on_disk()?;
        let amplification_after = self.store.db().space_amplification().unwrap_or(1.0);

        let duration = start.elapsed();

        // Record metrics
        record_maintenance_metrics(duration, size_before, size_after);

        let result = MaintenanceResult {
            duration,
            size_before,
            size_after,
            amplification_before,
            amplification_after,
            bytes_flushed,
            warnings,
        };

        info!(
            duration_ms = duration.as_millis(),
            size_before_mb = size_before / (1024 * 1024),
            size_after_mb = size_after / (1024 * 1024),
            amplification = amplification_after,
            "Storage maintenance completed"
        );

        Ok(result)
    }

    /// Start the background maintenance task
    ///
    /// Returns a handle that can be used to trigger immediate maintenance
    /// or stop the task.
    pub fn start_background_task(self: Arc<Self>) -> MaintenanceHandle {
        let shutdown = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(Notify::new());

        let handle = MaintenanceHandle {
            shutdown: Arc::clone(&shutdown),
            notify: Arc::clone(&notify),
        };

        let manager = Arc::clone(&self);
        let interval = Duration::from_secs(self.config.interval_secs);

        tokio::spawn(async move {
            info!(
                interval_secs = manager.config.interval_secs,
                "Starting storage maintenance task"
            );

            loop {
                // Wait for either the interval or a notification
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = notify.notified() => {
                        if shutdown.load(Ordering::SeqCst) {
                            info!("Storage maintenance task shutting down");
                            break;
                        }
                        debug!("Manual maintenance triggered");
                    }
                }

                // Check shutdown before running maintenance
                if shutdown.load(Ordering::SeqCst) {
                    info!("Storage maintenance task shutting down");
                    break;
                }

                // Run maintenance
                match manager.run_maintenance() {
                    Ok(result) => {
                        if !result.warnings.is_empty() {
                            warn!(
                                warnings = ?result.warnings,
                                "Maintenance completed with warnings"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Maintenance failed");
                        icn_obs::metrics::storage::maintenance_errors_inc();
                    }
                }
            }
        });

        handle
    }
}

/// Record maintenance metrics
fn record_maintenance_metrics(duration: Duration, size_before: u64, size_after: u64) {
    icn_obs::metrics::storage::maintenance_runs_inc();
    icn_obs::metrics::storage::maintenance_duration_record(duration.as_secs_f64());

    let space_change = size_before as i64 - size_after as i64;
    if space_change > 0 {
        icn_obs::metrics::storage::maintenance_space_reclaimed_add(space_change as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maintenance_config_defaults() {
        let config = MaintenanceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.interval_secs, 3600);
        assert_eq!(config.amplification_threshold, 2.0);
        assert_eq!(config.size_threshold_bytes, 1024 * 1024 * 1024);
        assert!(config.flush_on_maintenance);
    }

    #[test]
    fn test_maintenance_config_serde() {
        let json = r#"{
            "enabled": false,
            "interval_secs": 7200,
            "amplification_threshold": 3.0
        }"#;

        let config: MaintenanceConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.interval_secs, 7200);
        assert_eq!(config.amplification_threshold, 3.0);
        // Defaults for missing fields
        assert_eq!(config.size_threshold_bytes, default_size_threshold_bytes());
    }

    #[test]
    fn test_maintenance_result_space_change() {
        let result = MaintenanceResult {
            duration: Duration::from_secs(1),
            size_before: 1000,
            size_after: 800,
            amplification_before: 2.0,
            amplification_after: 1.5,
            bytes_flushed: Some(100),
            warnings: vec![],
        };

        // Space reclaimed: 1000 - 800 = 200
        assert_eq!(result.space_change(), 200);

        // Negative space change (database grew)
        let result2 = MaintenanceResult {
            size_before: 800,
            size_after: 1000,
            ..result
        };
        assert_eq!(result2.space_change(), -200);
    }

    #[tokio::test]
    async fn test_run_maintenance() {
        let store = Arc::new(SledStore::temporary().unwrap());

        // Put some data
        store.db().insert(b"key1", b"value1").unwrap();
        store.db().insert(b"key2", b"value2").unwrap();

        let manager = MaintenanceManager::new(store, MaintenanceConfig::default());
        let result = manager.run_maintenance().unwrap();

        // Maintenance completed in reasonable time
        assert!(result.duration < Duration::from_secs(10));
        assert!(result.size_after > 0);
        assert!(result.bytes_flushed.is_some());
    }

    #[test]
    fn test_maintenance_handle_stop() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(Notify::new());

        let handle = MaintenanceHandle {
            shutdown: Arc::clone(&shutdown),
            notify,
        };

        assert!(!shutdown.load(Ordering::SeqCst));
        handle.stop();
        assert!(shutdown.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_background_task_lifecycle() {
        let store = Arc::new(SledStore::temporary().unwrap());

        // Put some data to make maintenance meaningful
        store.db().insert(b"key1", b"value1").unwrap();

        // Use short interval for testing
        let config = MaintenanceConfig {
            enabled: true,
            interval_secs: 1, // 1 second for fast testing
            ..MaintenanceConfig::default()
        };

        let manager = Arc::new(MaintenanceManager::new(store, config));
        let handle = manager.start_background_task();

        // Give the task time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Trigger immediate maintenance
        handle.trigger();

        // Wait a bit for the triggered maintenance to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Stop the task
        handle.stop();

        // Give the task time to shut down
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify the task actually stopped (can't directly verify, but no crash is good)
    }
}
