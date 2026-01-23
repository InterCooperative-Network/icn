//! Compute-related RPC handlers

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
    use base64::Engine;

    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "compute.submit called"
        );
    }

    let compute_handle = match state.compute_handle() {
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
    let submitter = ctx
        .map(|c| c.caller_did.to_string())
        .unwrap_or_else(|| "rpc:anonymous".to_string());

    // Get coop_id: prefer request, fallback to ctx
    let coop_id = request
        .coop_id
        .clone()
        .or_else(|| ctx.and_then(|c| c.coop_id.clone()));

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
        created_at: icn_time::current_timestamp_millis(),
        deadline: request.deadline_ms,
        payment_rate: request.payment_rate,
        payment_currency: request.payment_currency,
        resource_profile,             // From request
        actor_mode: None,             // Not actor mode (Phase 16D)
        placement_constraints: None,  // No constraints from RPC (Phase 16E will set from policy)
        federation_constraints: None, // No federation constraints from RPC (Phase 21)
        estimated_value: None,        // Issue #478: Computed from task value or set by client
        verification: None,           // Issue #478: Auto-determined from estimated_value
    };

    match compute_handle.submit(task).await {
        Ok(hash) => {
            let response = SubmitTaskResponse {
                task_hash: hex::encode(hash),
            };
            RpcResponse::success(id, serde_json::to_value(response).unwrap_or_default())
        }
        Err(e) => RpcResponse::internal_error(id, e),
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

    let compute_handle = match state.compute_handle() {
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
            RpcResponse::success(id, serde_json::to_value(info).unwrap_or_default())
        }
        Ok(None) => RpcResponse::error(id, -32000, "Task not found".to_string()),
        Err(e) => RpcResponse::internal_error(id, e),
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

    let compute_handle = match state.compute_handle() {
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
    let caller_did_str = ctx
        .map(|c| c.caller_did.to_string())
        .unwrap_or_else(|| "rpc:anonymous".to_string());

    match compute_handle
        .cancel_task(&hash_bytes, &caller_did_str, reason)
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
        Err(e) => RpcResponse::internal_error(id, e),
    }
}
