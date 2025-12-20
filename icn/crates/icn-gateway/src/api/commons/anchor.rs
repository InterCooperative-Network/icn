//! PersonhoodAnchor API endpoints
//!
//! Provides REST API for Layer 0 of Commons Evolution:
//! - Anchor lookup and detail retrieval
//! - POP attestation management
//! - Status updates (via governance)

use actix_web::{get, post, put, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use crate::middleware::get_claims;
use icn_identity::{AnchorStatus, Did, POPAttestation, POPLevel, POPMethod, PersonhoodAnchor};

// ============================================================================
// Response/Request DTOs
// ============================================================================

/// Anchor detail response
#[derive(Debug, Serialize, Deserialize)]
pub struct AnchorDetailResponse {
    pub anchor_id: String,
    pub did: String,
    pub status: String,
    pub pop_level: Option<String>,
    pub attestation_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
    pub current_key: Option<String>,
}

/// POP attestation response
#[derive(Debug, Serialize, Deserialize)]
pub struct AttestationResponse {
    pub attestation_id: String,
    pub issuer_did: String,
    pub method: String,
    pub level: String,
    pub issued_at: u64,
    pub expiry: Option<u64>,
    pub is_valid: bool,
}

/// Update anchor status request
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAnchorStatusRequest {
    pub status: String,
    pub reason: Option<String>,
}

/// Add attestation request
#[derive(Debug, Serialize, Deserialize)]
pub struct AddAttestationRequest {
    pub method: String,
    pub level: String,
    #[serde(default)]
    pub expiry_seconds: Option<u64>,
}

fn anchor_to_response(anchor: &PersonhoodAnchor) -> AnchorDetailResponse {
    AnchorDetailResponse {
        anchor_id: hex::encode(anchor.id()),
        did: anchor.to_did().to_string(),
        status: format!("{}", anchor.status),
        pop_level: anchor.pop_level().map(|l| l.to_string()),
        attestation_count: anchor.pop_attestations.len(),
        created_at: anchor.anchor.created_at,
        updated_at: anchor.updated_at,
        current_key: anchor.current_key.map(hex::encode),
    }
}

fn attestation_to_response(a: &POPAttestation) -> AttestationResponse {
    AttestationResponse {
        attestation_id: hex::encode(a.attestation_id),
        issuer_did: a.issuer_did.to_string(),
        method: format_pop_method(&a.method),
        level: a.level.to_string(),
        issued_at: a.issued_at,
        expiry: a.expiry,
        is_valid: a.is_valid(),
    }
}

fn format_pop_method(m: &POPMethod) -> String {
    match m {
        POPMethod::InPerson { .. } => "in_person".to_string(),
        POPMethod::Sponsored { .. } => "sponsored".to_string(),
        POPMethod::Biometric { .. } => "biometric".to_string(),
        POPMethod::Ceremony { .. } => "ceremony".to_string(),
        POPMethod::VideoCall { .. } => "video_call".to_string(),
        POPMethod::GovernmentId { .. } => "government_id".to_string(),
        POPMethod::WebOfTrust { .. } => "web_of_trust".to_string(),
        POPMethod::Genesis { .. } => "genesis".to_string(),
    }
}

// ============================================================================
// Anchor Lookup Endpoints
// ============================================================================

/// GET /v1/commons/anchor/{id} - Get anchor by ID
#[get("/{anchor_id}")]
pub async fn get_anchor_by_id(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let anchor_id = path.into_inner();

    let anchor = commons_manager
        .get_anchor(&anchor_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    Ok(HttpResponse::Ok().json(anchor_to_response(&anchor)))
}

/// GET /v1/commons/anchor/by-did/{did} - Get anchor by DID
#[get("/by-did/{did}")]
pub async fn get_anchor_by_did(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let did_str = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let anchor = commons_manager
        .get_anchor_by_did(&did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    Ok(HttpResponse::Ok().json(anchor_to_response(&anchor)))
}

// ============================================================================
// Attestation Endpoints
// ============================================================================

/// GET /v1/commons/anchor/{id}/attestations - List attestations
#[get("/{anchor_id}/attestations")]
pub async fn list_attestations(
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let anchor_id = path.into_inner();

    let anchor = commons_manager
        .get_anchor(&anchor_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    let attestations: Vec<AttestationResponse> = anchor
        .pop_attestations
        .iter()
        .map(attestation_to_response)
        .collect();

    Ok(HttpResponse::Ok().json(attestations))
}

/// POST /v1/commons/anchor/{id}/attestations - Add attestation (steward only)
#[post("/{anchor_id}/attestations")]
pub async fn add_attestation(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<AddAttestationRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // Verify authenticated user is a steward
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let steward_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Check if caller is an active steward and get their record
    let steward = commons_manager
        .get_steward_by_did(&steward_did)
        .await?
        .ok_or_else(|| {
            GatewayError::AuthorizationFailed(
                "Only registered stewards can add attestations".to_string(),
            )
        })?;

    if !steward.can_attest() {
        return Err(GatewayError::AuthorizationFailed(
            "Steward is not authorized to issue attestations (suspended, expired term, or low reputation)".to_string(),
        ));
    }

    let anchor_id = path.into_inner();

    // Get anchor to verify it exists
    let anchor = commons_manager
        .get_anchor(&anchor_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    // Parse level
    let level = POPLevel::parse(&body.level)
        .ok_or_else(|| GatewayError::BadRequest("Invalid POP level".to_string()))?;

    // Parse method
    let method = match body.method.to_lowercase().as_str() {
        "in_person" => POPMethod::InPerson {
            location_hash: None,
        },
        "sponsored" => POPMethod::Sponsored {
            sponsor_did: steward_did.clone(),
        },
        "ceremony" => POPMethod::Ceremony {
            witnesses: vec![steward_did.clone()],
            ceremony_type: "attestation".to_string(),
        },
        "video_call" => POPMethod::VideoCall {
            verifier_did: steward_did.clone(),
            recording_hash: None,
        },
        "web_of_trust" => POPMethod::WebOfTrust {
            vouchers: vec![steward_did.clone()],
        },
        _ => {
            return Err(GatewayError::BadRequest(
                "Invalid attestation method".to_string(),
            ))
        }
    };

    // Calculate expiry
    let expiry = body
        .expiry_seconds
        .map(|secs| icn_time::current_timestamp_secs() + secs);

    // Create attestation
    let attestation = POPAttestation::new(
        anchor.id(),
        steward_did,
        method,
        level,
        vec![], // Signature placeholder
        expiry,
    );

    // Add attestation to anchor
    commons_manager
        .add_attestation(&anchor_id, attestation.clone())
        .await?;

    // Record attestation on steward's record (for reputation tracking)
    let steward_id = steward.steward_id.to_hex();
    let _ = commons_manager
        .record_steward_attestation(&steward_id)
        .await;

    Ok(HttpResponse::Created().json(attestation_to_response(&attestation)))
}

// ============================================================================
// Status Update Endpoints
// ============================================================================

/// PUT /v1/commons/anchor/{id}/status - Update anchor status (governance only)
#[put("/{anchor_id}/status")]
pub async fn update_anchor_status(
    http_req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateAnchorStatusRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    // This would typically require governance authority
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let authority_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let anchor_id = path.into_inner();

    // Verify anchor exists
    let _anchor = commons_manager
        .get_anchor(&anchor_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    // Parse status
    let status = match body.status.to_lowercase().as_str() {
        "active" => AnchorStatus::Active,
        "suspended" => {
            let reason = body
                .reason
                .clone()
                .unwrap_or_else(|| "No reason provided".to_string());
            let now = icn_time::current_timestamp_secs();
            AnchorStatus::Suspended {
                reason,
                suspended_at: now,
                until: None,
                authority: authority_did,
            }
        }
        "revoked" => {
            let reason = body
                .reason
                .clone()
                .unwrap_or_else(|| "No reason provided".to_string());
            let now = icn_time::current_timestamp_secs();
            AnchorStatus::Revoked {
                reason,
                revoked_at: now,
                evidence: vec![],
                authority: authority_did,
            }
        }
        _ => {
            return Err(GatewayError::BadRequest(
                "Invalid status. Must be: active, suspended, or revoked".to_string(),
            ))
        }
    };

    commons_manager
        .update_anchor_status(&anchor_id, status)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "updated",
        "anchor_id": anchor_id,
        "new_status": body.status,
    })))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure anchor routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_anchor_by_id)
        .service(get_anchor_by_did)
        .service(list_attestations)
        .service(add_attestation)
        .service(update_anchor_status);
}
