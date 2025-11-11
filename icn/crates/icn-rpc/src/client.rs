//! JSON-RPC client for CLI communication with daemon

use anyhow::{Context, Result};
use std::net::SocketAddr;

use crate::types::{NetworkStats, NetworkStatus, PeerInfo, RpcRequest, RpcResponse};

/// RPC client for daemon communication
pub struct RpcClient {
    base_url: String,
    next_id: u64,
}

impl RpcClient {
    /// Create a new RPC client
    pub fn new(addr: SocketAddr) -> Self {
        RpcClient {
            base_url: format!("http://{}", addr),
            next_id: 1,
        }
    }

    /// Call an RPC method
    async fn call(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: self.next_id,
        };
        self.next_id += 1;

        let client = reqwest::Client::new();
        let response = client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await
            .context("Failed to send RPC request")?;

        let rpc_response: RpcResponse = response
            .json()
            .await
            .context("Failed to parse RPC response")?;

        if let Some(error) = rpc_response.error {
            anyhow::bail!("RPC error {}: {}", error.code, error.message);
        }

        rpc_response
            .result
            .context("RPC response missing result field")
    }

    /// Get list of discovered peers
    pub async fn get_peers(&mut self) -> Result<Vec<PeerInfo>> {
        let result = self.call("network.peers", serde_json::json!({})).await?;
        let peers: Vec<PeerInfo> = serde_json::from_value(result)
            .context("Failed to deserialize peers")?;
        Ok(peers)
    }

    /// Dial a peer
    pub async fn dial(&mut self, did: String, addr: String) -> Result<()> {
        let params = serde_json::json!({
            "did": did,
            "addr": addr,
        });
        self.call("network.dial", params).await?;
        Ok(())
    }

    /// Get network statistics
    pub async fn get_stats(&mut self) -> Result<NetworkStats> {
        let result = self.call("network.stats", serde_json::json!({})).await?;
        let stats: NetworkStats = serde_json::from_value(result)
            .context("Failed to deserialize stats")?;
        Ok(stats)
    }

    /// Get network status
    pub async fn get_status(&mut self) -> Result<NetworkStatus> {
        let result = self.call("network.status", serde_json::json!({})).await?;
        let status: NetworkStatus = serde_json::from_value(result)
            .context("Failed to deserialize status")?;
        Ok(status)
    }
}
