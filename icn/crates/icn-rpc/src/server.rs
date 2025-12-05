//! JSON-RPC server for daemon communication
//!
//! ## Authentication
//!
//! The RPC server supports JWT-based authentication. When authentication is enabled:
//! 1. Clients must first call `auth.challenge` to get a nonce
//! 2. Sign the nonce with their DID keypair
//! 3. Call `auth.verify` to exchange the signature for a JWT token
//! 4. Include the token in the `Authorization: Bearer <token>` header
//!
//! Methods are protected based on their scope requirements (see `auth::required_scope_for_method`).

use anyhow::Result;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use metrics::{counter, gauge, histogram};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use icn_ccl::ContractRuntime;
use icn_compute::ComputeHandle;
use icn_federation::CooperativeRegistry;
use icn_governance::{GovernanceOps, MembershipAction};
use icn_identity::recovery::{RecoveryAttestation, RecoveryEvent, RecoveryStatus};
use icn_identity::Did;
use icn_ledger::{ContentHash, Dispute, DisputeManager, DisputeOutcome, DisputeStatus, Ledger};
use icn_net::NetworkHandle;
use icn_store::Store;
use icn_trust::{TrustEdge, TrustGraph};

use crate::auth::{required_scope_for_method, RpcAuthManager, RpcTokenClaims};
use crate::receipt::ReceiptStore;
use crate::types::{
    CastVoteRequest, CloseProposalRequest, CodeType, ContractExecutionResponse,
    CreateDomainRequest, CreateProposalRequest, CreateProposalResponse, GovernanceDomainInfo,
    GovernanceParamsInfo, LedgerAccountDelta, LedgerBalance, LedgerEntry, MembershipConfigInfo,
    NetworkStats, NetworkStatus, OpenProposalRequest, PeerInfo, ProposalInfo, ProposalPayloadInfo,
    RecoveryAttestationInfo, RecoveryEventInfo, RpcRequest, RpcResponse, SubmitTaskRequest,
    SubmitTaskResponse, TaskResultInfo, TaskStatusInfo,
};

use icn_gossip::GossipActor;

/// RPC server state
pub struct RpcServer {
    network_handle: Option<Arc<RwLock<NetworkHandle>>>,
    ledger_handle: Option<Arc<RwLock<Ledger>>>,
    contract_runtime: Option<Arc<RwLock<ContractRuntime>>>,
    gossip_handle: Option<Arc<RwLock<GossipActor>>>,
    governance_handle: Option<Box<dyn GovernanceOps>>,
    compute_handle: Option<ComputeHandle>,
    trust_handle: Option<Arc<RwLock<TrustGraph>>>,
    store_handle: Option<Arc<dyn Store>>,
    dispute_manager: Option<Arc<RwLock<DisputeManager>>>,
    federation_registry: Option<Arc<CooperativeRegistry>>,
    own_keypair: Option<Arc<icn_identity::KeyPair>>,
    receipt_store: Arc<ReceiptStore>,
    auth_manager: Option<Arc<RpcAuthManager>>,
    listen_addr: SocketAddr,
}

impl RpcServer {
    /// Create a new RPC server (without authentication - for backward compatibility/dev mode)
    pub fn new(listen_addr: SocketAddr) -> Self {
        RpcServer {
            network_handle: None,
            ledger_handle: None,
            contract_runtime: None,
            gossip_handle: None,
            governance_handle: None,
            compute_handle: None,
            trust_handle: None,
            store_handle: None,
            dispute_manager: None,
            federation_registry: None,
            own_keypair: None,
            receipt_store: Arc::new(ReceiptStore::new(10_000, 86400)), // 10k receipts, 24h TTL
            auth_manager: None,
            listen_addr,
        }
    }

    /// Create a new RPC server with authentication enabled
    ///
    /// # Arguments
    /// * `listen_addr` - Address to bind the server
    /// * `jwt_secret` - Secret for signing JWT tokens (should be at least 32 bytes)
    pub fn new_with_auth(listen_addr: SocketAddr, jwt_secret: Vec<u8>) -> Self {
        RpcServer {
            network_handle: None,
            ledger_handle: None,
            contract_runtime: None,
            gossip_handle: None,
            governance_handle: None,
            compute_handle: None,
            trust_handle: None,
            store_handle: None,
            dispute_manager: None,
            federation_registry: None,
            own_keypair: None,
            receipt_store: Arc::new(ReceiptStore::new(10_000, 86400)),
            auth_manager: Some(Arc::new(RpcAuthManager::new(jwt_secret, true))),
            listen_addr,
        }
    }

    /// Set the authentication manager (for configuring after construction)
    pub fn set_auth_manager(&mut self, jwt_secret: Vec<u8>) {
        self.auth_manager = Some(Arc::new(RpcAuthManager::new(jwt_secret, true)));
    }

    /// Check if authentication is enabled
    pub fn auth_enabled(&self) -> bool {
        self.auth_manager
            .as_ref()
            .map(|m| m.is_enabled())
            .unwrap_or(false)
    }

    /// Set the network handle (called after NetworkActor spawns)
    pub fn set_network_handle(&mut self, handle: NetworkHandle) {
        self.network_handle = Some(Arc::new(RwLock::new(handle)));
    }

    /// Set the ledger handle (called after Ledger initializes)
    pub fn set_ledger_handle(&mut self, handle: Arc<RwLock<Ledger>>) {
        self.ledger_handle = Some(handle);
    }

    /// Set the contract runtime handle (called after ContractRuntime initializes)
    pub fn set_contract_runtime(&mut self, handle: Arc<RwLock<ContractRuntime>>) {
        self.contract_runtime = Some(handle);
    }

    /// Set the gossip handle (called after GossipActor initializes)
    pub fn set_gossip_handle(&mut self, handle: Arc<RwLock<GossipActor>>) {
        self.gossip_handle = Some(handle);
    }

    /// Set the governance handle (called after GovernanceActor initializes)
    pub fn set_governance_handle(&mut self, handle: impl GovernanceOps + 'static) {
        self.governance_handle = Some(Box::new(handle));
    }

    /// Set the compute handle (called after ComputeActor spawns)
    pub fn set_compute_handle(&mut self, handle: ComputeHandle) {
        self.compute_handle = Some(handle);
    }

    /// Set the trust graph handle (called after TrustGraph initializes)
    pub fn set_trust_handle(&mut self, handle: Arc<RwLock<TrustGraph>>) {
        self.trust_handle = Some(handle);
    }

    /// Set the store handle (for recovery events and other persistent data)
    pub fn set_store_handle(&mut self, handle: Arc<dyn Store>) {
        self.store_handle = Some(handle);
    }

    /// Set the own keypair handle (for signing recovery attestations)
    pub fn set_own_keypair(&mut self, keypair: Arc<icn_identity::KeyPair>) {
        self.own_keypair = Some(keypair);
    }

    /// Set the dispute manager handle (for ledger dispute operations)
    pub fn set_dispute_manager(&mut self, manager: Arc<RwLock<DisputeManager>>) {
        self.dispute_manager = Some(manager);
    }

