//! JSON-RPC client for CLI communication with daemon
//!
//! Supports optional JWT authentication:
//! - Without credentials: works in dev mode (auth disabled)
//! - With credentials: performs challenge-response auth and includes JWT token

use anyhow::{Context, Result};
use icn_identity::KeyPair;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::types::{
    ContractExecutionResponse, ContractInfo, GovernanceDomainInfo, LedgerBalance, LedgerEntry,
    NetworkStats, NetworkStatus, PeerInfo, ProposalInfo, RpcRequest, RpcResponse, TrustEdgeInfo,
};

/// RPC client for daemon communication
pub struct RpcClient {
    base_url: String,
    next_id: u64,
    /// JWT token for authenticated requests
    token: Option<String>,
    /// Credentials for authentication (DID + keypair)
    credentials: Option<Arc<KeyPair>>,
}

impl RpcClient {
    /// Create a new RPC client without authentication
    ///
    /// This works only when the daemon has auth disabled (development mode).
    pub fn new(addr: SocketAddr) -> Self {
        RpcClient {
            base_url: format!("http://{addr}"),
            next_id: 1,
            token: None,
            credentials: None,
        }
    }

    /// Create a new RPC client with authentication credentials
    ///
    /// The client will automatically authenticate when making its first request.
    pub fn with_credentials(addr: SocketAddr, keypair: Arc<KeyPair>) -> Self {
        RpcClient {
            base_url: format!("http://{addr}"),
            next_id: 1,
            token: None,
            credentials: Some(keypair),
        }
    }

    /// Authenticate with the RPC server using challenge-response flow
    ///
    /// This is called automatically when credentials are provided and a token is needed.
    /// Can also be called manually to refresh an expired token.
    pub async fn authenticate(&mut self, scopes: Vec<String>) -> Result<()> {
        // Clone the keypair to avoid borrow conflicts
        let keypair = self
            .credentials
            .clone()
            .context("No credentials configured for authentication")?;

        let did = keypair.did();
        let did_str = did.to_string();

        // Step 1: Request challenge
        let challenge_result = self
            .call_raw("auth.challenge", serde_json::json!({ "did": did_str }))
            .await?;

        let nonce = challenge_result["nonce"]
            .as_str()
            .context("Missing nonce in challenge response")?;

        // Step 2: Sign the nonce
        let nonce_bytes = hex::decode(nonce).context("Invalid nonce format")?;
        let signature = keypair.sign(&nonce_bytes);
        let signature_hex = hex::encode(signature.to_bytes());

        // Step 3: Verify and get token
        let verify_result = self
            .call_raw(
                "auth.verify",
                serde_json::json!({
                    "did": did_str,
                    "signature": signature_hex,
                    "scopes": scopes,
                }),
            )
            .await?;

        let token = verify_result["token"]
            .as_str()
            .context("Missing token in verify response")?
            .to_string();

        self.token = Some(token);

        tracing::debug!(did = %did_str, "RPC client authenticated");
        Ok(())
    }

    /// Check if the client is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    /// Clear the current authentication token
    pub fn clear_token(&mut self) {
        self.token = None;
    }

    /// Call an RPC method (with automatic auth if credentials are set)
    pub async fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // Auto-authenticate if we have credentials but no token
        // (and this isn't an auth method)
        if self.credentials.is_some() && self.token.is_none() && !method.starts_with("auth.") {
            // Request all read/write scopes for general use
            self.authenticate(vec![
                "network:read".to_string(),
                "network:write".to_string(),
                "ledger:read".to_string(),
                "ledger:write".to_string(),
                "contract:read".to_string(),
                "contract:write".to_string(),
                "compute:read".to_string(),
                "compute:write".to_string(),
                "governance:read".to_string(),
                "governance:write".to_string(),
                "policy:read".to_string(),
                "policy:write".to_string(),
                "trust:read".to_string(),
                "trust:write".to_string(),
                "recovery:read".to_string(),
                "recovery:write".to_string(),
            ])
            .await?;
        }

