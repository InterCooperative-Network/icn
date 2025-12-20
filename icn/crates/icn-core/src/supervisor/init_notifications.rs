//! Notification callback handlers for gossip topic processing
//!
//! This module extracts the notification callback handlers from the supervisor,
//! providing a cleaner separation of concerns for gossip message routing.

use crate::supervisor::init_coop;
use anyhow::Result;
use icn_gossip::GossipEntry;
use icn_identity::{Did, RecoveryMessage, IDENTITY_RECOVERY_TOPIC};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Gossip topic for NAT traversal connection candidate announcements
pub const NETWORK_CANDIDATES_TOPIC: &str = "network:candidates";

/// Type aliases for common handle types
pub type TrustGraphHandle = Arc<RwLock<icn_trust::TrustGraph>>;
pub type LedgerHandle = Arc<RwLock<icn_ledger::Ledger>>;
pub type GossipHandle = Arc<RwLock<icn_gossip::GossipActor>>;
pub type ContractActorHandle = Arc<RwLock<icn_ccl::ContractActor>>;
pub type SnapshotCoordinatorHandle = Arc<RwLock<icn_snapshot::SnapshotCoordinator>>;
pub type ComputeHandleHolder = Arc<RwLock<Option<icn_compute::ComputeHandle>>>;
pub type DisputeHandleHolder = Arc<RwLock<Option<icn_ccl::DisputeActorHandle>>>;
pub type NodeProfileHandle = Arc<RwLock<crate::node::NodeProfile>>;
pub type ProfileCache = Arc<RwLock<HashMap<Did, crate::node::NodeProfile>>>;
pub type CandidateCache = Arc<icn_net::CandidateCache>;
pub type FederationGossipHandle = Arc<icn_federation::FederationGossipHandler>;

/// Dependencies required for notification callback handlers
#[derive(Clone)]
pub struct NotificationDeps {
    /// Trust graph for attestation handling
    pub trust_graph: TrustGraphHandle,
    /// This node's DID
    pub own_did: Did,
    /// Contract actor for deployment handling
    pub contract_actor: ContractActorHandle,
    /// Recovery store for identity recovery
    pub recovery_store: Arc<dyn icn_store::Store>,
    /// Ledger for balance transfers
    pub ledger: LedgerHandle,
    /// Snapshot coordinator
    pub snapshot_coordinator: SnapshotCoordinatorHandle,
    /// Network handle for dialing peers
    pub network_handle: icn_net::NetworkHandle,
    /// Candidate cache for NAT traversal
    pub candidate_cache: CandidateCache,
    /// Gossip handle for publishing responses
    pub gossip_handle: GossipHandle,
    /// Compute handle holder (set after compute actor spawns)
    pub compute_handle: ComputeHandleHolder,
    /// Dispute handle holder (set after dispute actor spawns)
    pub dispute_handle: DisputeHandleHolder,
    /// Node profile for this node
    pub node_profile: NodeProfileHandle,
    /// Profile cache for peer profiles
    pub profile_cache: ProfileCache,
    /// Cooperative store
    pub coop_store: Arc<icn_coop::CoopStore>,
    /// Optional federation handler
    pub federation_handler: Option<FederationGossipHandle>,
}

/// Handle trust attestation entries
pub async fn handle_trust_attestation(
    entry: &GossipEntry,
    trust_graph: &TrustGraphHandle,
    own_did: &Did,
) {
    if let Err(e) = crate::trust_propagation::handle_trust_attestation_entry(
        entry, trust_graph, own_did,
        None, // TODO: Add rate limiter in Phase 8A+
    )
    .await
    {
        warn!("Failed to handle trust attestation: {}", e);
    }
}

/// Handle contract deployment entries
pub async fn handle_contract_deployment(entry_data: Vec<u8>, contract_actor: ContractActorHandle) {
    match serde_json::from_slice::<icn_ccl::ContractDeploymentMessage>(&entry_data) {
        Ok(deployment_msg) => {
            let deployer = deployment_msg.installation.installed_by.to_string();
            let actor = contract_actor.write().await;
            if let Err(e) = actor.handle_deployment_message(deployment_msg).await {
                let error_str = e.to_string();
                if error_str.contains("signature") {
                    warn!(
                        "Contract deployment signature verification failed from {}: {}",
                        deployer, e
                    );
                    icn_obs::metrics::contract::deployments_rejected_signature_inc(&deployer);
                } else {
                    warn!("Failed to handle contract deployment: {}", e);
                    icn_obs::metrics::contract::deployments_rejected_inc("handling_error");
                }
            } else {
                info!("Contract deployment processed successfully");
                icn_obs::metrics::contract::deployments_received_inc();
            }
        }
        Err(e) => {
            warn!("Failed to deserialize contract deployment message: {}", e);
            icn_obs::metrics::contract::deployments_rejected_inc("deserialization_error");
        }
    }
}