    /// Set the federation registry handle (for inter-coop federation operations)
    pub fn set_federation_registry(&mut self, registry: Arc<CooperativeRegistry>) {
        self.federation_registry = Some(registry);
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

/// Extract Bearer token from Authorization header
fn extract_bearer_token(req: &Request<Incoming>) -> Option<String> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Handle a single HTTP request
async fn handle_request(
    req: Request<Incoming>,
    state: Arc<RpcServer>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    // Extract Bearer token before consuming request body
    let bearer_token = extract_bearer_token(&req);

    // Parse JSON-RPC request
    let whole_body = req.collect().await?.to_bytes();

    let rpc_request: RpcRequest = match serde_json::from_slice(&whole_body) {
        Ok(req) => req,
        Err(e) => {
            warn!("Failed to parse RPC request: {}", e);
            counter!("icn_rpc_errors_total", "method" => "unknown", "error_code" => "-32700")
                .increment(1);
            let response = RpcResponse::error(0, -32700, "Parse error".to_string());
            return Ok(json_response(StatusCode::OK, &response));
        }
    };

    let method = rpc_request.method.clone();
    debug!("RPC request: {:?}", rpc_request);

    // Track active requests
    gauge!("icn_rpc_active_requests").increment(1.0);
    let start_time = Instant::now();

    // Check authentication if enabled
    let claims: Option<RpcTokenClaims> = if let Some(auth_manager) = &state.auth_manager {
        // Check if this method requires authentication
        if let Some(required_scope) = required_scope_for_method(&rpc_request.method) {
            // Method requires auth - verify token
            match bearer_token {
                Some(token) => match auth_manager.verify_token(&token) {
                    Ok(claims) => {
                        // Check scope
                        if !claims.has_scope(required_scope) {
                            warn!(
                                "Insufficient scope for method {}: required {}, has {:?}",
                                rpc_request.method, required_scope, claims.scopes
                            );
                            counter!("icn_rpc_auth_failures_total", "reason" => "insufficient_scope").increment(1);
                            gauge!("icn_rpc_active_requests").decrement(1.0);
                            let response = RpcResponse::error(
                                rpc_request.id,
                                -32403,
                                format!("Insufficient scope: requires {required_scope}"),
                            );
                            return Ok(json_response(StatusCode::OK, &response));
                        }
                        Some(claims)
                    }
                    Err(e) => {
                        warn!("Token verification failed: {}", e);
                        counter!("icn_rpc_auth_failures_total", "reason" => "invalid_token")
                            .increment(1);
                        gauge!("icn_rpc_active_requests").decrement(1.0);
                        let response = RpcResponse::error(
                            rpc_request.id,
                            -32401,
                            "Authentication failed: invalid token".to_string(),
                        );
                        return Ok(json_response(StatusCode::OK, &response));
                    }
                },
                None => {
                    warn!(
                        "Missing Authorization header for method {}",
                        rpc_request.method
                    );
                    counter!("icn_rpc_auth_failures_total", "reason" => "missing_token")
                        .increment(1);
                    gauge!("icn_rpc_active_requests").decrement(1.0);
                    let response = RpcResponse::error(
                        rpc_request.id,
                        -32401,
                        "Authentication required: include Authorization: Bearer <token>"
                            .to_string(),
                    );
                    return Ok(json_response(StatusCode::OK, &response));
                }
            }
        } else {
            // Method doesn't require auth (e.g., auth.challenge)
            None
        }
    } else {
        // Auth not enabled - allow all (backward compatibility / dev mode)
        None
    };

    // Increment request counter
    counter!("icn_rpc_requests_total", "method" => method.clone()).increment(1);

    // Dispatch to handler with claims
    let response = dispatch_request(&rpc_request, &state, claims.as_ref()).await;

    // Record duration and decrement active requests
    let duration = start_time.elapsed().as_secs_f64();
    histogram!("icn_rpc_request_duration_seconds", "method" => method.clone()).record(duration);
    gauge!("icn_rpc_active_requests").decrement(1.0);

    // Track errors
    if response.error.is_some() {
        let error_code = response
            .error
            .as_ref()
            .map(|e| e.code.to_string())
            .unwrap_or_default();
        counter!("icn_rpc_errors_total", "method" => method, "error_code" => error_code)
            .increment(1);
    }

    Ok(json_response(StatusCode::OK, &response))
}

/// Dispatch RPC request to appropriate handler
async fn dispatch_request(
    req: &RpcRequest,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    match req.method.as_str() {
        // Authentication methods (no auth required - bootstrap)
        "auth.challenge" => handle_auth_challenge(req.id, &req.params, state).await,
        "auth.verify" => handle_auth_verify(req.id, &req.params, state).await,

        // Network methods
        "network.peers" => handle_network_peers(req.id, state).await,
        "network.dial" => handle_network_dial(req.id, &req.params, state).await,
        "network.stats" => handle_network_stats(req.id, state).await,
        "network.status" => handle_network_status(req.id, state).await,

        // Ledger methods
        "ledger.head" => handle_ledger_head(req.id, state).await,
        "ledger.balance" => handle_ledger_balance(req.id, &req.params, state).await,
        "ledger.history" => handle_ledger_history(req.id, &req.params, state).await,
        "ledger.quarantine.list" => handle_quarantine_list(req.id, &req.params, state).await,
        "ledger.quarantine.get" => handle_quarantine_get(req.id, &req.params, state).await,
        "ledger.quarantine.release" => handle_quarantine_release(req.id, &req.params, state).await,
        "ledger.quarantine.drop" => handle_quarantine_drop(req.id, &req.params, state).await,
        "ledger.quarantine.purge" => handle_quarantine_purge(req.id, state).await,

        // Contract methods
        "contract.deploy" => handle_contract_deploy(req.id, &req.params, state).await,
        "contract.call" => handle_contract_call(req.id, &req.params, state).await,
        "contract.list" => handle_contract_list(req.id, &req.params, state).await,
        "receipt.get" => handle_receipt_get(req.id, &req.params, state).await,

        // Governance methods
        "governance.domain.list" => handle_governance_domain_list(req.id, state).await,
        "governance.domain.get" => handle_governance_domain_get(req.id, &req.params, state).await,
        "governance.domain.create" => {
            handle_governance_domain_create(req.id, &req.params, state).await
        }
        "governance.proposal.list" => handle_governance_proposal_list(req.id, state).await,
        "governance.proposal.get" => {
            handle_governance_proposal_get(req.id, &req.params, state).await
        }
        "governance.proposal.create" => {
            handle_governance_proposal_create(req.id, &req.params, state).await
        }
        "governance.proposal.open" => {
            handle_governance_proposal_open(req.id, &req.params, state).await
        }
        "governance.proposal.close" => {
            handle_governance_proposal_close(req.id, &req.params, state).await
        }
        "governance.vote.cast" => handle_governance_vote_cast(req.id, &req.params, state).await,

        // Compute methods (pass claims for authenticated submitter)
        "compute.submit" => handle_compute_submit(req.id, &req.params, state, claims).await,
        "compute.status" => handle_compute_status(req.id, &req.params, state).await,
        "compute.cancel" => handle_compute_cancel(req.id, &req.params, state, claims).await,

        // Policy methods
        "policy.set" => handle_policy_set(req.id, &req.params, state).await,
        "policy.get" => handle_policy_get(req.id, &req.params, state).await,
        "policy.list" => handle_policy_list(req.id, &req.params, state).await,
        "policy.remove" => handle_policy_remove(req.id, &req.params, state).await,
        "quota.usage" => handle_quota_usage(req.id, &req.params, state).await,
        "quota.list" => handle_quota_list(req.id, &req.params, state).await,

        // Trust methods
        "trust.add" => handle_trust_add(req.id, &req.params, state).await,
        "trust.remove" => handle_trust_remove(req.id, &req.params, state).await,
        "trust.list" => handle_trust_list(req.id, state).await,
        "trust.compute" => handle_trust_compute(req.id, &req.params, state).await,

        // Recovery methods
        "recovery.initiate" => handle_recovery_initiate(req.id, &req.params, state, claims).await,
        "recovery.attest" => handle_recovery_attest(req.id, &req.params, state).await,
        "recovery.list" => handle_recovery_list(req.id, state).await,
        "recovery.status" => handle_recovery_status(req.id, &req.params, state).await,
        "recovery.finalize" => handle_recovery_finalize(req.id, &req.params, state).await,
        "recovery.cancel" => handle_recovery_cancel(req.id, &req.params, state, claims).await,

        // Dispute methods (ledger entry disputes)
        "dispute.file" => handle_dispute_file(req.id, &req.params, state, claims).await,
        "dispute.list" => handle_dispute_list(req.id, &req.params, state).await,
        "dispute.get" => handle_dispute_get(req.id, &req.params, state).await,
        "dispute.add_evidence" => {
            handle_dispute_add_evidence(req.id, &req.params, state, claims).await
        }
        "dispute.assign_mediator" => {
            handle_dispute_assign_mediator(req.id, &req.params, state, claims).await
        }
        "dispute.resolve" => handle_dispute_resolve(req.id, &req.params, state, claims).await,

        // Federation methods (inter-cooperative coordination)
        "federation.coop.list" => handle_federation_coop_list(req.id, state).await,
        "federation.coop.get" => handle_federation_coop_get(req.id, &req.params, state).await,
        "federation.coop.register" => {
            handle_federation_coop_register(req.id, &req.params, state).await
        }
        "federation.coop.remove" => {
            handle_federation_coop_remove(req.id, &req.params, state).await
        }
        "federation.own.get" => handle_federation_own_get(req.id, state).await,
        "federation.own.update" => {
            handle_federation_own_update(req.id, &req.params, state).await
        }
        "federation.vouch.list" => handle_federation_vouch_list(req.id, &req.params, state).await,
        "federation.vouch.issue" => {
            handle_federation_vouch_issue(req.id, &req.params, state, claims).await
        }
        "federation.vouch.remove" => {
            handle_federation_vouch_remove(req.id, &req.params, state).await
        }

        // Attestation methods (cross-cooperative trust)
        "federation.attestation.list" => {
            handle_federation_attestation_list(req.id, &req.params, state).await
        }
        "federation.attestation.from" => {
            handle_federation_attestation_from(req.id, &req.params, state).await
        }
        "federation.attestation.issue" => {
            handle_federation_attestation_issue(req.id, &req.params, state, claims).await
        }

        // Clearing methods (bilateral settlement)
        "federation.clearing.list" => handle_federation_clearing_list(req.id, state).await,
        "federation.clearing.show" => {
            handle_federation_clearing_show(req.id, &req.params, state).await
        }
        "federation.clearing.create" => {
            handle_federation_clearing_create(req.id, &req.params, state, claims).await
        }
        "federation.clearing.position" => {
            handle_federation_clearing_position(req.id, &req.params, state).await
        }
        "federation.clearing.settle" => {
            handle_federation_clearing_settle(req.id, &req.params, state, claims).await
        }

        _ => {
            counter!("icn_rpc_method_not_found_total").increment(1);
            RpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method))
        }
    }
}

// ============================================================================
// Authentication handlers
// ============================================================================

/// Handle auth.challenge RPC call - get a challenge nonce for DID authentication
async fn handle_auth_challenge(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let auth_manager = match &state.auth_manager {
        Some(am) => am,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Authentication not enabled on this server".to_string(),
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct ChallengeParams {
        did: String,
    }

    let params: ChallengeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse DID
    let did = match Did::from_str(&params.did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}"));
        }
    };

    // Create challenge
    match auth_manager.create_challenge(&did) {
        Ok(nonce) => {
            counter!("icn_rpc_auth_challenges_total").increment(1);
            let response = serde_json::json!({
                "nonce": nonce,
                "expires_in_seconds": 300
            });
            RpcResponse::success(id, response)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to create challenge: {e}")),
    }
}

/// Handle auth.verify RPC call - verify signed challenge and get JWT token
async fn handle_auth_verify(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let auth_manager = match &state.auth_manager {
        Some(am) => am,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Authentication not enabled on this server".to_string(),
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct VerifyParams {
        did: String,
        signature: String, // hex-encoded
        scopes: Vec<String>,
    }

    let params: VerifyParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse DID
    let did = match Did::from_str(&params.did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}"));
        }
    };

    // Parse signature
    let signature_bytes = match hex::decode(&params.signature) {
        Ok(b) => b,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid signature encoding: {e}"));
        }
    };

    // Verify challenge and issue token
    counter!("icn_rpc_auth_verifications_total").increment(1);
    match auth_manager.verify_challenge(&did, &signature_bytes, params.scopes) {
        Ok(token) => {
            counter!("icn_rpc_auth_successes_total").increment(1);
            let response = serde_json::json!({
                "token": token,
                "token_type": "Bearer",
                "expires_in_seconds": 86400
            });
            RpcResponse::success(id, response)
        }
        Err(e) => {
            counter!("icn_rpc_auth_failures_total", "reason" => "verification_failed").increment(1);
            RpcResponse::error(id, -32401, format!("Authentication failed: {e}"))
        }
    }
}

// ============================================================================
// Network handlers
// ============================================================================

