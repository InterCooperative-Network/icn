//! Supervisor lifecycle management
//!
//! This module contains the main supervisor startup and shutdown orchestration logic.
//! It coordinates actor initialization, background task spawning, and graceful shutdown.

use anyhow::Result;
use std::sync::Arc;
use tokio::select;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use icn_identity::IdentityBundle;

use crate::config::Config;
use crate::runtime::ShutdownTx;

use super::actors::{
    CoreActorHandles, EventSubscriptionHandles, GatewayActorHandles, ShutdownHandles,
};

/// Run the supervisor lifecycle
///
/// This is the main entry point for supervisor operation. It:
/// 1. Initializes metrics and observability
/// 2. Spawns all actors (if identity bundle is available)
/// 3. Configures actor communication and bridging
/// 4. Spawns background tasks
/// 5. Waits for shutdown signal
/// 6. Performs graceful shutdown
pub async fn run_supervisor(
    config: Config,
    identity_bundle: Option<IdentityBundle>,
    shutdown_tx: ShutdownTx,
) -> Result<()> {
    info!("Supervisor starting");

    // Initialize metrics
    icn_obs::init_metrics()?;

    // Set supervisor state to starting
    icn_obs::metrics::supervisor::state_set(1);

    // Start metrics server
    let metrics_port = config.observability.metrics_port;
    if let Err(e) = icn_obs::start_metrics_server(metrics_port).await {
        warn!("Failed to start metrics server: {}", e);
        icn_obs::metrics::supervisor::error_inc("metrics_server_start");
    }

    let mut shutdown_rx = shutdown_tx.subscribe();

    // Track spawned background tasks for graceful shutdown
    let mut background_tasks: JoinSet<()> = JoinSet::new();

    let mut gateway_handles = GatewayActorHandles::default();
    let mut event_subscriptions = EventSubscriptionHandles::default();
    let mut shutdown_handles = ShutdownHandles::default();

    // Spawn actors (requires identity bundle from unlocked keystore)
    let core_handles = if let Some(identity_bundle) = &identity_bundle {
        spawn_actors_with_identity(
            &config,
            identity_bundle,
            &shutdown_tx,
            &mut background_tasks,
            &mut gateway_handles,
            &mut event_subscriptions,
            &mut shutdown_handles,
        )
        .await?
    } else {
        spawn_without_identity(&config, &shutdown_tx, &mut background_tasks).await
    };

    // Spawn Gateway API server if enabled
    super::init_gateway::spawn_gateway(
        &config.gateway,
        config.data_dir.clone(),
        super::init_gateway::GatewayHandles {
            event_broadcaster: gateway_handles.event_broadcaster,
            compute: gateway_handles.compute,
            coop: gateway_handles.coop,
            community: gateway_handles.community,
            trust_graph: gateway_handles.trust_graph,
            governance: gateway_handles.governance,
            treasury: gateway_handles.treasury,
            ledger: gateway_handles.ledger,
            entity: gateway_handles.entity,
            steward: gateway_handles.steward,
            agreement_manager: gateway_handles.agreement_manager,
        },
    );

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
            let _ = shutdown_tx.send(());
        }
    }

    // Wait for background tasks to complete gracefully (with timeout)
    info!("Waiting for background tasks to complete...");
    let shutdown_timeout =
        tokio::time::Duration::from_secs(config.supervisor.shutdown_timeout_secs);
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
    super::shutdown::save_shutdown_snapshot(
        core_handles.gossip.as_ref(),
        core_handles.network.as_ref(),
        &config.store_path(),
    )
    .await;

    // Save misbehavior detector state (reputation scores, bans, quarantine)
    if let (Some(detector), Some(store)) = (
        &shutdown_handles.misbehavior_detector,
        &shutdown_handles.security_store,
    ) {
        super::shutdown::save_misbehavior_state(detector, store).await;
    }

    // Log actor shutdown status
    super::shutdown::log_actor_shutdown_status(
        core_handles.network.as_ref(),
        core_handles.gossip.as_ref(),
        core_handles.ledger.is_some(),
    );

    // Governance event subscriptions are kept alive by holding their handles
    // for the lifetime of this function. When this function returns, the handles
    // are dropped and the subscriptions are automatically removed from the event bus.
    // This prevents memory leaks while ensuring subscriptions remain active during runtime.
    drop(event_subscriptions.governance_event_subscription);
    drop(event_subscriptions.policy_governance_subscription);

    // Set supervisor state to stopped
    icn_obs::metrics::supervisor::state_set(0);
    info!("Supervisor stopped");
    Ok(())
}

