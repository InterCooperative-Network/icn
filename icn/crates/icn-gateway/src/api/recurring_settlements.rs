//! Recurring Settlements API
//!
//! Support for scheduled and recurring settlement automation.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use icn_identity::Did;
pub use icn_ledger_app::{
    PaymentFrequency, RecurringPayment, RecurringPaymentStore, RecurringStatus,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::ledger_mgr::LedgerManager;
use crate::middleware::{get_claims, require_coop_access, require_scope};

/// Request to create recurring settlement
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRecurringSettlementRequest {
    /// Cooperative ID (ledger namespace)
    pub coop_id: String,
    pub from_account: String,
    pub to_account: String,
    pub amount: i64,
    pub unit: String,
    pub frequency: PaymentFrequency,
    pub start_date: Option<u64>,
    pub end_date: Option<u64>,
}

/// Request to update recurring settlement
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateRecurringSettlementRequest {
    pub amount: Option<i64>,
    pub frequency: Option<PaymentFrequency>,
    pub end_date: Option<u64>,
    pub status: Option<RecurringStatus>,
}

/// POST /settlements/recurring - Create recurring settlement
#[post("/settlements/recurring")]
pub async fn create_recurring_settlement(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    req: web::Json<CreateRecurringSettlementRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:write")?;
    // Bind the body-supplied coop_id to the caller's token coop. `settlements:write`
    // is requestable per coop, so without this a token for coop A could schedule a
    // recurring settlement under coop B. CRITICAL: prevent cross-coop write.
    require_coop_access(&http_req, &req.coop_id)?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let now = icn_time::current_timestamp_secs();

    let next_execution = req.start_date.unwrap_or(now);

    let payment = RecurringPayment {
        id: uuid::Uuid::new_v4().to_string(),
        coop_id: req.coop_id.clone(),
        owner: owner.to_string(),
        from_account: req.from_account.clone(),
        to_account: req.to_account.clone(),
        amount: req.amount,
        currency: req.unit.clone(),
        frequency: req.frequency,
        next_execution,
        end_date: req.end_date,
        status: RecurringStatus::Active,
        execution_count: 0,
        last_execution: None,
        created_at: now,
        updated_at: now,
    };

    store
        .insert(payment.clone())
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

    Ok(HttpResponse::Created().json(payment))
}

/// GET /settlements/recurring - List recurring settlements
#[get("/settlements/recurring")]
pub async fn list_recurring_settlements(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    // Use optimized query if possible, otherwise list all and filter (for now, store.list() does scan)
    // Optimally: store.list_by_owner(&owner_did)
    let mut payments = store
        .list_by_owner(&owner_did)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

    // Filter by status if provided
    if let Some(status_str) = query.get("status") {
        let status = match status_str.as_str() {
            "active" => RecurringStatus::Active,
            "paused" => RecurringStatus::Paused,
            "cancelled" => RecurringStatus::Cancelled,
            "completed" => RecurringStatus::Completed,
            _ => {
                return Err(GatewayError::BadRequest(format!(
                    "Invalid status: {status_str}"
                )))
            }
        };
        payments.retain(|p| p.status == status);
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "settlements": payments,
        "count": payments.len()
    })))
}

/// GET /settlements/recurring/{id} - Get recurring settlement
#[get("/settlements/recurring/{id}")]
pub async fn get_recurring_settlement(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let payment_id = id.into_inner();
    let payment = store
        .get(&payment_id)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Settlement not found: {payment_id}")))?;

    // Verify ownership
    if payment.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to access this settlement".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(payment))
}

/// PUT /settlements/recurring/{id} - Update recurring settlement
#[put("/settlements/recurring/{id}")]
pub async fn update_recurring_settlement(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    id: web::Path<String>,
    req: web::Json<UpdateRecurringSettlementRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let payment_id = id.into_inner();
    let mut payment = store
        .get(&payment_id)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Settlement not found: {payment_id}")))?;

    // Verify ownership
    if payment.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to modify this settlement".to_string(),
        ));
    }

    // Apply updates
    if let Some(amount) = req.amount {
        payment.amount = amount;
    }
    if let Some(frequency) = req.frequency {
        payment.frequency = frequency;
    }
    if let Some(end_date) = req.end_date {
        payment.end_date = Some(end_date);
    }
    if let Some(status) = req.status {
        payment.status = status;
    }

    payment.updated_at = icn_time::current_timestamp_secs();

    store
        .update(&payment_id, payment.clone())
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

    Ok(HttpResponse::Ok().json(payment))
}

