//! Compute-related RPC handlers
//!
//! # Coop Isolation
//!
//! TODO(#769): Verify `ctx.coop_id` matches the task's `coop_id` for all operations.
//! Currently compute tasks include coop_id for attribution. Handlers should:
//! 1. Require `ctx` to be `Some` for task submission
//! 2. Validate that submitted task's coop_id matches ctx.coop_id
//! 3. Restrict task queries to the caller's coop scope

use std::sync::Arc;

use crate::context::RpcContext;
use crate::server::RpcServer;
use crate::types::{
    CodeType, RpcResponse, SubmitTaskRequest, SubmitTaskResponse, TaskResultInfo, TaskStatusInfo,
};

/// Handle compute.submit RPC call - submit a task for distributed execution
pub async fn handle_compute_submit(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "compute.submit called"
        );
    }

    // Get ComputeService
    let compute_service = match state.compute_service() {
        Some(service) => service,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    // Parse request
    let request: SubmitTaskRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Build API context
    let api_ctx = icn_api::ApiContext {
        caller_did: ctx
            .map(|c| c.caller_did.to_string())
            .unwrap_or_else(|| "did:icn:rpc-anonymous".to_string()),
        coop_id: request
            .coop_id
            .clone()
            .or_else(|| ctx.and_then(|c| c.coop_id.clone())),
    };

    // Convert RPC request to API params
    let code_type = match request.code_type {
        CodeType::Ccl => icn_api::compute::CodeTypeParam::Ccl,
        CodeType::Wasm => icn_api::compute::CodeTypeParam::Wasm,
    };

    let priority = icn_api::compute::TaskPriorityParam::from_str(&request.priority);

    let resource_profile = request.resource_profile.as_ref().map(|rp| {
        icn_api::compute::ResourceProfileParam {
            cpu_cores: rp.cpu_cores,
            memory_mb: rp.memory_mb,
            storage_mb: rp.storage_mb,
            network_mbps: rp.network_mbps,
            duration_estimate_secs: None, // Not exposed in RPC yet
        }
    });

    let params = icn_api::SubmitTaskParams {
        task_id: request.task_id,
        code: request.code,
        wasm_bytes: request.wasm_bytes,
        wasm_hash: None, // RPC layer uses inline bytes; wasm_hash is a gateway-level concept
        code_type,
        inputs: request.inputs,
        fuel_limit: request.fuel_limit,
        priority,
        deadline_ms: request.deadline_ms,
        payment_rate: request.payment_rate,
        payment_currency: request.payment_currency,
        coop_id: None, // Already in api_ctx
        resource_profile,
    };

    // Submit via service
    match compute_service.submit_task(&api_ctx, params).await {
        Ok(hash) => {
            let response = SubmitTaskResponse {
                task_hash: hex::encode(hash),
            };
            RpcResponse::success(id, serde_json::to_value(response).unwrap_or_default())
        }
        Err(e) => RpcResponse::error(id, e.to_rpc_code(), e.to_string()),
    }
}

/// Handle compute.status RPC call - get task status
pub async fn handle_compute_status(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "compute.status called"
        );
    }

    // Get ComputeService
    let compute_service = match state.compute_service() {
        Some(service) => service,
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

    // Build API context
    let api_ctx = icn_api::ApiContext {
        caller_did: ctx
            .map(|c| c.caller_did.to_string())
            .unwrap_or_else(|| "did:icn:rpc-anonymous".to_string()),
        coop_id: ctx.and_then(|c| c.coop_id.clone()),
    };

    match compute_service.get_status(&api_ctx, &hash_bytes).await {
        Ok(status) => {
            // Convert API response to RPC response
            let info = TaskStatusInfo {
                task_hash: status.task_hash,
                status: status.status,
                executor: status.executor,
                result: status.result.map(|r| TaskResultInfo {
                    outcome: r.outcome,
                    output: r.output,
                    error: r.error,
                    fuel_used: r.fuel_used,
                    duration_ms: r.duration_ms,
                }),
            };
            RpcResponse::success(id, serde_json::to_value(info).unwrap_or_default())
        }
        Err(e) => RpcResponse::error(id, e.to_rpc_code(), e.to_string()),
    }
}

/// Handle compute.cancel RPC call - cancel a task
pub async fn handle_compute_cancel(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "compute.cancel called"
        );
    }

    // Get ComputeService
    let compute_service = match state.compute_service() {
        Some(service) => service,
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

    // Build API context
    let api_ctx = icn_api::ApiContext {
        caller_did: ctx
            .map(|c| c.caller_did.to_string())
            .unwrap_or_else(|| "did:icn:rpc-anonymous".to_string()),
        coop_id: ctx.and_then(|c| c.coop_id.clone()),
    };

    match compute_service
        .cancel_task(&api_ctx, &hash_bytes, params.reason)
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
            RpcResponse::success(id, serde_json::to_value(response).unwrap_or_default())
        }
        Err(e) => RpcResponse::error(id, e.to_rpc_code(), e.to_string()),
    }
}
