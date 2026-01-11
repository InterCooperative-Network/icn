//! Supervisor for managing actors

pub mod background_tasks;
pub mod governance_handlers;
pub mod init_bootstrap;
pub mod init_community;
pub mod init_compute;
pub mod init_contract_registry;
pub mod init_coop;
pub mod init_entity;
pub mod init_federation;
pub mod init_gateway;
pub mod init_gossip;
pub mod init_governance;
pub mod init_ledger;
pub mod init_network;
pub mod init_notifications;
pub mod init_rpc;
pub mod init_send_callback;
pub mod init_snapshot;
pub mod init_steward;
pub mod init_trust;
pub mod nat_dial;
pub mod registry;
pub mod shutdown;
pub mod version_tracker;

use anyhow::{bail, Context, Result};
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;
use serde_json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::select;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::runtime::ShutdownTx;

/// Parse bootstrap peer URL in format: icn://did:icn:PUBKEY@HOST:PORT
/// Returns (DID, SocketAddr) on success
///
/// Supports both IP addresses and DNS hostnames. DNS resolution is performed
/// asynchronously when needed.
async fn parse_bootstrap_peer(url: &str) -> Result<(Did, SocketAddr)> {
    // Check for icn:// prefix
    let url = url
        .strip_prefix("icn://")
        .context("Bootstrap peer URL must start with 'icn://'")?;

    // Split on @ to get DID and address
    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        bail!("Invalid bootstrap peer format, expected icn://DID@HOST:PORT");
    }

    let did_str = parts[0];
    let addr_str = parts[1];

    // Parse DID
    let did: Did = serde_json::from_value(serde_json::Value::String(did_str.to_string()))
        .context("Failed to parse DID")?;

    // Try to parse as direct IP:PORT first (fast path)
    let addr = if let Ok(sock_addr) = addr_str.parse::<SocketAddr>() {
        sock_addr
    } else {
        // Fall back to DNS resolution for hostname:port
        resolve_address(addr_str).await?
    };

    Ok((did, addr))
}

/// Resolve a hostname:port string to a SocketAddr using DNS lookup.
///
/// # Arguments
/// * `addr_str` - Address string in format "hostname:port" or "ip:port"
///
/// # Returns
/// * `Ok(SocketAddr)` - The resolved socket address (first result if multiple)
/// * `Err` - If DNS resolution fails or no addresses are returned
async fn resolve_address(addr_str: &str) -> Result<SocketAddr> {
    use tokio::net::lookup_host;

    debug!("Resolving DNS for: {addr_str}");

    // Use tokio's async DNS resolution
    let mut addrs = lookup_host(addr_str)
        .await
        .with_context(|| format!("DNS resolution failed for '{addr_str}'"))?;

    // Take the first resolved address
    let addr = addrs
        .next()
        .with_context(|| format!("DNS resolution returned no addresses for '{addr_str}'"))?;

    debug!("Resolved {addr_str} -> {addr}");
    Ok(addr)
}

