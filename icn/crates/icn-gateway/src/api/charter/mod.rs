//! Charter API endpoints
//!
//! Provides REST API for Layer 2 of Commons Evolution:
//! - Charter creation and management
//! - Founder signatures
//! - Charter ratification
//! - Status management

use actix_web::{get, post, put, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use crate::middleware::get_claims;
use icn_governance::{
    Charter, CharterStatus, DisputePolicy, FounderSignature, GovernanceConfig, MembershipPolicy,
    OrgType,
};
use icn_identity::Did;

// ============================================================================
// Response/Request DTOs
// ============================================================================

/// Charter summary response (for list endpoints)
#[derive(Debug, Serialize, Deserialize)]
pub struct CharterSummaryResponse {
    pub charter_id: String,
    pub domain_id: String,
    pub name: String,
    pub org_type: String,
    pub status: String,
    pub founder_count: usize,
    pub created_at: u64,
}

/// Charter detail response
#[derive(Debug, Serialize, Deserialize)]
pub struct CharterDetailResponse {
    pub charter_id: String,
    pub domain_id: String,
    pub name: String,
    pub description: Option<String>,
    pub org_type: String,
    pub status: String,
    pub founders: Vec<FounderResponse>,
    pub created_at: u64,
    pub bootstrap_endpoints: Vec<String>,
}

/// Founder signature response
#[derive(Debug, Serialize, Deserialize)]
pub struct FounderResponse {
    pub did: String,
    pub role: Option<String>,
    pub timestamp: u64,
}

/// Create charter request
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCharterRequest {
    pub domain_id: String,
    pub name: String,
    pub description: Option<String>,
    pub org_type: String,
    #[serde(default)]
    pub bootstrap_endpoints: Vec<String>,
}

/// Sign charter request
#[derive(Debug, Serialize, Deserialize)]
pub struct SignCharterRequest {
    pub role: Option<String>,
    pub signature: String,
}

/// Update status request
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateCharterStatusRequest {
    pub status: String,
    pub reason: Option<String>,
}

fn charter_to_summary(c: &Charter) -> CharterSummaryResponse {
    CharterSummaryResponse {
        charter_id: c.charter_id.to_hex(),
        domain_id: c.domain_id.clone(),
        name: c.name.clone(),
        org_type: format_org_type(&c.org_type),
        status: format_status(&c.status),
        founder_count: c.founders.len(),
        created_at: c.created_at,
    }
}

fn charter_to_detail(c: &Charter) -> CharterDetailResponse {
    CharterDetailResponse {
        charter_id: c.charter_id.to_hex(),
        domain_id: c.domain_id.clone(),
        name: c.name.clone(),
        description: c.description.clone(),
        org_type: format_org_type(&c.org_type),
        status: format_status(&c.status),
        founders: c
            .founders
            .iter()
            .map(|f| FounderResponse {
                did: f.did.to_string(),
                role: f.role.clone(),
                timestamp: f.timestamp,
            })
            .collect(),
        created_at: c.created_at,
        bootstrap_endpoints: c.bootstrap_endpoints.clone(),
    }
}

fn format_org_type(t: &OrgType) -> String {
    match t {
        OrgType::Cooperative => "cooperative".to_string(),
        OrgType::Community => "community".to_string(),
        OrgType::Federation => "federation".to_string(),
    }
}

fn format_status(s: &CharterStatus) -> String {
    match s {
        CharterStatus::Draft => "draft".to_string(),
        CharterStatus::Active => "active".to_string(),
        CharterStatus::Suspended { .. } => "suspended".to_string(),
        CharterStatus::Dissolved { .. } => "dissolved".to_string(),
    }
}

fn parse_org_type(s: &str) -> Option<OrgType> {
    match s.to_lowercase().as_str() {
        "cooperative" | "coop" => Some(OrgType::Cooperative),
        "community" => Some(OrgType::Community),
        "federation" => Some(OrgType::Federation),
        _ => None,
    }
}

fn parse_status(s: &str, reason: Option<String>) -> Option<CharterStatus> {
    match s.to_lowercase().as_str() {
        "draft" => Some(CharterStatus::Draft),
        "active" => Some(CharterStatus::Active),
        "suspended" => Some(CharterStatus::Suspended {
            reason: reason.unwrap_or_else(|| "No reason provided".to_string()),
        }),
        _ => None,
    }
}

// ============================================================================
// Charter CRUD Endpoints
// ============================================================================

/// POST /v1/charter - Create a new charter
#[post("")]
pub async fn create_charter(
    http_req: HttpRequest,
    body: web::Json<CreateCharterRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // Require authentication
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let founder_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Parse org type
    let org_type = parse_org_type(&body.org_type).ok_or_else(|| {
        GatewayError::BadRequest(
            "Invalid org_type. Must be: cooperative, community, or federation".to_string(),
        )
    })?;

    // Create charter with defaults
    let mut charter = Charter::new(
        org_type,
        body.domain_id.clone(),
        body.name.clone(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );

    // Add description and endpoints
    charter.description = body.description.clone();
    charter.bootstrap_endpoints = body.bootstrap_endpoints.clone();

    // Add creator as first founder
    let founder_sig = FounderSignature::new(founder_did, vec![]);
    charter.founders.push(founder_sig);

    // Store charter
    commons_manager.store_charter(charter.clone()).await?;

    Ok(HttpResponse::Created().json(charter_to_detail(&charter)))
}

/// GET /v1/charter/{id} - Get charter by ID
#[get("/{charter_id}")]
pub async fn get_charter(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let charter_id = path.into_inner();

    let charter = commons_manager
        .get_charter(&charter_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    Ok(HttpResponse::Ok().json(charter_to_detail(&charter)))
}

/// GET /v1/charter/by-domain/{domain_id} - Get charter by domain ID
#[get("/by-domain/{domain_id}")]
pub async fn get_charter_by_domain(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let domain_id = path.into_inner();

    let charter = commons_manager
        .get_charter_by_domain(&domain_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    Ok(HttpResponse::Ok().json(charter_to_detail(&charter)))
}

/// GET /v1/charter - List charters with optional filters
#[get("")]
pub async fn list_charters(
    query: web::Query<ListChartersQuery>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let org_type = query.org_type.as_ref().and_then(|s| parse_org_type(s));
    let status = query.status.as_ref().and_then(|s| parse_status(s, None));

    let charters = commons_manager.list_charters(org_type, status).await?;

    let response: Vec<CharterSummaryResponse> = charters.iter().map(charter_to_summary).collect();

    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
pub struct ListChartersQuery {
    pub org_type: Option<String>,
    pub status: Option<String>,
}

// ============================================================================
// Charter Lifecycle Endpoints
// ============================================================================

/// POST /v1/charter/{id}/sign - Add founder signature
#[post("/{charter_id}/sign")]
pub async fn sign_charter(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SignCharterRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let signer_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let charter_id = path.into_inner();

    // Get charter
    let charter = commons_manager
        .get_charter(&charter_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    // Check if already signed
    if charter.founders.iter().any(|f| f.did == signer_did) {
        return Err(GatewayError::BadRequest(
            "Already signed this charter".to_string(),
        ));
    }

    // Create signature
    let signature = hex::decode(&body.signature).unwrap_or_default();
    let mut sig = FounderSignature::new(signer_did, signature);
    sig.role = body.role.clone();

    // Note: In production, we'd update the charter in storage
    // For now, return success with current count + 1
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "signed",
        "charter_id": charter_id,
        "total_founders": charter.founders.len() + 1,
    })))
}

/// POST /v1/charter/{id}/activate - Activate charter (requires minimum founders)
#[post("/{charter_id}/activate")]
pub async fn activate_charter(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let charter_id = path.into_inner();

    // Get charter
    let charter = commons_manager
        .get_charter(&charter_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    // Check minimum founders (default: 3)
    let min_founders = 3;
    if charter.founders.len() < min_founders {
        return Err(GatewayError::BadRequest(format!(
            "Need at least {min_founders} founders to activate, have {}",
            charter.founders.len()
        )));
    }

    // Update status to active
    commons_manager
        .update_charter_status(&charter_id, CharterStatus::Active)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "activated",
        "charter_id": charter_id,
    })))
}

/// PUT /v1/charter/{id}/status - Update charter status
#[put("/{charter_id}/status")]
pub async fn update_charter_status(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateCharterStatusRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let charter_id = path.into_inner();

    // Verify charter exists
    let _charter = commons_manager
        .get_charter(&charter_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    // Parse status
    let status = parse_status(&body.status, body.reason.clone()).ok_or_else(|| {
        GatewayError::BadRequest("Invalid status. Must be: draft, active, or suspended".to_string())
    })?;

    commons_manager
        .update_charter_status(&charter_id, status)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "updated",
        "charter_id": charter_id,
        "new_status": body.status,
    })))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure charter routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_charter)
        .service(get_charter)
        .service(get_charter_by_domain)
        .service(list_charters)
        .service(sign_charter)
        .service(activate_charter)
        .service(update_charter_status);
}
