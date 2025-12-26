//! Background tasks for supervisor
//!
//! This module contains factory functions for spawning background tasks
//! that run alongside the main supervisor loop.

use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::sync::broadcast::Receiver as BroadcastReceiver;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_net::actor::NetworkHandle;
use icn_time::ClockSync;

/// Configuration for clock synchronization task
pub struct ClockSyncConfig {
    /// Interval between sync attempts
    pub sync_interval: Duration,
}

impl Default for ClockSyncConfig {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_secs(600), // 10 minutes
        }
    }
}

/// Spawn the clock synchronization background task
///
/// This task periodically synchronizes the node's clock with NTP servers
/// to maintain accurate timestamps for cryptographic operations.
pub fn spawn_clock_sync_task(
    config: ClockSyncConfig,
    mut shutdown_rx: BroadcastReceiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut clock_sync = ClockSync::new();

        loop {
            // Perform sync
            if let Err(e) = clock_sync.sync().await {
                warn!("Clock sync failed: {}", e);
                icn_obs::metrics::scalability::clock_sync_failed_inc();
            }

            // Wait for next sync interval or shutdown
            select! {
                _ = tokio::time::sleep(config.sync_interval) => {
                    // Continue to next sync
                }
                _ = shutdown_rx.recv() => {
                    info!("Clock sync task shutting down");
                    break;
                }
            }
        }
    })
}

/// Configuration for metrics update task
pub struct MetricsUpdateConfig {
    /// Interval between metrics updates
    pub update_interval: Duration,
    /// Number of active actors to report
    pub active_actor_count: usize,
}

impl Default for MetricsUpdateConfig {
    fn default() -> Self {
        Self {
            update_interval: Duration::from_secs(10),
            active_actor_count: 7, // network + gossip + ledger + rpc + anti-entropy + digest-emitter + cache-cleanup
        }
    }
}

/// Spawn the metrics update background task
///
/// This task periodically updates system metrics including:
/// - Node uptime
/// - Active actor count
/// - Network statistics
/// - Contribution tracking
pub fn spawn_metrics_update_task(
    config: MetricsUpdateConfig,
    network_handle: NetworkHandle,
    did: Did,
    start_time: std::time::Instant,
    mut shutdown_rx: BroadcastReceiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.update_interval);
        let mut last_uptime_recorded: u64 = 0;

        loop {
            select! {
                _ = interval.tick() => {
                    // Update uptime
                    let uptime_secs = start_time.elapsed().as_secs();
                    icn_obs::metrics::system::uptime_seconds_set(uptime_secs);

                    // Record uptime contribution (Phase 21.1)
                    // Track the delta since last recording to avoid double-counting
                    let uptime_delta = uptime_secs - last_uptime_recorded;
                    if uptime_delta > 0 {
                        icn_obs::metrics::contribution::uptime_seconds_add(
                            did.as_str(),
                            uptime_delta,
                        );
                        // Record heartbeat timestamp for liveness tracking
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        icn_obs::metrics::contribution::uptime_heartbeat_record(
                            did.as_str(),
                            now,
                        );
                        last_uptime_recorded = uptime_secs;
                    }

                    // Count active actors
                    icn_obs::metrics::system::actors_active_set(config.active_actor_count as u64);

                    // Update network stats (this also updates metrics via GetStats handler)
                    let _ = network_handle.get_stats().await;
                }
                _ = shutdown_rx.recv() => {
                    info!("Metrics update task shutting down");
                    break;
                }
            }
        }
    })
}

/// Steward gossip topic routing helper
pub mod steward {
    use super::*;
    use icn_steward::{StewardHandle, StewardMessage};

