//! Invite API endpoints
//!
//! RESTful API for managing cooperative invitations.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::auth::AuthManager;
use crate::commons_mgr::CommonsManager;
use crate::error::Result;
use crate::invite::InviteManager;
use crate::middleware::{get_claims, require_scope};
use crate::models::{
    CreateInviteRequest, InviteInfo, InviteListResponse, InviteResponse, JoinRequest, JoinResponse,
};
use crate::validation;
use icn_identity::Did;
use icn_obs::metrics::gateway;

// ============================================================================
// Invite Endpoints
// ============================================================================

/// POST /invites - Create a new invite code
#[post("")]
pub async fn create_invite(
    http_req: HttpRequest,
    invite_mgr: web::Data<Arc<InviteManager>>,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    req: web::Json<CreateInviteRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let creator_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate inputs
    validation::validate_coop_id(&req.coop_id)?;
    validation::validate_role(&req.role)?;

    // Default to 7 days if not specified
    let expires_in = req.expires_in_seconds.unwrap_or(7 * 24 * 3600);

    // Validate expiration (max 30 days)
    if expires_in > 30 * 24 * 3600 {
        return Err(crate::error::GatewayError::BadRequest(
            "Expiration cannot exceed 30 days".to_string(),
        ));
    }

    // Create invite
    let invite = invite_mgr
        .create_invite(
            req.coop_id.clone(),
            req.role.clone(),
            creator_did,
            expires_in,
        )
        .await
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to create invite: {e}"))
        })?;

    // Get coop name from charter
    let coop_name = match commons_mgr.get_charter_by_domain(&req.coop_id).await {
        Ok(Some(charter)) => charter.name,
        _ => format!("Coop {}", req.coop_id), // Fallback if charter not found
    };

    // Construct invite URL
    let base_url =
        std::env::var("GATEWAY_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let invite_url = format!("{}/join?code={}", base_url, invite.code);

    // Record metrics
    gateway::invites_created(&req.coop_id);

    Ok(HttpResponse::Created().json(InviteResponse {
        code: invite.code,
        coop_id: invite.coop_id,
        coop_name,
        role: invite.role,
        expires_at: invite.expires_at,
        invite_url,
    }))
}

/// GET /invites - List all invites for a cooperative
#[get("")]
pub async fn list_invites(
    http_req: HttpRequest,
    invite_mgr: web::Data<Arc<InviteManager>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:read")?;

    // Get coop_id from query params
    let coop_id = query.get("coop_id").ok_or_else(|| {
        crate::error::GatewayError::BadRequest("Missing coop_id parameter".to_string())
    })?;

    validation::validate_coop_id(coop_id)?;

    // List invites
    let invites = invite_mgr.list_invites(coop_id).await.map_err(|e| {
        crate::error::GatewayError::InternalError(format!("Failed to list invites: {e}"))
    })?;

    // Convert to response format
    let invite_infos: Vec<InviteInfo> = invites
        .into_iter()
        .map(|i| InviteInfo {
            code: i.code,
            role: i.role,
            created_by: i.created_by.to_string(),
            created_at: i.created_at,
            expires_at: i.expires_at,
            used: i.used,
        })
        .collect();

    Ok(HttpResponse::Ok().json(InviteListResponse {
        invites: invite_infos,
    }))
}

/// POST /invites/join - Join a cooperative via invite code
#[post("/join")]
pub async fn join_via_invite(
    invite_mgr: web::Data<Arc<InviteManager>>,
    auth_mgr: web::Data<Arc<AuthManager>>,
    req: web::Json<JoinRequest>,
) -> Result<HttpResponse> {
    // Validate invite code
    let invite = invite_mgr
        .validate_invite(&req.invite_code)
        .await
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid invite: {e}")))?;

    // Validate the provided DID
    let did: Did = req
        .did
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Mark invite as used
    invite_mgr
        .mark_used(&req.invite_code, did.clone())
        .await
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to mark invite as used: {e}"))
        })?;

    // Generate capability token
    let scopes = vec![
        "coop:read".to_string(),
        "coop:write".to_string(),
        "ledger:read".to_string(),
        "ledger:transact".to_string(),
    ];

    let token = auth_mgr
        .issue_token(&did, &invite.coop_id, scopes)
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to generate token: {e}"))
        })?;

    // Record metrics
    gateway::invites_used(&invite.coop_id);

    Ok(HttpResponse::Ok().json(JoinResponse {
        did: did.to_string(),
        token,
        token_expires_in: 86400, // 24 hours
        coop_id: invite.coop_id,
        role: invite.role,
        private_key: String::new(), // Client generates their own keypair
    }))
}
