//! Supervisor for managing actors

pub mod background_tasks;
pub mod governance_handlers;
pub mod init_compute;
pub mod init_coop;
pub mod init_gossip;
pub mod init_governance;
pub mod init_ledger;
pub mod init_network;
pub mod init_notifications;
pub mod init_rpc;
pub mod init_snapshot;
pub mod init_steward;
pub mod init_trust;
pub mod registry;
pub mod shutdown;
pub mod version_tracker;

use anyhow::{bail, Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_rpc::RpcServer;
use icn_store::SledStore;
use serde_json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::select;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::runtime::ShutdownTx;

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

        // Set supervisor state to starting
        icn_obs::metrics::supervisor::state_set(1);

        // Start metrics server
        let metrics_port = self.config.observability.metrics_port;
        if let Err(e) = icn_obs::start_metrics_server(metrics_port).await {
            warn!("Failed to start metrics server: {}", e);
            icn_obs::metrics::supervisor::error_inc("metrics_server_start");
        }

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Track spawned background tasks for graceful shutdown
        let mut background_tasks: JoinSet<()> = JoinSet::new();

        // Event broadcaster for WebSocket delivery (shared between compute actor and gateway)
        let event_broadcaster: Option<Arc<icn_gateway::EventBroadcaster>>;

        // Compute handle for gateway integration
        let compute_handle_for_gateway: Option<icn_compute::ComputeHandle>;

        // Cooperative handle for gateway integration
        let coop_handle_for_gateway: Option<icn_coop::CoopHandle>;

        // Trust graph handle for gateway integration
        let trust_graph_handle_for_gateway: Option<
            Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>,
        >;

        // Governance handle for gateway integration
        let governance_handle_for_gateway: Option<
            Arc<dyn icn_governance::GovernanceOps + Send + Sync>,
        >;

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

            // Initialize trust graph, recovery store, and misbehavior detector
            let trust_services = init_trust::init_trust_services(&self.config, did.clone()).await?;
            let trust_graph_handle = trust_services.trust_graph.clone();
            let misbehavior_detector = trust_services.misbehavior_detector.clone();
            let recovery_store = trust_services.recovery_store.clone();

            // Initialize snapshot coordinator
            let snapshot_coordinator =
                init_snapshot::init_snapshot_coordinator(did.clone()).await?;
            info!("Snapshot coordinator initialized");

            // Create trust lookup closure for gossip actor
            let trust_lookup = init_trust::create_trust_lookup(trust_graph_handle.clone());

            // Initialize gossip services
            let gossip_services = init_gossip::init_gossip_services(
                &self.config,
                did.clone(),
                init_gossip::GossipDeps {
                    trust_graph: trust_graph_handle.clone(),
                    trust_lookup,
                    misbehavior_detector: misbehavior_detector.clone(),
                    identity_bundle: identity_bundle.clone(),
                },
            )
            .await?;
            icn_obs::metrics::supervisor::actor_spawned_inc("gossip");
            let gossip_handle = gossip_services.gossip_handle.clone();
            let _gossip_store = gossip_services.gossip_store.clone(); // Kept for potential future use
            let loaded_snapshot = gossip_services.loaded_snapshot;

            // Initialize ledger and contract services
            let ledger_services = init_ledger::init_ledger_services(
                &self.config,
                did.clone(),
                init_ledger::LedgerDeps {
                    gossip_handle: gossip_handle.clone(),
                    misbehavior_detector: misbehavior_detector.clone(),
                    trust_graph: trust_graph_handle.clone(),
                },
            )
            .await?;
            icn_obs::metrics::supervisor::actor_spawned_inc("ledger");
            let ledger_handle = ledger_services.ledger_handle.clone();
            let dispute_manager_handle = ledger_services.dispute_manager.clone();
            let contract_runtime_handle = ledger_services.contract_runtime.clone();
            let contract_actor_handle = ledger_services.contract_actor.clone();

            // Initialize cooperative services
            let coop_services =
                init_coop::init_coop_services(&self.config, gossip_handle.clone(), did.clone())
                    .await?;
            icn_obs::metrics::supervisor::actor_spawned_inc("coop");
            let coop_handle = coop_services.coop_handle.clone();
            let coop_store = coop_services.coop_store.clone(); // Used for gossip sync

            // Store handles for gateway integration (outside of identity_bundle scope)
            coop_handle_for_gateway = Some(coop_handle);
            trust_graph_handle_for_gateway = Some(trust_graph_handle.clone());

            // Spawn Identity actor (provides signing and trust graph access)
            let identity_handle = crate::identity::IdentityActor::spawn(
                identity_bundle.keypair().clone(),
                trust_graph_handle.clone(),
                self.shutdown_tx.clone(),
            );

            info!("Identity actor spawned with DID: {}", identity_handle.did());

            // Spawn Network actor with gossip bridge
            let listen_addr: std::net::SocketAddr = self.config.network.listen_addr.parse()?;
            let federation_enabled = self.config.federation.enabled;

            // Create incoming message handler that routes to gossip
            let network_handle_for_handler =
                Arc::new(tokio::sync::RwLock::new(None::<icn_net::NetworkHandle>));

            let incoming_handler =
                init_network::create_incoming_handler(init_network::MessageHandlerDeps {
                    gossip_handle: gossip_handle.clone(),
                    network_handle_holder: network_handle_for_handler.clone(),
                    own_did: did.clone(),
                    federation_enabled,
                });

            // Prepare rate limiting configuration
            let (trust_graph_for_rate_limit, trust_gated_config, fallback_config) =
                if self.config.rate_limiting.enabled {
                    (
                        Some(trust_graph_handle.clone()), // Enable trust-gated rate limiting
                        Some(
                            self.config
                                .rate_limiting
                                .to_trust_gated_config(self.config.network.min_trust_threshold),
                        ),
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

            // Get TURN config for NAT traversal fallback (Phase 4 M1)
            let turn_config = self.config.network.turn_config();

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
                turn_config,
                Some(misbehavior_detector.clone()), // Shared Byzantine fault detector
            )
            .await?;

            // Initialize network handle for the incoming message handler
            *network_handle_for_handler.write().await = Some(network_handle.clone());

            icn_obs::metrics::supervisor::actor_spawned_inc("network");
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

            // Create dispute handle holder (will be filled after DisputeActor is spawned)
            let dispute_handle_holder: Arc<
                tokio::sync::RwLock<Option<icn_ccl::DisputeActorHandle>>,
            > = Arc::new(tokio::sync::RwLock::new(None));

            // Create steward handle holder (will be filled if steward is enabled)
            let steward_handle_holder: Arc<
                tokio::sync::RwLock<Option<icn_steward::StewardHandle>>,
            > = Arc::new(tokio::sync::RwLock::new(None));

            // Keep a reference to federation registry for RPC server (declared here to be in scope for later use)
            let federation_registry_for_rpc: Option<Arc<icn_federation::CooperativeRegistry>>;

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
                let coop_store_for_notifications = coop_store.clone();

                // Create candidate cache for NAT traversal
                let candidate_cache = Arc::new(icn_net::CandidateCache::new());
                let candidate_cache_for_notifications = candidate_cache.clone();
                let candidate_cache_for_cleanup = candidate_cache.clone();
                let network_handle_for_candidates = network_handle.clone();

                let compute_handle_for_notifications = compute_handle_holder.clone();
                let dispute_handle_for_notifications = dispute_handle_holder.clone();

                // Clone snapshot coordinator for notifications
                let snapshot_coordinator_for_notifications = snapshot_coordinator.clone();
                let gossip_handle_for_notifications = gossip_handle.clone();

                // Create profile cache for peer capability discovery
                let profile_cache: Arc<
                    tokio::sync::RwLock<std::collections::HashMap<Did, crate::node::NodeProfile>>,
                > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
                let profile_cache_for_notifications = profile_cache.clone();
                let node_profile_for_notifications = node_profile_handle.clone();

                // Create rate limiter for trust attestation anti-flood protection
                let attestation_rate_limiter =
                    Arc::new(crate::trust_propagation::AttestationRateLimiter::new());

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

                    // Keep reference for RPC server
                    federation_registry_for_rpc = Some(federation_registry.clone());

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
                    federation_registry_for_rpc = None;
                    None
                };

                // Clone federation handler for announcement task (before it's moved into notification callback)
                let federation_handler_for_announce = federation_handler_for_notifications.clone();

                // Create notification callback using extracted module
                let notification_callback = init_notifications::create_notification_callback(
                    init_notifications::NotificationDeps {
                        trust_graph: trust_graph_for_notifications,
                        own_did: own_did_for_notifications,
                        contract_actor: contract_actor_for_notifications,
                        recovery_store: recovery_store_for_notifications,
                        ledger: ledger_for_notifications,
                        snapshot_coordinator: snapshot_coordinator_for_notifications,
                        network_handle: network_handle_for_candidates.clone(),
                        candidate_cache: candidate_cache_for_notifications,
                        gossip_handle: gossip_handle_for_notifications,
                        compute_handle: compute_handle_for_notifications,
                        dispute_handle: dispute_handle_for_notifications,
                        node_profile: node_profile_for_notifications,
                        profile_cache: profile_cache_for_notifications,
                        coop_store: coop_store_for_notifications,
                        federation_handler: federation_handler_for_notifications,
                        attestation_rate_limiter,
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

                // Subscribe to standard gossip topics
                init_gossip::subscribe_standard_topics(
                    &mut gossip,
                    &did,
                    init_gossip::TopicSubscriptionConfig { federation_enabled },
                );

                // Spawn periodic federation announcement task (every 5 minutes) if enabled
                if federation_enabled {
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

                // Spawn candidate cache cleanup task
                let cache_cleanup_interval = std::time::Duration::from_secs(
                    self.config.supervisor.candidate_cleanup_interval_secs,
                );
                let mut cache_cleanup_shutdown = self.shutdown_tx.subscribe();
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(cache_cleanup_interval);
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

                    let peer_exchange_delay = std::time::Duration::from_millis(
                        self.config.supervisor.peer_exchange_delay_ms,
                    );
                    let peer_exchange_max = self.config.supervisor.peer_exchange_max_peers;

                    for peer_did in connected_bootstrap_peers {
                        // Small delay to allow Hello handshake to complete
                        tokio::time::sleep(peer_exchange_delay).await;

                        match network_handle
                            .request_peer_exchange(
                                &peer_did,
                                Some(peer_exchange_max),
                                network_filter.clone(),
                            )
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
                                match gossip
                                    .publish(init_gossip::NETWORK_CANDIDATES_TOPIC, candidate_bytes)
                                {
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

            // Initialize governance services (governance actor, upgrade actor, dead-letter queue)
            let governance_services = init_governance::init_governance_services(
                &self.config,
                did.clone(),
                init_governance::GovernanceDeps {
                    gossip_handle: gossip_handle.clone(),
                    event_bus: event_bus.clone(),
                    shutdown_rx: self.shutdown_tx.subscribe(),
                },
            )
            .await?;

            let governance_handle = governance_services.governance_handle;
            let _upgrade_handle = governance_services.upgrade_handle;
            let _version_tracker = governance_services.version_tracker;
            let dead_letter_queue = governance_services.dead_letter_queue;
            let gov_store = governance_services.governance_store;

            // Store governance handle for gateway integration
            governance_handle_for_gateway = Some(Arc::new(governance_handle.clone()));

            // Subscribe to governance events for ledger execution
            // CRITICAL: Must store handle to keep subscription alive for daemon lifetime
            governance_event_subscription = Some({
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
                        debug!("No treasury_did configured, using node DID for budget payouts");
                        did.clone()
                    });

                let handler = governance_handlers::GovernanceEventHandler::new(
                    ledger_handle.clone(),
                    gov_store.clone(),
                    dead_letter_queue.clone(),
                    governance_handle.clone(),
                    dispute_manager_handle.clone(),
                    treasury_did,
                );

                event_bus
                    .subscribe(governance_handlers::create_governance_subscription(handler))
                    .await
            });

            info!("✓ Governance event handlers registered");

            // Initialize compute actor using extracted module
            let compute_services = init_compute::init_compute_services(init_compute::ComputeDeps {
                trust_graph: trust_graph_handle.clone(),
                ledger: ledger_handle.clone(),
                gossip_handle: gossip_handle.clone(),
                own_did: did.clone(),
                compute_handle_holder: compute_handle_holder.clone(),
                dispute_handle_holder: dispute_handle_holder.clone(),
                network_handle: network_handle.clone(),
                misbehavior_detector: misbehavior_detector.clone(),
                identity_bundle: identity_bundle.clone(),
                store_path: self.config.store_path(),
            })
            .await?;

            let compute_handle = compute_services.compute_handle;
            let _dispute_handle = compute_services.dispute_handle;
            let broadcaster = compute_services.broadcaster;
            let _policy_manager = compute_services.policy_manager;

            // Subscribe to governance events for scheduling policy updates (Phase 16E integration)
            policy_governance_subscription = Some({
                let policy_handler = governance_handlers::PolicyEventHandler::new(
                    compute_handle.clone(),
                    gov_store.clone(),
                );
                event_bus
                    .subscribe(governance_handlers::create_policy_subscription(
                        policy_handler,
                    ))
                    .await
            });

            info!("✓ Policy governance integration active");

            // Spawn RPC server with network, ledger, contract, gossip, and governance handles
            let rpc_port = self.config.network.rpc_port;
            let rpc_addr = format!("127.0.0.1:{rpc_port}").parse()?;

            // Enable RPC auth if gateway is enabled with a jwt_secret
            let mut rpc_server =
                if self.config.gateway.enabled && !self.config.gateway.jwt_secret.is_empty() {
                    info!("RPC server auth enabled (using gateway JWT secret)");
                    RpcServer::new_with_auth(
                        rpc_addr,
                        self.config.gateway.jwt_secret.as_bytes().to_vec(),
                    )
                } else {
                    RpcServer::new(rpc_addr)
                };
            rpc_server.set_network_handle(network_handle.clone());
            rpc_server.set_ledger_handle(ledger_handle.clone());
            rpc_server.set_contract_runtime(contract_runtime_handle.clone());
            rpc_server.set_gossip_handle(gossip_handle.clone());
            rpc_server.set_governance_handle(governance_handle);
            // Clone compute_handle for gateway before giving to RPC
            compute_handle_for_gateway = Some(compute_handle.clone());
            rpc_server.set_compute_handle(compute_handle);
            rpc_server.set_trust_handle(trust_graph_handle.clone());
            rpc_server.set_dispute_manager(dispute_manager_handle);
            if let Some(registry) = federation_registry_for_rpc {
                rpc_server.set_federation_registry(registry);
            }

            // Enable trust-based rate limiting for API requests (C8)
            rpc_server.enable_trust_rate_limiting();

            background_tasks.spawn(async move {
                if let Err(e) = rpc_server.run().await {
                    warn!("RPC server error: {}", e);
                    icn_obs::metrics::supervisor::error_inc("rpc_server");
                }
            });

            icn_obs::metrics::supervisor::actor_spawned_inc("rpc_server");
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
            let _clock_sync_handle = background_tasks::spawn_clock_sync_task(
                background_tasks::ClockSyncConfig::default(),
                self.shutdown_tx.subscribe(),
            );

            info!("Clock sync background task spawned (interval: 10 minutes)");

            // Spawn steward actor if enabled (SDIS Phase S3)
            let _steward_services = init_steward::init_steward_services(
                &self.config.steward,
                init_steward::StewardDeps {
                    gossip_handle: gossip_handle.clone(),
                    steward_handle_holder: steward_handle_holder.clone(),
                    did: did.clone(),
                    keypair: identity_bundle.keypair().clone(),
                    shutdown_tx: self.shutdown_tx.clone(),
                },
            )
            .await
            .ok();

            // Spawn metrics update task
            let _metrics_handle = background_tasks::spawn_metrics_update_task(
                background_tasks::MetricsUpdateConfig::default(),
                network_handle.clone(),
                did.clone(),
                std::time::Instant::now(),
                self.shutdown_tx.subscribe(),
            );

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
            icn_obs::metrics::supervisor::error_inc("identity_bundle_missing");

            // No governance subscriptions when identity bundle unavailable
            governance_event_subscription = None;
            policy_governance_subscription = None;

            // Still spawn metrics update task for system metrics
            let start_time = std::time::Instant::now();
            let metrics_interval =
                std::time::Duration::from_secs(self.config.supervisor.metrics_update_interval_secs);
            let mut metrics_shutdown = self.shutdown_tx.subscribe();
            background_tasks.spawn(async move {
                let mut interval = tokio::time::interval(metrics_interval);
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

            // No event broadcaster or compute/trust/governance handles without identity
            event_broadcaster = None;
            compute_handle_for_gateway = None;
            coop_handle_for_gateway = None;
            trust_graph_handle_for_gateway = None;
            governance_handle_for_gateway = None;

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
                icn_obs::metrics::supervisor::error_inc("gateway_jwt_secret_missing");
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
                    // SAFETY: Runtime creation only fails with invalid config or resource exhaustion
                    #[allow(clippy::unwrap_used)]
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let mut gateway_server = if let Some(broadcaster) = broadcaster_for_gateway
                        {
                            icn_gateway::GatewayServer::new_with_broadcaster(
                                gateway_addr,
                                jwt_secret,
                                Some(data_dir),
                                broadcaster,
                            )
                        } else {
                            icn_gateway::GatewayServer::new(gateway_addr, jwt_secret)
                        };

                        // Connect compute handle if available
                        if let Some(handle) = compute_handle_for_gateway {
                            gateway_server = gateway_server.with_compute_handle(handle);
                        }

                        // Connect cooperative handle if available
                        if let Some(handle) = coop_handle_for_gateway {
                            gateway_server = gateway_server.with_coop_handle(handle);
                        }

                        // Connect trust graph handle if available
                        if let Some(handle) = trust_graph_handle_for_gateway {
                            gateway_server = gateway_server.with_trust_handle(handle);
                        }

                        // Connect governance handle if available
                        if let Some(handle) = governance_handle_for_gateway {
                            gateway_server = gateway_server.with_governance_handle(handle);
                        }

                        if let Err(e) = gateway_server.run().await {
                            warn!("Gateway server error: {}", e);
                            icn_obs::metrics::supervisor::error_inc("gateway_server");
                        }
                    });
                });

                icn_obs::metrics::supervisor::actor_spawned_inc("gateway");
                info!("Gateway API spawned on {}", gateway_addr);
            }
        } else {
            debug!("Gateway API disabled in configuration");
        }

        // Set supervisor state to running
        icn_obs::metrics::supervisor::state_set(2);

        // Wait for shutdown signal
        select! {
            _ = shutdown_rx.recv() => {
                info!("Supervisor received shutdown signal");
                icn_obs::metrics::supervisor::state_set(3); // stopping
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Supervisor received Ctrl+C");
                icn_obs::metrics::supervisor::state_set(3); // stopping
                let _ = self.shutdown_tx.send(());
            }
        }

        // Wait for background tasks to complete gracefully (with timeout)
        info!("Waiting for background tasks to complete...");
        let shutdown_timeout =
            tokio::time::Duration::from_secs(self.config.supervisor.shutdown_timeout_secs);
        match tokio::time::timeout(shutdown_timeout, async {
            while background_tasks.join_next().await.is_some() {
                // Task completed successfully
            }
        })
        .await
        {
            Ok(_) => info!("All background tasks completed successfully"),
            Err(_) => {
                warn!(
                    "Shutdown timeout reached, {} tasks may still be running",
                    background_tasks.len()
                );
                icn_obs::metrics::supervisor::error_inc("shutdown_timeout");
            }
        }

        // Abort any remaining tasks that didn't finish
        background_tasks.shutdown().await;

        // Graceful shutdown of actors
        info!("Supervisor shutting down actors");

        // Save state snapshot before actors are dropped
        shutdown::save_shutdown_snapshot(
            gossip_handle.as_ref(),
            network_handle.as_ref(),
            &self.config.store_path(),
        )
        .await;

        // Log actor shutdown status
        shutdown::log_actor_shutdown_status(
            network_handle.as_ref(),
            gossip_handle.as_ref(),
            ledger_handle.is_some(),
        );

        // Governance event subscriptions are kept alive by holding their handles
        // for the lifetime of this function. When this function returns, the handles
        // are dropped and the subscriptions are automatically removed from the event bus.
        // This prevents memory leaks while ensuring subscriptions remain active during runtime.
        drop(governance_event_subscription);
        drop(policy_governance_subscription);

        // Set supervisor state to stopped
        icn_obs::metrics::supervisor::state_set(0);
        info!("Supervisor stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valid test DID (proper multibase-encoded Ed25519 public key)
    /// Generated from deterministic seed [1u8; 32]
    const TEST_ALICE_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
    /// Generated from deterministic seed [3u8; 32]
    const TEST_BOB_DID: &str = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse";

    #[test]
    fn test_parse_bootstrap_peer_valid() {
        let url = format!("icn://{TEST_ALICE_DID}@203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), TEST_ALICE_DID);
        assert_eq!(addr.to_string(), "203.0.113.50:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_ipv4() {
        let url = format!("icn://{TEST_BOB_DID}@192.168.1.100:7777");
        let result = parse_bootstrap_peer(&url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), TEST_BOB_DID);
        assert_eq!(addr.to_string(), "192.168.1.100:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_prefix() {
        let url = format!("{TEST_ALICE_DID}@203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with 'icn://'"));
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_at() {
        let url = format!("icn://{TEST_ALICE_DID}_203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected icn://DID@IP:PORT"));
    }

    #[test]
    fn test_parse_bootstrap_peer_invalid_port() {
        let url = format!("icn://{TEST_ALICE_DID}@203.0.113.50:invalid");
        let result = parse_bootstrap_peer(&url);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse socket address"));
    }
}
