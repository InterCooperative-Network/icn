//! Entity API endpoints
//!
//! Provides REST API for entity management including:
//! - Entity CRUD operations (cooperatives, federations)
//! - Membership management
//!
//! Entities are organizational units in the ICN that can own treasuries,
//! have governance domains, and contain members (individuals or other entities).
//!
//! ## Authorization Model
//!
//! This module uses a two-layer authorization model:
//!
//! 1. **Scope-based (coarse-grained)**: JWT must include the required scope
//!    (e.g., `entity:write`) to access the endpoint at all. This is an
//!    application-level capability check.
//!
//! 2. **Membership-based (fine-grained)**: For mutating operations, the caller
//!    must be a Founder or BoardMember of the specific entity. This is enforced
//!    by `require_entity_write_access()`.
//!
//! This design intentionally separates "can this client use entity APIs" from
//! "can this user modify THIS entity". A token with `entity:write` scope can
//! attempt modifications, but will be rejected unless the caller has the
//! appropriate role in the target entity.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use icn_entity::{CooperativeEntity, EntityId, EntityType, Membership, MembershipRole};
use icn_identity::Did;
use icn_obs::metrics::gateway as gateway_metrics;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::entity_audit::{EntityAuditManager, EntityOperation};
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
    audit_mgr: web::Data<Arc<EntityAuditManager>>,
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
    let founder_membership = Membership::new(
        creator_id.clone(),
        entity_id.clone(),
        MembershipRole::Founder,
    );
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

    // Record audit trail - fail the request if audit logging fails for compliance
    // If audit fails, we must clean up both entity and membership to maintain consistency
    if let Err(e) = audit_mgr.record_audit(
        &entity_id,
        EntityOperation::Registered {
            entity_type: format_entity_type(&entity_type).to_string(),
            name: body.name.clone(),
        },
        &creator_id,
        None,
        None,
    ) {
        warn!(
            entity_id = %entity_id,
            error = %e,
            "Failed to record entity registration audit - rolling back"
        );
        // Cleanup: remove entity first, then membership
        // Order matters: if entity removal fails, we still have a valid entity with its founder.
        // If we removed membership first and entity removal failed, we'd have an orphaned entity.
        // Track rollback failures for operational alerting
        let mut rollback_errors = Vec::new();
        if let Err(cleanup_err) = entity_mgr.remove(&entity_id) {
            rollback_errors.push(format!("entity: {cleanup_err}"));
            tracing::error!(
                entity_id = %entity_id,
                error = %cleanup_err,
                "Failed to cleanup entity after audit failure"
            );
        }
        if let Err(cleanup_err) = entity_mgr.remove_membership(&entity_id, &creator_id) {
            rollback_errors.push(format!("membership: {cleanup_err}"));
            tracing::error!(
                entity_id = %entity_id,
                member_id = %creator_id,
                error = %cleanup_err,
                "Failed to cleanup membership after audit failure"
            );
        }
        // Record metric if any rollback failed
        if !rollback_errors.is_empty() {
            gateway_metrics::entity_audit_rollback_failure_inc("register_entity");
            return Err(GatewayError::InternalError(format!(
                "Failed to record audit: {e}. CRITICAL: Rollback also failed: [{}]. Entity may be in inconsistent state.",
                rollback_errors.join(", ")
            )));
        }
        return Err(GatewayError::InternalError(format!(
            "Failed to record audit: {e}"
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
    audit_mgr: web::Data<Arc<EntityAuditManager>>,
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

    // Store previous entity state BEFORE mutation for potential rollback
    // This ensures we capture the true pre-mutation state even if EntityManager
    // implements caching in the future
    let previous_entity = entity.clone();

    // Track changed fields for audit
    let mut changed_fields = Vec::new();

    // Apply updates
    if let Some(ref name) = body.name {
        if name.trim().is_empty() {
            return Err(GatewayError::BadRequest(
                "Entity name cannot be empty".to_string(),
            ));
        }
        entity.name = name.clone();
        changed_fields.push("name".to_string());
    }

    if let Some(ref desc) = body.description {
        entity
            .metadata
            .insert("description".to_string(), desc.clone());
        changed_fields.push("description".to_string());
    }

    // Skip no-op updates - return early if no changes were requested
    if changed_fields.is_empty() {
        let members = entity_mgr.get_members(&entity_id).unwrap_or_default();
        let response = entity_to_response(&entity, members.len());
        return Ok(HttpResponse::Ok().json(response));
    }

    info!(
        entity_id = %entity_id,
        changed_fields = ?changed_fields,
        "Updating entity"
    );

    entity_mgr
        .update(entity.clone())
        .map_err(|e| GatewayError::InternalError(format!("Failed to update entity: {e}")))?;

    // Record audit trail - fail the request if audit logging fails for compliance
    // NOTE: Compensation tradeoff - if audit fails after update, we attempt to restore
    // the previous entity state. This provides stronger consistency guarantees but adds
    // complexity. Alternative would be "best effort" auditing (warn but don't fail).
    if let Err(e) = audit_mgr.record_audit(
        &entity_id,
        EntityOperation::Updated {
            changed_fields: changed_fields.clone(),
        },
        &caller_id,
        None,
        None,
    ) {
        warn!(
            entity_id = %entity_id,
            error = %e,
            "Failed to record entity update audit - attempting rollback"
        );
        // Attempt compensation: restore pre-mutation state (cloned BEFORE applying updates)
        if let Err(rollback_err) = entity_mgr.update(previous_entity) {
            gateway_metrics::entity_audit_rollback_failure_inc("update_entity");
            tracing::error!(
                entity_id = %entity_id,
                error = %rollback_err,
                "Failed to rollback entity after audit failure - entity in inconsistent state"
            );
            return Err(GatewayError::InternalError(format!(
                "Failed to record audit: {e}. CRITICAL: Rollback also failed: {rollback_err}. Entity may be in inconsistent state."
            )));
        }
        return Err(GatewayError::InternalError(format!(
            "Failed to record audit: {e}"
        )));
    }

    let members = entity_mgr.get_members(&entity_id).unwrap_or_default();
    let response = entity_to_response(&entity, members.len());

    Ok(HttpResponse::Ok().json(response))
}

/// DELETE /entities/:id - Delete entity (only if no members)
#[delete("/{id}")]
pub async fn delete_entity(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    audit_mgr: web::Data<Arc<EntityAuditManager>>,
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

    // Verify entity exists and store for potential restoration
    let entity = entity_mgr
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

    // Record audit trail - fail the request if audit logging fails for compliance
    // NOTE: Compensation tradeoff - if audit fails after deletion, we attempt to restore
    // the entity. We only need to restore the entity itself, not memberships, because
    // entity_mgr.remove() enforces that entities can only be deleted when they have
    // no members (see entity_mgr.rs:118-123). If there were members, the deletion
    // would have failed before reaching this point.
    if let Err(e) =
        audit_mgr.record_audit(&entity_id, EntityOperation::Deleted, &caller_id, None, None)
    {
        warn!(
            entity_id = %entity_id,
            error = %e,
            "Failed to record entity deletion audit - attempting restoration"
        );
        // Attempt compensation: re-register the entity (no memberships to restore
        // since remove() only succeeds when entity has no members)
        if let Err(restore_err) = entity_mgr.register(entity.clone()) {
            gateway_metrics::entity_audit_rollback_failure_inc("delete_entity");
            tracing::error!(
                entity_id = %entity_id,
                error = %restore_err,
                "Failed to restore entity after audit failure - data loss occurred"
            );
            return Err(GatewayError::InternalError(format!(
                "Failed to record audit: {e}. CRITICAL: Restoration also failed: {restore_err}. Entity data lost."
            )));
        }
        return Err(GatewayError::InternalError(format!(
            "Failed to record audit: {e}"
        )));
    }

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
    audit_mgr: web::Data<Arc<EntityAuditManager>>,
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

    // Validate shares if provided
    // Max 1 million shares per member to prevent overflow in weighted voting calculations
    const MAX_SHARES: u64 = 1_000_000;
    if let Some(shares) = body.shares {
        if shares == 0 {
            return Err(GatewayError::BadRequest(
                "Shares must be greater than 0. Omit the field to use default (1).".to_string(),
            ));
        }
        if shares > MAX_SHARES {
            return Err(GatewayError::BadRequest(format!(
                "Shares cannot exceed {MAX_SHARES}. Requested: {shares}"
            )));
        }
    }

    info!(
        entity_id = %entity_id,
        member_id = %member_id,
        role = ?role,
        "Adding membership"
    );

    let mut membership = Membership::new(member_id.clone(), entity_id.clone(), role.clone());
    if let Some(shares) = body.shares {
        membership.shares = shares;
    }

    entity_mgr
        .add_membership(membership.clone())
        .map_err(|e| GatewayError::InternalError(format!("Failed to add membership: {e}")))?;

    // Record audit trail - fail the request if audit logging fails for compliance
    // NOTE: Compensation - if audit fails, rollback the membership addition
    if let Err(e) = audit_mgr.record_audit(
        &entity_id,
        EntityOperation::MemberAdded {
            member_id: member_id.clone(),
            role,
        },
        &caller_id,
        None,
        None,
    ) {
        warn!(
            entity_id = %entity_id,
            member_id = %member_id,
            error = %e,
            "Failed to record member addition audit - rolling back"
        );
        // Rollback: remove the membership we just added
        if let Err(rollback_err) = entity_mgr.remove_membership(&entity_id, &member_id) {
            gateway_metrics::entity_audit_rollback_failure_inc("add_membership");
            tracing::error!(
                entity_id = %entity_id,
                member_id = %member_id,
                error = %rollback_err,
                "Failed to rollback membership after audit failure - inconsistent state"
            );
            return Err(GatewayError::InternalError(format!(
                "Failed to record audit: {e}. CRITICAL: Rollback also failed: {rollback_err}. Membership may be in inconsistent state."
            )));
        }
        return Err(GatewayError::InternalError(format!(
            "Failed to record audit: {e}"
        )));
    }

    let response = membership_to_response(&membership);

    Ok(HttpResponse::Created().json(response))
}

/// DELETE /entities/:id/members/:member_id - Remove membership
#[delete("/{id}/members/{member_id}")]
pub async fn remove_membership(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    audit_mgr: web::Data<Arc<EntityAuditManager>>,
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

    // Get current members to check if removing last founder
    let members = entity_mgr
        .get_members(&entity_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get members: {e}")))?;

    // Find the membership being removed
    let membership_to_remove = members.iter().find(|m| m.member_id == member_id);

    // Prevent removing the last founder
    if let Some(membership) = membership_to_remove {
        if matches!(membership.role, MembershipRole::Founder) {
            let founder_count = members
                .iter()
                .filter(|m| matches!(m.role, MembershipRole::Founder))
                .count();

            if founder_count <= 1 {
                return Err(GatewayError::BadRequest(
                    "Cannot remove the last founder. Transfer founder role to another member first."
                        .to_string(),
                ));
            }
        }
    }

    info!(
        entity_id = %entity_id,
        member_id = %member_id,
        "Removing membership"
    );

    // Store membership for potential rollback
    let removed_membership = membership_to_remove.cloned();

    entity_mgr
        .remove_membership(&entity_id, &member_id)
        .map_err(|e| GatewayError::InternalError(format!("Failed to remove membership: {e}")))?;

    // Record audit trail - fail the request if audit logging fails for compliance
    // NOTE: Compensation - if audit fails, restore the membership we just removed
    let reason = if is_self_removal {
        Some("Self-removal".to_string())
    } else {
        None
    };
    if let Err(e) = audit_mgr.record_audit(
        &entity_id,
        EntityOperation::MemberRemoved {
            member_id: member_id.clone(),
            reason,
        },
        &caller_id,
        None,
        None,
    ) {
        warn!(
            entity_id = %entity_id,
            member_id = %member_id,
            error = %e,
            "Failed to record member removal audit - attempting rollback"
        );
        // Rollback: restore the membership we just removed
        if let Some(membership) = removed_membership {
            if let Err(rollback_err) = entity_mgr.add_membership(membership) {
                gateway_metrics::entity_audit_rollback_failure_inc("remove_membership");
                tracing::error!(
                    entity_id = %entity_id,
                    member_id = %member_id,
                    error = %rollback_err,
                    "Failed to rollback membership removal - member lost from entity"
                );
                return Err(GatewayError::InternalError(format!(
                    "Failed to record audit: {e}. CRITICAL: Rollback also failed: {rollback_err}. Member may have been lost."
                )));
            }
        }
        return Err(GatewayError::InternalError(format!(
            "Failed to record audit: {e}"
        )));
    }

    Ok(HttpResponse::NoContent().finish())
}

/// Query parameters for audit trail endpoint
#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    /// Maximum number of records to return (default: 50)
    pub limit: Option<usize>,
    /// Number of records to skip (default: 0)
    pub offset: Option<usize>,
}

/// GET /entities/:id/audit - Get entity audit trail
///
/// Requires the caller to either:
/// - Be a member of the entity, or
/// - Have "entity:audit" scope (for admin access)
#[get("/{id}/audit")]
pub async fn get_entity_audit(
    req: HttpRequest,
    entity_mgr: web::Data<Arc<EntityManager>>,
    audit_mgr: web::Data<Arc<EntityAuditManager>>,
    path: web::Path<String>,
    query: web::Query<AuditQueryParams>,
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

    // Authorization: caller must be a member of the entity OR have entity:audit scope
    let claims = get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // Check for admin audit scope (allows viewing any entity's audit trail)
    let has_audit_scope = claims
        .scopes
        .iter()
        .any(|s| s == "entity:audit" || s == "admin");

    if !has_audit_scope {
        // If no admin scope, must be a member of the entity
        let caller_did: Did = claims
            .sub
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;
        let caller_id = EntityId::from_did(&caller_did);

        let members = entity_mgr
            .get_members(&entity_id)
            .map_err(|e| GatewayError::InternalError(format!("Failed to get members: {e}")))?;

        let is_member = members.iter().any(|m| m.member_id == caller_id);
        if !is_member {
            return Err(GatewayError::Forbidden(
                "You must be a member of the entity to view its audit trail".to_string(),
            ));
        }
    }

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let trail = audit_mgr
        .get_audit_trail(&entity_id, limit, offset)
        .map_err(|e| GatewayError::InternalError(format!("Failed to get audit trail: {e}")))?;

    Ok(HttpResponse::Ok().json(trail))
}

/// Configure entity routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(register_entity)
        .service(get_entity)
        .service(update_entity)
        .service(delete_entity)
        .service(list_members)
        .service(add_membership)
        .service(remove_membership)
        .service(get_entity_audit);
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

    #[test]
    fn test_format_entity_type() {
        assert_eq!(format_entity_type(&EntityType::Individual), "individual");
        assert_eq!(format_entity_type(&EntityType::Cooperative), "cooperative");
        assert_eq!(format_entity_type(&EntityType::Federation), "federation");
        assert_eq!(format_entity_type(&EntityType::Unknown), "unknown");
    }

    #[test]
    fn test_format_entity_status() {
        use icn_entity::EntityStatus;

        assert_eq!(format_entity_status(&EntityStatus::Forming), "forming");
        assert_eq!(format_entity_status(&EntityStatus::Active), "active");

        let suspended = EntityStatus::Suspended {
            reason: "audit".to_string(),
            suspended_at: 123456,
        };
        assert_eq!(format_entity_status(&suspended), "suspended:audit");

        let dissolving = EntityStatus::Dissolving { started_at: 123456 };
        assert_eq!(format_entity_status(&dissolving), "dissolving");

        let dissolved = EntityStatus::Dissolved {
            dissolved_at: 123456,
        };
        assert_eq!(format_entity_status(&dissolved), "dissolved");

        let merged = EntityStatus::Merged {
            into: "entity:icn:cooperative:target-coop".parse().unwrap(),
            merged_at: 123456,
        };
        assert!(format_entity_status(&merged).starts_with("merged:"));

        let split = EntityStatus::Split {
            into: vec!["entity:icn:cooperative:coop-a".parse().unwrap()],
            split_at: 123456,
        };
        assert_eq!(format_entity_status(&split), "split");
    }

    #[test]
    fn test_format_role() {
        assert_eq!(format_role(&MembershipRole::Founder), "founder");
        assert_eq!(format_role(&MembershipRole::Member), "member");
        assert_eq!(format_role(&MembershipRole::Worker), "worker");
        assert_eq!(format_role(&MembershipRole::Consumer), "consumer");
        assert_eq!(format_role(&MembershipRole::Producer), "producer");
        assert_eq!(format_role(&MembershipRole::BoardMember), "board_member");
        assert_eq!(
            format_role(&MembershipRole::Officer {
                title: "President".to_string()
            }),
            "officer:President"
        );
        assert_eq!(
            format_role(&MembershipRole::FederatedMember),
            "federated_member"
        );
        assert_eq!(
            format_role(&MembershipRole::AssociateMember),
            "associate_member"
        );
        assert_eq!(
            format_role(&MembershipRole::ObserverMember),
            "observer_member"
        );
        assert_eq!(
            format_role(&MembershipRole::ProvisionalMember),
            "provisional_member"
        );
        assert_eq!(
            format_role(&MembershipRole::Custom {
                name: "Steward".to_string()
            }),
            "custom:Steward"
        );
    }

    #[test]
    fn test_format_membership_status() {
        use icn_entity::MembershipStatus;

        assert_eq!(
            format_membership_status(&MembershipStatus::Pending),
            "pending"
        );
        assert_eq!(
            format_membership_status(&MembershipStatus::Active),
            "active"
        );
        assert_eq!(
            format_membership_status(&MembershipStatus::Suspended),
            "suspended"
        );
        assert_eq!(
            format_membership_status(&MembershipStatus::Inactive),
            "inactive"
        );
        assert_eq!(
            format_membership_status(&MembershipStatus::Removed),
            "removed"
        );
        assert_eq!(
            format_membership_status(&MembershipStatus::Resigned),
            "resigned"
        );
        assert_eq!(
            format_membership_status(&MembershipStatus::Expelled),
            "expelled"
        );
    }
}
