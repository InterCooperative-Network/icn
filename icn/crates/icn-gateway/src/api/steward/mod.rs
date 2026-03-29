//! Steward API endpoints
//!
//! Provides REST API for steward management:
//! - Steward registration and lookup
//! - Status management (suspend, reinstate, retire, revoke)
//! - Attestation tracking
//! - Bond management

use actix_web::{get, post, put, web, HttpRequest, HttpResponse};
use hex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::authority::{require_membership_in_jurisdiction, require_office_in_jurisdiction};
use crate::commons_mgr::{steward_to_detail, steward_to_summary, CommonsManager};
use crate::error::{GatewayError, Result};
use crate::middleware::get_claims;
use icn_identity::{Did, JurisdictionId};

// ============================================================================
// Response/Request DTOs
// ============================================================================

pub use crate::models::{StewardDetailResponse, StewardSummaryResponse};

/// Register steward request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterStewardRequest {
    /// Operational DID for the steward (can be same as holder DID)
    pub steward_did: Option<String>,
    /// Term duration in days (e.g., 365 for 1 year)
    pub term_duration_days: u64,
    /// Bond amount in network credits
    pub bond_amount: u64,
    /// Governance proposal ID that approved this steward
    pub governance_approval: String,
    /// Optional jurisdiction scope
    pub jurisdiction: Option<String>,
    /// Optional specializations (e.g., "identity", "mediation")
    #[serde(default)]
    pub specializations: Vec<String>,
}

/// Update steward status request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateStatusRequest {
    pub status: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// Extend term request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExtendTermRequest {
    /// Additional days to extend
    pub additional_days: u64,
}

/// Bond operation request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BondOperationRequest {
    pub amount: u64,
}

// ============================================================================
// Steward CRUD Endpoints
// ============================================================================

/// POST /v1/steward - Register as a steward
#[post("")]
pub async fn register_steward(
    http_req: HttpRequest,
    body: web::Json<RegisterStewardRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // Require authentication
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let holder_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Use provided steward_did or default to holder_did
    let steward_did = if let Some(ref did_str) = body.steward_did {
        did_str
            .parse::<Did>()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid steward DID: {e}")))?
    } else {
        holder_did.clone()
    };

    // Validate term duration
    if body.term_duration_days < 30 {
        return Err(GatewayError::BadRequest(
            "Term duration must be at least 30 days".to_string(),
        ));
    }
    if body.term_duration_days > 730 {
        return Err(GatewayError::BadRequest(
            "Term duration cannot exceed 730 days (2 years)".to_string(),
        ));
    }

    // Validate bond amount
    if body.bond_amount < 100 {
        return Err(GatewayError::BadRequest(
            "Minimum bond amount is 100 credits".to_string(),
        ));
    }

    // Register steward
    let steward = commons_manager
        .register_steward(
            &holder_did,
            &steward_did,
            body.term_duration_days,
            body.bond_amount,
            body.governance_approval.clone(),
            body.jurisdiction.clone(),
            body.specializations.clone(),
        )
        .await
        .map_err(|e| GatewayError::BadRequest(format!("Registration failed: {e}")))?;

    Ok(HttpResponse::Created().json(steward_to_detail(&steward)))
}

/// GET /v1/steward/{id} - Get steward by ID
#[get("/{steward_id}")]
pub async fn get_steward(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let steward_id = path.into_inner();

    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(steward_to_detail(&steward)))
}

/// GET /v1/steward/by-did/{did} - Get steward by DID
#[get("/by-did/{did}")]
pub async fn get_steward_by_did(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let did_str = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let steward = commons_manager
        .get_steward_by_did(&did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(steward_to_detail(&steward)))
}

