//! Membership API endpoints
//!
//! Provides REST API for membership management:
//! - Apply for membership in a jurisdiction
//! - Approve/reject membership applications
//! - Promote members (provisional -> full)
//! - Suspend/reinstate members
//! - Grant/revoke capabilities
//! - Ban members (with revocation tracking)

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use crate::middleware::get_claims;
use icn_identity::{
    CommonsRevocationReason, Did, JurisdictionId, MembershipCapability, MembershipStatus,
};

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Apply for membership request
#[derive(Debug, Serialize, Deserialize)]
pub struct ApplyMembershipRequest {
    pub jurisdiction_id: String,
    #[serde(default)]
    pub capabilities_requested: Vec<String>,
}

/// Membership action request (approve, promote, suspend, etc.)
#[derive(Debug, Serialize, Deserialize)]
pub struct MembershipActionRequest {
    pub holder_id: String,
    pub jurisdiction_id: String,
}

/// Grant/revoke capability request
#[derive(Debug, Serialize, Deserialize)]
pub struct CapabilityRequest {
    pub holder_id: String,
    pub jurisdiction_id: String,
    pub capability: String,
}

/// Role request
#[derive(Debug, Serialize, Deserialize)]
pub struct RoleRequest {
    pub holder_id: String,
    pub jurisdiction_id: String,
    pub role: String,
}

/// Ban member request
#[derive(Debug, Serialize, Deserialize)]
pub struct BanMemberRequest {
    pub holder_id: String,
    pub jurisdiction_id: String,
    pub reason: String,
    #[serde(default)]
    pub evidence_summary: Option<String>,
}

/// Revoke membership request (with appeal window)
#[derive(Debug, Serialize, Deserialize)]
pub struct RevokeMembershipRequest {
    pub holder_id: String,
    pub jurisdiction_id: String,
    pub reason: String,
    #[serde(default = "default_appeal_days")]
    pub appeal_window_days: u64,
    #[serde(default)]
    pub policy_ref: Option<String>,
}

fn default_appeal_days() -> u64 {
    30
}

/// Member response
#[derive(Debug, Serialize, Deserialize)]
pub struct MemberResponse {
    pub holder_id: String,
    pub holder_did: String,
    pub jurisdiction_id: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub roles: Vec<String>,
    pub joined_at: u64,
    pub expires_at: Option<u64>,
}

/// Membership list response
#[derive(Debug, Serialize, Deserialize)]
pub struct MemberListResponse {
    pub members: Vec<MemberResponse>,
    pub total: usize,
}

fn parse_capability(s: &str) -> Option<MembershipCapability> {
    match s.to_lowercase().as_str() {
        "vote" => Some(MembershipCapability::Vote),
        "propose" => Some(MembershipCapability::Propose),
        "transact" => Some(MembershipCapability::Transact),
        "holdoffice" | "hold_office" => Some(MembershipCapability::HoldOffice),
        "accessprivate" | "access_private" => Some(MembershipCapability::AccessPrivate),
        "sponsor" => Some(MembershipCapability::Sponsor),
        _ => None,
    }
}

fn format_capability(cap: &MembershipCapability) -> String {
    match cap {
        MembershipCapability::Vote => "vote".to_string(),
        MembershipCapability::Propose => "propose".to_string(),
        MembershipCapability::Transact => "transact".to_string(),
        MembershipCapability::HoldOffice => "hold_office".to_string(),
        MembershipCapability::AccessPrivate => "access_private".to_string(),
        MembershipCapability::Sponsor => "sponsor".to_string(),
    }
}

// ============================================================================
// Membership Application Endpoints
// ============================================================================

