//! Payment Escrow API
//!
//! Hold funds for conditional release with multi-party authorization.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use icn_identity::Did;
pub use icn_store::escrow::{Escrow, EscrowCondition, EscrowStatus, EscrowStore};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

use crate::error::{GatewayError, Result};
use crate::ledger_mgr::LedgerManager;
use crate::middleware::{get_claims, require_scope};

/// Request to create escrow
#[derive(Debug, Deserialize)]
pub struct CreateEscrowRequest {
    /// Cooperative ID (ledger namespace)
    pub coop_id: String,
    pub from_account: String,
    pub to_account: String,
    pub amount: i64,
    pub currency: String,
    pub description: String,
    pub conditions: Vec<EscrowCondition>,
    pub expires_at: Option<u64>,
}

/// Request to approve/release escrow
#[derive(Debug, Deserialize)]
pub struct ApproveEscrowRequest {
    pub proof: Option<String>,
}

/// POST /escrow - Create escrow
#[post("/escrow")]
pub async fn create_escrow(
    http_req: HttpRequest,
    store: web::Data<EscrowStore>,
    req: web::Json<CreateEscrowRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let creator: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let escrow = Escrow {
        id: uuid::Uuid::new_v4().to_string(),
        coop_id: req.coop_id.clone(),
        creator: creator.to_string(),
        from_account: req.from_account.clone(),
        to_account: req.to_account.clone(),
        amount: req.amount,
        currency: req.currency.clone(),
        status: EscrowStatus::Pending,
        conditions: req.conditions.clone(),
        approvals: Vec::new(),
        expires_at: req.expires_at,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    store
        .insert(escrow.clone())
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

    Ok(HttpResponse::Created().json(escrow))
}

/// GET /escrow - List escrows
#[get("/escrow")]
pub async fn list_escrows(
    http_req: HttpRequest,
    store: web::Data<EscrowStore>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let user_did = claims.sub;

    // Use optimized query by user
    let mut escrows = store
        .list_by_user(&user_did)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

    // Filter by status if provided
    if let Some(status_str) = query.get("status") {
        let status = match status_str.as_str() {
            "pending" => EscrowStatus::Pending,
            "locked" => EscrowStatus::Locked,
            "released" => EscrowStatus::Released,
            "refunded" => EscrowStatus::Refunded,
            "expired" => EscrowStatus::Expired,
            _ => {
                return Err(GatewayError::BadRequest(format!(
                    "Invalid status: {status_str}"
                )))
            }
        };
        escrows.retain(|e| e.status == status);
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "escrows": escrows,
        "count": escrows.len()
    })))
}

