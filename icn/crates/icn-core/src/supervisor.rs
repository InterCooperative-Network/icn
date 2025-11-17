//! Supervisor for managing actors

use anyhow::{bail, Context, Result};
use icn_gossip::GossipActor;
use icn_identity::{Did, IdentityBundle, RecoveryMessage, IDENTITY_RECOVERY_TOPIC};
use icn_ledger::Ledger;
use icn_rpc::RpcServer;
use icn_store::SledStore;
use serde_json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::select;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::runtime::ShutdownTx;

/// Gossip topic for NAT traversal connection candidate announcements
const NETWORK_CANDIDATES_TOPIC: &str = "network:candidates";

/// Parse bootstrap peer URL in format: icn://did:icn:PUBKEY@IP:PORT
/// Returns (DID, SocketAddr) on success
///
/// Note: Currently only supports IP addresses. DNS hostname resolution can be added later.
fn parse_bootstrap_peer(url: &str) -> Result<(Did, SocketAddr)> {
    // Check for icn:// prefix
    let url = url.strip_prefix("icn://")
        .context("Bootstrap peer URL must start with 'icn://'")?;

    // Split on @ to get DID and address
    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        bail!("Invalid bootstrap peer format, expected icn://DID@IP:PORT");
    }

    let did_str = parts[0];
    let addr_str = parts[1];

    // Parse DID
    let did: Did = serde_json::from_value(serde_json::Value::String(did_str.to_string()))
        .context("Failed to parse DID")?;

    // Parse socket address (IP:PORT)
    // Note: This requires an IP address, not a DNS hostname
    let addr: SocketAddr = addr_str.parse()
        .context("Failed to parse socket address (must be IP:PORT, DNS names not yet supported)")?;

    Ok((did, addr))
}

/// Supervisor manages all actors and restarts them on failure
pub struct Supervisor {
    config: Config,
    identity_bundle: Option<IdentityBundle>,
    shutdown_tx: ShutdownTx,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(config: Config, identity_bundle: Option<IdentityBundle>, shutdown_tx: ShutdownTx) -> Self {
        Supervisor {
            config,
            identity_bundle,
            shutdown_tx,
        }
    }

    /// Run the supervisor
    pub async fn run(self) -> Result<()> {
        info!("Supervisor starting");

        // Initialize metrics
        icn_obs::init_metrics()?;

        // Start metrics server
        let metrics_port = self.config.observability.metrics_port;
        if let Err(e) = icn_obs::start_metrics_server(metrics_port).await {
            warn!("Failed to start metrics server: {}", e);
        }

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Spawn actors (requires identity bundle from unlocked keystore)
        let (network_handle, gossip_handle, ledger_handle) = if let Some(identity_bundle) = &self.identity_bundle {
            info!("Identity bundle available - spawning actors");

            let did = identity_bundle.did().clone();

            // Create trust graph
            let trust_store_path = self.config.store_path().join("trust");
            let trust_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&trust_store_path)?);
            let trust_graph = icn_trust::TrustGraph::new(trust_store, did.clone());
            let trust_graph_handle = Arc::new(tokio::sync::RwLock::new(trust_graph));

            info!("Trust graph initialized at {}", trust_store_path.display());

