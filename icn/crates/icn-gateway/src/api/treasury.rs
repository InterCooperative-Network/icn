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

use actix_web::{get, post, web, HttpMessage, HttpRequest, HttpResponse};
use icn_entity::EntityId;
#[cfg_attr(not(test), allow(unused_imports))]
use icn_governance::{
    GovernanceDomainId, GovernanceParams, MembershipConfig, ProposalId, ProposalPayload,
    TreasuryProposalOperation,
};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use crate::auth::TokenClaims;
use crate::authority::{
    active_treasury_entity_auth_mode, evaluate_treasury_entity_access,
    observe_treasury_entity_access, treasury_gate_enforcement_denial, EntityAction,
    TreasuryEntityAuthMode,
};
use crate::coop_entity_resolver::{
    CoopEntityResolver, ObserveCoopEntityResolver, UnwiredCoopEntityResolver,
};
use crate::entity_mgr::EntityManager;
use crate::error::{GatewayError, Result};
use crate::governance_mgr::GovernanceManager;
use crate::middleware::{get_claims, require_coop_access, require_scope};
use crate::treasury_mgr::GatewayTreasuryManager;

// ============================================================================
// Configurable Limits
// ============================================================================

/// Minimum allowed value for limits (prevents invalid zero limits)
const MIN_LIMIT: usize = 1;

/// Default fallback values (used when env vars are not set)
const DEFAULT_AUDIT_LIMIT_FALLBACK: usize = 20;
const MAX_AUDIT_LIMIT_FALLBACK: usize = 100;