/// Handle identity recovery entries
pub async fn handle_identity_recovery(
    entry_data: Vec<u8>,
    recovery_store: Arc<dyn icn_store::Store>,
    trust_graph: TrustGraphHandle,
    ledger: LedgerHandle,
) {
    match RecoveryMessage::from_bytes(&entry_data) {
        Ok(recovery_msg) => {
            info!("Received recovery message: {}", recovery_msg.summary());

            // Handle different recovery message types
            let result: Result<bool> = handle_recovery_message(&recovery_msg, recovery_store.as_ref());

            match result {
                Err(ref e) => {
                    warn!("Failed to handle recovery message: {}", e);
                }
                Ok(true) => {
                    // Recovery was successfully finalized - update trust graph and ledger
                    if let RecoveryMessage::Finalized {
                        id, old_did, new_did, ..
                    } = &recovery_msg
                    {
                        apply_recovery_finalization(
                            id,
                            old_did,
                            new_did,
                            &trust_graph,
                            &ledger,
                        )
                        .await;
                    }
                }
                Ok(false) => {
                    // Message handled but no finalization occurred - do nothing
                }
            }
        }
        Err(e) => {
            warn!("Failed to deserialize recovery message: {}", e);
        }
    }
}

/// Handle a single recovery message, returning Ok(true) if recovery was finalized
fn handle_recovery_message(
    recovery_msg: &RecoveryMessage,
    recovery_store: &dyn icn_store::Store,
) -> Result<bool> {
    use icn_identity::RecoveryEvent;

    match recovery_msg {
        RecoveryMessage::Initiated {
            id,
            old_did,
            new_did,
            threshold,
            delay_period,
            ..
        } => {
            let recovery =
                RecoveryEvent::new(old_did.clone(), new_did.clone(), *threshold, *delay_period);

            let key = format!("recovery:{id}");
            let value =
                serde_json::to_vec(&recovery).map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
            recovery_store.put(key.as_bytes(), &value)?;

            info!("Stored new recovery: {} ({} -> {})", id, old_did, new_did);
            Ok(false)
        }
        RecoveryMessage::Attestation {
            recovery_id,
            attestation,
            ..
        } => {
            let key = format!("recovery:{recovery_id}");
            match recovery_store.get(key.as_bytes())? {
                Some(data) => {
                    let mut recovery: RecoveryEvent = serde_json::from_slice(&data)
                        .map_err(|e| anyhow::anyhow!("Deserialization error: {e}"))?;

                    match recovery.add_attestation(attestation.clone()) {
                        Ok(threshold_reached) => {
                            let value = serde_json::to_vec(&recovery)
                                .map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                            recovery_store.put(key.as_bytes(), &value)?;

                            if threshold_reached {
                                info!(
                                    "Recovery {} reached threshold, entering delay period",
                                    recovery_id
                                );
                            } else {
                                info!(
                                    "Added attestation to recovery {}: {}",
                                    recovery_id,
                                    recovery.progress_summary()
                                );
                            }
                            Ok(false)
                        }
                        Err(e) => {
                            warn!(
                                "Failed to add attestation to recovery {}: {}",
                                recovery_id, e
                            );
                            Err(e)
                        }
                    }
                }
                None => {
                    warn!(
                        "Received attestation for unknown recovery: {}",
                        recovery_id
                    );
                    Ok(false)
                }
            }
        }
        RecoveryMessage::Finalized {
            id,
            old_did,
            new_did,
            ..
        } => {
            let key = format!("recovery:{id}");
            match recovery_store.get(key.as_bytes())? {
                Some(data) => {
                    let mut recovery: RecoveryEvent = serde_json::from_slice(&data)
                        .map_err(|e| anyhow::anyhow!("Deserialization error: {e}"))?;

                    match recovery.finalize() {
                        Ok(_) => {
                            let value = serde_json::to_vec(&recovery)
                                .map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                            recovery_store.put(key.as_bytes(), &value)?;

                            info!("Recovery finalized: {} -> {}", old_did, new_did);
                            Ok(true) // Successfully finalized
                        }
                        Err(e) => {
                            warn!("Failed to finalize recovery {}: {}", id, e);
                            Err(e)
                        }
                    }
                }
                None => {
                    warn!("Received finalization for unknown recovery: {}", id);
                    Ok(false)
                }
            }
        }
        RecoveryMessage::Cancelled {
            id,
            cancelled_by,
            reason,
            ..
        } => {
            let key = format!("recovery:{id}");
            match recovery_store.get(key.as_bytes())? {
                Some(data) => {
                    let mut recovery: RecoveryEvent = serde_json::from_slice(&data)
                        .map_err(|e| anyhow::anyhow!("Deserialization error: {e}"))?;

                    match recovery.cancel(cancelled_by.clone(), reason.clone()) {
                        Ok(_) => {
                            let value = serde_json::to_vec(&recovery)
                                .map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                            recovery_store.put(key.as_bytes(), &value)?;

                            info!("Recovery {} cancelled by {}: {}", id, cancelled_by, reason);
                            Ok(false)
                        }
                        Err(e) => {
                            warn!("Failed to cancel recovery {}: {}", id, e);
                            Err(e)
                        }
                    }
                }
                None => {
                    warn!("Received cancellation for unknown recovery: {}", id);
                    Ok(false)
                }
            }
        }
    }
}

