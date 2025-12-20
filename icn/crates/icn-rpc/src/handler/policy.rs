//! Policy and quota-related RPC handlers

use std::sync::Arc;

use crate::server::RpcServer;
use crate::types::RpcResponse;

/// Handle policy.set RPC call - set scheduling policy for a cooperative
pub async fn handle_policy_set(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match state.compute_handle() {
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
pub async fn handle_policy_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match state.compute_handle() {
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
            let result = serde_json::to_value(policy).unwrap_or_default();
            RpcResponse::success(id, result)
        }
        None => RpcResponse::success(id, serde_json::Value::Null),
    }
}

/// Handle policy.list RPC call - list all policies
pub async fn handle_policy_list(
    id: u64,
    _params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match state.compute_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Compute not available".to_string());
        }
    };

    let policies = compute_handle.list_policies().await;
    let result = serde_json::to_value(policies).unwrap_or_default();
    RpcResponse::success(id, result)
}

/// Handle policy.remove RPC call - remove a policy
pub async fn handle_policy_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match state.compute_handle() {
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
            let result = serde_json::to_value(policy).unwrap_or_default();
            RpcResponse::success(id, result)
        }
        None => RpcResponse::success(id, serde_json::Value::Null),
    }
}

/// Handle quota.usage RPC call - get usage for a member
pub async fn handle_quota_usage(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match state.compute_handle() {
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
            let result = serde_json::to_value(usage).unwrap_or_default();
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get usage: {e}")),
    }
}

/// Handle quota.list RPC call - list all usage for a cooperative
pub async fn handle_quota_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let compute_handle = match state.compute_handle() {
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
            let result = serde_json::to_value(usage_records).unwrap_or_default();
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list usage: {e}")),
    }
}
