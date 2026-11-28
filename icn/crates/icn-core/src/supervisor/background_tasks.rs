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
    _did: Did, // Unused after per-DID tracking removal
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

                    // Record uptime contribution (aggregate, no per-DID tracking)
                    // Track the delta since last recording to avoid double-counting
                    let uptime_delta = uptime_secs - last_uptime_recorded;
                    if uptime_delta > 0 {
                        icn_obs::metrics::contribution::total_uptime_seconds_add(uptime_delta);
                        // Record heartbeat for liveness tracking
                        icn_obs::metrics::contribution::heartbeats_received_inc();
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
                let data = match icn_encoding::encode(&steward_msg) {
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
                match icn_encoding::decode::<StewardMessage>(&data) {
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
    parameter_store: Arc<dyn icn_kernel_api::protocol_params::ProtocolParameterStore>,
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
    store: &Arc<dyn icn_kernel_api::protocol_params::ProtocolParameterStore>,
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
    store: &Arc<dyn icn_kernel_api::protocol_params::ProtocolParameterStore>,
    change: &icn_kernel_api::protocol_params::PendingParameterChange,
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
    local_addr: Option<std::net::SocketAddr>,
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
                        local_addr: candidate.local_addr(),
                        public_addr: candidate.public_addr(),
                        relay_addr: candidate.relay_addr(),
                    };

                    let changed = last_candidate.as_ref() != Some(&current);

                    if changed {
                        info!(
                            "Connection candidate changed: local={:?}, public={:?}, relay={:?}",
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

/// Storage challenge integration helpers
pub mod storage_challenge {
    use super::*;
    use crate::storage_challenge::ChallengeSchedulerHandle;

    /// Create a storage proof callback that forwards proofs to the ChallengeScheduler
    ///
    /// This callback is set on the GossipActor to handle incoming storage proofs.
    /// When a proof is received, it spawns an async task to forward it to the
    /// ChallengeScheduler for verification.
    pub fn create_proof_callback(
        scheduler_handle: ChallengeSchedulerHandle,
    ) -> icn_gossip::StorageProofCallback {
        Arc::new(move |proof| {
            let handle = scheduler_handle.clone();
            tokio::spawn(async move {
                if let Err(e) = handle.handle_proof(proof).await {
                    warn!("Error handling storage proof: {}", e);
                }
            });
            Ok(())
        })
    }
}

/// Spawn the storage maintenance background task
///
/// This task periodically runs Sled database maintenance operations:
/// - Flush data to disk for durability
/// - Monitor space amplification
/// - Generate warnings when thresholds are exceeded
///
/// # Parameters
/// - `config`: Maintenance configuration (interval, thresholds)
/// - `store`: The Sled store to maintain
/// - `shutdown_rx`: Shutdown signal receiver from supervisor
///
/// # Returns
/// A `MaintenanceHandle` that can be used to trigger immediate maintenance
/// or stop the task.
pub fn spawn_storage_maintenance_task(
    config: icn_store::MaintenanceConfig,
    store: Arc<icn_store::SledStore>,
    mut shutdown_rx: BroadcastReceiver<()>,
) -> icn_store::MaintenanceHandle {
    let manager = Arc::new(icn_store::MaintenanceManager::new(store, config));
    let handle = manager.start_background_task();

    // Spawn a task that listens for supervisor shutdown and stops the maintenance task
    let handle_for_shutdown = handle.clone();
    tokio::spawn(async move {
        let _ = shutdown_rx.recv().await;
        info!("Stopping storage maintenance task due to supervisor shutdown");
        handle_for_shutdown.stop();
    });

    handle
}

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

/// What a bootstrap peer's configured endpoint looks like right now.
///
/// Three outcomes are kept apart because they call for different responses. Only `Healthy`
/// means the operator's relationship holds. `IdentityMismatch` is an authenticated, live
/// session at the configured address with a *different* DID than configured: transport works,
/// but the configured peer is not the one there, so it is never reported healthy — and
/// redialling cannot change who answers at that address, so it is not redialled while the other
/// node holds it. `Unreachable` is the only case a redial can fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapPeerHealth {
    /// The configured DID is authenticated on a live session.
    Healthy,
    /// A live session reaches the configured address, but authenticated as `authenticated`.
    IdentityMismatch { authenticated: Did },
    /// No live session reaches the configured peer or its address.
    Unreachable,
}

/// `addr` with an IPv4-mapped IP (`::ffff:a.b.c.d`) rewritten to plain IPv4.
///
/// A dual-stack socket — the default, `listen_addr` is `[::]:7777` — reports IPv4 peers in mapped
/// form, while an operator configures `a.b.c.d`. Same rule as `SourceKey::from_addr` in
/// `icn-net`: only `::ffff:0:0/96` is unmapped, never deprecated IPv4-compatible addresses (that
/// would also turn `::1` into `0.0.0.1`).
fn unmapped(addr: std::net::SocketAddr) -> std::net::SocketAddr {
    match addr.ip() {
        std::net::IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(v4) => std::net::SocketAddr::new(std::net::IpAddr::V4(v4), addr.port()),
            None => addr,
        },
        std::net::IpAddr::V4(_) => addr,
    }
}

/// Classify one configured bootstrap peer.
///
/// `expected_is_live` is whether the configured DID holds a live, authenticated session;
/// `live_endpoints` are every live, authenticated session's `(DID, remote address)`. Pure, so
/// the policy is testable without a network.
pub fn classify_bootstrap_peer(
    expected: &Did,
    address: std::net::SocketAddr,
    expected_is_live: bool,
    live_endpoints: &[(Did, std::net::SocketAddr)],
) -> BootstrapPeerHealth {
    if expected_is_live {
        return BootstrapPeerHealth::Healthy;
    }
    match live_endpoints
        .iter()
        .find(|(did, addr)| unmapped(*addr) == unmapped(address) && did != expected)
    {
        Some((other, _)) => BootstrapPeerHealth::IdentityMismatch {
            authenticated: other.clone(),
        },
        None => BootstrapPeerHealth::Unreachable,
    }
}

/// Status of bootstrap peer connectivity
#[derive(Debug, Clone)]
pub struct BootstrapPeerStatus {
    /// Peer DID
    pub did: Did,
    /// Peer address
    pub address: std::net::SocketAddr,
    /// Last observed health; `None` before the first check.
    pub health: Option<BootstrapPeerHealth>,
    /// Consecutive failed reconnection attempts
    pub failure_count: u32,
    /// Checks to skip before the next reconnection attempt (exponential backoff)
    pub backoff_ticks_remaining: u32,
}

/// How many checks to skip after `failures` consecutive failed reconnections: 1, 3, 7, 15, then
/// 31 at most — attempts spaced 2, 4, 8, 16 and finally 32 check intervals apart.
///
/// Each attempt to an unreachable peer occupies the network actor for up to its dial timeout
/// (the actor awaits dials inline), so retrying a peer that stays down on every check would keep
/// the actor busy indefinitely.
pub fn reconnect_backoff_ticks(failures: u32) -> u32 {
    (1u32 << failures.min(5)) - 1
}

/// Spawn the bootstrap health checking background task
///
/// This task periodically checks connectivity to configured bootstrap peers
/// and attempts reconnection if disconnected. It provides early warning when
/// bootstrap connectivity is lost.
///
/// Key behaviors:
/// - Checks bootstrap peer connectivity every `check_interval`
/// - Classifies each peer as healthy, identity-mismatched, or unreachable
///   ([`classify_bootstrap_peer`]); only healthy peers count as connected
/// - Redials only unreachable peers, through `dial_expecting`, with exponential backoff
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
    shutdown_rx: BroadcastReceiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(run_bootstrap_health_task(
        config,
        bootstrap_peers,
        network_handle,
        peer_cache,
        shutdown_rx,
    ))
}

/// The bootstrap health task itself, for callers that own its lifetime — the daemon runs it in
/// the supervisor's `JoinSet`, so shutdown joins or aborts it with its sibling tasks.
/// [`spawn_bootstrap_health_task`] documents the behaviour.
pub async fn run_bootstrap_health_task(
    config: BootstrapHealthConfig,
    bootstrap_peers: Vec<(Did, std::net::SocketAddr)>,
    network_handle: NetworkHandle,
    peer_cache: Option<PeerCacheCallback>,
    mut shutdown_rx: BroadcastReceiver<()>,
) {
    {
        if bootstrap_peers.is_empty() {
            info!("No bootstrap peers configured, health task exiting");
            return;
        }

        info!(
            peer_count = bootstrap_peers.len(),
            interval_secs = config.check_interval.as_secs(),
            "Bootstrap health task started"
        );

        let mut peer_status: Vec<BootstrapPeerStatus> = bootstrap_peers
            .iter()
            .map(|(did, addr)| BootstrapPeerStatus {
                did: did.clone(),
                address: *addr,
                health: None,
                failure_count: 0,
                backoff_ticks_remaining: 0,
            })
            .collect();

        let mut interval = tokio::time::interval(config.check_interval);
        // A reconnection attempt can hold this loop for the reconnect timeout. `Delay` keeps the
        // spacing after such a stall; the default `Burst` would fire the missed checks back to
        // back and consume the backoff in a moment.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await; // Skip first immediate tick (we just dialed at startup)

        'checks: loop {
            tokio::select! {
                _ = interval.tick() => {
                    let mut connected_count = 0u64;
                    let mut disconnected_count = 0u64;
                    let live_endpoints = network_handle.connected_peer_endpoints().await;

                    for status in &mut peer_status {
                        let expected_is_live = network_handle
                            .is_peer_connected(&status.did)
                            .await
                            .unwrap_or(false);
                        let health = classify_bootstrap_peer(
                            &status.did,
                            status.address,
                            expected_is_live,
                            &live_endpoints,
                        );

                        match &health {
                            BootstrapPeerHealth::Healthy => {
                                connected_count += 1;
                                status.failure_count = 0;
                                status.backoff_ticks_remaining = 0;
                                if let Some(ref cache_fn) = peer_cache {
                                    cache_fn(&status.did, status.address);
                                }
                            }
                            BootstrapPeerHealth::IdentityMismatch { authenticated } => {
                                disconnected_count += 1;
                                // Once per episode: the state, not each check, is the news.
                                if status.health.as_ref() != Some(&health) {
                                    warn!(
                                        expected_did = %status.did,
                                        authenticated_did = %authenticated,
                                        address = %status.address,
                                        "Bootstrap peer identity mismatch: a different DID holds \
                                         the configured endpoint. Not counted as connected, and \
                                         not redialled while it holds the endpoint"
                                    );
                                }
                            }
                            BootstrapPeerHealth::Unreachable => {
                                disconnected_count += 1;
                                if status.backoff_ticks_remaining > 0 {
                                    status.backoff_ticks_remaining -= 1;
                                } else {
                                    debug!(
                                        peer = %status.did,
                                        address = %status.address,
                                        failures = status.failure_count,
                                        "Bootstrap peer disconnected, attempting reconnection"
                                    );
                                    // `dial_expecting`, as at startup: this DID came from
                                    // operator configuration, so a different node answering
                                    // after an outage surfaces as a divergence (#2533) and is
                                    // classified as a mismatch on the next check.
                                    // Raced against shutdown: an attempt can take the whole
                                    // reconnect timeout, and the supervisor waits for this task
                                    // before aborting what is left. Shutdown must not wait for a
                                    // dial to a peer that is not there.
                                    let attempt = tokio::select! {
                                        attempt = tokio::time::timeout(
                                            config.reconnect_timeout,
                                            network_handle.dial_expecting(
                                                status.address,
                                                status.did.clone(),
                                            ),
                                        ) => attempt,
                                        _ = shutdown_rx.recv() => {
                                            info!(
                                                "Bootstrap health task shutting down \
                                                 (reconnection attempt abandoned)"
                                            );
                                            break 'checks;
                                        }
                                    };
                                    match attempt {
                                        Ok(Ok(_)) => {
                                            // Transport came up; whether the configured peer is
                                            // there is decided by the next check. Count the
                                            // attempt until then: an address that answers but
                                            // never becomes healthy (failed Hello, refusal, a
                                            // node that drops us) must back off like one that
                                            // does not answer. At least one check of grace, so a
                                            // Hello in flight does not cost a second connection.
                                            status.failure_count += 1;
                                            status.backoff_ticks_remaining =
                                                reconnect_backoff_ticks(status.failure_count)
                                                    .max(1);
                                            info!(
                                                peer = %status.did,
                                                "Reached bootstrap peer; identity is confirmed \
                                                 by the next check"
                                            );
                                            icn_obs::metrics::nat::dial_success_inc(
                                                "bootstrap_reconnect",
                                            );
                                        }
                                        Ok(Err(e)) => {
                                            status.failure_count += 1;
                                            status.backoff_ticks_remaining =
                                                reconnect_backoff_ticks(status.failure_count);
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
                                            status.backoff_ticks_remaining =
                                                reconnect_backoff_ticks(status.failure_count);
                                            warn!(
                                                peer = %status.did,
                                                timeout_ms = config.reconnect_timeout.as_millis(),
                                                failures = status.failure_count,
                                                "Bootstrap reconnection timed out"
                                            );
                                            icn_obs::metrics::nat::dial_failure_inc();
                                        }
                                    }
                                    if status.failure_count >= config.max_failures {
                                        warn!(
                                            peer = %status.did,
                                            failures = status.failure_count,
                                            "Bootstrap peer not healthy after {} consecutive reconnection attempts",
                                            config.max_failures
                                        );
                                    }
                                }
                            }
                        }
                        status.health = Some(health);
                    }

                    icn_obs::metrics::scalability::bootstrap_peers_connected_set(connected_count);
                    icn_obs::metrics::scalability::bootstrap_peers_disconnected_set(disconnected_count);

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

    fn did(seed: u8) -> Did {
        icn_identity::IdentityBundle::generate()
            .map(|b| b.did().clone())
            .unwrap_or_else(|_| panic!("identity {seed}"))
    }

    #[test]
    fn only_the_expected_did_makes_a_bootstrap_peer_healthy() {
        let expected = did(1);
        let other = did(2);
        let addr: std::net::SocketAddr = "192.0.2.11:7777".parse().unwrap();

        assert_eq!(
            classify_bootstrap_peer(&expected, addr, true, &[(expected.clone(), addr)]),
            BootstrapPeerHealth::Healthy
        );
        // A different authenticated DID at the configured address is not the configured peer.
        assert_eq!(
            classify_bootstrap_peer(&expected, addr, false, &[(other.clone(), addr)]),
            BootstrapPeerHealth::IdentityMismatch {
                authenticated: other.clone()
            }
        );
        // A session to some other address says nothing about this endpoint.
        let elsewhere: std::net::SocketAddr = "192.0.2.12:7777".parse().unwrap();
        assert_eq!(
            classify_bootstrap_peer(&expected, addr, false, &[(other, elsewhere)]),
            BootstrapPeerHealth::Unreachable
        );
        assert_eq!(
            classify_bootstrap_peer(&expected, addr, false, &[]),
            BootstrapPeerHealth::Unreachable
        );
    }

    #[test]
    fn a_dual_stack_report_of_the_configured_ipv4_endpoint_is_the_same_endpoint() {
        let expected = did(1);
        let other = did(2);
        let configured: std::net::SocketAddr = "192.0.2.11:7777".parse().unwrap();
        let mapped: std::net::SocketAddr = "[::ffff:192.0.2.11]:7777".parse().unwrap();
        assert_eq!(
            classify_bootstrap_peer(&expected, configured, false, &[(other.clone(), mapped)]),
            BootstrapPeerHealth::IdentityMismatch {
                authenticated: other.clone()
            }
        );
        // IPv6 loopback must not be merged into an IPv4 address.
        let v6_loopback: std::net::SocketAddr = "[::1]:7777".parse().unwrap();
        let v4_odd: std::net::SocketAddr = "0.0.0.1:7777".parse().unwrap();
        assert_eq!(
            classify_bootstrap_peer(&expected, v4_odd, false, &[(other, v6_loopback)]),
            BootstrapPeerHealth::Unreachable
        );
    }

    #[test]
    fn reconnection_backoff_grows_and_is_capped() {
        let spacing: Vec<u32> = (1..=7).map(reconnect_backoff_ticks).collect();
        assert_eq!(spacing, vec![1, 3, 7, 15, 31, 31, 31]);
        assert_eq!(reconnect_backoff_ticks(u32::MAX), 31);
    }

    #[test]
    fn test_bootstrap_health_config_default() {
        let config = BootstrapHealthConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(60));
        assert_eq!(config.reconnect_timeout, Duration::from_secs(10));
        assert_eq!(config.max_failures, 3);
    }

    #[test]
    fn test_maintenance_config_default() {
        let config = icn_store::MaintenanceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.interval_secs, 3600); // 1 hour
        assert_eq!(config.amplification_threshold, 2.0);
    }
}
