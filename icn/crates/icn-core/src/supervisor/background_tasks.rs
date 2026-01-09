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
                    if let Err(e) = gossip.publish(topic, data).await {
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
            if let Err(e) = gossip.subscribe(topic, did.clone()).await {
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
        // this later change should be superseded (since get_changes_due_before returns
        // changes ordered by effective_at ascending, earlier changes are processed first)
        if processed_params.contains(&change.parameter_id) {
            let mut updated = change.clone();
            updated.mark_superseded("Superseded by change with earlier effective time");
            if let Err(e) = store.update_pending_change(updated) {
                warn!(
                    pending_change_id = %change.id,
                    error = %e,
                    "Failed to mark pending change as superseded"
                );
            } else {
                icn_obs::metrics::protocol::pending_parameter_changes_superseded_inc();
            }
            continue;
        }

        // Mark this parameter as processed BEFORE attempting to apply.
        // This ensures that if apply_pending_change() fails, any subsequent changes
        // for the same parameter are still marked as superseded (not attempted).
        processed_params.insert(change.parameter_id.clone());

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

    // Update the active pending changes gauge
    if let Ok(active_count) = store.count_pending_changes() {
        icn_obs::metrics::protocol::pending_parameter_changes_active_set(active_count as u64);
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
            if store.update_pending_change(updated).is_ok() {
                icn_obs::metrics::protocol::pending_parameter_changes_cancelled_inc();
            }
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

    // Validate scope consistency: the pending change's scope must match the parameter's scope.
    // This prevents a cooperative-scoped pending change from modifying a global parameter.
    if current_param.scope != change.scope {
        let mut updated = change.clone();
        updated.mark_cancelled(format!(
            "Scope mismatch: pending change has scope {:?} but parameter has scope {:?}",
            change.scope, current_param.scope
        ));
        if store.update_pending_change(updated).is_ok() {
            icn_obs::metrics::protocol::pending_parameter_changes_cancelled_inc();
        }
        return ParameterApplyResult::Skipped {
            reason: format!(
                "Scope mismatch: expected {:?}, found {:?}",
                change.scope, current_param.scope
            ),
        };
    }

    // Note: Superseding of later changes for the same parameter is handled
    // in process_due_changes() at the batch level, not here.

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

/// Configuration for entity audit pruning task
pub struct AuditPruneConfig {
    /// Interval between pruning runs
    pub prune_interval: Duration,
    /// Maximum age of audit records in days
    pub retention_days: u64,
    /// Maximum records to keep per entity
    pub max_records_per_entity: usize,
    /// Batch size for pruning operations
    pub batch_size: usize,
}

impl Default for AuditPruneConfig {
    fn default() -> Self {
        Self {
            prune_interval: Duration::from_secs(3600), // 1 hour
            retention_days: 365,
            max_records_per_entity: 10000,
            batch_size: 1000,
        }
    }
}

impl From<&crate::config::AuditRetentionConfig> for AuditPruneConfig {
    fn from(config: &crate::config::AuditRetentionConfig) -> Self {
        Self {
            prune_interval: Duration::from_secs(config.prune_interval_secs),
            retention_days: config.retention_days,
            max_records_per_entity: config.max_records_per_entity,
            batch_size: config.prune_batch_size,
        }
    }
}

/// Spawn the entity audit pruning background task
///
/// This task periodically prunes old audit records based on the retention policy.
/// It runs at the configured interval and removes records that:
/// 1. Exceed the maximum age (retention_days)
/// 2. Exceed the maximum count per entity (max_records_per_entity)
///
/// # Parameters
/// - `config`: Pruning configuration
/// - `audit_manager`: The entity audit manager
/// - `shutdown_rx`: Shutdown signal receiver
pub fn spawn_audit_prune_task(
    config: AuditPruneConfig,
    audit_manager: Arc<icn_gateway::EntityAuditManager>,
    mut shutdown_rx: BroadcastReceiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            retention_days = config.retention_days,
            max_records = config.max_records_per_entity,
            interval_secs = config.prune_interval.as_secs(),
            "Audit prune task started"
        );

        let mut interval = tokio::time::interval(config.prune_interval);

        // Skip first tick to avoid immediate pruning on startup
        interval.tick().await;

        loop {
            select! {
                _ = interval.tick() => {
                    debug!("Running scheduled audit pruning");

                    match audit_manager.prune_audit_records(
                        config.retention_days,
                        config.max_records_per_entity,
                        config.batch_size,
                    ) {
                        Ok(stats) => {
                            if stats.records_pruned > 0 {
                                info!(
                                    records_pruned = stats.records_pruned,
                                    entities_processed = stats.entities_processed,
                                    duration_ms = stats.duration_ms,
                                    "Audit pruning completed"
                                );
                            } else {
                                debug!("Audit pruning completed - no records to prune");
                            }

                            // Log any non-fatal errors
                            for error in &stats.errors {
                                warn!("Audit prune error: {}", error);
                            }
                        }
                        Err(e) => {
                            warn!("Audit pruning failed: {}", e);
                            icn_obs::metrics::gateway::entity_audit_prune_failed_inc();
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Audit prune task shutting down");
                    break;
                }
            }
        }
    })
}

/// Configuration for connection candidate re-announcement task
pub struct CandidateAnnouncementConfig {
    /// Interval between candidate announcements (default: 150s = 2.5 min)
    /// This should be less than the gossip entry TTL (typically 5 min)
    pub announce_interval: Duration,
}

impl Default for CandidateAnnouncementConfig {
    fn default() -> Self {
        Self {
            announce_interval: Duration::from_secs(150), // 2.5 minutes
        }
    }
}

/// Cached connection candidate for change detection
#[derive(Clone, Debug, PartialEq)]
struct CachedCandidate {
    local_addr: std::net::SocketAddr,
    public_addr: Option<std::net::SocketAddr>,
    relay_addr: Option<std::net::SocketAddr>,
}

/// Spawn the connection candidate re-announcement background task
///
/// This task periodically re-announces the node's connection candidate to gossip.
/// It ensures peers have up-to-date connectivity information even if the gossip
/// entry TTL expires.
///
/// Key behaviors:
/// - Announces every `announce_interval` (default 2.5 min, before 5-min TTL)
/// - Detects address changes and announces immediately
/// - Handles STUN/TURN updates that may change addresses
///
/// # Parameters
/// - `config`: Announcement configuration
/// - `network_handle`: Handle to get connection candidate
/// - `gossip_handle`: Handle to publish to gossip
/// - `shutdown_rx`: Shutdown signal receiver
pub fn spawn_candidate_announcement_task(
    config: CandidateAnnouncementConfig,
    network_handle: NetworkHandle,
    gossip_handle: Arc<RwLock<GossipActor>>,
    mut shutdown_rx: BroadcastReceiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            interval_secs = config.announce_interval.as_secs(),
            "Connection candidate announcement task started"
        );

        let mut interval = tokio::time::interval(config.announce_interval);
        let mut last_candidate: Option<CachedCandidate> = None;

        // Skip first tick - initial announcement happens at startup
        interval.tick().await;

        loop {
            select! {
                _ = interval.tick() => {
                    // Get current connection candidate
                    let candidate = match network_handle.connection_candidate().await {
                        Ok(c) => c,
                        Err(e) => {
                            debug!("Failed to get connection candidate for re-announcement: {}", e);
                            continue;
                        }
                    };

                    // Check if candidate has changed
                    let current = CachedCandidate {
                        local_addr: candidate.local_addr,
                        public_addr: candidate.public_addr,
                        relay_addr: candidate.relay_addr,
                    };

                    let changed = last_candidate.as_ref() != Some(&current);

                    if changed {
                        info!(
                            "Connection candidate changed: local={}, public={:?}, relay={:?}",
                            current.local_addr, current.public_addr, current.relay_addr
                        );
                        icn_obs::metrics::nat::dial_attempt_inc("candidate_change");
                    }

                    // Serialize and publish
                    match serde_json::to_vec(&candidate) {
                        Ok(candidate_bytes) => {
                            let mut gossip = gossip_handle.write().await;
                            match gossip.publish(super::init_gossip::NETWORK_CANDIDATES_TOPIC, candidate_bytes).await {
                                Ok(_) => {
                                    debug!(
                                        changed = changed,
                                        "Re-announced connection candidate"
                                    );
                                    last_candidate = Some(current);
                                }
                                Err(e) => {
                                    warn!("Failed to re-announce connection candidate: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to serialize connection candidate: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Connection candidate announcement task shutting down");
                    break;
                }
            }
        }
    })
}

/// Callback type for recording peer cache successes
pub type PeerCacheCallback = Arc<dyn Fn(&Did, std::net::SocketAddr) + Send + Sync>;

/// Configuration for bootstrap peer health checking task
pub struct BootstrapHealthConfig {
    /// Interval between health checks (default: 60s)
    pub check_interval: Duration,
    /// Timeout for reconnection attempts (default: 10s)
    pub reconnect_timeout: Duration,
    /// Maximum consecutive failures before alerting (default: 3)
    pub max_failures: u32,
}

impl Default for BootstrapHealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(60),
            reconnect_timeout: Duration::from_secs(10),
            max_failures: 3,
        }
    }
}

