//! Community API endpoints
//!
//! REST API for the ICN Community (Civic Engine).
//! Communities are non-economic civic organizations that can include
//! both individuals and cooperatives as members.
//!
//! # Security Note
//!
//! Current implementation uses scope-based access control (community:read, community:write,
//! community:admin). Community-scoped authorization (verifying membership/admin status for
//! specific communities) is planned for Phase 2 governance integration.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use icn_community::{CommunityType, MemberType};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::community_mgr::CommunityManager;
use crate::error::{GatewayError, Result};
use crate::middleware::require_scope;

/// Extract the CommunityManager from optional app data
fn get_community_mgr(
    mgr: &web::Data<Option<Arc<CommunityManager>>>,
) -> Result<&Arc<CommunityManager>> {
    mgr.as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Community service not configured".into()))
}

/// Request to create a new community
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateCommunityRequest {
    /// Optional explicit ID (auto-generated if not provided)
    pub id: Option<String>,
    /// Community name
    pub name: String,
    /// Community type
    pub community_type: String,
    /// Founder type: "individual" (default) or "cooperative"
    pub founder_type: Option<String>,
    /// Charter content (CCL)
    #[serde(default)]
    pub charter: String,
}

/// Request to join a community
#[derive(Debug, Deserialize, Serialize)]
pub struct JoinCommunityRequest {
    /// Member ID (DID or cooperative ID)
    pub member_id: String,
    /// Member type: "individual" or "cooperative"
    pub member_type: String,
}

/// Request to allocate resources
#[derive(Debug, Deserialize, Serialize)]
pub struct AllocateResourceRequest {
    /// Pool name
    pub pool_name: String,
    /// Recipient ID
    pub recipient: String,
    /// Amount to allocate
    pub amount: u64,
}

/// Request to update community
#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateCommunityRequest {
    /// New name (optional)
    pub name: Option<String>,
    /// New metadata (optional)
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

fn parse_community_type(type_str: &str) -> Result<CommunityType> {
    match type_str.to_lowercase().as_str() {
        "geographic" => Ok(CommunityType::Geographic),
        "interest" => Ok(CommunityType::Interest),
        "solidarity" => Ok(CommunityType::Solidarity),
        "ecosystem" => Ok(CommunityType::Ecosystem),
        _ => Err(crate::error::GatewayError::BadRequest(format!(
            "Invalid community type: {type_str}. Valid types: geographic, interest, solidarity, ecosystem"
        ))),
    }
}

fn parse_member_type(type_str: &str, member_id: &str) -> Result<MemberType> {
    match type_str.to_lowercase().as_str() {
        "individual" => Ok(MemberType::Individual(member_id.to_string())),
        "cooperative" => Ok(MemberType::Cooperative(member_id.to_string())),
        _ => Err(crate::error::GatewayError::BadRequest(format!(
            "Invalid member type: {type_str}. Valid types: individual, cooperative"
        ))),
    }
}

/// POST /communities - Create a new community
#[post("")]
pub async fn create_community(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    req: web::Json<CreateCommunityRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:write")?;
    let mgr = get_community_mgr(&community_mgr)?;

    // Get founder from authenticated token
    use crate::middleware::get_claims;
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let founder_id = claims.sub.clone();
    let community_type = parse_community_type(&req.community_type)?;

    // Parse founder type (defaults to Individual if not specified)
    let founder_type = match req.founder_type.as_deref() {
        Some("cooperative") => MemberType::Cooperative(founder_id.clone()),
        _ => MemberType::Individual(founder_id.clone()),
    };

    let community = mgr
        .create_community(
            req.id.clone(),
            req.name.clone(),
            community_type,
            founder_id,
            founder_type,
            req.charter.clone(),
        )
        .await?;

    Ok(HttpResponse::Created().json(community))
}

/// GET /communities - List all communities
#[get("")]
pub async fn list_communities(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:read")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let communities = mgr.list_communities().await?;
    Ok(HttpResponse::Ok().json(communities))
}

/// GET /communities/{id} - Get community by ID
#[get("/{id}")]
pub async fn get_community(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:read")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let community = mgr.get_community(&id).await?;
    Ok(HttpResponse::Ok().json(community))
}

/// PUT /communities/{id} - Update community
#[put("/{id}")]
pub async fn update_community(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
    req: web::Json<UpdateCommunityRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:admin")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let community = mgr
        .update_community(&id, req.name.clone(), req.metadata.clone())
        .await?;

    Ok(HttpResponse::Ok().json(community))
}

