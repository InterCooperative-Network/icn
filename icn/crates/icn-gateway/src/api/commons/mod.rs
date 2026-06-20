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
use icn_identity::{
    Affiliation, CommonsHolderRecord, Did, JurisdictionId, KeyPair, MembershipCapability,
    MembershipStatus,
};

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
// Dev/Demo-only standing bootstrap
// ============================================================================

/// Request body for [`dev_bootstrap_standing`].
#[derive(Debug, Deserialize)]
pub struct DevBootstrapStandingRequest {
    /// Jurisdiction (governance domain id) to establish Member standing in.
    pub jurisdiction_id: String,
}

/// Returns `true` only when BOTH dev gates are satisfied:
/// 1. `ICN_ENABLE_ADMIN_ENDPOINTS=true` (same explicit opt-in as the SDIS
///    `approve_ceremony` / `approve_recovery` admin endpoints), AND
/// 2. the governance build posture is NOT `Production`
///    (`ICN_GOVERNANCE_BUILD_MODE=production`).
///
/// Default (no env set) is `false` → the endpoint is unavailable. The posture
/// gate makes it impossible to enable in production even if the opt-in flag is
/// set by mistake.
fn demo_standing_bootstrap_enabled() -> bool {
    let admin_enabled = std::env::var("ICN_ENABLE_ADMIN_ENDPOINTS")
        .unwrap_or_else(|_| "false".to_string())
        .to_lowercase()
        == "true";
    let is_production = icn_governance_actor::http::GovernanceContextBuildMode::from_env()
        == icn_governance_actor::http::GovernanceContextBuildMode::Production;
    admin_enabled && !is_production
}

