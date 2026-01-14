//! Gateway API server initialization
//!
//! Spawns the REST + WebSocket gateway server in a dedicated thread.
//! The gateway provides HTTP API access to ICN functionality for
//! cooperative applications.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
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
    /// Trust graph for trust queries
    pub trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,
    /// Governance handle for governance operations
    pub governance: Option<Arc<dyn icn_governance::GovernanceOps + Send + Sync>>,
    /// Treasury handle for treasury operations
    pub treasury: Option<icn_gateway::TreasuryHandle>,
    /// Ledger handle for balance queries
    pub ledger: Option<icn_gateway::LedgerHandle>,
    /// Entity handle for entity management
    pub entity: Option<super::init_entity::EntityHandle>,
    /// Steward handle for SDIS ceremonies
    pub steward: Option<icn_steward::StewardHandle>,
    /// Agreement manager for inter-cooperative agreements
    pub agreement_manager: Option<Arc<icn_federation::agreement::AgreementManager<icn_federation::agreement::InMemoryAgreementStore>>>,
}

/// Spawn the Gateway API server if enabled
///
/// The gateway runs in a dedicated thread with its own tokio runtime
/// because actix-web has specific runtime requirements.
///
/// Returns true if the gateway was spawned, false if disabled or failed.
pub fn spawn_gateway(config: &GatewayConfig, data_dir: PathBuf, handles: GatewayHandles) -> bool {
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
        return false;
    }

    info!("Gateway JWT secret verified, spawning server...");

    let jwt_secret = config.jwt_secret.clone().into_bytes();

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
    let trust_graph_handle = handles.trust_graph;
    let governance_handle = handles.governance;
    let treasury_handle = handles.treasury;
    let ledger_handle = handles.ledger;
    let entity_handle = handles.entity;
    let steward_handle = handles.steward;
    let agreement_manager_handle = handles.agreement_manager;

    // Spawn gateway in a dedicated thread (actix-web has its own runtime)
    std::thread::spawn(move || {
        // SAFETY: Runtime creation only fails with invalid config or resource exhaustion
        #[allow(clippy::unwrap_used)]
        let rt = tokio::runtime::Runtime::new().unwrap();
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

            if let Some(handle) = trust_graph_handle {
                gateway_server = gateway_server.with_trust_handle(handle);
            }

            if let Some(handle) = governance_handle {
                gateway_server = gateway_server.with_governance_handle(handle);
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

            if let Some(handle) = steward_handle {
                gateway_server = gateway_server.with_steward_handle(handle);
            }

            if let Some(handle) = agreement_manager_handle {
                gateway_server = gateway_server.with_agreement_manager_handle(handle);
            }

            if let Err(e) = gateway_server.run().await {
                warn!("Gateway server error: {}", e);
                icn_obs::metrics::supervisor::error_inc("gateway_server");
            }
        });
    });

    icn_obs::metrics::supervisor::actor_spawned_inc("gateway");
    icn_obs::metrics::supervisor::actor_active_set("gateway", true);
    info!("Gateway API spawned on {}", gateway_addr);

    true
}