/// DELETE /settlements/recurring/{id} - Cancel recurring settlement
#[delete("/settlements/recurring/{id}")]
pub async fn cancel_recurring_settlement(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let payment_id = id.into_inner();
    let mut payment = store
        .get(&payment_id)
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Settlement not found: {payment_id}")))?;

    // Verify ownership
    if payment.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to cancel this settlement".to_string(),
        ));
    }

    // Mark as cancelled
    payment.status = RecurringStatus::Cancelled;
    payment.updated_at = icn_time::current_timestamp_secs();

    store
        .update(&payment_id, payment.clone())
        .map_err(|e| GatewayError::InternalError(format!("Store error: {e}")))?;

    Ok(HttpResponse::Ok().json(payment))
}

/// Configure recurring settlement routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_recurring_settlement)
        .service(list_recurring_settlements)
        .service(get_recurring_settlement)
        .service(update_recurring_settlement)
        .service(cancel_recurring_settlement);
}

// ============================================================================
// Scheduler Functions
// ============================================================================

/// Calculate next execution time based on frequency
fn calculate_next_execution(current: u64, frequency: PaymentFrequency) -> u64 {
    match frequency {
        PaymentFrequency::Daily => current + 86400,   // 24 hours
        PaymentFrequency::Weekly => current + 604800, // 7 days
        PaymentFrequency::Monthly => current + 2592000, // 30 days (approximate)
        PaymentFrequency::Yearly => current + 31536000, // 365 days
    }
}

/// Execute all due recurring settlements
///
/// This function should be called periodically by a scheduler (e.g., every minute).
/// It executes all settlements that are due and updates their next execution time.
pub async fn execute_due_settlements(
    store: &RecurringPaymentStore,
    ledger_mgr: &Arc<LedgerManager>,
) -> Vec<(String, std::result::Result<String, String>)> {
    let now = icn_time::current_timestamp_secs();

    let due_payments = match store.get_due_payments(now) {
        Ok(payments) => payments,
        Err(e) => {
            warn!(error = %e, "Failed to get due payments");
            return Vec::new();
        }
    };

    let mut results = Vec::new();

    debug!(
        count = due_payments.len(),
        "Checking due recurring payments"
    );

    for mut payment in due_payments {
        let payment_id = payment.id.clone();

        // Parse DIDs
        let from_did: Did = match payment.from_account.parse() {
            Ok(did) => did,
            Err(e) => {
                warn!(
                    payment_id = %payment_id,
                    error = %e,
                    "Invalid from_account DID in recurring payment"
                );
                results.push((payment_id, Err(format!("Invalid from_account: {e}"))));
                continue;
            }
        };

        let to_did: Did = match payment.to_account.parse() {
            Ok(did) => did,
            Err(e) => {
                warn!(
                    payment_id = %payment_id,
                    error = %e,
                    "Invalid to_account DID in recurring payment"
                );
                results.push((payment_id, Err(format!("Invalid to_account: {e}"))));
                continue;
            }
        };

        // Execute settlement via ledger
        match ledger_mgr
            .create_settlement(
                &payment.coop_id,
                &to_did,
                &from_did,
                payment.amount,
                payment.currency.clone(),
                None, // system-executed path: no per-request JWT proof
            )
            .await
        {
            Ok(tx_hash) => {
                info!(
                    payment_id = %payment_id,
                    tx_hash = %tx_hash,
                    amount = payment.amount,
                    execution_count = payment.execution_count + 1,
                    "Recurring payment executed"
                );

                let now = icn_time::current_timestamp_secs();

                // Update payment state
                payment.execution_count += 1;
                payment.last_execution = Some(now);
                payment.next_execution = calculate_next_execution(now, payment.frequency);
                payment.updated_at = now;

                // Check if payment has reached end date
                if let Some(end_date) = payment.end_date {
                    if payment.next_execution > end_date {
                        payment.status = RecurringStatus::Completed;
                        info!(
                            payment_id = %payment_id,
                            "Recurring payment completed (reached end date)"
                        );
                    }
                }

                if let Err(e) = store.update(&payment_id, payment) {
                    warn!(error = %e, payment_id = %payment_id, "Failed to update payment status after execution");
                }
                results.push((payment_id, Ok(tx_hash)));
            }
            Err(e) => {
                warn!(
                    payment_id = %payment_id,
                    error = %e,
                    "Failed to execute recurring payment"
                );
                results.push((payment_id, Err(format!("Ledger error: {e}"))));
            }
        }
    }

    results
}

