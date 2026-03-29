//! Commons Evolution API endpoints
//!
//! Provides REST API for Commons Evolution Layer 0-2:
//! - PersonhoodAnchor management (Layer 0)
//! - CommonsHolderRecord management (Layer 1)
//! - Affiliation/membership management

pub mod anchor;

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use crate::middleware::get_claims;
use icn_identity::{Affiliation, CommonsHolderRecord, Did, JurisdictionId, MembershipStatus};

// ============================================================================
// Response/Request DTOs
// ============================================================================

/// Commons status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommonsStatusResponse {
    /// Whether the user has a commons identity
    pub enrolled: bool,
    /// Anchor ID if enrolled
    pub anchor_id: Option<String>,
    /// Holder ID if enrolled
    pub holder_id: Option<String>,
    /// DID string
    pub did: String,
    /// Current personhood level
    pub personhood_level: Option<String>,
    /// Holder status
    pub status: Option<String>,
    /// Number of affiliations
    pub affiliation_count: usize,
}

/// Holder detail response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HolderDetailResponse {
    pub holder_id: String,
    pub anchor_id: String,
    pub did: String,
    pub status: String,
    pub personhood_level: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub affiliations: Vec<AffiliationResponse>,
}

/// Affiliation response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AffiliationResponse {
    pub jurisdiction_id: String,
    pub membership_status: String,
    pub capabilities: Vec<String>,
    pub roles: Vec<String>,
    pub joined_at: u64,
    /// Scope level derived from the jurisdiction type (e.g., "org", "community",
    /// "federation", "network"). None if the jurisdiction ID format is unrecognized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_level: Option<String>,
}

impl From<&Affiliation> for AffiliationResponse {
    fn from(a: &Affiliation) -> Self {
        let scope_level = a.jurisdiction_id.jurisdiction_type().map(|jt| {
            match jt {
                icn_identity::JurisdictionType::Cooperative => "org",
                icn_identity::JurisdictionType::Community => "community",
                icn_identity::JurisdictionType::Federation => "federation",
                icn_identity::JurisdictionType::Network => "network",
            }
            .to_string()
        });

        Self {
            jurisdiction_id: a.jurisdiction_id.to_string(),
            membership_status: format!("{}", a.membership_status),
            capabilities: a.capabilities.iter().map(|c| format!("{c}")).collect(),
            roles: a.roles.clone(),
            joined_at: a.joined_at,
            scope_level,
        }
    }
}

/// Join jurisdiction request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct JoinJurisdictionRequest {
    pub jurisdiction_id: String,
}

/// Update affiliation status request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateAffiliationRequest {
    pub status: String,
}

// ============================================================================
// Commons Status Endpoint
// ============================================================================