/// Status of bootstrap peer connectivity
#[derive(Debug, Clone)]
pub struct BootstrapPeerStatus {
    /// Peer DID
    pub did: Did,
    /// Peer address
    pub address: std::net::SocketAddr,
    /// Currently connected
    pub connected: bool,
    /// Consecutive connection failures
    pub failure_count: u32,
}

/// Spawn the bootstrap health checking background task
///
/// This task periodically checks connectivity to configured bootstrap peers
/// and attempts reconnection if disconnected. It provides early warning when
/// bootstrap connectivity is lost.
///
/// Key behaviors:
/// - Checks bootstrap peer connectivity every `check_interval`
/// - Attempts reconnection for disconnected bootstrap peers
/// - Records successful connections to peer cache for faster rejoining
/// - Emits metrics for bootstrap connectivity monitoring
///
/// # Parameters
/// - `config`: Health check configuration
/// - `bootstrap_peers`: List of (DID, Address) pairs for bootstrap peers
/// - `network_handle`: Handle to check connectivity and dial
/// - `peer_cache`: Optional peer cache for recording connections
/// - `shutdown_rx`: Shutdown signal receiver
pub fn spawn_bootstrap_health_task(
    config: BootstrapHealthConfig,
    bootstrap_peers: Vec<(Did, std::net::SocketAddr)>,
    network_handle: NetworkHandle,
    peer_cache: Option<PeerCacheCallback>,
    mut shutdown_rx: BroadcastReceiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if bootstrap_peers.is_empty() {
            info!("No bootstrap peers configured, health task exiting");
            return;
        }

        info!(
            peer_count = bootstrap_peers.len(),
            interval_secs = config.check_interval.as_secs(),
            "Bootstrap health task started"
        );

        // Track status for each bootstrap peer
        let mut peer_status: Vec<BootstrapPeerStatus> = bootstrap_peers
            .iter()
            .map(|(did, addr)| BootstrapPeerStatus {
                did: did.clone(),
                address: *addr,
                connected: true, // Assume initially connected (we dial at startup)
                failure_count: 0,
            })
            .collect();

        let mut interval = tokio::time::interval(config.check_interval);

        // Skip first tick - startup connection happens elsewhere
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let mut connected_count = 0;
                    let mut disconnected_count = 0;

                    for status in &mut peer_status {
                        // Check if peer is currently connected
                        let is_connected = network_handle
                            .is_peer_connected(&status.did)
                            .await
                            .unwrap_or(false);

                        if is_connected {
                            // Peer is connected
                            connected_count += 1;
                            status.connected = true;
                            status.failure_count = 0;

                            // Record success to peer cache if available
                            if let Some(ref cache_fn) = peer_cache {
                                cache_fn(&status.did, status.address);
                            }
                        } else {
                            // Peer is disconnected - attempt reconnection
                            disconnected_count += 1;
                            status.connected = false;

                            debug!(
                                peer = %status.did,
                                address = %status.address,
                                failures = status.failure_count,
                                "Bootstrap peer disconnected, attempting reconnection"
                            );

                            // Attempt reconnection with timeout
                            match tokio::time::timeout(
                                config.reconnect_timeout,
                                network_handle.dial(status.address, status.did.clone()),
                            )
                            .await
                            {
                                Ok(Ok(_)) => {
                                    info!(
                                        peer = %status.did,
                                        "Reconnected to bootstrap peer"
                                    );
                                    status.connected = true;
                                    status.failure_count = 0;
                                    connected_count += 1;
                                    disconnected_count -= 1;

                                    // Record success to peer cache
                                    if let Some(ref cache_fn) = peer_cache {
                                        cache_fn(&status.did, status.address);
                                    }

                                    icn_obs::metrics::nat::dial_success_inc("bootstrap_reconnect");
                                }
                                Ok(Err(e)) => {
                                    status.failure_count += 1;
                                    warn!(
                                        peer = %status.did,
                                        error = %e,
                                        failures = status.failure_count,
                                        "Failed to reconnect to bootstrap peer"
                                    );
                                    icn_obs::metrics::nat::dial_failure_inc();
                                }
                                Err(_) => {
                                    status.failure_count += 1;
                                    warn!(
                                        peer = %status.did,
                                        timeout_ms = config.reconnect_timeout.as_millis(),
                                        failures = status.failure_count,
                                        "Bootstrap reconnection timed out"
                                    );
                                    icn_obs::metrics::nat::dial_failure_inc();
                                }
                            }

                            // Alert if max failures exceeded
                            if status.failure_count >= config.max_failures {
                                warn!(
                                    peer = %status.did,
                                    failures = status.failure_count,
                                    "Bootstrap peer unreachable after {} consecutive failures",
                                    config.max_failures
                                );
                            }
                        }
                    }

                    // Update metrics
                    icn_obs::metrics::scalability::bootstrap_peers_connected_set(connected_count);
                    icn_obs::metrics::scalability::bootstrap_peers_disconnected_set(disconnected_count);

                    // Log summary if any disconnected
                    if disconnected_count > 0 {
                        debug!(
                            connected = connected_count,
                            disconnected = disconnected_count,
                            total = peer_status.len(),
                            "Bootstrap peer health check summary"
                        );
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Bootstrap health task shutting down");
                    break;
                }
            }
        }
    })
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
    fn test_candidate_announcement_config_default() {
        let config = CandidateAnnouncementConfig::default();
        assert_eq!(config.announce_interval, Duration::from_secs(150));
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

    #[test]
    fn test_audit_prune_config_default() {
        let config = AuditPruneConfig::default();
        assert_eq!(config.prune_interval, Duration::from_secs(3600));
        assert_eq!(config.retention_days, 365);
        assert_eq!(config.max_records_per_entity, 10000);
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn test_bootstrap_health_config_default() {
        let config = BootstrapHealthConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(60));
        assert_eq!(config.reconnect_timeout, Duration::from_secs(10));
        assert_eq!(config.max_failures, 3);
    }
}
