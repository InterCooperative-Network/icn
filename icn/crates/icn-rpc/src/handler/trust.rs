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

use icn_identity::Did;
use icn_trust::TrustEdge;

use crate::context::RpcContext;
use crate::server::RpcServer;
use crate::types::RpcResponse;

/// Handle trust.add RPC call - add a trust edge
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

    let trust_graph = match state.trust_handle() {
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
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle trust.remove RPC call - remove a trust edge
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

    let trust_graph = match state.trust_handle() {
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
        Err(e) => RpcResponse::internal_error(id, e),
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
    let trust_graph = match state.trust_handle() {
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
        Err(e) => RpcResponse::internal_error(id, e),
    }
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
    let trust_graph = match state.trust_handle() {
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
        Err(e) => RpcResponse::internal_error(id, e),
    }
}
