//! Recovery-related RPC handlers

use std::sync::Arc;

use tracing::info;

use icn_identity::recovery::{RecoveryAttestation, RecoveryEvent, RecoveryStatus};
use icn_identity::Did;

use crate::context::RpcContext;
use crate::server::RpcServer;
use crate::types::{RecoveryAttestationInfo, RecoveryEventInfo, RpcResponse};

/// Handle recovery.initiate RPC call - initiate social recovery for a lost identity
pub async fn handle_recovery_initiate(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "recovery.initiate called"
        );
    }

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    // Get the new DID from authenticated context
    let new_did_str = match ctx {
        Some(c) => c.caller_did.to_string(),
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

    let new_did = match Did::from_str(&new_did_str) {
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
            return RpcResponse::internal_error(id, e);
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::internal_error(id, e);
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
pub async fn handle_recovery_attest(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "recovery.attest called"
        );
    }

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    let keypair = match state.own_keypair() {
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
            return RpcResponse::internal_error(id, e);
        }
    };

    let mut recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
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
            return RpcResponse::internal_error(id, e);
        }
    };

    // Add attestation to recovery
    let threshold_reached = match recovery.add_attestation(attestation) {
        Ok(t) => t,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
        }
    };

    // Save updated recovery
    let recovery_json = match serde_json::to_vec(&recovery) {
        Ok(j) => j,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::internal_error(id, e);
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
pub async fn handle_recovery_list(
    id: u64,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "recovery.list called"
        );
    }

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    let items = match store.scan(b"recovery:") {
        Ok(i) => i,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
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

    RpcResponse::success(id, serde_json::to_value(recoveries).unwrap_or_default())
}

/// Handle recovery.status RPC call - get status of a specific recovery
pub async fn handle_recovery_status(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "recovery.status called"
        );
    }
    let store = match state.store_handle() {
        Some(s) => s,
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
            return RpcResponse::internal_error(id, e);
        }
    };

    let recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
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
pub async fn handle_recovery_finalize(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "recovery.finalize called"
        );
    }

    let store = match state.store_handle() {
        Some(s) => s,
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
            return RpcResponse::internal_error(id, e);
        }
    };

    let mut recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
        }
    };

    // Check if delay expired
    recovery.check_delay_expired();

    // Finalize
    if let Err(e) = recovery.finalize() {
        return RpcResponse::internal_error(id, e);
    }

    // Save updated recovery
    let recovery_json = match serde_json::to_vec(&recovery) {
        Ok(j) => j,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::internal_error(id, e);
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
pub async fn handle_recovery_cancel(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "recovery.cancel called"
        );
    }

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not configured".to_string());
        }
    };

    // Get the canceller DID from authenticated context
    let canceller_did_str = match ctx {
        Some(c) => c.caller_did.to_string(),
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
    let canceller_did = match Did::from_str(&canceller_did_str) {
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
            return RpcResponse::internal_error(id, e);
        }
    };

    let mut recovery: RecoveryEvent = match serde_json::from_slice(&recovery_data) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
        }
    };

    // Cancel
    if let Err(e) = recovery.cancel(canceller_did.clone(), params.reason.clone()) {
        return RpcResponse::internal_error(id, e);
    }

    // Save updated recovery
    let recovery_json = match serde_json::to_vec(&recovery) {
        Ok(j) => j,
        Err(e) => {
            return RpcResponse::internal_error(id, e);
        }
    };

    if let Err(e) = store.put(recovery_key.as_bytes(), &recovery_json) {
        return RpcResponse::internal_error(id, e);
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