            // Create recovery store for social recovery events
            let recovery_store_path = self.config.store_path().join("recovery");
            let recovery_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&recovery_store_path)?);
            info!("Recovery store initialized at {}", recovery_store_path.display());

            // Create trust lookup closure for gossip actor
            let trust_graph_for_gossip = trust_graph_handle.clone();
            let trust_lookup = Arc::new(move |peer_did: &icn_identity::Did| {
                // Use a blocking task since we're in a sync context
                let graph = trust_graph_for_gossip.clone();
                let peer = peer_did.clone();
                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let graph = graph.read().await;
                        graph.trust_class(&peer).ok()
                    })
                })
            });

            // Spawn Gossip actor with trust graph for fine-grained trust-based subscription control
            let gossip_handle = GossipActor::spawn_with_trust_graph(
                did.clone(),
                trust_lookup,
                Some(trust_graph_handle.clone()),
            );

            info!("Gossip actor spawned with trust-gated subscription support");

            // Set keypair for signing outgoing gossip messages
            {
                let mut gossip = gossip_handle.write().await;
                gossip.set_keypair(identity_bundle.keypair().clone());
            }

            info!("Gossip actor configured with signing keypair");

            // Restore gossip state from snapshot if available
            let data_dir = self.config.store_path();
            let load_start = std::time::Instant::now();
            let load_result = icn_snapshot::load_snapshot(&data_dir);
            let load_duration = load_start.elapsed();

            let loaded_snapshot = match load_result {
                Ok(Some(snapshot)) => {
                    icn_obs::metrics::snapshot::load_total_inc();
                    icn_obs::metrics::snapshot::load_duration_record(load_duration.as_secs_f64());

                    info!("Found state snapshot (version {}, created at {}) - loaded in {:.3}s",
                          snapshot.version, snapshot.created_at, load_duration.as_secs_f64());

                    // Warn if checksum file is missing (legacy snapshot)
                    let checksum_path = data_dir.join("state.snapshot.sha256");
                    if !checksum_path.exists() {
                        warn!("⚠️  Snapshot loaded without checksum verification (legacy snapshot). Run 'icnctl snapshot create' to generate checksum.");
                    } else {
                        info!("✅ Snapshot checksum verified");
                    }

                    // Record snapshot contents metrics
                    if let Some(ref gossip_state) = snapshot.gossip_state {
                        icn_obs::metrics::snapshot::gossip_vector_clock_entries_set(gossip_state.vector_clock.len());
                        icn_obs::metrics::snapshot::gossip_subscriptions_set(gossip_state.subscriptions.len());
                        icn_obs::metrics::snapshot::gossip_topics_set(gossip_state.topics.len());
                    }
                    if let Some(ref network_state) = snapshot.network_state {
                        icn_obs::metrics::snapshot::network_x25519_keys_set(network_state.peer_x25519_keys.len());
                    }

                    if let Some(gossip_state) = snapshot.gossip_state.clone() {
                        let mut gossip = gossip_handle.write().await;
                        if let Err(e) = gossip.restore_state(gossip_state) {
                            warn!("Failed to restore gossip state: {}", e);
                        } else {
                            info!("✅ Gossip state restored from snapshot");
                        }
                    }

                    Some(snapshot)
                }
                Ok(None) => {
                    debug!("No state snapshot found, starting with fresh state");
                    None
                }
                Err(e) => {
                    icn_obs::metrics::snapshot::load_errors_inc();
                    warn!("Failed to load state snapshot: {}", e);
                    None
                }
            };

            // Spawn Ledger
            let store_path = self.config.store_path().join("ledger");
            let store = Arc::new(SledStore::open(&store_path)?);
            let mut ledger = Ledger::new(store)?;
            ledger.set_gossip(gossip_handle.clone());
            let ledger_handle = Arc::new(tokio::sync::RwLock::new(ledger));

            info!("Ledger initialized at {}", store_path.display());

            // Initialize Contract Runtime
            let contract_runtime = icn_ccl::ContractRuntime::new(ledger_handle.clone());
            let contract_runtime_handle = Arc::new(tokio::sync::RwLock::new(contract_runtime));

            info!("Contract runtime initialized");

            // Create ContractActor
            let contract_actor = icn_ccl::ContractActor::new(
                did.clone(),
                contract_runtime_handle.clone(),
                Some(trust_graph_handle.clone()),
            );
            let contract_actor_handle = Arc::new(tokio::sync::RwLock::new(contract_actor));

            info!("Contract actor created");

            // TODO: Spawn Identity actor
            // let identity_handle = IdentityActor::spawn(
            //     self.config.keystore_path(),
            //     self.config.store_path(),
            //     keypair.clone(),
            //     self.shutdown_tx.clone()
            // )?;

            // Spawn Network actor with gossip bridge
            let listen_addr: std::net::SocketAddr = self.config.network.listen_addr.parse()?;

            // Create incoming message handler that routes to gossip
            let gossip_handle_clone = gossip_handle.clone();
            let network_handle_for_handler = Arc::new(tokio::sync::RwLock::new(None::<icn_net::NetworkHandle>));
            let network_handle_for_handler_clone = network_handle_for_handler.clone();
            let own_did_for_handler = did.clone();

            let incoming_handler: icn_net::IncomingMessageHandler = Arc::new(move |net_msg| {
                let sender_did = net_msg.from.clone();

                match net_msg.payload {
                    icn_net::MessagePayload::Gossip(gossip_msg) => {
                        // Spawn async task to avoid blocking the callback thread
                        let gossip_handle = gossip_handle_clone.clone();
                        let sender = sender_did.clone();
                        tokio::spawn(async move {
                            let mut gossip = gossip_handle.write().await;
                            if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                                warn!("Failed to handle gossip message: {}", e);
                            }
                        });
                    }

                    icn_net::MessagePayload::Subscribe { topics } => {
                        info!("Received Subscribe from {} for topics: {:?}", sender_did, topics);
                        icn_obs::metrics::gossip::subscribes_received_inc();

                        // Spawn async task to avoid blocking the callback thread
                        let gossip_handle = gossip_handle_clone.clone();
                        let network_handle_lock = network_handle_for_handler_clone.clone();
                        let own_did = own_did_for_handler.clone();

                        tokio::spawn(async move {
                            let mut gossip = gossip_handle.write().await;
                            let mut acked_topics = Vec::new();

                            for topic in &topics {
                                match gossip.subscribe(topic, sender_did.clone()) {
                                    Ok(_) => {
                                        info!("Subscribed {} to topic: {}", sender_did, topic);
                                        acked_topics.push(topic.clone());
                                    }
                                    Err(e) => {
                                        warn!("Failed to subscribe {} to topic {}: {}", sender_did, topic, e);
                                    }
                                }
                            }

                            // Send SubscribeAck back if we have any successful subscriptions
                            if !acked_topics.is_empty() {
                                icn_obs::metrics::gossip::subscribe_acks_sent_inc();
                                if let Some(net_handle) = network_handle_lock.read().await.as_ref() {
                                    let ack_msg = icn_net::NetworkMessage::subscribe_ack(
                                        own_did,
                                        sender_did.clone(),
                                        acked_topics
                                    );
                                    if let Err(e) = net_handle.send_message(sender_did, ack_msg).await {
                                        warn!("Failed to send SubscribeAck: {}", e);
                                    }
                                }
                            }
                        });
                    }

                    icn_net::MessagePayload::Unsubscribe { topics } => {
                        info!("Received Unsubscribe from {} for topics: {:?}", sender_did, topics);
                        icn_obs::metrics::gossip::unsubscribes_received_inc();

                        // Spawn async task to avoid blocking the callback thread
                        let gossip_handle = gossip_handle_clone.clone();
                        tokio::spawn(async move {
                            let mut gossip = gossip_handle.write().await;
                            for topic in &topics {
                                match gossip.unsubscribe(topic, &sender_did) {
                                    Ok(_) => {
                                        info!("Unsubscribed {} from topic: {}", sender_did, topic);
                                    }
                                    Err(e) => {
                                        warn!("Failed to unsubscribe {} from topic {}: {}", sender_did, topic, e);
                                    }
                                }
                            }
                        });
                    }

                    icn_net::MessagePayload::SubscribeAck { topics } => {
                        info!("Received SubscribeAck from {} for topics: {:?}", sender_did, topics);
                        // Track successful subscription acknowledgment
                        // In a full implementation, this could update a local subscription registry
                    }

                    icn_net::MessagePayload::Ping => {
                        // Ping/Pong handled by network actor
                    }

                    icn_net::MessagePayload::Pong => {
                        // Ping/Pong handled by network actor
                    }

                    icn_net::MessagePayload::Handshake { .. } => {
                        // Handshake handled internally by network actor
                    }

                    icn_net::MessagePayload::HandshakeAck => {
                        // Handshake ack handled internally by network actor
                    }

                    icn_net::MessagePayload::Hello { .. } => {
                        // Hello message with DID-TLS binding handled internally by network actor
                    }

                    icn_net::MessagePayload::Signed(ref envelope) => {
                        // Signed messages have been verified by NetworkActor
                        // The signature and replay protection checks have passed
                        debug!("Received verified signed message from {} (seq={}, type={:?})",
                               envelope.from, envelope.sequence, envelope.payload_type);

                        // Route based on payload type
                        match envelope.payload_type {
                            icn_net::PayloadType::Gossip => {
                                // Decode gossip message from signed envelope
                                let gossip_msg: icn_gossip::GossipMessage = match envelope.decode_payload() {
                                    Ok(msg) => msg,
                                    Err(e) => {
                                        warn!("Failed to decode gossip payload from {}: {}", envelope.from, e);
                                        return;
                                    }
                                };

                                // Handle gossip message (sender is authenticated via signature)
                                let gossip_handle = gossip_handle_clone.clone();
                                let sender = envelope.from.clone();
                                tokio::spawn(async move {
                                    let mut gossip = gossip_handle.write().await;
                                    if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                                        warn!("Failed to handle gossip message from {}: {}", sender, e);
                                    }
                                });
                            }

                            _ => {
                                // Other payload types not yet implemented
                                debug!("Received signed message with unhandled payload type: {:?}", envelope.payload_type);
                            }
                        }
                    }
                }
            });

            // Prepare rate limiting configuration
            let (trust_graph_for_rate_limit, trust_gated_config, fallback_config) = if self.config.rate_limiting.enabled {
                (
                    Some(trust_graph_handle.clone()), // Enable trust-gated rate limiting
                    Some(self.config.rate_limiting.to_trust_gated_config()),
                    Some(self.config.rate_limiting.to_fallback_config()),
                )
            } else {
                (None, None, None) // Disable trust-gated rate limiting
            };

            // Use identity bundle loaded from keystore (preserves TLS cert across restarts)
            info!("Using identity bundle with DID-TLS binding: {}", identity_bundle.did());

            // Parse STUN servers from config
            let stun_servers = if !self.config.network.stun_servers.is_empty() {
                let mut parsed_servers = Vec::new();
                for server_str in &self.config.network.stun_servers {
                    // Try DNS resolution for hostname-based servers
                    match tokio::net::lookup_host(server_str).await {
                        Ok(mut addrs) => {
                            if let Some(addr) = addrs.next() {
                                parsed_servers.push(addr);
                                info!("Resolved STUN server {} to {}", server_str, addr);
                            } else {
                                warn!("No addresses found for STUN server: {}", server_str);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to resolve STUN server {}: {}", server_str, e);
                        }
                    }
                }
                if !parsed_servers.is_empty() {
                    Some(parsed_servers)
                } else {
                    None
                }
            } else {
                None
            };

            let network_handle = icn_net::NetworkActor::spawn(
                identity_bundle.clone(),
                listen_addr,
                self.shutdown_tx.clone(),
                Some(incoming_handler),
                trust_graph_for_rate_limit,
                trust_gated_config,
                fallback_config,
                Some(self.config.topology.clone()),
                stun_servers,
            )
            .await?;

            // Initialize network handle for the incoming message handler
            *network_handle_for_handler.write().await = Some(network_handle.clone());

            info!("Network actor spawned on {}", listen_addr);

            // Restore network state from snapshot if available (re-use snapshot loaded earlier)
            if let Some(snapshot) = loaded_snapshot {
                if let Some(network_state) = snapshot.network_state {
                    if let Err(e) = network_handle.restore_state(network_state).await {
                        warn!("Failed to restore network state: {}", e);
                    } else {
                        info!("✅ Network state restored from snapshot");
                    }
                }
            }

            // Set send callback on gossip actor to enable request/response
            {
                let mut gossip = gossip_handle.write().await;
                let network_handle_clone = network_handle.clone();
                let own_did_clone = did.clone();
                let keypair_clone = identity_bundle.keypair().clone();
                let sequence_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

                let send_callback: icn_gossip::SendMessageCallback = Arc::new(move |recipient, gossip_msg| {
                    let net_handle = network_handle_clone.clone();
                    let from_did = own_did_clone.clone();
                    let keypair = keypair_clone.clone();
                    let sequence_ctr = sequence_counter.clone();

                    // Track metrics based on message type
                    use icn_gossip::GossipMessage;
                    match &gossip_msg {
                        GossipMessage::Announce { .. } => icn_obs::metrics::gossip::announces_sent_inc(),
                        GossipMessage::Request { .. } => icn_obs::metrics::gossip::requests_sent_inc(),
                        GossipMessage::Response { .. } => icn_obs::metrics::gossip::responses_sent_inc(),
                        _ => {} // Other message types
                    }

                    // Spawn async task to send message
                    tokio::spawn(async move {
                        // Get next sequence number
                        let sequence = sequence_ctr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                        // Create signed envelope
                        let envelope = match icn_net::SignedEnvelope::from_payload(
                            &from_did,
                            &keypair,
                            sequence,
                            icn_net::PayloadType::Gossip,
                            &gossip_msg,
                        ) {
                            Ok(env) => env,
                            Err(e) => {
                                warn!("Failed to create signed envelope: {}", e);
                                return;
                            }
                        };

                        // Send signed message
                        let result = if let Some(target_did) = recipient {
                            // Unicast
                            let net_msg = icn_net::NetworkMessage::signed(Some(target_did.clone()), envelope);
                            net_handle.send_message(target_did, net_msg).await
                        } else {
                            // Broadcast
                            let net_msg = icn_net::NetworkMessage::signed(None, envelope);
                            net_handle.broadcast(net_msg).await
                        };

                        if let Err(e) = result {
                            warn!("Failed to send gossip message: {}", e);
                        }
                    });
                });

                gossip.set_send_callback(send_callback);

                // Set up notification callback for trust attestations, contract deployments, and recovery events
                let trust_graph_for_notifications = trust_graph_handle.clone();
                let own_did_for_notifications = did.clone();
                let contract_actor_for_notifications = contract_actor_handle.clone();
                let recovery_store_for_notifications = recovery_store.clone();
                let ledger_for_notifications = ledger_handle.clone();

                // Create candidate cache for NAT traversal
                let candidate_cache = Arc::new(icn_net::CandidateCache::new());
                let candidate_cache_for_notifications = candidate_cache.clone();
                let network_handle_for_candidates = network_handle.clone();

                let notification_callback: icn_gossip::EntryNotificationCallback = Arc::new(move |topic, entry, _subscriber_did| {
                    // Handle trust attestations
                    if topic == crate::trust_propagation::TRUST_ATTESTATIONS_TOPIC {
                        let trust_graph = trust_graph_for_notifications.clone();
                        let own_did = own_did_for_notifications.clone();

                        tokio::spawn(async move {
                            if let Err(e) = crate::trust_propagation::handle_trust_attestation_entry(
                                &entry,
                                &trust_graph,
                                &own_did,
                                None, // TODO: Add rate limiter in Phase 8A+
                            ).await {
                                warn!("Failed to handle trust attestation: {}", e);
                            }
                        });
                    }
                    // Handle contract deployments
                    else if topic == "contracts:deploy" {
                        let contract_actor = contract_actor_for_notifications.clone();
                        // Use get_data() to handle decompression if needed
                        let entry_data = match entry.get_data() {
                            Ok(data) => data,
                            Err(e) => {
                                warn!("Failed to get entry data: {}", e);
                                return;
                            }
                        };

                        tokio::spawn(async move {
                            // Deserialize contract deployment message
                            match serde_json::from_slice::<icn_ccl::ContractDeploymentMessage>(&entry_data) {
                                Ok(deployment_msg) => {
                                    let deployer = deployment_msg.installation.installed_by.to_string();
                                    let actor = contract_actor.write().await;
                                    if let Err(e) = actor.handle_deployment_message(deployment_msg).await {
                                        let error_str = e.to_string();
                                        if error_str.contains("signature") {
                                            warn!("Contract deployment signature verification failed from {}: {}", deployer, e);
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
                        });
                    }
                    // Handle identity recovery events
                    else if topic == IDENTITY_RECOVERY_TOPIC {
                        let recovery_store = recovery_store_for_notifications.clone();
                        let trust_graph = trust_graph_for_notifications.clone();
                        let ledger = ledger_for_notifications.clone();

                        // Use get_data() to handle decompression if needed
                        let entry_data = match entry.get_data() {
                            Ok(data) => data,
                            Err(e) => {
                                warn!("Failed to get recovery entry data: {}", e);
                                return;
                            }
                        };

                        tokio::spawn(async move {
                            use icn_identity::RecoveryEvent;

                            // Deserialize recovery message
                            match RecoveryMessage::from_bytes(&entry_data) {
                                Ok(recovery_msg) => {
                                    info!("Received recovery message: {}", recovery_msg.summary());

                                    // Handle different recovery message types
                                    // Returns Ok(true) if recovery was finalized, Ok(false) otherwise, Err on failure
                                    let result: Result<bool> = (|| {
                                        match &recovery_msg {
                                            RecoveryMessage::Initiated { id, old_did, new_did, threshold, delay_period, timestamp: _ } => {
                                                // Create new recovery event
                                                let recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), *threshold, *delay_period);

                                                // Store recovery event
                                                let key = format!("recovery:{id}");
                                                let value = serde_json::to_vec(&recovery).map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                                                recovery_store.put(key.as_bytes(), &value)?;

                                                info!("Stored new recovery: {} ({} → {})", id, old_did, new_did);
                                                Ok(false)
                                            }
                                            RecoveryMessage::Attestation { recovery_id, attestation, .. } => {
                                            // Load existing recovery
                                            let key = format!("recovery:{recovery_id}");
                                            match recovery_store.get(key.as_bytes())? {
                                                Some(data) => {
                                                    let mut recovery: RecoveryEvent = serde_json::from_slice(&data)
                                                        .map_err(|e| anyhow::anyhow!("Deserialization error: {e}"))?;

                                                    // Add attestation
                                                    match recovery.add_attestation(attestation.clone()) {
                                                        Ok(threshold_reached) => {
                                                            // Save updated recovery
                                                            let value = serde_json::to_vec(&recovery)
                                                                .map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                                                            recovery_store.put(key.as_bytes(), &value)?;

                                                            if threshold_reached {
                                                                info!("Recovery {} reached threshold, entering delay period", recovery_id);
                                                            } else {
                                                                info!("Added attestation to recovery {}: {}", recovery_id, recovery.progress_summary());
                                                            }
                                                            Ok(false)
                                                        }
                                                        Err(e) => {
                                                            warn!("Failed to add attestation to recovery {}: {}", recovery_id, e);
                                                            Err(e)
                                                        }
                                                    }
                                                }
                                                None => {
                                                    warn!("Received attestation for unknown recovery: {}", recovery_id);
                                                    Ok(false)
                                                }
                                            }
                                        }
                                        RecoveryMessage::Finalized { id, old_did, new_did, .. } => {
                                            // Load recovery and mark as finalized
                                            let key = format!("recovery:{id}");
                                            match recovery_store.get(key.as_bytes())? {
                                                Some(data) => {
                                                    let mut recovery: RecoveryEvent = serde_json::from_slice(&data)
                                                        .map_err(|e| anyhow::anyhow!("Deserialization error: {e}"))?;

                                                    // Finalize the recovery
                                                    match recovery.finalize() {
                                                        Ok(_) => {
                                                            let value = serde_json::to_vec(&recovery)
                                                                .map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                                                            recovery_store.put(key.as_bytes(), &value)?;

                                                            info!("✅ Recovery finalized: {} → {}", old_did, new_did);
                                                            Ok(true)  // Successfully finalized - trigger trust/ledger updates
                                                        }
                                                        Err(e) => {
                                                            warn!("Failed to finalize recovery {}: {}", id, e);
                                                            Err(e)
                                                        }
                                                    }
                                                }
                                                None => {
                                                    warn!("Received finalization for unknown recovery: {}", id);
                                                    Ok(false)  // Unknown recovery - don't trigger trust/ledger updates
                                                }
                                            }
                                        }
                                        RecoveryMessage::Cancelled { id, cancelled_by, reason, .. } => {
                                            // Load recovery and mark as cancelled
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

                                                            info!("⚠️  Recovery {} cancelled by {}: {}", id, cancelled_by, reason);
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
                                    })();

                                    match result {
                                        Err(ref e) => {
                                            warn!("Failed to handle recovery message: {}", e);
                                        }
                                        Ok(true) => {
                                            // Recovery was successfully finalized - update trust graph and ledger
                                            if let RecoveryMessage::Finalized { id, old_did, new_did, .. } = &recovery_msg {
                                            // Update trust graph: map old_did relationships to new_did
                                            let mut trust = trust_graph.write().await;
                                            match trust.map_did_recovery(old_did, new_did) {
                                                Ok(count) => {
                                                    info!("Trust graph: migrated {} edges from {} to {}", count, old_did, new_did);
                                                }
                                                Err(e) => {
                                                    warn!("Failed to migrate trust graph for recovery {}: {}", id, e);
                                                }
                                            }
                                            drop(trust); // Release write lock

                                            // Update ledger: transfer balances from old_did to new_did
                                            let mut ledger_guard = ledger.write().await;
                                            match ledger_guard.transfer_balances_for_recovery(old_did, new_did, id) {
                                                Ok(count) => {
                                                    info!("Ledger: transferred {} currencies from {} to {}", count, old_did, new_did);
                                                }
                                                Err(e) => {
                                                    warn!("Failed to transfer ledger balances for recovery {}: {}", id, e);
                                                }
                                            }
                                            drop(ledger_guard); // Release write lock
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
                        });
                    }
                    // Handle connection candidates for NAT traversal
                    else if topic == NETWORK_CANDIDATES_TOPIC {
                        // Use get_data() to handle decompression if needed
                        let entry_data = match entry.get_data() {
                            Ok(data) => data,
                            Err(e) => {
                                warn!("Failed to get candidate entry data: {}", e);
                                return;
                            }
                        };

                        // Deserialize connection candidate
                        match serde_json::from_slice::<icn_net::ConnectionCandidate>(&entry_data) {
                            Ok(candidate) => {
                                info!("Received connection candidate from {}: local={}, public={:?}, relay={:?}",
                                      candidate.did, candidate.local_addr, candidate.public_addr, candidate.relay_addr);

                                // Store candidate in cache and attempt connection (Phase 3: Hole Punching)
                                let cache = candidate_cache_for_notifications.clone();
                                let net_handle = network_handle_for_candidates.clone();
                                let did = candidate.did.clone();

                                tokio::spawn(async move {
                                    // Store the candidate
                                    if !cache.store(candidate.clone()).await {
                                        debug!("Ignored stale/older candidate for {}", did);
                                        return;
                                    }

                                    info!("✓ Stored fresh candidate for {}", did);

                                    // Check if already connected
                                    match net_handle.get_peers().await {
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
                                    // Priority: 1) local_addr, 2) public_addr (for NAT hole punching)
                                    let mut connected = false;

                                    // Try local address first (LAN connectivity)
                                    debug!("Attempting connection to {} via local address {}", did, candidate.local_addr);
                                    match net_handle.dial(candidate.local_addr, did.clone()).await {
                                        Ok(_) => {
                                            info!("✅ Connected to {} via local address {}", did, candidate.local_addr);
                                            connected = true;
                                        }
                                        Err(e) => {
                                            debug!("Failed to connect via local address: {}", e);
                                        }
                                    }

                                    // If local connection failed, try public address (NAT hole punching)
                                    if !connected {
                                        if let Some(public_addr) = candidate.public_addr {
                                            debug!("Attempting connection to {} via public address {}", did, public_addr);
                                            match net_handle.dial(public_addr, did.clone()).await {
                                                Ok(_) => {
                                                    info!("✅ Connected to {} via public address {} (NAT traversal)", did, public_addr);
                                                    connected = true;
                                                }
                                                Err(e) => {
                                                    debug!("Failed to connect via public address: {}", e);
                                                }
                                            }
                                        }
                                    }

                                    // TODO Phase 4: Try relay address (TURN relay) if both direct methods failed
                                    if !connected {
                                        debug!("Could not establish direct connection to {}", did);
                                    }
                                });
                            }
                            Err(e) => {
                                warn!("Failed to deserialize connection candidate: {}", e);
                            }
                        }
                    }
                });

                gossip.set_notification_callback(notification_callback);

                // Set up peer sampling callback for scope-aware gossip fanout
                let network_handle_for_sampling = network_handle.clone();
                let peer_sampling_callback: icn_gossip::PeerSamplingCallback = Arc::new(move |scope, count| {
                    let net_handle = network_handle_for_sampling.clone();
                    // Use tokio::task::block_in_place to safely block in async context
                    tokio::task::block_in_place(move || {
                        tokio::runtime::Handle::current().block_on(async move {
                            net_handle.sample_peers(scope, count).await
                        })
                    })
                });

                gossip.set_peer_sampling(peer_sampling_callback);

                // Subscribe to trust attestations topic
                if let Err(e) = gossip.subscribe(crate::trust_propagation::TRUST_ATTESTATIONS_TOPIC, did.clone()) {
                    warn!("Failed to subscribe to trust attestations topic: {}", e);
                } else {
                    info!("Subscribed to trust:attestations topic");
                }

                // Subscribe to contracts:deploy topic with trust-gated access (min trust 0.4)
                if let Err(e) = gossip.subscribe("contracts:deploy", did.clone()) {
                    warn!("Failed to subscribe to contracts:deploy topic: {}", e);
                } else {
                    info!("Subscribed to contracts:deploy topic");
                }

                // Subscribe to identity:recovery topic for social recovery events
                if let Err(e) = gossip.subscribe(IDENTITY_RECOVERY_TOPIC, did.clone()) {
                    warn!("Failed to subscribe to identity:recovery topic: {}", e);
                } else {
                    info!("Subscribed to identity:recovery topic");
                }

                // Subscribe to network:candidates topic for NAT traversal peer discovery
                if let Err(e) = gossip.subscribe(NETWORK_CANDIDATES_TOPIC, did.clone()) {
                    warn!("Failed to subscribe to network:candidates topic: {}", e);
                } else {
                    info!("Subscribed to network:candidates topic");
                }
            }

            info!("Gossip send callback configured");

            // Dial bootstrap peers for WAN connectivity
            if !self.config.network.bootstrap_peers.is_empty() {
                info!("Dialing {} bootstrap peers", self.config.network.bootstrap_peers.len());
                for peer_url in &self.config.network.bootstrap_peers {
                    match parse_bootstrap_peer(peer_url) {
                        Ok((peer_did, peer_addr)) => {
                            info!("Connecting to bootstrap peer: {} at {}", peer_did, peer_addr);
                            match network_handle.dial(peer_addr, peer_did.clone()).await {
                                Ok(_) => info!("✓ Connected to bootstrap peer: {}", peer_did),
                                Err(e) => warn!("Failed to connect to bootstrap peer {}: {}", peer_did, e),
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse bootstrap peer URL '{}': {}", peer_url, e);
                        }
                    }
                }
            }

            // Announce connection candidate for NAT traversal
            {
                info!("Announcing connection candidate for NAT traversal...");
                match network_handle.connection_candidate().await {
                    Ok(candidate) => {
                        info!("Connection candidate: local={}, public={:?}, relay={:?}",
                              candidate.local_addr, candidate.public_addr, candidate.relay_addr);

                        // Serialize candidate and publish to gossip
                        match serde_json::to_vec(&candidate) {
                            Ok(candidate_bytes) => {
                                let mut gossip = gossip_handle.write().await;
                                match gossip.publish(NETWORK_CANDIDATES_TOPIC, candidate_bytes) {
                                    Ok(_) => info!("✓ Published connection candidate to gossip"),
                                    Err(e) => warn!("Failed to publish connection candidate: {}", e),
                                }
                            }
                            Err(e) => warn!("Failed to serialize connection candidate: {}", e),
                        }
                    }
                    Err(e) => warn!("Failed to get connection candidate: {}", e),
                }
            }

            // Spawn Governance actor
            let gov_store_path = self.config.store_path().join("governance");
            let gov_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&gov_store_path)?);
            let gov_resolver: Arc<dyn icn_governance::MembershipResolver + Send + Sync> =
                Arc::new(icn_governance::StaticMembershipResolver::new());

            let governance_handle = crate::governance::GovernanceActor::spawn(
                did.clone(),
                gov_store,
                gossip_handle.clone(),
                gov_resolver,
            )
            .await?;

            info!("✓ Governance actor spawned at {}", gov_store_path.display());

            // Spawn RPC server with network, ledger, contract, gossip, and governance handles
            let rpc_port = self.config.network.rpc_port;
            let rpc_addr = format!("127.0.0.1:{rpc_port}").parse()?;
            let mut rpc_server = RpcServer::new(rpc_addr);
            rpc_server.set_network_handle(network_handle.clone());
            rpc_server.set_ledger_handle(ledger_handle.clone());
            rpc_server.set_contract_runtime(contract_runtime_handle.clone());
            rpc_server.set_gossip_handle(gossip_handle.clone());
            rpc_server.set_governance_handle(governance_handle);

            tokio::spawn(async move {
                if let Err(e) = rpc_server.run().await {
                    warn!("RPC server error: {}", e);
                }
            });

            info!("RPC server spawned on {}", rpc_addr);

            // Spawn Gateway API server if enabled
            if self.config.gateway.enabled {
                let gateway_addr: std::net::SocketAddr = self.config.gateway.bind_addr.parse()?;

                // Check that JWT secret is configured
                if self.config.gateway.jwt_secret.is_empty() {
                    warn!("Gateway enabled but JWT secret not configured - gateway will not start");
                    warn!("Set jwt_secret in config or ICN_GATEWAY_JWT_SECRET environment variable");
                } else {
                    let jwt_secret = self.config.gateway.jwt_secret.clone().into_bytes();

                    // Spawn gateway in a dedicated thread (actix-web has its own runtime)
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            let gateway_server = icn_gateway::GatewayServer::new(gateway_addr, jwt_secret);
                            if let Err(e) = gateway_server.run().await {
                                warn!("Gateway server error: {}", e);
                            }
                        });
                    });

                    info!("Gateway API spawned on {}", gateway_addr);
                }
            } else {
                debug!("Gateway API disabled in configuration");
            }

            // Spawn anti-entropy task
            let anti_entropy_config = crate::anti_entropy::AntiEntropyConfig::default();
            let _anti_entropy_handle = crate::anti_entropy::spawn_anti_entropy_task(
                gossip_handle.clone(),
                network_handle.clone(),
                did.clone(),
                anti_entropy_config,
                self.shutdown_tx.subscribe(),
            );

            info!("Anti-entropy task spawned");

            // Start periodic digest emitter
            let _digest_emitter_handle = icn_gossip::start_digest_emitter(
                gossip_handle.clone(),
                10_000, // 10 seconds base interval
                2_000,  // ±2 seconds jitter
                self.shutdown_tx.subscribe(),
            );

            info!("Digest emitter spawned");

            // Spawn metrics update task
            let start_time = std::time::Instant::now();
            let network_handle_metrics = network_handle.clone();
            let mut metrics_shutdown = self.shutdown_tx.subscribe();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Update uptime
                            let uptime_secs = start_time.elapsed().as_secs();
                            icn_obs::metrics::system::uptime_seconds_set(uptime_secs);

                            // Count active actors (network + gossip + ledger + rpc + anti-entropy + digest-emitter = 6)
                            icn_obs::metrics::system::actors_active_set(6);

                            // Update network stats (this also updates metrics via GetStats handler)
                            let _ = network_handle_metrics.get_stats().await;
                        }
                        _ = metrics_shutdown.recv() => {
                            break;
                        }
                    }
                }
            });

            info!("Metrics update task spawned");

            (Some(network_handle), Some(gossip_handle), Some(ledger_handle))
        } else {
            warn!("No identity bundle available - actors not spawned");
            warn!("Run 'icnctl id init' to create an identity");

            // Still spawn metrics update task for system metrics
            let start_time = std::time::Instant::now();
            let mut metrics_shutdown = self.shutdown_tx.subscribe();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Update system metrics even without actors
                            let uptime_secs = start_time.elapsed().as_secs();
                            icn_obs::metrics::system::uptime_seconds_set(uptime_secs);
                            icn_obs::metrics::system::actors_active_set(0);
                        }
                        _ = metrics_shutdown.recv() => {
                            break;
                        }
                    }
                }
            });

            info!("Metrics update task spawned (system metrics only)");

            (None, None, None)
        };

        // Wait for shutdown signal
        select! {
            _ = shutdown_rx.recv() => {
                info!("Supervisor received shutdown signal");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Supervisor received Ctrl+C");
                let _ = self.shutdown_tx.send(());
            }
        }

        // Graceful shutdown of actors
        info!("Supervisor shutting down actors");

        // Save state snapshot before actors are dropped
        if gossip_handle.is_some() || network_handle.is_some() {
            info!("Saving state snapshot before shutdown");
            let mut snapshot = icn_snapshot::StateSnapshot::new();

            // Export gossip state
            if let Some(ref gossip_handle) = gossip_handle {
                let gossip = gossip_handle.read().await;
                snapshot.gossip_state = Some(gossip.export_state());
                info!("Exported gossip state: {} vector clock entries, {} subscriptions",
                      snapshot.gossip_state.as_ref().unwrap().vector_clock.len(),
                      snapshot.gossip_state.as_ref().unwrap().subscriptions.len());
            }

            // Export network state
            if let Some(ref network_handle) = network_handle {
                // Need to use blocking context for async export_state
                let state = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        network_handle.export_state().await
                    })
                });
                snapshot.network_state = Some(state);
                info!("Exported network state: {} peer X25519 keys",
                      snapshot.network_state.as_ref().unwrap().peer_x25519_keys.len());
            }

            // Record snapshot metrics before saving
            if let Some(ref gossip_state) = snapshot.gossip_state {
                icn_obs::metrics::snapshot::gossip_vector_clock_entries_set(gossip_state.vector_clock.len());
                icn_obs::metrics::snapshot::gossip_subscriptions_set(gossip_state.subscriptions.len());
                icn_obs::metrics::snapshot::gossip_topics_set(gossip_state.topics.len());
            }
            if let Some(ref network_state) = snapshot.network_state {
                icn_obs::metrics::snapshot::network_x25519_keys_set(network_state.peer_x25519_keys.len());
            }

            // Save snapshot to disk
            let data_dir = self.config.store_path();
            let save_start = std::time::Instant::now();
            let save_result = icn_snapshot::save_snapshot(&snapshot, &data_dir);
            let save_duration = save_start.elapsed();

            match save_result {
                Ok(()) => {
                    icn_obs::metrics::snapshot::save_total_inc();
                    icn_obs::metrics::snapshot::save_duration_record(save_duration.as_secs_f64());

                    // Record snapshot file size
                    let snapshot_path = data_dir.join("state.snapshot");
                    if let Ok(metadata) = std::fs::metadata(&snapshot_path) {
                        icn_obs::metrics::snapshot::size_bytes_set(metadata.len());
                    }

                    info!("✅ State snapshot saved to {}/state.snapshot in {:.3}s",
                          data_dir.display(), save_duration.as_secs_f64());

                    // Save timestamped backup for archival
                    if let Err(e) = icn_snapshot::save_timestamped_snapshot(&snapshot, &data_dir) {
                        warn!("Failed to save timestamped snapshot backup: {}", e);
                    }

                    // Cleanup old snapshots (keep last 3)
                    match icn_snapshot::cleanup_old_snapshots(&data_dir, 3) {
                        Ok(deleted) if deleted > 0 => {
                            info!("Cleaned up {} old snapshot(s)", deleted);
                        }
                        Ok(_) => {},
                        Err(e) => {
                            warn!("Failed to cleanup old snapshots: {}", e);
                        }
                    }
                }
                Err(e) => {
                    icn_obs::metrics::snapshot::save_errors_inc();
                    warn!("Failed to save state snapshot: {}", e);
                }
            }
        }

        // Network actor will shut down gracefully via the shutdown signal
        // The actor's run loop listens for shutdown_rx and cleans up properly
        if network_handle.is_some() {
            info!("Network actor will shut down via shutdown signal");
        }

        // Gossip and Ledger are wrapped in Arc<RwLock> and will be dropped when
        // all references are released
        if gossip_handle.is_some() {
            info!("Gossip actor will be dropped when all references are released");
        }
        if ledger_handle.is_some() {
            info!("Ledger will be dropped when all references are released");
        }

        info!("Supervisor stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bootstrap_peer_valid() {
        let url = "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
        assert_eq!(addr.to_string(), "203.0.113.50:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_ipv4() {
        let url = "icn://did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH@192.168.1.100:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), "did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH");
        assert_eq!(addr.to_string(), "192.168.1.100:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_prefix() {
        let url = "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must start with 'icn://'"));
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_at() {
        let url = "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK_203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected icn://DID@IP:PORT"));
    }

    #[test]
    fn test_parse_bootstrap_peer_invalid_port() {
        let url = "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:invalid";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse socket address"));
    }
}
