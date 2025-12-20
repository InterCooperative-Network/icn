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
use icn_governance::GovernanceOps;
use icn_identity::Did;
use icn_ledger::{DisputeManager, Ledger};
use icn_net::{NetworkHandle, RateLimiter, TrustGatedRateLimitConfig};
use icn_store::Store;
use icn_trust::TrustGraph;

use crate::auth::{required_scope_for_method, RpcAuthManager, RpcTokenClaims};
use crate::handler;
use crate::receipt::ReceiptStore;
use crate::types::{RpcRequest, RpcResponse};

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
    /// Trust-gated rate limiter for API requests (C8: Trust-based API rate limiting)
    rate_limiter: Option<Arc<RateLimiter>>,
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
            rate_limiter: None,
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
            rate_limiter: None,
            listen_addr,
        }
    }

    /// Set the authentication manager (for configuring after construction)
    pub fn set_auth_manager(&mut self, jwt_secret: Vec<u8>) {
        self.auth_manager = Some(Arc::new(RpcAuthManager::new(jwt_secret, true)));
    }

    /// Enable trust-based rate limiting for API requests (C8)
    ///
    /// Requires a trust graph to be set first via `set_trust_handle`.
    /// Different trust levels get different rate limits:
    /// - Isolated (< 0.1): 10 req/sec
    /// - Known (0.1-0.4): 50 req/sec
    /// - Partner (0.4-0.7): 100 req/sec
    /// - Federated (0.7+): 200 req/sec
    pub fn enable_trust_rate_limiting(&mut self) {
        if let Some(ref trust_graph) = self.trust_handle {
            let config = TrustGatedRateLimitConfig::default();
            self.rate_limiter = Some(Arc::new(RateLimiter::new_trust_gated(
                config,
                trust_graph.clone(),
            )));
            info!("Trust-based rate limiting enabled for RPC server");
        } else {
            warn!("Cannot enable trust rate limiting: no trust graph configured");
        }
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

    // =========================================================================
    // Accessor methods for handler modules
    // =========================================================================

    /// Get trust graph handle (for handler modules)
    pub fn trust_handle(&self) -> Option<&Arc<RwLock<TrustGraph>>> {
        self.trust_handle.as_ref()
    }

    /// Get network handle (for handler modules)
    pub fn network_handle(&self) -> Option<&Arc<RwLock<NetworkHandle>>> {
        self.network_handle.as_ref()
    }

    /// Get listen address (for handler modules)
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// Get auth manager handle (for handler modules)
    pub fn auth_manager(&self) -> Option<&Arc<RpcAuthManager>> {
        self.auth_manager.as_ref()
    }

    /// Get governance handle (for handler modules)
    pub fn governance_handle(&self) -> Option<&dyn GovernanceOps> {
        self.governance_handle.as_deref()
    }

    /// Get ledger handle (for handler modules)
    pub fn ledger_handle(&self) -> Option<&Arc<RwLock<Ledger>>> {
        self.ledger_handle.as_ref()
    }

    /// Get gossip handle (for handler modules)
    pub fn gossip_handle(&self) -> Option<&Arc<RwLock<GossipActor>>> {
        self.gossip_handle.as_ref()
    }

    /// Get contract runtime (for handler modules)
    pub fn contract_runtime(&self) -> Option<&Arc<RwLock<ContractRuntime>>> {
        self.contract_runtime.as_ref()
    }

    /// Get receipt store (for handler modules)
    pub fn receipt_store(&self) -> &ReceiptStore {
        &self.receipt_store
    }

    /// Get compute handle (for handler modules)
    pub fn compute_handle(&self) -> Option<&ComputeHandle> {
        self.compute_handle.as_ref()
    }

    /// Get store handle (for handler modules)
    pub fn store_handle(&self) -> Option<Arc<dyn Store>> {
        self.store_handle.clone()
    }

    /// Get own keypair (for handler modules)
    pub fn own_keypair(&self) -> Option<&Arc<icn_identity::KeyPair>> {
        self.own_keypair.as_ref()
    }

    /// Get dispute manager (for handler modules)
    pub fn dispute_manager(&self) -> Option<Arc<RwLock<DisputeManager>>> {
        self.dispute_manager.clone()
    }

    /// Get federation registry (for handler modules)
    pub fn federation_registry(&self) -> Option<&Arc<CooperativeRegistry>> {
        self.federation_registry.as_ref()
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

    // Apply trust-based rate limiting if enabled (C8)
    // Rate limit based on the authenticated user's DID
    if let (Some(ref rate_limiter), Some(ref claims)) = (&state.rate_limiter, &claims) {
        // Parse DID from claims
        if let Ok(did) = claims.sub.parse::<Did>() {
            let allowed = rate_limiter.check_rate_limit(&did).await;
            if !allowed {
                warn!(
                    "Rate limit exceeded for DID {} on method {}",
                    claims.sub, rpc_request.method
                );
                counter!("icn_rpc_rate_limited_total", "method" => method.clone()).increment(1);
                gauge!("icn_rpc_active_requests").decrement(1.0);
                let response = RpcResponse::error(
                    rpc_request.id,
                    -32429, // Custom error code for rate limiting
                    "Rate limit exceeded. Please slow down your requests.".to_string(),
                );
                return Ok(json_response(StatusCode::TOO_MANY_REQUESTS, &response));
            }
        }
    }

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
        "auth.challenge" => handler::auth::handle_auth_challenge(req.id, &req.params, state).await,
        "auth.verify" => handler::auth::handle_auth_verify(req.id, &req.params, state).await,

        // Network methods
        "network.peers" => handler::network::handle_network_peers(req.id, state).await,
        "network.dial" => handler::network::handle_network_dial(req.id, &req.params, state).await,
        "network.stats" => handler::network::handle_network_stats(req.id, state).await,
        "network.status" => handler::network::handle_network_status(req.id, state).await,

        // Ledger methods
        "ledger.head" => handler::ledger::handle_ledger_head(req.id, state).await,
        "ledger.balance" => {
            handler::ledger::handle_ledger_balance(req.id, &req.params, state).await
        }
        "ledger.history" => {
            handler::ledger::handle_ledger_history(req.id, &req.params, state).await
        }
        "ledger.quarantine.list" => {
            handler::ledger::handle_quarantine_list(req.id, &req.params, state).await
        }
        "ledger.quarantine.get" => {
            handler::ledger::handle_quarantine_get(req.id, &req.params, state).await
        }
        "ledger.quarantine.release" => {
            handler::ledger::handle_quarantine_release(req.id, &req.params, state).await
        }
        "ledger.quarantine.drop" => {
            handler::ledger::handle_quarantine_drop(req.id, &req.params, state).await
        }
        "ledger.quarantine.purge" => handler::ledger::handle_quarantine_purge(req.id, state).await,

        // Contract methods
        "contract.deploy" => {
            handler::contract::handle_contract_deploy(req.id, &req.params, state).await
        }
        "contract.call" => {
            handler::contract::handle_contract_call(req.id, &req.params, state).await
        }
        "contract.list" => {
            handler::contract::handle_contract_list(req.id, &req.params, state).await
        }
        "receipt.get" => handler::ledger::handle_receipt_get(req.id, &req.params, state).await,

        // Governance methods
        "governance.domain.list" => {
            handler::governance::handle_governance_domain_list(req.id, state).await
        }
        "governance.domain.get" => {
            handler::governance::handle_governance_domain_get(req.id, &req.params, state).await
        }
        "governance.domain.create" => {
            handler::governance::handle_governance_domain_create(req.id, &req.params, state).await
        }
        "governance.proposal.list" => {
            handler::governance::handle_governance_proposal_list(req.id, state).await
        }
        "governance.proposal.get" => {
            handler::governance::handle_governance_proposal_get(req.id, &req.params, state).await
        }
        "governance.proposal.create" => {
            handler::governance::handle_governance_proposal_create(req.id, &req.params, state).await
        }
        "governance.proposal.open" => {
            handler::governance::handle_governance_proposal_open(req.id, &req.params, state).await
        }
        "governance.proposal.close" => {
            handler::governance::handle_governance_proposal_close(req.id, &req.params, state).await
        }
        "governance.vote.cast" => {
            handler::governance::handle_governance_vote_cast(req.id, &req.params, state).await
        }

        // Compute methods (pass claims for authenticated submitter)
        "compute.submit" => {
            handler::compute::handle_compute_submit(req.id, &req.params, state, claims).await
        }
        "compute.status" => {
            handler::compute::handle_compute_status(req.id, &req.params, state).await
        }
        "compute.cancel" => {
            handler::compute::handle_compute_cancel(req.id, &req.params, state, claims).await
        }

        // Policy methods
        "policy.set" => handler::policy::handle_policy_set(req.id, &req.params, state).await,
        "policy.get" => handler::policy::handle_policy_get(req.id, &req.params, state).await,
        "policy.list" => handler::policy::handle_policy_list(req.id, &req.params, state).await,
        "policy.remove" => handler::policy::handle_policy_remove(req.id, &req.params, state).await,
        "quota.usage" => handler::policy::handle_quota_usage(req.id, &req.params, state).await,
        "quota.list" => handler::policy::handle_quota_list(req.id, &req.params, state).await,

        // Trust methods
        "trust.add" => handler::trust::handle_trust_add(req.id, &req.params, state).await,
        "trust.remove" => handler::trust::handle_trust_remove(req.id, &req.params, state).await,
        "trust.list" => handler::trust::handle_trust_list(req.id, state).await,
        "trust.compute" => handler::trust::handle_trust_compute(req.id, &req.params, state).await,

        // Recovery methods
        "recovery.initiate" => {
            handler::recovery::handle_recovery_initiate(req.id, &req.params, state, claims).await
        }
        "recovery.attest" => {
            handler::recovery::handle_recovery_attest(req.id, &req.params, state).await
        }
        "recovery.list" => handler::recovery::handle_recovery_list(req.id, state).await,
        "recovery.status" => {
            handler::recovery::handle_recovery_status(req.id, &req.params, state).await
        }
        "recovery.finalize" => {
            handler::recovery::handle_recovery_finalize(req.id, &req.params, state).await
        }
        "recovery.cancel" => {
            handler::recovery::handle_recovery_cancel(req.id, &req.params, state, claims).await
        }

        // Dispute methods (ledger entry disputes)
        "dispute.file" => {
            handler::dispute::handle_dispute_file(req.id, &req.params, state, claims).await
        }
        "dispute.list" => handler::dispute::handle_dispute_list(req.id, &req.params, state).await,
        "dispute.get" => handler::dispute::handle_dispute_get(req.id, &req.params, state).await,
        "dispute.add_evidence" => {
            handler::dispute::handle_dispute_add_evidence(req.id, &req.params, state, claims).await
        }
        "dispute.assign_mediator" => {
            handler::dispute::handle_dispute_assign_mediator(req.id, &req.params, state, claims)
                .await
        }
        "dispute.resolve" => {
            handler::dispute::handle_dispute_resolve(req.id, &req.params, state, claims).await
        }

        // Federation methods (inter-cooperative coordination)
        "federation.coop.list" => {
            handler::federation::handle_federation_coop_list(req.id, state).await
        }
        "federation.coop.get" => {
            handler::federation::handle_federation_coop_get(req.id, &req.params, state).await
        }
        "federation.coop.register" => {
            handler::federation::handle_federation_coop_register(req.id, &req.params, state).await
        }
        "federation.coop.remove" => {
            handler::federation::handle_federation_coop_remove(req.id, &req.params, state).await
        }
        "federation.own.get" => handler::federation::handle_federation_own_get(req.id, state).await,
        "federation.own.update" => {
            handler::federation::handle_federation_own_update(req.id, &req.params, state).await
        }
        "federation.vouch.list" => {
            handler::federation::handle_federation_vouch_list(req.id, &req.params, state).await
        }
        "federation.vouch.issue" => {
            handler::federation::handle_federation_vouch_issue(req.id, &req.params, state, claims)
                .await
        }
        "federation.vouch.remove" => {
            handler::federation::handle_federation_vouch_remove(req.id, &req.params, state).await
        }

        // Attestation methods (cross-cooperative trust)
        "federation.attestation.list" => {
            handler::federation::handle_federation_attestation_list(req.id, &req.params, state)
                .await
        }
        "federation.attestation.from" => {
            handler::federation::handle_federation_attestation_from(req.id, &req.params, state)
                .await
        }
        "federation.attestation.issue" => {
            handler::federation::handle_federation_attestation_issue(
                req.id,
                &req.params,
                state,
                claims,
            )
            .await
        }

        // Clearing methods (bilateral settlement)
        "federation.clearing.list" => {
            handler::federation::handle_federation_clearing_list(req.id, state).await
        }
        "federation.clearing.show" => {
            handler::federation::handle_federation_clearing_show(req.id, &req.params, state).await
        }
        "federation.clearing.create" => {
            handler::federation::handle_federation_clearing_create(
                req.id,
                &req.params,
                state,
                claims,
            )
            .await
        }
        "federation.clearing.position" => {
            handler::federation::handle_federation_clearing_position(req.id, &req.params, state)
                .await
        }
        "federation.clearing.settle" => {
            handler::federation::handle_federation_clearing_settle(
                req.id,
                &req.params,
                state,
                claims,
            )
            .await
        }

        _ => {
            counter!("icn_rpc_method_not_found_total").increment(1);
            RpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method))
        }
    }
}

/// Create a JSON response
fn json_response(status: StatusCode, response: &RpcResponse) -> Response<Full<Bytes>> {
    let json = serde_json::to_string(response).unwrap_or_else(|_| "{}".to_string());
    let body = Full::new(Bytes::from(json));
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from("{}"))))
}