/// GET /v1/commons/status - Get current user's commons status
///
/// Returns the authenticated user's commons enrollment status,
/// including anchor ID, holder ID, and basic profile info.
#[get("/status")]
pub async fn get_commons_status(
    http_req: HttpRequest,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // Get authenticated user's DID
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Check if user has an anchor
    let anchor = commons_manager.get_anchor_by_did(&did).await?;
    let holder = commons_manager.get_holder_by_did(&did).await?;

    let response = CommonsStatusResponse {
        enrolled: anchor.is_some() && holder.is_some(),
        anchor_id: anchor.as_ref().map(|a| hex::encode(a.id())),
        holder_id: holder.as_ref().map(|h| hex::encode(h.id())),
        did: did.to_string(),
        personhood_level: holder.as_ref().map(|h| h.personhood_level.to_string()),
        status: holder.as_ref().map(|h| format!("{}", h.status)),
        affiliation_count: holder.map(|h| h.affiliations.len()).unwrap_or(0),
    };

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Holder Endpoints
// ============================================================================

/// GET /v1/commons/holder/{did} - Get holder by DID
///
/// Authentication required. Returns the holder record for the given DID.
#[get("/holder/{did}")]
pub async fn get_holder_by_did(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let did_str = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let holder = commons_manager
        .get_holder_by_did(&did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Holder not found".to_string()))?;

    let response = holder_to_response(&holder);
    Ok(HttpResponse::Ok().json(response))
}

/// GET /v1/commons/holder/id/{holder_id} - Get holder by ID
///
/// Authentication required. Returns the holder record for the given internal ID.
#[get("/holder/id/{holder_id}")]
pub async fn get_holder_by_id(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let holder_id = path.into_inner();

    let holder = commons_manager
        .get_holder(&holder_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Holder not found".to_string()))?;

    let response = holder_to_response(&holder);
    Ok(HttpResponse::Ok().json(response))
}

fn holder_to_response(holder: &CommonsHolderRecord) -> HolderDetailResponse {
    HolderDetailResponse {
        holder_id: hex::encode(holder.id()),
        anchor_id: hex::encode(holder.anchor_id),
        did: holder.holder_did.to_string(),
        status: format!("{}", holder.status),
        personhood_level: holder.personhood_level.to_string(),
        created_at: holder.created_at,
        updated_at: holder.updated_at,
        affiliations: holder.affiliations.iter().map(|a| a.into()).collect(),
    }
}

// ============================================================================
// Affiliation Endpoints
// ============================================================================

/// GET /v1/commons/holder/{did}/affiliations - List affiliations
///
/// Self-only: caller must be the holder being queried. A holder's full
/// affiliation history is personal — it reveals which cooperatives and
/// jurisdictions they belong to, their capabilities in each, and their
/// membership timeline.
///
/// Use `GET /v1/membership/list/{jurisdiction}` to enumerate a
/// jurisdiction's roster (requires member standing in that jurisdiction).
#[get("/holder/{did}/affiliations")]
pub async fn list_affiliations(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let did_str = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Self-only: affiliation history spans all jurisdictions and must not be
    // enumerable by arbitrary callers.
    if claims.sub != did.to_string() {
        return Err(GatewayError::AuthorizationFailed(
            "Can only list affiliations for your own identity".to_string(),
        ));
    }

    let holder = commons_manager
        .get_holder_by_did(&did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Holder not found".to_string()))?;

    let holder_id = hex::encode(holder.id());
    let affiliations = commons_manager.list_affiliations(&holder_id).await?;

    let response: Vec<AffiliationResponse> = affiliations.iter().map(|a| a.into()).collect();
    Ok(HttpResponse::Ok().json(response))
}

/// POST /v1/commons/holder/{did}/affiliations - Join jurisdiction
#[post("/holder/{did}/affiliations")]
pub async fn join_jurisdiction(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<JoinJurisdictionRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // Verify authenticated user is the holder
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let did_str = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Verify caller is the holder
    if claims.sub != did.to_string() {
        return Err(GatewayError::AuthorizationFailed(
            "Can only join jurisdictions for your own identity".to_string(),
        ));
    }

    let holder = commons_manager
        .get_holder_by_did(&did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Holder not found".to_string()))?;

    let holder_id = hex::encode(holder.id());
    let jurisdiction = JurisdictionId::new(&body.jurisdiction_id);

    // Capabilities are not accepted from the caller — they are jurisdiction-granted.
    // System-initiated flows (e.g., SDIS enrollment) use the internal CommonsManager
    // API directly with explicit initial_capabilities.
    let affiliation = commons_manager
        .join_jurisdiction(&holder_id, jurisdiction, vec![])
        .await?;

    Ok(HttpResponse::Created().json(AffiliationResponse::from(&affiliation)))
}

/// PUT /v1/commons/holder/{did}/affiliations/{jurisdiction} - Update affiliation status
#[put("/holder/{did}/affiliations/{jurisdiction}")]
pub async fn update_affiliation(
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<UpdateAffiliationRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // This endpoint would typically be called by coop governance
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let (did_str, jurisdiction_id) = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let holder = commons_manager
        .get_holder_by_did(&did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Holder not found".to_string()))?;

    let holder_id = hex::encode(holder.id());
    let jurisdiction = JurisdictionId::new(&jurisdiction_id);

    let status = parse_membership_status(&body.status)
        .ok_or_else(|| GatewayError::BadRequest("Invalid status".to_string()))?;

    commons_manager
        .update_affiliation_status(&holder_id, &jurisdiction, status)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "updated",
        "jurisdiction_id": jurisdiction_id,
        "new_status": body.status,
    })))
}

/// DELETE /v1/commons/holder/{did}/affiliations/{jurisdiction} - Leave jurisdiction
#[delete("/holder/{did}/affiliations/{jurisdiction}")]
pub async fn leave_jurisdiction(
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // Verify authenticated user is the holder
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let (did_str, jurisdiction_id) = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Verify caller is the holder
    if claims.sub != did.to_string() {
        return Err(GatewayError::AuthorizationFailed(
            "Can only leave jurisdictions from your own identity".to_string(),
        ));
    }

    let holder = commons_manager
        .get_holder_by_did(&did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Holder not found".to_string()))?;

    let holder_id = hex::encode(holder.id());
    let jurisdiction = JurisdictionId::new(&jurisdiction_id);

    commons_manager
        .leave_jurisdiction(&holder_id, &jurisdiction)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "exited",
        "jurisdiction_id": jurisdiction_id,
    })))
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_membership_status(s: &str) -> Option<MembershipStatus> {
    match s.to_lowercase().as_str() {
        "candidate" => Some(MembershipStatus::Candidate),
        "provisional" => Some(MembershipStatus::Provisional),
        "member" => Some(MembershipStatus::Member),
        "suspended" => Some(MembershipStatus::Suspended),
        "exited" => Some(MembershipStatus::Exited),
        "banned" => Some(MembershipStatus::Banned),
        _ => None,
    }
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure commons routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_commons_status)
        .service(get_holder_by_did)
        .service(get_holder_by_id)
        .service(list_affiliations)
        .service(join_jurisdiction)
        .service(update_affiliation)
        .service(leave_jurisdiction)
        .service(web::scope("/anchor").configure(anchor::configure));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App, HttpMessage};
    use crate::auth::TokenClaims;

    #[actix_web::test]
    async fn test_get_holder_not_found() {
        let commons_manager = Arc::new(CommonsManager::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(commons_manager))
                .service(web::scope("/v1/commons").configure(configure)),
        )
        .await;

        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().to_string();

        // Inject claims directly into request extensions — the canonical test bypass
        // for handlers that call get_claims(). No holder exists for this DID, so the
        // handler should return 404 after auth passes.
        let claims = TokenClaims {
            sub: did.clone(),
            iat: 1_000_000_000,
            exp: 9_999_999_999,
            coop_id: "test-coop".to_string(),
            scopes: vec![],
        };
        let req = test::TestRequest::get()
            .uri(&format!("/v1/commons/holder/{did}"))
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }
}
