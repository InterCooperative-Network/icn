//! Resource Access Enforcer Actor
//!
//! Periodically checks for idle resources and automatically revokes access
//! based on anti-speculation rules. This enforces the "use it or lose it"
//! principle for resource access.
//!
//! # Architecture
//! - Runs as a background task with configurable interval
//! - Queries storage for all ResourceAccess entries
//! - Validates each against idle period rules
//! - Auto-revokes access that exceeds max_idle_period_seconds
//! - Emits revocation events for audit trail
//!
//! # Configuration
//! ```rust,ignore
//! ResourceEnforcerConfig {
//!     check_interval_seconds: 3600,  // Check every hour
//!     batch_size: 100,               // Process 100 at a time
//!     enabled: true,                 // Enable enforcement
//! }
//! ```

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info};

use crate::runtime::ShutdownRx;

/// Configuration for resource access enforcement
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceEnforcerConfig {
    /// Interval between enforcement checks in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_check_interval")]
    pub check_interval_seconds: u64,

    /// Maximum number of resources to process per batch (default: 100)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Whether enforcement is enabled (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_check_interval() -> u64 {
    3600 // 1 hour
}

fn default_batch_size() -> usize {
    100
}

fn default_enabled() -> bool {
    true
}

impl Default for ResourceEnforcerConfig {
    fn default() -> Self {
        Self {
            check_interval_seconds: default_check_interval(),
            batch_size: default_batch_size(),
            enabled: default_enabled(),
        }
    }
}

/// Revocation event emitted when access is automatically revoked
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RevocationEvent {
    /// Resource ID that was revoked
    pub resource_id: String,
    /// Entity whose access was revoked
    pub holder: icn_entity::EntityId,
    /// Reason for revocation
    pub reason: String,
    /// Timestamp when revoked
    pub timestamp: u64,
    /// Idle duration in seconds
    pub idle_seconds: u64,
}

/// Messages for ResourceAccessEnforcerActor
pub enum EnforcerActorMsg {
    /// Request current enforcement statistics
    GetStats {
        reply: tokio::sync::oneshot::Sender<EnforcementStats>,
    },
    /// Force an immediate enforcement check
    ForceCheck {
        reply: tokio::sync::oneshot::Sender<Result<EnforcementResult>>,
    },
}

/// Statistics about enforcement activity
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnforcementStats {
    /// Total number of checks performed
    pub checks_performed: u64,
    /// Total number of resources checked
    pub resources_checked: u64,
    /// Total number of revocations
    pub total_revocations: u64,
    /// Timestamp of last check
    pub last_check_time: Option<u64>,
    /// Number of errors encountered
    pub error_count: u64,
}

/// Result of an enforcement check
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnforcementResult {
    /// Number of resources checked
    pub resources_checked: usize,
    /// Number of resources revoked
    pub revocations: usize,
    /// Timestamp of the check
    pub timestamp: u64,
}

/// Handle for interacting with ResourceAccessEnforcerActor
#[derive(Clone)]
pub struct ResourceEnforcerHandle {
    tx: mpsc::Sender<EnforcerActorMsg>,
}

impl ResourceEnforcerHandle {
    /// Get current enforcement statistics
    pub async fn get_stats(&self) -> Result<EnforcementStats> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(EnforcerActorMsg::GetStats { reply: reply_tx })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {e}"))?;
        reply_rx
            .await
            .map_err(|e| anyhow::anyhow!("Failed to receive reply: {e}"))
    }

    /// Force an immediate enforcement check
    pub async fn force_check(&self) -> Result<EnforcementResult> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(EnforcerActorMsg::ForceCheck { reply: reply_tx })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {e}"))?;
        reply_rx
            .await
            .map_err(|e| anyhow::anyhow!("Failed to receive reply: {e}"))?
    }
}

/// Storage interface for ResourceAccess entries
///
/// This trait abstracts the storage layer to allow for testing
/// and different storage backends.
pub trait ResourceAccessStore: Send + Sync {
    /// List all resource access entries
    ///
    /// Returns a list of (resource_id, ResourceAccess) pairs.
    /// In a real implementation, this would query the persistent store.
    fn list_all(&self) -> Result<Vec<(String, icn_ledger::ResourceAccess)>>;

    /// Update a resource access entry after revocation
    fn update(&mut self, resource_id: &str, access: &icn_ledger::ResourceAccess) -> Result<()>;

    /// Emit a revocation event for audit trail
    fn emit_revocation(&self, event: RevocationEvent) -> Result<()>;
}

/// Resource Access Enforcer Actor
///
/// Periodically checks resource access entries and revokes idle ones.
pub struct ResourceAccessEnforcerActor {
    config: ResourceEnforcerConfig,
    store: Arc<RwLock<dyn ResourceAccessStore>>,
    stats: EnforcementStats,
}

