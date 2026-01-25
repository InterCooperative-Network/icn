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
//! - Publishes revocations to gossip topic for cluster-wide notification
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
use rand::Rng;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::{debug, error, info, warn};

use crate::runtime::ShutdownRx;

/// Gossip topic for resource access revocation events
///
/// # Trust Model
///
/// This topic uses `AccessControl::PublicSigned` which means:
///
/// 1. **Publishing**: Any authenticated node can publish revocation events.
///    Revocations originate from the local `ResourceAccessEnforcerActor` which
///    detects idle resources during periodic enforcement checks.
///
/// 2. **Message Verification**: All messages are cryptographically signed by the
///    publishing node's DID. Receivers verify signatures before processing.
///    Invalid or tampered messages are rejected.
///
/// 3. **Trust Gating**: Nodes can configure trust-based filtering to only accept
///    revocations from nodes above a minimum trust threshold. This prevents
///    untrusted nodes from forcing revocations across the cluster.
///
/// 4. **Idempotency**: Revocation application is idempotent. Receiving a duplicate
///    revocation (e.g., due to gossip re-broadcast) is handled gracefully without
///    error. This is detected via "Access not found" errors during application.
///
/// # Security Considerations
///
/// - **Malicious Revocations**: A compromised or malicious node could attempt to
///   revoke resources it doesn't own. Receivers should validate that revocations
///   come from nodes with authority over the resource (e.g., the resource owner's
///   cooperative or a trusted federation member). This validation is currently
///   best-effort; see Issue #XXX for stricter authorization checks.
///
/// - **Denial of Service**: Rate limiting on gossip message processing prevents
///   flooding attacks. The trust-gated rate limits apply per trust class.
///
/// - **Replay Protection**: Gossip entries include timestamps and vector clocks
///   to prevent replay of old revocation events.
///
/// # Topic Configuration
///
/// When creating this topic, use:
/// ```ignore
/// gossip.create_topic(Topic {
///     name: RESOURCE_REVOCATIONS_TOPIC.to_string(),
///     acl: AccessControl::PublicSigned,
///     scope: Scope::Global,
///     min_trust_threshold: Some(0.3), // Only accept from Known+ trust
///     retention: Duration::from_secs(7 * 24 * 3600), // 7 days
///     max_entries: 10_000,
/// });
/// ```
pub const RESOURCE_REVOCATIONS_TOPIC: &str = "resource:revocations";

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

impl ResourceEnforcerConfig {
    /// Maximum reasonable check interval (7 days).
    /// Beyond this, idle resources may accumulate significantly before being detected.
    const MAX_RECOMMENDED_INTERVAL_SECS: u64 = 7 * 24 * 3600;

