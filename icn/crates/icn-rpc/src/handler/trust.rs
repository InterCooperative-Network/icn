//! Trust-related RPC handlers
//!
//! # Coop Isolation
//!
//! TODO(#769): Add `ctx.require_coop()` enforcement when multi-coop trust graphs are implemented.
//! Currently the trust graph is global. When per-coop trust graphs exist, handlers should:
//! 1. Require `ctx` to be `Some` for write operations
//! 2. Call `ctx.require_coop(requested_coop_id)` to validate access
//! 3. Route requests to the appropriate coop-scoped trust graph

use std::sync::Arc;

use crate::context::RpcContext;
use crate::server::RpcServer;
use crate::types::RpcResponse;

/// Handle trust.add RPC call - submit a trust attestation
pub async fn handle_trust_add(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "trust.add called"
        );
    }

    let trust_service = match state.trust_service() {
        Some(svc) => svc,
        None => {
            return RpcResponse::error(id, -32000, "Trust service not available".to_string());
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

    let labels = params.label.into_iter().collect::<Vec<_>>();

    // Returned bytes are the serialized attestation for gossip broadcast;
    // gossip propagation is handled by the trust app's subscription loop,
    // not by the RPC handler.
    match trust_service.submit_attestation(&params.target_did, params.score, labels) {
        Ok(_attestation_bytes) => RpcResponse::success(id, serde_json::json!({"success": true})),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to add trust: {e}")),
    }
}

/// Handle trust.remove RPC call - revoke trust for a target
pub async fn handle_trust_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "trust.remove called"
        );
    }

    let trust_service = match state.trust_service() {
        Some(svc) => svc,
        None => {
            return RpcResponse::error(id, -32000, "Trust service not available".to_string());
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

    match trust_service.revoke_trust(&params.target_did) {
        Ok(_bytes) => RpcResponse::success(id, serde_json::json!({"success": true})),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to remove trust: {e}")),
    }
}

/// Handle trust.list RPC call - list outgoing trust edges
pub async fn handle_trust_list(
    id: u64,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "trust.list called"
        );
    }
    let trust_service = match state.trust_service() {
        Some(svc) => svc,
        None => {
            return RpcResponse::error(id, -32000, "Trust service not available".to_string());
        }
    };

    // Get the node's own DID for listing outgoing edges
    let own_did = match state.own_keypair() {
        Some(kp) => kp.did().to_string(),
        None => {
            return RpcResponse::error(id, -32000, "Node keypair not available".to_string());
        }
    };

    let edges = trust_service.get_edges(&own_did);
    RpcResponse::success(id, serde_json::json!(edges))
}

/// Handle trust.compute RPC call - compute trust score for a target DID
pub async fn handle_trust_compute(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "trust.compute called"
        );
    }
    let trust_service = match state.trust_service() {
        Some(svc) => svc,
        None => {
            return RpcResponse::error(id, -32000, "Trust service not available".to_string());
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

    let score = trust_service.trust_score(&params.target_did);
    RpcResponse::success(id, serde_json::json!({"score": score}))
}