/// POST /v1/commons/dev/bootstrap-standing — DEV/DEMO-ONLY.
///
/// Establishes commons `Member` standing for the **authenticated caller's own
/// DID** (`claims.sub`) in `jurisdiction_id`, so a local secured-gateway demo
/// (e.g. a NYCN v4 bash flow) can then submit governed proposals, which the
/// gateway gates on Member standing via `member_checker`.
///
/// This deliberately bypasses the multi-steward SDIS proof-of-personhood
/// ceremony and is therefore **double dev-gated** (see
/// [`demo_standing_bootstrap_enabled`]): it returns `403 Forbidden` unless
/// `ICN_ENABLE_ADMIN_ENDPOINTS=true` AND the posture is non-`Production`.
///
/// Safety: it grants standing for the caller's own DID only — never an
/// arbitrary DID — and removes no membership/SDIS/governance check. It adds a
/// dev path to *create* a commons holder; production enrollment, SDIS, and the
/// `member_checker` gate are unchanged and still enforced for every request.
/// It is NOT a production "make me a Member" route.
#[post("/dev/bootstrap-standing")]
pub async fn dev_bootstrap_standing(
    http_req: HttpRequest,
    body: web::Json<DevBootstrapStandingRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // Dev gate: disabled by default; impossible to enable in Production posture.
    if !demo_standing_bootstrap_enabled() {
        return Err(GatewayError::Forbidden(
            "Demo standing bootstrap is disabled (requires ICN_ENABLE_ADMIN_ENDPOINTS=true and \
             non-production posture)"
                .to_string(),
        ));
    }

    // Operate on the caller's own DID only.
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;
    let did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let jurisdiction = JurisdictionId::new(&body.jurisdiction_id);

    // Idempotent enrollment: reuse an existing holder if the DID already has one.
    let holder = match commons_manager.get_holder_by_did(&did).await? {
        Some(existing) => existing,
        None => {
            // Idempotent enrollment: reuse an existing personhood anchor for this
            // DID if one is already present (e.g. from a prior partial run or an
            // earlier SDIS enrollment) — `create_anchor_from_enrollment` errors on
            // a duplicate anchor, so only mint a new one when none exists.
            //
            // A freshly minted demo anchor uses a synthetic voucher (a throwaway
            // key) to produce a WebOfTrust/`Strong` anchor: the commons Sybil gate
            // (`join_jurisdiction`) requires at least `Strong` POP level, so this
            // SATISFIES the gate rather than weakening it (the same shape PR
            // #1980's proven in-process helper uses). Production obtains real
            // steward attestations via the SDIS ceremony.
            let anchor_id = match commons_manager.get_anchor_by_did(&did).await? {
                Some(existing_anchor) => {
                    // A reused anchor that was suspended/revoked at the SDIS-anchor
                    // level must not yield fresh standing: `create_holder_from_anchor`
                    // builds an active holder regardless of anchor status, so fail
                    // closed here before reusing it.
                    if !existing_anchor.is_active() {
                        return Err(GatewayError::Forbidden(format!(
                            "cannot bootstrap standing: personhood anchor is not active \
                             ({}); a suspended/revoked anchor must be resolved via \
                             SDIS/governance, not the demo bridge",
                            existing_anchor.status
                        )));
                    }
                    hex::encode(existing_anchor.id())
                }
                None => {
                    let voucher = KeyPair::generate().map_err(|e| {
                        GatewayError::InternalError(format!(
                            "demo voucher key generation failed: {e}"
                        ))
                    })?;
                    let anchor = commons_manager
                        .create_anchor_from_enrollment(&did, Some(voucher.did()))
                        .await?;
                    hex::encode(anchor.id())
                }
            };
            commons_manager
                .create_holder_from_anchor(&anchor_id, &did)
                .await?
        }
    };

    // A holder removed at the commons-holder level (`Suspended`/`Exited`/`Revoked`)
    // must not regain standing via the dev bridge: the gateway `member_checker`
    // only checks the jurisdiction affiliation, not the holder's lifecycle status,
    // so fail closed here before touching affiliations. A freshly enrolled holder
    // is `Active`, so this only rejects a reused, removed holder.
    if !holder.is_active() {
        return Err(GatewayError::Forbidden(format!(
            "cannot bootstrap standing: commons holder is not active ({}); a removed holder \
             must be resolved via governance/recovery, not the demo bridge",
            holder.status
        )));
    }

    // The holder's backing personhood anchor must also be active. Anchor status
    // changes do not cascade to holders, so a reused `Active` holder may be backed
    // by an anchor that was suspended/revoked after the holder was created. (For
    // the freshly enrolled path the anchor was just minted/validated `Active`, so
    // this is a no-op there.)
    let backing_anchor = commons_manager
        .get_anchor(&hex::encode(holder.anchor_id))
        .await?
        .ok_or_else(|| {
            GatewayError::InternalError("backing personhood anchor missing for holder".to_string())
        })?;
    if !backing_anchor.is_active() {
        return Err(GatewayError::Forbidden(format!(
            "cannot bootstrap standing: backing personhood anchor is not active ({}); a \
             suspended/revoked anchor must be resolved via SDIS/governance, not the demo bridge",
            backing_anchor.status
        )));
    }

    let holder_id = hex::encode(holder.id());

    // Resolve the caller's current affiliation (if any) in this jurisdiction.
    // The dev bridge advances onboarding to `Member`, but it must NOT override
    // governance-imposed removed/blocked states (`Suspended`/`Banned`/`Exited`):
    // those must be cleared via the governance/recovery path, not this route, so
    // a removed identity cannot regain proposal standing through the demo bridge.
    let existing_status = commons_manager
        .list_affiliations(&holder_id)
        .await?
        .into_iter()
        .find(|a| a.jurisdiction_id == jurisdiction)
        .map(|a| a.membership_status);

    match existing_status {
        // Already an active Member — idempotent no-op.
        Some(MembershipStatus::Member) => {}
        // Mid-onboarding — advance to Member (the intended demo path).
        Some(MembershipStatus::Candidate) | Some(MembershipStatus::Provisional) => {
            commons_manager
                .update_affiliation_status(&holder_id, &jurisdiction, MembershipStatus::Member)
                .await?;
        }
        // Not yet affiliated — join (lands at Candidate) then advance to Member.
        None => {
            commons_manager
                .join_jurisdiction(
                    &holder_id,
                    jurisdiction.clone(),
                    vec![MembershipCapability::Vote],
                )
                .await?;
            commons_manager
                .update_affiliation_status(&holder_id, &jurisdiction, MembershipStatus::Member)
                .await?;
        }
        // Governance-imposed removed/blocked status — refuse; do not reactivate.
        Some(blocked) => {
            return Err(GatewayError::Forbidden(format!(
                "cannot bootstrap standing: existing affiliation status is {blocked:?}; a \
                 Suspended/Banned/Exited membership must be resolved via governance/recovery, \
                 not the demo bridge"
            )));
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "member_standing_bootstrapped",
        "did": did.to_string(),
        "jurisdiction_id": body.jurisdiction_id,
        "holder_id": holder_id,
        "note": "dev/demo-only; not a production enrollment path",
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
        // Dev/demo-only; refuses unless ICN_ENABLE_ADMIN_ENDPOINTS=true and
        // non-Production posture (see dev_bootstrap_standing).
        .service(dev_bootstrap_standing)
        .service(web::scope("/anchor").configure(anchor::configure));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use actix_web::{test, App, HttpMessage};

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
            entity_id: None,
            entity_type: None,
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