/// Handle network.peers RPC call
async fn handle_network_peers(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let network_handle = match &state.network_handle {
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
async fn handle_network_dial(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let network_handle = match &state.network_handle {
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
async fn handle_network_stats(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let network_handle = match &state.network_handle {
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
async fn handle_network_status(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let status = if state.network_handle.is_some() {
        NetworkStatus {
            running: true,
            listen_addr: state.listen_addr.to_string(),
        }
    } else {
        NetworkStatus {
            running: false,
            listen_addr: state.listen_addr.to_string(),
        }
    };

    match serde_json::to_value(&status) {
        Ok(value) => RpcResponse::success(id, value),
        Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
    }
}

/// Handle ledger.head RPC call - get the most recent ledger entry
async fn handle_ledger_head(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    let ledger = ledger_handle.read().await;
    match ledger.get_all_entries() {
        Ok(entries) => {
            if let Some(last_entry) = entries.last() {
                let hash = last_entry
                    .id
                    .as_ref()
                    .map(|h| h.to_hex())
                    .unwrap_or_else(|| "unknown".to_string());

                let rpc_entry = LedgerEntry {
                    hash,
                    timestamp: last_entry.timestamp,
                    author: last_entry.author.as_str().to_string(),
                    accounts: last_entry
                        .accounts
                        .iter()
                        .map(|delta| LedgerAccountDelta {
                            account_id: delta.account_id.as_str().to_string(),
                            currency: delta.currency.clone(),
                            debit: delta.debit,
                            credit: delta.credit,
                        })
                        .collect(),
                };

                match serde_json::to_value(&rpc_entry) {
                    Ok(value) => RpcResponse::success(id, value),
                    Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
                }
            } else {
                RpcResponse::success(id, serde_json::json!(null))
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get entries: {e}")),
    }
}

/// Handle ledger.balance RPC call - get balance for an account
async fn handle_ledger_balance(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct BalanceParams {
        account_id: String,
        currency: Option<String>,
    }

    let balance_params: BalanceParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let account_did = match serde_json::from_value(serde_json::Value::String(
        balance_params.account_id.clone(),
    )) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID: {e}"));
        }
    };

    let ledger = ledger_handle.read().await;

    if let Some(currency) = balance_params.currency {
        // Get balance for specific currency
        let amount = ledger.get_balance(&account_did, &currency);
        let balance = LedgerBalance {
            account_id: balance_params.account_id,
            currency,
            amount,
        };

        match serde_json::to_value(&balance) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
        }
    } else {
        // Get all balances for account
        let account_balances = ledger.get_account_balances(&account_did);
        let balances: Vec<LedgerBalance> = account_balances
            .balances
            .iter()
            .map(|(currency, amount)| LedgerBalance {
                account_id: balance_params.account_id.clone(),
                currency: currency.clone(),
                amount: *amount,
            })
            .collect();

        match serde_json::to_value(&balances) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
        }
    }
}

/// Handle ledger.history RPC call - get recent ledger entries (paginated)
async fn handle_ledger_history(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse pagination parameters
    let page_request: crate::PageRequest =
        serde_json::from_value(params.clone()).unwrap_or_default();

    let ledger = ledger_handle.read().await;
    match ledger.get_all_entries() {
        Ok(entries) => {
            // Convert all entries (in reverse order - most recent first)
            let all_entries: Vec<LedgerEntry> = entries
                .iter()
                .rev()
                .map(|entry| {
                    let hash = entry
                        .id
                        .as_ref()
                        .map(|h| h.to_hex())
                        .unwrap_or_else(|| "unknown".to_string());

                    LedgerEntry {
                        hash,
                        timestamp: entry.timestamp,
                        author: entry.author.as_str().to_string(),
                        accounts: entry
                            .accounts
                            .iter()
                            .map(|delta| LedgerAccountDelta {
                                account_id: delta.account_id.as_str().to_string(),
                                currency: delta.currency.clone(),
                                debit: delta.debit,
                                credit: delta.credit,
                            })
                            .collect(),
                    }
                })
                .collect();

            // Apply pagination
            let page = crate::paginate(all_entries, &page_request, crate::DEFAULT_MAX_PAGE_SIZE);

            match serde_json::to_value(&page) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get entries: {e}")),
    }
}

/// Handle contract.deploy RPC call - deploy a new contract with signature verification
async fn handle_contract_deploy(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let gossip_handle = match &state.gossip_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Gossip not available".to_string());
        }
    };

    // Parse parameters - expect full ContractDeploymentMessage
    #[derive(serde::Deserialize)]
    struct DeployParams {
        deployment_message: String, // JSON-encoded ContractDeploymentMessage
    }

    let deploy_params: DeployParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse deployment message from JSON
    let deployment_msg: icn_ccl::ContractDeploymentMessage =
        match serde_json::from_str(&deploy_params.deployment_message) {
            Ok(m) => m,
            Err(e) => {
                return RpcResponse::error(
                    id,
                    -32602,
                    format!("Invalid deployment message JSON: {e}"),
                );
            }
        };

    // Pre-verify signatures before publishing to gossip (early rejection of invalid sigs)
    if let Err(e) = deployment_msg.verify() {
        return RpcResponse::error(id, -32602, format!("Signature verification failed: {e}"));
    }

    let code_hash = deployment_msg.installation.code_hash.clone();

    // Serialize deployment message for gossip
    let message_bytes = match serde_json::to_vec(&deployment_msg) {
        Ok(bytes) => bytes,
        Err(e) => {
            return RpcResponse::error(id, -32603, format!("Failed to serialize deployment: {e}"));
        }
    };

    // Publish to contracts:deploy gossip topic
    let mut gossip = gossip_handle.write().await;
    match gossip.publish("contracts:deploy", message_bytes) {
        Ok(_) => {
            info!(
                "Contract deployment published to gossip: {}",
                code_hash.to_hex()
            );

            // Create receipt for successful deployment
            let receipt = crate::receipt::Receipt::new(
                deployment_msg.installation.installed_by.clone(),
                crate::receipt::Operation::ContractDeploy {
                    code_hash: code_hash.to_hex(),
                },
                crate::receipt::Outcome::success(Some(code_hash.to_hex())),
            );
            let receipt_id = receipt.id.clone();
            state.receipt_store.insert(receipt).await;

            let response = serde_json::json!({
                "code_hash": code_hash.to_hex(),
                "receipt_id": receipt_id.to_string(),
                "success": true,
            });
            RpcResponse::success(id, response)
        }
        Err(e) => {
            error!("Failed to publish contract deployment to gossip: {}", e);

            // Create receipt for failed deployment
            let receipt = crate::receipt::Receipt::new(
                deployment_msg.installation.installed_by.clone(),
                crate::receipt::Operation::ContractDeploy {
                    code_hash: code_hash.to_hex(),
                },
                crate::receipt::Outcome::failure(e.to_string()),
            );
            state.receipt_store.insert(receipt).await;

            RpcResponse::error(id, -32000, format!("Failed to publish deployment: {e}"))
        }
    }
}

/// Handle contract.call RPC call - execute a contract rule
async fn handle_contract_call(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let contract_runtime = match &state.contract_runtime {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Contract runtime not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct CallParams {
        code_hash: String,
        rule_name: String,
        caller: String,
        args: serde_json::Value,
    }

    let call_params: CallParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse code hash
    let hash_bytes = match hex::decode(&call_params.code_hash) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            return RpcResponse::error(id, -32602, "Invalid code hash format".to_string());
        }
    };
    let code_hash = icn_ledger::ContentHash::from_bytes(hash_bytes);

    // Parse caller DID
    let caller_did: icn_identity::Did =
        match serde_json::from_value(serde_json::Value::String(call_params.caller.clone())) {
            Ok(d) => d,
            Err(e) => {
                return RpcResponse::error(id, -32602, format!("Invalid DID: {e}"));
            }
        };

    // Parse arguments
    let args: std::collections::HashMap<String, icn_ccl::Value> =
        match serde_json::from_value(call_params.args) {
            Ok(a) => a,
            Err(e) => {
                return RpcResponse::error(id, -32602, format!("Invalid args: {e}"));
            }
        };

    // Create execution context with generous fuel
    let context = icn_ccl::ExecutionContext::new(
        caller_did.clone(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        10000,  // Generous fuel limit
        vec![], // No capabilities for now
        vec![], // No participants for now
    );

    // Track bytes processed (input params size)
    let input_bytes = params.to_string().len();

    // Start timing for wall_time tracking
    let exec_start = Instant::now();

    // Execute rule
    let mut runtime = contract_runtime.write().await;
    match runtime
        .execute_rule(&code_hash, &call_params.rule_name, context, args)
        .await
    {
        Ok(result) => {
            let response_value =
                serde_json::to_value(&result.value).unwrap_or(serde_json::json!(null));

            // Calculate output bytes and total bytes processed
            let output_bytes = response_value.to_string().len();
            let bytes_processed = input_bytes + output_bytes;

            // Calculate wall time in milliseconds
            let wall_time_ms = exec_start.elapsed().as_millis() as u64;

            // Create receipt for successful execution
            let resources = crate::receipt::Resources {
                fuel_used: result.fuel_consumed,
                bytes_processed,
                wall_time_ms,
            };

            let receipt = crate::receipt::Receipt::with_resources(
                caller_did.clone(),
                crate::receipt::Operation::ContractExecute {
                    code_hash: call_params.code_hash.clone(),
                    rule: call_params.rule_name.clone(),
                },
                crate::receipt::Outcome::success(None),
                resources,
            );
            let receipt_id = receipt.id.clone();
            state.receipt_store.insert(receipt).await;

            let response = ContractExecutionResponse {
                success: true,
                fuel_consumed: result.fuel_consumed,
                return_value: response_value,
            };

            // Add receipt_id to response
            let mut value = serde_json::to_value(&response).unwrap();
            value.as_object_mut().unwrap().insert(
                "receipt_id".to_string(),
                serde_json::Value::String(receipt_id.to_string()),
            );

            RpcResponse::success(id, value)
        }
        Err(e) => {
            // Create receipt for failed execution
            let receipt = crate::receipt::Receipt::new(
                caller_did,
                crate::receipt::Operation::ContractExecute {
                    code_hash: call_params.code_hash.clone(),
                    rule: call_params.rule_name.clone(),
                },
                crate::receipt::Outcome::failure(e.to_string()),
            );
            state.receipt_store.insert(receipt).await;

            RpcResponse::error(id, -32000, format!("Contract execution failed: {e}"))
        }
    }
}

/// Handle contract.list RPC call - list all installed contracts (paginated)
async fn handle_contract_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let contract_runtime = match &state.contract_runtime {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Contract runtime not available".to_string());
        }
    };

    // Parse pagination parameters
    let page_request: crate::PageRequest =
        serde_json::from_value(params.clone()).unwrap_or_default();

    let runtime = contract_runtime.read().await;
    let contracts = runtime.list_contracts();

    // Convert from icn-ccl::ContractInfo to RPC ContractInfo format
    let contracts_rpc: Vec<crate::types::ContractInfo> = contracts
        .iter()
        .map(|info| crate::types::ContractInfo {
            code_hash: info.code_hash.to_hex(),
            name: info.name.clone(),
            participants: info
                .participants
                .iter()
                .map(|did| format!("{did:?}"))
                .collect(),
            currency: info.currency.clone(),
            rules: info.rules.clone(),
        })
        .collect();

    // Apply pagination
    let page = crate::paginate(contracts_rpc, &page_request, crate::DEFAULT_MAX_PAGE_SIZE);

    match serde_json::to_value(&page) {
        Ok(value) => RpcResponse::success(id, value),
        Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
    }
}

/// Handle ledger.quarantine.list RPC call - list all quarantined entries (paginated)
async fn handle_quarantine_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse pagination parameters
    let page_request: crate::PageRequest =
        serde_json::from_value(params.clone()).unwrap_or_default();

    let ledger = ledger_handle.read().await;
    match ledger.quarantine().list() {
        Ok(items) => {
            let items_json: Vec<serde_json::Value> = items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "entry_id": item.entry_id.to_hex(),
                        "reason": format!("{:?}", item.reason),
                        "author": format!("{:?}", item.author),
                        "observed_at": item.observed_at,
                        "metadata": item.metadata,
                    })
                })
                .collect();

            // Apply pagination
            let page = crate::paginate(items_json, &page_request, crate::DEFAULT_MAX_PAGE_SIZE);

            match serde_json::to_value(&page) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list quarantine: {e}")),
    }
}

/// Handle ledger.quarantine.get RPC call - get a specific quarantined entry
async fn handle_quarantine_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct GetParams {
        entry_id: String,
    }

    let get_params: GetParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry ID
    let hash_bytes = match hex::decode(&get_params.entry_id) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            return RpcResponse::error(id, -32602, "Invalid entry ID format".to_string());
        }
    };
    let entry_id = icn_ledger::ContentHash::from_bytes(hash_bytes);

    let ledger = ledger_handle.read().await;
    match ledger.quarantine().get(&entry_id) {
        Ok(Some((entry, item))) => {
            let result = serde_json::json!({
                "entry": {
                    "id": entry.id.map(|id| id.to_hex()),
                    "author": format!("{:?}", entry.author),
                    "parents": entry.parents.iter().map(|p| p.to_hex()).collect::<Vec<_>>(),
                    "timestamp": entry.timestamp,
                    "num_accounts": entry.accounts.len(),
                },
                "quarantine_info": {
                    "entry_id": item.entry_id.to_hex(),
                    "reason": format!("{:?}", item.reason),
                    "author": format!("{:?}", item.author),
                    "observed_at": item.observed_at,
                    "metadata": item.metadata,
                }
            });
            RpcResponse::success(id, result)
        }
        Ok(None) => RpcResponse::error(id, -32000, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get quarantine entry: {e}")),
    }
}

/// Handle ledger.quarantine.release RPC call - release an entry for retry
async fn handle_quarantine_release(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct ReleaseParams {
        entry_id: String,
    }

    let release_params: ReleaseParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry ID
    let hash_bytes = match hex::decode(&release_params.entry_id) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            return RpcResponse::error(id, -32602, "Invalid entry ID format".to_string());
        }
    };
    let entry_id = icn_ledger::ContentHash::from_bytes(hash_bytes);

    let mut ledger = ledger_handle.write().await;
    match ledger.quarantine_mut().release(&entry_id) {
        Ok(Some(entry)) => {
            // Try to append the released entry back to the ledger
            // The intent of "release" is to retry the entry, so if reappend fails,
            // the operation has not fully succeeded and should return an error.
            match ledger.append_entry(entry) {
                Ok(_) => RpcResponse::success(
                    id,
                    serde_json::json!({
                        "released": true,
                        "reappended": true,
                        "entry_id": entry_id.to_hex()
                    }),
                ),
                Err(e) => RpcResponse::error(
                    id,
                    -32000,
                    format!("Entry released from quarantine but reappend failed: {e}"),
                ),
            }
        }
        Ok(None) => RpcResponse::error(id, -32000, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to release entry: {e}")),
    }
}