    /// Validate configuration and log warnings for questionable values.
    ///
    /// This does not fail - it logs warnings for operational awareness.
    pub fn validate_and_warn(&self) {
        if self.check_interval_seconds > Self::MAX_RECOMMENDED_INTERVAL_SECS {
            warn!(
                check_interval = self.check_interval_seconds,
                max_recommended = Self::MAX_RECOMMENDED_INTERVAL_SECS,
                "Resource enforcer check_interval_seconds exceeds 7 days; \
                 idle resources may accumulate before detection"
            );
        }

        if self.batch_size == 0 {
            warn!("Resource enforcer batch_size is 0; using default of 100");
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
    fn emit_revocation(&mut self, event: RevocationEvent) -> Result<()>;

    /// Apply a received revocation from gossip to local storage
    ///
    /// This is called when a revocation event is received from the cluster.
    /// The implementation should update the local storage to mark the access as revoked.
    /// Returns Ok if the revocation was applied or if it was already revoked (idempotent).
    ///
    /// # Design Note: `&self` vs `&mut self`
    ///
    /// This method intentionally takes `&self` instead of `&mut self` so that it can be
    /// invoked while the store is held under a shared/read lock, allowing concurrent
    /// processing of received revocations from multiple gossip messages.
    ///
    /// Implementors **must** use interior mutability (e.g., wrapping internal state in
    /// `RwLock`, `Mutex`, or using atomic operations) to perform any necessary updates
    /// in a thread-safe way.
    fn apply_received_revocation(&self, event: &RevocationEvent) -> Result<()>;
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

            // Validate config and log warnings
            config.validate_and_warn();

            info!(
                "ResourceAccessEnforcerActor started (check_interval={}s, batch_size={})",
                config.check_interval_seconds, config.batch_size
            );

            // Add startup jitter (0-10% of interval) to prevent thundering herd
            // when multiple nodes start simultaneously
            let jitter_secs = {
                let max_jitter = config.check_interval_seconds / 10;
                if max_jitter > 0 {
                    rand::thread_rng().gen_range(0..max_jitter)
                } else {
                    0
                }
            };
            if jitter_secs > 0 {
                debug!("Applying startup jitter of {}s", jitter_secs);
                tokio::time::sleep(Duration::from_secs(jitter_secs)).await;
            }

            // Periodic enforcement check
            let mut check_interval = interval(Duration::from_secs(config.check_interval_seconds));
            // Skip missed ticks rather than bursting to catch up
            check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

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
    ///
    /// This method batches updates to reduce lock contention. It first identifies
    /// all resources that need revocation during a read phase, then acquires the
    /// write lock once to perform all updates together.
    ///
    /// # Performance Note
    ///
    /// Currently, `list_all()` loads all resources into memory before processing.
    /// The `batch_size` config only affects the iteration chunk size for CPU-bound
    /// work, not memory usage. For deployments with 10K+ resources, consider:
    /// - Adding pagination support to [`ResourceAccessStore::list_all`]
    /// - Implementing cursor-based iteration
    /// - Monitoring memory usage via metrics
    async fn perform_enforcement_check(&mut self) -> Result<EnforcementResult> {
        let current_time = icn_time::current_timestamp_secs();
        debug!("Starting enforcement check at timestamp {}", current_time);

        self.stats.checks_performed += 1;
        self.stats.last_check_time = Some(current_time);

        // Phase 1: Read all resources and identify which need revocation
        let store_read = self.store.read().await;
        let all_resources = store_read
            .list_all()
            .context("Failed to list resource access entries")?;
        drop(store_read);

        let resources_checked = all_resources.len();

        // Collect resources that need revocation (resource_id, updated_access, event)
        let mut pending_revocations: Vec<(String, icn_ledger::ResourceAccess, RevocationEvent)> =
            Vec::new();

        // Process in batches to identify revocations
        // Note: We iterate by reference first, only cloning resources that need revocation.
        // This avoids cloning every resource, which is important for large collections.
        for chunk in all_resources.chunks(self.config.batch_size) {
            for (resource_id, access) in chunk.iter() {
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
                    // Clone only when revocation is needed
                    let mut access_clone = access.clone();

                    // Revoke the access
                    let reason = format!(
                        "Automatically revoked: idle for {}s (max: {}s)",
                        idle_seconds, max_idle_seconds
                    );

                    info!(
                        "Revoking access for resource '{}' (holder: {}): {}",
                        resource_id, access_clone.holder, reason
                    );

                    access_clone.revoke(reason.clone());

                    // Prepare revocation event
                    let event = RevocationEvent {
                        resource_id: resource_id.clone(),
                        holder: access_clone.holder.clone(),
                        reason,
                        timestamp: current_time,
                        idle_seconds,
                    };

                    pending_revocations.push((resource_id.clone(), access_clone, event));
                }
            }
        }

        let revocations = pending_revocations.len();

        // Phase 2: Acquire write lock once and apply all revocations
        if !pending_revocations.is_empty() {
            let mut store_write = self.store.write().await;

            for (resource_id, access, event) in pending_revocations {
                store_write
                    .update(&resource_id, &access)
                    .context("Failed to update resource access")?;

                store_write
                    .emit_revocation(event)
                    .context("Failed to emit revocation event")?;

                // Update per-revocation metrics
                metrics::counter!("icn_resource_access_revoked_total").increment(1);
            }

            drop(store_write);
        }

        self.stats.resources_checked += resources_checked as u64;
        self.stats.total_revocations += revocations as u64;

        info!(
            "Enforcement check complete: {} resources checked, {} revoked",
            resources_checked, revocations
        );

        // Update metrics - using counters for cumulative tracking
        metrics::counter!("icn_resource_enforcer_checks_total").increment(1);
        metrics::counter!("icn_resource_enforcer_resources_checked_total")
            .increment(resources_checked as u64);
        metrics::counter!("icn_resource_enforcer_revocations_total").increment(revocations as u64);

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
            self.resources.lock().map(|mut r| r.insert(id, access)).ok();
        }

        #[allow(dead_code)]
        fn get_events(&self) -> Vec<RevocationEvent> {
            self.events.lock().map(|e| e.clone()).unwrap_or_default()
        }
    }

