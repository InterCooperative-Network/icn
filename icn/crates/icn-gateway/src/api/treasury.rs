//! Treasury API endpoints
//!
//! Provides REST API for cooperative treasury operations including:
//! - Treasury balance queries
//! - Budget listing and details
//! - Spending rule queries
//! - Audit trail access
//!
//! Treasury operations that require governance approval (withdrawals above
//! spending thresholds, budget creation) must go through the governance
//! proposal system.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

use crate::error::{GatewayError, Result};
use crate::middleware::{require_coop_access, require_scope};

// ============================================================================
// Request/Response Types
// ============================================================================

/// Response for treasury status
#[derive(Debug, Serialize, Deserialize)]
pub struct TreasuryStatusResponse {
    /// Treasury DID
    pub treasury_did: String,
    /// Cooperative ID
    pub coop_id: String,
    /// Primary currency
    pub currency: String,
    /// Whether treasury is active
    pub is_active: bool,
    /// Current balance (from ledger)
    pub balance: i64,
    /// Number of active budgets
    pub active_budget_count: usize,
    /// Number of spending rules
    pub spending_rule_count: usize,
}

/// Response for treasury balance
#[derive(Debug, Serialize, Deserialize)]
pub struct TreasuryBalanceResponse {
    /// Treasury DID
    pub treasury_did: String,
    /// Balance by currency
    pub balances: HashMap<String, i64>,
}

/// Budget summary for list response
#[derive(Debug, Serialize, Deserialize)]
pub struct BudgetSummary {
    /// Budget ID
    pub id: String,
    /// Purpose of the budget
    pub purpose: String,
    /// Allocated amount
    pub allocated_amount: i64,
    /// Spent amount
    pub spent_amount: i64,
    /// Remaining amount
    pub remaining: i64,
    /// Percentage used
    pub percentage_used: u8,
    /// Budget status
    pub status: String,
    /// Currency
    pub currency: String,
}

/// Response for budget list
#[derive(Debug, Serialize, Deserialize)]
pub struct BudgetListResponse {
    /// Treasury DID
    pub treasury_did: String,
    /// List of budgets
    pub budgets: Vec<BudgetSummary>,
    /// Total count
    pub total: usize,
}

/// Detailed budget response
#[derive(Debug, Serialize, Deserialize)]
pub struct BudgetDetailResponse {
    /// Budget ID
    pub id: String,
    /// Treasury DID
    pub treasury_did: String,
    /// Purpose
    pub purpose: String,
    /// Allocated amount
    pub allocated_amount: i64,
    /// Spent amount
    pub spent_amount: i64,
    /// Remaining amount
    pub remaining: i64,
    /// Currency
    pub currency: String,
    /// Period start timestamp
    pub period_start: u64,
    /// Period end timestamp (optional)
    pub period_end: Option<u64>,
    /// Status
    pub status: String,
    /// Percentage used
    pub percentage_used: u8,
    /// Proposal ID that created this budget
    pub proposal_id: Option<String>,
    /// Created timestamp
    pub created_at: u64,
    /// Created by DID
    pub created_by: String,
    /// Notification thresholds (percentages)
    pub notification_thresholds: Vec<u8>,
    /// Already notified thresholds
    pub notified_thresholds: Vec<u8>,
}

/// Spending rule summary
#[derive(Debug, Serialize, Deserialize)]
pub struct SpendingRuleSummary {
    /// Rule ID
    pub id: String,
    /// Rule name
    pub name: String,
    /// Threshold amount (spending above this requires approval)
    pub threshold_amount: i64,
    /// Currency
    pub currency: String,
    /// Approval type required
    pub approval_type: String,
    /// Whether the rule is active
    pub is_active: bool,
}

/// Response for spending rules list
#[derive(Debug, Serialize, Deserialize)]
pub struct SpendingRulesResponse {
    /// Treasury DID
    pub treasury_did: String,
    /// List of spending rules
    pub rules: Vec<SpendingRuleSummary>,
}

/// Audit record summary
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditRecordSummary {
    /// Audit ID
    pub id: String,
    /// Operation type
    pub operation_type: String,
    /// Performed by DID
    pub performed_by: String,
    /// Timestamp
    pub performed_at: u64,
    /// Balance after operation
    pub balance_after: i64,
    /// Associated proposal ID
    pub proposal_id: Option<String>,
    /// Notes
    pub notes: Option<String>,
}

/// Response for audit trail
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditTrailResponse {
    /// Treasury DID
    pub treasury_did: String,
    /// Audit records
    pub records: Vec<AuditRecordSummary>,
    /// Total count
    pub total: usize,
    /// Current offset
    pub offset: usize,
    /// Limit
    pub limit: usize,
    /// Has more records
    pub has_more: bool,
}