/// POST /communities/{id}/activate - Activate a forming community
#[post("/{id}/activate")]
pub async fn activate_community(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:admin")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let community = mgr.activate_community(&id).await?;
    Ok(HttpResponse::Ok().json(community))
}

/// DELETE /communities/{id} - Dissolve a community
#[delete("/{id}")]
pub async fn dissolve_community(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:admin")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let community = mgr.dissolve_community(&id).await?;
    Ok(HttpResponse::Ok().json(community))
}

/// POST /communities/{id}/members - Join a community
///
/// # Authorization
///
/// Users can only join themselves to a community. To join someone else,
/// the `community:admin` scope is required to prevent unauthorized proxy joins.
#[post("/{id}/members")]
pub async fn join_community(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
    req: web::Json<JoinCommunityRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:write")?;
    let mgr = get_community_mgr(&community_mgr)?;

    // Get authenticated user's DID
    use crate::middleware::{get_claims, has_scope};
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // Security: Only allow joining yourself unless you have admin scope
    // This prevents unauthorized proxy joins (attacker joining victim to communities)
    if claims.sub != req.member_id && !has_scope(&http_req, "community:admin") {
        return Err(GatewayError::AuthorizationFailed(
            "Cannot join on behalf of others without community:admin scope".to_string(),
        ));
    }

    let member_type = parse_member_type(&req.member_type, &req.member_id)?;

    let community = mgr
        .join_community(&id, req.member_id.clone(), member_type)
        .await?;

    Ok(HttpResponse::Ok().json(community))
}

/// DELETE /communities/{id}/members/{member_id} - Leave a community
///
/// # Authorization
///
/// Users can only remove themselves from a community. To remove someone else,
/// the `community:admin` scope is required.
#[delete("/{id}/members/{member_id}")]
pub async fn leave_community(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:write")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let (community_id, member_id) = path.into_inner();

    // Get authenticated user's DID
    use crate::middleware::{get_claims, has_scope};
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // Security: Only allow leaving yourself unless you have admin scope
    // This prevents unauthorized removal of members by malicious actors
    if claims.sub != member_id && !has_scope(&http_req, "community:admin") {
        return Err(GatewayError::AuthorizationFailed(
            "Cannot remove others from community without community:admin scope".to_string(),
        ));
    }

    let community = mgr.leave_community(&community_id, &member_id).await?;

    Ok(HttpResponse::Ok().json(community))
}

/// GET /communities/{id}/members - List community members
#[get("/{id}/members")]
pub async fn list_members(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:read")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let members = mgr.list_members(&id).await?;
    Ok(HttpResponse::Ok().json(members))
}

/// POST /communities/{id}/resources - Allocate resource from pool
#[post("/{id}/resources")]
pub async fn allocate_resource(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
    req: web::Json<AllocateResourceRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:admin")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let community = mgr
        .allocate_resource(
            &id,
            req.pool_name.clone(),
            req.recipient.clone(),
            req.amount,
        )
        .await?;

    Ok(HttpResponse::Ok().json(community))
}

/// GET /communities/{id}/resources - Get resource pools
#[get("/{id}/resources")]
pub async fn get_resource_pools(
    http_req: HttpRequest,
    community_mgr: web::Data<Option<Arc<CommunityManager>>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "community:read")?;
    let mgr = get_community_mgr(&community_mgr)?;

    let pools = mgr.get_resource_pools(&id).await?;
    Ok(HttpResponse::Ok().json(pools))
}

/// Configure community routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/communities")
            .service(create_community)
            .service(list_communities)
            .service(get_community)
            .service(update_community)
            .service(activate_community)
            .service(dissolve_community)
            .service(join_community)
            .service(leave_community)
            .service(list_members)
            .service(allocate_resource)
            .service(get_resource_pools),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_community_type() {
        assert!(parse_community_type("geographic").is_ok());
        assert!(parse_community_type("INTEREST").is_ok());
        assert!(parse_community_type("Solidarity").is_ok());
        assert!(parse_community_type("ecosystem").is_ok());
        assert!(parse_community_type("invalid").is_err());
    }

    #[test]
    fn test_parse_member_type() {
        let member = parse_member_type("individual", "did:icn:test").unwrap();
        assert!(matches!(member, MemberType::Individual(_)));

        let member = parse_member_type("cooperative", "coop-123").unwrap();
        assert!(matches!(member, MemberType::Cooperative(_)));

        assert!(parse_member_type("invalid", "test").is_err());
    }
}