/// Apply finalization effects to trust graph and ledger
async fn apply_recovery_finalization(
    id: &str,
    old_did: &Did,
    new_did: &Did,
    trust_graph: &TrustGraphHandle,
    ledger: &LedgerHandle,
) {
    // Update trust graph
    let mut trust = trust_graph.write().await;
    match trust.map_did_recovery(old_did, new_did) {
        Ok(count) => {
            info!(
                "Trust graph: migrated {} edges from {} to {}",
                count, old_did, new_did
            );
        }
        Err(e) => {
            warn!(
                "Failed to migrate trust graph for recovery {}: {}",
                id, e
            );
        }
    }
    drop(trust);

    // Update ledger
    let mut ledger_guard = ledger.write().await;
    match ledger_guard.transfer_balances_for_recovery(old_did, new_did, id) {
        Ok(count) => {
            info!(
                "Ledger: transferred {} currencies from {} to {}",
                count, old_did, new_did
            );
        }
        Err(e) => {
            warn!(
                "Failed to transfer ledger balances for recovery {}: {}",
                id, e
            );
        }
    }
}

/// Handle connection candidate entries for NAT traversal
pub async fn handle_connection_candidate(
    entry_data: Vec<u8>,
    candidate_cache: CandidateCache,
    network_handle: icn_net::NetworkHandle,
) {
    match serde_json::from_slice::<icn_net::ConnectionCandidate>(&entry_data) {
        Ok(candidate) => {
            info!(
                "Received connection candidate from {}: local={}, public={:?}, relay={:?}",
                candidate.did, candidate.local_addr, candidate.public_addr, candidate.relay_addr
            );

            let did = candidate.did.clone();

            // Store the candidate
            if !candidate_cache.store(candidate.clone()).await {
                debug!("Ignored stale/older candidate for {}", did);
                return;
            }

            info!("Stored fresh candidate for {}", did);

            // Check if already connected
            match network_handle.get_peers().await {
                Ok(peers) => {
                    if peers.iter().any(|p| p.did == did) {
                        debug!("Already connected to {}, skipping dial", did);
                        return;
                    }
                }
                Err(e) => {
                    warn!("Failed to get peers: {}", e);
                    return;
                }
            }

            // Try to establish connection
            let mut connected = false;

            // Try local address first (LAN connectivity)
            debug!(
                "Attempting connection to {} via local address {}",
                did, candidate.local_addr
            );
            match network_handle.dial(candidate.local_addr, did.clone()).await {
                Ok(_) => {
                    info!(
                        "Connected to {} via local address {}",
                        did, candidate.local_addr
                    );
                    connected = true;
                }
                Err(e) => {
                    debug!("Failed to connect via local address: {}", e);
                }
            }

            // Try public address if local failed (NAT hole punching)
            if !connected {
                if let Some(public_addr) = candidate.public_addr {
                    debug!(
                        "Attempting connection to {} via public address {}",
                        did, public_addr
                    );
                    match network_handle.dial(public_addr, did.clone()).await {
                        Ok(_) => {
                            info!(
                                "Connected to {} via public address {} (NAT traversal)",
                                did, public_addr
                            );
                            connected = true;
                        }
                        Err(e) => {
                            debug!("Failed to connect via public address: {}", e);
                        }
                    }
                }
            }

            if !connected {
                debug!("Could not establish direct connection to {}", did);
            }
        }
        Err(e) => {
            warn!("Failed to deserialize connection candidate: {}", e);
        }
    }
}