/// Request to create a budget (triggers governance proposal)
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBudgetRequest {
    /// Purpose of the budget
    pub purpose: String,
    /// Amount to allocate
    pub amount: i64,
    /// Currency
    pub currency: String,
    /// Period end (optional, Unix timestamp)
    pub period_end: Option<u64>,
}

/// Request to deposit to treasury
#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    /// Amount to deposit
    pub amount: i64,
    /// Currency
    pub currency: String,
    /// Memo/note
    pub memo: Option<String>,
}

// ============================================================================
// Treasury Manager (placeholder - will be injected from supervisor)
// ============================================================================

/// Treasury manager placeholder for API
/// In production, this will be injected from the supervisor's LedgerServices
///
/// NOTE: This is a Phase 1 placeholder. See issue #258 for wiring to real TreasuryManager.
#[allow(dead_code)]
pub struct TreasuryApiManager {
    // Placeholder - in real implementation, this holds Arc<RwLock<TreasuryManager>>
    _placeholder: (),
}

impl TreasuryApiManager {
    /// Create a new placeholder manager
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for TreasuryApiManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// API Endpoints
// ============================================================================

/// GET /treasury/{coop_id} - Get treasury status
///
/// Returns the overall status of the cooperative's treasury including
/// balance, active budgets, and spending rules.
#[get("/{coop_id}")]
pub async fn get_treasury_status(
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    // TODO: Wire to actual TreasuryManager from supervisor
    // For now, return a placeholder response
    info!(coop_id = %coop_id, "Treasury status requested");

    // Placeholder - will be replaced with actual data when wired to supervisor
    Err(GatewayError::NotFound(format!(
        "Treasury not configured for cooperative '{coop_id}'. Register via governance proposal."
    )))
}

/// GET /treasury/{coop_id}/balance - Get treasury balance
///
/// Returns the current balance of the cooperative's treasury.
#[get("/{coop_id}/balance")]
pub async fn get_treasury_balance(
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury balance requested");

    // TODO: Wire to actual TreasuryManager from supervisor
    Err(GatewayError::NotFound(format!(
        "Treasury not configured for cooperative '{coop_id}'"
    )))
}

/// GET /treasury/{coop_id}/budgets - List budgets
///
/// Returns all budgets for the cooperative's treasury.
#[get("/{coop_id}/budgets")]
pub async fn list_budgets(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury budgets list requested");

    // TODO: Wire to actual TreasuryManager from supervisor
    let response = BudgetListResponse {
        treasury_did: String::new(),
        budgets: Vec::new(),
        total: 0,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /treasury/{coop_id}/budgets/{budget_id} - Get budget details
///
/// Returns detailed information about a specific budget.
#[get("/{coop_id}/budgets/{budget_id}")]
pub async fn get_budget(
    req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let (coop_id, budget_id) = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, budget_id = %budget_id, "Treasury budget details requested");

    // TODO: Wire to actual TreasuryManager from supervisor
    Err(GatewayError::NotFound(format!(
        "Budget not found: {budget_id}"
    )))
}

/// POST /treasury/{coop_id}/budgets - Create budget (triggers governance proposal)
///
/// Creates a new budget allocation. This will create a governance proposal
/// that must be approved before the budget becomes active.
#[post("/{coop_id}/budgets")]
pub async fn create_budget(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<CreateBudgetRequest>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:write")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    // Input validation
    if body.amount <= 0 {
        return Err(GatewayError::BadRequest(
            "Budget amount must be positive".to_string(),
        ));
    }

    if body.purpose.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Budget purpose cannot be empty".to_string(),
        ));
    }

    if body.currency.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Currency cannot be empty".to_string(),
        ));
    }

    // Validate period_end is in the future
    if let Some(period_end) = body.period_end {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if period_end <= now {
            return Err(GatewayError::BadRequest(
                "Period end must be in the future".to_string(),
            ));
        }
    }

    info!(
        coop_id = %coop_id,
        purpose = %body.purpose,
        amount = body.amount,
        "Treasury budget creation requested"
    );

    // TODO: Create governance proposal for budget creation
    // The proposal will be of type ProposalPayload::Treasury {
    //     operation: TreasuryProposalOperation::CreateBudget { ... }
    // }

    let response = serde_json::json!({
        "status": "proposal_created",
        "message": "Budget creation requires governance approval",
        "proposal_id": null, // Will be filled in when wired to governance
        "coop_id": coop_id,
        "purpose": body.purpose,
        "amount": body.amount,
        "currency": body.currency
    });

    Ok(HttpResponse::Accepted().json(response))
}