/// Start the recurring payments scheduler
///
/// Spawns a background task that checks for and executes due payments
/// at the specified interval.
pub fn start_scheduler(
    store: RecurringPaymentStore,
    ledger_mgr: Arc<LedgerManager>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            interval_secs = check_interval_secs,
            "Starting recurring settlements scheduler"
        );

        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(check_interval_secs));

        loop {
            interval.tick().await;

            let results = execute_due_settlements(&store, &ledger_mgr).await;

            if !results.is_empty() {
                let succeeded = results.iter().filter(|(_, r)| r.is_ok()).count();
                let failed = results.iter().filter(|(_, r)| r.is_err()).count();
                info!(
                    succeeded = succeeded,
                    failed = failed,
                    "Recurring settlements scheduler tick completed"
                );
            }
        }
    })
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
            scopes: vec!["settlements:write".to_string()],
        }
    }

    /// `settlements:write` is requestable per coop, so `create_recurring_settlement`
    /// binds the body coop_id to the token's coop. Same-coop create succeeds (201)
    /// and persists exactly one settlement; cross-coop is rejected (403) and stores
    /// nothing. Store-state assertions prove "no mutation on reject".
    #[actix_web::test]
    async fn create_recurring_settlement_rejects_cross_coop_write() {
        async fn run(
            token_coop: &str,
            body_coop: &str,
            sub: &str,
        ) -> (StatusCode, RecurringPaymentStore) {
            let db = sled::Config::new().temporary(true).open().unwrap();
            let store = RecurringPaymentStore::new(db.clone());
            let probe = RecurringPaymentStore::new(db);
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(store))
                    .service(create_recurring_settlement),
            )
            .await;
            let req = actix_test::TestRequest::post()
                .uri("/settlements/recurring")
                .set_json(serde_json::json!({
                    "coop_id": body_coop,
                    "from_account": "acct-from",
                    "to_account": "acct-to",
                    "amount": 100,
                    "unit": "hours",
                    "frequency": "daily"
                }))
                .to_request();
            req.extensions_mut().insert(write_claims(token_coop, sub));
            let status = actix_test::call_service(&app, req).await.status();
            (status, probe)
        }

        // The success path parses claims.sub into a Did, so use a real DID subject.
        let sub = icn_identity::KeyPair::generate().unwrap().did().to_string();

        // Assert on persisted state via `list()` (full primary-key scan) rather than
        // `list_by_owner()`: the latter splits the index key on ':' expecting 3 parts
        // and so cannot match DID owners (which themselves contain ':'). That's a
        // pre-existing store-layer limitation, out of scope for this authz fix.
        let (same, same_store) = run("coopA", "coopA", &sub).await;
        assert_eq!(same, StatusCode::CREATED);
        assert_eq!(
            same_store.list().unwrap().len(),
            1,
            "same-coop create should persist exactly one recurring settlement"
        );

        let (cross, cross_store) = run("coopA", "coopB", &sub).await;
        assert_eq!(cross, StatusCode::FORBIDDEN);
        assert!(
            cross_store.list().unwrap().is_empty(),
            "cross-coop reject must not create a recurring settlement"
        );
    }
}
