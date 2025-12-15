//! Recurring Payments API
//!
//! Support for scheduled and recurring payment automation.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

/// Payment frequency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentFrequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Recurring payment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecurringStatus {
    Active,
    Paused,
    Cancelled,
    Completed,
}

/// Recurring payment schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringPayment {
    /// Unique ID
    pub id: String,
    /// Owner/creator DID
    pub owner: String,
    /// From account
    pub from_account: String,
    /// To account
    pub to_account: String,
    /// Amount per payment
    pub amount: i64,
    /// Currency
    pub currency: String,
    /// Payment frequency
    pub frequency: PaymentFrequency,
    /// Next execution timestamp
    pub next_execution: u64,
    /// Optional end date
    pub end_date: Option<u64>,
    /// Current status
    pub status: RecurringStatus,
    /// Total executions
    pub execution_count: u64,
    /// Last execution timestamp
    pub last_execution: Option<u64>,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
}

/// In-memory store for recurring payments (production should use persistent storage)
#[derive(Clone)]
pub struct RecurringPaymentStore {
    payments: Arc<RwLock<HashMap<String, RecurringPayment>>>,
}

impl RecurringPaymentStore {
    pub fn new() -> Self {
        Self {
            payments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(&self, payment: RecurringPayment) {
        self.payments.write().await.insert(payment.id.clone(), payment);
    }

    pub async fn get(&self, id: &str) -> Option<RecurringPayment> {
        self.payments.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<RecurringPayment> {
        self.payments.read().await.values().cloned().collect()
    }

    pub async fn update(&self, id: &str, payment: RecurringPayment) -> bool {
        let mut payments = self.payments.write().await;
        if payments.contains_key(id) {
            payments.insert(id.to_string(), payment);
            true
        } else {
            false
        }
    }

    pub async fn delete(&self, id: &str) -> bool {
        self.payments.write().await.remove(id).is_some()
    }

    /// Get payments ready for execution
    pub async fn get_due_payments(&self) -> Vec<RecurringPayment> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.payments
            .read()
            .await
            .values()
            .filter(|p| {
                p.status == RecurringStatus::Active && p.next_execution <= now
            })
            .cloned()
            .collect()
    }
}

/// Request to create recurring payment
#[derive(Debug, Deserialize)]
pub struct CreateRecurringPaymentRequest {
    pub from_account: String,
    pub to_account: String,
    pub amount: i64,
    pub currency: String,
    pub frequency: PaymentFrequency,
    pub start_date: Option<u64>,
    pub end_date: Option<u64>,
}

/// Request to update recurring payment
#[derive(Debug, Deserialize)]
pub struct UpdateRecurringPaymentRequest {
    pub amount: Option<i64>,
    pub frequency: Option<PaymentFrequency>,
    pub end_date: Option<u64>,
    pub status: Option<RecurringStatus>,
}

/// POST /payments/recurring - Create recurring payment
#[post("/payments/recurring")]
pub async fn create_recurring_payment(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    req: web::Json<CreateRecurringPaymentRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let next_execution = req.start_date.unwrap_or(now);

    let payment = RecurringPayment {
        id: uuid::Uuid::new_v4().to_string(),
        owner: owner.to_string(),
        from_account: req.from_account.clone(),
        to_account: req.to_account.clone(),
        amount: req.amount,
        currency: req.currency.clone(),
        frequency: req.frequency,
        next_execution,
        end_date: req.end_date,
        status: RecurringStatus::Active,
        execution_count: 0,
        last_execution: None,
        created_at: now,
        updated_at: now,
    };

    store.insert(payment.clone()).await;

    Ok(HttpResponse::Created().json(payment))
}

/// GET /payments/recurring - List recurring payments
#[get("/payments/recurring")]
pub async fn list_recurring_payments(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let mut payments = store.list().await;

    // Filter by owner
    payments.retain(|p| p.owner == owner_did);

    // Filter by status if provided
    if let Some(status_str) = query.get("status") {
        let status = match status_str.as_str() {
            "active" => RecurringStatus::Active,
            "paused" => RecurringStatus::Paused,
            "cancelled" => RecurringStatus::Cancelled,
            "completed" => RecurringStatus::Completed,
            _ => {
                return Err(GatewayError::BadRequest(format!(
                    "Invalid status: {}",
                    status_str
                )))
            }
        };
        payments.retain(|p| p.status == status);
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "payments": payments,
        "count": payments.len()
    })))
}

/// GET /payments/recurring/{id} - Get recurring payment
#[get("/payments/recurring/{id}")]
pub async fn get_recurring_payment(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let payment_id = id.into_inner();
    let payment = store
        .get(&payment_id)
        .await
        .ok_or_else(|| GatewayError::NotFound(format!("Payment not found: {}", payment_id)))?;

    // Verify ownership
    if payment.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to access this payment".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(payment))
}

/// PUT /payments/recurring/{id} - Update recurring payment
#[put("/payments/recurring/{id}")]
pub async fn update_recurring_payment(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    id: web::Path<String>,
    req: web::Json<UpdateRecurringPaymentRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let payment_id = id.into_inner();
    let mut payment = store
        .get(&payment_id)
        .await
        .ok_or_else(|| GatewayError::NotFound(format!("Payment not found: {}", payment_id)))?;

    // Verify ownership
    if payment.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to modify this payment".to_string(),
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

    payment.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    store.update(&payment_id, payment.clone()).await;

    Ok(HttpResponse::Ok().json(payment))
}

/// DELETE /payments/recurring/{id} - Cancel recurring payment
#[delete("/payments/recurring/{id}")]
pub async fn cancel_recurring_payment(
    http_req: HttpRequest,
    store: web::Data<RecurringPaymentStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let payment_id = id.into_inner();
    let mut payment = store
        .get(&payment_id)
        .await
        .ok_or_else(|| GatewayError::NotFound(format!("Payment not found: {}", payment_id)))?;

    // Verify ownership
    if payment.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to cancel this payment".to_string(),
        ));
    }

    // Mark as cancelled
    payment.status = RecurringStatus::Cancelled;
    payment.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    store.update(&payment_id, payment.clone()).await;

    Ok(HttpResponse::Ok().json(payment))
}

/// Configure recurring payment routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_recurring_payment)
        .service(list_recurring_payments)
        .service(get_recurring_payment)
        .service(update_recurring_payment)
        .service(cancel_recurring_payment);
}