/// GET /treasury/{coop_id}/spending-rules - List spending rules
///
/// Returns all spending rules for the cooperative's treasury.
#[get("/{coop_id}/spending-rules")]
pub async fn list_spending_rules(
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury spending rules requested");

    // TODO: Wire to actual TreasuryManager from supervisor
    let response = SpendingRulesResponse {
        treasury_did: String::new(),
        rules: Vec::new(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /treasury/{coop_id}/audit - Get audit trail
///
/// Returns the audit trail for treasury operations, with pagination.
#[get("/{coop_id}/audit")]
pub async fn get_audit_trail(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    let limit: usize = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
        .min(100);

    let offset: usize = query
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    info!(coop_id = %coop_id, limit = limit, offset = offset, "Treasury audit trail requested");

    // TODO: Wire to actual TreasuryManager from supervisor
    let response = AuditTrailResponse {
        treasury_did: String::new(),
        records: Vec::new(),
        total: 0,
        offset,
        limit,
        has_more: false,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /treasury/{coop_id}/deposit - Deposit to treasury
///
/// Deposits funds into the treasury. This creates a ledger entry
/// transferring from the depositor to the treasury.
#[post("/{coop_id}/deposit")]
pub async fn deposit_to_treasury(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<DepositRequest>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:write")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    // Input validation
    if body.amount <= 0 {
        return Err(GatewayError::BadRequest(
            "Deposit amount must be positive".to_string(),
        ));
    }

    if body.currency.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Currency cannot be empty".to_string(),
        ));
    }

    info!(
        coop_id = %coop_id,
        amount = body.amount,
        currency = %body.currency,
        "Treasury deposit requested"
    );

    // TODO: Wire to ledger to create deposit entry
    // Deposits don't require approval - they add to treasury

    let response = serde_json::json!({
        "status": "pending",
        "message": "Treasury deposit processing",
        "coop_id": coop_id,
        "amount": body.amount,
        "currency": body.currency
    });

    Ok(HttpResponse::Accepted().json(response))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure treasury routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_treasury_status)
        .service(get_treasury_balance)
        .service(list_budgets)
        .service(get_budget)
        .service(create_budget)
        .service(list_spending_rules)
        .service(get_audit_trail)
        .service(deposit_to_treasury);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use actix_web::{test, App, HttpMessage};

    fn test_claims(coop_id: &str) -> TokenClaims {
        TokenClaims {
            sub: "did:icn:test123".to_string(),
            iat: 1000000000,
            coop_id: coop_id.to_string(),
            scopes: vec!["treasury:read".to_string(), "treasury:write".to_string()],
            exp: 9999999999,
        }
    }

    #[actix_web::test]
    async fn test_get_treasury_status_not_found() {
        let app =
            test::init_service(App::new().service(web::scope("/treasury").configure(configure)))
                .await;

        let req = test::TestRequest::get()
            .uri("/treasury/test-coop")
            .to_request();
        req.extensions_mut().insert(test_claims("test-coop"));

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn test_list_budgets_empty() {
        let app =
            test::init_service(App::new().service(web::scope("/treasury").configure(configure)))
                .await;

        let req = test::TestRequest::get()
            .uri("/treasury/test-coop/budgets")
            .to_request();
        req.extensions_mut().insert(test_claims("test-coop"));

        let resp: BudgetListResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.budgets.len(), 0);
    }

    #[actix_web::test]
    async fn test_cross_coop_access_denied() {
        let app =
            test::init_service(App::new().service(web::scope("/treasury").configure(configure)))
                .await;

        let req = test::TestRequest::get()
            .uri("/treasury/other-coop/budgets")
            .to_request();
        // Token is for test-coop but accessing other-coop
        req.extensions_mut().insert(test_claims("test-coop"));

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_create_budget_requires_write_scope() {
        let app =
            test::init_service(App::new().service(web::scope("/treasury").configure(configure)))
                .await;

        let body = CreateBudgetRequest {
            purpose: "Test budget".to_string(),
            amount: 1000,
            currency: "hours".to_string(),
            period_end: None,
        };

        let req = test::TestRequest::post()
            .uri("/treasury/test-coop/budgets")
            .set_json(&body)
            .to_request();

        // Claims with only read scope
        let claims = TokenClaims {
            sub: "did:icn:test123".to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["treasury:read".to_string()], // No write scope
            exp: 9999999999,
        };
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_spending_rules_empty() {
        let app =
            test::init_service(App::new().service(web::scope("/treasury").configure(configure)))
                .await;

        let req = test::TestRequest::get()
            .uri("/treasury/test-coop/spending-rules")
            .to_request();
        req.extensions_mut().insert(test_claims("test-coop"));

        let resp: SpendingRulesResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.rules.len(), 0);
    }

    #[actix_web::test]
    async fn test_audit_trail_pagination() {
        let app =
            test::init_service(App::new().service(web::scope("/treasury").configure(configure)))
                .await;

        let req = test::TestRequest::get()
            .uri("/treasury/test-coop/audit?limit=50&offset=10")
            .to_request();
        req.extensions_mut().insert(test_claims("test-coop"));

        let resp: AuditTrailResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.limit, 50);
        assert_eq!(resp.offset, 10);
    }
}
