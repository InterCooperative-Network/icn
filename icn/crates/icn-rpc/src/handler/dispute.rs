//! Dispute-related RPC handlers

use std::sync::Arc;

use tracing::info;

use icn_identity::Did;
use icn_ledger::{ContentHash, Dispute, DisputeOutcome, DisputeStatus};

use crate::auth::RpcTokenClaims;
use crate::server::RpcServer;
use crate::types::RpcResponse;

/// Handle dispute.file RPC call - file a dispute against a ledger entry
pub async fn handle_dispute_file(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match state.dispute_manager() {
        Some(dm) => dm,
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
pub async fn handle_dispute_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let dispute_manager = match state.dispute_manager() {
        Some(dm) => dm,
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
pub async fn handle_dispute_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let dispute_manager = match state.dispute_manager() {
        Some(dm) => dm,
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
pub async fn handle_dispute_add_evidence(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match state.dispute_manager() {
        Some(dm) => dm,
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
pub async fn handle_dispute_assign_mediator(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match state.dispute_manager() {
        Some(dm) => dm,
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
pub async fn handle_dispute_resolve(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let dispute_manager = match state.dispute_manager() {
        Some(dm) => dm,
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
        DisputeStatus::Normal => "normal",
        DisputeStatus::Contested { .. } => "contested",
        DisputeStatus::Resolved { .. } => "resolved",
        DisputeStatus::Escalated { .. } => "escalated",
    };

    let (mediator, outcome, resolved_at) = match &dispute.status {
        DisputeStatus::Resolved {
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

    // Extract escalation info if present
    let (proposal_id, escalation_reason, escalated_at) = match &dispute.status {
        DisputeStatus::Escalated {
            proposal_id,
            escalation_reason,
            escalated_at,
        } => (
            Some(proposal_id.clone()),
            Some(escalation_reason.clone()),
            Some(*escalated_at),
        ),
        _ => (None, None, None),
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
        "proposal_id": proposal_id,
        "escalation_reason": escalation_reason,
        "escalated_at": escalated_at,
    })
}