/// Handle ledger.quarantine.drop RPC call - permanently drop an entry
async fn handle_quarantine_drop(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct DropParams {
        entry_id: String,
    }

    let drop_params: DropParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry ID
    let hash_bytes = match hex::decode(&drop_params.entry_id) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            return RpcResponse::error(id, -32602, "Invalid entry ID format".to_string());
        }
    };
    let entry_id = icn_ledger::ContentHash::from_bytes(hash_bytes);

    let mut ledger = ledger_handle.write().await;
    match ledger.quarantine_mut().drop(&entry_id) {
        Ok(true) => RpcResponse::success(
            id,
            serde_json::json!({
                "dropped": true,
                "entry_id": entry_id.to_hex()
            }),
        ),
        Ok(false) => RpcResponse::error(id, -32000, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to drop entry: {e}")),
    }
}

/// Handle ledger.quarantine.purge RPC call - purge all expired entries
async fn handle_quarantine_purge(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    let mut ledger = ledger_handle.write().await;
    match ledger.quarantine_mut().purge_expired() {
        Ok(purged) => RpcResponse::success(
            id,
            serde_json::json!({
                "purged": purged
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to purge expired entries: {e}")),
    }
}

/// Handle receipt.get RPC call - get a receipt by ID
async fn handle_receipt_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    // Parse parameters
    #[derive(serde::Deserialize)]
    struct GetReceiptParams {
        receipt_id: String,
    }

    let get_params: GetReceiptParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let receipt_id = crate::receipt::ReceiptId::from_string(get_params.receipt_id);

    match state.receipt_store.get(&receipt_id).await {
        Some(receipt) => match serde_json::to_value(&receipt) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
        },
        None => RpcResponse::error(id, -32000, "Receipt not found".to_string()),
    }
}

/// Handle governance.domain.list RPC call - list all governance domains
async fn handle_governance_domain_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    match governance_handle.list_domains().await {
        Ok(domains) => {
            let domain_infos: Vec<GovernanceDomainInfo> = domains
                .into_iter()
                .map(|d| GovernanceDomainInfo {
                    id: d.id.0,
                    name: d.name,
                    created_at: d.created_at,
                    profile: d.config.profile.0,
                    membership_type: match &d.config.membership.source {
                        icn_governance::MembershipSource::StaticList(_) => {
                            "static_list".to_string()
                        }
                        icn_governance::MembershipSource::TrustThreshold(_) => {
                            "trust_threshold".to_string()
                        }
                    },
                    params: GovernanceParamsInfo {
                        quorum_percentage: d.config.params.quorum_percentage,
                        approval_threshold_percentage: d
                            .config
                            .params
                            .approval_threshold_percentage,
                        voting_period_seconds: d.config.params.voting_period_seconds,
                    },
                })
                .collect();

            match serde_json::to_value(&domain_infos) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list domains: {e}")),
    }
}

/// Handle governance.domain.get RPC call - get a specific domain
async fn handle_governance_domain_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct DomainGetParams {
        domain_id: String,
    }

    let domain_params: DomainGetParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let domain_id = icn_governance::GovernanceDomainId(domain_params.domain_id);

    match governance_handle.get_domain(&domain_id).await {
        Ok(Some(d)) => {
            let domain_info = GovernanceDomainInfo {
                id: d.id.0,
                name: d.name,
                created_at: d.created_at,
                profile: d.config.profile.0,
                membership_type: match &d.config.membership.source {
                    icn_governance::MembershipSource::StaticList(_) => "static_list".to_string(),
                    icn_governance::MembershipSource::TrustThreshold(_) => {
                        "trust_threshold".to_string()
                    }
                },
                params: GovernanceParamsInfo {
                    quorum_percentage: d.config.params.quorum_percentage,
                    approval_threshold_percentage: d.config.params.approval_threshold_percentage,
                    voting_period_seconds: d.config.params.voting_period_seconds,
                },
            };

            match serde_json::to_value(&domain_info) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Ok(None) => RpcResponse::error(id, -32000, "Domain not found".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get domain: {e}")),
    }
}

/// Handle governance.proposal.list RPC call - list all proposals
async fn handle_governance_proposal_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    match governance_handle.list_proposals().await {
        Ok(proposals) => {
            let proposal_infos: Vec<ProposalInfo> = proposals
                .into_iter()
                .map(|p| {
                    let (state_str, opened_at, closes_at, closed_at) = match &p.state {
                        icn_governance::ProposalState::Draft => {
                            ("draft".to_string(), None, None, None)
                        }
                        icn_governance::ProposalState::Open {
                            opened_at,
                            closes_at,
                        } => ("open".to_string(), Some(*opened_at), Some(*closes_at), None),
                        icn_governance::ProposalState::Accepted { closed_at } => {
                            ("accepted".to_string(), None, None, Some(*closed_at))
                        }
                        icn_governance::ProposalState::Rejected { closed_at } => {
                            ("rejected".to_string(), None, None, Some(*closed_at))
                        }
                        icn_governance::ProposalState::NoQuorum { closed_at } => {
                            ("no_quorum".to_string(), None, None, Some(*closed_at))
                        }
                        icn_governance::ProposalState::Cancelled { cancelled_at } => {
                            ("cancelled".to_string(), None, None, Some(*cancelled_at))
                        }
                        icn_governance::ProposalState::Vetoed { vetoed_at, .. } => {
                            ("vetoed".to_string(), None, None, Some(*vetoed_at))
                        }
                        icn_governance::ProposalState::ForceClosed { closed_at, .. } => {
                            ("force_closed".to_string(), None, None, Some(*closed_at))
                        }
                    };

                    ProposalInfo {
                        id: p.id.0,
                        domain_id: p.domain_id.0,
                        proposer: p.proposer.to_string(),
                        title: p.title,
                        description: p.description,
                        state: state_str,
                        created_at: p.created_at,
                        updated_at: p.updated_at,
                        opened_at,
                        closes_at,
                        closed_at,
                    }
                })
                .collect();

            match serde_json::to_value(&proposal_infos) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list proposals: {e}")),
    }
}

/// Handle governance.proposal.get RPC call - get a specific proposal
async fn handle_governance_proposal_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct ProposalGetParams {
        proposal_id: String,
    }

    let proposal_params: ProposalGetParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(proposal_params.proposal_id);

    match governance_handle.get_proposal(&proposal_id).await {
        Ok(Some(p)) => {
            let (state_str, opened_at, closes_at, closed_at) = match &p.state {
                icn_governance::ProposalState::Draft => ("draft".to_string(), None, None, None),
                icn_governance::ProposalState::Open {
                    opened_at,
                    closes_at,
                } => ("open".to_string(), Some(*opened_at), Some(*closes_at), None),
                icn_governance::ProposalState::Accepted { closed_at } => {
                    ("accepted".to_string(), None, None, Some(*closed_at))
                }
                icn_governance::ProposalState::Rejected { closed_at } => {
                    ("rejected".to_string(), None, None, Some(*closed_at))
                }
                icn_governance::ProposalState::NoQuorum { closed_at } => {
                    ("no_quorum".to_string(), None, None, Some(*closed_at))
                }
                icn_governance::ProposalState::Cancelled { cancelled_at } => {
                    ("cancelled".to_string(), None, None, Some(*cancelled_at))
                }
                icn_governance::ProposalState::Vetoed { vetoed_at, .. } => {
                    ("vetoed".to_string(), None, None, Some(*vetoed_at))
                }
                icn_governance::ProposalState::ForceClosed { closed_at, .. } => {
                    ("force_closed".to_string(), None, None, Some(*closed_at))
                }
            };

            let proposal_info = ProposalInfo {
                id: p.id.0,
                domain_id: p.domain_id.0,
                proposer: p.proposer.to_string(),
                title: p.title,
                description: p.description,
                state: state_str,
                created_at: p.created_at,
                updated_at: p.updated_at,
                opened_at,
                closes_at,
                closed_at,
            };

            match serde_json::to_value(&proposal_info) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Ok(None) => RpcResponse::error(id, -32000, "Proposal not found".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get proposal: {e}")),
    }
}

/// Handle governance.domain.create RPC call - create a new governance domain
async fn handle_governance_domain_create(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    let request: CreateDomainRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Convert membership config
    let membership = match request.membership {
        MembershipConfigInfo::StaticList { members } => {
            let dids: Result<Vec<icn_identity::Did>, _> =
                members.iter().map(|s| s.parse()).collect();

            match dids {
                Ok(dids) => icn_governance::MembershipConfig::static_list(dids),
                Err(e) => {
                    return RpcResponse::error(
                        id,
                        -32602,
                        format!("Invalid DID in member list: {e}"),
                    );
                }
            }
        }
        MembershipConfigInfo::TrustThreshold { threshold } => {
            icn_governance::MembershipConfig::trust_threshold(threshold)
        }
    };

    let params = icn_governance::GovernanceParams {
        quorum_percentage: request.params.quorum_percentage,
        approval_threshold_percentage: request.params.approval_threshold_percentage,
        voting_period_seconds: request.params.voting_period_seconds,
    };

    let domain_id = icn_governance::GovernanceDomainId(request.domain_id);

    match governance_handle
        .create_domain(
            domain_id.clone(),
            request.name,
            request.profile,
            params,
            membership,
        )
        .await
    {
        Ok(()) => {
            let result = serde_json::json!({
                "success": true,
                "domain_id": domain_id.0
            });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to create domain: {e}")),
    }
}

/// Handle governance.proposal.create RPC call - create a new proposal
async fn handle_governance_proposal_create(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    let request: CreateProposalRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Convert payload
    let payload = match request.payload {
        ProposalPayloadInfo::Text { body } => icn_governance::ProposalPayload::Text { body },
        ProposalPayloadInfo::Budget {
            amount,
            currency,
            recipient,
            purpose,
        } => {
            let recipient_did = match recipient.parse::<icn_identity::Did>() {
                Ok(did) => did,
                Err(e) => {
                    return RpcResponse::error(id, -32602, format!("Invalid recipient DID: {e}"));
                }
            };
            icn_governance::ProposalPayload::Budget {
                amount,
                currency,
                recipient: recipient_did,
                purpose,
            }
        }
        ProposalPayloadInfo::ConfigChange { new_config } => {
            icn_governance::ProposalPayload::ConfigChange { new_config }
        }
        ProposalPayloadInfo::Membership { action, member } => {
            let member_did = match member.parse::<icn_identity::Did>() {
                Ok(d) => d,
                Err(e) => {
                    return RpcResponse::error(id, -32602, format!("Invalid DID: {e}"));
                }
            };
            let action_enum = match action.as_str() {
                "add" => MembershipAction::Add,
                "remove" => MembershipAction::Remove,
                _ => {
                    return RpcResponse::error(
                        id,
                        -32602,
                        "Invalid action (must be 'add' or 'remove')".to_string(),
                    );
                }
            };
            icn_governance::ProposalPayload::Membership {
                action: action_enum,
                member: member_did,
            }
        }
    };

    let domain_id = icn_governance::GovernanceDomainId(request.domain_id);

    match governance_handle
        .create_proposal(domain_id, request.title, request.description, payload)
        .await
    {
        Ok(proposal_id) => {
            let response = CreateProposalResponse {
                proposal_id: proposal_id.0,
            };
            match serde_json::to_value(&response) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to create proposal: {e}")),
    }
}

/// Handle governance.proposal.open RPC call - open a proposal for voting
async fn handle_governance_proposal_open(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    let request: OpenProposalRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(request.proposal_id);

    match governance_handle
        .open_proposal(proposal_id, request.voting_period_seconds)
        .await
    {
        Ok(()) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to open proposal: {e}")),
    }
}

/// Handle governance.vote.cast RPC call - cast a vote on a proposal
async fn handle_governance_vote_cast(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    let request: CastVoteRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(request.proposal_id);

    let choice = match request.choice.as_str() {
        "for" => icn_governance::VoteChoice::For,
        "against" => icn_governance::VoteChoice::Against,
        "abstain" => icn_governance::VoteChoice::Abstain,
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid vote choice (must be 'for', 'against', or 'abstain')".to_string(),
            );
        }
    };

    match governance_handle
        .cast_vote(proposal_id, choice, request.comment)
        .await
    {
        Ok(()) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to cast vote: {e}")),
    }
}

/// Handle governance.proposal.close RPC call - close a proposal
async fn handle_governance_proposal_close(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let governance_handle = match &state.governance_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Governance not available".to_string());
        }
    };

    let request: CloseProposalRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(request.proposal_id);

    match governance_handle.close_proposal(proposal_id).await {
        Ok(()) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to close proposal: {e}")),
    }
}

/// Handle compute.submit RPC call - submit a task for distributed execution
async fn handle_compute_submit(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    use base64::Engine;

    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    let request: SubmitTaskRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Get authenticated submitter DID (or fallback for unauthenticated dev mode)
    let submitter = claims
        .as_ref()
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "rpc:anonymous".to_string());

    // Get coop_id: prefer request, fallback to JWT claims
    let coop_id = request
        .coop_id
        .clone()
        .or_else(|| claims.as_ref().and_then(|c| c.coop_id.clone()));

    // Convert resource_profile from request to compute ResourceProfile
    let resource_profile = request.resource_profile.as_ref().map(|rp| {
        icn_compute::ResourceProfile {
            cpu_cores: rp.cpu_cores,
            memory_mb: rp.memory_mb,
            storage_mb: rp.storage_mb,
            network_mbps: rp.network_mbps,
            gpu_spec: None,          // GPU not supported via RPC yet
            duration_estimate: None, // Duration estimation not supported via RPC yet
        }
    });

    // Build compute task
    let inputs = if request.inputs.is_null() {
        vec![]
    } else {
        serde_json::to_vec(&request.inputs).unwrap_or_default()
    };

    // Parse priority string (case-insensitive)
    let priority = match request.priority.to_lowercase().as_str() {
        "low" => icn_compute::TaskPriority::Low,
        "normal" => icn_compute::TaskPriority::Normal,
        "high" => icn_compute::TaskPriority::High,
        "critical" => icn_compute::TaskPriority::Critical,
        _ => icn_compute::TaskPriority::Normal, // Default to normal for invalid values
    };

    // Build TaskCode based on code_type
    let (task_code, required_capabilities) = match request.code_type {
        CodeType::Ccl => {
            let code = match request.code {
                Some(c) => c,
                None => {
                    return RpcResponse::error(
                        id,
                        -32602,
                        "Missing 'code' field for CCL task".to_string(),
                    );
                }
            };
            (
                icn_compute::TaskCode::Ccl(code),
                vec![icn_compute::ExecutorCapability::Ccl],
            )
        }
        CodeType::Wasm => {
            let wasm_b64 = match &request.wasm_bytes {
                Some(b) => b,
                None => {
                    return RpcResponse::error(
                        id,
                        -32602,
                        "Missing 'wasm_bytes' field for WASM task".to_string(),
                    );
                }
            };
            let wasm_bytes = match base64::engine::general_purpose::STANDARD.decode(wasm_b64) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return RpcResponse::error(id, -32602, format!("Invalid base64: {e}"));
                }
            };
            (
                icn_compute::TaskCode::WasmInline(wasm_bytes),
                vec![icn_compute::ExecutorCapability::Wasm],
            )
        }
    };

    let task = icn_compute::ComputeTask {
        id: request.task_id,
        submitter, // Authenticated DID from JWT claims
        coop_id,   // From request or JWT claims
        code: task_code,
        inputs,
        fuel_limit: icn_compute::FuelLimit(request.fuel_limit),
        required_capabilities,
        priority,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        deadline: request.deadline_ms,
        payment_rate: request.payment_rate,
        payment_currency: request.payment_currency,
        resource_profile,            // From request
        actor_mode: None,            // Not actor mode (Phase 16D)
        placement_constraints: None, // No constraints from RPC (Phase 16E will set from policy)
    };

    match compute_handle.submit(task).await {
        Ok(hash) => {
            let response = SubmitTaskResponse {
                task_hash: hex::encode(hash),
            };
            RpcResponse::success(id, serde_json::to_value(response).unwrap())
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to submit task: {e}")),
    }
}