/// POST /v1/membership/apply - Apply for membership
#[post("/apply")]
pub async fn apply_for_membership(
    http_req: HttpRequest,
    body: web::Json<ApplyMembershipRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let applicant_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Get holder ID from DID
    let holder = commons_manager
        .get_holder_by_did(&applicant_did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Commons holder not found".to_string()))?;

    let holder_id = hex::encode(holder.id());
    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    // Parse requested capabilities
    let capabilities: Vec<MembershipCapability> = body
        .capabilities_requested
        .iter()
        .filter_map(|s| parse_capability(s))
        .collect();

    let affiliation = commons_manager
        .apply_for_membership(&holder_id, jurisdiction_id.clone(), capabilities)
        .await?;

    Ok(HttpResponse::Created().json(MemberResponse {
        holder_id,
        holder_did: applicant_did.to_string(),
        jurisdiction_id: jurisdiction_id.to_string(),
        status: format!("{}", affiliation.membership_status),
        capabilities: affiliation
            .capabilities
            .iter()
            .map(format_capability)
            .collect(),
        roles: affiliation.roles.clone(),
        joined_at: affiliation.joined_at,
        expires_at: affiliation.expires_at,
    }))
}

/// GET /v1/membership/status/{jurisdiction} - Get membership status
#[get("/status/{jurisdiction}")]
pub async fn get_membership_status(
    http_req: HttpRequest,
    path: web::Path<String>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let user_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let holder = commons_manager
        .get_holder_by_did(&user_did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Commons holder not found".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(path.into_inner());
    let affiliation = holder
        .get_affiliation(&jurisdiction_id)
        .ok_or_else(|| GatewayError::NotFound(format!("Not a member of {jurisdiction_id}")))?;

    Ok(HttpResponse::Ok().json(MemberResponse {
        holder_id: hex::encode(holder.id()),
        holder_did: user_did.to_string(),
        jurisdiction_id: jurisdiction_id.to_string(),
        status: format!("{}", affiliation.membership_status),
        capabilities: affiliation
            .capabilities
            .iter()
            .map(format_capability)
            .collect(),
        roles: affiliation.roles.clone(),
        joined_at: affiliation.joined_at,
        expires_at: affiliation.expires_at,
    }))
}

// ============================================================================
// Admin Membership Management Endpoints
// ============================================================================

/// POST /v1/membership/approve - Approve membership application
#[post("/approve")]
pub async fn approve_membership(
    http_req: HttpRequest,
    body: web::Json<MembershipActionRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    // Authorization: Caller must have HoldOffice capability in the jurisdiction
    let caller_holder = commons_manager
        .get_holder_by_did(&caller_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::AuthorizationFailed("Caller is not a commons holder".to_string())
        })?;

    let holder_id_hex = hex::encode(caller_holder.holder_id);
    let has_authority = commons_manager
        .member_has_capability(
            &holder_id_hex,
            &jurisdiction_id,
            MembershipCapability::HoldOffice,
        )
        .await
        .unwrap_or(false);

    if !has_authority {
        return Err(GatewayError::AuthorizationFailed(
            "Caller does not have admin rights (HoldOffice capability required)".to_string(),
        ));
    }

    commons_manager
        .approve_membership(&body.holder_id, &jurisdiction_id)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "approved",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "new_status": "Provisional",
    })))
}

/// POST /v1/membership/promote - Promote to full member
#[post("/promote")]
pub async fn promote_member(
    http_req: HttpRequest,
    body: web::Json<MembershipActionRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    commons_manager
        .promote_member(&body.holder_id, &jurisdiction_id)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "promoted",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "new_status": "Member",
    })))
}

/// POST /v1/membership/suspend - Suspend a member
#[post("/suspend")]
pub async fn suspend_member(
    http_req: HttpRequest,
    body: web::Json<MembershipActionRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    commons_manager
        .suspend_member(&body.holder_id, &jurisdiction_id)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "suspended",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "new_status": "Suspended",
    })))
}

/// POST /v1/membership/reinstate - Reinstate a suspended member
#[post("/reinstate")]
pub async fn reinstate_member(
    http_req: HttpRequest,
    body: web::Json<MembershipActionRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    commons_manager
        .reinstate_member(&body.holder_id, &jurisdiction_id)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "reinstated",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "new_status": "Member",
    })))
}

/// POST /v1/membership/ban - Ban a member (immediate, severe)
#[post("/ban")]
pub async fn ban_member(
    http_req: HttpRequest,
    body: web::Json<BanMemberRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let authority = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    let reason = if let Some(summary) = &body.evidence_summary {
        CommonsRevocationReason::PolicyViolation {
            policy_ref: body.reason.clone(),
            details: summary.clone(),
        }
    } else {
        CommonsRevocationReason::Other {
            description: body.reason.clone(),
        }
    };

    let revocation = commons_manager
        .ban_member(&body.holder_id, &jurisdiction_id, authority, reason)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "banned",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "revocation_id": revocation.id_hex(),
        "effective_immediately": true,
    })))
}

/// POST /v1/membership/revoke - Revoke membership (with appeal window)
#[post("/revoke")]
pub async fn revoke_membership(
    http_req: HttpRequest,
    body: web::Json<RevokeMembershipRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let authority = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    let reason = if let Some(policy) = &body.policy_ref {
        CommonsRevocationReason::PolicyViolation {
            policy_ref: policy.clone(),
            details: body.reason.clone(),
        }
    } else {
        CommonsRevocationReason::Other {
            description: body.reason.clone(),
        }
    };

    let revocation = commons_manager
        .revoke_membership(
            &body.holder_id,
            &jurisdiction_id,
            authority,
            reason,
            body.appeal_window_days,
        )
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "revoked",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "revocation_id": revocation.id_hex(),
        "appeal_deadline": revocation.appeal_deadline,
        "effective_at": revocation.effective_at,
    })))
}

// ============================================================================
// Capability Management Endpoints
// ============================================================================

/// POST /v1/membership/capability/grant - Grant a capability
#[post("/capability/grant")]
pub async fn grant_capability(
    http_req: HttpRequest,
    body: web::Json<CapabilityRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);
    let capability = parse_capability(&body.capability)
        .ok_or_else(|| GatewayError::BadRequest("Invalid capability".to_string()))?;

    commons_manager
        .grant_capability(&body.holder_id, &jurisdiction_id, capability)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "granted",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "capability": body.capability,
    })))
}

