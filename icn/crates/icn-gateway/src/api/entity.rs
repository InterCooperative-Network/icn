//! Entity API endpoints
//!
//! Provides REST API for entity management including:
//! - Entity CRUD operations (cooperatives, federations)
//! - Membership management
//!
//! Entities are organizational units in the ICN that can own treasuries,
//! have governance domains, and contain members (individuals or other entities).

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use icn_entity::{CooperativeEntity, EntityId, EntityType, Membership, MembershipRole};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::entity_mgr::EntityManager;
use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to register a new entity
#[derive(Debug, Deserialize)]
pub struct RegisterEntityRequest {
    /// Entity type (cooperative, federation)
    pub entity_type: String,
    /// Unique identifier for the entity
    pub identifier: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional parent entity ID (for federations)
    pub parent_id: Option<String>,
}

/// Request to update an entity
#[derive(Debug, Deserialize)]
pub struct UpdateEntityRequest {
    /// Updated name (optional)
    pub name: Option<String>,
    /// Updated description (optional)
    pub description: Option<String>,
}

/// Request to add a membership
#[derive(Debug, Deserialize)]
pub struct AddMembershipRequest {
    /// Member entity ID (individual DID or entity ID)
    pub member_id: String,
    /// Role for the member
    pub role: String,
    /// Initial shares (for weighted voting)
    pub shares: Option<u64>,
}

/// Entity summary response
#[derive(Debug, Serialize)]
pub struct EntityResponse {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    pub status: String,
    pub parent_id: Option<String>,
    pub created_at: u64,
    pub member_count: usize,
}

/// Membership summary response
#[derive(Debug, Serialize)]
pub struct MembershipResponse {
    pub member_id: String,
    pub parent_id: String,
    pub role: String,
    pub status: String,
    pub shares: u64,
    pub joined_at: u64,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_entity_type(type_str: &str) -> Result<EntityType> {
    match type_str.to_lowercase().as_str() {
        "cooperative" | "coop" => Ok(EntityType::Cooperative),
        "federation" | "fed" => Ok(EntityType::Federation),
        "individual" => Ok(EntityType::Individual),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid entity type: {type_str}. Valid types: cooperative, federation"
        ))),
    }
}

fn parse_role(role_str: &str) -> Result<MembershipRole> {
    // Split on colon first to preserve title case
    let parts: Vec<&str> = role_str.splitn(2, ':').collect();
    let role_type = parts[0].to_lowercase();

    match role_type.as_str() {
        "founder" => Ok(MembershipRole::Founder),
        "member" => Ok(MembershipRole::Member),
        "worker" => Ok(MembershipRole::Worker),
        "consumer" => Ok(MembershipRole::Consumer),
        "producer" => Ok(MembershipRole::Producer),
        "board_member" | "board" => Ok(MembershipRole::BoardMember),
        "officer" => {
            // Support "officer:President" or just "officer" (defaults to "Officer")
            // Preserve original case of title
            let title = parts
                .get(1)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Officer".to_string());
            Ok(MembershipRole::Officer { title })
        }
        "federated_member" | "federated" => Ok(MembershipRole::FederatedMember),
        "associate_member" | "associate" => Ok(MembershipRole::AssociateMember),
        "observer_member" | "observer" => Ok(MembershipRole::ObserverMember),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid role: {role_str}. Valid roles: founder, member, worker, consumer, producer, \
             board_member, officer[:title], federated_member, associate_member, observer_member"
        ))),
    }
}

/// Format entity type for API response (cleaner than Debug)
fn format_entity_type(entity_type: &EntityType) -> &'static str {
    match entity_type {
        EntityType::Individual => "individual",
        EntityType::Cooperative => "cooperative",
        EntityType::Federation => "federation",
        EntityType::Unknown => "unknown",
    }
}

/// Format entity status for API response (cleaner than Debug)
fn format_entity_status(status: &icn_entity::EntityStatus) -> String {
    use icn_entity::EntityStatus;
    match status {
        EntityStatus::Forming => "forming".to_string(),
        EntityStatus::Active => "active".to_string(),
        EntityStatus::Suspended { reason, .. } => format!("suspended:{reason}"),
        EntityStatus::Dissolving { .. } => "dissolving".to_string(),
        EntityStatus::Dissolved { .. } => "dissolved".to_string(),
        EntityStatus::Merged { into, .. } => format!("merged:{into}"),
        EntityStatus::Split { .. } => "split".to_string(),
    }
}