/// Handle compute.status RPC call - get task status
async fn handle_compute_status(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct StatusParams {
        task_hash: String,
    }

    let params: StatusParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse hash
    let hash_bytes = match hex::decode(&params.task_hash) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid task_hash (expected 32 hex bytes)".to_string(),
            );
        }
    };

    match compute_handle.status(hash_bytes).await {
        Ok(Some(status)) => {
            let (status_str, executor, result) = match status {
                icn_compute::TaskStatus::Pending => ("pending".to_string(), None, None),
                icn_compute::TaskStatus::Claimed { executor, .. } => {
                    ("claimed".to_string(), Some(executor), None)
                }
                icn_compute::TaskStatus::Completed { result } => {
                    let (outcome, output, error) = match &result.outcome {
                        icn_compute::ExecutionOutcome::Success(data) => {
                            let output = serde_json::from_slice(data).ok();
                            ("success".to_string(), output, None)
                        }
                        icn_compute::ExecutionOutcome::Failed(e) => {
                            ("failed".to_string(), None, Some(e.clone()))
                        }
                        icn_compute::ExecutionOutcome::OutOfFuel => {
                            ("out_of_fuel".to_string(), None, None)
                        }
                        icn_compute::ExecutionOutcome::Timeout => {
                            ("timeout".to_string(), None, None)
                        }
                    };
                    let result_info = TaskResultInfo {
                        outcome,
                        output,
                        error,
                        fuel_used: result.fuel_used,
                        duration_ms: result.duration_ms,
                    };
                    (
                        "completed".to_string(),
                        Some(result.executor.clone()),
                        Some(result_info),
                    )
                }
                icn_compute::TaskStatus::Failed { reason } => {
                    let result_info = TaskResultInfo {
                        outcome: "failed".to_string(),
                        output: None,
                        error: Some(reason),
                        fuel_used: 0,
                        duration_ms: 0,
                    };
                    ("failed".to_string(), None, Some(result_info))
                }
                icn_compute::TaskStatus::Cancelled { reason, .. } => {
                    let result_info = TaskResultInfo {
                        outcome: "cancelled".to_string(),
                        output: None,
                        error: Some(reason),
                        fuel_used: 0,
                        duration_ms: 0,
                    };
                    ("cancelled".to_string(), None, Some(result_info))
                }
            };

            let info = TaskStatusInfo {
                task_hash: params.task_hash,
                status: status_str,
                executor,
                result,
            };
            RpcResponse::success(id, serde_json::to_value(info).unwrap())
        }
        Ok(None) => RpcResponse::error(id, -32000, "Task not found".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get status: {e}")),
    }
}

/// Handle compute.cancel RPC call - cancel a task
async fn handle_compute_cancel(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct CancelParams {
        task_hash: String,
        #[serde(default)]
        reason: Option<String>,
    }

    let params: CancelParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse hash
    let hash_bytes = match hex::decode(&params.task_hash) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid task_hash (expected 32 hex bytes)".to_string(),
            );
        }
    };

    let reason = params
        .reason
        .unwrap_or_else(|| "Cancelled by submitter".to_string());

    // Get authenticated caller DID (or fallback for unauthenticated dev mode)
    let caller_did = claims.map(|c| c.sub.as_str()).unwrap_or("rpc:anonymous");

    match compute_handle
        .cancel_task(&hash_bytes, caller_did, reason)
        .await
    {
        Ok(_) => {
            #[derive(serde::Serialize)]
            struct CancelResponse {
                task_hash: String,
                status: String,
            }
            let response = CancelResponse {
                task_hash: params.task_hash,
                status: "cancelled".to_string(),
            };
            RpcResponse::success(id, serde_json::to_value(response).unwrap())
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to cancel task: {e}")),
    }
}

/// Handle policy.set RPC call - set or update a cooperative scheduling policy
async fn handle_policy_set(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct SetPolicyParams {
        /// Cooperative ID (reserved for future per-coop policy support)
        #[allow(dead_code)]
        coop_id: String,
        policy: serde_json::Value,
    }

    let params: SetPolicyParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse policy JSON
    let policy: icn_compute::CoopSchedulingPolicy = match serde_json::from_value(params.policy) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid policy: {e}"));
        }
    };

    match compute_handle.set_policy(policy).await {
        Ok(_) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to set policy: {e}")),
    }
}

/// Handle policy.get RPC call - get policy for a cooperative
async fn handle_policy_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct GetPolicyParams {
        coop_id: String,
    }

    let params: GetPolicyParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    match compute_handle.get_policy(&params.coop_id).await {
        Some(policy) => {
            let result = serde_json::to_value(policy).unwrap();
            RpcResponse::success(id, result)
        }
        None => RpcResponse::success(id, serde_json::Value::Null),
    }
}

/// Handle policy.list RPC call - list all policies
async fn handle_policy_list(
    id: u64,
    _params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    let policies = compute_handle.list_policies().await;
    let result = serde_json::to_value(policies).unwrap();
    RpcResponse::success(id, result)
}

/// Handle policy.remove RPC call - remove a policy
async fn handle_policy_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct RemovePolicyParams {
        coop_id: String,
    }

    let params: RemovePolicyParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    match compute_handle.remove_policy(&params.coop_id).await {
        Some(policy) => {
            let result = serde_json::to_value(policy).unwrap();
            RpcResponse::success(id, result)
        }
        None => RpcResponse::success(id, serde_json::Value::Null),
    }
}

/// Handle quota.usage RPC call - get usage for a member
async fn handle_quota_usage(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct UsageParams {
        coop_id: String,
        member_did: String,
    }

    let params: UsageParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    match compute_handle
        .get_usage(&params.coop_id, &params.member_did)
        .await
    {
        Ok(usage) => {
            let result = serde_json::to_value(usage).unwrap();
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get usage: {e}")),
    }
}

/// Handle quota.list RPC call - list all usage for a cooperative
async fn handle_quota_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match &state.compute_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct ListUsageParams {
        coop_id: String,
    }

    let params: ListUsageParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    match compute_handle.list_coop_usage(&params.coop_id).await {
        Ok(usage_records) => {
            let result = serde_json::to_value(usage_records).unwrap();
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list usage: {e}")),
    }
}

// ============================================================================
// Trust handlers
// ============================================================================

/// Handle trust.add RPC call - add a trust edge
async fn handle_trust_add(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let trust_graph = match &state.trust_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Trust graph not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct AddTrustParams {
        target_did: String,
        score: f64,
        label: Option<String>,
    }

    let params: AddTrustParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Validate score
    if !(0.0..=1.0).contains(&params.score) {
        return RpcResponse::error(id, -32602, "Score must be between 0.0 and 1.0".to_string());
    }

    // Parse target DID
    let target_did = match Did::from_str(&params.target_did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid target DID: {e}"));
        }
    };

    let mut graph = trust_graph.write().await;
    let own_did = graph.own_did().clone();

    // Create the trust edge
    let mut edge = TrustEdge::new(own_did, target_did, params.score);
    if let Some(label) = params.label {
        edge = edge.with_label(label);
    }

    match graph.add_edge(edge) {
        Ok(()) => RpcResponse::success(id, serde_json::json!({"success": true})),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to add trust edge: {e}")),
    }
}

