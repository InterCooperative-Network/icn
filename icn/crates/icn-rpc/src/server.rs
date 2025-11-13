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
use icn_ledger::Ledger;
use icn_ccl::ContractRuntime;

use crate::types::{ContractExecutionResponse, ContractInfo, LedgerAccountDelta, LedgerBalance, LedgerEntry, NetworkStats, NetworkStatus, PeerInfo, RpcRequest, RpcResponse};
use crate::receipt::ReceiptStore;

use icn_gossip::GossipActor;

/// RPC server state
pub struct RpcServer {
    network_handle: Option<Arc<RwLock<NetworkHandle>>>,
    ledger_handle: Option<Arc<RwLock<Ledger>>>,
    contract_runtime: Option<Arc<RwLock<ContractRuntime>>>,
    gossip_handle: Option<Arc<RwLock<GossipActor>>>,
    receipt_store: Arc<ReceiptStore>,
    listen_addr: SocketAddr,
}

impl RpcServer {
    /// Create a new RPC server
    pub fn new(listen_addr: SocketAddr) -> Self {
        RpcServer {
            network_handle: None,
            ledger_handle: None,
            contract_runtime: None,
            gossip_handle: None,
            receipt_store: Arc::new(ReceiptStore::new(10_000, 86400)), // 10k receipts, 24h TTL
            listen_addr,
        }
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
        "ledger.head" => handle_ledger_head(req.id, state).await,
        "ledger.balance" => handle_ledger_balance(req.id, &req.params, state).await,
        "ledger.history" => handle_ledger_history(req.id, &req.params, state).await,
        "ledger.quarantine.list" => handle_quarantine_list(req.id, &req.params, state).await,
        "ledger.quarantine.get" => handle_quarantine_get(req.id, &req.params, state).await,
        "ledger.quarantine.release" => handle_quarantine_release(req.id, &req.params, state).await,
        "ledger.quarantine.drop" => handle_quarantine_drop(req.id, &req.params, state).await,
        "ledger.quarantine.purge" => handle_quarantine_purge(req.id, state).await,
        "contract.deploy" => handle_contract_deploy(req.id, &req.params, state).await,
        "contract.call" => handle_contract_call(req.id, &req.params, state).await,
        "contract.list" => handle_contract_list(req.id, &req.params, state).await,
        "receipt.get" => handle_receipt_get(req.id, &req.params, state).await,
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

/// Handle ledger.head RPC call - get the most recent ledger entry
async fn handle_ledger_head(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let ledger_handle = match &state.ledger_handle {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Ledger not available".to_string(),
            );
        }
    };

