//! JSON-RPC server for daemon communication

use anyhow::Result;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use icn_net::NetworkHandle;

use crate::types::{NetworkStats, NetworkStatus, PeerInfo, RpcRequest, RpcResponse};

/// RPC server state
pub struct RpcServer {
    network_handle: Option<Arc<RwLock<NetworkHandle>>>,
    listen_addr: SocketAddr,
}

impl RpcServer {
    /// Create a new RPC server
    pub fn new(listen_addr: SocketAddr) -> Self {
        RpcServer {
            network_handle: None,
            listen_addr,
        }
    }

    /// Set the network handle (called after NetworkActor spawns)
    pub fn set_network_handle(&mut self, handle: NetworkHandle) {
        self.network_handle = Some(Arc::new(RwLock::new(handle)));
    }

    /// Start the RPC server
    pub async fn run(self) -> Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        info!("RPC server listening on {}", self.listen_addr);

        let shared_state = Arc::new(self);

        loop {
            let (stream, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    continue;
                }
            };

            let io = TokioIo::new(stream);
            let state = shared_state.clone();

            tokio::task::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(
                        io,
                        service_fn(move |req| {
                            let state = state.clone();
                            async move { handle_request(req, state).await }
                        }),
                    )
                    .await
                {
                    error!("Error serving connection: {:?}", err);
                }
            });
        }
    }
}

/// Handle a single HTTP request
async fn handle_request(
    req: Request<Incoming>,
    state: Arc<RpcServer>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    // Parse JSON-RPC request
    let whole_body = req.collect().await?.to_bytes();

    let rpc_request: RpcRequest = match serde_json::from_slice(&whole_body) {
        Ok(req) => req,
        Err(e) => {
            warn!("Failed to parse RPC request: {}", e);
            let response = RpcResponse::error(0, -32700, "Parse error".to_string());
            return Ok(json_response(StatusCode::OK, &response));
        }
    };

    debug!("RPC request: {:?}", rpc_request);

    // Dispatch to handler
    let response = dispatch_request(&rpc_request, &state).await;

    Ok(json_response(StatusCode::OK, &response))
}

/// Dispatch RPC request to appropriate handler
async fn dispatch_request(req: &RpcRequest, state: &Arc<RpcServer>) -> RpcResponse {
    match req.method.as_str() {
        "network.peers" => handle_network_peers(req.id, state).await,
        "network.dial" => handle_network_dial(req.id, &req.params, state).await,
        "network.stats" => handle_network_stats(req.id, state).await,
        "network.status" => handle_network_status(req.id, state).await,
        _ => RpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method)),
    }
}

/// Handle network.peers RPC call
async fn handle_network_peers(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let network_handle = match &state.network_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Network actor not available".to_string(),
            );
        }
    };

    let handle = network_handle.read().await;
    match handle.get_peers().await {
        Ok(peers) => {
            let peer_infos: Vec<PeerInfo> = peers
                .into_iter()
                .map(|p| PeerInfo {
                    did: p.did.as_str().to_string(),
                    addr: p.addr.to_string(),
                    version: p.version,
                })
                .collect();

            match serde_json::to_value(&peer_infos) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get peers: {}", e)),
    }
}

/// Handle network.dial RPC call
async fn handle_network_dial(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let network_handle = match &state.network_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Network actor not available".to_string(),
            );
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct DialParams {
        did: String,
        addr: String,
    }

    let dial_params: DialParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
        }
    };

    let addr: SocketAddr = match dial_params.addr.parse() {
        Ok(a) => a,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid address: {}", e));
        }
    };

    let did = match serde_json::from_value(serde_json::Value::String(dial_params.did)) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID: {}", e));
        }
    };

    let handle = network_handle.read().await;
    match handle.dial(addr, did).await {
        Ok(_) => RpcResponse::success(id, serde_json::json!({"success": true})),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to dial: {}", e)),
    }
}

/// Handle network.stats RPC call
async fn handle_network_stats(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let network_handle = match &state.network_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Network actor not available".to_string(),
            );
        }
    };

    let handle = network_handle.read().await;
    match handle.get_stats().await {
        Ok(stats) => {
            let stats_info = NetworkStats {
                peers_discovered: stats.peers_discovered,
                connections_active: stats.connections_active,
                connections_total: stats.connections_total,
            };

            match serde_json::to_value(&stats_info) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get stats: {}", e)),
    }
}

/// Handle network.status RPC call
async fn handle_network_status(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let status = if state.network_handle.is_some() {
        NetworkStatus {
            running: true,
            listen_addr: "0.0.0.0:4433".to_string(), // TODO: Get from config
        }
    } else {
        NetworkStatus {
            running: false,
            listen_addr: "".to_string(),
        }
    };

    match serde_json::to_value(&status) {
        Ok(value) => RpcResponse::success(id, value),
        Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
    }
}

/// Create a JSON response
fn json_response(status: StatusCode, response: &RpcResponse) -> Response<Full<Bytes>> {
    let json = serde_json::to_string(response).unwrap();
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .unwrap()
}