/// Handle trust.remove RPC call - remove a trust edge
async fn handle_trust_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let trust_graph = match &state.trust_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Trust graph not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct RemoveTrustParams {
        target_did: String,
    }

    let params: RemoveTrustParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse target DID
    let target_did = match Did::from_str(&params.target_did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid target DID: {e}"));
        }
    };

    let mut graph = trust_graph.write().await;
    let own_did = graph.own_did().clone();
    match graph.remove_edge(&own_did, &target_did) {
        Ok(()) => RpcResponse::success(id, serde_json::json!({"success": true})),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to remove trust edge: {e}")),
    }
}

/// Handle trust.list RPC call - list outgoing trust edges
async fn handle_trust_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let trust_graph = match &state.trust_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Trust graph not available".to_string());
        }
    };

    let graph = trust_graph.read().await;
    let own_did = graph.own_did().clone();
    match graph.get_outgoing_edges(&own_did) {
        Ok(edges) => {
            let result: Vec<serde_json::Value> = edges
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "target_did": e.target.to_string(),
                        "score": e.score,
                        "labels": e.labels,
                    })
                })
                .collect();
            RpcResponse::success(id, serde_json::json!(result))
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list trust edges: {e}")),
    }
}

/// Handle trust.compute RPC call - compute trust score for a target DID
async fn handle_trust_compute(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let trust_graph = match &state.trust_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Trust graph not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct ComputeTrustParams {
        target_did: String,
    }

    let params: ComputeTrustParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse target DID
    let target_did = match Did::from_str(&params.target_did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid target DID: {e}"));
        }
    };

    let graph = trust_graph.read().await;
    match graph.compute_trust_score(&target_did) {
        Ok(score) => RpcResponse::success(id, serde_json::json!({"score": score})),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to compute trust: {e}")),
    }
}

// ============================================================================
// Recovery handlers
// ============================================================================

/// Handle recovery.initiate RPC call - initiate social recovery for a lost identity
async fn handle_recovery_initiate(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    // Get the new DID from authenticated claims
    let new_did_str = match claims {
        Some(c) => &c.sub,
        None => {
            return RpcResponse::error(id, -32001, "Authentication required".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct InitiateParams {
        old_did: String,
        threshold: usize,
        #[serde(default = "default_delay_period")]
        delay_period: u64,
    }

    fn default_delay_period() -> u64 {
        86400 // 24 hours
    }

    let params: InitiateParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse DIDs
    let old_did = match Did::from_str(&params.old_did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid old_did: {e}"));
        }
    };

    let new_did = match Did::from_str(new_did_str) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid new_did: {e}"));
        }
    };

    // Create recovery event
    let recovery = RecoveryEvent::new(old_did, new_did, params.threshold, params.delay_period);

    // Save to store
    let recovery_key = format!("recovery:{}", recovery.id);
    let recovery_json = match serde_json::to_vec(&recovery) {
        Ok(j) => j,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to serialize recovery: {e}"));
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::error(id, -32000, format!("Failed to save recovery: {e}"));
    }

    info!(
        "Recovery initiated: {} -> {} (id: {})",
        recovery.old_did, recovery.new_did, recovery.id
    );

    RpcResponse::success(
        id,
        serde_json::json!({
            "recovery_id": recovery.id,
            "status": recovery.progress_summary(),
        }),
    )
}

/// Handle recovery.attest RPC call - sign a recovery attestation as a trustee
async fn handle_recovery_attest(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    let keypair = match &state.own_keypair {
        Some(kp) => kp.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Keypair not configured".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct AttestParams {
        recovery_id: String,
        verification_method: String,
    }

    let params: AttestParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Load recovery event
    let recovery_key = format!("recovery:{}", params.recovery_id);
    let recovery_data = match store.get(recovery_key.as_bytes()) {
        Ok(Some(d)) => d,
        Ok(None) => {
            return RpcResponse::error(id, -32000, "Recovery not found".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to load recovery: {e}"));
        }
    };

    let mut recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to parse recovery: {e}"));
        }
    };

    // Create attestation
    let attestation = match RecoveryAttestation::new(
        &keypair,
        recovery.old_did.clone(),
        recovery.new_did.clone(),
        params.verification_method.clone(),
    ) {
        Ok(a) => a,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to create attestation: {e}"));
        }
    };

    // Add attestation to recovery
    let threshold_reached = match recovery.add_attestation(attestation) {
        Ok(t) => t,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to add attestation: {e}"));
        }
    };

    // Save updated recovery
    let recovery_json = match serde_json::to_vec(&recovery) {
        Ok(j) => j,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to serialize recovery: {e}"));
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::error(id, -32000, format!("Failed to save recovery: {e}"));
    }

    info!(
        "Attestation added to recovery {}: trustee={}",
        params.recovery_id,
        keypair.did()
    );

    RpcResponse::success(
        id,
        serde_json::json!({
            "threshold_reached": threshold_reached,
            "status": recovery.progress_summary(),
        }),
    )
}

/// Handle recovery.list RPC call - list all recovery events
async fn handle_recovery_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    let items = match store.scan(b"recovery:") {
        Ok(i) => i,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to scan recoveries: {e}"));
        }
    };

    let mut recoveries: Vec<RecoveryEventInfo> = Vec::new();

    for (_key, value) in items {
        let recovery: RecoveryEvent = match serde_json::from_slice(&value) {
            Ok(r) => r,
            Err(_) => continue,
        };

        recoveries.push(recovery_to_info(&recovery));
    }

    RpcResponse::success(id, serde_json::to_value(recoveries).unwrap())
}

/// Handle recovery.status RPC call - get status of a specific recovery
async fn handle_recovery_status(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct StatusParams {
        recovery_id: String,
    }

    let params: StatusParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Load recovery event
    let recovery_key = format!("recovery:{}", params.recovery_id);
    let recovery_data = match store.get(recovery_key.as_bytes()) {
        Ok(Some(d)) => d,
        Ok(None) => {
            return RpcResponse::error(id, -32000, "Recovery not found".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to load recovery: {e}"));
        }
    };

    let recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to parse recovery: {e}"));
        }
    };

    // Include full attestation info
    let attestations: Vec<RecoveryAttestationInfo> = recovery
        .attestations
        .iter()
        .map(|a| RecoveryAttestationInfo {
            trustee: a.trustee.to_string(),
            verification_method: a.verification_method.clone(),
            timestamp: a.timestamp,
        })
        .collect();

    let info = recovery_to_info(&recovery);
    let result = serde_json::json!({
        "id": info.id,
        "old_did": info.old_did,
        "new_did": info.new_did,
        "initiated_at": info.initiated_at,
        "finalized_at": info.finalized_at,
        "threshold": info.threshold,
        "delay_period": info.delay_period,
        "status": info.status,
        "attestations_count": info.attestations_count,
        "progress_summary": info.progress_summary,
        "attestations": attestations,
    });

    RpcResponse::success(id, result)
}

/// Handle recovery.finalize RPC call - finalize a recovery after threshold + delay
async fn handle_recovery_finalize(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct FinalizeParams {
        recovery_id: String,
    }

    let params: FinalizeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Load recovery event
    let recovery_key = format!("recovery:{}", params.recovery_id);
    let recovery_data = match store.get(recovery_key.as_bytes()) {
        Ok(Some(d)) => d,
        Ok(None) => {
            return RpcResponse::error(id, -32000, "Recovery not found".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to load recovery: {e}"));
        }
    };

    let mut recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to parse recovery: {e}"));
        }
    };

    // Check if delay expired
    recovery.check_delay_expired();

    // Finalize
    if let Err(e) = recovery.finalize() {
        return RpcResponse::error(id, -32000, format!("Failed to finalize: {e}"));
    }

    // Save updated recovery
    let recovery_json = match serde_json::to_vec(&recovery) {
        Ok(j) => j,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to serialize recovery: {e}"));
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::error(id, -32000, format!("Failed to save recovery: {e}"));
    }

    info!(
        "Recovery finalized: {} -> {}",
        recovery.old_did, recovery.new_did
    );

    RpcResponse::success(
        id,
        serde_json::json!({
            "finalized": true,
            "old_did": recovery.old_did.to_string(),
            "new_did": recovery.new_did.to_string(),
        }),
    )
}

/// Handle recovery.cancel RPC call - cancel a recovery (fraud detection)
async fn handle_recovery_cancel(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    // Get the canceller DID from authenticated claims
    let canceller_did_str = match claims {
        Some(c) => &c.sub,
        None => {
            return RpcResponse::error(id, -32001, "Authentication required".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct CancelParams {
        recovery_id: String,
        reason: String,
    }

    let params: CancelParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse canceller DID
    let canceller_did = match Did::from_str(canceller_did_str) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid canceller DID: {e}"));
        }
    };

    // Load recovery event
    let recovery_key = format!("recovery:{}", params.recovery_id);
    let recovery_data = match store.get(recovery_key.as_bytes()) {
        Ok(Some(d)) => d,
        Ok(None) => {
            return RpcResponse::error(id, -32000, "Recovery not found".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to load recovery: {e}"));
        }
    };

    let mut recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to parse recovery: {e}"));
        }
    };

    // Cancel
    if let Err(e) = recovery.cancel(canceller_did.clone(), params.reason.clone()) {
        return RpcResponse::error(id, -32000, format!("Failed to cancel: {e}"));
    }

    // Save updated recovery
    let recovery_json = match serde_json::to_vec(&recovery) {
        Ok(j) => j,
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to serialize recovery: {e}"));
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::error(id, -32000, format!("Failed to save recovery: {e}"));
    }

    info!(
        "Recovery cancelled: {} by {} (reason: {})",
        params.recovery_id, canceller_did, params.reason
    );

    RpcResponse::success(
        id,
        serde_json::json!({
            "cancelled": true,
            "cancelled_by": canceller_did.to_string(),
            "reason": params.reason,
        }),
    )
}

/// Convert RecoveryEvent to RecoveryEventInfo
fn recovery_to_info(recovery: &RecoveryEvent) -> RecoveryEventInfo {
    let status = match &recovery.status {
        RecoveryStatus::Pending { .. } => "pending",
        RecoveryStatus::Delayed { .. } => "delayed",
        RecoveryStatus::ReadyToFinalize => "ready",
        RecoveryStatus::Finalized => "finalized",
        RecoveryStatus::Cancelled { .. } => "cancelled",
    };

    RecoveryEventInfo {
        id: recovery.id.clone(),
        old_did: recovery.old_did.to_string(),
        new_did: recovery.new_did.to_string(),
        initiated_at: recovery.initiated_at,
        finalized_at: recovery.finalized_at,
        threshold: recovery.threshold,
        delay_period: recovery.delay_period,
        status: status.to_string(),
        attestations_count: recovery.attestations.len(),
        progress_summary: recovery.progress_summary(),
    }
}

// ============================================================================
// Dispute handlers (ledger entry disputes)
// ============================================================================