/// Handle snapshot coordination messages
pub async fn handle_snapshot_coordination(
    entry_data: Vec<u8>,
    sender: Did,
    snapshot_coordinator: SnapshotCoordinatorHandle,
    gossip_handle: GossipHandle,
) {
    match bincode::serde::decode_from_slice::<icn_snapshot::SnapshotMessage, _>(
        &entry_data,
        bincode::config::legacy(),
    )
    .map(|(v, _)| v)
    {
        Ok(snapshot_msg) => {
            match snapshot_coordinator
                .write()
                .await
                .handle_message(&sender, snapshot_msg)
                .await
            {
                Ok(response_msgs) => {
                    // Broadcast any response messages
                    for response_msg in response_msgs {
                        let response_bytes = match bincode::serde::encode_to_vec(
                            &response_msg,
                            bincode::config::legacy(),
                        ) {
                            Ok(bytes) => bytes,
                            Err(e) => {
                                warn!("Failed to serialize snapshot response: {}", e);
                                continue;
                            }
                        };

                        let mut gossip_guard = gossip_handle.write().await;
                        match gossip_guard.publish("snapshot:coordinate", response_bytes) {
                            Ok(hash) => {
                                debug!("Published snapshot response with hash: {:?}", hash);
                            }
                            Err(e) => {
                                warn!("Failed to publish snapshot response: {}", e);
                            }
                        }
                    }

                    info!("Processed snapshot message from {}", sender);
                }
                Err(e) => {
                    warn!("Failed to handle snapshot message: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to deserialize snapshot message: {}", e);
        }
    }
}

/// Handle compute topic messages
pub async fn handle_compute_message(entry_data: Vec<u8>, compute_handle: ComputeHandleHolder) {
    match bincode::serde::decode_from_slice::<icn_compute::ComputeMessage, _>(
        &entry_data,
        bincode::config::legacy(),
    )
    .map(|(v, _)| v)
    {
        Ok(compute_msg) => {
            if let Some(handle) = compute_handle.read().await.as_ref() {
                if let Err(e) = handle.handle_gossip(compute_msg).await {
                    warn!("Failed to handle compute message: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to deserialize compute message: {}", e);
        }
    }
}

/// Handle dispute filing messages
pub async fn handle_dispute_message(entry_data: Vec<u8>, dispute_handle: DisputeHandleHolder) {
    match bincode::serde::decode_from_slice::<icn_ccl::DisputeMessage, _>(
        &entry_data,
        bincode::config::legacy(),
    )
    .map(|(v, _)| v)
    {
        Ok(dispute_msg) => {
            if let Some(handle) = dispute_handle.read().await.as_ref() {
                match dispute_msg {
                    icn_ccl::DisputeMessage::DisputeFiled { dispute_id, .. } => {
                        info!(
                            "Received dispute filing notification: {}",
                            hex::encode(dispute_id)
                        );
                        let stats = handle.get_stats().await;
                        icn_obs::metrics::disputes::pending_set(stats.pending as u64);
                        icn_obs::metrics::disputes::investigating_set(stats.investigating as u64);
                    }
                    icn_ccl::DisputeMessage::DisputeResolved {
                        dispute_id,
                        outcome,
                        ..
                    } => {
                        info!(
                            "Received dispute resolution notification: {} - {:?}",
                            hex::encode(dispute_id),
                            outcome
                        );
                        let outcome_type = match outcome {
                            icn_ccl::DisputeOutcome::SubmitterCorrect { .. } => "submitter_correct",
                            icn_ccl::DisputeOutcome::ExecutorCorrect { .. } => "executor_correct",
                            icn_ccl::DisputeOutcome::BothWrong { .. } => "both_wrong",
                            icn_ccl::DisputeOutcome::Inconclusive { .. } => "inconclusive",
                        };
                        icn_obs::metrics::disputes::resolved_inc(outcome_type);

                        let stats = handle.get_stats().await;
                        icn_obs::metrics::disputes::pending_set(stats.pending as u64);
                        icn_obs::metrics::disputes::investigating_set(stats.investigating as u64);
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to deserialize dispute message: {}", e);
        }
    }
}

/// Handle node profile messages
pub async fn handle_node_profile(
    entry_data: Vec<u8>,
    own_did: Did,
    node_profile: NodeProfileHandle,
    profile_cache: ProfileCache,
    gossip_handle: GossipHandle,
) {
    match serde_json::from_slice::<crate::node::ProfileMessage>(&entry_data) {
        Ok(crate::node::ProfileMessage::Announce(profile)) => {
            let peer_did = profile.node_did.clone();
            let roles_count = profile.roles.len();
            let extended_count = profile.extended.capabilities.len();

            let mut cache = profile_cache.write().await;
            cache.insert(peer_did.clone(), profile);

            debug!(
                "Cached profile for peer {}: {} roles, {} extended capabilities",
                peer_did, roles_count, extended_count
            );
        }
        Ok(crate::node::ProfileMessage::Query(queried_did)) => {
            debug!("Received profile query for {}", queried_did);

            let profile_opt = if queried_did == own_did {
                Some(node_profile.read().await.clone())
            } else {
                profile_cache.read().await.get(&queried_did).cloned()
            };

            let response_msg = crate::node::ProfileMessage::Response(profile_opt.clone());
            if let Ok(response_data) = serde_json::to_vec(&response_msg) {
                let mut gossip_write = gossip_handle.write().await;
                if let Err(e) =
                    gossip_write.publish(crate::node::TOPIC_NODE_PROFILES, response_data)
                {
                    warn!("Failed to publish profile response: {}", e);
                } else {
                    debug!(
                        "Published profile response for {}: {}",
                        queried_did,
                        if profile_opt.is_some() {
                            "found"
                        } else {
                            "not found"
                        }
                    );
                }
            }
        }
        Ok(crate::node::ProfileMessage::Response(_)) => {
            // Profile responses are handled by the requester
        }
        Err(e) => {
            warn!("Failed to deserialize profile message: {}", e);
        }
    }
}

/// Handle cooperative update messages
pub async fn handle_coop_update(entry_data: Vec<u8>, coop_store: Arc<icn_coop::CoopStore>) {
    match bincode::serde::decode_from_slice::<icn_coop::Cooperative, _>(
        &entry_data,
        bincode::config::legacy(),
    )
    .map(|(v, _)| v)
    {
        Ok(coop) => {
            let existing = coop_store.get_cooperative(&coop.id);
            if existing.is_ok() {
                debug!(
                    coop_id = %coop.id,
                    "Received coop update for existing cooperative (merging)"
                );
            }

            if let Err(e) = coop_store.save_cooperative(&coop) {
                warn!(
                    coop_id = %coop.id,
                    error = %e,
                    "Failed to save cooperative from gossip"
                );
            } else {
                info!(
                    coop_id = %coop.id,
                    coop_name = %coop.name,
                    "Synced cooperative from gossip"
                );
            }
        }
        Err(e) => {
            warn!(
                error = %e,
                "Failed to deserialize cooperative from gossip"
            );
        }
    }
}

/// Handle federation topic messages
pub async fn handle_federation_message(
    topic: &str,
    entry_data: Vec<u8>,
    federation_handler: FederationGossipHandle,
) {
    if let Err(e) = federation_handler.handle_message(topic, &entry_data) {
        warn!(
            "Failed to handle federation message on {}: {}",
            topic, e
        );
    }
}

/// Create the notification callback that routes gossip entries to handlers
///
/// This returns a callback that can be set on the GossipActor to receive
/// notifications when new entries are received on subscribed topics.
pub fn create_notification_callback(deps: NotificationDeps) -> icn_gossip::EntryNotificationCallback {
    Arc::new(move |topic, entry, _subscriber_did| {
        // Clone dependencies for the async task
        let deps = deps.clone();
        let topic = topic.to_string();
        let entry = entry.clone();

        // Get entry data once for topics that need it
        let entry_data = match entry.get_data() {
            Ok(data) => Some(data),
            Err(e) => {
                if topic != crate::trust_propagation::TRUST_ATTESTATIONS_TOPIC {
                    warn!("Failed to get entry data for topic {}: {}", topic, e);
                }
                None
            }
        };

        // Route based on topic
        if topic == crate::trust_propagation::TRUST_ATTESTATIONS_TOPIC {
            let trust_graph = deps.trust_graph.clone();
            let own_did = deps.own_did.clone();
            tokio::spawn(async move {
                handle_trust_attestation(&entry, &trust_graph, &own_did).await;
            });
        } else if topic == "contracts:deploy" {
            if let Some(data) = entry_data {
                let contract_actor = deps.contract_actor.clone();
                tokio::spawn(async move {
                    handle_contract_deployment(data, contract_actor).await;
                });
            }
        } else if topic == IDENTITY_RECOVERY_TOPIC {
            if let Some(data) = entry_data {
                let recovery_store = deps.recovery_store.clone();
                let trust_graph = deps.trust_graph.clone();
                let ledger = deps.ledger.clone();
                tokio::spawn(async move {
                    handle_identity_recovery(data, recovery_store, trust_graph, ledger).await;
                });
            }
        } else if topic == NETWORK_CANDIDATES_TOPIC {
            if let Some(data) = entry_data {
                let candidate_cache = deps.candidate_cache.clone();
                let network_handle = deps.network_handle.clone();
                tokio::spawn(async move {
                    handle_connection_candidate(data, candidate_cache, network_handle).await;
                });
            }
        } else if topic == "snapshot:coordinate" {
            if let Some(data) = entry_data {
                let sender = entry.author.clone();
                let snapshot_coordinator = deps.snapshot_coordinator.clone();
                let gossip_handle = deps.gossip_handle.clone();
                tokio::spawn(async move {
                    handle_snapshot_coordination(data, sender, snapshot_coordinator, gossip_handle)
                        .await;
                });
            }
        } else if topic == icn_compute::TOPIC_SUBMIT
            || topic == icn_compute::TOPIC_CLAIM
            || topic == icn_compute::TOPIC_RESULT
        {
            if let Some(data) = entry_data {
                let compute_handle = deps.compute_handle.clone();
                tokio::spawn(async move {
                    handle_compute_message(data, compute_handle).await;
                });
            }
        } else if topic == icn_ccl::TOPIC_DISPUTES_FILE {
            if let Some(data) = entry_data {
                let dispute_handle = deps.dispute_handle.clone();
                tokio::spawn(async move {
                    handle_dispute_message(data, dispute_handle).await;
                });
            }
        } else if topic == crate::node::TOPIC_NODE_PROFILES {
            if let Some(data) = entry_data {
                let own_did = deps.own_did.clone();
                let node_profile = deps.node_profile.clone();
                let profile_cache = deps.profile_cache.clone();
                let gossip_handle = deps.gossip_handle.clone();
                tokio::spawn(async move {
                    handle_node_profile(data, own_did, node_profile, profile_cache, gossip_handle)
                        .await;
                });
            }
        } else if topic == init_coop::COOP_UPDATES_TOPIC {
            if let Some(data) = entry_data {
                let coop_store = deps.coop_store.clone();
                tokio::spawn(async move {
                    handle_coop_update(data, coop_store).await;
                });
            }
        } else if topic == icn_federation::TOPIC_FEDERATION_REGISTRY
            || topic == icn_federation::TOPIC_FEDERATION_TRUST
            || topic == icn_federation::TOPIC_FEDERATION_CLEARING
        {
            if let Some(ref handler) = deps.federation_handler {
                if let Some(data) = entry_data {
                    let handler = handler.clone();
                    let topic = topic.clone();
                    tokio::spawn(async move {
                        handle_federation_message(&topic, data, handler).await;
                    });
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_deps_clone() {
        // This test verifies that NotificationDeps implements Clone
        // The actual construction requires many dependencies so we just verify the trait
        fn assert_clone<T: Clone>() {}
        assert_clone::<NotificationDeps>();
    }
}
