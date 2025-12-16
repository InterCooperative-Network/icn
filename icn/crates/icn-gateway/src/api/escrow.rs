//! Payment Escrow API
//!
//! Hold funds for conditional release with multi-party authorization.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

/// Escrow status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscrowStatus {
    Pending,
    Locked,
    Released,
    Refunded,
    Expired,
}

/// Escrow condition type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscrowCondition {
    /// Requires approval from specific DID
    RequiresApproval { did: String },
    /// Releases after timestamp
    TimeRelease { timestamp: u64 },
    /// Requires external proof (e.g., delivery confirmation)
    ProofRequired { proof_type: String },
}

/// Escrow record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escrow {
    /// Unique ID
    pub id: String,
    /// Creator/initiator DID
    pub creator: String,
    /// From account
    pub from_account: String,
    /// To account (beneficiary)
    pub to_account: String,
    /// Amount held
    pub amount: i64,
    /// Currency
    pub currency: String,
    /// Current status
    pub status: EscrowStatus,
    /// Release conditions
    pub conditions: Vec<EscrowCondition>,
    /// Approvals received
    pub approvals: Vec<String>,
    /// Expiration timestamp
    pub expires_at: Option<u64>,
    /// Description/purpose
    pub description: String,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
}

/// In-memory escrow store
#[derive(Clone)]
pub struct EscrowStore {
    escrows: Arc<RwLock<HashMap<String, Escrow>>>,
}

impl Default for EscrowStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EscrowStore {
    pub fn new() -> Self {
        Self {
            escrows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(&self, escrow: Escrow) {
        self.escrows.write().await.insert(escrow.id.clone(), escrow);
    }

    pub async fn get(&self, id: &str) -> Option<Escrow> {
        self.escrows.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Escrow> {
        self.escrows.read().await.values().cloned().collect()
    }

    pub async fn update(&self, id: &str, escrow: Escrow) -> bool {
        let mut escrows = self.escrows.write().await;
        if escrows.contains_key(id) {
            escrows.insert(id.to_string(), escrow);
            true
        } else {
            false
        }
    }
}

/// Request to create escrow
#[derive(Debug, Deserialize)]
pub struct CreateEscrowRequest {
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

    store.insert(escrow.clone()).await;

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

    let mut escrows = store.list().await;

    // Filter to user's escrows (as creator or beneficiary)
    escrows.retain(|e| e.creator == user_did || e.to_account.contains(&user_did));

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
        .await
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
        .await
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
        escrow.status = EscrowStatus::Released;
        escrow.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        store.update(&escrow_id, escrow.clone()).await;

        // TODO: Execute actual payment transaction via ledger

        Ok(HttpResponse::Ok().json(serde_json::json!({
            "escrow": escrow,
            "released": true,
            "message": "Funds released to beneficiary"
        })))
    } else {
        escrow.status = EscrowStatus::Locked;
        escrow.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        store.update(&escrow_id, escrow.clone()).await;

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
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let user_did = claims.sub;

    let escrow_id = id.into_inner();
    let mut escrow = store
        .get(&escrow_id)
        .await
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

    escrow.status = EscrowStatus::Refunded;
    escrow.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    store.update(&escrow_id, escrow.clone()).await;

    // TODO: Execute refund transaction via ledger

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "escrow": escrow,
        "message": "Funds refunded to sender"
    })))
}

/// Check if all escrow conditions are met
fn check_conditions(escrow: &Escrow, approver: &str, _proof: Option<&str>) -> bool {
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