/// Handle dispute.file RPC call - file a dispute against a ledger entry
async fn handle_dispute_file(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match &state.dispute_manager {
        Some(dm) => dm.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Dispute manager not configured".to_string());
        }
    };

    // Get authenticated DID (filer)
    let filer_did_str = claims
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "rpc:anonymous".to_string());

    #[derive(serde::Deserialize)]
    struct FileDisputeParams {
        entry_hash: String,
        reason: String,
    }

    let params: FileDisputeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse filer DID
    let filer_did = match Did::from_str(&filer_did_str) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid filer DID: {e}"));
        }
    };

    // Parse entry hash (hex)
    let entry_hash = match hex::decode(&params.entry_hash) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            ContentHash::from_bytes(arr)
        }
        Ok(_) => {
            return RpcResponse::error(id, -32602, "Entry hash must be 32 bytes".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid entry hash hex: {e}"));
        }
    };

    // File dispute
    let filed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut manager = dispute_manager.write().await;
    match manager.file_dispute(
        entry_hash.clone(),
        filer_did.clone(),
        params.reason.clone(),
        filed_at,
    ) {
        Ok(_dispute) => {
            info!(
                "Dispute filed against entry {} by {}",
                params.entry_hash, filer_did
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "entry_hash": params.entry_hash,
                    "filed_by": filer_did.to_string(),
                    "reason": params.reason,
                    "filed_at": filed_at,
                    "status": "contested",
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to file dispute: {e}")),
    }
}

/// Handle dispute.list RPC call - list disputes
async fn handle_dispute_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let dispute_manager = match &state.dispute_manager {
        Some(dm) => dm.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Dispute manager not configured".to_string());
        }
    };

    #[derive(serde::Deserialize, Default)]
    struct ListParams {
        status: Option<String>, // "pending", "resolved", "all"
        filer: Option<String>,  // Filter by filer DID
    }

    let params: ListParams = serde_json::from_value(params.clone()).unwrap_or_default();

    let manager = dispute_manager.read().await;

    // Get disputes based on filter
    let disputes: Vec<&Dispute> = if let Some(filer_str) = &params.filer {
        match Did::from_str(filer_str) {
            Ok(filer_did) => manager.get_disputes_by_filer(&filer_did),
            Err(_) => return RpcResponse::error(id, -32602, "Invalid filer DID".to_string()),
        }
    } else {
        manager.get_active_disputes()
    };

    // Filter by status if specified
    let filtered_disputes: Vec<&Dispute> = match params.status.as_deref() {
        Some("pending") => disputes
            .into_iter()
            .filter(|d| matches!(d.status, DisputeStatus::Contested { .. }))
            .collect(),
        Some("resolved") => disputes
            .into_iter()
            .filter(|d| matches!(d.status, DisputeStatus::Resolved { .. }))
            .collect(),
        Some("all") | None => disputes,
        Some(other) => {
            return RpcResponse::error(
                id,
                -32602,
                format!("Invalid status filter '{other}': must be pending, resolved, or all"),
            );
        }
    };

    // Convert to JSON-serializable format
    let disputes_json: Vec<serde_json::Value> = filtered_disputes
        .iter()
        .map(|d| dispute_to_json(d))
        .collect();

    RpcResponse::success(
        id,
        serde_json::json!({
            "disputes": disputes_json,
            "count": disputes_json.len(),
        }),
    )
}

/// Handle dispute.get RPC call - get details of a specific dispute
async fn handle_dispute_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let dispute_manager = match &state.dispute_manager {
        Some(dm) => dm.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Dispute manager not configured".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct GetParams {
        entry_hash: String,
    }

    let params: GetParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry hash
    let entry_hash = match hex::decode(&params.entry_hash) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            ContentHash::from_bytes(arr)
        }
        Ok(_) => {
            return RpcResponse::error(id, -32602, "Entry hash must be 32 bytes".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid entry hash hex: {e}"));
        }
    };

    let manager = dispute_manager.read().await;
    match manager.get_dispute(&entry_hash) {
        Some(dispute) => RpcResponse::success(id, dispute_to_json(dispute)),
        None => RpcResponse::error(id, -32000, "Dispute not found".to_string()),
    }
}

/// Handle dispute.add_evidence RPC call - add evidence to a dispute
async fn handle_dispute_add_evidence(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match &state.dispute_manager {
        Some(dm) => dm.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Dispute manager not configured".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct AddEvidenceParams {
        entry_hash: String,
        evidence: String,
    }

    let params: AddEvidenceParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry hash
    let entry_hash = match hex::decode(&params.entry_hash) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            ContentHash::from_bytes(arr)
        }
        Ok(_) => {
            return RpcResponse::error(id, -32602, "Entry hash must be 32 bytes".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid entry hash hex: {e}"));
        }
    };

    let mut manager = dispute_manager.write().await;
    match manager.add_evidence(&entry_hash, params.evidence.clone()) {
        Ok(()) => {
            info!("Evidence added to dispute {}", params.entry_hash);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "entry_hash": params.entry_hash,
                    "evidence_added": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to add evidence: {e}")),
    }
}

