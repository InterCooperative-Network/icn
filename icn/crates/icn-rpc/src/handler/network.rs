//! Network-related RPC handlers

use std::net::SocketAddr;
use std::sync::Arc;

use crate::server::RpcServer;
use crate::types::{NetworkStats, NetworkStatus, PeerInfo, RpcResponse};

/// Handle network.peers RPC call
pub async fn handle_network_peers(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let network_handle = match state.network_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Network actor not available".to_string());
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
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get peers: {e}")),
    }
}

/// Handle network.dial RPC call
pub async fn handle_network_dial(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let network_handle = match state.network_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Network actor not available".to_string());
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let addr: SocketAddr = match dial_params.addr.parse() {
        Ok(a) => a,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid address: {e}"));
        }
    };

    let did = match serde_json::from_value(serde_json::Value::String(dial_params.did)) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID: {e}"));
        }
    };

    let handle = network_handle.read().await;
    match handle.dial(addr, did).await {
        Ok(_) => RpcResponse::success(id, serde_json::json!({"success": true})),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to dial: {e}")),
    }
}

/// Handle network.stats RPC call
pub async fn handle_network_stats(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let network_handle = match state.network_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Network actor not available".to_string());
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
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get stats: {e}")),
    }
}

/// Handle network.status RPC call
pub async fn handle_network_status(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let status = if state.network_handle().is_some() {
        NetworkStatus {
            running: true,
            listen_addr: state.listen_addr().to_string(),
        }
    } else {
        NetworkStatus {
            running: false,
            listen_addr: state.listen_addr().to_string(),
        }
    };

    match serde_json::to_value(&status) {
        Ok(value) => RpcResponse::success(id, value),
        Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
    }
}