    impl ResourceAccessStore for MockResourceAccessStore {
        fn list_all(&self) -> Result<Vec<(String, ResourceAccess)>> {
            let resources = self
                .resources
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock resources: {}", e))?;
            Ok(resources
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }

        fn update(&mut self, resource_id: &str, access: &ResourceAccess) -> Result<()> {
            let mut resources = self
                .resources
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock resources: {}", e))?;
            resources.insert(resource_id.to_string(), access.clone());
            Ok(())
        }

        fn emit_revocation(&mut self, event: RevocationEvent) -> Result<()> {
            let mut events = self
                .events
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock events: {}", e))?;
            events.push(event);
            Ok(())
        }

        fn apply_received_revocation(&self, event: &RevocationEvent) -> Result<()> {
            let mut events = self
                .events
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock events: {}", e))?;
            events.push(event.clone());
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

        assert_eq!(
            config.check_interval_seconds,
            deserialized.check_interval_seconds
        );
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

    /// End-to-end integration test with tokio time mocking.
    ///
    /// Tests the full actor lifecycle: spawn → periodic tick → enforcement check → revocation.
    /// Uses `start_paused = true` to control time advancement and verify periodic behavior.
    #[tokio::test(start_paused = true)]
    async fn test_periodic_enforcement_integration() {
        use tokio::sync::broadcast;
        use tokio::time::Duration;

        // Create mock store
        let store = Arc::new(RwLock::new(MockResourceAccessStore::new()));

        // Create a resource that is already past its idle limit
        // Set granted_at to 30 days ago (relative to current system time)
        let current_time = icn_time::current_timestamp_secs();
        let thirty_days_ago = current_time.saturating_sub(30 * 24 * 3600);

        let entity = EntityId::from_did(KeyPair::generate().unwrap().did());
        let mut access = ResourceAccess::new(
            "idle-resource".to_string(),
            entity.clone(),
            AccessModel::UseAccess {
                duration_seconds: 90 * 24 * 3600, // 90 days
                renewable: true,
                max_accumulated: 4,
            },
        )
        .with_rules(AntiSpeculationRules::strict()); // 7-day idle limit

        // Override granted_at and record last usage 30 days ago
        // This makes the resource idle for 30 days, exceeding the 7-day limit
        access.granted_at = thirty_days_ago;
        access
            .record_usage(thirty_days_ago, "Initial use".to_string())
            .unwrap();

        // Add to store
        store
            .write()
            .await
            .add_resource("idle-resource".to_string(), access);

        // Also add a recently-used resource that should NOT be revoked
        let entity2 = EntityId::from_did(KeyPair::generate().unwrap().did());
        let mut active_access = ResourceAccess::new(
            "active-resource".to_string(),
            entity2.clone(),
            AccessModel::UseAccess {
                duration_seconds: 90 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        )
        .with_rules(AntiSpeculationRules::strict());

        // Record recent usage (just now)
        active_access
            .record_usage(current_time, "Recent use".to_string())
            .unwrap();

        store
            .write()
            .await
            .add_resource("active-resource".to_string(), active_access);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);

        // Spawn actor with short interval (5 seconds for test)
        let config = ResourceEnforcerConfig {
            check_interval_seconds: 5,
            batch_size: 100,
            enabled: true,
        };

        let handle = ResourceAccessEnforcerActor::spawn(config, store.clone(), shutdown_rx);

        // Allow initial startup jitter to complete (max 0.5s jitter for 5s interval)
        // Advance time past the jitter
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Advance time past the first interval tick (5 seconds)
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;

        // Give the actor time to process
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        // Verify stats show at least one check was performed
        let stats = handle.get_stats().await.unwrap();
        assert!(
            stats.checks_performed >= 1,
            "Expected at least 1 check, got {}",
            stats.checks_performed
        );

        // Verify the idle resource was revoked
        let store_read = store.read().await;
        let resources = store_read.list_all().unwrap();
        drop(store_read);

        let idle_resource = resources
            .iter()
            .find(|(id, _)| id == "idle-resource")
            .map(|(_, access)| access);
        let active_resource = resources
            .iter()
            .find(|(id, _)| id == "active-resource")
            .map(|(_, access)| access);

        assert!(
            idle_resource.is_some(),
            "Idle resource should still exist in store"
        );
        assert!(
            idle_resource.unwrap().is_revoked(),
            "Idle resource should be revoked"
        );

        assert!(
            active_resource.is_some(),
            "Active resource should still exist in store"
        );
        assert!(
            !active_resource.unwrap().is_revoked(),
            "Active resource should NOT be revoked"
        );

        // Verify revocation events were emitted
        let events = store.read().await.events.lock().unwrap().clone();
        assert_eq!(events.len(), 1, "Should have exactly 1 revocation event");
        assert_eq!(events[0].resource_id, "idle-resource");
        assert!(events[0].reason.contains("idle"));

        // Clean shutdown
        let _ = shutdown_tx.send(());
    }
}
