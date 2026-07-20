//! Gateway API server initialization
//!
//! Spawns the REST + WebSocket gateway server in a dedicated thread.
//! The gateway provides HTTP API access to ICN functionality for
//! cooperative applications.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::config::GatewayConfig;

/// Handles that can be optionally provided to the gateway
#[derive(Default)]
pub struct GatewayHandles {
    /// Event broadcaster for WebSocket delivery
    pub event_broadcaster: Option<Arc<icn_gateway::EventBroadcaster>>,
    /// Compute handle for task execution
    pub compute: Option<icn_compute::ComputeHandle>,
    /// Cooperative handle for coop management
    pub coop: Option<icn_coop::CoopHandle>,
    /// Community handle for civic features
    pub community: Option<icn_community::CommunityHandle>,
    /// Trust service for trust queries (kernel/app separated)
    pub trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    /// Ledger service for treasury nonce queries
    pub ledger_service: Option<Arc<dyn icn_kernel_api::services::LedgerService>>,
    /// Governance handle for governance operations
    pub governance: Option<icn_gateway::governance_mgr::GovernanceHandle>,
    /// Concrete actor-backed governance handle (parallel to the trait-object
    /// `governance` handle above). Needed so the gateway can install its
    /// receipt_store on the actor via `install_receipt_store`, closing the
    /// actor-path parity gap for force-close accept and deadline auto-close
    /// (which would otherwise emit no `InstitutionalEffectRecord`).
    ///
    /// The trait-object `governance` alias (`Arc<dyn GovernanceOps>`) does
    /// not expose the setter because doing so would pull
    /// `GovernanceReceiptBackend` (app-layer) into the kernel trait.
    pub governance_actor_handle: Option<icn_governance_actor::GovernanceHandle>,
    /// Treasury handle for treasury operations
    pub treasury: Option<icn_gateway::TreasuryHandle>,
    /// Ledger handle for balance queries
    pub ledger: Option<icn_gateway::LedgerHandle>,
    /// Entity handle for entity management
    pub entity: Option<icn_entity::EntityHandle>,
    /// Canonical, provenance-aware coop_id↔EntityId name-binding store (#2082/#2190).
    /// When present, the gateway builds a trusted, fail-closed
    /// `StoreBackedCoopEntityResolver` for observe-mode treasury classification (A2c).
    pub coop_entity_map: Option<icn_coop::CoopEntityMapHandle>,
    /// Steward handle for SDIS ceremonies
    pub steward: Option<icn_steward::StewardHandle>,
    /// Agreement manager for inter-cooperative agreements
    pub agreement_manager: Option<icn_federation::agreement::AgreementManagerHandle>,
    /// Service discovery manager with gossip wiring
    pub service_discovery_manager:
        Option<Arc<icn_gateway::service_discovery_mgr::ServiceDiscoveryManager>>,
    /// Naming service for Flow B name resolution endpoints
    pub naming_service: Option<Arc<dyn icn_kernel_api::naming::NamingService>>,
    /// Commons handle for substrate state (anchors, holders, charters, stewards, amendments, etc.)
    /// When provided, the gateway's CommonsManager uses this shared handle instead of
    /// opening its own sled-backed store, preventing dual ownership.
    pub commons: Option<icn_commons::CommonsHandle>,
    /// Hook invoked when a Charter proposal is accepted.
    pub charter_accepted_hook: Option<Arc<dyn Fn(String, String) + Send + Sync>>,
    /// Federation service for clearing position queries and settlement.
    /// When provided, the gateway uses this service-owned clearing state rather
    /// than its own divergent ClearingManager instance.
    pub federation_service: Option<Arc<dyn icn_kernel_api::FederationService>>,
    /// Settlement engine for compute audit queries (task_id → settled status).
    pub settlement_engine: Option<Arc<dyn icn_kernel_api::services::SettlementQueryService>>,
    /// Deferred dispatch-evidence sink: the gateway calls
    /// `install_backend(receipt_store)` on this once it opens its
    /// `ReceiptStore`, activating durable actor-path dispatch evidence.
    pub dispatch_evidence_sink_installer:
        Option<Arc<icn_governance_actor::DeferredDispatchEvidenceSink>>,
    /// Runtime-owned execution-record store the decision executor writes to.
    /// Shared into the gateway receipt-chain read API so audit reads see real
    /// execution records instead of an empty temp-store fallback (Gap C).
    pub execution_query_store: Option<Arc<dyn icn_store::Store>>,
    /// Persistent store backing session revocation (issue #2437).
    ///
    /// Shared with the RPC auth manager so a credential revoked on one
    /// authenticated surface is rejected on the other. Its presence is what
    /// lets the gateway assemble the institutional authority profile.
    pub revocation_store: Option<Arc<dyn icn_store::Store>>,
    /// Execution-record retention cleanup deferred to run after the gateway's
    /// dispatch-evidence backfill (Issue #1987 follow-up), so pruning cannot
    /// delete a terminal record whose evidence the backfill still needs to heal.
    pub post_backfill_cleanup: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Spawn the Gateway API server if enabled
///
/// The gateway runs in a dedicated thread with its own tokio runtime
/// because actix-web has specific runtime requirements.
///
/// Returns true only after the gateway has initialized and bound its socket;
/// returns false if disabled or if startup fails before readiness.
pub async fn spawn_gateway(
    config: &GatewayConfig,
    data_dir: PathBuf,
    handles: GatewayHandles,
) -> bool {
    if !config.enabled {
        debug!("Gateway API disabled in configuration");
        return false;
    }

    info!(
        "Gateway spawn check - enabled: true, jwt_secret length: {}",
        config.jwt_secret.len()
    );

    let gateway_addr: SocketAddr = match config.bind_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
            warn!("Failed to parse gateway bind address: {}", e);
            icn_obs::metrics::supervisor::error_inc("gateway_bind_addr_invalid");
            return false;
        }
    };

    // Check that JWT secret is configured
    if config.jwt_secret.is_empty() {
        warn!("Gateway enabled but JWT secret not configured - gateway will not start");
        warn!("Set jwt_secret in config or ICN_GATEWAY_JWT_SECRET environment variable");
        icn_obs::metrics::supervisor::error_inc("gateway_jwt_secret_missing");
        icn_obs::metrics::supervisor::actor_active_set("gateway", false);
        return false;
    }

    // Validate security-sensitive policy before creating a thread. The server
    // receives the already-validated value, so invalid configuration cannot
    // race ahead of the supervisor's startup report.
    let token_lifetime = match icn_gateway::session_authority::TokenLifetimePolicy::from_hours(
        config.token_expiry_hours,
    ) {
        Ok(lifetime) => lifetime,
        Err(error) => {
            warn!(
                "Gateway not started: invalid token_expiry_hours — {}",
                error
            );
            icn_obs::metrics::supervisor::error_inc("gateway_token_lifetime_invalid");
            icn_obs::metrics::supervisor::actor_active_set("gateway", false);
            return false;
        }
    };

    info!("Gateway JWT secret verified, spawning server...");

    // Derive the HMAC signing key through the ONE canonical function shared with
    // the `icnctl --local-mint` trusted-local issuer, so the daemon and any
    // co-issuer can never diverge on how the secret string becomes key bytes
    // (raw UTF-8, never hex/base64-decoded). See `icn_gateway::auth`.
    let jwt_secret = icn_gateway::auth::signing_key_bytes(&config.jwt_secret);

    // Get event broadcaster if available
    let broadcaster_for_gateway = if let Some(ref broadcaster) = handles.event_broadcaster {
        info!("Using shared EventBroadcaster for real-time WebSocket delivery");
        Some(broadcaster.clone())
    } else {
        None
    };

    // Extract handles for move into thread
    let compute_handle = handles.compute;
    let coop_handle = handles.coop;
    let community_handle = handles.community;
    let trust_service_handle = handles.trust_service;
    let ledger_service_handle = handles.ledger_service;
    let governance_handle = handles.governance;
    let governance_actor_handle = handles.governance_actor_handle;
    let treasury_handle = handles.treasury;
    let ledger_handle = handles.ledger;
    let entity_handle = handles.entity;
    let coop_entity_map_handle = handles.coop_entity_map;
    let steward_handle = handles.steward;
    let agreement_manager_handle = handles.agreement_manager;
    let service_discovery_manager = handles.service_discovery_manager;
    let naming_service = handles.naming_service;
    let commons_handle = handles.commons;
    let charter_accepted_hook = handles.charter_accepted_hook;
    let federation_service_handle = handles.federation_service;
    let settlement_engine_handle = handles.settlement_engine;
    let dispatch_evidence_sink_installer = handles.dispatch_evidence_sink_installer;
    let execution_query_store = handles.execution_query_store;
    let revocation_store = handles.revocation_store;
    let post_backfill_cleanup = handles.post_backfill_cleanup;
    let default_trust_score = config.default_trust_score;
    // Dev/demo posture (issue #2075): carried by config (set for the loopback
    // `--insecure-gateway-no-jwt` hatch) instead of an env var, so it does not
    // race threads reading the environment after the runtime has started.
    let dev_self_serve_auth = config.dev_self_serve_auth;

    // Spawn gateway in a dedicated thread (actix-web has its own runtime). The
    // supervisor waits for an explicit post-bind acknowledgement before marking
    // the actor active.
    let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
    let (activation_tx, activation_rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                warn!("Failed to create gateway async runtime: {}", error);
                icn_obs::metrics::supervisor::error_inc("gateway_server");
                return;
            }
        };
        rt.block_on(async move {
            let mut gateway_server = if let Some(broadcaster) = broadcaster_for_gateway {
                icn_gateway::GatewayServer::new_with_broadcaster(
                    gateway_addr,
                    jwt_secret,
                    Some(data_dir),
                    broadcaster,
                )
            } else {
                icn_gateway::GatewayServer::new(gateway_addr, jwt_secret)
            };

            // Dev/demo self-serve auth posture (issue #2075), race-free via config.
            gateway_server = gateway_server.with_dev_self_serve_auth(dev_self_serve_auth);

            // Connect optional handles
            if let Some(handle) = compute_handle {
                gateway_server = gateway_server.with_compute_handle(handle);
            }

            if let Some(handle) = coop_handle {
                gateway_server = gateway_server.with_coop_handle(handle);
            }

            if let Some(handle) = community_handle {
                gateway_server = gateway_server.with_community_handle(handle);
            }

            if let Some(service) = trust_service_handle {
                gateway_server = gateway_server.with_trust_service(service);
            }

            if let Some(service) = ledger_service_handle {
                gateway_server = gateway_server.with_ledger_service(service);
            }

            if let Some(handle) = governance_handle {
                gateway_server = gateway_server.with_governance_handle(handle);
            }

            if let Some(handle) = governance_actor_handle {
                gateway_server = gateway_server.with_governance_actor_handle(handle);
            }

            if let Some(handle) = treasury_handle {
                gateway_server = gateway_server.with_treasury_handle(handle);
            }

            if let Some(handle) = ledger_handle {
                gateway_server = gateway_server.with_ledger_handle(handle);
            }

            if let Some(handle) = entity_handle {
                gateway_server = gateway_server.with_entity_handle(handle);
            }

            if let Some(handle) = coop_entity_map_handle {
                gateway_server = gateway_server.with_coop_entity_map_handle(handle);
            }

            if let Some(handle) = steward_handle {
                gateway_server = gateway_server.with_steward_handle(handle);
            }

            if let Some(handle) = agreement_manager_handle {
                gateway_server = gateway_server.with_agreement_manager_handle(handle);
            }

            if let Some(mgr) = service_discovery_manager {
                gateway_server = gateway_server.with_service_discovery_manager(mgr);
            }

            if let Some(service) = naming_service {
                gateway_server = gateway_server.with_naming_service(service);
            }

            if let Some(handle) = commons_handle {
                gateway_server = gateway_server.with_commons_handle(handle);
            }

            if let Some(score) = default_trust_score {
                gateway_server = gateway_server.with_default_trust_score(score);
            }

            if let Some(hook) = charter_accepted_hook {
                gateway_server = gateway_server.with_charter_accepted_hook(hook);
            }

            if let Some(service) = federation_service_handle {
                gateway_server = gateway_server.with_federation_service(service);
            }

            if let Some(engine) = settlement_engine_handle {
                gateway_server = gateway_server.with_settlement_engine(engine);
            }

            if let Some(installer) = dispatch_evidence_sink_installer {
                gateway_server = gateway_server.with_dispatch_evidence_sink_installer(installer);
            }

            if let Some(store) = execution_query_store {
                gateway_server = gateway_server.with_execution_query_store(store);
            }

            // Session authority (issues #2436, #2437). A daemon deployment is an
            // operator-run node, so it declares the institutional profile: the
            // gateway then refuses to start unless durable revocation is
            // actually installed, rather than serving requests whose credentials
            // cannot be withdrawn across a restart.
            if let Some(store) = revocation_store {
                gateway_server = gateway_server
                    .with_revocation_store(store)
                    .with_authority_profile(
                        icn_gateway::session_authority::AuthorityProfile::Institutional,
                    );
            }
            gateway_server = gateway_server.with_token_lifetime(token_lifetime);

            if let Some(cleanup) = post_backfill_cleanup {
                gateway_server = gateway_server.with_post_backfill_cleanup(cleanup);
            }

            let result = gateway_server
                .run_with_startup_signal(startup_tx, activation_rx)
                .await;
            icn_obs::metrics::supervisor::actor_active_set("gateway", false);
            if let Err(e) = result {
                warn!("Gateway server error: {}", e);
                icn_obs::metrics::supervisor::error_inc("gateway_server");
            }
        });
    });

    match startup_rx.await {
        Ok(()) => {
            icn_obs::metrics::supervisor::actor_spawned_inc("gateway");
            icn_obs::metrics::supervisor::actor_active_set("gateway", true);
            if activation_tx.send(()).is_err() {
                icn_obs::metrics::supervisor::error_inc("gateway_activation_ack");
                icn_obs::metrics::supervisor::actor_active_set("gateway", false);
                warn!("Gateway exited before supervisor acknowledged activation");
                false
            } else {
                info!("Gateway API ready on {}", gateway_addr);
                true
            }
        }
        Err(error) => {
            icn_obs::metrics::supervisor::error_inc("gateway_startup_ack");
            icn_obs::metrics::supervisor::actor_active_set("gateway", false);
            warn!(
                "Gateway startup thread exited without reporting readiness: {}",
                error
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_token_lifetime_is_rejected_before_gateway_spawn() {
        let config = GatewayConfig {
            enabled: true,
            jwt_secret: "a".repeat(32),
            token_expiry_hours: 0,
            ..GatewayConfig::default()
        };
        let data_dir = tempfile::tempdir().expect("temporary gateway data directory");

        assert!(
            !spawn_gateway(
                &config,
                data_dir.path().to_path_buf(),
                GatewayHandles::default()
            )
            .await,
            "invalid authority policy must not report a ready gateway actor"
        );
    }
}