    /// Create a send callback for steward messages via gossip
    pub fn create_send_callback(
        gossip_handle: Arc<RwLock<GossipActor>>,
    ) -> icn_steward::actor::SendGossipCallback {
        Arc::new(move |steward_msg| {
            let gossip = gossip_handle.clone();

            tokio::spawn(async move {
                // Determine topic based on message type
                let topic = match &steward_msg {
                    StewardMessage::Announce(_) => icn_steward::topics::STEWARD_ANNOUNCE,
                    StewardMessage::Enrollment(_) => icn_steward::topics::ENROLLMENT,
                    StewardMessage::Recovery(_) => icn_steward::topics::RECOVERY,
                    StewardMessage::VuiSync(_) => icn_steward::topics::VUI_SYNC,
                };

                // Serialize message
                let data =
                    match bincode::serde::encode_to_vec(&steward_msg, bincode::config::legacy()) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!("Failed to serialize steward message: {}", e);
                            return;
                        }
                    };

                // Publish via gossip
                {
                    let mut gossip = gossip.write().await;
                    if let Err(e) = gossip.publish(topic, data) {
                        warn!("Failed to publish steward message: {}", e);
                    }
                }
            });
        })
    }

    /// Subscribe to steward gossip topics
    pub async fn subscribe_to_topics(gossip_handle: &Arc<RwLock<GossipActor>>, did: &Did) {
        let mut gossip = gossip_handle.write().await;
        for topic in &[
            icn_steward::topics::STEWARD_ANNOUNCE,
            icn_steward::topics::VUI_SYNC,
            icn_steward::topics::ENROLLMENT,
            icn_steward::topics::RECOVERY,
        ] {
            if let Err(e) = gossip.subscribe(topic, did.clone()) {
                warn!("Failed to subscribe to steward topic {}: {}", topic, e);
            } else {
                info!("Subscribed to steward topic: {}", topic);
            }
        }
    }

    /// Create notification callback for steward messages
    pub fn create_notification_callback(
        steward_handle_holder: Arc<RwLock<Option<StewardHandle>>>,
    ) -> Arc<dyn Fn(String, icn_gossip::GossipEntry, Did) + Send + Sync> {
        Arc::new(move |topic, entry, _subscriber_did| {
            // Only process steward topics
            if !topic.starts_with("steward:") {
                return;
            }

            let steward_holder = steward_handle_holder.clone();
            let data = entry.data.clone();
            let topic_clone = topic.clone();

            tokio::spawn(async move {
                let steward_guard = steward_holder.read().await;
                if steward_guard.is_none() {
                    return;
                }

                // Parse steward message
                match bincode::serde::decode_from_slice::<StewardMessage, _>(
                    &data,
                    bincode::config::legacy(),
                )
                .map(|(v, _)| v)
                {
                    Ok(msg) => {
                        debug!(
                            "Received steward message on topic {}: {:?}",
                            topic_clone, msg
                        );
                        // Message handling would be routed to StewardActor here
                        // via handle methods in a full implementation
                    }
                    Err(e) => {
                        debug!(
                            "Failed to deserialize steward message on {}: {}",
                            topic_clone, e
                        );
                    }
                }
            });
        })
    }
}

/// Configuration for parameter scheduler task
pub struct ParameterSchedulerConfig {
    /// Interval between checking for due parameter changes
    pub check_interval: Duration,
}

impl Default for ParameterSchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(10), // Check every 10 seconds
        }
    }
}

/// Result of applying a single pending parameter change
pub enum ParameterApplyResult {
    /// Change was successfully applied
    Applied,
    /// Change was skipped (e.g., parameter no longer exists)
    Skipped { reason: String },
    /// Change application failed
    Failed { error: String },
}

/// Spawn the parameter scheduler background task
///
/// This task periodically checks for pending protocol parameter changes
/// that are due and applies them. This enables delayed execution of
/// governance-approved parameter changes.
///
/// # Parameters
/// - `config`: Scheduler configuration
/// - `parameter_store`: The parameter store (must support pending changes)
/// - `shutdown_rx`: Shutdown signal receiver
///
/// # Behavior
/// - Checks for due changes every `check_interval`
/// - Applies changes in chronological order (earliest first)
/// - Handles conflicts by superseding older pending changes for the same parameter
/// - Logs all actions for audit trail
pub fn spawn_parameter_scheduler_task(
    config: ParameterSchedulerConfig,
    parameter_store: Arc<dyn icn_governance::ProtocolParameterStore>,
    mut shutdown_rx: BroadcastReceiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("Parameter scheduler task started");

        let mut interval = tokio::time::interval(config.check_interval);

        loop {
            select! {
                _ = interval.tick() => {
                    if let Err(e) = process_due_changes(&parameter_store).await {
                        warn!("Error processing due parameter changes: {}", e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Parameter scheduler task shutting down");
                    break;
                }
            }
        }
    })
}