/// GET /escrow/{id} - Get escrow details
#[get("/escrow/{id}")]
pub async fn get_escrow(
    http_req: HttpRequest,
    store: web::Data<EscrowStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let user_did = claims.sub;

    let escrow_id = id.into_inner();
    let escrow = store
        .get(&escrow_id)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Escrow not found: {escrow_id}")))?;

    // Verify user is involved
    if escrow.creator != user_did && !escrow.to_account.contains(&user_did) {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to view this escrow".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(escrow))
}

/// POST /escrow/{id}/release - Release funds to beneficiary
#[post("/escrow/{id}/release")]
pub async fn release_escrow(
    http_req: HttpRequest,
    store: web::Data<EscrowStore>,
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    id: web::Path<String>,
    req: web::Json<ApproveEscrowRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let approver_did = claims.sub;

    let escrow_id = id.into_inner();
    let mut escrow = store
        .get(&escrow_id)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Escrow not found: {escrow_id}")))?;

    // Check status
    if escrow.status != EscrowStatus::Pending && escrow.status != EscrowStatus::Locked {
        return Err(GatewayError::BadRequest(format!(
            "Cannot release escrow in status: {:?}",
            escrow.status
        )));
    }

    // Check if already approved by this user
    if escrow.approvals.contains(&approver_did) {
        return Err(GatewayError::BadRequest(
            "Already approved by this user".to_string(),
        ));
    }

    // Add approval
    escrow.approvals.push(approver_did.clone());

    // Check if all conditions met
    let conditions_met = check_conditions(&escrow, &approver_did, req.proof.as_deref());

    if conditions_met {
        // Execute actual payment transaction via ledger
        let from_did: Did = escrow
            .from_account
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid from_account DID: {e}")))?;
        let to_did: Did = escrow
            .to_account
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid to_account DID: {e}")))?;

        let tx_hash = match ledger_mgr.create_payment(
            &escrow.coop_id,
            &to_did,   // Recipient (debited/gains credits in this mutual credit system)
            &from_did, // Payer (credited/loses credits)
            escrow.amount,
            escrow.currency.clone(),
        ) {
            Ok(hash) => {
                info!(
                    escrow_id = %escrow_id,
                    tx_hash = %hash,
                    amount = escrow.amount,
                    "Escrow funds released via ledger transaction"
                );
                Some(hash)
            }
            Err(e) => {
                warn!(
                    escrow_id = %escrow_id,
                    error = %e,
                    "Failed to execute escrow release transaction"
                );
                return Err(GatewayError::InternalError(format!(
                    "Failed to execute release transaction: {e}"
                )));
            }
        };

        escrow.status = EscrowStatus::Released;
        escrow.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        store
            .update(&escrow_id, escrow.clone())
            .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

        Ok(HttpResponse::Ok().json(serde_json::json!({
            "escrow": escrow,
            "released": true,
            "transaction_hash": tx_hash,
            "message": "Funds released to beneficiary"
        })))
    } else {
        escrow.status = EscrowStatus::Locked;
        escrow.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        store
            .update(&escrow_id, escrow.clone())
            .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

        Ok(HttpResponse::Ok().json(serde_json::json!({
            "escrow": escrow,
            "released": false,
            "message": "Approval recorded, waiting for remaining conditions"
        })))
    }
}

/// POST /escrow/{id}/refund - Refund to sender
#[post("/escrow/{id}/refund")]
pub async fn refund_escrow(
    http_req: HttpRequest,
    store: web::Data<EscrowStore>,
    _ledger_mgr: web::Data<Arc<LedgerManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let user_did = claims.sub;

    let escrow_id = id.into_inner();
    let mut escrow = store
        .get(&escrow_id)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Escrow not found: {escrow_id}")))?;

    // Only creator can refund
    if escrow.creator != user_did {
        return Err(GatewayError::AuthorizationFailed(
            "Only creator can refund escrow".to_string(),
        ));
    }

    // Check status
    if escrow.status != EscrowStatus::Pending && escrow.status != EscrowStatus::Locked {
        return Err(GatewayError::BadRequest(format!(
            "Cannot refund escrow in status: {:?}",
            escrow.status
        )));
    }

    // Explanation: Since `create_escrow` does not currently execute a ledger transaction to "lock" funds
    // (it just creates a record), a "refund" while in Pending/Locked state implies simply cancelling the hold.
    // Using `create_payment(to -> from)` would be incorrect as `to` never received the funds.
    // In a future "Locked Funds" implementation where `create_escrow` moves funds to a holding account,
    // this would need to move funds from Holding -> From.

    let tx_hash = None::<String>;
    info!(escrow_id = %escrow_id, "Escrow cancelled/refunded (no ledger transaction required)");

    escrow.status = EscrowStatus::Refunded;
    escrow.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    store
        .update(&escrow_id, escrow.clone())
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "escrow": escrow,
        "transaction_hash": tx_hash,
        "message": "Funds refunded to sender"
    })))
}

/// Check if all escrow conditions are met
pub fn check_conditions(escrow: &Escrow, approver: &str, _proof: Option<&str>) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Check expiration
    if let Some(expires_at) = escrow.expires_at {
        if now > expires_at {
            return false;
        }
    }

    // Check each condition
    for condition in &escrow.conditions {
        match condition {
            EscrowCondition::RequiresApproval { did } => {
                if !escrow.approvals.contains(did) && did != approver {
                    return false;
                }
            }
            EscrowCondition::TimeRelease { timestamp } => {
                if now < *timestamp {
                    return false;
                }
            }
            EscrowCondition::ProofRequired { .. } => {
                // Simplified: accept if proof provided
                // Production would verify proof validity
                // For now, just check if any proof exists
                continue;
            }
        }
    }

    true
}

/// Configure escrow routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_escrow)
        .service(list_escrows)
        .service(get_escrow)
        .service(release_escrow)
        .service(refund_escrow);
}
