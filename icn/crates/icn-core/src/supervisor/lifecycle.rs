//! Supervisor lifecycle management
//!
//! This module contains the main supervisor startup and shutdown orchestration logic.
//! It coordinates actor initialization, background task spawning, and graceful shutdown.

use anyhow::Result;
use std::sync::Arc;
use tokio::select;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

use icn_identity::IdentityBundle;
use icn_kernel_api::services::ServiceRegistry;

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
///
/// # Service Registry
///
/// If `service_registry` is provided, services from it will be used instead of
/// creating internal implementations. This enables proper kernel/app separation
/// where the daemon constructs domain services from app crates.
pub async fn run_supervisor(
    config: Config,
    identity_bundle: Option<IdentityBundle>,
    shutdown_tx: ShutdownTx,
    service_registry: Option<ServiceRegistry>,
    bootstrap_handles: Option<super::BootstrapHandles>,
) -> Result<()> {
    info!("Supervisor starting");

    // Log service registry status
    if let Some(ref registry) = service_registry {
        info!("Using daemon-provided service registry");
        if registry.trust().is_some() {
            info!("  - TrustService: provided");
        }
        if registry.governance().is_some() {
            info!("  - GovernanceService: provided");
        }
        if registry.ledger().is_some() {
            info!("  - LedgerService: provided");
        }
        if registry.security().is_some() {
            info!("  - SecurityService: provided");
        }
    } else {
        debug!("No service registry provided, using internal implementations");
    }

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
            service_registry.as_ref(),
            bootstrap_handles,
        )
        .await?
    } else {
        spawn_without_identity(&config, &shutdown_tx, &mut background_tasks).await
    };

    // Initialize commons handle — open once here so gateway and any future
    // commons-aware actors all share a single sled-backed CommonsHandle.
    // This prevents the dual-ownership problem where gateway would open its
    // own sled store at the same path.
    let commons_handle: Option<icn_commons::CommonsHandle> = {
        let commons_path = config.data_dir.join("commons.sled");
        match icn_commons::CommonsHandle::with_sled_path(&commons_path) {
            Ok(handle) => {
                info!("CommonsHandle opened at {:?}", commons_path);
                Some(handle)
            }
            Err(e) => {
                warn!(
                    "CommonsHandle: failed to open sled store at {:?}: {}; \
                     gateway will fall back to in-memory commons state",
                    commons_path, e
                );
                None
            }
        }
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
            trust_service: gateway_handles.trust_service,
            ledger_service: gateway_handles.ledger_service,
            governance: gateway_handles.governance,
            treasury: gateway_handles.treasury,
            ledger: gateway_handles.ledger,
            entity: gateway_handles.entity,
            steward: gateway_handles.steward,
            agreement_manager: gateway_handles.agreement_manager,
            service_discovery_manager: gateway_handles.service_discovery_manager,
            naming_service: gateway_handles.naming_service,
            commons: commons_handle,
            charter_accepted_hook: gateway_handles.charter_accepted_hook,
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
    service_registry: Option<&ServiceRegistry>,
    bootstrap_handles: Option<super::BootstrapHandles>,
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

    // Initialize recovery store and misbehavior detector.
    // Extract TrustService early so it can be wired into the MisbehaviorDetector.
    let trust_service_from_registry = service_registry.and_then(|r| r.trust().cloned());

    let trust_services = icn_trust_app::init::init_trust_services(
        &config.store_path(),
        trust_service_from_registry.clone(),
    )
    .await?;
    let misbehavior_detector = trust_services.misbehavior_detector.clone();
    let recovery_store = trust_services.recovery_store.clone();
    let security_store = trust_services.security_store.clone();

    // Apply reputation policy config (severity weights, penalty rate, thresholds) from governance config.
    {
        let mut det = misbehavior_detector.write().await;
        det.set_severity_weights(config.security.reputation.to_severity_weights());
        det.set_penalty_rate(config.security.reputation.penalty_rate);
        det.set_max_violations_per_hour(config.security.reputation.max_violations_per_hour);
        det.set_violation_retention_secs(config.security.reputation.violation_retention_secs);
    }

    // Initialize snapshot coordinator
    let snapshot_coordinator = super::init_snapshot::init_snapshot_coordinator(did.clone()).await?;
    info!("Snapshot coordinator initialized");

    // Initialize OracleRegistry for centralized policy authorization.
    // The registry routes policy requests to domain-specific oracles and manages
    // bootstrap phases (Genesis → CoreApps → Running).
    let oracle_registry = Arc::new(icn_kernel_api::OracleRegistry::new());

    // Get TrustService from ServiceRegistry for ReplicationManager and oracle registration.
    let trust_service_from_registry = service_registry.and_then(|r| r.trust().cloned());

    // Register trust PolicyOracle with OracleRegistry if available.
    // This replaces the direct PolicyOracle passing and provides phase-aware authorization.
    if let Some(ref trust_service) = trust_service_from_registry {
        let trust_oracle = trust_service.oracle();
        let trust_domain = trust_oracle.domain();
        oracle_registry.register(trust_domain.clone(), trust_oracle);
        info!(
            "Registered TrustPolicyOracle with OracleRegistry for domain '{}'",
            trust_domain
        );
    }

    // Register charter oracle with OracleRegistry if available (Phase 1 charter engine).
    if let Some(charter_oracle) = service_registry.and_then(|r| r.charter_oracle().cloned()) {
        let charter_domain = charter_oracle.domain();
        oracle_registry.register(charter_domain.clone(), charter_oracle);
        info!(
            "Registered CharterPolicyOracle with OracleRegistry for domain '{}'",
            charter_domain
        );
    }

    // Transition to CoreApps phase: first-party oracles are registered.
    oracle_registry.set_phase(icn_kernel_api::bootstrap::BootstrapPhase::CoreApps);
    info!("OracleRegistry phase: CoreApps");

    // Use OracleRegistry as the PolicyOracle for all kernel components.
    // OracleRegistry implements PolicyOracle, so it can be passed wherever
    // a PolicyOracle is expected, providing domain routing and caching.
    let oracle_for_components: Arc<dyn icn_kernel_api::authz::PolicyOracle> =
        oracle_registry.clone();

    // Initialize gossip services with OracleRegistry as the policy oracle.
    let gossip_services = super::init_gossip::init_gossip_services(
        config,
        did.clone(),
        super::init_gossip::GossipDeps {
            trust_service: trust_service_from_registry.clone(),
            misbehavior_detector: misbehavior_detector.clone(),
            identity_bundle: identity_bundle.clone(),
            policy_oracle: Some(oracle_for_components.clone()),
        },
    )
    .await?;
    icn_obs::metrics::supervisor::actor_spawned_inc("gossip");
    icn_obs::metrics::supervisor::actor_active_set("gossip", true);
    let gossip_handle = gossip_services.gossip_handle.clone();
    let gossip_store = gossip_services.gossip_store.clone();
    let loaded_snapshot = gossip_services.loaded_snapshot;

    // Extract pre-initialized domain handles from BootstrapHandles.
    // These were created by icn_ledger_app::init::init_ledger_services() in the daemon.
    let handles = bootstrap_handles.ok_or_else(|| {
        anyhow::anyhow!(
            "BootstrapHandles not provided by daemon. When running with an identity, \
             the daemon must call Runtime::with_bootstrap_handles(...) to supply \
             ledger, ledger_store, dispute_manager, treasury_manager, contract_runtime, \
             contract_actor, and protocol_parameter_store handles. This requires a \
             successfully unlocked identity bundle."
        )
    })?;
    let ledger_handle = handles.ledger;
    let ledger_store = handles.ledger_store;
    let dispute_manager_handle = handles.dispute_manager;
    let treasury_manager_handle = handles.treasury_manager;
    let contract_runtime_handle = handles.contract_runtime;
    let contract_actor_handle = handles.contract_actor;
    let protocol_parameter_store_from_daemon = handles.protocol_parameter_store;
    let effect_subscription_factory = handles.effect_subscription_factory;
    gateway_handles.charter_accepted_hook = handles.charter_accepted_hook;

    // Wire runtime handles into the pre-initialized Ledger.
    // These depend on gossip/trust which are only available after gossip init.
    {
        let mut ledger = ledger_handle.write().await;
        ledger.set_gossip(gossip_handle.clone());
        ledger.set_misbehavior_detector(misbehavior_detector.clone());
        if let Some(trust_service) = trust_service_from_registry.clone() {
            ledger.set_trust_service(trust_service);
            info!("Ledger wired with TrustService");
        } else {
            info!("Ledger initialized without TrustService (trust validation disabled)");
        }
    }
    info!("Ledger runtime handles wired");
    icn_obs::metrics::supervisor::actor_spawned_inc("ledger");
    icn_obs::metrics::supervisor::actor_active_set("ledger", true);

    // Initialize cooperative services with treasury manager for ledger integration
    let coop_services = super::init_coop::init_coop_services_with_treasury(
        config,
        gossip_handle.clone(),
        did.clone(),
        Some(treasury_manager_handle.clone()),
    )
    .await?;
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

    // Initialize service discovery manager with gossip propagation (and optional sled persistence)
    let service_discovery_mgr =
        match icn_gateway::service_discovery_mgr::ServiceDiscoveryManager::with_gossip(
            gossip_handle.clone(),
            did.clone(),
        )
        .await
        {
            Ok(mut mgr) => {
                if config.gateway.service_discovery_persist {
                    let sd_path = config.store_path().join("service-discovery");
                    match sled::open(&sd_path) {
                        Ok(db) => {
                            if let Err(e) = mgr.with_persistence(&db).await {
                                warn!(
                                    "Failed to attach service discovery persistence: {}; \
                                     continuing without persistence",
                                    e
                                );
                            } else {
                                info!(
                                    "Service discovery persistence enabled at {}",
                                    sd_path.display()
                                );
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to open service discovery sled db at {}: {}; \
                                 continuing without persistence",
                                sd_path.display(),
                                e
                            );
                        }
                    }
                }
                let mgr = Arc::new(mgr);
                info!("Service discovery manager initialized with gossip wiring");
                Some(mgr)
            }
            Err(e) => {
                warn!(
                    "Failed to initialize service discovery manager with gossip: {}",
                    e
                );
                None
            }
        };

    // Store handles for gateway integration
    gateway_handles.coop = Some(coop_handle);
    gateway_handles.community = Some(community_services.community_handle);
    gateway_handles.trust_service = trust_service_from_registry.clone();
    gateway_handles.entity = Some(entity_services.entity_handle);
    gateway_handles.service_discovery_manager = service_discovery_mgr.clone();
    gateway_handles.naming_service = match super::init_naming::init_naming_service(config) {
        Ok(service) => Some(service),
        Err(error) => {
            // NOTE: When the supervisor-provided naming service fails to initialize,
            // the gateway falls back to opening its own local store at the same path.
            // This means the /names endpoint will serve from a potentially empty store
            // rather than returning 503. Operators should monitor for this log.
            error!("Failed to initialize naming service: {}", error);
            None
        }
    };

    // Spawn Identity actor (provides signing and trust service access)
    let identity_handle = crate::identity::IdentityActor::spawn(
        identity_bundle.keypair()?,
        trust_service_from_registry.clone(),
        shutdown_tx.clone(),
    );

    info!("Identity actor spawned with DID: {}", identity_handle.did());

    // Spawn Network actor with OracleRegistry for trust-based rate limiting
    let network_handle = spawn_network_actor(
        config,
        identity_bundle,
        &did,
        &gossip_handle,
        &misbehavior_detector,
        shutdown_tx,
        &oracle_for_components,
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
        _attestation_store_for_governance,
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
        &trust_service_from_registry,
        &service_discovery_mgr,
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
        trust_service_from_registry.clone(),
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

    // Protocol parameter store was extracted from BootstrapHandles above.

    // Extract signing key for GovernanceProof generation.
    // If keypair() returns Err (hardware-backed key or locked keystore), signing
    // falls back to None: proposals still close and allocate correctly (the
    // governance gate enforces Invariant 7 regardless), but proof attestations
    // won't be generated and /v1/gov/proposals/{id}/proof will return 404.
    let governance_signing_key = match identity_bundle.keypair() {
        Ok(kp) => {
            let bytes = kp.to_signing_key_bytes();
            info!("GovernanceProof signing enabled — proposals will produce cryptographic attestations");
            Some(Arc::new(ed25519_dalek::SigningKey::from_bytes(&bytes)))
        }
        Err(e) => {
            warn!(
                error = %e,
                "GovernanceProof signing key unavailable — proposals will close and execute \
                 without cryptographic attestations. \
                 Proof endpoint (/v1/gov/proposals/<id>/proof) will return 404. \
                 To enable: ensure keystore is software-backed and unlocked at startup."
            );
            None
        }
    };

    // Initialize governance services
    let governance_services = super::init_governance::init_governance_services(
        config,
        did.clone(),
        super::init_governance::GovernanceDeps {
            gossip_handle: gossip_handle.clone(),
            event_bus: event_bus.clone(),
            shutdown_rx: shutdown_tx.subscribe(),
            protocol_parameter_store: protocol_parameter_store_from_daemon,
            signing_key: governance_signing_key,
        },
    )
    .await?;

    let governance_handle = governance_services.governance_handle;
    let _dead_letter_queue = governance_services.dead_letter_queue;
    let _gov_store = governance_services.governance_store;
    let protocol_parameter_store = governance_services.protocol_parameter_store;

    // Store handles for gateway
    gateway_handles.governance = Some(Arc::new(governance_handle.clone()));
    gateway_handles.treasury = Some(treasury_manager_handle.clone());
    gateway_handles.ledger = Some(ledger_handle.clone());

    // Subscribe to governance events via the effect path
    // The effect path is now the default - legacy governance_handlers have been removed.
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

        // Create kernel executor with protocol parameter store
        let mut kernel_executor = super::governance_executor::KernelGovernanceExecutor::new(
            protocol_parameter_store.clone(),
        );

        // Wire ledger service adapter
        let oracle: Arc<dyn icn_kernel_api::authz::PolicyOracle> =
            Arc::new(icn_kernel_api::authz::AllowAllOracle::wildcard());
        let receipt_index_path = config.store_path().join("ledger-receipt-index");
        let receipt_index_store: Arc<icn_store::SledStore> =
            Arc::new(icn_store::SledStore::open(&receipt_index_path)?);
        let ledger_service: Arc<dyn icn_kernel_api::services::LedgerService> =
            Arc::new(crate::services::LedgerServiceImpl::new_with_receipt_index(
                ledger_handle.clone(),
                oracle,
                treasury_did.clone(),
                receipt_index_store,
            ));
        gateway_handles.ledger_service = Some(ledger_service.clone());
        kernel_executor = kernel_executor.with_ledger_service(ledger_service);

        // Wire federation service adapter if available
        if let Some(registry) = federation_registry_for_rpc.clone() {
            let mut federation_service = crate::services::FederationServiceImpl::new(registry);
            if let Some(clearing) = clearing_manager_for_governance.clone() {
                federation_service = federation_service.with_clearing_manager(clearing);
                info!("✓ Federation clearing manager wired to federation service");
            }
            kernel_executor = kernel_executor.with_federation_service(Arc::new(federation_service));
            info!("✓ Federation service wired to governance executor");
        }

        // Wire settlement ledger so SettleClearing effects can produce ledger entries.
        // Reuses the same LedgerService Arc already wired to the treasury executor;
        // the service is stateless (no mutable fields), so sharing is safe.
        if let Some(settlement_ledger) = gateway_handles.ledger_service.clone() {
            kernel_executor = kernel_executor.with_settlement_ledger(settlement_ledger);
            info!("✓ Settlement ledger wired to federation executor");
        }

        // Wire control service adapter
        let control_service = Arc::new(crate::services::ControlServiceImpl::new(
            governance_handle.clone(),
        ));
        kernel_executor = kernel_executor.with_control_service(control_service);

        // Wire membership service adapter
        let membership_service = Arc::new(crate::services::MembershipServiceImpl::new(
            coop_store.clone(),
        ));
        kernel_executor = kernel_executor.with_membership_service(membership_service);
        info!("✓ Membership service wired to governance executor");

        // Create escrow and budget stores via the ledger app crate
        let ledger_stores = icn_ledger_actor::init::create_stores(&config.store_path())?;
        let escrow_store = ledger_stores.escrow_store;
        kernel_executor = kernel_executor.with_escrow_store(escrow_store.clone());
        kernel_executor = kernel_executor.with_budget_store(ledger_stores.budget_store);
        info!("✓ Escrow and budget stores wired to governance executor");

        // Create effect dispatcher
        let effect_dispatcher = Arc::new(super::effect_dispatcher::EffectDispatcher::new(
            Arc::new(kernel_executor),
        ));

        // Create execution store for persistent idempotency tracking
        let exec_store_path = config.store_path().join("execution");
        let exec_sled_store: Arc<icn_store::SledStore> =
            Arc::new(icn_store::SledStore::open(&exec_store_path)?);
        let execution_store: Arc<dyn icn_kernel_api::execution::ExecutionStore> = Arc::new(
            super::execution_store::SledExecutionStore::new(exec_sled_store),
        );
        info!(
            "Execution store opened at {} for decision idempotency",
            exec_store_path.display()
        );

        // Wrap dispatcher with DecisionExecutor for idempotent execution
        let decision_executor = Arc::new(
            super::decision_executor::DecisionExecutor::new(effect_dispatcher, execution_store)
                .with_escrow_store(escrow_store),
        );

        // Recover in-flight decisions from prior crash
        // This must run BEFORE the event subscription is established
        // so we don't race with new incoming decisions.
        match decision_executor.recover_in_flight().await {
            Ok(report) => {
                if report.total() > 0 {
                    info!(
                        confirmed = report.recovered_confirmed,
                        not_executed = report.recovered_not_executed,
                        failed = report.recovered_failed,
                        skipped = report.skipped_no_effects + report.skipped_max_retries,
                        "Startup recovery: processed {} in-flight decisions",
                        report.total()
                    );
                } else {
                    debug!("Startup recovery: no in-flight decisions found");
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Startup recovery failed (non-fatal, new decisions will still process)"
                );
            }
        }

        // Best-effort startup retention cleanup (non-fatal).
        // Must run after recovery so pending/in-flight records are not pruned.
        match decision_executor.cleanup_old_records() {
            Ok(report) => {
                if report.deleted_old > 0 || report.deleted_excess > 0 {
                    info!(
                        deleted_old = report.deleted_old,
                        deleted_excess = report.deleted_excess,
                        "Startup cleanup pruned terminal execution records"
                    );
                } else {
                    debug!("Startup cleanup: no terminal execution records pruned");
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Startup cleanup failed (non-fatal, continuing startup)"
                );
            }
        }

        // Create callback that routes effects through DecisionExecutor
        let effect_callback =
            super::decision_executor::create_decision_executor_callback(decision_executor);

        // Create effect-based subscription via factory from BootstrapHandles
        // (avoids direct icn_governance_actor reference from lifecycle.rs)
        let effect_callback_arc: Arc<
            dyn Fn(Vec<icn_kernel_api::effects::KernelEffect>, String) + Send + Sync,
        > = Arc::new(move |effects, receipt_id| {
            effect_callback(effects, receipt_id);
        });
        let factory = effect_subscription_factory.ok_or_else(|| {
            anyhow::anyhow!(
                "BootstrapHandles must provide effect_subscription_factory for governance effect routing"
            )
        })?;
        let effect_subscription = factory(effect_callback_arc);

        // Subscribe and return handle for lifecycle tracking
        let effect_handle = event_bus.subscribe(effect_subscription).await;
        info!("✓ Effect-based governance path active");
        effect_handle
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
    let trust_service_for_compute = trust_service_from_registry
        .clone()
        .ok_or_else(|| anyhow::anyhow!("TrustService required for compute actor"))?;
    let compute_services =
        super::init_compute::init_compute_services(super::init_compute::ComputeDeps {
            trust_service: trust_service_for_compute,
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
            policy_config: config.compute.policy.clone(),
        })
        .await?;

    let compute_handle = compute_services.compute_handle;
    let broadcaster = compute_services.broadcaster;

    // Policy governance now routes through the effect path.
    // SchedulingPolicy proposals will be handled by ProtocolService when implemented.
    info!("✓ Compute integration active");

    // Spawn RPC server with OracleRegistry-backed trust-based rate limiting
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
            trust_service: trust_service_from_registry.clone(),
            dispute_manager: dispute_manager_handle,
            federation_registry: federation_registry_for_rpc,
            policy_oracle: Some(oracle_for_components.clone()),
        },
        background_tasks,
    );
    gateway_handles.compute = Some(rpc_compute_handle);

    // Extract LedgerService for background tasks (resource enforcer)
    let ledger_service_for_bg: Option<Arc<dyn icn_kernel_api::services::LedgerService>> =
        service_registry.and_then(|r| r.ledger().cloned());

    // Spawn background tasks
    spawn_background_tasks(
        config,
        &gossip_handle,
        &network_handle,
        &did,
        protocol_parameter_store,
        ledger_store,
        ledger_service_for_bg,
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

    // Transition OracleRegistry to Running phase.
    // All first-party apps are loaded and oracles registered.
    // From this point, unknown domains are denied by default (security).
    oracle_registry.set_phase(icn_kernel_api::bootstrap::BootstrapPhase::Running);
    info!(
        "OracleRegistry phase: Running (registered domains: {:?})",
        oracle_registry.domains()
    );

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
    misbehavior_detector: &Arc<RwLock<icn_security::MisbehaviorDetector>>,
    shutdown_tx: &ShutdownTx,
    policy_oracle: &Arc<dyn icn_kernel_api::authz::PolicyOracle>,
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

    // Prepare rate limiting configuration with OracleRegistry-backed PolicyOracle.
    // The oracle provides trust-based rate limits via the OracleRegistry.
    // Fallback config is still provided for cases where the oracle returns no constraints.
    let (oracle, fallback_config) = if config.rate_limiting.enabled {
        (
            Some(policy_oracle.clone()),
            Some(config.rate_limiting.to_fallback_config()),
        )
    } else {
        (None, None)
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

    // Parse optional explicit advertised address (for containerized deployments)
    let advertised_addr =
        config.network.advertised_addr.as_deref().and_then(|s| {
            match s.parse::<std::net::SocketAddr>() {
                Ok(addr) => {
                    info!("Using explicit advertised address: {}", addr);
                    if addr.port() != listen_addr.port() {
                        warn!(
                        "advertised_addr {} uses port {} but network.listen_addr {} uses port {}. \
                        Peers will dial a port this node may not be listening on — \
                        verify port-forwarding or proxy configuration.",
                        addr, addr.port(), listen_addr, listen_addr.port()
                    );
                    }
                    Some(addr)
                }
                Err(e) => {
                    warn!(
                        "Failed to parse advertised_addr '{}': {} — falling back to auto-detect",
                        s, e
                    );
                    None
                }
            }
        });

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
        oracle,
        fallback_config,
        Some(config.topology.clone()),
        stun_servers,
        turn_config,
        Some(misbehavior_detector.clone()),
        Some(network_store),
        None, // Personhood store for Sybil resistance
        None, // Anchor rate limit config
        advertised_addr,
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
    contract_actor_handle: &Arc<RwLock<icn_ccl::ContractActor>>,
    recovery_store: &Arc<dyn icn_store::Store>,
    ledger_handle: &super::actors::LedgerHandle,
    coop_store: &Arc<icn_coop::CoopStore>,
    community_store: &Arc<icn_community::CommunityStore>,
    snapshot_coordinator: &Arc<RwLock<icn_snapshot::SnapshotCoordinator>>,
    compute_handle_holder: &Arc<RwLock<Option<icn_compute::ComputeHandle>>>,
    dispute_handle_holder: &Arc<RwLock<Option<icn_ccl::DisputeActorHandle>>>,
    node_profile_handle: &Arc<RwLock<crate::node::NodeProfile>>,
    federation_handler: &Option<Arc<icn_federation::FederationGossipHandler>>,
    contract_registry_holder: &Arc<RwLock<Option<icn_ccl::ContractRegistryHandle>>>,
    entity_handle: &icn_entity::EntityHandle,
    trust_service: &Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    service_discovery_manager: &Option<
        Arc<icn_gateway::service_discovery_mgr::ServiceDiscoveryManager>,
    >,
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

    // Create notification callback
    let notification_callback = super::init_notifications::create_notification_callback(
        super::init_notifications::NotificationDeps {
            trust_service: trust_service.clone(),
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
            entity_handle: Some(entity_handle.clone()), // Pass entity handle for gossip sync
            service_discovery_manager: service_discovery_manager.clone(),
        },
    );

    gossip.add_notification_callback(notification_callback);

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
    trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
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
        trust_service,
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
    protocol_parameter_store: Arc<dyn icn_kernel_api::protocol_params::ProtocolParameterStore>,
    ledger_store: Arc<icn_store::SledStore>,
    ledger_service: Option<Arc<dyn icn_kernel_api::services::LedgerService>>,
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
        if let Some(ledger_svc) = ledger_service {
            // Create and subscribe to revocations topic for cluster-wide notifications
            {
                let mut gossip = gossip_handle.write().await;

                // Create the topic with explicit retention for audit trail.
                // - 7-day retention provides sufficient window for cluster synchronization
                // - 1000 max entries prevents unbounded growth while allowing burst activity
                gossip.create_topic(icn_gossip::types::Topic {
                    name: crate::resource_enforcer_actor::RESOURCE_REVOCATIONS_TOPIC.to_string(),
                    acl: icn_gossip::AccessControl::Public,
                    scope: icn_gossip::types::Scope::Global,
                    min_trust_threshold: None,
                    retention: std::time::Duration::from_secs(86400 * 7), // 7 days
                    max_entries: 1000,
                });

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

            let deps = crate::resource_enforcer_actor::ResourceEnforcerDeps {
                ledger_service: ledger_svc,
                gossip_handle: Some(gossip_handle.clone()),
            };

            // The resource enforcer actor is fully autonomous and only needs the shutdown
            // signal via the broadcast channel. We intentionally discard the handle for now;
            // future work may expose it via the Gateway API for manual checks or statistics
            // queries if needed.
            if let Ok(_enforcer_handle) = super::init_resource_enforcer::spawn_resource_enforcer(
                &config.supervisor.resource_enforcer,
                deps,
                shutdown_tx,
            ) {
                info!(
                    "Resource access enforcer task spawned (interval: {} seconds, gossip: enabled)",
                    config.supervisor.resource_enforcer.check_interval_seconds
                );
            } else {
                warn!("Failed to spawn resource access enforcer");
            }
        } else {
            info!("Resource access enforcer skipped: no LedgerService registered");
        }
    } else {
        info!("Resource access enforcer disabled by configuration");
    }
}