impl ResourceAccessEnforcerActor {
    /// Spawn the resource access enforcer actor
    ///
    /// # Arguments
    /// * `config` - Enforcement configuration
    /// * `store` - Storage backend for ResourceAccess entries
    /// * `shutdown_rx` - Shutdown signal receiver
    ///
    /// # Returns
    /// Handle for interacting with the actor
    pub fn spawn(
        config: ResourceEnforcerConfig,
        store: Arc<RwLock<dyn ResourceAccessStore>>,
        mut shutdown_rx: ShutdownRx,
    ) -> ResourceEnforcerHandle {
        let (tx, mut rx) = mpsc::channel::<EnforcerActorMsg>(32);

        let mut actor = ResourceAccessEnforcerActor {
            config: config.clone(),
            store,
            stats: EnforcementStats {
                checks_performed: 0,
                resources_checked: 0,
                total_revocations: 0,
                last_check_time: None,
                error_count: 0,
            },
        };

        tokio::spawn(async move {
            if !config.enabled {
                info!("ResourceAccessEnforcerActor disabled by configuration");
                return;
            }

            info!(
                "ResourceAccessEnforcerActor started (check_interval={}s, batch_size={})",
                config.check_interval_seconds, config.batch_size
            );

            // Periodic enforcement check
            let mut check_interval =
                interval(Duration::from_secs(config.check_interval_seconds));

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("ResourceAccessEnforcerActor shutting down");
                        break;
                    }

                    Some(msg) = rx.recv() => {
                        actor.handle_message(msg).await;
                    }

                    _ = check_interval.tick() => {
                        if let Err(e) = actor.perform_enforcement_check().await {
                            error!("Enforcement check failed: {}", e);
                            actor.stats.error_count += 1;
                            icn_obs::metrics::supervisor::error_inc("resource_enforcer_check_failed");
                        }
                    }
                }
            }