/// POST /v1/membership/capability/revoke - Revoke a capability
#[post("/capability/revoke")]
pub async fn revoke_capability(
    http_req: HttpRequest,
    body: web::Json<CapabilityRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);
    let capability = parse_capability(&body.capability)
        .ok_or_else(|| GatewayError::BadRequest("Invalid capability".to_string()))?;

    commons_manager
        .revoke_capability(&body.holder_id, &jurisdiction_id, capability)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "revoked",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "capability": body.capability,
    })))
}

/// GET /v1/membership/capability/check - Check if member has capability
#[get("/capability/check")]
pub async fn check_capability(
    http_req: HttpRequest,
    query: web::Query<CapabilityRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&query.jurisdiction_id);
    let capability = parse_capability(&query.capability)
        .ok_or_else(|| GatewayError::BadRequest("Invalid capability".to_string()))?;

    let has_capability = commons_manager
        .member_has_capability(&query.holder_id, &jurisdiction_id, capability)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "holder_id": query.holder_id,
        "jurisdiction_id": query.jurisdiction_id,
        "capability": query.capability,
        "has_capability": has_capability,
    })))
}

// ============================================================================
// Role Management Endpoints
// ============================================================================

/// POST /v1/membership/role/add - Add a role
#[post("/role/add")]
pub async fn add_role(
    http_req: HttpRequest,
    body: web::Json<RoleRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    commons_manager
        .add_member_role(&body.holder_id, &jurisdiction_id, body.role.clone())
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "added",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "role": body.role,
    })))
}

/// POST /v1/membership/role/remove - Remove a role
#[post("/role/remove")]
pub async fn remove_role(
    http_req: HttpRequest,
    body: web::Json<RoleRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    commons_manager
        .remove_member_role(&body.holder_id, &jurisdiction_id, &body.role)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "removed",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
        "role": body.role,
    })))
}

// ============================================================================
// Member Listing Endpoints
// ============================================================================

/// GET /v1/membership/list/{jurisdiction} - List members of a jurisdiction
#[get("/list/{jurisdiction}")]
pub async fn list_members(
    http_req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<ListMembersQuery>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let jurisdiction_id = JurisdictionId::new(path.into_inner());

    let status_filter = query
        .status
        .as_ref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "candidate" => Some(MembershipStatus::Candidate),
            "provisional" => Some(MembershipStatus::Provisional),
            "member" => Some(MembershipStatus::Member),
            "suspended" => Some(MembershipStatus::Suspended),
            "exited" => Some(MembershipStatus::Exited),
            "banned" => Some(MembershipStatus::Banned),
            _ => None,
        });

    let members = commons_manager
        .list_members_by_status(&jurisdiction_id, status_filter)
        .await;

    let member_responses: Vec<MemberResponse> = members
        .into_iter()
        .map(|(holder_id, affiliation)| MemberResponse {
            holder_id,
            holder_did: String::new(), // Would need to look up
            jurisdiction_id: jurisdiction_id.to_string(),
            status: format!("{}", affiliation.membership_status),
            capabilities: affiliation
                .capabilities
                .iter()
                .map(format_capability)
                .collect(),
            roles: affiliation.roles,
            joined_at: affiliation.joined_at,
            expires_at: affiliation.expires_at,
        })
        .collect();

    let total = member_responses.len();

    Ok(HttpResponse::Ok().json(MemberListResponse {
        members: member_responses,
        total,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ListMembersQuery {
    pub status: Option<String>,
}

/// POST /v1/membership/exit - Voluntarily exit membership
#[post("/exit")]
pub async fn exit_membership(
    http_req: HttpRequest,
    body: web::Json<MembershipActionRequest>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let user_did = claims
        .sub
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Verify the holder matches the authenticated user
    let holder = commons_manager
        .get_holder_by_did(&user_did)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Commons holder not found".to_string()))?;

    let holder_id = hex::encode(holder.id());
    if holder_id != body.holder_id {
        return Err(GatewayError::AuthorizationFailed(
            "Can only exit your own membership".to_string(),
        ));
    }

    let jurisdiction_id = JurisdictionId::new(&body.jurisdiction_id);

    commons_manager
        .leave_jurisdiction(&holder_id, &jurisdiction_id)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "exited",
        "holder_id": body.holder_id,
        "jurisdiction_id": body.jurisdiction_id,
    })))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure membership routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(apply_for_membership)
        .service(get_membership_status)
        .service(approve_membership)
        .service(promote_member)
        .service(suspend_member)
        .service(reinstate_member)
        .service(ban_member)
        .service(revoke_membership)
        .service(grant_capability)
        .service(revoke_capability)
        .service(check_capability)
        .service(add_role)
        .service(remove_role)
        .service(list_members)
        .service(exit_membership);
}
