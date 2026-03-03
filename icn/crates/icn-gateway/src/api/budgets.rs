//! Budget Limits API
//!
//! Spending controls and budget management for cooperative accounts.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use icn_identity::Did;
pub use icn_ledger_app::{Budget, BudgetPeriod, BudgetStatus, BudgetStore};
use serde::Deserialize;
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

/// Request to create budget
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBudgetRequest {
    pub account: String,
    pub unit: String,
    pub limit: i64,
    pub period: BudgetPeriod,
    pub description: String,
    pub notification_thresholds: Option<Vec<u8>>,
}

/// Request to update budget
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBudgetRequest {
    pub limit: Option<i64>,
    pub status: Option<BudgetStatus>,
    pub notification_thresholds: Option<Vec<u8>>,
}

/// POST /budgets - Create budget
#[post("/budgets")]
pub async fn create_budget(
    http_req: HttpRequest,
    store: web::Data<BudgetStore>,
    req: web::Json<CreateBudgetRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let now = icn_time::current_timestamp_secs();

    // Calculate period end based on period type
    let period_end = calculate_period_end(now, req.period);

    let default_thresholds = vec![80, 100];
    let notification_thresholds = req
        .notification_thresholds
        .clone()
        .unwrap_or(default_thresholds);

    let budget = Budget {
        id: uuid::Uuid::new_v4().to_string(),
        owner: owner.to_string(),
        account: req.account.clone(),
        currency: req.unit.clone(),
        limit: req.limit,
        spent: 0,
        period: req.period,
        period_start: now,
        period_end,
        status: BudgetStatus::Active,
        notification_thresholds,
        notified_thresholds: Vec::new(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    store
        .insert(budget.clone())
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    Ok(HttpResponse::Created().json(budget))
}

/// GET /budgets - List budgets
#[get("/budgets")]
pub async fn list_budgets(
    http_req: HttpRequest,
    store: web::Data<BudgetStore>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    // Use indexed lookup by owner
    let mut budgets = store
        .list_by_owner(&owner_did)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    // Filter by status if provided
    if let Some(status_str) = query.get("status") {
        let status = match status_str.as_str() {
            "active" => BudgetStatus::Active,
            "paused" => BudgetStatus::Paused,
            "exceeded" => BudgetStatus::Exceeded,
            "expired" => BudgetStatus::Expired,
            _ => {
                return Err(GatewayError::BadRequest(format!(
                    "Invalid status: {status_str}"
                )))
            }
        };
        budgets.retain(|b| b.status == status);
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "budgets": budgets,
        "count": budgets.len()
    })))
}

/// GET /budgets/{id} - Get budget details
#[get("/budgets/{id}")]
pub async fn get_budget(
    http_req: HttpRequest,
    store: web::Data<BudgetStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let budget_id = id.into_inner();
    let budget = store
        .get(&budget_id)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound(format!("Budget not found: {budget_id}")))?;

    // Verify ownership
    if budget.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to view this budget".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "budget": budget,
        "remaining": budget.remaining(),
        "percentage_used": budget.percentage_used(),
        "is_exceeded": budget.is_exceeded()
    })))
}

/// PUT /budgets/{id} - Update budget
#[put("/budgets/{id}")]
pub async fn update_budget(
    http_req: HttpRequest,
    store: web::Data<BudgetStore>,
    id: web::Path<String>,
    req: web::Json<UpdateBudgetRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let budget_id = id.into_inner();
    let mut budget = store
        .get(&budget_id)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound(format!("Budget not found: {budget_id}")))?;

    // Verify ownership
    if budget.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to modify this budget".to_string(),
        ));
    }

    // Apply updates
    if let Some(limit) = req.limit {
        budget.limit = limit;
        // Re-check if exceeded
        if budget.is_exceeded() {
            budget.status = BudgetStatus::Exceeded;
        }
    }
    if let Some(status) = req.status {
        budget.status = status;
    }
    if let Some(thresholds) = &req.notification_thresholds {
        budget.notification_thresholds = thresholds.clone();
    }

    budget.updated_at = icn_time::current_timestamp_secs();

    store
        .update(&budget_id, budget.clone())
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    Ok(HttpResponse::Ok().json(budget))
}

/// DELETE /budgets/{id} - Delete budget
#[delete("/budgets/{id}")]
pub async fn delete_budget(
    http_req: HttpRequest,
    store: web::Data<BudgetStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "settlements:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let budget_id = id.into_inner();
    let budget = store
        .get(&budget_id)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound(format!("Budget not found: {budget_id}")))?;

    // Verify ownership
    if budget.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to delete this budget".to_string(),
        ));
    }

    store
        .delete(&budget_id)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Budget deleted",
        "id": budget_id
    })))
}

/// Calculate period end timestamp
fn calculate_period_end(start: u64, period: BudgetPeriod) -> u64 {
    match period {
        BudgetPeriod::Daily => start + 86400,     // 24 hours
        BudgetPeriod::Weekly => start + 604800,   // 7 days
        BudgetPeriod::Monthly => start + 2592000, // 30 days (approximation)
        BudgetPeriod::Yearly => start + 31536000, // 365 days
    }
}

/// Configure budget routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_budget)
        .service(list_budgets)
        .service(get_budget)
        .service(update_budget)
        .service(delete_budget);
}