/// Format membership role for API response (cleaner than Debug)
fn format_role(role: &MembershipRole) -> String {
    match role {
        MembershipRole::Founder => "founder".to_string(),
        MembershipRole::Member => "member".to_string(),
        MembershipRole::Worker => "worker".to_string(),
        MembershipRole::Consumer => "consumer".to_string(),
        MembershipRole::Producer => "producer".to_string(),
        MembershipRole::BoardMember => "board_member".to_string(),
        MembershipRole::Officer { title } => format!("officer:{title}"),
        MembershipRole::FederatedMember => "federated_member".to_string(),
        MembershipRole::AssociateMember => "associate_member".to_string(),
        MembershipRole::ObserverMember => "observer_member".to_string(),
        MembershipRole::ProvisionalMember => "provisional_member".to_string(),
        MembershipRole::Custom { name } => format!("custom:{name}"),
    }
}

/// Format membership status for API response (cleaner than Debug)
fn format_membership_status(status: &icn_entity::MembershipStatus) -> &'static str {
    use icn_entity::MembershipStatus;
    match status {
        MembershipStatus::Pending => "pending",
        MembershipStatus::Active => "active",
        MembershipStatus::Suspended => "suspended",
        MembershipStatus::Inactive => "inactive",
        MembershipStatus::Removed => "removed",
        MembershipStatus::Resigned => "resigned",
        MembershipStatus::Expelled => "expelled",
    }
}

fn entity_to_response(entity: &CooperativeEntity, member_count: usize) -> EntityResponse {
    EntityResponse {
        id: entity.id.to_string(),
        entity_type: format_entity_type(&entity.entity_type).to_string(),
        name: entity.name.clone(),
        status: format_entity_status(&entity.status),
        parent_id: entity.parent_id.as_ref().map(|p| p.to_string()),
        created_at: entity.created_at,
        member_count,
    }
}

fn membership_to_response(m: &Membership) -> MembershipResponse {
    MembershipResponse {
        member_id: m.member_id.to_string(),
        parent_id: m.parent_id.to_string(),
        role: format_role(&m.role),
        status: format_membership_status(&m.status).to_string(),
        shares: m.shares,
        joined_at: m.joined_at,
    }
}

/// Check if the caller has permission to modify an entity.
///
/// Returns Ok(()) if:
/// - The caller is a Founder or BoardMember of the entity
/// - The caller is the entity itself (for individual entities)
///
/// Returns Err if the caller lacks permission.
fn require_entity_write_access(
    entity_mgr: &EntityManager,
    entity_id: &EntityId,
    caller_id: &EntityId,
) -> Result<()> {
    let members = entity_mgr
        .get_members(entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to check permissions: {e}")))?;

    // Check if caller is a founder or board member
    let has_access = members.iter().any(|m| {
        &m.member_id == caller_id
            && matches!(
                m.role,
                MembershipRole::Founder | MembershipRole::BoardMember
            )
    });

    if has_access {
        return Ok(());
    }

    Err(GatewayError::Forbidden(format!(
        "Caller {caller_id} lacks permission to modify entity {entity_id}. \
         Only Founders and BoardMembers can modify entities."
    )))
}

// ============================================================================
// Endpoints
// ============================================================================

/// POST /entities - Register a new entity
#[post("")]
pub async fn register_entity(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    body: web::Json<RegisterEntityRequest>,
) -> Result<HttpResponse> {
    require_scope(&req, "entity:write")?;

    // Get creator DID from claims
    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let creator_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in claims: {e}")))?;
    let creator_id = EntityId::from_did(&creator_did);

    // Validate identifier
    if body.identifier.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Entity identifier cannot be empty".to_string(),
        ));
    }

    if body.name.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Entity name cannot be empty".to_string(),
        ));
    }

    // Parse entity type
    let entity_type = parse_entity_type(&body.entity_type)?;

    // Create the entity based on type
    let mut entity = match entity_type {
        EntityType::Cooperative => CooperativeEntity::cooperative(&body.identifier, &body.name)
            .map_err(|e| {
                GatewayError::BadRequest(format!("Invalid cooperative identifier: {e}"))
            })?,
        EntityType::Federation => CooperativeEntity::federation(&body.identifier, &body.name)
            .map_err(|e| GatewayError::BadRequest(format!("Invalid federation identifier: {e}")))?,
        EntityType::Individual => {
            return Err(GatewayError::BadRequest(
                "Cannot register individual entities via this endpoint".to_string(),
            ));
        }
        EntityType::Unknown => {
            return Err(GatewayError::BadRequest(
                "Unknown entity type not supported".to_string(),
            ));
        }
    };

    let entity_id = entity.id.clone();

    if let Some(ref desc) = body.description {
        entity
            .metadata
            .insert("description".to_string(), desc.clone());
    }

    if let Some(ref parent_str) = body.parent_id {
        let parent_id: EntityId = parent_str
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid parent entity ID: {e}")))?;
        entity.parent_id = Some(parent_id);
    }

    info!(
        entity_id = %entity_id,
        entity_type = ?entity_type,
        name = %body.name,
        creator = %creator_id,
        "Registering new entity"
    );

    // Register the entity
    entity_mgr
        .register(entity.clone())
        .map_err(|e| GatewayError::InternalError(format!("Failed to register entity: {e}")))?;

    // Add the creator as a founder member
    // If this fails, clean up the entity to avoid orphaned entities
    let founder_membership =
        Membership::new(creator_id, entity_id.clone(), MembershipRole::Founder);
    if let Err(e) = entity_mgr.add_membership(founder_membership) {
        // Cleanup: remove the entity we just registered
        if let Err(cleanup_err) = entity_mgr.remove(&entity_id) {
            tracing::error!(
                entity_id = %entity_id,
                error = %cleanup_err,
                "Failed to cleanup entity after membership failure"
            );
        }
        return Err(GatewayError::InternalError(format!(
            "Failed to add founder membership: {e}"
        )));
    }

    let members = entity_mgr.get_members(&entity_id).unwrap_or_default();
    let response = entity_to_response(&entity, members.len());

    Ok(HttpResponse::Created().json(response))
}