    let ledger = ledger_handle.read().await;
    match ledger.get_all_entries() {
        Ok(entries) => {
            if let Some(last_entry) = entries.last() {
                let hash = last_entry.id.as_ref()
                    .map(|h| h.to_hex())
                    .unwrap_or_else(|| "unknown".to_string());

                let rpc_entry = LedgerEntry {
                    hash,
                    timestamp: last_entry.timestamp,
                    author: last_entry.author.as_str().to_string(),
                    accounts: last_entry.accounts.iter().map(|delta| {
                        LedgerAccountDelta {
                            account_id: delta.account_id.as_str().to_string(),
                            currency: delta.currency.clone(),
                            debit: delta.debit,
                            credit: delta.credit,
                        }
                    }).collect(),
                };

                match serde_json::to_value(&rpc_entry) {
                    Ok(value) => RpcResponse::success(id, value),
                    Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
                }
            } else {
                RpcResponse::success(id, serde_json::json!(null))
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get entries: {}", e)),
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
            return RpcResponse::error(
                id,
                -32000,
                "Ledger not available".to_string(),
            );
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
        }
    };

    let account_did = match serde_json::from_value(serde_json::Value::String(balance_params.account_id.clone())) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID: {}", e));
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
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
        }
    } else {
        // Get all balances for account
        let account_balances = ledger.get_account_balances(&account_did);
        let balances: Vec<LedgerBalance> = account_balances.balances.iter().map(|(currency, amount)| {
            LedgerBalance {
                account_id: balance_params.account_id.clone(),
                currency: currency.clone(),
                amount: *amount,
            }
        }).collect();

        match serde_json::to_value(&balances) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
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
            return RpcResponse::error(
                id,
                -32000,
                "Ledger not available".to_string(),
            );
        }
    };

    // Parse pagination parameters
    let page_request: crate::PageRequest = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(_) => crate::PageRequest::default(), // Use default if params are empty
    };

    let ledger = ledger_handle.read().await;
    match ledger.get_all_entries() {
        Ok(entries) => {
            // Convert all entries (in reverse order - most recent first)
            let all_entries: Vec<LedgerEntry> = entries
                .iter()
                .rev()
                .map(|entry| {
                    let hash = entry.id.as_ref()
                        .map(|h| h.to_hex())
                        .unwrap_or_else(|| "unknown".to_string());

                    LedgerEntry {
                        hash,
                        timestamp: entry.timestamp,
                        author: entry.author.as_str().to_string(),
                        accounts: entry.accounts.iter().map(|delta| {
                            LedgerAccountDelta {
                                account_id: delta.account_id.as_str().to_string(),
                                currency: delta.currency.clone(),
                                debit: delta.debit,
                                credit: delta.credit,
                            }
                        }).collect(),
                    }
                })
                .collect();

            // Apply pagination
            let page = crate::paginate(all_entries, &page_request, crate::DEFAULT_MAX_PAGE_SIZE);

            match serde_json::to_value(&page) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get entries: {}", e)),
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
            return RpcResponse::error(
                id,
                -32000,
                "Gossip not available".to_string(),
            );
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
        }
    };

    // Parse deployment message from JSON
    let deployment_msg: icn_ccl::ContractDeploymentMessage = match serde_json::from_str(&deploy_params.deployment_message) {
        Ok(m) => m,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid deployment message JSON: {}", e));
        }
    };

    // Pre-verify signatures before publishing to gossip (early rejection of invalid sigs)
    if let Err(e) = deployment_msg.verify() {
        return RpcResponse::error(
            id,
            -32602,
            format!("Signature verification failed: {}", e)
        );
    }

    let code_hash = deployment_msg.installation.code_hash.clone();

    // Serialize deployment message for gossip
    let message_bytes = match serde_json::to_vec(&deployment_msg) {
        Ok(bytes) => bytes,
        Err(e) => {
            return RpcResponse::error(id, -32603, format!("Failed to serialize deployment: {}", e));
        }
    };

    // Publish to contracts:deploy gossip topic
    let mut gossip = gossip_handle.write().await;
    match gossip.publish("contracts:deploy", message_bytes) {
        Ok(_) => {
            info!("Contract deployment published to gossip: {}", code_hash.to_hex());

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

            RpcResponse::error(id, -32000, format!("Failed to publish deployment: {}", e))
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
            return RpcResponse::error(
                id,
                -32000,
                "Contract runtime not available".to_string(),
            );
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
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
    let caller_did: icn_identity::Did = match serde_json::from_value(serde_json::Value::String(call_params.caller.clone())) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID: {}", e));
        }
    };

    // Parse arguments
    let args: std::collections::HashMap<String, icn_ccl::Value> = match serde_json::from_value(call_params.args) {
        Ok(a) => a,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid args: {}", e));
        }
    };

    // Create execution context with generous fuel
    let context = icn_ccl::ExecutionContext::new(
        caller_did.clone(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        10000, // Generous fuel limit
        vec![], // No capabilities for now
        vec![], // No participants for now
    );

    // Execute rule
    let mut runtime = contract_runtime.write().await;
    match runtime.execute_rule(&code_hash, &call_params.rule_name, context, args).await {
        Ok(result) => {
            let response_value = serde_json::to_value(&result.value)
                .unwrap_or_else(|_| serde_json::json!(null));

            // Create receipt for successful execution
            let resources = crate::receipt::Resources {
                fuel_used: result.fuel_consumed,
                bytes_processed: 0, // TODO: Track bytes processed
                wall_time_ms: 0,    // TODO: Track wall time
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

            RpcResponse::error(id, -32000, format!("Contract execution failed: {}", e))
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
            return RpcResponse::error(
                id,
                -32000,
                "Contract runtime not available".to_string(),
            );
        }
    };

    // Parse pagination parameters
    let page_request: crate::PageRequest = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(_) => crate::PageRequest::default(), // Use default if params are empty
    };

    let runtime = contract_runtime.read().await;
    let contracts = runtime.list_contracts();

    // Convert from icn-ccl::ContractInfo to RPC ContractInfo format
    let contracts_rpc: Vec<crate::types::ContractInfo> = contracts
        .iter()
        .map(|info| crate::types::ContractInfo {
            code_hash: info.code_hash.to_hex(),
            name: info.name.clone(),
            participants: info.participants.iter().map(|did| format!("{:?}", did)).collect(),
            currency: info.currency.clone(),
            rules: info.rules.clone(),
        })
        .collect();

    // Apply pagination
    let page = crate::paginate(contracts_rpc, &page_request, crate::DEFAULT_MAX_PAGE_SIZE);

    match serde_json::to_value(&page) {
        Ok(value) => RpcResponse::success(id, value),
        Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
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
    let page_request: crate::PageRequest = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(_) => crate::PageRequest::default(), // Use default if params are empty
    };

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
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list quarantine: {}", e)),
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
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
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get quarantine entry: {}", e)),
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
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
                    format!("Entry released from quarantine but reappend failed: {}", e),
                ),
            }
        }
        Ok(None) => RpcResponse::error(id, -32000, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to release entry: {}", e)),
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
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
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to drop entry: {}", e)),
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
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to purge expired entries: {}", e)),
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
            return RpcResponse::error(id, -32602, format!("Invalid params: {}", e));
        }
    };

    let receipt_id = crate::receipt::ReceiptId::from_string(get_params.receipt_id);

    match state.receipt_store.get(&receipt_id).await {
        Some(receipt) => match serde_json::to_value(&receipt) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {}", e)),
        },
        None => RpcResponse::error(id, -32000, "Receipt not found".to_string()),
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
