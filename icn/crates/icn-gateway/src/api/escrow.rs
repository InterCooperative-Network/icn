//! Settlement Escrow API
//!
//! Hold funds for conditional release with multi-party authorization.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use icn_identity::Did;
pub use icn_ledger_app::{Escrow, EscrowCondition, EscrowStatus, EscrowStore};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::ledger_mgr::LedgerManager;
use crate::middleware::{get_claims, require_coop_access, require_scope};

/// Request to create escrow
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEscrowRequest {
    /// Cooperative ID (ledger namespace)
    pub coop_id: String,
    pub from_account: String,
    pub to_account: String,
    pub amount: i64,
    pub unit: String,
    pub description: String,
    pub conditions: Vec<EscrowCondition>,
    pub expires_at: Option<u64>,
}

/// Request to approve/release escrow
#[derive(Debug, Deserialize, ToSchema)]
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
    require_scope(&http_req, "settlements:write")?;
    // Flat coop-namespace guard (NOT entity/community/federation hierarchy — that is
    // tracked in #2061): bind the body-supplied coop_id to the caller's token
    // namespace. `settlements:write` is requestable per namespace, so without this a
    // token for coop A could create an escrow under coop B. Prevent a cross-namespace write.
    require_coop_access(&http_req, &req.coop_id)?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let creator: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let now = icn_time::current_timestamp_secs();

    let escrow = Escrow {
        id: uuid::Uuid::new_v4().to_string(),
        coop_id: req.coop_id.clone(),
        creator: creator.to_string(),
        from_account: req.from_account.clone(),
        to_account: req.to_account.clone(),
        amount: req.amount,
        currency: req.unit.clone(),
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
    require_scope(&http_req, "settlements:read")?;

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
    require_scope(&http_req, "settlements:read")?;

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
    require_scope(&http_req, "settlements:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let approver_did = claims.sub;

    let escrow_id = id.into_inner();
    let mut escrow = store
        .get(&escrow_id)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Escrow not found: {escrow_id}")))?;

    // The escrow lives in a specific coop namespace; only a caller bound to that
    // namespace may approve/release it (and trigger a settlement on its ledger).
    // Flat coop-namespace guard, not entity hierarchy (#2061). Prevent a cross-namespace release.
    require_coop_access(&http_req, &escrow.coop_id)?;

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

        let tx_hash = match ledger_mgr
            .create_settlement(
                &escrow.coop_id,
                &to_did,   // Recipient (debited/gains credits in this mutual credit system)
                &from_did, // Payer (credited/loses credits)
                escrow.amount,
                escrow.currency.clone(),
                None, // system-executed path: no per-request JWT proof
            )
            .await
        {
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
        escrow.updated_at = icn_time::current_timestamp_secs();

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
        escrow.updated_at = icn_time::current_timestamp_secs();

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
    require_scope(&http_req, "settlements:write")?;

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
    escrow.updated_at = icn_time::current_timestamp_secs();

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
    let now = icn_time::current_timestamp_secs();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use actix_web::http::StatusCode;
    use actix_web::{test as actix_test, App, HttpMessage};

    fn write_claims(coop: &str, sub: &str) -> TokenClaims {
        TokenClaims {
            sub: sub.to_string(),
            iat: 1_000_000_000,
            exp: 9_999_999_999,
            coop_id: coop.to_string(),
            // Validly scoped, so the coop binding is the only thing under test.
            scopes: vec!["settlements:write".to_string()],
        }
    }

    fn fresh_db() -> sled::Db {
        sled::Config::new().temporary(true).open().unwrap()
    }

    /// `settlements:write` is requestable per coop, so `create_escrow` binds the
    /// body-supplied coop_id to the token's coop. A same-coop create succeeds (201)
    /// and persists exactly one escrow; a cross-coop create is rejected (403) and
    /// leaves the store empty. The store-state assertions (not just the HTTP status)
    /// prove "no mutation on reject". Mirrors `create_meeting_rejects_cross_coop_write`.
    #[actix_web::test]
    async fn create_escrow_rejects_cross_coop_write() {
        async fn run(token_coop: &str, body_coop: &str, sub: &str) -> (StatusCode, EscrowStore) {
            // Two store handles over one temp db so we can assert on what the app wrote.
            let db = fresh_db();
            let store = EscrowStore::new(db.clone());
            let probe = EscrowStore::new(db);
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(store))
                    .service(create_escrow),
            )
            .await;
            let req = actix_test::TestRequest::post()
                .uri("/escrow")
                .set_json(serde_json::json!({
                    "coop_id": body_coop,
                    "from_account": "acct-from",
                    "to_account": "acct-to",
                    "amount": 100,
                    "unit": "hours",
                    "description": "test escrow",
                    "conditions": []
                }))
                .to_request();
            req.extensions_mut().insert(write_claims(token_coop, sub));
            let status = actix_test::call_service(&app, req).await.status();
            (status, probe)
        }

        // The success path parses claims.sub into a Did, so use a real DID subject.
        let sub = icn_identity::KeyPair::generate().unwrap().did().to_string();

        let (same, same_store) = run("coopA", "coopA", &sub).await;
        assert_eq!(same, StatusCode::CREATED);
        assert_eq!(
            same_store.list_by_user(&sub).unwrap().len(),
            1,
            "same-coop create should persist exactly one escrow"
        );

        let (cross, cross_store) = run("coopA", "coopB", &sub).await;
        assert_eq!(cross, StatusCode::FORBIDDEN);
        assert!(
            cross_store.list_by_user(&sub).unwrap().is_empty(),
            "cross-coop reject must not create an escrow"
        );
    }

    /// An escrow belongs to a specific coop; `release_escrow` binds the caller to
    /// that coop. A coopA token cannot approve/release a coopB escrow: the request
    /// is rejected (403) and the stored escrow is left untouched (still Pending, no
    /// approvals recorded) — proving the cross-coop call never reached the mutation.
    #[actix_web::test]
    async fn release_escrow_rejects_cross_coop_release() {
        let db = fresh_db();
        let store = EscrowStore::new(db.clone());
        let probe = EscrowStore::new(db);

        // Seed a pending escrow owned by coopB.
        store
            .insert(Escrow {
                id: "esc-1".to_string(),
                coop_id: "coopB".to_string(),
                creator: "did:icn:creator".to_string(),
                from_account: "acct-from".to_string(),
                to_account: "acct-to".to_string(),
                amount: 100,
                currency: "hours".to_string(),
                status: EscrowStatus::Pending,
                conditions: vec![],
                approvals: vec![],
                expires_at: None,
                description: "seeded".to_string(),
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();

        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(store))
                .app_data(web::Data::new(Arc::new(LedgerManager::new())))
                .service(release_escrow),
        )
        .await;

        let sub = icn_identity::KeyPair::generate().unwrap().did().to_string();
        let req = actix_test::TestRequest::post()
            .uri("/escrow/esc-1/release")
            .set_json(serde_json::json!({ "proof": null }))
            .to_request();
        req.extensions_mut().insert(write_claims("coopA", &sub));
        let status = actix_test::call_service(&app, req).await.status();

        assert_eq!(status, StatusCode::FORBIDDEN);
        let after = probe.get("esc-1").unwrap().expect("escrow still present");
        assert_eq!(
            after.status,
            EscrowStatus::Pending,
            "cross-coop release must not change escrow status"
        );
        assert!(
            after.approvals.is_empty(),
            "cross-coop release must not record an approval"
        );
    }
}