/// Spawn all actors when identity bundle is available
#[allow(clippy::too_many_arguments)]
async fn spawn_actors_with_identity(
    config: &Config,
    identity_bundle: &IdentityBundle,
    shutdown_tx: &ShutdownTx,
    background_tasks: &mut JoinSet<()>,
    gateway_handles: &mut GatewayActorHandles,
    event_subscriptions: &mut EventSubscriptionHandles,
    shutdown_handles: &mut ShutdownHandles,
) -> Result<CoreActorHandles> {
    info!("Identity bundle available - spawning actors");

    let did = identity_bundle.did().clone();

    // Create NodeProfile with hardware sensing (Phase 17)
    let node_profile = crate::node::create_node_profile(
        did.clone(),
        did.clone(), // For now, operator is same as node DID
        &config.topology,
    );
    let node_profile_handle = Arc::new(RwLock::new(node_profile.clone()));

    info!(
        "Node profile created: {} roles detected ({:?}), stage={:?}, capacity={}MB RAM / {}MB storage",
        node_profile.roles.len(),
        node_profile.roles_sorted(),
        node_profile.stage,
        node_profile.capacity.memory_mb_available,
        node_profile.capacity.storage_mb_available,
    );

    // Initialize trust graph, recovery store, and misbehavior detector
    let trust_services = super::init_trust::init_trust_services(config, did.clone()).await?;
    let trust_graph_handle = trust_services.trust_graph.clone();
    let misbehavior_detector = trust_services.misbehavior_detector.clone();
    let recovery_store = trust_services.recovery_store.clone();
    let security_store = trust_services.security_store.clone();

    // Initialize snapshot coordinator
    let snapshot_coordinator = super::init_snapshot::init_snapshot_coordinator(did.clone()).await?;
    info!("Snapshot coordinator initialized");

    // Create trust lookup closure for gossip actor
    let trust_lookup = super::init_trust::create_trust_lookup(trust_graph_handle.clone());

    // Initialize gossip services
    let gossip_services = super::init_gossip::init_gossip_services(
        config,
        did.clone(),
        super::init_gossip::GossipDeps {
            trust_graph: trust_graph_handle.clone(),
            trust_lookup,
            misbehavior_detector: misbehavior_detector.clone(),
            identity_bundle: identity_bundle.clone(),
        },
    )
    .await?;
    icn_obs::metrics::supervisor::actor_spawned_inc("gossip");
    icn_obs::metrics::supervisor::actor_active_set("gossip", true);
    let gossip_handle = gossip_services.gossip_handle.clone();
    let gossip_store = gossip_services.gossip_store.clone();
    let loaded_snapshot = gossip_services.loaded_snapshot;

    // Initialize ledger and contract services
    let ledger_services = super::init_ledger::init_ledger_services(
        config,
        did.clone(),
        super::init_ledger::LedgerDeps {
            gossip_handle: gossip_handle.clone(),
            misbehavior_detector: misbehavior_detector.clone(),
            trust_graph: trust_graph_handle.clone(),
        },
    )
    .await?;
    icn_obs::metrics::supervisor::actor_spawned_inc("ledger");
    icn_obs::metrics::supervisor::actor_active_set("ledger", true);
    let ledger_handle = ledger_services.ledger_handle.clone();
    let dispute_manager_handle = ledger_services.dispute_manager.clone();
    let treasury_manager_handle = ledger_services.treasury_manager.clone();
    let contract_runtime_handle = ledger_services.contract_runtime.clone();
    let contract_actor_handle = ledger_services.contract_actor.clone();
    let ledger_store = ledger_services.ledger_store.clone();

    // Initialize cooperative services
    let coop_services =
        super::init_coop::init_coop_services(config, gossip_handle.clone(), did.clone()).await?;
    icn_obs::metrics::supervisor::actor_spawned_inc("coop");
    icn_obs::metrics::supervisor::actor_active_set("coop", true);
    let coop_handle = coop_services.coop_handle.clone();
    let coop_store = coop_services.coop_store.clone();

    // Initialize community services (civic engine)
    let community_services =
        super::init_community::init_community_services(config, gossip_handle.clone(), did.clone())
            .await?;
    icn_obs::metrics::supervisor::actor_spawned_inc("community");
    let community_store = community_services.community_store.clone();

    // Initialize entity services with gossip synchronization
    let entity_services =
        super::init_entity::init_entity_services(config, gossip_handle.clone(), did.clone())
            .await?;
    icn_obs::metrics::supervisor::actor_spawned_inc("entity");

    // Store entity handle for notification routing
    let entity_handle = entity_services.entity_handle.clone();

    // Store handles for gateway integration
    gateway_handles.coop = Some(coop_handle);
    gateway_handles.community = Some(community_services.community_handle);
    gateway_handles.trust_graph = Some(trust_graph_handle.clone());
    gateway_handles.entity = Some(entity_services.entity_handle);

    // Spawn Identity actor (provides signing and trust graph access)
    let identity_handle = crate::identity::IdentityActor::spawn(
        identity_bundle.keypair()?,
        trust_graph_handle.clone(),
        shutdown_tx.clone(),
    );

    info!("Identity actor spawned with DID: {}", identity_handle.did());

    // Spawn Network actor
    let network_handle = spawn_network_actor(
        config,
        identity_bundle,
        &did,
        &gossip_handle,
        &trust_graph_handle,
        &misbehavior_detector,
        shutdown_tx,
    )
    .await?;

    // Restore network state from snapshot if available
    if let Some(snapshot) = loaded_snapshot {
        if let Some(network_state) = snapshot.network_state {
            if let Err(e) = network_handle.restore_state(network_state).await {
                warn!("Failed to restore network state: {}", e);
            } else {
                info!("✅ Network state restored from snapshot");
            }
        }
    }

    // Create handle holders for late-bound actors
    let compute_handle_holder: Arc<RwLock<Option<icn_compute::ComputeHandle>>> =
        Arc::new(RwLock::new(None));
    let dispute_handle_holder: Arc<RwLock<Option<icn_ccl::DisputeActorHandle>>> =
        Arc::new(RwLock::new(None));
    let steward_handle_holder: Arc<RwLock<Option<icn_steward::StewardHandle>>> =
        Arc::new(RwLock::new(None));
    let contract_registry_holder: Arc<RwLock<Option<icn_ccl::ContractRegistryHandle>>> =
        Arc::new(RwLock::new(None));

    // Initialize federation services if enabled
    let federation_services = super::init_federation::init_federation_services(
        &config.federation,
        super::init_federation::FederationDeps {
            gossip_handle: gossip_handle.clone(),
            did: did.clone(),
            keypair: Arc::new(identity_bundle.keypair()?),
            store_path: config.store_path(),
        },
    )
    .await?;

    let (
        federation_registry_for_rpc,
        clearing_manager_for_governance,
        attestation_store_for_governance,
        federation_handler_for_notifications,
    ) = if let Some(ref services) = federation_services {
        gateway_handles.agreement_manager = Some(services.agreement_manager.clone());
        (
            Some(services.registry.clone()),
            Some(services.clearing_manager.clone()),
            Some(services.attestation_store.clone()),
            Some(services.federation_handler.clone()),
        )
    } else {
        gateway_handles.agreement_manager = None;
        (None, None, None, None)
    };

    // Initialize send callback with E2E encryption support
    let send_callback_services = super::init_send_callback::init_send_callback(
        super::init_send_callback::SendCallbackDeps {
            network_handle: network_handle.clone(),
            own_did: did.clone(),
            keypair: identity_bundle.keypair()?,
            x25519_secret_bytes: *identity_bundle.x25519_secret().as_bytes(),
            gossip_store: gossip_store.clone(),
            encryption_enabled: config.network.e2e_encryption_enabled,
            circuit_breaker_threshold: config.network.encryption_cleanup_circuit_breaker_threshold,
            shutdown_tx: shutdown_tx.clone(),
        },
        background_tasks,
    )
    .await?;

    // Configure gossip actor with callbacks and subscriptions
    configure_gossip_actor(
        &gossip_handle,
        send_callback_services.send_callback,
        &network_handle,
        &did,
        &trust_graph_handle,
        &contract_actor_handle,
        &recovery_store,
        &ledger_handle,
        &coop_store,
        &community_store,
        &snapshot_coordinator,
        &compute_handle_holder,
        &dispute_handle_holder,
        &node_profile_handle,
        &federation_handler_for_notifications,
        &contract_registry_holder,
        &entity_handle,
        config,
        shutdown_tx,
        background_tasks,
        federation_services.is_some(),
    )
    .await;

    info!("Gossip send callback configured");

    // Spawn storage challenge scheduler
    spawn_storage_challenge_scheduler(
        &did,
        identity_bundle,
        &gossip_store,
        &trust_graph_handle,
        &gossip_handle,
        &misbehavior_detector,
        shutdown_tx,
        background_tasks,
    );

    // Run network bootstrap
    let bootstrap_config = super::init_bootstrap::BootstrapConfig::from_configs(
        config.network.bootstrap_peers.clone(),
        &config.federation,
        &config.supervisor,
    );
    super::init_bootstrap::run_bootstrap(
        &bootstrap_config,
        &network_handle,
        &gossip_handle,
        &did,
        &node_profile,
    )
    .await;

    // Create event bus for inter-actor communication
    let event_bus = Arc::new(crate::events::EventBus::new());
    info!("Event bus created");

    // Initialize governance services
    let governance_services = super::init_governance::init_governance_services(
        config,
        did.clone(),
        super::init_governance::GovernanceDeps {
            gossip_handle: gossip_handle.clone(),
            event_bus: event_bus.clone(),
            shutdown_rx: shutdown_tx.subscribe(),
        },
    )
    .await?;

    let governance_handle = governance_services.governance_handle;
    let dead_letter_queue = governance_services.dead_letter_queue;
    let gov_store = governance_services.governance_store;
    let protocol_parameter_store = governance_services.protocol_parameter_store;

    // Store handles for gateway
    gateway_handles.governance = Some(Arc::new(governance_handle.clone()));
    gateway_handles.treasury = Some(treasury_manager_handle.clone());
    gateway_handles.ledger = Some(ledger_handle.clone());

    // Subscribe to governance events for ledger execution
    event_subscriptions.governance_event_subscription = Some({
        let treasury_did = config
            .cooperative
            .treasury_did
            .as_ref()
            .and_then(|s| {
                serde_json::from_value::<icn_identity::Did>(serde_json::Value::String(s.clone()))
                    .ok()
            })
            .unwrap_or_else(|| {
                debug!("No treasury_did configured, using node DID for budget payouts");
                did.clone()
            });

        let mut handler = super::governance_handlers::GovernanceEventHandler::new(
            ledger_handle.clone(),
            gov_store.clone(),
            dead_letter_queue.clone(),
            governance_handle.clone(),
            dispute_manager_handle.clone(),
            treasury_manager_handle.clone(),
            treasury_did,
        );

        if let (Some(registry), Some(clearing), Some(attestations)) = (
            federation_registry_for_rpc.clone(),
            clearing_manager_for_governance.clone(),
            attestation_store_for_governance.clone(),
        ) {
            handler = handler.with_federation(registry, clearing, attestations);
            info!("✓ Federation components wired to governance event handler");
        }

        event_bus
            .subscribe(super::governance_handlers::create_governance_subscription(
                handler,
            ))
            .await
    });

    info!("✓ Governance event handlers registered");

    // Initialize contract registry
    let contract_registry_services =
        super::init_contract_registry::init_contract_registry_services(
            config,
            did.clone(),
            super::init_contract_registry::ContractRegistryDeps {
                gossip_handle: gossip_handle.clone(),
            },
        )
        .await?;
    let contract_registry_handle = contract_registry_services.registry_handle;
    *contract_registry_holder.write().await = Some(contract_registry_handle.clone());
    info!("✓ Contract registry handle available for gossip routing");

    // Initialize compute actor
    let compute_services =
        super::init_compute::init_compute_services(super::init_compute::ComputeDeps {
            trust_graph: trust_graph_handle.clone(),
            ledger: ledger_handle.clone(),
            gossip_handle: gossip_handle.clone(),
            own_did: did.clone(),
            compute_handle_holder: compute_handle_holder.clone(),
            dispute_handle_holder: dispute_handle_holder.clone(),
            network_handle: network_handle.clone(),
            misbehavior_detector: misbehavior_detector.clone(),
            identity_bundle: identity_bundle.clone(),
            store_path: config.store_path(),
            contract_registry: Some(contract_registry_handle.clone()),
        })
        .await?;

    let compute_handle = compute_services.compute_handle;
    let broadcaster = compute_services.broadcaster;

    // Subscribe to policy governance events
    event_subscriptions.policy_governance_subscription = Some({
        let policy_handler = super::governance_handlers::PolicyEventHandler::new(
            compute_handle.clone(),
            gov_store.clone(),
        );
        event_bus
            .subscribe(super::governance_handlers::create_policy_subscription(
                policy_handler,
            ))
            .await
    });

    info!("✓ Policy governance integration active");

    // Spawn RPC server
    let rpc_config = super::init_rpc::RpcConfig::from_daemon_config(config);
    let rpc_compute_handle = super::init_rpc::spawn_rpc_server(
        rpc_config,
        super::init_rpc::RpcDeps {
            network_handle: network_handle.clone(),
            ledger_handle: ledger_handle.clone(),
            contract_runtime: contract_runtime_handle.clone(),
            gossip_handle: gossip_handle.clone(),
            governance_handle,
            compute_handle,
            trust_graph: trust_graph_handle.clone(),
            dispute_manager: dispute_manager_handle,
            federation_registry: federation_registry_for_rpc,
        },
        background_tasks,
    );
    gateway_handles.compute = Some(rpc_compute_handle);

    // Spawn background tasks
    spawn_background_tasks(
        config,
        &gossip_handle,
        &network_handle,
        &did,
        protocol_parameter_store,
        ledger_store,
        shutdown_tx,
        background_tasks,
    )
    .await;

    // Spawn steward actor if enabled
    let steward_services = super::init_steward::init_steward_services(
        &config.steward,
        super::init_steward::StewardDeps {
            gossip_handle: gossip_handle.clone(),
            steward_handle_holder: steward_handle_holder.clone(),
            did: did.clone(),
            keypair: identity_bundle.keypair()?,
            shutdown_tx: shutdown_tx.clone(),
        },
    )
    .await
    .ok();

    gateway_handles.steward = steward_services.and_then(|s| s.steward_handle);

    // Activate node profile
    {
        let mut profile = node_profile_handle.write().await;
        profile.activate();
        info!(
            "Node profile activated: {} ready to accept work",
            profile.node_did
        );
    }

    // Store broadcaster for gateway
    gateway_handles.event_broadcaster = Some(broadcaster);

    // Store shutdown handles
    shutdown_handles.misbehavior_detector = Some(misbehavior_detector);
    shutdown_handles.security_store = Some(security_store);

    Ok(CoreActorHandles {
        network: Some(network_handle),
        gossip: Some(gossip_handle),
        ledger: Some(ledger_handle),
    })
}