/// Default pagination limit for audit trail queries
/// Can be overridden via ICN_AUDIT_DEFAULT_LIMIT environment variable
///
/// Validation:
/// - Must be >= MIN_LIMIT (1)
/// - Must be <= max_audit_limit()
/// - Invalid values fall back to DEFAULT_AUDIT_LIMIT_FALLBACK
fn default_audit_limit() -> usize {
    let max = max_audit_limit_raw();
    let raw = std::env::var("ICN_AUDIT_DEFAULT_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_AUDIT_LIMIT_FALLBACK);

    // Clamp to valid range: [MIN_LIMIT, max]
    raw.max(MIN_LIMIT).min(max)
}

/// Maximum pagination limit for audit trail queries
/// Can be overridden via ICN_AUDIT_MAX_LIMIT environment variable
///
/// Validation:
/// - Must be >= MIN_LIMIT (1)
/// - Invalid values fall back to MAX_AUDIT_LIMIT_FALLBACK
fn max_audit_limit() -> usize {
    max_audit_limit_raw()
}

/// Internal: get raw max limit without clamping default
fn max_audit_limit_raw() -> usize {
    let raw = std::env::var("ICN_AUDIT_MAX_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MAX_AUDIT_LIMIT_FALLBACK);

    // Ensure at least MIN_LIMIT
    raw.max(MIN_LIMIT)
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Response for treasury status
#[derive(Debug, Serialize, Deserialize)]
pub struct TreasuryStatusResponse {
    /// Treasury DID
    pub treasury_did: String,
    /// Cooperative ID (derived from entity_id if present)
    pub coop_id: String,
    /// Entity ID (type-safe entity reference, preferred over coop_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Primary unit of account
    pub unit: String,
    /// Whether treasury is active
    pub is_active: bool,
    /// Current position (from ledger) - None if ledger lookup not yet implemented
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    /// Number of active budgets
    pub active_budget_count: usize,
    /// Number of spending rules
    pub spending_rule_count: usize,
}

/// Response for treasury position
#[derive(Debug, Serialize, Deserialize)]
pub struct TreasuryBalanceResponse {
    /// Treasury DID
    pub treasury_did: String,
    /// Position by unit of account
    pub positions: HashMap<String, i64>,
}

/// Response for treasury nonce
#[derive(Debug, Serialize, Deserialize)]
pub struct TreasuryNonceResponse {
    /// Cooperative ID
    pub coop_id: String,
    /// Treasury DID
    pub treasury_did: String,
    /// Current spend nonce for this treasury
    pub nonce: u64,
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
    /// Unit of account
    pub unit: String,
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
    /// Unit of account
    pub unit: String,
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
    /// Unit of account
    pub unit: String,
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
    /// Unit of account
    pub unit: String,
    /// Period end (optional, Unix timestamp)
    pub period_end: Option<u64>,
}

/// Request to deposit to treasury
#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    /// Amount to deposit
    pub amount: i64,
    /// Unit of account
    pub unit: String,
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
/// Observe-mode entity-aware authorization (RFC-0018, ADR-0035).
///
/// Computes and records the entity-aware authorization decision ALONGSIDE the
/// authoritative flat `require_coop_access` guard, which has already run by the
/// time this is called. This is **observation-only**: it emits a metric and a log
/// line and returns nothing, so it can never deny a request. Call it AFTER
/// `require_coop_access` and (where available) after the treasury is loaded,
/// passing `treasury.entity_id()` as the authoritative target.
async fn observe_treasury(
    req: &HttpRequest,
    entity_mgr: &web::Data<Arc<EntityManager>>,
    treasury_entity_id: Option<&EntityId>,
    coop_id: &str,
    action: EntityAction,
) -> Result<()> {
    // Resolve the caller synchronously (cheap); bail quietly (proceed) if the request is
    // unauthenticated or the subject DID is unparseable — the flat `require_coop_access`
    // guard above remains authoritative for those cases.
    let Some(claims) = get_claims(req) else {
        return Ok(());
    };
    let Ok(caller_did) = claims.sub.parse::<Did>() else {
        return Ok(());
    };
    let caller = EntityId::from_did(&caller_did);

    let entity_mgr = entity_mgr.get_ref().clone();
    let resolver = observe_coop_entity_resolver(req);
    let target = treasury_entity_id.cloned();
    let coop_id = coop_id.to_string();

    match active_treasury_entity_auth_mode() {
        // A2e ENFORCE (operator-gated, off by default): consume the gate decision INLINE on
        // the request path. A `WouldDeny` denies with the existing forbidden pattern (403)
        // BEFORE any treasury mutation; `ProceedUnchanged` proceeds exactly as the flat
        // guard decided. The same metrics/log/evidence chain is recorded as in observe mode
        // (`evaluate_treasury_entity_access`). Awaiting inline is the deliberate cost of
        // enforcement: an unverified entity gate must not let a slow/erroring EntityManager
        // pass — a stall/error surfaces as `WouldDeny(ObservationError)` and fails closed.
        TreasuryEntityAuthMode::EnforceTrustedResolver => {
            let outcome = evaluate_treasury_entity_access(
                resolver.as_ref(),
                &entity_mgr,
                &caller,
                target.as_ref(),
                &coop_id,
                action,
            )
            .await;
            match treasury_gate_enforcement_denial(outcome.decision) {
                Some(err) => Err(err),
                None => Ok(()),
            }
        }
        // OBSERVE (default): run as a detached, best-effort task and discard the result.
        // The entity-registry lookup may be a daemon-actor round trip; awaiting it inline
        // would let a slow or stalled EntityManager block an already-authorized treasury
        // response. Spawning keeps observe mode strictly off the request path — it can
        // affect neither the response nor its latency, and never denies. Route allow/deny
        // is byte-identical to the pre-A2e behavior. (RFC-0018 observe mode, ADR-0035.)
        TreasuryEntityAuthMode::ObserveOnly => {
            actix_web::rt::spawn(async move {
                let _ = observe_treasury_entity_access(
                    resolver.as_ref(),
                    &entity_mgr,
                    &caller,
                    target.as_ref(),
                    &coop_id,
                    action,
                )
                .await;
            });
            Ok(())
        }
    }
}

/// Fetch the observe-mode `coop_id → EntityId` resolver from gateway app state (A2c).
///
/// The gateway registers exactly one [`ObserveCoopEntityResolver`] as shared
/// `web::Data` — a `StoreBackedCoopEntityResolver` when a trusted, provenance-aware
/// `CoopEntityMap` handle is wired, otherwise the fail-closed
/// [`UnwiredCoopEntityResolver`]. When none is registered (e.g. a standalone or test
/// app), this falls back to the fail-closed default, so the observe path never trusts
/// a resolution it cannot verify. The resolver is consulted observe-only and changes
/// no authorization outcome.
fn observe_coop_entity_resolver(req: &HttpRequest) -> Arc<dyn CoopEntityResolver> {
    req.app_data::<web::Data<ObserveCoopEntityResolver>>()
        .map(|d| d.get_ref().0.clone())
        .unwrap_or_else(|| Arc::new(UnwiredCoopEntityResolver))
}

/// Like [`observe_treasury`], but keyed by the **owning treasury's `treasury_did`**
/// rather than a path `coop_id`. Used by `get_budget`, which fetches a budget by a
/// globally-keyed `budget_id`: the budget may belong to a different cooperative than
/// the path `coop_id` (the flat guard only checks the path coop). Observing against
/// the path coop would record the entity decision for the wrong entity. Resolving
/// from the budget's own `treasury_did` makes the observation reflect the budget's
/// real owner — so a cross-coop budget read surfaces as a genuine `flat_allow_entity_deny`
/// instead of a misleading `agree_allow`. The treasury is loaded **inside** the
/// detached task (off the request hot path). (RFC-0018 observe mode, ADR-0035.)
async fn observe_treasury_by_did(
    req: &HttpRequest,
    entity_mgr: &web::Data<Arc<EntityManager>>,
    treasury_mgr: &web::Data<Arc<GatewayTreasuryManager>>,
    treasury_did: &Did,
    action: EntityAction,
) -> Result<()> {
    let Some(claims) = get_claims(req) else {
        return Ok(());
    };
    let Ok(caller_did) = claims.sub.parse::<Did>() else {
        return Ok(());
    };
    let caller = EntityId::from_did(&caller_did);

    let entity_mgr = entity_mgr.get_ref().clone();
    let treasury_mgr = treasury_mgr.get_ref().clone();
    let treasury_did = treasury_did.clone();
    let resolver = observe_coop_entity_resolver(req);

    match active_treasury_entity_auth_mode() {
        // A2e ENFORCE (operator-gated, off by default): resolve the budget's owning treasury
        // and consume the gate decision INLINE, before returning the budget. A `WouldDeny`
        // (including the fail-closed case where the owning treasury cannot be resolved, so no
        // trusted target exists) denies with the existing forbidden pattern (403);
        // `ProceedUnchanged` proceeds. This enforces against the budget's REAL owning entity,
        // not the path coop.
        TreasuryEntityAuthMode::EnforceTrustedResolver => {
            let (target, coop_id) = match treasury_mgr.get_treasury(&treasury_did).await {
                Ok(Some(t)) => (t.entity_id().cloned(), t.coop_id.clone()),
                _ => (None, String::new()),
            };
            let outcome = evaluate_treasury_entity_access(
                resolver.as_ref(),
                &entity_mgr,
                &caller,
                target.as_ref(),
                &coop_id,
                action,
            )
            .await;
            match treasury_gate_enforcement_denial(outcome.decision) {
                Some(err) => Err(err),
                None => Ok(()),
            }
        }
        // OBSERVE (default): resolve the owning treasury and observe off the hot path; the
        // result is discarded and never denies (byte-identical to the pre-A2e behavior).
        TreasuryEntityAuthMode::ObserveOnly => {
            actix_web::rt::spawn(async move {
                // Resolve the budget's owning treasury off the hot path; observe against
                // its stored entity_id (and its own coop_id for the fallback projection).
                let (target, coop_id) = match treasury_mgr.get_treasury(&treasury_did).await {
                    Ok(Some(t)) => (t.entity_id().cloned(), t.coop_id.clone()),
                    _ => (None, String::new()),
                };
                let _ = observe_treasury_entity_access(
                    resolver.as_ref(),
                    &entity_mgr,
                    &caller,
                    target.as_ref(),
                    &coop_id,
                    action,
                )
                .await;
            });
            Ok(())
        }
    }
}

#[get("/{coop_id}")]
pub async fn get_treasury_status(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
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

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryRead,
    )
    .await?;

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

    // Query actual balance from ledger (returns None if ledger not wired)
    let balance = treasury_mgr
        .get_treasury_balance(&treasury.treasury_did, &treasury.currency)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let response = TreasuryStatusResponse {
        treasury_did: treasury.treasury_did.to_string(),
        coop_id: treasury.coop_id.clone(),
        entity_id: treasury.entity_id().map(|e| e.to_string()),
        unit: treasury.currency,
        is_active: treasury.is_active,
        position: balance,
        active_budget_count,
        spending_rule_count: rules.len(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /treasury/{coop_id}/position - Get treasury position
///
/// Returns the current position of the cooperative's treasury.
#[get("/{coop_id}/position")]
pub async fn get_treasury_position(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
) -> Result<HttpResponse> {
    do_get_treasury_position(req, path, treasury_mgr, entity_mgr).await
}

pub(crate) async fn do_get_treasury_position(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    info!(coop_id = %coop_id, "Treasury position requested");

    // Get treasury for this cooperative
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(treasury) = treasury else {
        return Err(GatewayError::NotFound(format!(
            "Treasury not configured for cooperative '{coop_id}'"
        )));
    };

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryRead,
    )
    .await?;

    // Check if ledger is wired for position queries
    if !treasury_mgr.is_ledger_wired() {
        return Err(GatewayError::ServiceUnavailable(
            "Treasury position lookup requires daemon integration. \
             Start icnd with full identity to enable position queries."
                .to_string(),
        ));
    }

    // Query all positions from ledger
    let positions = treasury_mgr
        .get_all_treasury_balances(&treasury.treasury_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let response = TreasuryBalanceResponse {
        treasury_did: treasury.treasury_did.to_string(),
        positions,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /treasury/{coop_id}/nonce - Get treasury spend nonce
///
/// Returns the current nonce used for treasury spend ordering.
#[get("/{coop_id}/nonce")]
pub async fn get_treasury_nonce(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let Some(treasury) = treasury else {
        return Err(GatewayError::NotFound(format!(
            "Treasury not configured for cooperative '{coop_id}'"
        )));
    };

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryRead,
    )
    .await?;

    let treasury_did = treasury.treasury_did.to_string();
    let nonce = treasury_mgr
        .get_treasury_nonce(&treasury_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::ServiceUnavailable(
                "Treasury nonce lookup requires daemon integration with ledger service."
                    .to_string(),
            )
        })?;

    Ok(HttpResponse::Ok().json(TreasuryNonceResponse {
        coop_id,
        treasury_did,
        nonce,
    }))
}

/// GET /treasury/{coop_id}/budgets - List budgets
///
/// Returns all budgets for the cooperative's treasury.
#[get("/{coop_id}/budgets")]
pub async fn list_budgets(
    req: HttpRequest,
    path: web::Path<String>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
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

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryRead,
    )
    .await?;

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
            unit: b.currency.clone(),
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
    entity_mgr: web::Data<Arc<EntityManager>>,
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
        // Generic message (no id): a genuinely missing budget and a foreign budget
        // (the ownership check below) must be indistinguishable, so this endpoint
        // cannot be used as a cross-coop budget-id enumeration oracle.
        return Err(GatewayError::NotFound("Budget not found".to_string()));
    };

    // RFC-0018 observe mode (ADR-0035): observe against the budget's OWN treasury
    // (resolved from budget.treasury_did inside a detached task), not the path coop
    // — the budget is globally keyed and may belong to a different cooperative.
    // Observation-only; the flat require_coop_access guard above stays authoritative.
    // This fires BEFORE the ownership enforcement below so a cross-coop attempt
    // still registers (as flat_allow_entity_deny) even though it is then denied.
    observe_treasury_by_did(
        &req,
        &entity_mgr,
        &treasury_mgr,
        &budget.treasury_did,
        EntityAction::TreasuryRead,
    )
    .await?;

    // #2085: object-context binding in the flat-authz regime. The budget was
    // fetched by a globally-keyed id; require it to belong to the treasury of the
    // path coop before returning it. A foreign (or path-coop-has-no-treasury)
    // budget is reported as NotFound — the same shape as a missing budget — so the
    // caller cannot distinguish "exists elsewhere" from "does not exist." The flat
    // require_coop_access guard above remains authoritative; this only ties the
    // loaded object to the request path (no entity-auth enforcement; see #2081).
    let path_treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;
    let owns_budget = path_treasury
        .as_ref()
        .is_some_and(|t| t.treasury_did == budget.treasury_did);
    if !owns_budget {
        return Err(GatewayError::NotFound("Budget not found".to_string()));
    }

    let response = BudgetDetailResponse {
        id: budget.id.clone(),
        treasury_did: budget.treasury_did.to_string(),
        purpose: budget.purpose.clone(),
        allocated_amount: budget.allocated_amount,
        spent_amount: budget.spent_amount,
        remaining: budget.remaining(),
        unit: budget.currency.clone(),
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
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
    governance_mgr: web::Data<Arc<GovernanceManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:write")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    // Get proposer DID from JWT claims
    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let proposer_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

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

    if body.unit.trim().is_empty() {
        return Err(GatewayError::BadRequest("Unit cannot be empty".to_string()));
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

    // Get treasury for this cooperative to get treasury_did
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let treasury = treasury.ok_or_else(|| {
        GatewayError::NotFound(format!(
            "Treasury not configured for cooperative '{coop_id}'. \
             Register a treasury before creating budgets."
        ))
    })?;

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryWrite,
    )
    .await?;

    info!(
        coop_id = %coop_id,
        purpose = %body.purpose,
        amount = body.amount,
        treasury_did = %treasury.treasury_did,
        proposer = %proposer_did,
        "Creating governance proposal for treasury budget"
    );

    // Create governance proposal for budget creation
    let proposal_id = ProposalId::generate();
    let domain_id = GovernanceDomainId::new(&coop_id);

    let payload = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::CreateBudget {
            treasury_did: treasury.treasury_did.clone(),
            purpose: body.purpose.clone(),
            amount: body.amount,
            currency: body.unit.clone(),
            period_end: body.period_end,
        },
    };

    let title = format!("Budget Allocation: {}", body.purpose);
    let description = format!(
        "Proposal to allocate {} {} from treasury for: {}",
        body.amount, body.unit, body.purpose
    );

    // Submit proposal to governance system
    let created_proposal_id = governance_mgr
        .create_proposal(
            proposal_id.clone(),
            domain_id,
            proposer_did,
            title,
            description,
            payload,
            icn_governance::ProposalScope::Local,
        )
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to create proposal: {e}")))?;

    info!(
        proposal_id = %created_proposal_id,
        coop_id = %coop_id,
        "Treasury budget proposal created successfully"
    );

    let response = serde_json::json!({
        "status": "proposal_created",
        "message": "Budget creation requires governance approval",
        "proposal_id": created_proposal_id.to_string(),
        "coop_id": coop_id,
        "purpose": body.purpose,
        "amount": body.amount,
        "unit": body.unit
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
    entity_mgr: web::Data<Arc<EntityManager>>,
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

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryRead,
    )
    .await?;

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
            unit: r.currency.clone(),
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
    entity_mgr: web::Data<Arc<EntityManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:read")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    let limit: usize = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(default_audit_limit)
        .min(max_audit_limit());

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

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryRead,
    )
    .await?;

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
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
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

    if body.unit.trim().is_empty() {
        return Err(GatewayError::BadRequest("Unit cannot be empty".to_string()));
    }

    // Get depositor DID from auth token
    let claims = req
        .extensions()
        .get::<TokenClaims>()
        .cloned()
        .ok_or_else(|| {
            GatewayError::AuthenticationFailed("Missing authentication claims".to_string())
        })?;

    let depositor_did: Did = claims.sub.parse().map_err(|e| {
        GatewayError::InternalError(format!("Invalid depositor DID in claims: {e}"))
    })?;

    // Get the treasury for this cooperative
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::NotFound(format!("Treasury not found for cooperative: {coop_id}"))
        })?;

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryWrite,
    )
    .await?;

    info!(
        coop_id = %coop_id,
        depositor = %depositor_did,
        treasury = %treasury.treasury_did,
        amount = body.amount,
        unit = %body.unit,
        "Treasury deposit requested"
    );

    // Check if ledger is wired for deposits
    if !treasury_mgr.is_ledger_wired() {
        return Err(GatewayError::ServiceUnavailable(
            "Treasury deposits require daemon integration. \
             Start icnd with full identity to enable deposits."
                .to_string(),
        ));
    }

    // Create the deposit entry
    let entry_hash = treasury_mgr
        .create_deposit(
            &treasury.treasury_did,
            &depositor_did,
            body.amount,
            body.unit.clone(),
            body.memo.clone(),
        )
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    info!(
        coop_id = %coop_id,
        entry_hash = %entry_hash,
        "Treasury deposit completed"
    );

    let response = serde_json::json!({
        "status": "completed",
        "message": "Treasury deposit successful",
        "coop_id": coop_id,
        "amount": body.amount,
        "unit": body.unit,
        "entry_hash": entry_hash.to_string()
    });

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Treasury Spend
// ============================================================================

/// Request to propose a treasury spend
#[derive(Debug, Deserialize)]
pub struct SpendRequest {
    /// Amount to spend (must be positive)
    pub amount: i64,
    /// Recipient DID
    pub recipient: String,
    /// Human-readable memo / purpose
    pub memo: String,
    /// Unit of account (defaults to "credits")
    #[serde(default = "default_spend_unit")]
    pub unit: String,
    /// Optional expected treasury nonce. If omitted, gateway resolves current nonce.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_nonce: Option<u64>,
}

fn default_spend_unit() -> String {
    "credits".to_string()
}

/// POST /treasury/{coop_id}/spend - Propose a treasury spend
///
/// Creates a governance proposal for a direct treasury disbursement.
/// The spend is charged against the treasury's unallocated balance.
/// Requires governance approval before execution.
#[post("/{coop_id}/spend")]
pub async fn propose_spend(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SpendRequest>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
    governance_mgr: web::Data<Arc<GovernanceManager>>,
) -> Result<HttpResponse> {
    do_propose_spend(req, path, body, treasury_mgr, entity_mgr, governance_mgr).await
}

pub(crate) async fn do_propose_spend(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SpendRequest>,
    treasury_mgr: web::Data<Arc<GatewayTreasuryManager>>,
    entity_mgr: web::Data<Arc<EntityManager>>,
    governance_mgr: web::Data<Arc<GovernanceManager>>,
) -> Result<HttpResponse> {
    require_scope(&req, "treasury:write")?;

    let coop_id = path.into_inner();
    require_coop_access(&req, &coop_id)?;

    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let proposer_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Validate inputs
    if body.amount <= 0 {
        return Err(GatewayError::BadRequest(
            "Spend amount must be positive".to_string(),
        ));
    }
    if body.memo.trim().is_empty() {
        return Err(GatewayError::BadRequest("Memo cannot be empty".to_string()));
    }
    let recipient_did: Did = body
        .recipient
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid recipient DID: {e}")))?;

    // Ensure treasury exists
    let treasury = treasury_mgr
        .get_treasury_by_coop(&coop_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;
    let treasury = treasury.ok_or_else(|| {
        GatewayError::NotFound(format!(
            "Treasury not configured for cooperative '{coop_id}'"
        ))
    })?;

    // RFC-0018 observe mode (ADR-0035): flat guard above remains authoritative.
    observe_treasury(
        &req,
        &entity_mgr,
        treasury.entity_id(),
        &coop_id,
        EntityAction::TreasuryWrite,
    )
    .await?;

    let nonce = match body.expected_nonce {
        Some(n) => n,
        None => treasury_mgr
            .get_treasury_nonce(&treasury.treasury_did.to_string())
            .await
            .map_err(|e| GatewayError::InternalError(e.to_string()))?
            .ok_or_else(|| {
                GatewayError::ServiceUnavailable(
                    "Treasury nonce lookup requires daemon integration with ledger service."
                        .to_string(),
                )
            })?,
    };

    info!(
        coop_id = %coop_id,
        amount = body.amount,
        recipient = %body.recipient,
        nonce = nonce,
        "Creating governance proposal for treasury spend"
    );

    let proposal_id = ProposalId::generate();
    let domain_id = GovernanceDomainId::new(&coop_id);

    let payload = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Spend {
            treasury_did: treasury.treasury_did.clone(),
            amount: body.amount,
            currency: body.unit.clone(),
            recipient: recipient_did,
            memo: body.memo.clone(),
            nonce,
        },
    };

    let title = format!("Treasury Spend: {} {}", body.amount, body.unit);
    let description = format!(
        "Proposal to spend {} {} from treasury to {} — {}",
        body.amount, body.unit, body.recipient, body.memo
    );

    let created_proposal_id = governance_mgr
        .create_proposal(
            proposal_id.clone(),
            domain_id,
            proposer_did,
            title,
            description,
            payload,
            icn_governance::ProposalScope::Local,
        )
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to create proposal: {e}")))?;

    info!(
        proposal_id = %created_proposal_id,
        coop_id = %coop_id,
        "Treasury spend proposal created"
    );

    let response = serde_json::json!({
        "status": "proposal_created",
        "message": "Treasury spend requires governance approval",
        "operation": "spend",
        "proposal_id": created_proposal_id.to_string(),
        "coop_id": coop_id,
        "amount": body.amount,
        "unit": body.unit,
        "recipient": body.recipient,
        "memo": body.memo,
        "nonce": nonce
    });

    Ok(HttpResponse::Accepted().json(response))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure treasury routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_treasury_status)
        .service(get_treasury_position)
        .service(get_treasury_nonce)
        .service(list_budgets)
        .service(get_budget)
        .service(create_budget)
        .service(list_spending_rules)
        .service(get_audit_trail)
        .service(deposit_to_treasury)
        .service(propose_spend);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use actix_web::{test, App, HttpMessage};
    use icn_identity::KeyPair;
    use icn_ledger::TreasuryManager as LedgerTreasuryManager;

    fn test_claims(coop_id: &str) -> TokenClaims {
        TokenClaims {
            entity_id: None,
            entity_type: None,
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

    fn create_test_governance_manager() -> Arc<GovernanceManager> {
        Arc::new(GovernanceManager::new())
    }

    /// Actor-backed gateway treasury manager. Standalone mode cannot register
    /// treasuries or create budgets (both bail), so seeding real, coop-owned
    /// budgets requires the ledger-backed handle.
    fn actor_backed_treasury_manager() -> Arc<GatewayTreasuryManager> {
        let handle = Arc::new(tokio::sync::RwLock::new(LedgerTreasuryManager::new()));
        Arc::new(GatewayTreasuryManager::with_handle(handle))
    }

    /// Register a treasury for `coop_id` and create one budget in it.
    /// Returns the new budget's globally-keyed id.
    async fn seed_treasury_with_budget(mgr: &GatewayTreasuryManager, coop_id: &str) -> String {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let creator = KeyPair::generate().unwrap().did().clone();
        mgr.register_treasury(
            treasury_did.clone(),
            coop_id.to_string(),
            "USD".to_string(),
            creator.clone(),
            None,
        )
        .await
        .unwrap();
        mgr.create_budget(
            treasury_did,
            "operations".to_string(),
            1000,
            "USD".to_string(),
            None,
            creator,
            None,
        )
        .await
        .unwrap()
        .id
    }

    #[actix_web::test]
    async fn test_get_treasury_status_not_found() {
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
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
    async fn test_get_treasury_nonce_not_found() {
        let treasury_mgr = create_test_treasury_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/treasury/test-coop/nonce")
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
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
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
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
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

    /// #2085: a caller authorized for coop-a must not retrieve coop-b's budget
    /// through the globally-keyed budget id. The flat guard checks only the path
    /// coop; the budget is fetched by a global id and must be proven to belong to
    /// the path coop's treasury before it is returned.
    #[actix_web::test]
    async fn test_get_budget_cross_coop_read_denied() {
        let treasury_mgr = actor_backed_treasury_manager();
        // Caller's own coop, plus a victim coop whose budget id the caller knows/guesses.
        seed_treasury_with_budget(&treasury_mgr, "coop-a").await;
        let victim_budget_id = seed_treasury_with_budget(&treasury_mgr, "coop-b").await;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        // Valid coop-a token, reading coop-b's budget under the coop-a path.
        let req = test::TestRequest::get()
            .uri(&format!("/treasury/coop-a/budgets/{victim_budget_id}"))
            .to_request();
        req.extensions_mut().insert(test_claims("coop-a"));

        let resp = test::call_service(&app, req).await;
        let status = resp.status();

        // Body must carry no foreign budget detail (id or contents) — status alone is not enough.
        let body = test::read_body(resp).await;
        let body = String::from_utf8_lossy(&body);
        assert_eq!(
            status,
            actix_web::http::StatusCode::NOT_FOUND,
            "cross-coop budget read must be denied (body: {body})"
        );
        assert!(
            !body.contains(&victim_budget_id),
            "404 body leaked the foreign budget id: {body}"
        );
        assert!(
            !body.contains("operations"),
            "404 body leaked the foreign budget purpose: {body}"
        );
    }

    /// Same-coop read still works after the ownership check: the caller's own
    /// budget is returned unchanged.
    #[actix_web::test]
    async fn test_get_budget_same_coop_read_ok() {
        let treasury_mgr = actor_backed_treasury_manager();
        let own_budget_id = seed_treasury_with_budget(&treasury_mgr, "coop-a").await;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/treasury/coop-a/budgets/{own_budget_id}"))
            .to_request();
        req.extensions_mut().insert(test_claims("coop-a"));

        let resp = test::call_service(&app, req).await;
        let status = resp.status();
        let body = test::read_body(resp).await;
        let body = String::from_utf8_lossy(&body);
        assert_eq!(
            status,
            actix_web::http::StatusCode::OK,
            "same-coop budget read must succeed (body: {body})"
        );
        assert!(
            body.contains(&own_budget_id),
            "200 body should include the budget id"
        );
    }

    /// #2085 edge case: when the path coop has no treasury at all, a foreign
    /// budget id must not resolve — no ownership context, so NotFound with no leak.
    #[actix_web::test]
    async fn test_get_budget_path_coop_without_treasury_does_not_reveal_foreign_budget() {
        let treasury_mgr = actor_backed_treasury_manager();
        // Only coop-b has a treasury/budget; coop-a has none.
        let foreign_budget_id = seed_treasury_with_budget(&treasury_mgr, "coop-b").await;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/treasury/coop-a/budgets/{foreign_budget_id}"))
            .to_request();
        req.extensions_mut().insert(test_claims("coop-a"));

        let resp = test::call_service(&app, req).await;
        let status = resp.status();
        let body = test::read_body(resp).await;
        let body = String::from_utf8_lossy(&body);
        assert_eq!(
            status,
            actix_web::http::StatusCode::NOT_FOUND,
            "path coop without a treasury must not resolve a foreign budget (body: {body})"
        );
        assert!(
            !body.contains(&foreign_budget_id),
            "404 body leaked the foreign budget id: {body}"
        );
        assert!(
            !body.contains("operations"),
            "404 body leaked the foreign budget purpose: {body}"
        );
    }

    #[actix_web::test]
    async fn test_create_budget_requires_write_scope() {
        let treasury_mgr = create_test_treasury_manager();
        let governance_mgr = create_test_governance_manager();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr))
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
                .app_data(web::Data::new(governance_mgr))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        let body = CreateBudgetRequest {
            purpose: "Test budget".to_string(),
            amount: 1000,
            unit: "hours".to_string(),
            period_end: None,
        };

        let req = test::TestRequest::post()
            .uri("/treasury/test-coop/budgets")
            .set_json(&body)
            .to_request();

        // Claims with only read scope
        let claims = TokenClaims {
            entity_id: None,
            entity_type: None,
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
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
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
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
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

    #[actix_web::test]
    async fn test_propose_spend_creates_spend_operation_payload() {
        let proposer = KeyPair::generate().unwrap();
        let recipient = KeyPair::generate().unwrap();
        let treasury_did = KeyPair::generate().unwrap().did().clone();

        let treasury_handle = Arc::new(tokio::sync::RwLock::new(LedgerTreasuryManager::new()));
        let treasury_mgr = Arc::new(GatewayTreasuryManager::with_handle(treasury_handle));
        treasury_mgr
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "credits".to_string(),
                proposer.did().clone(),
                Some("test treasury".to_string()),
            )
            .await
            .unwrap();

        let governance_mgr = create_test_governance_manager();
        governance_mgr
            .create_domain(
                GovernanceDomainId("test-coop".to_string()),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![proposer.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(treasury_mgr.clone()))
                .app_data(web::Data::new(Arc::new(EntityManager::new())))
                .app_data(web::Data::new(governance_mgr.clone()))
                .service(web::scope("/treasury").configure(configure)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/treasury/test-coop/spend")
            .set_json(serde_json::json!({
                "amount": 42,
                "recipient": recipient.did().to_string(),
                "memo": "Ops budget",
                "unit": "credits",
                "expected_nonce": 0
            }))
            .to_request();
        req.extensions_mut().insert(TokenClaims {
            entity_id: None,
            entity_type: None,
            sub: proposer.did().to_string(),
            iat: 1_000_000_000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["treasury:write".to_string()],
            exp: 9_999_999_999,
        });

        let _resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;

        let proposals = governance_mgr.list_proposals().await.unwrap();
        assert_eq!(proposals.len(), 1);
        match &proposals[0].payload {
            ProposalPayload::Treasury { operation } => match operation {
                TreasuryProposalOperation::Spend {
                    treasury_did: op_treasury_did,
                    recipient: op_recipient,
                    amount,
                    currency,
                    memo,
                    nonce,
                } => {
                    assert_eq!(op_treasury_did, &treasury_did);
                    assert_eq!(op_recipient, recipient.did());
                    assert_eq!(*amount, 42);
                    assert_eq!(currency, "credits");
                    assert_eq!(memo, "Ops budget");
                    assert_eq!(*nonce, 0);
                }
                other => panic!("expected Spend payload, got {other:?}"),
            },
            other => panic!("expected Treasury payload, got {other:?}"),
        }
    }
}