/// GET /v1/steward - List stewards with optional filters
#[get("")]
pub async fn list_stewards(
    query: web::Query<ListStewardsQuery>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let active_only = query.active.unwrap_or(false);
    let jurisdiction = query.jurisdiction.as_deref();

    let stewards = commons_manager
        .list_stewards(active_only, jurisdiction)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let response: Vec<StewardSummaryResponse> = stewards.iter().map(steward_to_summary).collect();

    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListStewardsQuery {
    pub active: Option<bool>,
    pub jurisdiction: Option<String>,
}

/// GET /v1/steward/attesters - List stewards who can issue attestations
#[get("/attesters")]
pub async fn list_attesters(
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let attesters = commons_manager
        .list_attesters()
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    let response: Vec<StewardSummaryResponse> = attesters.iter().map(steward_to_summary).collect();

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Steward Lifecycle Endpoints
// ============================================================================

/// PUT /v1/steward/{id}/status - Update steward status
#[put("/{steward_id}/status")]
pub async fn update_steward_status(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateStatusRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Verify steward exists and resolve their jurisdiction for authority check
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    // Require caller to hold office in the steward's jurisdiction
    let jurisdiction_id = steward
        .jurisdiction
        .as_ref()
        .map(|j| JurisdictionId(j.clone()))
        .ok_or_else(|| {
            GatewayError::BadRequest(
                "Steward has no jurisdiction; cannot determine authority scope".to_string(),
            )
        })?;

    require_office_in_jurisdiction(&commons_manager, &caller_did, &jurisdiction_id).await?;

    let _steward = steward;

    match body.status.to_lowercase().as_str() {
        "suspend" | "suspended" => {
            let reason = body
                .reason
                .clone()
                .unwrap_or_else(|| "No reason provided".to_string());
            commons_manager
                .suspend_steward(&steward_id, reason)
                .await
                .map_err(|e| GatewayError::InternalError(e.to_string()))?;
        }
        "reinstate" | "active" => {
            commons_manager
                .reinstate_steward(&steward_id)
                .await
                .map_err(|e| GatewayError::InternalError(e.to_string()))?;
        }
        "retire" | "retired" => {
            commons_manager
                .retire_steward(&steward_id)
                .await
                .map_err(|e| GatewayError::InternalError(e.to_string()))?;
        }
        "revoke" | "revoked" => {
            let reason = body
                .reason
                .clone()
                .unwrap_or_else(|| "No reason provided".to_string());
            let evidence: Vec<[u8; 32]> = body
                .evidence
                .iter()
                .filter_map(|e| {
                    hex::decode(e).ok().and_then(|v| {
                        if v.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&v);
                            Some(arr)
                        } else {
                            None
                        }
                    })
                })
                .collect();
            commons_manager
                .revoke_steward(&steward_id, reason, evidence)
                .await
                .map_err(|e| GatewayError::InternalError(e.to_string()))?;
        }
        _ => {
            return Err(GatewayError::BadRequest(
                "Invalid status. Must be: suspend, reinstate, retire, or revoke".to_string(),
            ));
        }
    }

    // Return updated steward
    let updated = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(steward_to_detail(&updated)))
}

/// POST /v1/steward/{id}/retire - Retire (self-service)
#[post("/{steward_id}/retire")]
pub async fn retire_steward(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Verify caller is the steward
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    if steward.steward_did != caller_did && steward.holder_did != caller_did {
        return Err(GatewayError::AuthorizationFailed(
            "Can only retire your own stewardship".to_string(),
        ));
    }

    commons_manager
        .retire_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "retired",
        "steward_id": steward_id,
    })))
}

/// POST /v1/steward/{id}/extend-term - Extend steward term
#[post("/{steward_id}/extend-term")]
pub async fn extend_steward_term(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ExtendTermRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Verify steward exists and resolve jurisdiction for authority check
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    let jurisdiction_id = steward
        .jurisdiction
        .as_ref()
        .map(|j| JurisdictionId(j.clone()))
        .ok_or_else(|| {
            GatewayError::BadRequest(
                "Steward has no jurisdiction; cannot determine authority scope".to_string(),
            )
        })?;

    require_office_in_jurisdiction(&commons_manager, &caller_did, &jurisdiction_id).await?;

    // Calculate new term end
    let additional_secs = body.additional_days * 24 * 60 * 60;
    let new_term_end = steward.term_end + additional_secs;

    // Validate (max 2 years from now)
    let now = icn_time::current_timestamp_secs();
    let max_term = now + (730 * 24 * 60 * 60);
    if new_term_end > max_term {
        return Err(GatewayError::BadRequest(
            "Term cannot extend more than 2 years from now".to_string(),
        ));
    }

    commons_manager
        .extend_steward_term(&steward_id, new_term_end)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    // Return updated steward
    let updated = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(steward_to_detail(&updated)))
}

// ============================================================================
// Bond Management Endpoints
// ============================================================================

/// POST /v1/steward/{id}/bond/add - Add to steward's bond (self-service)
///
/// Only the steward's own holder may add to their bond. Increasing a bond is a
/// self-service economic commitment — governance (HoldOffice) is not required.
#[post("/{steward_id}/bond/add")]
pub async fn add_bond(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<BondOperationRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Verify steward exists and enforce self-service: caller must be this steward
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    if steward.holder_did != caller_did && steward.steward_did != caller_did {
        return Err(GatewayError::AuthorizationFailed(
            "Can only add bond to your own stewardship".to_string(),
        ));
    }

    let _steward = steward;

    commons_manager
        .add_steward_bond(&steward_id, body.amount)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    // Return updated steward
    let updated = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "bond_added",
        "steward_id": steward_id,
        "new_bond_amount": updated.bond_amount,
    })))
}

