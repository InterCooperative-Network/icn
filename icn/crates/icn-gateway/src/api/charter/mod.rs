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
use utoipa::ToSchema;

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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FounderResponse {
    pub did: String,
    pub role: Option<String>,
    pub timestamp: u64,
}

/// Create charter request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateCharterRequest {
    pub domain_id: String,
    pub name: String,
    pub description: Option<String>,
    pub org_type: String,
    #[serde(default)]
    pub bootstrap_endpoints: Vec<String>,
}

/// Sign charter request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SignCharterRequest {
    pub role: Option<String>,
    pub signature: String,
}

/// Update status request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
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

    // Create founder signature
    let signature_bytes = hex::decode(&body.signature).unwrap_or_default();
    let mut sig = FounderSignature::new(signer_did, signature_bytes);
    sig.role = body.role.clone();

    // Add signature to charter (validates status, checks duplicates, persists)
    let updated_charter = commons_manager
        .add_charter_signature(&charter_id, sig)
        .await?;

    // Check if charter is ready for activation (default: 3 founders)
    let min_founders = 3;
    let ready_for_activation = updated_charter.founders.len() >= min_founders;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "signed",
        "charter_id": charter_id,
        "total_founders": updated_charter.founders.len(),
        "ready_for_activation": ready_for_activation,
        "founders_needed": if ready_for_activation { 0 } else { min_founders - updated_charter.founders.len() },
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
// Enhanced Viewing Endpoints
// ============================================================================

/// Detailed founder response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FounderDetailResponse {
    pub did: String,
    pub role: Option<String>,
    pub timestamp: u64,
    /// Human-readable timestamp
    pub signed_at: String,
    /// Truncated DID for display
    pub display_name: String,
    /// Has valid signature
    pub has_signature: bool,
}

/// Founders list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FoundersResponse {
    pub charter_id: String,
    pub total_founders: usize,
    pub minimum_required: usize,
    pub ready_for_activation: bool,
    pub founders_needed: usize,
    pub founders: Vec<FounderDetailResponse>,
}