/// Spawn actors when no identity bundle is available
async fn spawn_without_identity(
    config: &Config,
    shutdown_tx: &ShutdownTx,
    background_tasks: &mut JoinSet<()>,
) -> CoreActorHandles {
    warn!("No identity bundle available - actors not spawned");
    warn!("Run 'icnctl id init' to create an identity");
    icn_obs::metrics::supervisor::error_inc("identity_bundle_missing");

    // Still spawn metrics update task for system metrics
    let start_time = std::time::Instant::now();
    let metrics_interval =
        std::time::Duration::from_secs(config.supervisor.metrics_update_interval_secs);
    let mut metrics_shutdown = shutdown_tx.subscribe();
    background_tasks.spawn(async move {
        let mut interval = tokio::time::interval(metrics_interval);
        loop {
            tokio::select! {
                _ = interval.tick() => {
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

    CoreActorHandles::default()
}

/// Spawn network actor with all configuration
async fn spawn_network_actor(
    config: &Config,
    identity_bundle: &IdentityBundle,
    did: &icn_identity::Did,
    gossip_handle: &Arc<RwLock<icn_gossip::GossipActor>>,
    trust_graph_handle: &Arc<RwLock<icn_trust::TrustGraph>>,
    misbehavior_detector: &Arc<RwLock<icn_security::MisbehaviorDetector>>,
    shutdown_tx: &ShutdownTx,
) -> Result<icn_net::NetworkHandle> {
    let listen_addr: std::net::SocketAddr = config.network.listen_addr.parse()?;
    let federation_enabled = config.federation.enabled;

    // Create incoming message handler that routes to gossip
    let network_handle_for_handler = Arc::new(RwLock::new(None::<icn_net::NetworkHandle>));

    let incoming_handler =
        super::init_network::create_incoming_handler(super::init_network::MessageHandlerDeps {
            gossip_handle: gossip_handle.clone(),
            network_handle_holder: network_handle_for_handler.clone(),
            own_did: did.clone(),
            federation_enabled,
        });

    // Prepare rate limiting configuration
    let (trust_graph_for_rate_limit, trust_gated_config, fallback_config) =
        if config.rate_limiting.enabled {
            (
                Some(trust_graph_handle.clone()),
                Some(
                    config
                        .rate_limiting
                        .to_trust_gated_config(config.network.min_trust_threshold.value()),
                ),
                Some(config.rate_limiting.to_fallback_config()),
            )
        } else {
            (None, None, None)
        };

    info!(
        "Using identity bundle with DID-TLS binding: {}",
        identity_bundle.did()
    );

    // Parse STUN servers
    let stun_servers = if !config.network.stun_servers.is_empty() {
        let mut parsed_servers = Vec::new();
        for server_str in &config.network.stun_servers {
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

    let turn_config = config.network.turn_config();

    // Create store for replay protection persistence
    let network_store_path = config.store_path().join("network");
    let network_store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(&network_store_path)?);
    info!(
        "Network store opened at {} for replay protection persistence",
        network_store_path.display()
    );

    let network_handle = icn_net::NetworkActor::spawn(
        identity_bundle.clone(),
        listen_addr,
        shutdown_tx.clone(),
        Some(incoming_handler),
        trust_graph_for_rate_limit,
        trust_gated_config,
        fallback_config,
        Some(config.topology.clone()),
        stun_servers,
        turn_config,
        Some(misbehavior_detector.clone()),
        Some(network_store),
        None, // Personhood store for Sybil resistance
        None, // Anchor rate limit config
    )
    .await?;

    // Initialize network handle for incoming message handler
    *network_handle_for_handler.write().await = Some(network_handle.clone());

    icn_obs::metrics::supervisor::actor_spawned_inc("network");
    icn_obs::metrics::supervisor::actor_active_set("network", true);
    info!("Network actor spawned on {}", listen_addr);

    Ok(network_handle)
}

/// Configure gossip actor with all callbacks and subscriptions
#[allow(clippy::too_many_arguments)]
async fn configure_gossip_actor(
    gossip_handle: &Arc<RwLock<icn_gossip::GossipActor>>,
    send_callback: icn_gossip::SendMessageCallback,
    network_handle: &icn_net::NetworkHandle,
    did: &icn_identity::Did,
    trust_graph_handle: &Arc<RwLock<icn_trust::TrustGraph>>,
    contract_actor_handle: &Arc<RwLock<icn_ccl::ContractActor>>,
    recovery_store: &Arc<dyn icn_store::Store>,
    ledger_handle: &Arc<RwLock<icn_ledger::Ledger>>,
    coop_store: &Arc<icn_coop::CoopStore>,
    community_store: &Arc<icn_community::CommunityStore>,
    snapshot_coordinator: &Arc<RwLock<icn_snapshot::SnapshotCoordinator>>,
    compute_handle_holder: &Arc<RwLock<Option<icn_compute::ComputeHandle>>>,
    dispute_handle_holder: &Arc<RwLock<Option<icn_ccl::DisputeActorHandle>>>,
    node_profile_handle: &Arc<RwLock<crate::node::NodeProfile>>,
    federation_handler: &Option<Arc<icn_federation::FederationGossipHandler>>,
    contract_registry_holder: &Arc<RwLock<Option<icn_ccl::ContractRegistryHandle>>>,
    entity_handle: &icn_entity::EntityHandle,
    config: &Config,
    shutdown_tx: &ShutdownTx,
    background_tasks: &mut JoinSet<()>,
    federation_enabled: bool,
) {
    let mut gossip = gossip_handle.write().await;
    gossip.set_send_callback(send_callback);

    // Create candidate cache for NAT traversal
    let candidate_cache = Arc::new(icn_net::CandidateCache::new());
    let candidate_cache_for_cleanup = candidate_cache.clone();

    // Create profile cache for peer capability discovery
    let profile_cache: Arc<
        RwLock<std::collections::HashMap<icn_identity::Did, crate::node::NodeProfile>>,
    > = Arc::new(RwLock::new(std::collections::HashMap::new()));

    // Create rate limiter for trust attestation anti-flood protection
    let attestation_rate_limiter =
        Arc::new(crate::trust_propagation::AttestationRateLimiter::new());

    // Create evidence validator
    let evidence_validator = Some(Arc::new(icn_trust::EvidenceValidator::new(
        recovery_store.clone(),
    )));

    // Create notification callback
    let notification_callback = super::init_notifications::create_notification_callback(
        super::init_notifications::NotificationDeps {
            trust_graph: trust_graph_handle.clone(),
            own_did: did.clone(),
            contract_actor: contract_actor_handle.clone(),
            recovery_store: recovery_store.clone(),
            ledger: ledger_handle.clone(),
            snapshot_coordinator: snapshot_coordinator.clone(),
            network_handle: network_handle.clone(),
            candidate_cache: candidate_cache.clone(),
            gossip_handle: gossip_handle.clone(),
            compute_handle: compute_handle_holder.clone(),
            dispute_handle: dispute_handle_holder.clone(),
            node_profile: node_profile_handle.clone(),
            profile_cache: profile_cache.clone(),
            coop_store: coop_store.clone(),
            community_store: community_store.clone(),
            federation_handler: federation_handler.clone(),
            attestation_rate_limiter,
            contract_registry: contract_registry_holder.clone(),
            nat_dial_config: config.network.nat_dial.clone(),
            evidence_validator,
            entity_handle: Some(entity_handle.clone()), // Pass entity handle for gossip sync
        },
    );

    gossip.set_notification_callback(notification_callback);

    // Set up peer sampling callback
    let network_handle_for_sampling = network_handle.clone();
    let peer_sampling_callback: icn_gossip::PeerSamplingCallback = Arc::new(move |scope, count| {
        let net_handle = network_handle_for_sampling.clone();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current()
                .block_on(async move { net_handle.sample_peers(scope, count).await })
        })
    });

    gossip.set_peer_sampling(peer_sampling_callback);

    // Subscribe to standard gossip topics
    super::init_gossip::subscribe_standard_topics(
        &mut gossip,
        did,
        super::init_gossip::TopicSubscriptionConfig { federation_enabled },
    )
    .await;

    // Spawn federation announcement task if enabled
    if let Some(ref handler) = federation_handler {
        super::init_federation::spawn_federation_announcement_task(
            handler.clone(),
            shutdown_tx.subscribe(),
            background_tasks,
        );
    }

    // Spawn candidate cache cleanup task
    let cache_cleanup_interval =
        std::time::Duration::from_secs(config.supervisor.candidate_cleanup_interval_secs);
    let mut cache_cleanup_shutdown = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(cache_cleanup_interval);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let removed = candidate_cache_for_cleanup.cleanup_expired().await;
                    if removed > 0 {
                        info!("Candidate cache cleanup removed {} stale entries", removed);
                    }
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

/// Spawn storage challenge scheduler
#[allow(clippy::too_many_arguments)]
fn spawn_storage_challenge_scheduler(
    did: &icn_identity::Did,
    identity_bundle: &IdentityBundle,
    gossip_store: &Arc<dyn icn_store::Store>,
    trust_graph_handle: &Arc<RwLock<icn_trust::TrustGraph>>,
    gossip_handle: &Arc<RwLock<icn_gossip::GossipActor>>,
    misbehavior_detector: &Arc<RwLock<icn_security::MisbehaviorDetector>>,
    shutdown_tx: &ShutdownTx,
    _background_tasks: &mut JoinSet<()>,
) {
    let keypair = match identity_bundle.keypair() {
        Ok(keypair) => keypair,
        Err(err) => {
            warn!("Failed to obtain keypair for storage challenge scheduler: {err}");
            return;
        }
    };
    let challenge_scheduler_handle = crate::storage_challenge::ChallengeScheduler::spawn(
        did.clone(),
        Arc::new(keypair),
        icn_store::ChallengeConfig::default(),
        gossip_store.clone(),
        trust_graph_handle.clone(),
        gossip_handle.clone(),
        misbehavior_detector.clone(),
        shutdown_tx.subscribe(),
    );

    // Wire up proof callback
    {
        let proof_callback = super::background_tasks::storage_challenge::create_proof_callback(
            challenge_scheduler_handle,
        );
        let mut gossip = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(gossip_handle.write())
        });
        gossip.set_storage_proof_callback(proof_callback);
    }

    info!("Storage challenge scheduler spawned (proof-of-storage verification active)");
}

/// Spawn all background maintenance tasks
#[allow(clippy::too_many_arguments)]
async fn spawn_background_tasks(
    config: &Config,
    gossip_handle: &Arc<RwLock<icn_gossip::GossipActor>>,
    network_handle: &icn_net::NetworkHandle,
    did: &icn_identity::Did,
    protocol_parameter_store: Arc<dyn icn_governance::ProtocolParameterStore>,
    ledger_store: Arc<icn_store::SledStore>,
    shutdown_tx: &ShutdownTx,
    _background_tasks: &mut JoinSet<()>,
) {
    // Anti-entropy task
    let anti_entropy_config = crate::anti_entropy::AntiEntropyConfig::default();
    let _anti_entropy_handle = crate::anti_entropy::spawn_anti_entropy_task(
        gossip_handle.clone(),
        network_handle.clone(),
        did.clone(),
        anti_entropy_config,
        shutdown_tx.subscribe(),
    );
    info!("Anti-entropy task spawned");

    // Digest emitter
    let _digest_emitter_handle = icn_gossip::start_digest_emitter(
        gossip_handle.clone(),
        10_000,
        2_000,
        shutdown_tx.subscribe(),
    );
    info!("Digest emitter spawned");

    // Partition checker
    let _partition_checker_handle =
        icn_gossip::start_partition_checker(gossip_handle.clone(), 30_000, shutdown_tx.subscribe());
    info!("Partition checker spawned");

    // Clock sync
    let _clock_sync_handle = super::background_tasks::spawn_clock_sync_task(
        super::background_tasks::ClockSyncConfig::default(),
        shutdown_tx.subscribe(),
    );
    info!("Clock sync background task spawned (interval: 10 minutes)");

    // Metrics update
    let _metrics_handle = super::background_tasks::spawn_metrics_update_task(
        super::background_tasks::MetricsUpdateConfig::default(),
        network_handle.clone(),
        did.clone(),
        std::time::Instant::now(),
        shutdown_tx.subscribe(),
    );
    info!("Metrics update task spawned");

    // Parameter scheduler
    let _parameter_scheduler_handle = super::background_tasks::spawn_parameter_scheduler_task(
        super::background_tasks::ParameterSchedulerConfig::default(),
        protocol_parameter_store,
        shutdown_tx.subscribe(),
    );
    info!("Parameter scheduler task spawned (interval: 10 seconds)");

    // Storage maintenance
    if config.supervisor.storage_maintenance.enabled {
        let _maintenance_handle = super::background_tasks::spawn_storage_maintenance_task(
            config.supervisor.storage_maintenance.clone(),
            ledger_store.clone(),
            shutdown_tx.subscribe(),
        );
        info!(
            "Storage maintenance task spawned (interval: {} seconds)",
            config.supervisor.storage_maintenance.interval_secs
        );
    } else {
        info!("Storage maintenance disabled by configuration");
    }

    // Candidate announcement
    let candidate_announce_config = super::background_tasks::CandidateAnnouncementConfig {
        announce_interval: std::time::Duration::from_secs(
            config.network.nat_dial.candidate_announce_interval_secs,
        ),
    };
    let _candidate_announce_handle = super::background_tasks::spawn_candidate_announcement_task(
        candidate_announce_config,
        network_handle.clone(),
        gossip_handle.clone(),
        shutdown_tx.subscribe(),
    );
    info!(
        "Connection candidate announcement task spawned (interval: {} seconds)",
        config.network.nat_dial.candidate_announce_interval_secs
    );

    // Resource access enforcer
    if config.supervisor.resource_enforcer.enabled {
        // Wrap NullResourceAccessStore with gossip integration
        // In the future, this should be replaced with a real persistent backend
        // (see TODO comment below about SledResourceAccessStore)
        let inner_store = Box::new(super::init_resource_enforcer::NullResourceAccessStore);
        let gossip_store = super::init_resource_enforcer::GossipResourceAccessStore::new(
            inner_store,
            gossip_handle.clone(),
        );
        let store = Arc::new(RwLock::new(gossip_store));

        // Subscribe to revocations topic to receive cluster-wide notifications
        {
            let mut gossip = gossip_handle.write().await;
            if let Err(e) = gossip
                .subscribe(
                    crate::resource_enforcer_actor::RESOURCE_REVOCATIONS_TOPIC,
                    did.clone(),
                )
                .await
            {
                warn!("Failed to subscribe to resource:revocations topic: {}", e);
            } else {
                info!("Subscribed to resource:revocations topic");
            }
        }

        // The resource enforcer actor is fully autonomous and only needs the shutdown
        // signal via the broadcast channel. We intentionally discard the handle for now;
        // future work may expose it via the Gateway API for manual checks or statistics
        // queries if needed.
        if let Ok(_enforcer_handle) = super::init_resource_enforcer::spawn_resource_enforcer(
            &config.supervisor.resource_enforcer,
            store,
            shutdown_tx,
        ) {
            info!(
                "Resource access enforcer task spawned (interval: {} seconds, gossip: enabled)",
                config.supervisor.resource_enforcer.check_interval_seconds
            );
        } else {
            warn!("Failed to spawn resource access enforcer");
        }

        // TODO: Replace `NullResourceAccessStore` with a real persistent backend.
        //
        // Integration plan:
        // 1. Storage backend:
        //    - Use the sled-backed SledResourceAccessStore from `icn-ledger` as the
        //      concrete implementation for `ResourceAccessStore`.
        // 2. Persistence layout:
        //    - Resource access entries are persisted in the ledger store using the
        //      `ledger:resource_access:` key prefix (see icn-ledger/src/use_access.rs).
        // 3. The GossipResourceAccessStore wrapper will handle cluster-wide notification
        //    when the real backend is integrated.
    } else {
        info!("Resource access enforcer disabled by configuration");
    }
}