/// GET /entities/:id - Get entity details
#[get("/{id}")]
pub async fn get_entity(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&req, "entity:read")?;

    let entity_id_str = path.into_inner();
    let entity_id: EntityId = entity_id_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid entity ID: {e}")))?;

    let entity = entity_mgr
        .get(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get entity: {e}")))?;

    let Some(entity) = entity else {
        return Err(GatewayError::NotFound(format!(
            "Entity not found: {entity_id}"
        )));
    };

    let members = entity_mgr.get_members(&entity_id).unwrap_or_default();
    let response = entity_to_response(&entity, members.len());

    Ok(HttpResponse::Ok().json(response))
}

/// PUT /entities/:id - Update entity
#[put("/{id}")]
pub async fn update_entity(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    path: web::Path<String>,
    body: web::Json<UpdateEntityRequest>,
) -> Result<HttpResponse> {
    require_scope(&req, "entity:write")?;

    // Get caller DID for authorization
    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in claims: {e}")))?;
    let caller_id = EntityId::from_did(&caller_did);

    let entity_id_str = path.into_inner();
    let entity_id: EntityId = entity_id_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid entity ID: {e}")))?;

    let mut entity = entity_mgr
        .get(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get entity: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Entity not found: {entity_id}")))?;

    // Check caller has permission to modify this entity
    require_entity_write_access(&entity_mgr, &entity_id, &caller_id)?;

    // Apply updates
    if let Some(ref name) = body.name {
        if name.trim().is_empty() {
            return Err(GatewayError::BadRequest(
                "Entity name cannot be empty".to_string(),
            ));
        }
        entity.name = name.clone();
    }

    if let Some(ref desc) = body.description {
        entity
            .metadata
            .insert("description".to_string(), desc.clone());
    }

    info!(
        entity_id = %entity_id,
        "Updating entity"
    );

    entity_mgr
        .update(entity.clone())
        .map_err(|e| GatewayError::InternalError(format!("Failed to update entity: {e}")))?;

    let members = entity_mgr.get_members(&entity_id).unwrap_or_default();
    let response = entity_to_response(&entity, members.len());

    Ok(HttpResponse::Ok().json(response))
}

/// DELETE /entities/:id - Delete entity (only if no members)
#[delete("/{id}")]
pub async fn delete_entity(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&req, "entity:write")?;

    // Get caller DID for authorization
    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in claims: {e}")))?;
    let caller_id = EntityId::from_did(&caller_did);

    let entity_id_str = path.into_inner();
    let entity_id: EntityId = entity_id_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid entity ID: {e}")))?;

    // Verify entity exists
    let _entity = entity_mgr
        .get(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get entity: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Entity not found: {entity_id}")))?;

    // Check caller has permission to delete this entity (Founders only for deletion)
    require_entity_write_access(&entity_mgr, &entity_id, &caller_id)?;

    info!(
        entity_id = %entity_id,
        "Deleting entity"
    );

    entity_mgr.remove(&entity_id).map_err(|e| {
        if e.to_string().contains("active members") {
            GatewayError::BadRequest(e.to_string())
        } else {
            GatewayError::InternalError(format!("Failed to delete entity: {e}"))
        }
    })?;

    Ok(HttpResponse::NoContent().finish())
}

/// GET /entities/:id/members - List members
#[get("/{id}/members")]
pub async fn list_members(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&req, "entity:read")?;

    let entity_id_str = path.into_inner();
    let entity_id: EntityId = entity_id_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid entity ID: {e}")))?;

    // Verify entity exists
    let _entity = entity_mgr
        .get(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get entity: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Entity not found: {entity_id}")))?;

    let members = entity_mgr
        .get_members(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get members: {e}")))?;

    let response: Vec<MembershipResponse> = members.iter().map(membership_to_response).collect();

    Ok(HttpResponse::Ok().json(response))
}

/// POST /entities/:id/members - Add membership
#[post("/{id}/members")]
pub async fn add_membership(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    path: web::Path<String>,
    body: web::Json<AddMembershipRequest>,
) -> Result<HttpResponse> {
    require_scope(&req, "entity:write")?;

    // Get caller DID for authorization
    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in claims: {e}")))?;
    let caller_id = EntityId::from_did(&caller_did);

    let entity_id_str = path.into_inner();
    let entity_id: EntityId = entity_id_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid entity ID: {e}")))?;

    // Verify entity exists
    let _entity = entity_mgr
        .get(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get entity: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Entity not found: {entity_id}")))?;

    // Check caller has permission to add members
    require_entity_write_access(&entity_mgr, &entity_id, &caller_id)?;

    // Parse member ID (can be DID or EntityId)
    let member_id = if body.member_id.starts_with("did:") {
        let did: Did = body
            .member_id
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;
        EntityId::from_did(&did)
    } else {
        body.member_id
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid member ID: {e}")))?
    };

    let role = parse_role(&body.role)?;

    info!(
        entity_id = %entity_id,
        member_id = %member_id,
        role = ?role,
        "Adding membership"
    );

    let mut membership = Membership::new(member_id.clone(), entity_id.clone(), role);
    if let Some(shares) = body.shares {
        membership.shares = shares;
    }

    entity_mgr
        .add_membership(membership.clone())
        .map_err(|e| GatewayError::InternalError(format!("Failed to add membership: {e}")))?;

    let response = membership_to_response(&membership);

    Ok(HttpResponse::Created().json(response))
}

/// DELETE /entities/:id/members/:member_id - Remove membership
#[delete("/{id}/members/{member_id}")]
pub async fn remove_membership(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&req, "entity:write")?;

    // Get caller DID for authorization
    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in claims: {e}")))?;
    let caller_id = EntityId::from_did(&caller_did);

    let (entity_id_str, member_id_str) = path.into_inner();

    let entity_id: EntityId = entity_id_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid entity ID: {e}")))?;

    let member_id = if member_id_str.starts_with("did:") {
        let did: Did = member_id_str
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;
        EntityId::from_did(&did)
    } else {
        member_id_str
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid member ID: {e}")))?
    };

    // Check caller has permission to remove members (founders/board) or is the member themselves
    let is_self_removal = caller_id == member_id;
    if !is_self_removal {
        require_entity_write_access(&entity_mgr, &entity_id, &caller_id)?;
    }

    // Verify entity exists
    let _entity = entity_mgr
        .get(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get entity: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Entity not found: {entity_id}")))?;

    info!(
        entity_id = %entity_id,
        member_id = %member_id,
        "Removing membership"
    );

    entity_mgr
        .remove_membership(&entity_id, &member_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to remove membership: {e}")))?;

    Ok(HttpResponse::NoContent().finish())
}

/// Configure entity routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(register_entity)
        .service(get_entity)
        .service(update_entity)
        .service(delete_entity)
        .service(list_members)
        .service(add_membership)
        .service(remove_membership);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_entity_type() {
        assert!(matches!(
            parse_entity_type("cooperative").unwrap(),
            EntityType::Cooperative
        ));
        assert!(matches!(
            parse_entity_type("coop").unwrap(),
            EntityType::Cooperative
        ));
        assert!(matches!(
            parse_entity_type("federation").unwrap(),
            EntityType::Federation
        ));
        assert!(parse_entity_type("invalid").is_err());
    }

    #[test]
    fn test_parse_role() {
        assert!(matches!(
            parse_role("founder").unwrap(),
            MembershipRole::Founder
        ));
        assert!(matches!(
            parse_role("member").unwrap(),
            MembershipRole::Member
        ));
        assert!(matches!(
            parse_role("board_member").unwrap(),
            MembershipRole::BoardMember
        ));
        // Officer with default title
        assert!(
            matches!(parse_role("officer").unwrap(), MembershipRole::Officer { title } if title == "Officer")
        );
        // Officer with custom title (case preserved)
        assert!(
            matches!(parse_role("officer:President").unwrap(), MembershipRole::Officer { title } if title == "President")
        );
        // Mixed case role type still works
        assert!(
            matches!(parse_role("OFFICER:CEO").unwrap(), MembershipRole::Officer { title } if title == "CEO")
        );
        assert!(parse_role("invalid").is_err());
    }
}