/// Timeline event
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TimelineEvent {
    /// Event type
    pub event_type: String,
    /// Human-readable description
    pub description: String,
    /// Timestamp (Unix)
    pub timestamp: u64,
    /// Human-readable timestamp
    pub date_time: String,
    /// Related actor (DID)
    pub actor: Option<String>,
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Timeline response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TimelineResponse {
    pub charter_id: String,
    pub charter_name: String,
    pub events: Vec<TimelineEvent>,
}

fn format_timestamp(timestamp: u64) -> String {
    // Simple ISO-like format
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    let duration_since_epoch = datetime.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration_since_epoch.as_secs();

    // Convert to date/time components (simplified)
    let days = secs / 86400;
    let years = 1970 + (days / 365); // Approximate
    let days_in_year = days % 365;
    let months = days_in_year / 30 + 1;
    let day = days_in_year % 30 + 1;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    format!("{years:04}-{months:02}-{day:02} {hours:02}:{mins:02} UTC")
}

fn truncate_did(did: &str) -> String {
    if did.len() > 24 {
        format!("{}...{}", &did[..16], &did[did.len() - 6..])
    } else {
        did.to_string()
    }
}

/// GET /v1/charter/{id}/summary - Get charter summary
#[get("/{charter_id}/summary")]
pub async fn get_charter_summary(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let charter_id = path.into_inner();

    let charter = commons_manager
        .get_charter(&charter_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    Ok(HttpResponse::Ok().json(charter_to_summary(&charter)))
}

/// GET /v1/charter/{id}/founders - Get detailed founder information
#[get("/{charter_id}/founders")]
pub async fn get_charter_founders(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let charter_id = path.into_inner();

    let charter = commons_manager
        .get_charter(&charter_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    let min_founders = 3;
    let total = charter.founders.len();

    let founders: Vec<FounderDetailResponse> = charter
        .founders
        .iter()
        .map(|f| FounderDetailResponse {
            did: f.did.to_string(),
            role: f.role.clone(),
            timestamp: f.timestamp,
            signed_at: format_timestamp(f.timestamp),
            display_name: truncate_did(&f.did.to_string()),
            has_signature: !f.signature.is_empty(),
        })
        .collect();

    Ok(HttpResponse::Ok().json(FoundersResponse {
        charter_id: charter.charter_id.to_hex(),
        total_founders: total,
        minimum_required: min_founders,
        ready_for_activation: total >= min_founders,
        founders_needed: min_founders.saturating_sub(total),
        founders,
    }))
}

/// GET /v1/charter/{id}/timeline - Get charter timeline
#[get("/{charter_id}/timeline")]
pub async fn get_charter_timeline(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let charter_id = path.into_inner();

    let charter = commons_manager
        .get_charter(&charter_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Charter not found".to_string()))?;

    let mut events: Vec<TimelineEvent> = Vec::new();

    // 1. Creation event
    events.push(TimelineEvent {
        event_type: "charter_created".to_string(),
        description: format!("Charter '{}' was created", charter.name),
        timestamp: charter.created_at,
        date_time: format_timestamp(charter.created_at),
        actor: charter.founders.first().map(|f| f.did.to_string()),
        metadata: Some(serde_json::json!({
            "org_type": format_org_type(&charter.org_type),
            "domain_id": charter.domain_id,
        })),
    });

    // 2. Founder signature events
    for founder in &charter.founders {
        events.push(TimelineEvent {
            event_type: "founder_signed".to_string(),
            description: format!(
                "{} signed as {}",
                truncate_did(&founder.did.to_string()),
                founder.role.as_deref().unwrap_or("founder")
            ),
            timestamp: founder.timestamp,
            date_time: format_timestamp(founder.timestamp),
            actor: Some(founder.did.to_string()),
            metadata: founder
                .role
                .as_ref()
                .map(|r| serde_json::json!({"role": r})),
        });
    }

    // 3. Status change event (if activated)
    if matches!(charter.status, CharterStatus::Active) {
        // Find the latest founder signature as approximate activation time
        let activation_time = charter
            .founders
            .iter()
            .map(|f| f.timestamp)
            .max()
            .unwrap_or(charter.created_at);

        events.push(TimelineEvent {
            event_type: "charter_activated".to_string(),
            description: "Charter was activated".to_string(),
            timestamp: activation_time,
            date_time: format_timestamp(activation_time),
            actor: None,
            metadata: None,
        });
    }

    // 4. Amendment events
    for (i, amendment_ref) in charter.amendments.iter().enumerate() {
        events.push(TimelineEvent {
            event_type: "amendment_added".to_string(),
            description: format!("Amendment #{} was ratified", i + 1),
            timestamp: amendment_ref.ratified_at,
            date_time: format_timestamp(amendment_ref.ratified_at),
            actor: None,
            metadata: Some(serde_json::json!({
                "amendment_id": hex::encode(amendment_ref.amendment_id),
            })),
        });
    }

    // 5. Suspension event (if suspended)
    if let CharterStatus::Suspended { reason } = &charter.status {
        events.push(TimelineEvent {
            event_type: "charter_suspended".to_string(),
            description: format!("Charter was suspended: {reason}"),
            timestamp: charter.created_at, // Would need actual suspension timestamp
            date_time: format_timestamp(charter.created_at),
            actor: None,
            metadata: Some(serde_json::json!({"reason": reason})),
        });
    }

    // Sort by timestamp
    events.sort_by_key(|e| e.timestamp);

    Ok(HttpResponse::Ok().json(TimelineResponse {
        charter_id: charter.charter_id.to_hex(),
        charter_name: charter.name.clone(),
        events,
    }))
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
        .service(update_charter_status)
        .service(get_charter_summary)
        .service(get_charter_founders)
        .service(get_charter_timeline);
}
