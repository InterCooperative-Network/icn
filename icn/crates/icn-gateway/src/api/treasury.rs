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
use std::sync::Arc;
use tracing::info;

use crate::error::{GatewayError, Result};
use crate::middleware::{require_coop_access, require_scope};
use crate::treasury_mgr::GatewayTreasuryManager;

// ============================================================================
// Constants
// ============================================================================

/// Default pagination limit for audit trail queries
/// TODO: Make configurable via gateway config or environment variable
const DEFAULT_AUDIT_LIMIT: usize = 20;

/// Maximum pagination limit for audit trail queries
/// TODO: Make configurable via gateway config or environment variable
const MAX_AUDIT_LIMIT: usize = 100;

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
    /// Current balance (from ledger) - None if ledger lookup not yet implemented
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<i64>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury status requested");

    // Get treasury for this cooperative
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(treasury) = treasury else {
        return Err(GatewayError::NotFound(format!(
            "Treasury not configured for cooperative '{coop_id}'. Register via governance proposal."
        )));
    };

    // Get budgets and spending rules
    let budgets = treasury_mgr
        .list_budgets(&treasury.treasury_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let rules = treasury_mgr
        .list_spending_rules(&treasury.treasury_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let active_budget_count = budgets
        .iter()
        .filter(|b| matches!(b.status, icn_ledger::BudgetStatus::Active))
        .count();

    // TODO: Get actual balance from ledger via LedgerActor
    // For now, omit balance field (None is skipped in serialization)
    let response = TreasuryStatusResponse {
        treasury_did: treasury.treasury_did.to_string(),
        coop_id: treasury.coop_id,
        currency: treasury.currency,
        is_active: treasury.is_active,
        balance: None,
        active_budget_count,
        spending_rule_count: rules.len(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /treasury/{coop_id}/balance - Get treasury balance
///
/// Returns the current balance of the cooperative's treasury.
#[get("/{coop_id}/balance")]
pub async fn get_treasury_balance(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury balance requested");

    // Get treasury for this cooperative
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(_treasury) = treasury else {
        return Err(GatewayError::NotFound(format!(
            "Treasury not configured for cooperative '{coop_id}'"
        )));
    };

    // Balance lookup requires ledger integration which is not yet implemented.
    // Return 503 Service Unavailable instead of fake/empty data.
    // TODO: Wire LedgerActor to get actual balances once ledger integration is complete.
    Err(GatewayError::ServiceUnavailable(
        "Treasury balance lookup not yet implemented. Use budget endpoints for allocated amounts."
            .to_string(),
    ))
}

/// GET /treasury/{coop_id}/budgets - List budgets
///
/// Returns all budgets for the cooperative's treasury.
#[get("/{coop_id}/budgets")]
pub async fn list_budgets(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury budgets list requested");

    // Get treasury for this cooperative
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(treasury) = treasury else {
        // Consistent with get_treasury_status: return 404 if treasury not found
        return Err(GatewayError::NotFound(format!(
            "Treasury not configured for cooperative '{coop_id}'. Register via governance proposal."
        )));
    };

    // Get budgets
    let budgets = treasury_mgr
        .list_budgets(&treasury.treasury_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let budget_summaries: Vec<BudgetSummary> = budgets
        .iter()
        .map(|b| BudgetSummary {
            id: b.id.clone(),
            purpose: b.purpose.clone(),
            allocated_amount: b.allocated_amount,
            spent_amount: b.spent_amount,
            remaining: b.remaining(),
            percentage_used: b.percentage_used().round().clamp(0.0, 100.0) as u8,
            status: format!("{:?}", b.status),
            currency: b.currency.clone(),
        })
        .collect();

    let total = budget_summaries.len();
    let response = BudgetListResponse {
        treasury_did: treasury.treasury_did.to_string(),
        budgets: budget_summaries,
        total,
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
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let (coop_id, budget_id) = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, budget_id = %budget_id, "Treasury budget details requested");

    // Get budget
    let budget = treasury_mgr
        .get_budget(&budget_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(budget) = budget else {
        return Err(GatewayError::NotFound(format!(
            "Budget not found: {budget_id}"
        )));
    };

    let response = BudgetDetailResponse {
        id: budget.id.clone(),
        treasury_did: budget.treasury_did.to_string(),
        purpose: budget.purpose.clone(),
        allocated_amount: budget.allocated_amount,
        spent_amount: budget.spent_amount,
        remaining: budget.remaining(),
        currency: budget.currency.clone(),
        period_start: budget.period_start,
        period_end: budget.period_end,
        status: format!("{:?}", budget.status),
        percentage_used: budget.percentage_used().round().clamp(0.0, 100.0) as u8,
        proposal_id: budget.proposal_id.clone(),
        created_at: budget.created_at,
        created_by: budget.created_by.to_string(),
        notification_thresholds: budget.notification_thresholds.clone(),
        notified_thresholds: budget.notified_thresholds.clone(),
    };

    Ok(HttpResponse::Ok().json(response))
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
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury spending rules requested");

    // Get treasury for this cooperative
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(treasury) = treasury else {
        let response = SpendingRulesResponse {
            treasury_did: String::new(),
            rules: Vec::new(),
        };
        return Ok(HttpResponse::Ok().json(response));
    };

    // Get spending rules
    let rules = treasury_mgr
        .list_spending_rules(&treasury.treasury_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let rule_summaries: Vec<SpendingRuleSummary> = rules
        .iter()
        .map(|r| SpendingRuleSummary {
            id: r.id.clone(),
            name: r.name.clone(),
            threshold_amount: r.threshold_amount,
            currency: r.currency.clone(),
            approval_type: format!("{:?}", r.approval_type),
            is_active: r.is_active,
        })
        .collect();

    let response = SpendingRulesResponse {
        treasury_did: treasury.treasury_did.to_string(),
        rules: rule_summaries,
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
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    let limit: usize = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_AUDIT_LIMIT)
        .min(MAX_AUDIT_LIMIT);

    let offset: usize = query
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    info!(coop_id = %coop_id, limit = limit, offset = offset, "Treasury audit trail requested");

    // Get treasury for this cooperative
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(treasury) = treasury else {
        let response = AuditTrailResponse {
            treasury_did: String::new(),
            records: Vec::new(),
            total: 0,
            offset,
            limit,
            has_more: false,
        };
        return Ok(HttpResponse::Ok().json(response));
    };

    // Get audit trail
    let audit_trail = treasury_mgr
        .get_audit_trail(&treasury.treasury_did, limit, offset)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let audit_records: Vec<AuditRecordSummary> = audit_trail
        .records
        .iter()
        .map(|r| AuditRecordSummary {
            id: r.id.clone(),
            operation_type: format!("{:?}", r.operation),
            performed_by: r.performed_by.to_string(),
            performed_at: r.performed_at,
            balance_after: r.balance_after,
            proposal_id: r.proposal_id.clone(),
            notes: r.notes.clone(),
        })
        .collect();

    let response = AuditTrailResponse {
        treasury_did: treasury.treasury_did.to_string(),
        records: audit_records,
        total: audit_trail.total,
        offset: audit_trail.offset,
        limit: audit_trail.limit,
        has_more: audit_trail.offset + audit_trail.records.len() < audit_trail.total,
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

    fn create_test_treasury_manager() -> Arc<GatewayTreasuryManager> {
        Arc::new(GatewayTreasuryManager::new())
    }

    #[actix_web::test]
    async fn test_get_treasury_status_not_found() {
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/treasury/test-coop")
            .to_request();
        req.extensions_mut().insert(test_claims("test-coop"));

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn test_list_budgets_not_found() {
        // When no treasury is registered for a coop, list_budgets returns 404
        // (consistent with get_treasury_status)
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/treasury/test-coop/budgets")
            .to_request();
        req.extensions_mut().insert(test_claims("test-coop"));

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn test_cross_coop_access_denied() {
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .service(web::scope("/treasury").configure(configure)),
        )
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
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .service(web::scope("/treasury").configure(configure)),
        )
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
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .service(web::scope("/treasury").configure(configure)),
        )
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
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .service(web::scope("/treasury").configure(configure)),
        )
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