/// Process all pending parameter changes that are due
async fn process_due_changes(
    store: &Arc<dyn icn_governance::ProtocolParameterStore>,
) -> anyhow::Result<()> {
    let now = icn_time::current_timestamp_secs();

    // Get all changes that are due
    let due_changes = store.get_changes_due_before(now)?;

    if due_changes.is_empty() {
        return Ok(());
    }

    debug!(
        due_count = due_changes.len(),
        "Processing due parameter changes"
    );

    // Track which parameters we've processed to handle superseding
    let mut processed_params: std::collections::HashSet<String> = std::collections::HashSet::new();

    for change in due_changes {
        // If we've already processed a change for this parameter in this batch,
        // this older change should be superseded
        if processed_params.contains(&change.parameter_id) {
            let mut updated = change.clone();
            updated.mark_superseded("Superseded by earlier change in batch");
            if let Err(e) = store.update_pending_change(updated) {
                warn!(
                    pending_change_id = %change.id,
                    error = %e,
                    "Failed to mark pending change as superseded"
                );
            }
            continue;
        }

        // Apply the change
        match apply_pending_change(store, &change).await {
            ParameterApplyResult::Applied => {
                info!(
                    pending_change_id = %change.id,
                    parameter_id = %change.parameter_id,
                    proposal_id = %change.proposal_id,
                    "Applied delayed parameter change"
                );
                icn_obs::metrics::protocol::pending_parameter_changes_applied_inc();
                processed_params.insert(change.parameter_id.clone());
            }
            ParameterApplyResult::Skipped { reason } => {
                warn!(
                    pending_change_id = %change.id,
                    parameter_id = %change.parameter_id,
                    reason = %reason,
                    "Skipped pending parameter change"
                );
            }
            ParameterApplyResult::Failed { error } => {
                warn!(
                    pending_change_id = %change.id,
                    parameter_id = %change.parameter_id,
                    error = %error,
                    "Failed to apply pending parameter change"
                );
            }
        }
    }

    Ok(())
}

/// Apply a single pending parameter change
async fn apply_pending_change(
    store: &Arc<dyn icn_governance::ProtocolParameterStore>,
    change: &icn_governance::PendingParameterChange,
) -> ParameterApplyResult {
    // Get the current parameter
    let current_param = match store.get(&change.parameter_id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            // Parameter no longer exists, mark as cancelled
            let mut updated = change.clone();
            updated.mark_cancelled("Parameter no longer exists");
            let _ = store.update_pending_change(updated);
            return ParameterApplyResult::Skipped {
                reason: "Parameter no longer exists".to_string(),
            };
        }
        Err(e) => {
            return ParameterApplyResult::Failed {
                error: format!("Failed to get parameter: {e}"),
            };
        }
    };

    // Check for existing pending changes for the same parameter that are older
    // and should be superseded
    if let Ok(pending_for_param) = store.list_pending_changes_for_parameter(&change.parameter_id) {
        for other in pending_for_param {
            if other.id != change.id
                && other.status == icn_governance::PendingChangeStatus::Pending
                && other.effective_at <= change.effective_at
            {
                // Supersede the older change
                let mut superseded = other.clone();
                superseded.mark_superseded(&change.id);
                if let Err(e) = store.update_pending_change(superseded) {
                    debug!(
                        superseded_id = %other.id,
                        error = %e,
                        "Failed to mark older change as superseded"
                    );
                }
            }
        }
    }

    // Create updated parameter with new value
    let mut updated_param = current_param.clone();
    updated_param.value = change.new_value.clone();
    updated_param.updated_at = icn_time::current_timestamp_secs();
    updated_param.updated_by = Some(change.proposal_id.clone());

    // Apply the parameter change
    if let Err(e) = store.set(
        updated_param,
        Some(change.proposal_id.clone()),
        Some(format!("delayed execution of {}", change.proposal_id)),
    ) {
        return ParameterApplyResult::Failed {
            error: format!("Failed to set parameter: {e}"),
        };
    }

    // Mark the pending change as applied
    let mut applied_change = change.clone();
    applied_change.mark_applied();
    if let Err(e) = store.update_pending_change(applied_change) {
        // Log but don't fail - the parameter was successfully updated
        warn!(
            pending_change_id = %change.id,
            error = %e,
            "Failed to mark pending change as applied"
        );
    }

    ParameterApplyResult::Applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_sync_config_default() {
        let config = ClockSyncConfig::default();
        assert_eq!(config.sync_interval, Duration::from_secs(600));
    }

    #[test]
    fn test_metrics_config_default() {
        let config = MetricsUpdateConfig::default();
        assert_eq!(config.update_interval, Duration::from_secs(10));
        assert_eq!(config.active_actor_count, 7);
    }

    #[test]
    fn test_parameter_scheduler_config_default() {
        let config = ParameterSchedulerConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(10));
    }
}
