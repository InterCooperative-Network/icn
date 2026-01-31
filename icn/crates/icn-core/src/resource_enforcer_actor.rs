//! Resource Access Enforcer Actor
//!
//! Periodically checks for idle resources and automatically revokes access
//! based on anti-speculation rules. This enforces the "use it or lose it"
//! principle for resource access.
//!
//! # Architecture
//! - Runs as a background task with configurable interval
//! - Queries the LedgerService for enforceable resources with idle violations
//! - Revokes resources that exceed max_idle_period_seconds
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

use anyhow::Result;
use rand::Rng;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::{debug, error, info, warn};

use icn_kernel_api::services::LedgerService;
use icn_kernel_api::types::Did;

use crate::runtime::ShutdownRx;

/// Gossip topic for resource access revocation events
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
    /// DID of the entity whose access was revoked
    pub holder: Did,
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

/// Dependencies for spawning the resource enforcer actor
pub struct ResourceEnforcerDeps {
    /// The ledger service used to query and revoke resource access
    pub ledger_service: Arc<dyn LedgerService>,
    /// Optional gossip handle for publishing revocations cluster-wide
    pub gossip_handle: Option<Arc<tokio::sync::RwLock<icn_gossip::GossipActor>>>,
}

/// Resource Access Enforcer Actor
///
/// Periodically checks resource access entries and revokes idle ones
/// using the LedgerService interface.
pub struct ResourceAccessEnforcerActor {
    config: ResourceEnforcerConfig,
    deps: ResourceEnforcerDeps,
    stats: EnforcementStats,
}