/// Handle dispute.assign_mediator RPC call - assign a mediator to a dispute
async fn handle_dispute_assign_mediator(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match &state.dispute_manager {
        Some(dm) => dm.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Dispute manager not configured".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct AssignMediatorParams {
        entry_hash: String,
        mediator: String,
    }

    let params: AssignMediatorParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry hash
    let entry_hash = match hex::decode(&params.entry_hash) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            ContentHash::from_bytes(arr)
        }
        Ok(_) => {
            return RpcResponse::error(id, -32602, "Entry hash must be 32 bytes".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid entry hash hex: {e}"));
        }
    };

    // Parse mediator DID
    let mediator_did = match Did::from_str(&params.mediator) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid mediator DID: {e}"));
        }
    };

    let mut manager = dispute_manager.write().await;
    match manager.assign_mediator(&entry_hash, mediator_did.clone()) {
        Ok(()) => {
            info!(
                "Mediator {} assigned to dispute {}",
                params.mediator, params.entry_hash
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "entry_hash": params.entry_hash,
                    "mediator": params.mediator,
                    "assigned": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to assign mediator: {e}")),
    }
}

/// Handle dispute.resolve RPC call - resolve a dispute (mediator only)
async fn handle_dispute_resolve(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match &state.dispute_manager {
        Some(dm) => dm.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Dispute manager not configured".to_string());
        }
    };

    // Get authenticated DID (mediator)
    let mediator_did_str = claims
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "rpc:anonymous".to_string());

    #[derive(serde::Deserialize)]
    struct ResolveParams {
        entry_hash: String,
        outcome: String, // "upheld", "reversed", "settlement", "writeoff"
        #[serde(default)]
        terms: Option<String>, // For settlement outcome
        #[serde(default)]
        reason: Option<String>, // For writeoff outcome
    }

    let params: ResolveParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse mediator DID
    let mediator_did = match Did::from_str(&mediator_did_str) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid mediator DID: {e}"));
        }
    };

    // Parse entry hash
    let entry_hash = match hex::decode(&params.entry_hash) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            ContentHash::from_bytes(arr)
        }
        Ok(_) => {
            return RpcResponse::error(id, -32602, "Entry hash must be 32 bytes".to_string());
        }
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid entry hash hex: {e}"));
        }
    };

    // Parse outcome
    let outcome = match params.outcome.to_lowercase().as_str() {
        "upheld" => DisputeOutcome::Upheld,
        "reversed" => DisputeOutcome::Reversed,
        "settlement" => DisputeOutcome::Settlement {
            terms: params
                .terms
                .unwrap_or_else(|| "Settlement agreed".to_string()),
            replacement_entry: None,
        },
        "writeoff" => DisputeOutcome::WriteOff {
            reason: params
                .reason
                .unwrap_or_else(|| "Debt written off".to_string()),
        },
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid outcome: must be upheld, reversed, settlement, or writeoff".to_string(),
            );
        }
    };

    let resolved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut manager = dispute_manager.write().await;
    match manager.resolve_dispute(
        &entry_hash,
        mediator_did.clone(),
        outcome.clone(),
        resolved_at,
    ) {
        Ok(()) => {
            info!(
                "Dispute {} resolved by {} with outcome {:?}",
                params.entry_hash, mediator_did, outcome
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "entry_hash": params.entry_hash,
                    "mediator": mediator_did.to_string(),
                    "outcome": params.outcome,
                    "resolved_at": resolved_at,
                    "resolved": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to resolve dispute: {e}")),
    }
}

/// Convert a Dispute to JSON
fn dispute_to_json(dispute: &Dispute) -> serde_json::Value {
    let status = match &dispute.status {
        icn_ledger::DisputeStatus::Normal => "normal",
        icn_ledger::DisputeStatus::Contested { .. } => "contested",
        icn_ledger::DisputeStatus::Resolved { .. } => "resolved",
    };

    let (mediator, outcome, resolved_at) = match &dispute.status {
        icn_ledger::DisputeStatus::Resolved {
            mediator,
            outcome,
            resolved_at,
        } => (
            Some(mediator.to_string()),
            Some(format!("{outcome:?}")),
            Some(*resolved_at),
        ),
        _ => (dispute.mediator.as_ref().map(|m| m.to_string()), None, None),
    };

    serde_json::json!({
        "entry_hash": dispute.entry_hash.to_hex(),
        "filed_by": dispute.filed_by.to_string(),
        "reason": dispute.reason,
        "filed_at": dispute.filed_at,
        "status": status,
        "evidence": dispute.evidence,
        "mediator": mediator,
        "outcome": outcome,
        "resolved_at": resolved_at,
    })
}

// ============================================================================
// Federation handlers
// ============================================================================

/// Handle federation.coop.list RPC call - list all known cooperatives
async fn handle_federation_coop_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    match registry.list() {
        Ok(coops) => {
            let coop_json: Vec<serde_json::Value> = coops
                .iter()
                .map(|c| coop_info_to_json(c))
                .collect();
            RpcResponse::success(id, serde_json::json!({ "cooperatives": coop_json }))
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list cooperatives: {e}")),
    }
}

/// Handle federation.coop.get RPC call - get a specific cooperative
async fn handle_federation_coop_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    match registry.get(&params.coop_id) {
        Ok(Some(coop)) => RpcResponse::success(id, coop_info_to_json(&coop)),
        Ok(None) => RpcResponse::error(
            id,
            -32000,
            format!("Cooperative not found: {}", params.coop_id),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get cooperative: {e}")),
    }
}

/// Handle federation.coop.register RPC call - register a new cooperative
async fn handle_federation_coop_register(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
        name: String,
        public_did: String,
        #[serde(default)]
        gateway_endpoints: Vec<String>,
        #[serde(default)]
        capabilities: Vec<String>,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Parse DID
    let public_did = match Did::from_str(&params.public_did) {
        Ok(d) => d,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}")),
    };

    // Build cooperative info
    let mut coop_info = icn_federation::CooperativeInfo::new(
        params.coop_id.clone(),
        params.name,
        public_did,
        icn_federation::FederationPolicy::Open,
    );

    for endpoint in params.gateway_endpoints {
        coop_info = coop_info.with_gateway(endpoint);
    }

    for capability in params.capabilities {
        coop_info = coop_info.with_capability(&capability);
    }

    match registry.register(coop_info) {
        Ok(()) => {
            info!("Registered cooperative: {}", params.coop_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "coop_id": params.coop_id,
                    "registered": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to register cooperative: {e}")),
    }
}

/// Handle federation.coop.remove RPC call - remove a cooperative
async fn handle_federation_coop_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    match registry.remove(&params.coop_id) {
        Ok(()) => {
            info!("Removed cooperative: {}", params.coop_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "coop_id": params.coop_id,
                    "removed": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to remove cooperative: {e}")),
    }
}

/// Handle federation.own.get RPC call - get own cooperative info
async fn handle_federation_own_get(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    let own_info = registry.own_coop_info();
    RpcResponse::success(id, coop_info_to_json(&own_info))
}

/// Handle federation.own.update RPC call - update own cooperative info
async fn handle_federation_own_update(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        gateway_endpoints: Option<Vec<String>>,
        #[serde(default)]
        capabilities: Option<Vec<String>>,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Get current own info and update fields
    let mut own_info = registry.own_coop_info();

    if let Some(name) = params.name {
        own_info.name = name;
    }
    if let Some(endpoints) = params.gateway_endpoints {
        own_info.gateway_endpoints = endpoints;
    }
    if let Some(caps) = params.capabilities {
        own_info.capabilities = caps;
    }

    match registry.update_own_info(own_info.clone()) {
        Ok(()) => {
            info!("Updated own cooperative info");
            RpcResponse::success(id, coop_info_to_json(&own_info))
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to update own info: {e}")),
    }
}

/// Handle federation.vouch.list RPC call - list vouches for a cooperative
async fn handle_federation_vouch_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    match registry.get_vouches(&params.coop_id) {
        Ok(vouches) => RpcResponse::success(
            id,
            serde_json::json!({
                "coop_id": params.coop_id,
                "vouchers": vouches,
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get vouches: {e}")),
    }
}

/// Handle federation.vouch.issue RPC call - issue a vouch for another cooperative
async fn handle_federation_vouch_issue(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        target_coop_id: String,
        #[serde(default = "default_trust_score")]
        trust_score: f64,
    }

    fn default_trust_score() -> f64 {
        0.5
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Get own coop info for the vouch
    let own_info = registry.own_coop_info();

    // Create the vouch
    let vouch = icn_federation::Vouch::new(
        own_info.coop_id.clone(),
        own_info.public_did.clone(),
        params.target_coop_id.clone(),
        params.trust_score,
    );

    // Sign if we have a keypair
    let signed_vouch = if let Some(keypair) = &state.own_keypair {
        vouch.sign(keypair)
    } else {
        vouch
    };

    match registry.add_vouch(&signed_vouch) {
        Ok(()) => {
            info!(
                "Issued vouch for {} with trust score {}",
                params.target_coop_id, params.trust_score
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "voucher_coop_id": own_info.coop_id,
                    "target_coop_id": params.target_coop_id,
                    "trust_score": params.trust_score,
                    "issued": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to issue vouch: {e}")),
    }
}

/// Handle federation.vouch.remove RPC call - remove a vouch
async fn handle_federation_vouch_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        target_coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_info = registry.own_coop_info();

    match registry.remove_vouch(&own_info.coop_id, &params.target_coop_id) {
        Ok(()) => {
            info!("Removed vouch for {}", params.target_coop_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "voucher_coop_id": own_info.coop_id,
                    "target_coop_id": params.target_coop_id,
                    "removed": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to remove vouch: {e}")),
    }
}

// ============================================================================
// Attestation handlers
// ============================================================================

/// Handle federation.attestation.list RPC call - list attestations for a member
async fn handle_federation_attestation_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::AttestationStore;

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        member_did: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let did = match Did::from_str(&params.member_did) {
        Ok(d) => d,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}")),
    };

    let att_store = AttestationStore::new(store);
    match att_store.get_valid_attestations_for(&did) {
        Ok(attestations) => {
            let atts_json: Vec<serde_json::Value> = attestations
                .iter()
                .map(|att| {
                    serde_json::json!({
                        "source_coop_id": att.source_coop_id,
                        "source_coop_did": att.source_coop_did.to_string(),
                        "member_did": att.member_did.to_string(),
                        "trust_score": att.trust_score,
                        "trust_context": format!("{:?}", att.trust_context),
                        "issued_at": att.issued_at,
                        "expires_at": att.expires_at,
                    })
                })
                .collect();

            RpcResponse::success(
                id,
                serde_json::json!({
                    "member_did": params.member_did,
                    "attestations": atts_json,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get attestations: {e}")),
    }
}

/// Handle federation.attestation.from RPC call - list attestations from a cooperative
async fn handle_federation_attestation_from(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::AttestationStore;

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let att_store = AttestationStore::new(store);
    match att_store.get_attestations_from(&params.coop_id) {
        Ok(attestations) => {
            let atts_json: Vec<serde_json::Value> = attestations
                .iter()
                .map(|att| {
                    serde_json::json!({
                        "source_coop_id": att.source_coop_id,
                        "source_coop_did": att.source_coop_did.to_string(),
                        "member_did": att.member_did.to_string(),
                        "trust_score": att.trust_score,
                        "trust_context": format!("{:?}", att.trust_context),
                        "issued_at": att.issued_at,
                        "expires_at": att.expires_at,
                    })
                })
                .collect();

            RpcResponse::success(
                id,
                serde_json::json!({
                    "coop_id": params.coop_id,
                    "attestations": atts_json,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get attestations: {e}")),
    }
}

/// Handle federation.attestation.issue RPC call - issue a new attestation
async fn handle_federation_attestation_issue(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    use icn_federation::{AttestationStore, FederatedTrustAttestation, TrustContext};

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        member_did: String,
        trust_score: f64,
        context: String,
        validity_days: u64,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Validate trust score
    if !(0.0..=1.0).contains(&params.trust_score) {
        return RpcResponse::error(id, -32602, "Trust score must be between 0.0 and 1.0".to_string());
    }

    let did = match Did::from_str(&params.member_did) {
        Ok(d) => d,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}")),
    };

    let trust_context = match params.context.to_lowercase().as_str() {
        "economic" => TrustContext::Economic,
        "governance" => TrustContext::Governance,
        "social" => TrustContext::Social,
        "general" => TrustContext::General,
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid context. Use: economic, governance, social, or general".to_string(),
            );
        }
    };

    let own_info = registry.own_coop_info();
    let validity_secs = params.validity_days * 24 * 60 * 60;

    let attestation = FederatedTrustAttestation::new(
        own_info.coop_id.clone(),
        own_info.public_did.clone(),
        did.clone(),
        params.trust_score,
        trust_context.clone(),
        validity_secs,
    );

    let att_store = AttestationStore::new(store);
    match att_store.store_attestation(attestation) {
        Ok(()) => {
            info!(
                "Issued attestation for {} with trust score {}",
                params.member_did, params.trust_score
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "source_coop_id": own_info.coop_id,
                    "member_did": params.member_did,
                    "trust_score": params.trust_score,
                    "context": params.context,
                    "validity_days": params.validity_days,
                    "issued": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to issue attestation: {e}")),
    }
}

// ============================================================================
// Clearing handlers
// ============================================================================

/// Handle federation.clearing.list RPC call - list clearing agreements
async fn handle_federation_clearing_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => return RpcResponse::error(id, -32000, format!("Failed to create clearing manager: {e}")),
    };

    let agreements = manager.list_agreements();
    let agreements_json: Vec<serde_json::Value> = agreements
        .iter()
        .map(|a| {
            serde_json::json!({
                "agreement_id": a.agreement_id,
                "coop_a": a.coop_a,
                "coop_b": a.coop_b,
                "max_imbalance": a.max_imbalance,
                "settlement_interval": format!("{:?}", a.settlement_interval),
                "signatures": a.signatures.len(),
            })
        })
        .collect();

    RpcResponse::success(
        id,
        serde_json::json!({
            "agreements": agreements_json,
        }),
    )
}

/// Handle federation.clearing.show RPC call - show clearing agreement details
async fn handle_federation_clearing_show(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => return RpcResponse::error(id, -32000, format!("Failed to create clearing manager: {e}")),
    };

    match manager.get_agreement(&params.agreement_id) {
        Ok(Some(agreement)) => RpcResponse::success(
            id,
            serde_json::json!({
                "agreement_id": agreement.agreement_id,
                "coop_a": agreement.coop_a,
                "coop_b": agreement.coop_b,
                "coop_a_did": agreement.coop_a_did.to_string(),
                "coop_b_did": agreement.coop_b_did.to_string(),
                "max_imbalance": agreement.max_imbalance,
                "settlement_interval": format!("{:?}", agreement.settlement_interval),
                "signatures": agreement.signatures.len(),
                "exchange_rates": agreement.exchange_rates,
            }),
        ),
        Ok(None) => RpcResponse::error(
            id,
            -32000,
            format!("Agreement '{}' not found", params.agreement_id),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get agreement: {e}")),
    }
}

/// Handle federation.clearing.create RPC call - create a new clearing agreement
async fn handle_federation_clearing_create(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    use icn_federation::{BilateralClearingAgreement, ClearingManager, SettlementInterval};

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
        partner_coop: String,
        max_imbalance: i64,
        settlement: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_info = registry.own_coop_info();
    let own_coop_id = own_info.coop_id.clone();

    // Get partner coop info
    let partner = match registry.get(&params.partner_coop) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return RpcResponse::error(
                id,
                -32000,
                format!("Partner cooperative '{}' not found", params.partner_coop),
            );
        }
        Err(e) => return RpcResponse::error(id, -32000, format!("Failed to get partner coop: {e}")),
    };

    let settlement_interval = match params.settlement.to_lowercase().as_str() {
        "daily" => SettlementInterval::Daily,
        "weekly" => SettlementInterval::Weekly,
        "monthly" => SettlementInterval::Monthly,
        "manual" => SettlementInterval::Manual,
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid settlement. Use: daily, weekly, monthly, or manual".to_string(),
            );
        }
    };

    let mut agreement = BilateralClearingAgreement::new(
        params.agreement_id.clone(),
        own_coop_id.clone(),
        own_info.public_did.clone(),
        params.partner_coop.clone(),
        partner.public_did.clone(),
    );
    agreement.max_imbalance = params.max_imbalance;
    agreement.settlement_interval = settlement_interval;

    let manager = match ClearingManager::new(store, own_coop_id.clone()) {
        Ok(m) => m,
        Err(e) => return RpcResponse::error(id, -32000, format!("Failed to create clearing manager: {e}")),
    };

    match manager.create_agreement(agreement) {
        Ok(_agreement_id) => {
            info!(
                "Created clearing agreement {} with {}",
                params.agreement_id, params.partner_coop
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "agreement_id": params.agreement_id,
                    "our_coop": own_coop_id,
                    "partner_coop": params.partner_coop,
                    "max_imbalance": params.max_imbalance,
                    "settlement": params.settlement,
                    "created": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to create agreement: {e}")),
    }
}

/// Handle federation.clearing.position RPC call - get clearing position
async fn handle_federation_clearing_position(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => return RpcResponse::error(id, -32000, format!("Failed to create clearing manager: {e}")),
    };

    match manager.calculate_position(&params.agreement_id) {
        Ok(position) => RpcResponse::success(
            id,
            serde_json::json!({
                "agreement_id": params.agreement_id,
                "coop_a_owes_b": position.coop_a_owes_b,
                "coop_b_owes_a": position.coop_b_owes_a,
                "net_position": position.net_position(),
                "pending_transfers": position.pending_transfers.len(),
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to calculate position: {e}")),
    }
}

/// Handle federation.clearing.settle RPC call - trigger settlement
async fn handle_federation_clearing_settle(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match &state.store_handle {
        Some(s) => s.clone(),
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match &state.federation_registry {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => return RpcResponse::error(id, -32000, format!("Failed to create clearing manager: {e}")),
    };

    match manager.trigger_settlement(&params.agreement_id) {
        Ok(report) => {
            info!("Settlement completed for {}", params.agreement_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "agreement_id": report.agreement_id,
                    "coop_a_owed": report.coop_a_owed,
                    "coop_b_owed": report.coop_b_owed,
                    "net_settlement": report.net_settlement,
                    "transfers_settled": report.transfers_settled,
                    "settled": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to settle: {e}")),
    }
}

/// Convert a CooperativeInfo to JSON
fn coop_info_to_json(coop: &icn_federation::CooperativeInfo) -> serde_json::Value {
    serde_json::json!({
        "coop_id": coop.coop_id,
        "name": coop.name,
        "public_did": coop.public_did.to_string(),
        "gateway_endpoints": coop.gateway_endpoints,
        "capabilities": coop.capabilities,
        "currencies": coop.currencies.iter().map(|c| serde_json::json!({
            "symbol": c.symbol,
            "name": c.name,
            "decimals": c.decimals,
        })).collect::<Vec<_>>(),
        "last_seen": coop.last_seen,
    })
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