        self.call_raw(method, params).await
    }

    /// Call an RPC method without automatic authentication
    async fn call_raw(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: self.next_id,
        };
        self.next_id += 1;

        let client = reqwest::Client::new();
        let mut req_builder = client.post(&self.base_url).json(&request);

        // Add authorization header if we have a token
        if let Some(token) = &self.token {
            req_builder = req_builder.header("Authorization", format!("Bearer {token}"));
        }

        let response = req_builder
            .send()
            .await
            .context("Failed to send RPC request")?;

        let rpc_response: RpcResponse = response
            .json()
            .await
            .context("Failed to parse RPC response")?;

        if let Some(error) = rpc_response.error {
            // Check for auth errors
            if error.code == -32001 {
                anyhow::bail!(
                    "Authentication required: {}. Use --credentials or run daemon with auth disabled.",
                    error.message
                );
            }
            if error.code == -32002 {
                anyhow::bail!("Authorization failed: {}", error.message);
            }
            anyhow::bail!("RPC error {}: {}", error.code, error.message);
        }

        rpc_response
            .result
            .context("RPC response missing result field")
    }

    /// Get list of discovered peers
    pub async fn get_peers(&mut self) -> Result<Vec<PeerInfo>> {
        let result = self.call("network.peers", serde_json::json!({})).await?;
        let peers: Vec<PeerInfo> =
            serde_json::from_value(result).context("Failed to deserialize peers")?;
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
        let stats: NetworkStats =
            serde_json::from_value(result).context("Failed to deserialize stats")?;
        Ok(stats)
    }

    /// Get network status
    pub async fn get_status(&mut self) -> Result<NetworkStatus> {
        let result = self.call("network.status", serde_json::json!({})).await?;
        let status: NetworkStatus =
            serde_json::from_value(result).context("Failed to deserialize status")?;
        Ok(status)
    }

    /// Get the most recent ledger entry (head)
    pub async fn get_ledger_head(&mut self) -> Result<Option<LedgerEntry>> {
        let result = self.call("ledger.head", serde_json::json!({})).await?;
        if result.is_null() {
            Ok(None)
        } else {
            let entry: LedgerEntry =
                serde_json::from_value(result).context("Failed to deserialize ledger entry")?;
            Ok(Some(entry))
        }
    }

    /// Get balance for an account
    /// If currency is provided, returns single balance; otherwise returns all balances
    pub async fn get_ledger_balance(
        &mut self,
        account_id: String,
        currency: Option<String>,
    ) -> Result<Vec<LedgerBalance>> {
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
            let balances: Vec<LedgerBalance> =
                serde_json::from_value(result).context("Failed to deserialize balances")?;
            Ok(balances)
        } else {
            let balance: LedgerBalance =
                serde_json::from_value(result).context("Failed to deserialize balance")?;
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
        let entries: Vec<LedgerEntry> =
            serde_json::from_value(result).context("Failed to deserialize ledger entries")?;
        Ok(entries)
    }

    /// List all quarantined ledger entries
    pub async fn quarantine_list(&mut self) -> Result<serde_json::Value> {
        self.call("ledger.quarantine.list", serde_json::json!({}))
            .await
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
        self.call("ledger.quarantine.purge", serde_json::json!({}))
            .await
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
        let contracts: Vec<ContractInfo> =
            serde_json::from_value(result).context("Failed to deserialize contracts")?;
        Ok(contracts)
    }

    // ========================================================================
    // Trust methods
    // ========================================================================

    /// Add a trust edge to another DID
    pub async fn add_trust(
        &mut self,
        target_did: &str,
        score: f64,
        label: Option<&str>,
    ) -> Result<()> {
        let params = serde_json::json!({
            "target_did": target_did,
            "score": score,
            "label": label,
        });
        self.call("trust.add", params).await?;
        Ok(())
    }

    /// Remove a trust edge to another DID
    pub async fn remove_trust(&mut self, target_did: &str) -> Result<()> {
        let params = serde_json::json!({
            "target_did": target_did,
        });
        self.call("trust.remove", params).await?;
        Ok(())
    }

    /// List outgoing trust edges
    pub async fn list_trust(&mut self) -> Result<Vec<TrustEdgeInfo>> {
        let result = self.call("trust.list", serde_json::json!({})).await?;
        let edges: Vec<TrustEdgeInfo> =
            serde_json::from_value(result).context("Failed to deserialize trust edges")?;
        Ok(edges)
    }

    /// Compute trust score for a target DID
    pub async fn compute_trust(&mut self, target_did: &str) -> Result<f64> {
        let params = serde_json::json!({
            "target_did": target_did,
        });
        let result = self.call("trust.compute", params).await?;
        let score = result["score"]
            .as_f64()
            .context("Missing score in response")?;
        Ok(score)
    }

    // ========================================================================
    // Governance methods
    // ========================================================================

    /// List all governance domains
    pub async fn list_domains(&mut self) -> Result<Vec<GovernanceDomainInfo>> {
        let result = self
            .call("governance.domain.list", serde_json::json!({}))
            .await?;
        let domains: Vec<GovernanceDomainInfo> =
            serde_json::from_value(result).context("Failed to deserialize domains")?;
        Ok(domains)
    }

    /// Get a governance domain by ID
    pub async fn get_domain(&mut self, domain_id: &str) -> Result<GovernanceDomainInfo> {
        let params = serde_json::json!({ "domain_id": domain_id });
        let result = self.call("governance.domain.get", params).await?;
        let domain: GovernanceDomainInfo =
            serde_json::from_value(result).context("Failed to deserialize domain")?;
        Ok(domain)
    }

    /// Create a new governance domain
    pub async fn create_domain(
        &mut self,
        domain_id: &str,
        name: &str,
        members: Vec<String>,
    ) -> Result<()> {
        let params = serde_json::json!({
            "domain_id": domain_id,
            "name": name,
            "members": members,
        });
        self.call("governance.domain.create", params).await?;
        Ok(())
    }

    /// List all proposals
    pub async fn list_proposals(&mut self) -> Result<Vec<ProposalInfo>> {
        let result = self
            .call("governance.proposal.list", serde_json::json!({}))
            .await?;
        let proposals: Vec<ProposalInfo> =
            serde_json::from_value(result).context("Failed to deserialize proposals")?;
        Ok(proposals)
    }

    /// Get a proposal by ID
    pub async fn get_proposal(&mut self, proposal_id: &str) -> Result<ProposalInfo> {
        let params = serde_json::json!({ "proposal_id": proposal_id });
        let result = self.call("governance.proposal.get", params).await?;
        let proposal: ProposalInfo =
            serde_json::from_value(result).context("Failed to deserialize proposal")?;
        Ok(proposal)
    }

    /// Create a new proposal
    pub async fn create_proposal(
        &mut self,
        domain_id: &str,
        title: &str,
        description: &str,
        payload_kind: &str,
    ) -> Result<String> {
        let params = serde_json::json!({
            "domain_id": domain_id,
            "title": title,
            "description": description,
            "payload_kind": payload_kind,
        });
        let result = self.call("governance.proposal.create", params).await?;
        let proposal_id = result["proposal_id"]
            .as_str()
            .context("Missing proposal_id in response")?
            .to_string();
        Ok(proposal_id)
    }

    /// Open a proposal for voting
    pub async fn open_proposal(&mut self, proposal_id: &str) -> Result<()> {
        let params = serde_json::json!({ "proposal_id": proposal_id });
        self.call("governance.proposal.open", params).await?;
        Ok(())
    }

    /// Close a proposal
    pub async fn close_proposal(&mut self, proposal_id: &str) -> Result<()> {
        let params = serde_json::json!({ "proposal_id": proposal_id });
        self.call("governance.proposal.close", params).await?;
        Ok(())
    }

    /// Cast a vote on a proposal
    pub async fn cast_vote(&mut self, proposal_id: &str, choice: &str) -> Result<()> {
        let params = serde_json::json!({
            "proposal_id": proposal_id,
            "choice": choice,
        });
        self.call("governance.vote.cast", params).await?;
        Ok(())
    }

    // ========================================================================
    // Recovery methods
    // ========================================================================

    /// Initiate social recovery for a lost identity
    pub async fn initiate_recovery(
        &mut self,
        old_did: &str,
        threshold: usize,
        delay_period: Option<u64>,
    ) -> Result<String> {
        let params = if let Some(delay) = delay_period {
            serde_json::json!({
                "old_did": old_did,
                "threshold": threshold,
                "delay_period": delay,
            })
        } else {
            serde_json::json!({
                "old_did": old_did,
                "threshold": threshold,
            })
        };

        let result = self.call("recovery.initiate", params).await?;
        let recovery_id = result["recovery_id"]
            .as_str()
            .context("Missing recovery_id in response")?
            .to_string();
        Ok(recovery_id)
    }

    /// Sign a recovery attestation as a trustee
    pub async fn attest_recovery(
        &mut self,
        recovery_id: &str,
        verification_method: &str,
    ) -> Result<bool> {
        let params = serde_json::json!({
            "recovery_id": recovery_id,
            "verification_method": verification_method,
        });

        let result = self.call("recovery.attest", params).await?;
        let threshold_reached = result["threshold_reached"]
            .as_bool()
            .context("Missing threshold_reached in response")?;
        Ok(threshold_reached)
    }

    /// List all recovery events
    pub async fn list_recoveries(&mut self) -> Result<serde_json::Value> {
        self.call("recovery.list", serde_json::json!({})).await
    }

    /// Get status of a specific recovery
    pub async fn get_recovery_status(&mut self, recovery_id: &str) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "recovery_id": recovery_id,
        });
        self.call("recovery.status", params).await
    }

    /// Finalize a recovery after threshold + delay
    pub async fn finalize_recovery(&mut self, recovery_id: &str) -> Result<()> {
        let params = serde_json::json!({
            "recovery_id": recovery_id,
        });
        self.call("recovery.finalize", params).await?;
        Ok(())
    }

    /// Cancel a recovery (fraud detection or device found)
    pub async fn cancel_recovery(&mut self, recovery_id: &str, reason: &str) -> Result<()> {
        let params = serde_json::json!({
            "recovery_id": recovery_id,
            "reason": reason,
        });
        self.call("recovery.cancel", params).await?;
        Ok(())
    }
}
