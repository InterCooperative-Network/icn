//! Budget Limits API
//!
//! Spending controls and budget management for cooperative accounts.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

/// Budget period
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetPeriod {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Budget status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetStatus {
    Active,
    Paused,
    Exceeded,
    Expired,
}

/// Budget limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    /// Unique ID
    pub id: String,
    /// Owner/creator DID
    pub owner: String,
    /// Account this budget applies to
    pub account: String,
    /// Currency
    pub currency: String,
    /// Spending limit
    pub limit: i64,
    /// Current spending in this period
    pub spent: i64,
    /// Budget period
    pub period: BudgetPeriod,
    /// Period start timestamp
    pub period_start: u64,
    /// Period end timestamp
    pub period_end: u64,
    /// Current status
    pub status: BudgetStatus,
    /// Notification thresholds (percentages)
    pub notification_thresholds: Vec<u8>,
    /// Thresholds that have been notified
    pub notified_thresholds: Vec<u8>,
    /// Description
    pub description: String,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
}

impl Budget {
    /// Check if limit is exceeded
    pub fn is_exceeded(&self) -> bool {
        self.spent >= self.limit
    }

    /// Get remaining budget
    pub fn remaining(&self) -> i64 {
        self.limit - self.spent
    }

    /// Get percentage used
    pub fn percentage_used(&self) -> f64 {
        if self.limit == 0 {
            return 100.0;
        }
        (self.spent as f64 / self.limit as f64) * 100.0
    }

    /// Check if threshold should trigger notification
    pub fn should_notify(&self) -> Option<u8> {
        let pct = self.percentage_used() as u8;
        for threshold in &self.notification_thresholds {
            if pct >= *threshold && !self.notified_thresholds.contains(threshold) {
                return Some(*threshold);
            }
        }
        None
    }
}

/// In-memory budget store
#[derive(Clone)]
pub struct BudgetStore {
    budgets: Arc<RwLock<HashMap<String, Budget>>>,
}

impl BudgetStore {
    pub fn new() -> Self {
        Self {
            budgets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(&self, budget: Budget) {
        self.budgets.write().await.insert(budget.id.clone(), budget);
    }

    pub async fn get(&self, id: &str) -> Option<Budget> {
        self.budgets.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Budget> {
        self.budgets.read().await.values().cloned().collect()
    }

    pub async fn update(&self, id: &str, budget: Budget) -> bool {
        let mut budgets = self.budgets.write().await;
        if budgets.contains_key(id) {
            budgets.insert(id.to_string(), budget);
            true
        } else {
            false
        }
    }

    pub async fn delete(&self, id: &str) -> bool {
        self.budgets.write().await.remove(id).is_some()
    }

    /// Check spending against budget
    pub async fn check_spending(&self, account: &str, amount: i64) -> Result<bool> {
        let budgets = self.budgets.read().await;
        
        for budget in budgets.values() {
            if budget.account == account && budget.status == BudgetStatus::Active {
                if budget.spent + amount > budget.limit {
                    return Ok(false);
                }
            }
        }
        
        Ok(true)
    }

    /// Record spending
    pub async fn record_spending(&self, account: &str, amount: i64) -> Result<Vec<String>> {
        let mut notified_budgets = Vec::new();
        let mut budgets = self.budgets.write().await;

        for budget in budgets.values_mut() {
            if budget.account == account && budget.status == BudgetStatus::Active {
                budget.spent += amount;
                budget.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                // Check if exceeded
                if budget.is_exceeded() {
                    budget.status = BudgetStatus::Exceeded;
                }

                // Check notification thresholds
                if let Some(threshold) = budget.should_notify() {
                    budget.notified_thresholds.push(threshold);
                    notified_budgets.push(budget.id.clone());
                }
            }
        }

        Ok(notified_budgets)
    }
}

/// Request to create budget
#[derive(Debug, Deserialize)]
pub struct CreateBudgetRequest {
    pub account: String,
    pub currency: String,
    pub limit: i64,
    pub period: BudgetPeriod,
    pub description: String,
    pub notification_thresholds: Option<Vec<u8>>,
}

/// Request to update budget
#[derive(Debug, Deserialize)]
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

    // Calculate period end based on period type
    let period_end = calculate_period_end(now, req.period);

    let default_thresholds = vec![80, 100];
    let notification_thresholds = req.notification_thresholds.clone()
        .unwrap_or(default_thresholds);

    let budget = Budget {
        id: uuid::Uuid::new_v4().to_string(),
        owner: owner.to_string(),
        account: req.account.clone(),
        currency: req.currency.clone(),
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

    store.insert(budget.clone()).await;

    Ok(HttpResponse::Created().json(budget))
}

/// GET /budgets - List budgets
#[get("/budgets")]
pub async fn list_budgets(
    http_req: HttpRequest,
    store: web::Data<BudgetStore>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let mut budgets = store.list().await;

    // Filter by owner
    budgets.retain(|b| b.owner == owner_did);

    // Filter by status if provided
    if let Some(status_str) = query.get("status") {
        let status = match status_str.as_str() {
            "active" => BudgetStatus::Active,
            "paused" => BudgetStatus::Paused,
            "exceeded" => BudgetStatus::Exceeded,
            "expired" => BudgetStatus::Expired,
            _ => {
                return Err(GatewayError::BadRequest(format!(
                    "Invalid status: {}",
                    status_str
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
    require_scope(&http_req, "payments:read")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let budget_id = id.into_inner();
    let budget = store
        .get(&budget_id)
        .await
        .ok_or_else(|| GatewayError::NotFound(format!("Budget not found: {}", budget_id)))?;

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
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let budget_id = id.into_inner();
    let mut budget = store
        .get(&budget_id)
        .await
        .ok_or_else(|| GatewayError::NotFound(format!("Budget not found: {}", budget_id)))?;

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

    budget.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    store.update(&budget_id, budget.clone()).await;

    Ok(HttpResponse::Ok().json(budget))
}

/// DELETE /budgets/{id} - Delete budget
#[delete("/budgets/{id}")]
pub async fn delete_budget(
    http_req: HttpRequest,
    store: web::Data<BudgetStore>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "payments:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub;

    let budget_id = id.into_inner();
    let budget = store
        .get(&budget_id)
        .await
        .ok_or_else(|| GatewayError::NotFound(format!("Budget not found: {}", budget_id)))?;

    // Verify ownership
    if budget.owner != owner_did {
        return Err(GatewayError::AuthorizationFailed(
            "Not authorized to delete this budget".to_string(),
        ));
    }

    store.delete(&budget_id).await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Budget deleted",
        "id": budget_id
    })))
}

/// Calculate period end timestamp
fn calculate_period_end(start: u64, period: BudgetPeriod) -> u64 {
    match period {
        BudgetPeriod::Daily => start + 86400,         // 24 hours
        BudgetPeriod::Weekly => start + 604800,       // 7 days
        BudgetPeriod::Monthly => start + 2592000,     // 30 days (approximation)
        BudgetPeriod::Yearly => start + 31536000,     // 365 days
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