/// POST /v1/steward/{id}/bond/slash - Slash steward's bond (governance action)
#[post("/{steward_id}/bond/slash")]
pub async fn slash_bond(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<BondOperationRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Verify steward exists and get their jurisdiction
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    // Authorization: Caller must have HoldOffice capability in steward's jurisdiction
    let jurisdiction_id = steward
        .jurisdiction
        .as_ref()
        .map(|j| icn_identity::JurisdictionId(j.clone()))
        .ok_or_else(|| GatewayError::InternalError("Steward has no jurisdiction".to_string()))?;

    // Get caller's holder record
    let caller_holder = commons_manager
        .get_holder_by_did(&caller_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::AuthorizationFailed("Caller is not a commons holder".to_string())
        })?;

    // Check if caller has HoldOffice capability in the jurisdiction
    let holder_id_hex = hex::encode(caller_holder.holder_id);
    let has_authority = commons_manager
        .member_has_capability(
            &holder_id_hex,
            &jurisdiction_id,
            icn_identity::MembershipCapability::HoldOffice,
        )
        .await
        .unwrap_or(false);

    if !has_authority {
        return Err(GatewayError::AuthorizationFailed(
            "Caller does not have governance authority (HoldOffice capability required)"
                .to_string(),
        ));
    }

    let slashed = commons_manager
        .slash_steward_bond(&steward_id, body.amount)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    // Return updated steward
    let updated = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "bond_slashed",
        "steward_id": steward_id,
        "amount_slashed": slashed,
        "remaining_bond": updated.bond_amount,
    })))
}

// ============================================================================
// Attestation Tracking Endpoints
// ============================================================================

/// POST /v1/steward/{id}/attestation - Record an attestation issued (self-service)
///
/// A steward calls this to log that they issued an attestation. Only the
/// steward's own DID may record this — it is a self-report of work performed.
#[post("/{steward_id}/attestation")]
pub async fn record_attestation(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Self-service: only the steward may record their own attestation
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    if steward.steward_did != caller_did && steward.holder_did != caller_did {
        return Err(GatewayError::AuthorizationFailed(
            "Can only record attestations for your own stewardship".to_string(),
        ));
    }

    commons_manager
        .record_steward_attestation(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    // Return updated steward
    let updated = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "attestation_recorded",
        "steward_id": steward_id,
        "attestations_issued": updated.attestations_issued,
    })))
}

/// POST /v1/steward/{id}/dispute - Record a dispute against the steward
///
/// Dispute filing is a member-standing action: any full member of the steward's
/// jurisdiction may report a concern, but unenrolled outsiders cannot inflate a
/// steward's dispute count.  Requires `Vote` capability in the steward's
/// jurisdiction — does NOT require `HoldOffice`.
#[post("/{steward_id}/dispute")]
pub async fn record_dispute(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Resolve jurisdiction from steward record before mutating
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    let jurisdiction_id = steward
        .jurisdiction
        .as_ref()
        .map(|j| JurisdictionId(j.clone()))
        .ok_or_else(|| {
            GatewayError::BadRequest(
                "Steward has no jurisdiction; cannot verify member standing".to_string(),
            )
        })?;

    // Require member-standing in the steward's jurisdiction (Vote capability).
    // Outsiders cannot file disputes — only enrolled members with voting rights.
    require_membership_in_jurisdiction(&commons_manager, &caller_did, &jurisdiction_id).await?;

    commons_manager
        .record_steward_dispute(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    // Return updated steward
    let updated = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "dispute_recorded",
        "steward_id": steward_id,
        "disputes_against": updated.disputes_against,
        "reputation_score": updated.reputation_score,
    })))
}

/// POST /v1/steward/{id}/dispute-won - Record a dispute won by the steward
///
/// Recording a dispute outcome that positively affects a steward's reputation
/// is a governance action. Requires the caller to hold office in the steward's
/// jurisdiction to prevent reputation inflation by the steward themselves.
#[post("/{steward_id}/dispute-won")]
pub async fn record_dispute_won(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let steward_id = path.into_inner();

    // Resolve steward and their jurisdiction for authority check
    let steward = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    let jurisdiction_id = steward
        .jurisdiction
        .as_ref()
        .map(|j| JurisdictionId(j.clone()))
        .ok_or_else(|| {
            GatewayError::BadRequest(
                "Steward has no jurisdiction; cannot determine authority scope".to_string(),
            )
        })?;

    require_office_in_jurisdiction(&commons_manager, &caller_did, &jurisdiction_id).await?;

    commons_manager
        .record_steward_dispute_won(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;

    // Return updated steward
    let updated = commons_manager
        .get_steward(&steward_id)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| GatewayError::NotFound("Steward not found".to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "dispute_won_recorded",
        "steward_id": steward_id,
        "disputes_won": updated.disputes_won,
        "reputation_score": updated.reputation_score,
    })))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure steward routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(register_steward)
        .service(list_attesters) // Must come before {steward_id} routes
        .service(get_steward)
        .service(get_steward_by_did)
        .service(list_stewards)
        .service(update_steward_status)
        .service(retire_steward)
        .service(extend_steward_term)
        .service(add_bond)
        .service(slash_bond)
        .service(record_attestation)
        .service(record_dispute)
        .service(record_dispute_won);
}
