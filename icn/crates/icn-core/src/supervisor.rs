//! Supervisor for managing actors

use anyhow::{bail, Context, Result};
use icn_gossip::GossipActor;
use icn_identity::{Did, IdentityBundle, RecoveryMessage, IDENTITY_RECOVERY_TOPIC};
use icn_ledger::Ledger;
use icn_rpc::RpcServer;
use icn_store::SledStore;
use icn_time::ClockSync;
use serde_json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

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
    let url = url
        .strip_prefix("icn://")
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
    let addr: SocketAddr = addr_str
        .parse()
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
    pub fn new(
        config: Config,
        identity_bundle: Option<IdentityBundle>,
        shutdown_tx: ShutdownTx,
    ) -> Self {
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

        // Track spawned background tasks for graceful shutdown
        let mut background_tasks: JoinSet<()> = JoinSet::new();

        // Event broadcaster for WebSocket delivery (shared between compute actor and gateway)
        let event_broadcaster: Option<Arc<icn_gateway::EventBroadcaster>>;

        // Governance event subscription handles - MUST persist for daemon lifetime
        // Stored at function scope to prevent premature Drop (which would unsubscribe)
        let governance_event_subscription: Option<crate::events::SubscriptionHandle>;
        let policy_governance_subscription: Option<crate::events::SubscriptionHandle>;

        // Spawn actors (requires identity bundle from unlocked keystore)
        let (network_handle, gossip_handle, ledger_handle) = if let Some(identity_bundle) =
            &self.identity_bundle
        {
            info!("Identity bundle available - spawning actors");

            let did = identity_bundle.did().clone();

            // Create NodeProfile with hardware sensing (Phase 17)
            let node_profile = crate::node::create_node_profile(
                did.clone(),
                did.clone(), // For now, operator is same as node DID
                &self.config.topology,
            );
            let node_profile_handle = Arc::new(tokio::sync::RwLock::new(node_profile.clone()));

            info!(
                "Node profile created: {} roles detected ({:?}), stage={:?}, capacity={}MB RAM / {}MB storage",
                node_profile.roles.len(),
                node_profile.roles_sorted(),
                node_profile.stage,
                node_profile.capacity.memory_mb_available,
                node_profile.capacity.storage_mb_available,
            );

            // Create trust graph
            let trust_store_path = self.config.store_path().join("trust");
            let trust_store: Arc<dyn icn_store::Store> =
                Arc::new(SledStore::open(&trust_store_path)?);
            let trust_graph = icn_trust::TrustGraph::new(trust_store, did.clone());
            let trust_graph_handle = Arc::new(tokio::sync::RwLock::new(trust_graph));

            info!("Trust graph initialized at {}", trust_store_path.display());

            // Create recovery store for social recovery events
            let recovery_store_path = self.config.store_path().join("recovery");
            let recovery_store: Arc<dyn icn_store::Store> =
                Arc::new(SledStore::open(&recovery_store_path)?);
            info!(
                "Recovery store initialized at {}",
                recovery_store_path.display()
            );

            // Create shared MisbehaviorDetector for Byzantine fault detection (Phase 18)
            // This is shared between NetworkActor and GossipActor to ensure unified tracking
            let mut detector = icn_security::MisbehaviorDetector::new(
                icn_security::MisbehaviorThresholds::default(),
            );

            // Set up trust penalty callback to update trust graph (Phase 18)
            let trust_graph_for_penalty = trust_graph_handle.clone();
            let own_did_for_penalty = did.clone();
            let trust_penalty_callback: icn_security::TrustPenaltyCallback =
                Arc::new(move |peer_did: &icn_identity::Did, reputation_score: f64| {
                    let graph = trust_graph_for_penalty.clone();
                    let peer = peer_did.clone();
                    let own = own_did_for_penalty.clone();

                    // Spawn async task to update trust graph
                    tokio::spawn(async move {
                        // Map reputation (0.0-1.0) to trust score (0.0-1.0)
                        // Reputation below 0.5 becomes untrusted (<0.1)
                        let trust_score = if reputation_score < 0.5 {
                            reputation_score * 0.2 // 0.5 → 0.1, 0.0 → 0.0
                        } else {
                            reputation_score // Keep 0.5-1.0 range
                        };

                        let mut graph = graph.write().await;
                        let edge =
                            icn_trust::TrustEdge::new(own.clone(), peer.clone(), trust_score);

                        if let Err(e) = graph.add_edge(edge) {
                            warn!(
                                "Failed to update trust graph for {} (reputation: {:.2}): {}",
                                peer, reputation_score, e
                            );
                        } else {
                            debug!(
                                "Updated trust for {} to {:.2} (reputation: {:.2})",
                                peer, trust_score, reputation_score
                            );
                        }
                    });
                });

            detector.set_trust_penalty_callback(trust_penalty_callback);

            let misbehavior_detector = Arc::new(tokio::sync::RwLock::new(detector));
            info!("Shared Byzantine fault detector created with trust graph integration");

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

                    info!(
                        "Found state snapshot (version {}, created at {}) - loaded in {:.3}s",
                        snapshot.version,
                        snapshot.created_at,
                        load_duration.as_secs_f64()
                    );

                    // Warn if checksum file is missing (legacy snapshot)
                    let checksum_path = data_dir.join("state.snapshot.sha256");
                    if !checksum_path.exists() {
                        warn!("⚠️  Snapshot loaded without checksum verification (legacy snapshot). Run 'icnctl snapshot create' to generate checksum.");
                    } else {
                        info!("✅ Snapshot checksum verified");
                    }

                    // Record snapshot contents metrics
                    if let Some(ref gossip_state) = snapshot.gossip_state {
                        icn_obs::metrics::snapshot::gossip_vector_clock_entries_set(
                            gossip_state.vector_clock.len(),
                        );
                        icn_obs::metrics::snapshot::gossip_subscriptions_set(
                            gossip_state.subscriptions.len(),
                        );
                        icn_obs::metrics::snapshot::gossip_topics_set(gossip_state.topics.len());
                    }
                    if let Some(ref network_state) = snapshot.network_state {
                        icn_obs::metrics::snapshot::network_x25519_keys_set(
                            network_state.peer_x25519_keys.len(),
                        );
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

            // Set up gossip store for replica metadata tracking (Phase 17)
            let gossip_store_path = self.config.store_path().join("gossip");
            let gossip_store: Arc<dyn icn_store::Store> =
                Arc::new(SledStore::open(&gossip_store_path)?);
            {
                let mut gossip = gossip_handle.write().await;
                gossip.set_store(gossip_store.clone());
            }
            info!(
                "Gossip store initialized at {} for replica tracking",
                gossip_store_path.display()
            );

            // Spawn ReplicationManager for monitoring and maintaining data durability (Phase 17 Week 3)
            let replication_config = crate::replication::ReplicationConfig::default();
            let _replication_handle = crate::replication::ReplicationManager::spawn(
                did.clone(),
                replication_config.clone(),
                gossip_store.clone(),
                trust_graph_handle.clone(),
                gossip_handle.clone(),
            );

            info!(
                "ReplicationManager spawned (target: {} replicas, health check: {}s)",
                replication_config.target_replicas, replication_config.health_check_interval_secs
            );

            // Phase 18 Week 3: Initialize PartitionDetector and PartitionHealer
            let partition_config = icn_gossip::PartitionConfig::default(); // 5 min threshold, 30s check interval
            let partition_detector = icn_gossip::PartitionDetector::new(partition_config.clone());
            let partition_detector_handle = Arc::new(tokio::sync::RwLock::new(partition_detector));

            // Create partition healer with new vector clock (will be merged during healing)
            let partition_healer = icn_gossip::PartitionHealer::new(icn_gossip::VectorClock::new());
            let partition_healer_handle = Arc::new(tokio::sync::RwLock::new(partition_healer));

            // Set partition detector and healer on gossip actor
            {
                let mut gossip = gossip_handle.write().await;
                gossip.set_partition_detector(partition_detector_handle.clone());
                gossip.set_partition_healer(partition_healer_handle.clone());
            }

            info!(
                "Partition detector and healer initialized (threshold: {:?})",
                partition_config.partition_threshold
            );

            // Set shared misbehavior detector on gossip actor for unified Byzantine fault tracking
            {
                let mut gossip = gossip_handle.write().await;
                gossip.set_misbehavior_detector(misbehavior_detector.clone());
            }
            info!("Gossip actor configured with shared Byzantine fault detector");

            // Phase 18 Week 6: Initialize StorageQuotaManager for gossip entries
            // Default: 1GB global limit, 90% eviction threshold, 10MB per-DID quota
            let storage_quota_manager = icn_store::StorageQuotaManager::new(
                1024 * 1024 * 1024, // 1GB global limit
                0.9,                // Start evicting at 90% capacity
            );
            let storage_quota_handle = Arc::new(tokio::sync::RwLock::new(storage_quota_manager));

            // Set storage quota manager on gossip actor
            {
                let mut gossip = gossip_handle.write().await;
                gossip.set_storage_quota_manager(storage_quota_handle.clone());
            }

            info!("Storage quota manager initialized (global limit: 1GB, eviction threshold: 90%)");

            // Spawn Ledger
            let store_path = self.config.store_path().join("ledger");
            let store = Arc::new(SledStore::open(&store_path)?);
            let mut ledger = Ledger::new(store)?;
            ledger.set_gossip(gossip_handle.clone());
            ledger.set_misbehavior_detector(misbehavior_detector.clone());
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

            // Spawn Identity actor (provides signing and trust graph access)
            let identity_handle = crate::identity::IdentityActor::spawn(
                identity_bundle.keypair().clone(),
                trust_graph_handle.clone(),
                self.shutdown_tx.clone(),
            );

            info!("Identity actor spawned with DID: {}", identity_handle.did());

            // Spawn Network actor with gossip bridge
            let listen_addr: std::net::SocketAddr = self.config.network.listen_addr.parse()?;

            // Create incoming message handler that routes to gossip
            let gossip_handle_clone = gossip_handle.clone();
            let network_handle_for_handler =
                Arc::new(tokio::sync::RwLock::new(None::<icn_net::NetworkHandle>));
            let network_handle_for_handler_clone = network_handle_for_handler.clone();
            let own_did_for_handler = did.clone();
            let federation_enabled = self.config.federation.enabled;

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
                        info!(
                            "Received Subscribe from {} for topics: {:?}",
                            sender_did, topics
                        );
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
                                        warn!(
                                            "Failed to subscribe {} to topic {}: {}",
                                            sender_did, topic, e
                                        );
                                    }
                                }
                            }

                            // Send SubscribeAck back if we have any successful subscriptions
                            if !acked_topics.is_empty() {
                                icn_obs::metrics::gossip::subscribe_acks_sent_inc();
                                if let Some(net_handle) = network_handle_lock.read().await.as_ref()
                                {
                                    let ack_msg = icn_net::NetworkMessage::subscribe_ack(
                                        own_did,
                                        sender_did.clone(),
                                        acked_topics,
                                    );
                                    if let Err(e) =
                                        net_handle.send_message(sender_did, ack_msg).await
                                    {
                                        warn!("Failed to send SubscribeAck: {}", e);
                                    }
                                }
                            }
                        });
                    }

                    icn_net::MessagePayload::Unsubscribe { topics } => {
                        info!(
                            "Received Unsubscribe from {} for topics: {:?}",
                            sender_did, topics
                        );
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
                                        warn!(
                                            "Failed to unsubscribe {} from topic {}: {}",
                                            sender_did, topic, e
                                        );
                                    }
                                }
                            }
                        });
                    }

                    icn_net::MessagePayload::SubscribeAck { topics } => {
                        info!(
                            "Received SubscribeAck from {} for topics: {:?}",
                            sender_did, topics
                        );
                        // Track successful subscription acknowledgment
                        // In a full implementation, this could update a local subscription registry
                    }

                    icn_net::MessagePayload::Ping { .. } => {
                        // Ping/Pong handled by network actor
                    }

                    icn_net::MessagePayload::Pong { .. } => {
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
                        debug!(
                            "Received verified signed message from {} (seq={}, type={:?})",
                            envelope.from, envelope.sequence, envelope.payload_type
                        );

                        // Route based on payload type
                        match envelope.payload_type {
                            icn_net::PayloadType::Gossip => {
                                // Decode gossip message from signed envelope
                                let gossip_msg: icn_gossip::GossipMessage =
                                    match envelope.decode_payload() {
                                        Ok(msg) => msg,
                                        Err(e) => {
                                            warn!(
                                                "Failed to decode gossip payload from {}: {}",
                                                envelope.from, e
                                            );
                                            return;
                                        }
                                    };

                                // Handle gossip message (sender is authenticated via signature)
                                let gossip_handle = gossip_handle_clone.clone();
                                let sender = envelope.from.clone();
                                tokio::spawn(async move {
                                    let mut gossip = gossip_handle.write().await;
                                    if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                                        warn!(
                                            "Failed to handle gossip message from {}: {}",
                                            sender, e
                                        );
                                    }
                                });
                            }

                            _ => {
                                // Other payload types not yet implemented
                                debug!(
                                    "Received signed message with unhandled payload type: {:?}",
                                    envelope.payload_type
                                );
                            }
                        }
                    }

                    icn_net::MessagePayload::PeerExchange(ref peer_msg) => {
                        // Peer exchange messages for cross-network federation discovery
                        // Request messages are handled directly by NetworkActor
                        // Response/Announce/Unannounce are forwarded here for processing
                        use icn_net::PeerExchangeMessage;
                        let network_handle_lock = network_handle_for_handler_clone.clone();

                        match peer_msg {
                            PeerExchangeMessage::Request { .. } => {
                                // Requests are handled by NetworkActor directly
                                debug!("Peer exchange request forwarded to handler (unexpected)");
                            }
                            PeerExchangeMessage::Response { peers, total_known } => {
                                // Received peer list from another node
                                info!(
                                    "Received {} federation peers from {} (total: {})",
                                    peers.len(),
                                    sender_did,
                                    total_known
                                );

                                if federation_enabled {
                                    // Auto-dial discovered peers
                                    let peers_to_dial: Vec<_> = peers
                                        .iter()
                                        .filter_map(|p| {
                                            // Parse first address
                                            p.addresses
                                                .first()
                                                .and_then(|addr_str| {
                                                    addr_str.parse::<std::net::SocketAddr>().ok()
                                                })
                                                .map(|addr| (p.did.clone(), addr))
                                        })
                                        .collect();

                                    tokio::spawn(async move {
                                        if let Some(net_handle) =
                                            network_handle_lock.read().await.as_ref()
                                        {
                                            for (did_str, addr) in peers_to_dial {
                                                // Skip if DID is invalid
                                                let peer_did =
                                                    match icn_identity::Did::from_str(&did_str) {
                                                        Ok(d) => d,
                                                        Err(e) => {
                                                            debug!(
                                                                "Skipping invalid DID {}: {}",
                                                                did_str, e
                                                            );
                                                            continue;
                                                        }
                                                    };

                                                info!(
                                                    "Auto-dialing discovered peer {} at {}",
                                                    peer_did, addr
                                                );
                                                match net_handle.dial(addr, peer_did.clone()).await
                                                {
                                                    Ok(_) => {
                                                        icn_obs::metrics::peer_exchange::peers_dialed_inc();
                                                    }
                                                    Err(e) => {
                                                        debug!(
                                                            "Failed to dial {}: {}",
                                                            peer_did, e
                                                        );
                                                        icn_obs::metrics::peer_exchange::dial_failures_inc();
                                                    }
                                                }
                                            }
                                        }
                                    });
                                } else {
                                    // Just log discovered peers
                                    for peer in peers {
                                        debug!(
                                            "Discovered peer (federation disabled): {} at {:?}",
                                            peer.did, peer.addresses
                                        );
                                    }
                                }
                            }
                            PeerExchangeMessage::Announce { peer } => {
                                info!(
                                    "Peer announced by {}: {} at {:?}",
                                    sender_did, peer.did, peer.addresses
                                );

                                if federation_enabled {
                                    // Auto-dial announced peer
                                    if let Some(addr_str) = peer.addresses.first() {
                                        if let Ok(addr) = addr_str.parse::<std::net::SocketAddr>() {
                                            let peer_did_str = peer.did.clone();
                                            tokio::spawn(async move {
                                                if let Some(net_handle) =
                                                    network_handle_lock.read().await.as_ref()
                                                {
                                                    if let Ok(peer_did) =
                                                        icn_identity::Did::from_str(&peer_did_str)
                                                    {
                                                        info!(
                                                            "Auto-dialing announced peer {} at {}",
                                                            peer_did, addr
                                                        );
                                                        match net_handle
                                                            .dial(addr, peer_did.clone())
                                                            .await
                                                        {
                                                            Ok(_) => {
                                                                icn_obs::metrics::peer_exchange::peers_dialed_inc();
                                                            }
                                                            Err(e) => {
                                                                debug!("Failed to dial announced peer {}: {}", peer_did, e);
                                                                icn_obs::metrics::peer_exchange::dial_failures_inc();
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                            PeerExchangeMessage::Unannounce { did } => {
                                info!("Peer unannounced by {}: {}", sender_did, did);
                                // Peer disconnected - no action needed
                                // (Network layer handles connection cleanup)
                            }
                        }
                    }

                    icn_net::MessagePayload::Onion(_) => {
                        // Onion messages are handled directly by NetworkActor
                        // They should not reach this handler as they are processed
                        // at the network layer (peel layer, forward, or extract)
                        debug!(
                            "Received Onion message in supervisor handler - this should not happen"
                        );
                    }
                }
            });

            // Prepare rate limiting configuration
            let (trust_graph_for_rate_limit, trust_gated_config, fallback_config) =
                if self.config.rate_limiting.enabled {
                    (
                        Some(trust_graph_handle.clone()), // Enable trust-gated rate limiting
                        Some(self.config.rate_limiting.to_trust_gated_config()),
                        Some(self.config.rate_limiting.to_fallback_config()),
                    )
                } else {
                    (None, None, None) // Disable trust-gated rate limiting
                };

            // Use identity bundle loaded from keystore (preserves TLS cert across restarts)
            info!(
                "Using identity bundle with DID-TLS binding: {}",
                identity_bundle.did()
            );

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
                Some(misbehavior_detector.clone()), // Shared Byzantine fault detector
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

            // Create compute handle holder (will be filled after ComputeActor is spawned)
            let compute_handle_holder: Arc<
                tokio::sync::RwLock<Option<icn_compute::ComputeHandle>>,
            > = Arc::new(tokio::sync::RwLock::new(None));

            // Set send callback on gossip actor to enable request/response
            {
                let mut gossip = gossip_handle.write().await;
                let network_handle_clone = network_handle.clone();
                let own_did_clone = did.clone();
                let keypair_clone = identity_bundle.keypair().clone();
                let sequence_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

                let send_callback: icn_gossip::SendMessageCallback =
                    Arc::new(move |recipient, gossip_msg| {
                        let net_handle = network_handle_clone.clone();
                        let from_did = own_did_clone.clone();
                        let keypair = keypair_clone.clone();
                        let sequence_ctr = sequence_counter.clone();

                        // Track metrics based on message type
                        use icn_gossip::GossipMessage;
                        match &gossip_msg {
                            GossipMessage::Announce { .. } => {
                                icn_obs::metrics::gossip::announces_sent_inc()
                            }
                            GossipMessage::Request { .. } => {
                                icn_obs::metrics::gossip::requests_sent_inc()
                            }
                            GossipMessage::Response { .. } => {
                                icn_obs::metrics::gossip::responses_sent_inc()
                            }
                            _ => {} // Other message types
                        }

                        // Spawn async task to send message
                        tokio::spawn(async move {
                            // Get next sequence number
                            let sequence =
                                sequence_ctr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

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
                                let net_msg = icn_net::NetworkMessage::signed(
                                    Some(target_did.clone()),
                                    envelope,
                                );
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
                let candidate_cache_for_cleanup = candidate_cache.clone();
                let network_handle_for_candidates = network_handle.clone();

                let compute_handle_for_notifications = compute_handle_holder.clone();

                // Create profile cache for peer capability discovery
                let profile_cache: Arc<
                    tokio::sync::RwLock<std::collections::HashMap<Did, crate::node::NodeProfile>>,
                > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
                let profile_cache_for_notifications = profile_cache.clone();

                // Create FederationGossipHandler if federation is enabled
                let federation_handler_for_notifications: Option<
                    Arc<icn_federation::FederationGossipHandler>,
                > = if federation_enabled {
                    // Derive coop_id and coop_name from config or defaults
                    let coop_id = if self.config.federation.coop_id.is_empty() {
                        // Use network_name as default coop_id
                        self.config.federation.network_name.clone()
                    } else {
                        self.config.federation.coop_id.clone()
                    };

                    let coop_name = if self.config.federation.coop_name.is_empty() {
                        // Use network_name as default coop_name
                        self.config.federation.network_name.clone()
                    } else {
                        self.config.federation.coop_name.clone()
                    };

                    // Create own cooperative info
                    let own_coop_info = icn_federation::CooperativeInfo::new(
                        coop_id.clone(),
                        coop_name.clone(),
                        did.clone(),
                        icn_federation::FederationPolicy::default(), // Open by default
                    );

                    // Create federation store
                    let federation_store_path = self.config.store_path().join("federation");
                    let federation_store: Arc<dyn icn_store::Store> =
                        Arc::new(SledStore::open(&federation_store_path)?);

                    // Create cooperative registry
                    let federation_registry = Arc::new(
                        icn_federation::CooperativeRegistry::new(
                            federation_store,
                            own_coop_info.clone(),
                        )
                        .map_err(|e| {
                            anyhow::anyhow!("Failed to create federation registry: {e}")
                        })?,
                    );

                    // Create federation gossip handler
                    let federation_handler = Arc::new(
                        icn_federation::FederationGossipHandler::new(federation_registry),
                    );

                    // Set own coop info on handler
                    federation_handler.set_own_coop(own_coop_info);

                    // Set up send callback for federation messages (using gossip)
                    let gossip_for_federation = gossip_handle.clone();
                    let federation_send_callback: icn_federation::gossip::GossipSendCallback =
                        Arc::new(move |topic: &str, data: Vec<u8>| {
                            let gossip = gossip_for_federation.clone();
                            let topic_owned = topic.to_string();
                            // Use a sync approach since we're in a sync callback
                            tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    let mut gossip = gossip.write().await;
                                    gossip
                                        .publish(&topic_owned, data)
                                        .map(|_| ()) // Discard the hash, just return ()
                                        .map_err(|e| {
                                            icn_federation::FederationError::GossipPublishFailed(
                                                e.to_string(),
                                            )
                                        })
                                })
                            })
                        });
                    federation_handler.set_send_callback(federation_send_callback);

                    info!(
                        "✓ Federation enabled: coop_id={}, coop_name={}, store={}",
                        coop_id,
                        coop_name,
                        federation_store_path.display()
                    );

                    Some(federation_handler)
                } else {
                    None
                };

                // Clone federation handler for announcement task (before it's moved into notification callback)
                let federation_handler_for_announce = federation_handler_for_notifications.clone();

                let notification_callback: icn_gossip::EntryNotificationCallback = Arc::new(
                    move |topic, entry, _subscriber_did| {
                        // Handle trust attestations
                        if topic == crate::trust_propagation::TRUST_ATTESTATIONS_TOPIC {
                            let trust_graph = trust_graph_for_notifications.clone();
                            let own_did = own_did_for_notifications.clone();

                            tokio::spawn(async move {
                                if let Err(e) =
                                    crate::trust_propagation::handle_trust_attestation_entry(
                                        &entry,
                                        &trust_graph,
                                        &own_did,
                                        None, // TODO: Add rate limiter in Phase 8A+
                                    )
                                    .await
                                {
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
                                match serde_json::from_slice::<icn_ccl::ContractDeploymentMessage>(
                                    &entry_data,
                                ) {
                                    Ok(deployment_msg) => {
                                        let deployer =
                                            deployment_msg.installation.installed_by.to_string();
                                        let actor = contract_actor.write().await;
                                        if let Err(e) =
                                            actor.handle_deployment_message(deployment_msg).await
                                        {
                                            let error_str = e.to_string();
                                            if error_str.contains("signature") {
                                                warn!("Contract deployment signature verification failed from {}: {}", deployer, e);
                                                icn_obs::metrics::contract::deployments_rejected_signature_inc(&deployer);
                                            } else {
                                                warn!(
                                                    "Failed to handle contract deployment: {}",
                                                    e
                                                );
                                                icn_obs::metrics::contract::deployments_rejected_inc("handling_error");
                                            }
                                        } else {
                                            info!("Contract deployment processed successfully");
                                            icn_obs::metrics::contract::deployments_received_inc();
                                        }
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Failed to deserialize contract deployment message: {}",
                                            e
                                        );
                                        icn_obs::metrics::contract::deployments_rejected_inc(
                                            "deserialization_error",
                                        );
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
                                        info!(
                                            "Received recovery message: {}",
                                            recovery_msg.summary()
                                        );

                                        // Handle different recovery message types
                                        // Returns Ok(true) if recovery was finalized, Ok(false) otherwise, Err on failure
                                        let result: Result<bool> = (|| {
                                            match &recovery_msg {
                                                RecoveryMessage::Initiated {
                                                    id,
                                                    old_did,
                                                    new_did,
                                                    threshold,
                                                    delay_period,
                                                    timestamp: _,
                                                } => {
                                                    // Create new recovery event
                                                    let recovery = RecoveryEvent::new(
                                                        old_did.clone(),
                                                        new_did.clone(),
                                                        *threshold,
                                                        *delay_period,
                                                    );

                                                    // Store recovery event
                                                    let key = format!("recovery:{id}");
                                                    let value = serde_json::to_vec(&recovery)
                                                        .map_err(|e| {
                                                            anyhow::anyhow!(
                                                                "Serialization error: {e}"
                                                            )
                                                        })?;
                                                    recovery_store.put(key.as_bytes(), &value)?;

                                                    info!(
                                                        "Stored new recovery: {} ({} → {})",
                                                        id, old_did, new_did
                                                    );
                                                    Ok(false)
                                                }
                                                RecoveryMessage::Attestation {
                                                    recovery_id,
                                                    attestation,
                                                    ..
                                                } => {
                                                    // Load existing recovery
                                                    let key = format!("recovery:{recovery_id}");
                                                    match recovery_store.get(key.as_bytes())? {
                                                        Some(data) => {
                                                            let mut recovery: RecoveryEvent = serde_json::from_slice(&data)
                                                        .map_err(|e| anyhow::anyhow!("Deserialization error: {e}"))?;

                                                            // Add attestation
                                                            match recovery.add_attestation(
                                                                attestation.clone(),
                                                            ) {
                                                                Ok(threshold_reached) => {
                                                                    // Save updated recovery
                                                                    let value = serde_json::to_vec(&recovery)
                                                                .map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                                                                    recovery_store.put(
                                                                        key.as_bytes(),
                                                                        &value,
                                                                    )?;

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
                                                RecoveryMessage::Finalized {
                                                    id,
                                                    old_did,
                                                    new_did,
                                                    ..
                                                } => {
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
                                                                    recovery_store.put(
                                                                        key.as_bytes(),
                                                                        &value,
                                                                    )?;

                                                                    info!("✅ Recovery finalized: {} → {}", old_did, new_did);
                                                                    Ok(true) // Successfully finalized - trigger trust/ledger updates
                                                                }
                                                                Err(e) => {
                                                                    warn!("Failed to finalize recovery {}: {}", id, e);
                                                                    Err(e)
                                                                }
                                                            }
                                                        }
                                                        None => {
                                                            warn!("Received finalization for unknown recovery: {}", id);
                                                            Ok(false) // Unknown recovery - don't trigger trust/ledger updates
                                                        }
                                                    }
                                                }
                                                RecoveryMessage::Cancelled {
                                                    id,
                                                    cancelled_by,
                                                    reason,
                                                    ..
                                                } => {
                                                    // Load recovery and mark as cancelled
                                                    let key = format!("recovery:{id}");
                                                    match recovery_store.get(key.as_bytes())? {
                                                        Some(data) => {
                                                            let mut recovery: RecoveryEvent = serde_json::from_slice(&data)
                                                        .map_err(|e| anyhow::anyhow!("Deserialization error: {e}"))?;

                                                            match recovery.cancel(
                                                                cancelled_by.clone(),
                                                                reason.clone(),
                                                            ) {
                                                                Ok(_) => {
                                                                    let value = serde_json::to_vec(&recovery)
                                                                .map_err(|e| anyhow::anyhow!("Serialization error: {e}"))?;
                                                                    recovery_store.put(
                                                                        key.as_bytes(),
                                                                        &value,
                                                                    )?;

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
                                        })(
                                        );

                                        match result {
                                            Err(ref e) => {
                                                warn!("Failed to handle recovery message: {}", e);
                                            }
                                            Ok(true) => {
                                                // Recovery was successfully finalized - update trust graph and ledger
                                                if let RecoveryMessage::Finalized {
                                                    id,
                                                    old_did,
                                                    new_did,
                                                    ..
                                                } = &recovery_msg
                                                {
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
                                                    match ledger_guard
                                                        .transfer_balances_for_recovery(
                                                            old_did, new_did, id,
                                                        ) {
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
                            match serde_json::from_slice::<icn_net::ConnectionCandidate>(
                                &entry_data,
                            ) {
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
                                                    debug!(
                                                        "Already connected to {}, skipping dial",
                                                        did
                                                    );
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
                                        debug!(
                                            "Attempting connection to {} via local address {}",
                                            did, candidate.local_addr
                                        );
                                        match net_handle
                                            .dial(candidate.local_addr, did.clone())
                                            .await
                                        {
                                            Ok(_) => {
                                                info!(
                                                    "✅ Connected to {} via local address {}",
                                                    did, candidate.local_addr
                                                );
                                                connected = true;
                                            }
                                            Err(e) => {
                                                debug!(
                                                    "Failed to connect via local address: {}",
                                                    e
                                                );
                                            }
                                        }

                                        // If local connection failed, try public address (NAT hole punching)
                                        if !connected {
                                            if let Some(public_addr) = candidate.public_addr {
                                                debug!("Attempting connection to {} via public address {}", did, public_addr);
                                                match net_handle
                                                    .dial(public_addr, did.clone())
                                                    .await
                                                {
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
                                            debug!(
                                                "Could not establish direct connection to {}",
                                                did
                                            );
                                        }
                                    });
                                }
                                Err(e) => {
                                    warn!("Failed to deserialize connection candidate: {}", e);
                                }
                            }
                        }
                        // Handle compute topics
                        else if topic == icn_compute::TOPIC_SUBMIT
                            || topic == icn_compute::TOPIC_CLAIM
                            || topic == icn_compute::TOPIC_RESULT
                        {
                            let entry_data = match entry.get_data() {
                                Ok(data) => data,
                                Err(e) => {
                                    warn!("Failed to get compute entry data: {}", e);
                                    return;
                                }
                            };

                            match bincode::deserialize::<icn_compute::ComputeMessage>(&entry_data) {
                                Ok(compute_msg) => {
                                    let compute_holder = compute_handle_for_notifications.clone();
                                    tokio::spawn(async move {
                                        if let Some(handle) = compute_holder.read().await.as_ref() {
                                            if let Err(e) = handle.handle_gossip(compute_msg).await
                                            {
                                                warn!("Failed to handle compute message: {}", e);
                                            }
                                        }
                                    });
                                }
                                Err(e) => {
                                    warn!("Failed to deserialize compute message: {}", e);
                                }
                            }
                        }
                        // Handle node profile announcements
                        else if topic == crate::node::TOPIC_NODE_PROFILES {
                            let entry_data = match entry.get_data() {
                                Ok(data) => data,
                                Err(e) => {
                                    warn!("Failed to get profile entry data: {}", e);
                                    return;
                                }
                            };

                            let profile_cache = profile_cache_for_notifications.clone();
                            tokio::spawn(async move {
                                match serde_json::from_slice::<crate::node::ProfileMessage>(
                                    &entry_data,
                                ) {
                                    Ok(crate::node::ProfileMessage::Announce(profile)) => {
                                        let peer_did = profile.node_did.clone();
                                        let roles_count = profile.roles.len();
                                        let extended_count = profile.extended.capabilities.len();

                                        // Update profile cache
                                        let mut cache = profile_cache.write().await;
                                        cache.insert(peer_did.clone(), profile);

                                        debug!(
                                            "Cached profile for peer {}: {} roles, {} extended capabilities",
                                            peer_did, roles_count, extended_count
                                        );
                                    }
                                    Ok(crate::node::ProfileMessage::Query(_)) => {
                                        // TODO: Respond to profile queries
                                        debug!("Received profile query (response not implemented)");
                                    }
                                    Ok(crate::node::ProfileMessage::Response(_)) => {
                                        // Profile responses are handled by the requester
                                    }
                                    Err(e) => {
                                        warn!("Failed to deserialize profile message: {}", e);
                                    }
                                }
                            });
                        }
                        // Handle federation topics
                        else if topic == icn_federation::TOPIC_FEDERATION_REGISTRY
                            || topic == icn_federation::TOPIC_FEDERATION_TRUST
                            || topic == icn_federation::TOPIC_FEDERATION_CLEARING
                        {
                            if let Some(ref handler) = federation_handler_for_notifications {
                                let entry_data = match entry.get_data() {
                                    Ok(data) => data,
                                    Err(e) => {
                                        warn!("Failed to get federation entry data: {}", e);
                                        return;
                                    }
                                };

                                let handler_clone = handler.clone();
                                let topic_owned = topic.to_string();
                                tokio::spawn(async move {
                                    if let Err(e) =
                                        handler_clone.handle_message(&topic_owned, &entry_data)
                                    {
                                        warn!(
                                            "Failed to handle federation message on {}: {}",
                                            topic_owned, e
                                        );
                                    }
                                });
                            }
                        }
                    },
                );

                gossip.set_notification_callback(notification_callback);

                // Set up peer sampling callback for scope-aware gossip fanout
                let network_handle_for_sampling = network_handle.clone();
                let peer_sampling_callback: icn_gossip::PeerSamplingCallback =
                    Arc::new(move |scope, count| {
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
                if let Err(e) = gossip.subscribe(
                    crate::trust_propagation::TRUST_ATTESTATIONS_TOPIC,
                    did.clone(),
                ) {
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

                // Subscribe to federation topics if federation is enabled
                if federation_enabled {
                    if let Err(e) =
                        gossip.subscribe(icn_federation::TOPIC_FEDERATION_REGISTRY, did.clone())
                    {
                        warn!("Failed to subscribe to federation:registry topic: {}", e);
                    } else {
                        info!("Subscribed to federation:registry topic");
                    }

                    if let Err(e) =
                        gossip.subscribe(icn_federation::TOPIC_FEDERATION_TRUST, did.clone())
                    {
                        warn!("Failed to subscribe to federation:trust topic: {}", e);
                    } else {
                        info!("Subscribed to federation:trust topic");
                    }

                    if let Err(e) =
                        gossip.subscribe(icn_federation::TOPIC_FEDERATION_CLEARING, did.clone())
                    {
                        warn!("Failed to subscribe to federation:clearing topic: {}", e);
                    } else {
                        info!("Subscribed to federation:clearing topic");
                    }

                    // Spawn periodic federation announcement task (every 5 minutes)
                    if let Some(ref handler) = federation_handler_for_announce {
                        let handler_clone = handler.clone();
                        let mut federation_shutdown = self.shutdown_tx.subscribe();

                        // Announce immediately on startup
                        if let Err(e) = handler_clone.announce() {
                            warn!("Failed to send initial federation announcement: {}", e);
                        } else {
                            info!("✓ Sent initial federation announcement");
                        }

                        // Spawn periodic announcement task
                        let handler_for_task = handler_clone.clone();
                        background_tasks.spawn(async move {
                            // Use icn_federation::defaults::ANNOUNCEMENT_INTERVAL (5 minutes)
                            let mut interval = tokio::time::interval(icn_federation::defaults::ANNOUNCEMENT_INTERVAL);
                            interval.tick().await; // Skip first tick (already announced above)

                            loop {
                                tokio::select! {
                                    _ = interval.tick() => {
                                        if let Err(e) = handler_for_task.announce() {
                                            warn!("Failed to send periodic federation announcement: {}", e);
                                        } else {
                                            debug!("Sent periodic federation announcement");
                                        }
                                    }
                                    _ = federation_shutdown.recv() => {
                                        info!("Federation announcement task shutting down");
                                        break;
                                    }
                                }
                            }
                        });

                        info!(
                            "Federation announcement task spawned (interval: {:?})",
                            icn_federation::defaults::ANNOUNCEMENT_INTERVAL
                        );
                    }
                }

                // Spawn candidate cache cleanup task (every 5 minutes)
                let mut cache_cleanup_shutdown = self.shutdown_tx.subscribe();
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // 5 minutes
                    loop {
                        tokio::select! {
                            _ = interval.tick() => {
                                let removed = candidate_cache_for_cleanup.cleanup_expired().await;
                                if removed > 0 {
                                    info!("Candidate cache cleanup removed {} stale entries", removed);
                                }
                                // Update metrics
                                let cache_size = candidate_cache_for_cleanup.len().await;
                                icn_obs::metrics::nat_traversal::candidates_cached_set(cache_size);
                            }
                            _ = cache_cleanup_shutdown.recv() => {
                                info!("Candidate cache cleanup task shutting down");
                                break;
                            }
                        }
                    }
                });

                info!("Candidate cache cleanup task spawned");
            }

            info!("Gossip send callback configured");

            // Dial bootstrap peers for WAN connectivity
            if !self.config.network.bootstrap_peers.is_empty() {
                info!(
                    "Dialing {} bootstrap peers",
                    self.config.network.bootstrap_peers.len()
                );
                let mut connected_bootstrap_peers = Vec::new();

                for peer_url in &self.config.network.bootstrap_peers {
                    match parse_bootstrap_peer(peer_url) {
                        Ok((peer_did, peer_addr)) => {
                            info!(
                                "Connecting to bootstrap peer: {} at {}",
                                peer_did, peer_addr
                            );
                            match network_handle.dial(peer_addr, peer_did.clone()).await {
                                Ok(_) => {
                                    info!("✓ Connected to bootstrap peer: {}", peer_did);
                                    connected_bootstrap_peers.push(peer_did);
                                }
                                Err(e) => {
                                    warn!("Failed to connect to bootstrap peer {}: {}", peer_did, e)
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse bootstrap peer URL '{}': {}", peer_url, e);
                        }
                    }
                }

                // Request peer exchange from bootstrap peers if federation is enabled
                if self.config.federation.enabled && !connected_bootstrap_peers.is_empty() {
                    info!(
                        "Federation enabled - requesting peer exchange from {} bootstrap peers",
                        connected_bootstrap_peers.len()
                    );

                    let network_filter = if self.config.federation.network_name != "icn-mainnet" {
                        Some(self.config.federation.network_name.clone())
                    } else {
                        None
                    };

                    for peer_did in connected_bootstrap_peers {
                        // Small delay to allow Hello handshake to complete
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        match network_handle
                            .request_peer_exchange(&peer_did, Some(50), network_filter.clone())
                            .await
                        {
                            Ok(_) => info!("✓ Requested peer exchange from {}", peer_did),
                            Err(e) => {
                                debug!("Failed to request peer exchange from {}: {}", peer_did, e)
                            }
                        }
                    }
                }
            }

            // Announce connection candidate for NAT traversal
            {
                info!("Announcing connection candidate for NAT traversal...");
                match network_handle.connection_candidate().await {
                    Ok(candidate) => {
                        info!(
                            "Connection candidate: local={}, public={:?}, relay={:?}",
                            candidate.local_addr, candidate.public_addr, candidate.relay_addr
                        );

                        // Serialize candidate and publish to gossip
                        match serde_json::to_vec(&candidate) {
                            Ok(candidate_bytes) => {
                                let mut gossip = gossip_handle.write().await;
                                match gossip.publish(NETWORK_CANDIDATES_TOPIC, candidate_bytes) {
                                    Ok(_) => info!("✓ Published connection candidate to gossip"),
                                    Err(e) => {
                                        warn!("Failed to publish connection candidate: {}", e)
                                    }
                                }
                            }
                            Err(e) => warn!("Failed to serialize connection candidate: {}", e),
                        }
                    }
                    Err(e) => warn!("Failed to get connection candidate: {}", e),
                }
            }

            // Subscribe to node profiles topic and announce our profile
            {
                let mut gossip = gossip_handle.write().await;

                // Subscribe to network:profiles topic for peer capability discovery
                if let Err(e) = gossip.subscribe(crate::node::TOPIC_NODE_PROFILES, did.clone()) {
                    warn!("Failed to subscribe to network:profiles topic: {}", e);
                } else {
                    info!("Subscribed to network:profiles topic");
                }

                // Publish our node profile announcement
                let profile_msg = crate::node::ProfileMessage::Announce(node_profile.clone());
                match serde_json::to_vec(&profile_msg) {
                    Ok(profile_bytes) => {
                        match gossip.publish(crate::node::TOPIC_NODE_PROFILES, profile_bytes) {
                            Ok(_) => {
                                info!(
                                    "✓ Published node profile: {} roles ({:?}), {} extended capabilities",
                                    node_profile.roles.len(),
                                    node_profile.roles_sorted(),
                                    node_profile.extended.capabilities.len(),
                                );
                            }
                            Err(e) => warn!("Failed to publish node profile: {}", e),
                        }
                    }
                    Err(e) => warn!("Failed to serialize node profile: {}", e),
                }
            }

            // Create event bus for inter-actor communication
            let event_bus = Arc::new(crate::events::EventBus::new());
            info!("Event bus created");

            // Spawn Governance actor
            let gov_store_path = self.config.store_path().join("governance");
            let gov_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&gov_store_path)?);
            let gov_resolver: Arc<dyn icn_governance::MembershipResolver + Send + Sync> =
                Arc::new(icn_governance::StaticMembershipResolver::new());

            // Create governance topic before spawning GovernanceActor
            // (subscribe does not auto-create topics like publish does)
            {
                let mut gossip = gossip_handle.write().await;
                gossip.create_topic(icn_gossip::Topic::new(
                    "governance:proposal".to_string(),
                    icn_gossip::AccessControl::Public,
                ));
            }

            let governance_handle = crate::governance::GovernanceActor::spawn(
                did.clone(),
                gov_store.clone(),
                gossip_handle.clone(),
                gov_resolver,
                Some(event_bus.clone()),
            )
            .await?;

            info!("✓ Governance actor spawned at {}", gov_store_path.display());

            // Subscribe to governance events for ledger execution
            // CRITICAL: Must store handle to keep subscription alive for daemon lifetime
            governance_event_subscription = Some({
                use crate::events::SystemEvent;
                use icn_governance::ProposalPayload;

                let ledger_clone = ledger_handle.clone();
                let audit_store = gov_store.clone();

                // Get treasury DID from config, falling back to node DID if not configured
                let treasury_did = self
                    .config
                    .cooperative
                    .treasury_did
                    .as_ref()
                    .and_then(|s| {
                        serde_json::from_value::<Did>(serde_json::Value::String(s.clone())).ok()
                    })
                    .unwrap_or_else(|| {
                        debug!(
                            "No treasury_did configured, using node DID for budget payouts"
                        );
                        did.clone()
                    });

                event_bus.subscribe(Arc::new(move |event| {
                    match event {
                        SystemEvent::ProposalAccepted { proposal_id, payload, decided_at, .. } => {
                            match payload {
                                ProposalPayload::Budget { amount, recipient, currency, purpose: _ } => {
                                    info!("📊 Executing budget proposal {}: {} {} to {}",
                                          proposal_id.0, amount, currency, recipient);

                                    // Spawn async task to execute ledger transaction
                                    let ledger = ledger_clone.clone();
                                    let prop_id = proposal_id.clone();
                                    // Use treasury DID for budget payouts (or node DID if not configured)
                                    let from_did = treasury_did.clone();
                                    let store = audit_store.clone();
                                    let decision_time = decided_at;

                                    tokio::spawn(async move {
                                        use icn_ledger::entry::JournalEntryBuilder;
                                        use hex;

                                        // Track execution duration
                                        let start = std::time::Instant::now();

                                        // IDEMPOTENCY CHECK: Skip if proposal already executed
                                        // CRITICAL: Fail-safe approach - refuse to execute if we can't verify
                                        let audit_key = format!("gov:audit:{}", prop_id.0);
                                        match store.get(audit_key.as_bytes()) {
                                            Ok(Some(_)) => {
                                                // Already executed, skip
                                                debug!("Proposal {} already executed, skipping duplicate event", prop_id.0);
                                                icn_obs::metrics::governance::idempotent_skips_inc();
                                                return;
                                            }
                                            Ok(None) => {
                                                // Not executed yet, proceed
                                            }
                                            Err(e) => {
                                                // Store read error: REFUSE to execute (fail-safe)
                                                error!("🚨 CRITICAL: Failed to check audit trail for proposal {}: {}", prop_id.0, e);
                                                error!("   Cannot verify if proposal was already executed");
                                                error!("   Refusing to execute to prevent potential duplicate");
                                                error!("   ACTION REQUIRED: Fix storage issue and manually verify proposal execution status");
                                                icn_obs::metrics::governance::execution_failures_inc("audit_check_failed");
                                                return;
                                            }
                                        }

                                        let mut ledger_guard = ledger.write().await;

                                        // Create double-entry: credit cooperative treasury (decrease balance), debit recipient (increase balance)
                                        // Uses treasury_did from config (or falls back to node DID if not configured)
                                        let entry_result = JournalEntryBuilder::new(from_did.clone())
                                            .credit(from_did.clone(), currency.clone(), amount)
                                            .debit(recipient.clone(), currency.clone(), amount)
                                            .build();

                                        match entry_result {
                                            Ok(entry) => {
                                                match ledger_guard.append_entry(entry) {
                                                    Ok(entry_hash) => {
                                                        info!("✅ Budget proposal {} executed: {} {} transferred to {}",
                                                              prop_id.0, amount, currency, recipient);

                                                        // Store audit trail: governance decision → ledger entry
                                                        let audit_key = format!("gov:audit:{}", prop_id.0);
                                                        let audit_record = serde_json::json!({
                                                            "proposal_id": prop_id.0,
                                                            "ledger_entry_hash": hex::encode(entry_hash.0),
                                                            "amount": amount,
                                                            "currency": currency,
                                                            "recipient": recipient.to_string(),
                                                            "decided_at": decision_time,  // When governance decision was made
                                                            "executed_at": std::time::SystemTime::now()
                                                                .duration_since(std::time::UNIX_EPOCH)
                                                                .unwrap()
                                                                .as_secs(),  // When ledger transaction completed
                                                        });

                                                        // CRITICAL: Store audit trail - failure here creates inconsistent state
                                                        // (ledger updated but no governance record)
                                                        match serde_json::to_vec(&audit_record) {
                                                            Ok(audit_json) => {
                                                                match store.put(audit_key.as_bytes(), &audit_json) {
                                                                    Ok(_) => {
                                                                        info!("📋 Audit trail recorded for proposal {}", prop_id.0);

                                                                        // Metrics: successful execution
                                                                        let duration = start.elapsed().as_secs_f64();
                                                                        icn_obs::metrics::governance::proposals_executed_inc("budget");
                                                                        icn_obs::metrics::governance::execution_duration_record("budget", duration);
                                                                    }
                                                                    Err(e) => {
                                                                        // CRITICAL ERROR: Ledger updated but audit trail failed
                                                                        // Manual reconciliation required!
                                                                        error!(
                                                                            "🚨 CRITICAL: Ledger updated but audit trail write failed for proposal {}",
                                                                            prop_id.0
                                                                        );
                                                                        error!("   Ledger entry hash: {}", hex::encode(entry_hash.0));
                                                                        error!("   Amount: {} {}", amount, currency);
                                                                        error!("   Recipient: {}", recipient);
                                                                        error!("   Error: {}", e);
                                                                        error!("   ACTION REQUIRED: Manual reconciliation needed");
                                                                        // TODO: Write to dead-letter queue for automated reconciliation

                                                                        // Metrics: audit trail failure
                                                                        icn_obs::metrics::governance::audit_failures_inc();
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                error!(
                                                                    "🚨 CRITICAL: Failed to serialize audit record for proposal {}: {}",
                                                                    prop_id.0, e
                                                                );
                                                                error!("   Ledger entry hash: {}", hex::encode(entry_hash.0));
                                                                error!("   ACTION REQUIRED: Manual reconciliation needed");

                                                                // Metrics: audit trail failure
                                                                icn_obs::metrics::governance::audit_failures_inc();
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!("❌ Failed to append ledger entry for proposal {}: {}", prop_id.0, e);

                                                        // Metrics: ledger append failure
                                                        icn_obs::metrics::governance::execution_failures_inc("ledger_append");
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                warn!("❌ Failed to build ledger entry for proposal {}: {}", prop_id.0, e);

                                                // Metrics: ledger build failure
                                                icn_obs::metrics::governance::execution_failures_inc("ledger_build");
                                            }
                                        }
                                    });
                                }

                                ProposalPayload::ConfigChange { new_config } => {
                                    info!("⚙️  Config change proposal {} accepted: {:?}",
                                          proposal_id.0, new_config);
                                    // TODO: Apply configuration change
                                }

                                ProposalPayload::Membership { action, member } => {
                                    info!("👥 Membership change proposal {} accepted: {:?} for {}",
                                          proposal_id.0, action, member);
                                    // TODO: Update membership
                                }

                                ProposalPayload::Text { .. } => {
                                    // Text proposals don't trigger actions
                                    info!("📝 Text proposal {} accepted (no action required)", proposal_id.0);
                                }

                                ProposalPayload::SchedulingPolicy { coop_id, .. } => {
                                    // Handled by separate governance subscription (after compute actor spawns)
                                    info!("📋 Scheduling policy proposal {} accepted for {} (will be handled after compute initialization)",
                                          proposal_id.0, coop_id);
                                }

                                // === Emergency Proposals (Issue #25) ===

                                ProposalPayload::FreezeMember { member, reason, duration_seconds } => {
                                    info!("🔒 EMERGENCY: Freeze member proposal {} accepted - freezing {}",
                                          proposal_id.0, member);

                                    let ledger = ledger_clone.clone();
                                    let prop_id = proposal_id.clone();

                                    tokio::spawn(async move {
                                        let mut ledger_guard = ledger.write().await;
                                        ledger_guard.freeze_member_with_metadata(
                                            member.clone(),
                                            reason,
                                            duration_seconds,
                                            Some(prop_id.0.clone()),
                                            None, // frozen_by is the governance system
                                        );
                                        info!("✅ Member {} frozen via proposal {}", member, prop_id.0);
                                    });
                                }

                                ProposalPayload::UnfreezeMember { member, reason } => {
                                    info!("🔓 EMERGENCY: Unfreeze member proposal {} accepted - unfreezing {}",
                                          proposal_id.0, member);

                                    let ledger = ledger_clone.clone();
                                    let prop_id = proposal_id.clone();

                                    tokio::spawn(async move {
                                        let mut ledger_guard = ledger.write().await;
                                        if let Some(_record) = ledger_guard.unfreeze_member_with_metadata(
                                            &member,
                                            reason,
                                            Some(prop_id.0.clone()),
                                            None,
                                        ) {
                                            info!("✅ Member {} unfrozen via proposal {}", member, prop_id.0);
                                        } else {
                                            warn!("⚠️ Member {} was not frozen, unfreeze proposal {} had no effect",
                                                  member, prop_id.0);
                                        }
                                    });
                                }

                                ProposalPayload::VetoProposal { target_proposal_id, reason } => {
                                    info!("🚫 EMERGENCY: Veto proposal {} accepted - vetoing proposal {}",
                                          proposal_id.0, target_proposal_id);
                                    info!("   Reason: {}", reason);
                                    // TODO: Implement veto logic - mark target proposal as vetoed
                                    // This requires governance store integration to update proposal state
                                    warn!("⚠️ Veto proposal execution not yet implemented");
                                }

                                ProposalPayload::ForceCloseProposal { target_proposal_id, reason, forced_outcome } => {
                                    info!("⚡ EMERGENCY: Force close proposal {} accepted - closing proposal {} as {:?}",
                                          proposal_id.0, target_proposal_id, forced_outcome);
                                    info!("   Reason: {}", reason);
                                    // TODO: Implement force close logic
                                    // This requires governance store integration to close proposal early
                                    warn!("⚠️ Force close proposal execution not yet implemented");
                                }

                                ProposalPayload::RollbackLedger { target_hash, reason, ref affected_accounts } => {
                                    error!("🚨 CRITICAL EMERGENCY: Ledger rollback proposal {} accepted",
                                           proposal_id.0);
                                    error!("   Target hash: {}", target_hash);
                                    error!("   Reason: {}", reason);
                                    error!("   Affected accounts: {:?}", affected_accounts);
                                    // TODO: Implement ledger rollback logic
                                    // This is a very dangerous operation that requires:
                                    // 1. Verify target_hash exists in ledger
                                    // 2. Archive entries after target_hash
                                    // 3. Recompute balances from genesis to target_hash
                                    // 4. Broadcast rollback notification to network
                                    warn!("⚠️ Ledger rollback execution not yet implemented - MANUAL INTERVENTION REQUIRED");
                                }
                            }
                        }

                        SystemEvent::ProposalRejected { proposal_id, .. } => {
                            info!("❌ Proposal {} rejected - no action taken", proposal_id.0);
                        }

                        _ => {}
                    }
                })).await
            });

            info!("✓ Governance event handlers registered");

            // Spawn Compute actor for distributed task execution
            let trust_graph_for_compute = trust_graph_handle.clone();
            let compute_trust_callback: icn_compute::TrustCallback =
                Arc::new(move |did_str: &str| {
                    let graph = trust_graph_for_compute.clone();
                    let did_string = did_str.to_string();
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let did: icn_identity::Did = match serde_json::from_value(
                                serde_json::Value::String(did_string.clone()),
                            ) {
                                Ok(d) => d,
                                Err(_) => return 0.0,
                            };
                            let graph = graph.read().await;
                            graph.compute_trust_score(&did).unwrap_or(0.0)
                        })
                    })
                });

            let mut compute_actor =
                icn_compute::ComputeActor::new(did.to_string(), compute_trust_callback);

            // Set up compute send callback to route through gossip
            let gossip_handle_for_compute = gossip_handle.clone();
            let compute_send_callback: icn_compute::SendCallback = Arc::new(move |compute_msg| {
                let gossip = gossip_handle_for_compute.clone();
                tokio::spawn(async move {
                    let (topic, data) = match &compute_msg {
                        icn_compute::ComputeMessage::TaskSubmitted(_) => {
                            (icn_compute::TOPIC_SUBMIT, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::TaskClaimed { .. } => {
                            (icn_compute::TOPIC_CLAIM, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::TaskResult(_) => {
                            (icn_compute::TOPIC_RESULT, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::TaskCancelled { .. } => {
                            (icn_compute::TOPIC_CANCEL, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::ExecutorAnnounce { .. } => {
                            (icn_compute::TOPIC_SUBMIT, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::PlacementRequest { .. } => {
                            // Phase 16B: Broadcast placement requests for executor bidding
                            (icn_compute::TOPIC_SUBMIT, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::PlacementOffer { .. } => {
                            // Phase 16B: Broadcast placement offers (executor bids)
                            (icn_compute::TOPIC_CLAIM, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::NodeCapacityAnnounce { .. } => {
                            // Phase 16B: Broadcast capacity updates
                            (icn_compute::TOPIC_SUBMIT, bincode::serialize(&compute_msg))
                        }
                        icn_compute::ComputeMessage::CheckpointQuery { .. } => {
                            // Phase 16D: Checkpoint query for migration
                            (
                                icn_compute::TOPIC_CHECKPOINT,
                                bincode::serialize(&compute_msg),
                            )
                        }
                        icn_compute::ComputeMessage::CheckpointResponse { .. } => {
                            // Phase 16D: Checkpoint response for migration
                            (
                                icn_compute::TOPIC_CHECKPOINT,
                                bincode::serialize(&compute_msg),
                            )
                        }
                        icn_compute::ComputeMessage::MigrationRequest { .. } => {
                            // Phase 16D: Migration request
                            (
                                icn_compute::TOPIC_MIGRATION,
                                bincode::serialize(&compute_msg),
                            )
                        }
                        icn_compute::ComputeMessage::MigrationAccept { .. } => {
                            // Phase 16D: Migration acceptance
                            (
                                icn_compute::TOPIC_MIGRATION,
                                bincode::serialize(&compute_msg),
                            )
                        }
                        icn_compute::ComputeMessage::CheckpointAnnounce { .. } => {
                            // Phase 16D: Checkpoint announcement
                            (
                                icn_compute::TOPIC_CHECKPOINT,
                                bincode::serialize(&compute_msg),
                            )
                        }
                        icn_compute::ComputeMessage::MigrationReject { .. } => {
                            // Phase 16D: Migration rejection
                            (
                                icn_compute::TOPIC_MIGRATION,
                                bincode::serialize(&compute_msg),
                            )
                        }
                        icn_compute::ComputeMessage::MigrationComplete { .. } => {
                            // Phase 16D: Migration completion
                            (
                                icn_compute::TOPIC_MIGRATION,
                                bincode::serialize(&compute_msg),
                            )
                        }
                    };

                    match data {
                        Ok(bytes) => {
                            let mut gossip = gossip.write().await;
                            if let Err(e) = gossip.publish(topic, bytes) {
                                warn!("Failed to publish compute message to {}: {}", topic, e);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to serialize compute message: {}", e);
                        }
                    }
                });
            });

            compute_actor.set_send_callback(compute_send_callback);

            // Set up payment callback to settle via ledger
            let ledger_for_compute = ledger_handle.clone();
            let compute_payment_callback: icn_compute::PaymentCallback = Arc::new(move |req| {
                let ledger = ledger_for_compute.clone();
                tokio::spawn(async move {
                    // Parse DIDs
                    let from_did: icn_identity::Did =
                        match serde_json::from_value(serde_json::Value::String(req.from.clone())) {
                            Ok(d) => d,
                            Err(e) => {
                                warn!("Failed to parse payer DID: {}", e);
                                return;
                            }
                        };
                    let to_did: icn_identity::Did =
                        match serde_json::from_value(serde_json::Value::String(req.to.clone())) {
                            Ok(d) => d,
                            Err(e) => {
                                warn!("Failed to parse payee DID: {}", e);
                                return;
                            }
                        };

                    // Create journal entry for transfer: debit from payer, credit to payee
                    let entry = match icn_ledger::entry::JournalEntryBuilder::new(from_did.clone())
                        .debit(from_did, req.currency.clone(), req.amount as i64)
                        .credit(to_did, req.currency.clone(), req.amount as i64)
                        .build()
                    {
                        Ok(e) => e,
                        Err(e) => {
                            warn!("Failed to build payment entry: {}", e);
                            return;
                        }
                    };

                    // Append to ledger
                    let mut ledger = ledger.write().await;
                    match ledger.append_entry(entry) {
                        Ok(_) => {
                            info!(
                                "✓ Compute payment settled: {} {} from {} to {} for task {}",
                                req.amount, req.currency, req.from, req.to, req.task_id
                            );
                        }
                        Err(e) => {
                            warn!("Failed to settle compute payment: {}", e);
                        }
                    }
                });
            });
            compute_actor.set_payment_callback(compute_payment_callback);

            // Create shared event broadcaster for WebSocket delivery
            let broadcaster = Arc::new(icn_gateway::EventBroadcaster::new());

            // Set up event callback for task status changes (logs + metrics + WebSocket forwarding)
            let broadcaster_for_callback = broadcaster.clone();
            let compute_event_callback: icn_compute::EventCallback = Arc::new(move |event| {
                // Log and update metrics
                match &event {
                    icn_compute::ComputeEvent::TaskClaimed {
                        task_hash,
                        executor,
                    } => {
                        info!("🔧 Task claimed: {} by {}", task_hash, executor);
                        icn_obs::metrics::compute::tasks_claimed_inc();
                    }
                    icn_compute::ComputeEvent::TaskCompleted {
                        task_hash,
                        executor,
                        outcome,
                        fuel_used,
                        duration_ms,
                    } => {
                        info!(
                            "✅ Task completed: {} by {} - outcome: {}, fuel: {}, duration: {}ms",
                            task_hash, executor, outcome, fuel_used, duration_ms
                        );
                        icn_obs::metrics::compute::tasks_completed_inc(outcome);
                    }
                }

                // Forward to EventBroadcaster for WebSocket delivery
                let broadcaster = broadcaster_for_callback.clone();
                tokio::spawn(async move {
                    icn_gateway::forward_compute_event(&broadcaster, event).await;
                });
            });
            compute_actor.set_event_callback(compute_event_callback);

            // Initialize policy manager for cooperative scheduling (Phase 16E)
            let usage_tracker = Arc::new(icn_compute::UsageTracker::new());
            let policy_manager = Arc::new(icn_compute::PolicyManager::new(usage_tracker.clone()));
            compute_actor.set_policy_manager(policy_manager.clone());
            info!("✓ Policy manager initialized for cooperative scheduling");

            // Initialize dispute resolution system (Phase 18 Week 4)
            let dispute_store_path = self.config.store_path().join("disputes");
            let dispute_store: Arc<dyn icn_store::Store> =
                Arc::new(SledStore::open(&dispute_store_path)?);
            let dispute_config = icn_ccl::DisputeConfig::default();
            let dispute_system =
                icn_ccl::DisputeResolutionSystem::new(dispute_config.clone(), dispute_store);
            let dispute_system_handle = Arc::new(tokio::sync::RwLock::new(dispute_system));
            compute_actor.set_dispute_resolution(dispute_system_handle.clone());
            info!(
                "✓ Dispute resolution system initialized (re-execution timeout: {:?})",
                dispute_config.re_execution_timeout
            );

            // Set signing key for result signatures
            let signing_key_bytes = identity_bundle.keypair().to_signing_key_bytes();
            compute_actor.set_signing_key(signing_key_bytes.to_vec());

            // Set misbehavior detector for Byzantine fault detection (Phase 18)
            compute_actor.set_misbehavior_detector(misbehavior_detector.clone());

            let compute_handle = compute_actor.spawn();

            // Fill compute handle holder for notification callback
            *compute_handle_holder.write().await = Some(compute_handle.clone());

            info!("✓ Compute actor spawned with payment settlement");

            // Subscribe to compute topics
            {
                let mut gossip = gossip_handle.write().await;
                for topic in &[
                    icn_compute::TOPIC_SUBMIT,
                    icn_compute::TOPIC_CLAIM,
                    icn_compute::TOPIC_RESULT,
                    icn_compute::TOPIC_CANCEL,
                ] {
                    if let Err(e) = gossip.subscribe(topic, did.clone()) {
                        warn!("Failed to subscribe to compute topic {}: {}", topic, e);
                    } else {
                        info!("Subscribed to compute topic: {}", topic);
                    }
                }
            }

            // Subscribe to governance events for scheduling policy updates (Phase 16E integration)
            policy_governance_subscription = Some({
                use crate::events::SystemEvent;
                use icn_governance::ProposalPayload;

                let compute = compute_handle.clone();
                let audit_store = gov_store.clone();

                event_bus.subscribe(Arc::new(move |event| {
                    if let SystemEvent::ProposalAccepted {
                        proposal_id,
                        payload: ProposalPayload::SchedulingPolicy { coop_id, policy_json },
                        decided_at,
                        ..
                    } = event {
                        info!("📋 Executing scheduling policy proposal {}: update policy for {}",
                              proposal_id.0, coop_id);

                        // Spawn async task to apply policy update
                        let compute_handle = compute.clone();
                        let prop_id = proposal_id.clone();
                        let store = audit_store.clone();
                        let coop = coop_id.clone();
                        let policy_str = policy_json.clone();
                        let decision_time = decided_at;

                        tokio::spawn(async move {
                            // Track execution duration
                            let start = std::time::Instant::now();

                            // IDEMPOTENCY CHECK: Skip if proposal already executed
                            let audit_key = format!("gov:audit:policy:{}", prop_id.0);
                            match store.get(audit_key.as_bytes()) {
                                Ok(Some(_)) => {
                                    debug!("Policy proposal {} already executed, skipping duplicate", prop_id.0);
                                    return;
                                }
                                Ok(None) => {
                                    // Not executed yet, proceed
                                }
                                Err(e) => {
                                    error!("🚨 Failed to check audit trail for policy proposal {}: {}", prop_id.0, e);
                                    error!("   Refusing to execute to prevent potential duplicate");
                                    return;
                                }
                            }

                            // Parse policy JSON
                            match serde_json::from_str::<icn_compute::CoopSchedulingPolicy>(&policy_str) {
                                Ok(policy) => {
                                    // Apply policy update via ComputeHandle
                                    match compute_handle.set_policy(policy.clone()).await {
                                        Ok(_) => {
                                            info!("✅ Scheduling policy proposal {} executed: policy updated for {}",
                                                  prop_id.0, coop);

                                            // Store audit trail
                                            let audit_record = serde_json::json!({
                                                "proposal_id": prop_id.0,
                                                "coop_id": coop,
                                                "decided_at": decision_time,
                                                "executed_at": std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .unwrap()
                                                    .as_secs(),
                                            });

                                            if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                                                if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                                                    error!("🚨 Failed to store audit trail for policy proposal {}: {}", prop_id.0, e);
                                                } else {
                                                    info!("📋 Audit trail recorded for policy proposal {}", prop_id.0);

                                                    // Metrics: successful execution
                                                    let duration = start.elapsed().as_secs_f64();
                                                    icn_obs::metrics::governance::proposals_executed_inc("scheduling_policy");
                                                    icn_obs::metrics::governance::execution_duration_record("scheduling_policy", duration);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("❌ Failed to apply scheduling policy for proposal {}: {}", prop_id.0, e);
                                            icn_obs::metrics::governance::execution_failures_inc("policy_apply");
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("❌ Failed to parse policy JSON for proposal {}: {}", prop_id.0, e);
                                    icn_obs::metrics::governance::execution_failures_inc("policy_parse");
                                }
                            }
                        });
                    }
                })).await
            });

            info!("✓ Policy governance integration active");

            // Spawn RPC server with network, ledger, contract, gossip, and governance handles
            let rpc_port = self.config.network.rpc_port;
            let rpc_addr = format!("127.0.0.1:{rpc_port}").parse()?;
            let mut rpc_server = RpcServer::new(rpc_addr);
            rpc_server.set_network_handle(network_handle.clone());
            rpc_server.set_ledger_handle(ledger_handle.clone());
            rpc_server.set_contract_runtime(contract_runtime_handle.clone());
            rpc_server.set_gossip_handle(gossip_handle.clone());
            rpc_server.set_governance_handle(governance_handle);
            rpc_server.set_compute_handle(compute_handle);

            background_tasks.spawn(async move {
                if let Err(e) = rpc_server.run().await {
                    warn!("RPC server error: {}", e);
                }
            });

            info!("RPC server spawned on {}", rpc_addr);

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

            // Start periodic partition checker (Phase 18 Week 3)
            let _partition_checker_handle = icn_gossip::start_partition_checker(
                gossip_handle.clone(),
                30_000, // 30 seconds check interval (matches PartitionConfig default)
                self.shutdown_tx.subscribe(),
            );

            info!("Partition checker spawned");

            // Start clock synchronization background task
            let clock_sync = ClockSync::new();
            let sync_interval = Duration::from_secs(600); // 10 minutes
            let mut clock_sync_shutdown = self.shutdown_tx.subscribe();
            background_tasks.spawn(async move {
                let mut sync = clock_sync;
                loop {
                    // Perform sync
                    if let Err(e) = sync.sync().await {
                        warn!("Clock sync failed: {}", e);
                        icn_obs::metrics::scalability::clock_sync_failed_inc();
                    }

                    // Wait for next sync interval or shutdown
                    select! {
                        _ = tokio::time::sleep(sync_interval) => {
                            // Continue to next sync
                        }
                        _ = clock_sync_shutdown.recv() => {
                            info!("Clock sync task shutting down");
                            break;
                        }
                    }
                }
            });

            info!("Clock sync background task spawned (interval: 10 minutes)");

            // Spawn metrics update task
            let start_time = std::time::Instant::now();
            let network_handle_metrics = network_handle.clone();
            let mut metrics_shutdown = self.shutdown_tx.subscribe();
            background_tasks.spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Update uptime
                            let uptime_secs = start_time.elapsed().as_secs();
                            icn_obs::metrics::system::uptime_seconds_set(uptime_secs);

                            // Count active actors (network + gossip + ledger + rpc + anti-entropy + digest-emitter + cache-cleanup = 7)
                            icn_obs::metrics::system::actors_active_set(7);

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

            // Activate node profile now that all actors are initialized (Phase 17)
            {
                let mut profile = node_profile_handle.write().await;
                profile.activate();
                info!(
                    "Node profile activated: {} ready to accept work",
                    profile.node_did
                );
            }

            // Assign broadcaster to outer scope for gateway use
            event_broadcaster = Some(broadcaster);

            (
                Some(network_handle),
                Some(gossip_handle),
                Some(ledger_handle),
            )
        } else {
            warn!("No identity bundle available - actors not spawned");
            warn!("Run 'icnctl id init' to create an identity");

            // No governance subscriptions when identity bundle unavailable
            governance_event_subscription = None;
            policy_governance_subscription = None;

            // Still spawn metrics update task for system metrics
            let start_time = std::time::Instant::now();
            let mut metrics_shutdown = self.shutdown_tx.subscribe();
            background_tasks.spawn(async move {
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

            // No event broadcaster without identity
            event_broadcaster = None;

            (None, None, None)
        };

        // Spawn Gateway API server if enabled (works with or without identity)
        if self.config.gateway.enabled {
            info!(
                "Gateway spawn check - enabled: true, jwt_secret length: {}",
                self.config.gateway.jwt_secret.len()
            );
            let gateway_addr: std::net::SocketAddr = self.config.gateway.bind_addr.parse()?;

            // Check that JWT secret is configured
            if self.config.gateway.jwt_secret.is_empty() {
                warn!("Gateway enabled but JWT secret not configured - gateway will not start");
                warn!("Set jwt_secret in config or ICN_GATEWAY_JWT_SECRET environment variable");
            } else {
                info!("Gateway JWT secret verified, spawning server...");

                let jwt_secret = self.config.gateway.jwt_secret.clone().into_bytes();
                let data_dir = self.config.data_dir.clone();

                // Get event broadcaster if compute actor was created
                let broadcaster_for_gateway = if let Some(ref broadcaster) = event_broadcaster {
                    info!("Using shared EventBroadcaster for real-time WebSocket delivery");
                    Some(broadcaster.clone())
                } else {
                    None
                };

                // Spawn gateway in a dedicated thread (actix-web has its own runtime)
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let gateway_server = if let Some(broadcaster) = broadcaster_for_gateway {
                            icn_gateway::GatewayServer::new_with_broadcaster(
                                gateway_addr,
                                jwt_secret,
                                Some(data_dir),
                                broadcaster,
                            )
                        } else {
                            icn_gateway::GatewayServer::new(gateway_addr, jwt_secret)
                        };

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

        // Wait for background tasks to complete gracefully (with timeout)
        info!("Waiting for background tasks to complete...");
        let shutdown_timeout = tokio::time::Duration::from_secs(5);
        match tokio::time::timeout(shutdown_timeout, async {
            while background_tasks.join_next().await.is_some() {
                // Task completed successfully
            }
        })
        .await
        {
            Ok(_) => info!("All background tasks completed successfully"),
            Err(_) => warn!(
                "Shutdown timeout reached, {} tasks may still be running",
                background_tasks.len()
            ),
        }

        // Abort any remaining tasks that didn't finish
        background_tasks.shutdown().await;

        // Graceful shutdown of actors
        info!("Supervisor shutting down actors");

        // Save state snapshot before actors are dropped
        // Errors during state export should not prevent graceful shutdown
        if gossip_handle.is_some() || network_handle.is_some() {
            info!("Saving state snapshot before shutdown");
            let snapshot_result = async {
                let mut snapshot = icn_snapshot::StateSnapshot::new();

                // Export gossip state
                if let Some(ref gossip_handle) = gossip_handle {
                    let gossip_state = gossip_handle.read().await.export_state();
                    info!(
                        "Exported gossip state: {} vector clock entries, {} subscriptions",
                        gossip_state.vector_clock.len(),
                        gossip_state.subscriptions.len()
                    );
                    snapshot.gossip_state = Some(gossip_state);
                }

                // Export network state
                if let Some(ref network_handle) = network_handle {
                    // Need to use blocking context for async export_state
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current()
                                .block_on(async { network_handle.export_state().await })
                        })
                    })) {
                        Ok(state) => {
                            info!(
                                "Exported network state: {} peer X25519 keys",
                                state.peer_x25519_keys.len()
                            );
                            snapshot.network_state = Some(state);
                        }
                        Err(e) => {
                            warn!("Failed to export network state (panic): {:?}", e);
                        }
                    }
                }

                Ok::<_, anyhow::Error>(snapshot)
            }
            .await;

            match snapshot_result {
                Ok(snapshot) => {
                    // Record snapshot metrics before saving
                    if let Some(ref gossip_state) = snapshot.gossip_state {
                        icn_obs::metrics::snapshot::gossip_vector_clock_entries_set(
                            gossip_state.vector_clock.len(),
                        );
                        icn_obs::metrics::snapshot::gossip_subscriptions_set(
                            gossip_state.subscriptions.len(),
                        );
                        icn_obs::metrics::snapshot::gossip_topics_set(gossip_state.topics.len());
                    }
                    if let Some(ref network_state) = snapshot.network_state {
                        icn_obs::metrics::snapshot::network_x25519_keys_set(
                            network_state.peer_x25519_keys.len(),
                        );
                    }

                    // Save snapshot to disk
                    let data_dir = self.config.store_path();
                    let save_start = std::time::Instant::now();
                    let save_result = icn_snapshot::save_snapshot(&snapshot, &data_dir);
                    let save_duration = save_start.elapsed();

                    match save_result {
                        Ok(()) => {
                            icn_obs::metrics::snapshot::save_total_inc();
                            icn_obs::metrics::snapshot::save_duration_record(
                                save_duration.as_secs_f64(),
                            );

                            // Record snapshot file size
                            let snapshot_path = data_dir.join("state.snapshot");
                            if let Ok(metadata) = std::fs::metadata(&snapshot_path) {
                                icn_obs::metrics::snapshot::size_bytes_set(metadata.len());
                            }

                            info!(
                                "✅ State snapshot saved to {}/state.snapshot in {:.3}s",
                                data_dir.display(),
                                save_duration.as_secs_f64()
                            );

                            // Save timestamped backup for archival
                            if let Err(e) =
                                icn_snapshot::save_timestamped_snapshot(&snapshot, &data_dir)
                            {
                                warn!("Failed to save timestamped snapshot backup: {}", e);
                            }

                            // Cleanup old snapshots (keep last 3)
                            match icn_snapshot::cleanup_old_snapshots(&data_dir, 3) {
                                Ok(deleted) if deleted > 0 => {
                                    info!("Cleaned up {} old snapshot(s)", deleted);
                                }
                                Ok(_) => {}
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
                Err(e) => {
                    warn!("Failed to export actor state during shutdown: {}", e);
                    // Continue with shutdown even if state export failed
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

        // Governance event subscriptions are kept alive by holding their handles
        // for the lifetime of this function. When this function returns, the handles
        // are dropped and the subscriptions are automatically removed from the event bus.
        // This prevents memory leaks while ensuring subscriptions remain active during runtime.
        drop(governance_event_subscription);
        drop(policy_governance_subscription);

        info!("Supervisor stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bootstrap_peer_valid() {
        let url =
            "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(
            did.as_str(),
            "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
        );
        assert_eq!(addr.to_string(), "203.0.113.50:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_ipv4() {
        let url =
            "icn://did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH@192.168.1.100:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(
            did.as_str(),
            "did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
        );
        assert_eq!(addr.to_string(), "192.168.1.100:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_prefix() {
        let url = "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with 'icn://'"));
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_at() {
        let url =
            "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK_203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected icn://DID@IP:PORT"));
    }

    #[test]
    fn test_parse_bootstrap_peer_invalid_port() {
        let url =
            "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:invalid";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse socket address"));
    }
}
