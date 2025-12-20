//! Contract-related RPC handlers

use icn_time;
use std::sync::Arc;
use std::time::Instant;

use tracing::{error, info};

use crate::pagination::{paginate, PageRequest, DEFAULT_MAX_PAGE_SIZE};
use crate::receipt::{Operation, Outcome, Receipt, Resources};
use crate::server::RpcServer;
use crate::types::{ContractExecutionResponse, RpcResponse};

/// Handle contract.deploy RPC call - deploy a new contract with signature verification
pub async fn handle_contract_deploy(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let gossip_handle = match state.gossip_handle() {
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
            let receipt = Receipt::new(
                deployment_msg.installation.installed_by.clone(),
                Operation::ContractDeploy {
                    code_hash: code_hash.to_hex(),
                },
                Outcome::success(Some(code_hash.to_hex())),
            );
            let receipt_id = receipt.id.clone();
            state.receipt_store().insert(receipt).await;

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
            let receipt = Receipt::new(
                deployment_msg.installation.installed_by.clone(),
                Operation::ContractDeploy {
                    code_hash: code_hash.to_hex(),
                },
                Outcome::failure(e.to_string()),
            );
            state.receipt_store().insert(receipt).await;

            RpcResponse::error(id, -32000, format!("Failed to publish deployment: {e}"))
        }
    }
}

/// Handle contract.call RPC call - execute a contract rule
pub async fn handle_contract_call(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let contract_runtime = match state.contract_runtime() {
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
        icn_time::current_timestamp_secs(),
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
            let resources = Resources {
                fuel_used: result.fuel_consumed,
                bytes_processed,
                wall_time_ms,
            };

            let receipt = Receipt::with_resources(
                caller_did.clone(),
                Operation::ContractExecute {
                    code_hash: call_params.code_hash.clone(),
                    rule: call_params.rule_name.clone(),
                },
                Outcome::success(None),
                resources,
            );
            let receipt_id = receipt.id.clone();
            state.receipt_store().insert(receipt).await;

            let response = ContractExecutionResponse {
                success: true,
                fuel_consumed: result.fuel_consumed,
                return_value: response_value,
            };

            // Add receipt_id to response
            let mut value = serde_json::to_value(&response).unwrap_or_default();
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "receipt_id".to_string(),
                    serde_json::Value::String(receipt_id.to_string()),
                );
            }

            RpcResponse::success(id, value)
        }
        Err(e) => {
            // Create receipt for failed execution
            let receipt = Receipt::new(
                caller_did,
                Operation::ContractExecute {
                    code_hash: call_params.code_hash.clone(),
                    rule: call_params.rule_name.clone(),
                },
                Outcome::failure(e.to_string()),
            );
            state.receipt_store().insert(receipt).await;

            RpcResponse::error(id, -32000, format!("Contract execution failed: {e}"))
        }
    }
}

/// Handle contract.list RPC call - list all installed contracts (paginated)
pub async fn handle_contract_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let contract_runtime = match state.contract_runtime() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Contract runtime not available".to_string());
        }
    };

    // Parse pagination parameters
    let page_request: PageRequest = serde_json::from_value(params.clone()).unwrap_or_default();

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
    let page = paginate(contracts_rpc, &page_request, DEFAULT_MAX_PAGE_SIZE);

    match serde_json::to_value(&page) {
        Ok(value) => RpcResponse::success(id, value),
        Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
    }
}
