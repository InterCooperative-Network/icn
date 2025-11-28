//! JSON-RPC client for CLI communication with daemon

use anyhow::{Context, Result};
use std::net::SocketAddr;

use crate::types::{ContractExecutionResponse, ContractInfo, LedgerBalance, LedgerEntry, NetworkStats, NetworkStatus, PeerInfo, RpcRequest, RpcResponse};

/// RPC client for daemon communication
pub struct RpcClient {
    base_url: String,
    next_id: u64,
}

impl RpcClient {
    /// Create a new RPC client
    pub fn new(addr: SocketAddr) -> Self {
        RpcClient {
            base_url: format!("http://{addr}"),
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

    /// Get the most recent ledger entry (head)
    pub async fn get_ledger_head(&mut self) -> Result<Option<LedgerEntry>> {
        let result = self.call("ledger.head", serde_json::json!({})).await?;
        if result.is_null() {
            Ok(None)
        } else {
            let entry: LedgerEntry = serde_json::from_value(result)
                .context("Failed to deserialize ledger entry")?;
            Ok(Some(entry))
        }
    }

    /// Get balance for an account
    /// If currency is provided, returns single balance; otherwise returns all balances
    pub async fn get_ledger_balance(&mut self, account_id: String, currency: Option<String>) -> Result<Vec<LedgerBalance>> {
        let params = if let Some(curr) = currency {
            serde_json::json!({
                "account_id": account_id,
                "currency": curr,
            })
        } else {
            serde_json::json!({
                "account_id": account_id,
            })
        };

        let result = self.call("ledger.balance", params).await?;

        // Result can be a single balance or array of balances
        if result.is_array() {
            let balances: Vec<LedgerBalance> = serde_json::from_value(result)
                .context("Failed to deserialize balances")?;
            Ok(balances)
        } else {
            let balance: LedgerBalance = serde_json::from_value(result)
                .context("Failed to deserialize balance")?;
            Ok(vec![balance])
        }
    }

    /// Get ledger history (recent entries)
    pub async fn get_ledger_history(&mut self, limit: Option<usize>) -> Result<Vec<LedgerEntry>> {
        let params = if let Some(l) = limit {
            serde_json::json!({ "limit": l })
        } else {
            serde_json::json!({})
        };

        let result = self.call("ledger.history", params).await?;
        let entries: Vec<LedgerEntry> = serde_json::from_value(result)
            .context("Failed to deserialize ledger entries")?;
        Ok(entries)
    }

    /// List all quarantined ledger entries
    pub async fn quarantine_list(&mut self) -> Result<serde_json::Value> {
        self.call("ledger.quarantine.list", serde_json::json!({})).await
    }

    /// Get detailed info about a quarantined entry
    pub async fn quarantine_get(&mut self, entry_id: String) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "entry_id": entry_id,
        });
        self.call("ledger.quarantine.get", params).await
    }

    /// Release a quarantined entry (retry)
    pub async fn quarantine_release(&mut self, entry_id: String) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "entry_id": entry_id,
        });
        self.call("ledger.quarantine.release", params).await
    }

    /// Permanently drop a quarantined entry
    pub async fn quarantine_drop(&mut self, entry_id: String) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "entry_id": entry_id,
        });
        self.call("ledger.quarantine.drop", params).await
    }

    /// Purge all expired quarantined entries
    pub async fn quarantine_purge(&mut self) -> Result<serde_json::Value> {
        self.call("ledger.quarantine.purge", serde_json::json!({})).await
    }

    /// Deploy a contract with signed deployment message
    pub async fn deploy_contract(&mut self, deployment_message: String) -> Result<String> {
        let params = serde_json::json!({
            "deployment_message": deployment_message,
        });

        let result = self.call("contract.deploy", params).await?;

        // Extract code_hash from response
        let code_hash = result["code_hash"]
            .as_str()
            .context("Missing code_hash in response")?
            .to_string();

        Ok(code_hash)
    }

    /// Call a contract rule
    pub async fn call_contract(
        &mut self,
        code_hash: String,
        rule_name: String,
        caller: String,
        args: serde_json::Value,
    ) -> Result<ContractExecutionResponse> {
        let params = serde_json::json!({
            "code_hash": code_hash,
            "rule_name": rule_name,
            "caller": caller,
            "args": args,
        });

        let result = self.call("contract.call", params).await?;
        let response: ContractExecutionResponse = serde_json::from_value(result)
            .context("Failed to deserialize contract execution response")?;
        Ok(response)
    }

    /// List deployed contracts
    pub async fn list_contracts(&mut self) -> Result<Vec<ContractInfo>> {
        let result = self.call("contract.list", serde_json::json!({})).await?;
        let contracts: Vec<ContractInfo> = serde_json::from_value(result)
            .context("Failed to deserialize contracts")?;
        Ok(contracts)
    }
}