/// Try to encrypt an inner SignedEnvelope for E2E encryption (Issue #404)
///
/// Creates a sign-encrypt-sign structure:
/// 1. Serialize inner SignedEnvelope
/// 2. Encrypt with recipient's X25519 public key
/// 3. Wrap in outer SignedEnvelope with PayloadType::Encrypted
///
/// # Sequence Numbers
///
/// Three sequence spaces are involved, but only TWO are incremented per message:
///
/// 1. **Signing sequence** (`outer_sequence` parameter):
///    - Per-sender, shared across all recipients
///    - Used for BOTH inner and outer SignedEnvelope (same value)
///    - The inner envelope's sequence doesn't trigger replay check (handled by
///      `handle_signed_inner()`) because the outer envelope already provides protection
///
/// 2. **Encryption nonce** (from `enc_seq_tracker`):
///    - Per-(sender, recipient) pair, persistent across restarts
///    - Used only for ChaCha20-Poly1305 nonce derivation (not for replay protection)
///    - Must be unique per pair to prevent nonce reuse attacks
///
/// This design ensures encrypted messages consume ONE signing sequence (not two),
/// maintaining consistent sequence advancement between encrypted and unencrypted paths.
///
/// # Sequence Allocation Design
///
/// The encryption sequence is allocated AFTER all fallible I/O operations (peer key lookup,
/// serialization) but BEFORE the actual encryption. This is intentional:
///
/// 1. The sequence is required to derive the ChaCha20-Poly1305 nonce
/// 2. Encryption with valid inputs is essentially infallible (ChaCha20 is deterministic)
/// 3. The only remaining operations (bincode serialization, Ed25519 signing) are also
///    effectively infallible with valid inputs
///
/// This ordering minimizes sequence waste: sequences are only consumed when all I/O and
/// validation is complete, and the remaining crypto operations cannot practically fail.
async fn try_encrypt_envelope(
    net_handle: &icn_net::NetworkHandle,
    from_did: &Did,
    to_did: &Did,
    inner_envelope: &icn_net::SignedEnvelope,
    keypair: &icn_identity::KeyPair,
    x25519_secret: &x25519_dalek::StaticSecret,
    enc_seq_tracker: &icn_net::OutgoingSequenceTracker,
    outer_sequence: u64,
) -> Result<icn_net::SignedEnvelope> {
    // Phase 1: All fallible I/O operations BEFORE sequence allocation
    // This ensures sequence is only consumed when encryption is highly likely to succeed.

    // Get recipient's X25519 public key (can fail if peer disconnected)
    let recipient_x25519_bytes = net_handle
        .get_peer_x25519_key(to_did)
        .await
        .context("Peer X25519 key not available")?;
    let recipient_x25519_public = x25519_dalek::PublicKey::from(recipient_x25519_bytes);

    // Serialize inner envelope (validates it's serializable before committing sequence)
    let inner_bytes = icn_encoding::encode_bincode_legacy(inner_envelope)
        .context("Failed to serialize inner envelope")?;

    // Phase 2: Sequence allocation - point of no return
    // All operations after this are essentially infallible with valid inputs.
    // Get encryption sequence number (persistent, unique per sender-recipient pair)
    // This is separate from signing sequence - used only for nonce derivation
    let enc_sequence = enc_seq_tracker.next_sequence(from_did, to_did).await?;

    // Encrypt the inner envelope
    let encrypted = icn_net::EncryptedEnvelope::encrypt(
        from_did,
        to_did,
        enc_sequence,
        x25519_secret,
        &recipient_x25519_public,
        &inner_bytes,
    )
    .context("Failed to encrypt envelope")?;

    // Serialize encrypted envelope
    let encrypted_bytes = icn_encoding::encode_bincode_legacy(&encrypted)
        .context("Failed to serialize encrypted envelope")?;

    // Create outer signed envelope with PayloadType::Encrypted
    // Uses signing sequence (outer_sequence) for replay protection, NOT encryption sequence
    let outer_envelope = icn_net::SignedEnvelope::new(
        from_did,
        keypair,
        outer_sequence,
        icn_net::PayloadType::Encrypted,
        encrypted_bytes,
    )
    .context("Failed to create outer signed envelope")?;

    Ok(outer_envelope)
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

        // Community handle for gateway integration
        let community_handle_for_gateway: Option<icn_community::CommunityHandle>;

        // Trust graph handle for gateway integration
        let trust_graph_handle_for_gateway: Option<
            Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>,
        >;

        // Governance handle for gateway integration
        let governance_handle_for_gateway: Option<
            Arc<dyn icn_governance::GovernanceOps + Send + Sync>,
        >;

        // Treasury handle for gateway integration
        let treasury_handle_for_gateway: Option<icn_gateway::TreasuryHandle>;

        // Ledger handle for gateway balance queries
        let ledger_handle_for_gateway: Option<icn_gateway::LedgerHandle>;

        // Entity handle for gateway integration
        let entity_handle_for_gateway: Option<init_entity::EntityHandle>;

        // Steward handle for gateway integration
        let steward_handle_for_gateway: Option<icn_steward::StewardHandle>;

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
            icn_obs::metrics::supervisor::actor_active_set("gossip", true);
            let gossip_handle = gossip_services.gossip_handle.clone();
            let gossip_store = gossip_services.gossip_store.clone(); // Used for encryption sequence tracking
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
            icn_obs::metrics::supervisor::actor_active_set("ledger", true);
            let ledger_handle = ledger_services.ledger_handle.clone();
            let dispute_manager_handle = ledger_services.dispute_manager.clone();
            let treasury_manager_handle = ledger_services.treasury_manager.clone();
            let contract_runtime_handle = ledger_services.contract_runtime.clone();
            let contract_actor_handle = ledger_services.contract_actor.clone();

            // Initialize cooperative services
            let coop_services =
                init_coop::init_coop_services(&self.config, gossip_handle.clone(), did.clone())
                    .await?;
            icn_obs::metrics::supervisor::actor_spawned_inc("coop");
            icn_obs::metrics::supervisor::actor_active_set("coop", true);
            let coop_handle = coop_services.coop_handle.clone();
            let coop_store = coop_services.coop_store.clone(); // Used for gossip sync

            // Initialize community services (civic engine)
            let community_services = init_community::init_community_services(
                &self.config,
                gossip_handle.clone(),
                did.clone(),
            )
            .await?;
            icn_obs::metrics::supervisor::actor_spawned_inc("community");
            let community_store = community_services.community_store.clone();

            // Initialize entity services (persistent storage)
            let entity_services = init_entity::init_entity_services(&self.config)?;
            icn_obs::metrics::supervisor::actor_spawned_inc("entity");

            // Store handles for gateway integration (outside of identity_bundle scope)
            coop_handle_for_gateway = Some(coop_handle);
            community_handle_for_gateway = Some(community_services.community_handle);
            trust_graph_handle_for_gateway = Some(trust_graph_handle.clone());
            entity_handle_for_gateway = Some(entity_services.entity_handle);

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

            // Create store for replay protection persistence (security critical)
            let network_store_path = self.config.store_path().join("network");
            let network_store: Arc<dyn icn_store::Store> =
                Arc::new(SledStore::open(&network_store_path)?);
            info!(
                "Network store opened at {} for replay protection persistence",
                network_store_path.display()
            );

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
                Some(network_store),                // Persistent store for replay protection
            )
            .await?;

            // Initialize network handle for the incoming message handler
            *network_handle_for_handler.write().await = Some(network_handle.clone());

            icn_obs::metrics::supervisor::actor_spawned_inc("network");
            icn_obs::metrics::supervisor::actor_active_set("network", true);
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

            // Create contract registry handle holder (will be filled after ContractRegistryActor is spawned)
            let contract_registry_holder: Arc<
                tokio::sync::RwLock<Option<icn_ccl::ContractRegistryHandle>>,
            > = Arc::new(tokio::sync::RwLock::new(None));

            // Keep references to federation components for RPC server and governance handlers
            let federation_registry_for_rpc: Option<Arc<icn_federation::CooperativeRegistry>>;
            let clearing_manager_for_governance: Option<Arc<icn_federation::ClearingManager>>;
            let attestation_store_for_governance: Option<Arc<icn_federation::AttestationStore>>;

            // Initialize send callback with E2E encryption support
            let send_callback_services = init_send_callback::init_send_callback(
                init_send_callback::SendCallbackDeps {
                    network_handle: network_handle.clone(),
                    own_did: did.clone(),
                    keypair: identity_bundle.keypair().clone(),
                    x25519_secret_bytes: *identity_bundle.x25519_secret().as_bytes(),
                    gossip_store: gossip_store.clone(),
                    encryption_enabled: self.config.network.e2e_encryption_enabled,
                    circuit_breaker_threshold: self
                        .config
                        .network
                        .encryption_cleanup_circuit_breaker_threshold,
                    shutdown_tx: self.shutdown_tx.clone(),
                },
                &mut background_tasks,
            )
            .await?;

            // Set send callback on gossip actor
            {
                let mut gossip = gossip_handle.write().await;
                gossip.set_send_callback(send_callback_services.send_callback);

                // Set up notification callback for trust attestations, contract deployments, and recovery events
                let trust_graph_for_notifications = trust_graph_handle.clone();
                let own_did_for_notifications = did.clone();
                let contract_actor_for_notifications = contract_actor_handle.clone();
                let recovery_store_for_notifications = recovery_store.clone();
                let ledger_for_notifications = ledger_handle.clone();
                let coop_store_for_notifications = coop_store.clone();
                let community_store_for_notifications = community_store.clone();

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

                // Initialize federation services if enabled
                let federation_services = init_federation::init_federation_services(
                    &self.config.federation,
                    init_federation::FederationDeps {
                        gossip_handle: gossip_handle.clone(),
                        did: did.clone(),
                        store_path: self.config.store_path(),
                    },
                )
                .await?;

                // Extract federation components for use by other modules
                let federation_handler_for_notifications: Option<
                    Arc<icn_federation::FederationGossipHandler>,
                >;
                if let Some(ref services) = federation_services {
                    federation_registry_for_rpc = Some(services.registry.clone());
                    clearing_manager_for_governance = Some(services.clearing_manager.clone());
                    attestation_store_for_governance = Some(services.attestation_store.clone());
                    federation_handler_for_notifications =
                        Some(services.federation_handler.clone());
                } else {
                    federation_registry_for_rpc = None;
                    clearing_manager_for_governance = None;
                    attestation_store_for_governance = None;
                    federation_handler_for_notifications = None;
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
                        community_store: community_store_for_notifications,
                        federation_handler: federation_handler_for_notifications,
                        attestation_rate_limiter,
                        contract_registry: contract_registry_holder.clone(),
                        nat_dial_config: self.config.network.nat_dial.clone(),
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
                )
                .await;

                // Spawn periodic federation announcement task if enabled
                if let Some(ref handler) = federation_handler_for_announce {
                    init_federation::spawn_federation_announcement_task(
                        handler.clone(),
                        self.shutdown_tx.subscribe(),
                        &mut background_tasks,
                    );
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

            // Run network bootstrap: dial peers, announce candidate, publish profile
            let bootstrap_config = init_bootstrap::BootstrapConfig::from_configs(
                self.config.network.bootstrap_peers.clone(),
                &self.config.federation,
                &self.config.supervisor,
            );
            init_bootstrap::run_bootstrap(
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
            let protocol_parameter_store = governance_services.protocol_parameter_store;

            // Store governance handle for gateway integration
            governance_handle_for_gateway = Some(Arc::new(governance_handle.clone()));

            // Store treasury handle for gateway integration
            treasury_handle_for_gateway = Some(treasury_manager_handle.clone());

            // Store ledger handle for gateway balance queries
            ledger_handle_for_gateway = Some(ledger_handle.clone());

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

                let mut handler = governance_handlers::GovernanceEventHandler::new(
                    ledger_handle.clone(),
                    gov_store.clone(),
                    dead_letter_queue.clone(),
                    governance_handle.clone(),
                    dispute_manager_handle.clone(),
                    treasury_manager_handle.clone(),
                    treasury_did,
                );

                // Add federation components if federation is enabled (Issue #517)
                if let (Some(registry), Some(clearing), Some(attestations)) = (
                    federation_registry_for_rpc.clone(),
                    clearing_manager_for_governance.clone(),
                    attestation_store_for_governance.clone(),
                ) {
                    handler = handler.with_federation(registry, clearing, attestations);
                    info!("✓ Federation components wired to governance event handler");
                }

                event_bus
                    .subscribe(governance_handlers::create_governance_subscription(handler))
                    .await
            });

            info!("✓ Governance event handlers registered");

            // Initialize contract registry services
            let contract_registry_services =
                init_contract_registry::init_contract_registry_services(
                    &self.config,
                    did.clone(),
                    init_contract_registry::ContractRegistryDeps {
                        gossip_handle: gossip_handle.clone(),
                    },
                )
                .await?;
            let contract_registry_handle = contract_registry_services.registry_handle;

            // Fill the contract registry holder for notification routing
            *contract_registry_holder.write().await = Some(contract_registry_handle.clone());
            info!("✓ Contract registry handle available for gossip routing");

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
                contract_registry: Some(contract_registry_handle.clone()),
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

            // Spawn RPC server with all actor handles
            let rpc_config = init_rpc::RpcConfig::from_daemon_config(&self.config);
            let rpc_compute_handle = init_rpc::spawn_rpc_server(
                rpc_config,
                init_rpc::RpcDeps {
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
                &mut background_tasks,
            );
            compute_handle_for_gateway = Some(rpc_compute_handle);

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
            let steward_services = init_steward::init_steward_services(
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

            // Extract steward handle for gateway integration
            steward_handle_for_gateway = steward_services.and_then(|s| s.steward_handle);

            // Spawn metrics update task
            let _metrics_handle = background_tasks::spawn_metrics_update_task(
                background_tasks::MetricsUpdateConfig::default(),
                network_handle.clone(),
                did.clone(),
                std::time::Instant::now(),
                self.shutdown_tx.subscribe(),
            );

            info!("Metrics update task spawned");

            // Spawn parameter scheduler task for delayed execution (Phase 20)
            let _parameter_scheduler_handle = background_tasks::spawn_parameter_scheduler_task(
                background_tasks::ParameterSchedulerConfig::default(),
                protocol_parameter_store,
                self.shutdown_tx.subscribe(),
            );

            info!("Parameter scheduler task spawned (interval: 10 seconds)");

            // Spawn connection candidate re-announcement task (Phase M2)
            let candidate_announce_config = background_tasks::CandidateAnnouncementConfig {
                announce_interval: std::time::Duration::from_secs(
                    self.config
                        .network
                        .nat_dial
                        .candidate_announce_interval_secs,
                ),
            };
            let _candidate_announce_handle = background_tasks::spawn_candidate_announcement_task(
                candidate_announce_config,
                network_handle.clone(),
                gossip_handle.clone(),
                self.shutdown_tx.subscribe(),
            );
            info!(
                "Connection candidate announcement task spawned (interval: {} seconds)",
                self.config
                    .network
                    .nat_dial
                    .candidate_announce_interval_secs
            );

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

            // No event broadcaster or compute/trust/governance/treasury/ledger/entity/community/steward handles without identity
            event_broadcaster = None;
            compute_handle_for_gateway = None;
            coop_handle_for_gateway = None;
            community_handle_for_gateway = None;
            steward_handle_for_gateway = None;
            trust_graph_handle_for_gateway = None;
            governance_handle_for_gateway = None;
            treasury_handle_for_gateway = None;
            ledger_handle_for_gateway = None;
            entity_handle_for_gateway = None;

            (None, None, None)
        };

        // Spawn Gateway API server if enabled
        init_gateway::spawn_gateway(
            &self.config.gateway,
            self.config.data_dir.clone(),
            init_gateway::GatewayHandles {
                event_broadcaster,
                compute: compute_handle_for_gateway,
                coop: coop_handle_for_gateway,
                community: community_handle_for_gateway,
                trust_graph: trust_graph_handle_for_gateway,
                governance: governance_handle_for_gateway,
                treasury: treasury_handle_for_gateway,
                ledger: ledger_handle_for_gateway,
                entity: entity_handle_for_gateway,
                steward: steward_handle_for_gateway,
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

    #[tokio::test]
    async fn test_parse_bootstrap_peer_valid() {
        let url = format!("icn://{TEST_ALICE_DID}@203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), TEST_ALICE_DID);
        assert_eq!(addr.to_string(), "203.0.113.50:7777");
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_ipv4() {
        let url = format!("icn://{TEST_BOB_DID}@192.168.1.100:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), TEST_BOB_DID);
        assert_eq!(addr.to_string(), "192.168.1.100:7777");
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_missing_prefix() {
        let url = format!("{TEST_ALICE_DID}@203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with 'icn://'"));
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_missing_at() {
        let url = format!("icn://{TEST_ALICE_DID}_203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected icn://DID@HOST:PORT"));
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_dns_resolution() {
        // Test DNS resolution with localhost (should resolve)
        let url = format!("icn://{TEST_ALICE_DID}@localhost:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok(), "Failed to resolve localhost: {result:?}");

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), TEST_ALICE_DID);
        // localhost typically resolves to 127.0.0.1 or ::1
        assert!(
            addr.ip().is_loopback(),
            "Expected loopback address, got: {}",
            addr.ip()
        );
        assert_eq!(addr.port(), 7777);
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_dns_failure() {
        // Test DNS resolution failure with a guaranteed-invalid hostname (RFC 6761)
        let url = format!("icn://{TEST_ALICE_DID}@test.invalid:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("DNS resolution failed"));
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_ipv6() {
        // Test IPv6 address parsing (fast path, no DNS needed)
        let url = format!("icn://{TEST_ALICE_DID}@[::1]:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok(), "Failed to parse IPv6 address: {result:?}");

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), TEST_ALICE_DID);
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), 7777);
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_invalid_port() {
        let url = format!("icn://{TEST_ALICE_DID}@203.0.113.50:invalid");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_err());
        // With invalid port, DNS resolution will fail
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("DNS resolution failed"));
    }
}