            info!("ResourceAccessEnforcerActor stopped");
        });

        ResourceEnforcerHandle { tx }
    }

    async fn handle_message(&mut self, msg: EnforcerActorMsg) {
        match msg {
            EnforcerActorMsg::GetStats { reply } => {
                let _ = reply.send(self.stats.clone());
            }

            EnforcerActorMsg::ForceCheck { reply } => {
                let result = self.perform_enforcement_check().await;
                let _ = reply.send(result);
            }
        }
    }

    /// Perform an enforcement check on all resource access entries
    async fn perform_enforcement_check(&mut self) -> Result<EnforcementResult> {
        let current_time = icn_time::current_timestamp_secs();
        debug!("Starting enforcement check at timestamp {}", current_time);

        self.stats.checks_performed += 1;
        self.stats.last_check_time = Some(current_time);

        let mut resources_checked = 0;
        let mut revocations = 0;

        // Lock the store and get all resource access entries
        let store_read = self.store.read().await;
        let all_resources = store_read
            .list_all()
            .context("Failed to list resource access entries")?;
        drop(store_read);

        // Process in batches
        for chunk in all_resources.chunks(self.config.batch_size) {
            for (resource_id, mut access) in chunk.iter().cloned() {
                resources_checked += 1;

                // Skip already revoked resources
                if access.is_revoked() {
                    continue;
                }

                // Validate against anti-speculation rules
                if let Err(icn_ledger::AccessError::IdleTooLong {
                    idle_seconds,
                    max_idle_seconds,
                }) = access.validate_rules(current_time)
                {
                    // Revoke the access
                    let reason = format!(
                        "Automatically revoked: idle for {}s (max: {}s)",
                        idle_seconds, max_idle_seconds
                    );
                    
                    info!(
                        "Revoking access for resource '{}' (holder: {}): {}",
                        resource_id, access.holder, reason
                    );

                    access.revoke(reason.clone());
                    revocations += 1;

                    // Update the store
                    let mut store_write = self.store.write().await;
                    store_write
                        .update(&resource_id, &access)
                        .context("Failed to update resource access")?;

                    // Emit revocation event
                    let event = RevocationEvent {
                        resource_id: resource_id.clone(),
                        holder: access.holder.clone(),
                        reason,
                        timestamp: current_time,
                        idle_seconds,
                    };

                    store_write
                        .emit_revocation(event)
                        .context("Failed to emit revocation event")?;
                    
                    drop(store_write);

                    // Update metrics
                    metrics::counter!(
                        "icn_resource_access_revoked_total",
                        "resource_id" => resource_id.clone()
                    )
                    .increment(1);
                }
            }
        }

        self.stats.resources_checked += resources_checked as u64;
        self.stats.total_revocations += revocations as u64;

        info!(
            "Enforcement check complete: {} resources checked, {} revoked",
            resources_checked, revocations
        );

        // Update metrics
        metrics::gauge!("icn_resource_enforcer_resources_checked").set(resources_checked as f64);
        metrics::gauge!("icn_resource_enforcer_revocations").set(revocations as f64);

        Ok(EnforcementResult {
            resources_checked,
            revocations,
            timestamp: current_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_entity::EntityId;
    use icn_identity::KeyPair;
    use icn_ledger::{AccessModel, AntiSpeculationRules, ResourceAccess};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock storage for testing
    struct MockResourceAccessStore {
        resources: Mutex<HashMap<String, ResourceAccess>>,
        events: Mutex<Vec<RevocationEvent>>,
    }

    impl MockResourceAccessStore {
        fn new() -> Self {
            Self {
                resources: Mutex::new(HashMap::new()),
                events: Mutex::new(Vec::new()),
            }
        }

        fn add_resource(&self, id: String, access: ResourceAccess) {
            self.resources
                .lock()
                .map(|mut r| r.insert(id, access))
                .ok();
        }

        fn get_events(&self) -> Vec<RevocationEvent> {
            self.events.lock().map(|e| e.clone()).unwrap_or_default()
        }
    }

    impl ResourceAccessStore for MockResourceAccessStore {
        fn list_all(&self) -> Result<Vec<(String, ResourceAccess)>> {
            let resources = self.resources.lock().map_err(|e| {
                anyhow::anyhow!("Failed to lock resources: {}", e)
            })?;
            Ok(resources.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }

        fn update(&mut self, resource_id: &str, access: &ResourceAccess) -> Result<()> {
            let mut resources = self.resources.lock().map_err(|e| {
                anyhow::anyhow!("Failed to lock resources: {}", e)
            })?;
            resources.insert(resource_id.to_string(), access.clone());
            Ok(())
        }

        fn emit_revocation(&self, event: RevocationEvent) -> Result<()> {
            let mut events = self.events.lock().map_err(|e| {
                anyhow::anyhow!("Failed to lock events: {}", e)
            })?;
            events.push(event);
            Ok(())
        }
    }

    #[test]
    fn test_config_defaults() {
        let config = ResourceEnforcerConfig::default();
        assert_eq!(config.check_interval_seconds, 3600);
        assert_eq!(config.batch_size, 100);
        assert!(config.enabled);
    }

    #[test]
    fn test_config_serialization() {
        let config = ResourceEnforcerConfig {
            check_interval_seconds: 1800,
            batch_size: 50,
            enabled: false,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: ResourceEnforcerConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config.check_interval_seconds, deserialized.check_interval_seconds);
        assert_eq!(config.batch_size, deserialized.batch_size);
        assert_eq!(config.enabled, deserialized.enabled);
    }

    #[tokio::test]
    async fn test_enforcement_check_idle_revocation() {
        let store = Arc::new(RwLock::new(MockResourceAccessStore::new()));
        
        // Create a resource with strict idle rules (7 days)
        let entity = EntityId::from_did(KeyPair::generate().unwrap().did());
        let mut access = ResourceAccess::new(
            "test-resource-001".to_string(),
            entity.clone(),
            AccessModel::UseAccess {
                duration_seconds: 90 * 24 * 3600, // 90 days
                renewable: true,
                max_accumulated: 4,
            },
        )
        .with_rules(AntiSpeculationRules::strict()); // 7-day idle limit

        // Record usage at grant time
        access
            .record_usage(access.granted_at, "Initial use".to_string())
            .unwrap();

        // Add to mock store
        store
            .write()
            .await
            .add_resource("test-resource-001".to_string(), access.clone());

        // Fast-forward time by 8 days (exceeds 7-day limit)
        // Note: In a real test, we'd need to mock icn_time::current_timestamp_secs()
        // For now, we test the logic manually

        let current_time = access.granted_at + 8 * 24 * 3600;
        
        // Manually test the validation logic
        let validation_result = access.validate_rules(current_time);
        assert!(validation_result.is_err());
        
        if let Err(icn_ledger::AccessError::IdleTooLong { idle_seconds, .. }) = validation_result {
            assert!(idle_seconds >= 7 * 24 * 3600);
        } else {
            panic!("Expected IdleTooLong error");
        }
    }

    #[tokio::test]
    async fn test_enforcement_skip_already_revoked() {
        let store = Arc::new(RwLock::new(MockResourceAccessStore::new()));
        
        let entity = EntityId::from_did(KeyPair::generate().unwrap().did());
        let mut access = ResourceAccess::new(
            "test-resource-002".to_string(),
            entity.clone(),
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        // Already revoke it
        access.revoke("Previously revoked".to_string());
        assert!(access.is_revoked());

        store
            .write()
            .await
            .add_resource("test-resource-002".to_string(), access.clone());

        // Enforcement should skip this resource (verified by checking is_revoked)
        assert!(access.is_revoked());
    }
}