impl ResourceAccessEnforcerActor {
    /// Spawn the resource access enforcer actor
    ///
    /// # Arguments
    /// * `config` - Enforcement configuration
    /// * `deps` - Dependencies (LedgerService, optional gossip)
    /// * `shutdown_rx` - Shutdown signal receiver
    ///
    /// # Returns
    /// Handle for interacting with the actor
    pub fn spawn(
        config: ResourceEnforcerConfig,
        deps: ResourceEnforcerDeps,
        mut shutdown_rx: ShutdownRx,
    ) -> ResourceEnforcerHandle {
        let (tx, mut rx) = mpsc::channel::<EnforcerActorMsg>(32);

        let mut actor = ResourceAccessEnforcerActor {
            config: config.clone(),
            deps,
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

    /// Perform an enforcement check using LedgerService
    ///
    /// Queries the ledger for enforceable resources, identifies idle violations,
    /// revokes violating resources, and publishes revocation events.
    async fn perform_enforcement_check(&mut self) -> Result<EnforcementResult> {
        let current_time = icn_time::current_timestamp_secs();
        debug!("Starting enforcement check at timestamp {}", current_time);

        self.stats.checks_performed += 1;
        self.stats.last_check_time = Some(current_time);

        // Query LedgerService for all enforceable resources with idle status
        let resources = self
            .deps
            .ledger_service
            .list_enforceable_resources(current_time)
            .map_err(|e| anyhow::anyhow!("Failed to list enforceable resources: {e}"))?;

        let resources_checked = resources.len();

        // Identify and process resources with idle violations
        let mut revocations = 0;

        for chunk in resources.chunks(self.config.batch_size) {
            for resource in chunk {
                if resource.is_revoked {
                    continue;
                }

                if let Some(ref violation) = resource.idle_violation {
                    let reason = format!(
                        "Automatically revoked: idle for {}s (max: {}s)",
                        violation.idle_seconds, violation.max_idle_seconds
                    );

                    info!(
                        "Revoking access for resource '{}' (holder: {}): {}",
                        resource.resource_id, resource.holder, reason
                    );

                    // Revoke via LedgerService
                    let req = icn_kernel_api::services::RevokeResourceAccessRequest {
                        resource_id: resource.resource_id.clone(),
                        reason: reason.clone(),
                    };

                    if let Err(e) = self.deps.ledger_service.revoke_resource_access(&req) {
                        warn!(
                            "Failed to revoke resource '{}': {}",
                            resource.resource_id, e
                        );
                        continue;
                    }

                    // Publish revocation event to gossip
                    let event = RevocationEvent {
                        resource_id: resource.resource_id.clone(),
                        holder: resource.holder.clone(),
                        reason,
                        timestamp: current_time,
                        idle_seconds: violation.idle_seconds,
                    };

                    self.publish_revocation(&event).await;

                    revocations += 1;
                    metrics::counter!("icn_resource_access_revoked_total").increment(1);
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

    /// Publish a revocation event to gossip for cluster-wide notification.
    ///
    /// Best-effort: if gossip is unavailable, the event is logged but not retried.
    async fn publish_revocation(&self, event: &RevocationEvent) {
        let gossip_handle = match &self.deps.gossip_handle {
            Some(h) => h.clone(),
            None => return,
        };

        let serialized = match serde_json::to_vec(event) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to serialize revocation event: {}", e);
                metrics::counter!(
                    "icn_resource_revocation_gossip_failures_total",
                    "reason" => "serialization"
                )
                .increment(1);
                return;
            }
        };

        let mut gossip = gossip_handle.write().await;
        if let Err(e) = gossip.publish(RESOURCE_REVOCATIONS_TOPIC, serialized).await {
            warn!("Failed to publish revocation to gossip: {}", e);
            metrics::counter!(
                "icn_resource_revocation_gossip_failures_total",
                "reason" => "publish"
            )
            .increment(1);
        } else {
            info!(
                resource_id = %event.resource_id,
                holder = %event.holder,
                "Published revocation event to gossip"
            );
            metrics::counter!("icn_resource_revocation_gossip_published_total").increment(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_revocation_event_serialization() {
        let event = RevocationEvent {
            resource_id: "test-resource".to_string(),
            holder: "did:icn:abc123".to_string(),
            reason: "Idle too long".to_string(),
            timestamp: 1000000,
            idle_seconds: 86400,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: RevocationEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.resource_id, deserialized.resource_id);
        assert_eq!(event.holder, deserialized.holder);
        assert_eq!(event.idle_seconds, deserialized.idle_seconds);
    }

    /// Verify that the actor works correctly when gossip_handle is None.
    /// Enforcement should still occur; revocations are just not broadcast.
    #[tokio::test]
    async fn test_enforce_without_gossip() {
        use icn_kernel_api::services::{LedgerService, ResourceAccessInfo};

        struct StubLedger;
        impl LedgerService for StubLedger {
            fn oracle(&self) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
                unimplemented!()
            }
            fn balance(&self, _: &icn_kernel_api::types::Did, _: &str) -> i64 {
                0
            }
            fn credit_limit(&self, _: &icn_kernel_api::types::Did, _: &str) -> i64 {
                0
            }
            fn record_event(&self, _: icn_kernel_api::services::LedgerEvent) {}
            fn list_enforceable_resources(
                &self,
                _current_time: u64,
            ) -> Result<Vec<ResourceAccessInfo>, String> {
                Ok(vec![ResourceAccessInfo {
                    resource_id: "res-1".to_string(),
                    holder: "did:icn:holder1".to_string(),
                    granted_at: 0,
                    is_revoked: false,
                    idle_violation: Some(icn_kernel_api::services::IdleViolationInfo {
                        idle_seconds: 999,
                        max_idle_seconds: 100,
                    }),
                }])
            }
            fn revoke_resource_access(
                &self,
                _req: &icn_kernel_api::services::RevokeResourceAccessRequest,
            ) -> Result<(), String> {
                Ok(())
            }
        }

        let config = ResourceEnforcerConfig {
            check_interval_seconds: 3600,
            batch_size: 100,
            enabled: true,
        };

        let deps = ResourceEnforcerDeps {
            ledger_service: Arc::new(StubLedger),
            gossip_handle: None, // No gossip — should not panic
        };

        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        let shutdown_rx = shutdown_tx.subscribe();
        let handle = ResourceAccessEnforcerActor::spawn(config, deps, shutdown_rx);

        // Force a check — should revoke 1 resource without crashing on None gossip
        let result = handle.force_check().await.unwrap();
        assert_eq!(result.resources_checked, 1);
        assert_eq!(result.revocations, 1);

        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.total_revocations, 1);

        let _ = shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
