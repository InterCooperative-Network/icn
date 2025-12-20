//! RPC and Gateway server initialization
//!
//! Initializes the gRPC server for daemon communication and the REST/WebSocket
//! gateway for cooperative applications.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use icn_ccl::ContractRuntime;
use icn_compute::ComputeHandle;
use icn_gossip::GossipActor;
use icn_ledger::{DisputeManager, Ledger};
use icn_net::NetworkHandle;
use icn_rpc::RpcServer;
use icn_trust::TrustGraph;

use crate::governance::GovernanceHandle;

/// Dependencies for RPC server initialization
pub struct RpcDeps {
    pub network_handle: NetworkHandle,
    pub ledger_handle: Arc<RwLock<Ledger>>,
    pub contract_runtime: Arc<RwLock<ContractRuntime>>,
    pub gossip_handle: Arc<RwLock<GossipActor>>,
    pub governance_handle: GovernanceHandle,
    pub compute_handle: ComputeHandle,
    pub trust_graph: Arc<RwLock<TrustGraph>>,
    pub dispute_manager: Arc<RwLock<DisputeManager>>,
    pub federation_registry: Option<Arc<icn_federation::CooperativeRegistry>>,
}

/// Configuration for RPC server
pub struct RpcConfig {
    /// RPC server address (usually 127.0.0.1:rpc_port)
    pub addr: SocketAddr,
    /// JWT secret for authentication (optional, enables auth if provided)
    pub jwt_secret: Option<Vec<u8>>,
    /// Enable trust-based rate limiting
    pub enable_trust_rate_limiting: bool,
}

impl RpcConfig {
    /// Create RPC config from daemon config
    pub fn from_daemon_config(config: &crate::config::Config) -> Self {
        // SAFETY: 127.0.0.1 is always valid, and rpc_port is u16, so this format always parses
        #[allow(clippy::expect_used)]
        let rpc_addr: SocketAddr = format!("127.0.0.1:{}", config.network.rpc_port)
            .parse()
            .expect("127.0.0.1:port is always a valid socket address");

        let jwt_secret = if config.gateway.enabled && !config.gateway.jwt_secret.is_empty() {
            Some(config.gateway.jwt_secret.as_bytes().to_vec())
        } else {
            None
        };

        Self {
            addr: rpc_addr,
            jwt_secret,
            enable_trust_rate_limiting: true,
        }
    }
}

/// Spawn the RPC server with all necessary handles
///
/// Returns the compute handle for use by the gateway (if needed)
pub fn spawn_rpc_server(
    config: RpcConfig,
    deps: RpcDeps,
    background_tasks: &mut tokio::task::JoinSet<()>,
) -> ComputeHandle {
    // Create RPC server with optional auth
    let mut rpc_server = if let Some(ref jwt_secret) = config.jwt_secret {
        info!("RPC server auth enabled (using gateway JWT secret)");
        RpcServer::new_with_auth(config.addr, jwt_secret.clone())
    } else {
        RpcServer::new(config.addr)
    };

    // Wire up all handles
    rpc_server.set_network_handle(deps.network_handle);
    rpc_server.set_ledger_handle(deps.ledger_handle);
    rpc_server.set_contract_runtime(deps.contract_runtime);
    rpc_server.set_gossip_handle(deps.gossip_handle);
    rpc_server.set_governance_handle(deps.governance_handle);

    // Clone compute handle for gateway before giving to RPC
    let compute_handle_for_gateway = deps.compute_handle.clone();
    rpc_server.set_compute_handle(deps.compute_handle);

    rpc_server.set_trust_handle(deps.trust_graph);
    rpc_server.set_dispute_manager(deps.dispute_manager);

    if let Some(registry) = deps.federation_registry {
        rpc_server.set_federation_registry(registry);
    }

    // Enable trust-based rate limiting for API requests (C8)
    if config.enable_trust_rate_limiting {
        rpc_server.enable_trust_rate_limiting();
    }

    // Spawn RPC server
    background_tasks.spawn(async move {
        if let Err(e) = rpc_server.run().await {
            warn!("RPC server error: {}", e);
        }
    });

    info!("RPC server spawned on {}", config.addr);

    compute_handle_for_gateway
}

/// Configuration for Gateway server
pub struct GatewayConfig {
    /// Gateway server address
    pub addr: SocketAddr,
    /// JWT secret for authentication
    pub jwt_secret: Vec<u8>,
    /// Data directory for persistent storage
    pub data_dir: Option<std::path::PathBuf>,
}

impl GatewayConfig {
    /// Create gateway config from daemon config
    pub fn from_daemon_config(config: &crate::config::Config) -> Option<Self> {
        if !config.gateway.enabled {
            debug!("Gateway API disabled in configuration");
            return None;
        }

        if config.gateway.jwt_secret.is_empty() {
            warn!("Gateway enabled but JWT secret not configured - gateway will not start");
            warn!("Set jwt_secret in config or ICN_GATEWAY_JWT_SECRET environment variable");
            return None;
        }

        let gateway_addr: SocketAddr = match config.gateway.bind_addr.parse() {
            Ok(addr) => addr,
            Err(e) => {
                warn!(
                    "Invalid gateway bind address '{}': {} - gateway will not start",
                    config.gateway.bind_addr, e
                );
                return None;
            }
        };

        Some(Self {
            addr: gateway_addr,
            jwt_secret: config.gateway.jwt_secret.clone().into_bytes(),
            data_dir: Some(config.data_dir.clone()),
        })
    }
}

/// Dependencies for Gateway server initialization
pub struct GatewayDeps {
    /// Event broadcaster for WebSocket delivery
    pub event_broadcaster: Option<Arc<icn_gateway::EventBroadcaster>>,
    /// Compute handle for task submission
    pub compute_handle: Option<ComputeHandle>,
}

/// Spawn the Gateway server in a dedicated thread
///
/// The gateway uses actix-web which has its own runtime, so we spawn it
/// in a separate thread to avoid conflicts with the main tokio runtime.
pub fn spawn_gateway_server(config: GatewayConfig, deps: GatewayDeps) -> JoinHandle<()> {
    info!("Gateway JWT secret verified, spawning server...");

    let addr = config.addr;
    let broadcaster = deps.event_broadcaster;
    let compute_handle = deps.compute_handle;

    // Spawn gateway in a dedicated thread (actix-web has its own runtime)
    let join_handle = std::thread::spawn(move || {
        // SAFETY: Runtime creation only fails with invalid config or resource exhaustion
        #[allow(clippy::unwrap_used)]
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut gateway_server = if let Some(b) = broadcaster {
                info!("Using shared EventBroadcaster for real-time WebSocket delivery");
                icn_gateway::GatewayServer::new_with_broadcaster(
                    config.addr,
                    config.jwt_secret,
                    config.data_dir,
                    b,
                )
            } else {
                icn_gateway::GatewayServer::new(config.addr, config.jwt_secret)
            };

            // Connect compute handle if available
            if let Some(handle) = compute_handle {
                gateway_server = gateway_server.with_compute_handle(handle);
            }

            if let Err(e) = gateway_server.run().await {
                warn!("Gateway server error: {}", e);
            }
        });
    });

    info!("Gateway API spawned on {}", addr);

    // Note: std::thread::JoinHandle needs to be converted to tokio::task::JoinHandle
    // The gateway runs in its own thread, so we return a blocking task that waits for it
    tokio::task::spawn_blocking(move || {
        let _ = join_handle.join();
    })
}
