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
    use icn_steward::{StewardMessage, StewardHandle};

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
                let data = match bincode::serialize(&steward_msg) {
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
    pub async fn subscribe_to_topics(
        gossip_handle: &Arc<RwLock<GossipActor>>,
        did: &Did,
    ) {
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
                match bincode::deserialize::<StewardMessage>(&data) {
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
}
