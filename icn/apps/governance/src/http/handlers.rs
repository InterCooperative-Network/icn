//! Governance HTTP handlers.
//!
//! All handlers are generic over `E: GovernanceEventEmitter` and receive
//! shared context via `web::Data<GovernanceContext<E>>`. Route registration
//! lives in `super::configure`.

use actix_web::{web, HttpRequest, HttpResponse};
use icn_federation::SettlementInterval;
use icn_governance::{
    ActionItemFilter, ActionItemId, ActionItemPriority, ActionItemStatus, DataSharingLevel,
    Delegation, DelegationId, DelegationScope, DisputeResolutionMethod, FederationProposal,
    FederationTerms, GovernanceDomainId, GovernanceParams, MembershipAction, MembershipConfig,
    ProposalId, ProposalPayload, ProposalScope, VoteChoice,
};
use icn_http_kit::{
    auth::{require_scope, BasicClaims},
    error::ApiError,
    pagination::{ListPagination, ListQuery, ListResponse},
};
use icn_identity::Did;

use super::configure::{GovernanceContext, GovernanceEffect};
use super::models::*;
use super::validation as val;
use crate::events::GovernanceEventEmitter;

// ============================================================================
// Internal helpers
// ============================================================================

fn current_time_secs() -> u64 {
    val::current_time_secs()
}

fn err_bad(msg: impl Into<String>) -> ApiError {
    ApiError::BadRequest(msg.into())
}

fn err_not_found(msg: impl Into<String>) -> ApiError {
    ApiError::NotFound(msg.into())
}

fn err_forbidden(msg: impl Into<String>) -> ApiError {
    ApiError::Forbidden(msg.into())
}

fn err_internal(msg: impl Into<String>) -> ApiError {
    ApiError::Internal(msg.into())
}

/// Map anyhow error to ApiError::Internal.
fn anyhow_to_api(e: anyhow::Error) -> ApiError {
    ApiError::Internal(e.to_string())
}

/// Parse a DID string, returning ApiError::BadRequest on failure.
fn parse_did(s: &str, context: &str) -> Result<Did, ApiError> {
    s.parse::<Did>()
        .map_err(|e| err_bad(format!("{context}: {e}")))
}

/// Check domain membership for a given user.
async fn check_domain_membership(
    mgr: &crate::manager::GovernanceManager,
    domain_id: &GovernanceDomainId,
    user_did: &Did,
) -> Result<(), ApiError> {
    let domain = mgr
        .get_domain(domain_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Domain not found: {}", domain_id.0)))?;

    let is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(user_did),
        icn_governance::MembershipSource::TrustThreshold(_) => true,
    };
    if !is_member {
        return Err(err_forbidden(format!(
            "Only domain members can perform this action (not a member of domain '{}')",
            domain_id.0
        )));
    }
    Ok(())
}

// ============================================================================
// Delegation helpers
// ============================================================================

fn delegation_to_response(d: &Delegation) -> DelegationResponse {
    let scope_str = match &d.scope {
        DelegationScope::Blanket => "blanket".to_string(),
        DelegationScope::Domain(domain_id) => format!("domain:{}", domain_id.0),
        DelegationScope::Proposal(proposal_id) => format!("proposal:{}", proposal_id.0),
    };
    let now = current_time_secs();
    DelegationResponse {
        id: d.id.0.clone(),
        delegator: d.delegator.to_string(),
        delegate: d.delegate.to_string(),
        scope: scope_str,
        created_at: d.created_at,
        expires_at: d.expires_at,
        revoked_at: d.revoked_at,
        is_active: d.is_active(now),
    }
}

fn parse_delegation_scope(scope: &str) -> Result<DelegationScope, ApiError> {
    if scope == "blanket" {
        return Ok(DelegationScope::Blanket);
    }
    if let Some(domain_id) = scope.strip_prefix("domain:") {
        if domain_id.is_empty() {
            return Err(err_bad("Domain ID cannot be empty in scope"));
        }
        return Ok(DelegationScope::Domain(GovernanceDomainId(
            domain_id.to_string(),
        )));
    }
    if let Some(proposal_id) = scope.strip_prefix("proposal:") {
        if proposal_id.is_empty() {
            return Err(err_bad("Proposal ID cannot be empty in scope"));
        }
        return Ok(DelegationScope::Proposal(ProposalId(
            proposal_id.to_string(),
        )));
    }
    Err(err_bad(format!(
        "Invalid delegation scope: '{scope}'. Must be 'blanket', 'domain:<id>', or 'proposal:<id>'"
    )))
}

// ============================================================================
// Action item helpers
// ============================================================================

fn parse_action_item_id(s: &str) -> Result<ActionItemId, ApiError> {
    uuid::Uuid::parse_str(s)
        .map(ActionItemId::from_uuid)
        .map_err(|e| err_bad(format!("Invalid action item ID: {e}")))
}

fn parse_priority(s: &str) -> Result<ActionItemPriority, ApiError> {
    match s.to_lowercase().as_str() {
        "low" => Ok(ActionItemPriority::Low),
        "medium" => Ok(ActionItemPriority::Medium),
        "high" => Ok(ActionItemPriority::High),
        "critical" => Ok(ActionItemPriority::Critical),
        _ => Err(err_bad(format!(
            "Invalid priority: {s}. Must be one of: low, medium, high, critical"
        ))),
    }
}

fn parse_status(s: &str) -> Result<ActionItemStatus, ApiError> {
    match s.to_lowercase().as_str() {
        "pending" => Ok(ActionItemStatus::Pending),
        "in_progress" | "inprogress" => Ok(ActionItemStatus::InProgress),
        "completed" => Ok(ActionItemStatus::Completed),
        "deferred" => Ok(ActionItemStatus::Deferred),
        "cancelled" | "canceled" => Ok(ActionItemStatus::Cancelled),
        _ => Err(err_bad(format!(
            "Invalid status: {s}. Must be one of: pending, in_progress, completed, deferred, cancelled"
        ))),
    }
}

fn build_action_item_filter(query: &ActionItemFilterParams) -> Result<ActionItemFilter, ApiError> {
    let mut filter = ActionItemFilter::default();
    if let Some(ref status) = query.status {
        filter.status = Some(parse_status(status)?);
    }
    if let Some(ref assignee) = query.assignee {
        filter.assignee = Some(parse_did(assignee, "Invalid assignee DID")?);
    }
    if let Some(ref priority) = query.priority {
        filter.priority = Some(parse_priority(priority)?);
    }
    if query.overdue == Some(true) {
        filter.overdue_only = Some(current_time_secs());
    }
    if let Some(ref tag) = query.tag {
        filter.tag = Some(tag.clone());
    }
    Ok(filter)
}

fn action_item_to_response(item: &icn_governance::ActionItem) -> ActionItemResponse {
    let now = current_time_secs();
    ActionItemResponse {
        id: item.id.to_string(),
        domain_id: item.domain_id.0.clone(),
        title: item.title.clone(),
        description: item.description.clone(),
        assignee: item.assignee.as_ref().map(|d| d.to_string()),
        due_date: item.due_date,
        status: match item.status {
            ActionItemStatus::Pending => "pending".to_string(),
            ActionItemStatus::InProgress => "in_progress".to_string(),
            ActionItemStatus::Completed => "completed".to_string(),
            ActionItemStatus::Deferred => "deferred".to_string(),
            ActionItemStatus::Cancelled => "cancelled".to_string(),
        },
        priority: match item.priority {
            ActionItemPriority::Low => "low".to_string(),
            ActionItemPriority::Medium => "medium".to_string(),
            ActionItemPriority::High => "high".to_string(),
            ActionItemPriority::Critical => "critical".to_string(),
        },
        created_by: item.created_by.to_string(),
        created_at: item.created_at,
        updated_at: item.updated_at,
        linked_proposal: item.linked_proposal.as_ref().map(|p| p.to_string()),
        meeting_context: item.meeting_context.clone(),
        tags: item.tags.clone(),
        notes: item
            .notes
            .iter()
            .map(|n| ActionItemNoteResponse {
                id: n.id.to_string(),
                author: n.author.to_string(),
                content: n.content.clone(),
                created_at: n.created_at,
            })
            .collect(),
        is_overdue: item.is_overdue(now),
    }
}

fn comment_to_response(c: icn_governance::Comment) -> CommentResponse {
    CommentResponse {
        id: c.id.0,
        proposal_id: c.proposal_id.0,
        author: c.author.to_string(),
        content: c.content,
        parent_id: c.parent_id.map(|p| p.0),
        created_at: c.created_at,
        updated_at: c.updated_at,
        reactions: c.reactions.into_iter().map(|(k, v)| (k, v.len())).collect(),
        is_edited: c.is_edited,
        is_deleted: c.is_deleted,
    }
}

// ============================================================================
// Federation proposal helpers
// ============================================================================

/// Validate and extract common federation proposal fields.
fn extract_federation_common(
    http_req: &HttpRequest,
    domain_id: &str,
    title: &str,
    description: &str,
) -> Result<(Did, String, String, String), ApiError> {
    let claims = require_scope::<BasicClaims>(http_req, "governance:write")?;
    let proposer_did = parse_did(&claims.sub, "Invalid DID in token")?;
    val::validate_domain_id(domain_id)?;
    val::validate_proposal_title(title)?;
    val::validate_proposal_description(description)?;
    Ok((
        proposer_did,
        domain_id.to_string(),
        title.to_string(),
        description.to_string(),
    ))
}

/// Shared implementation for creating a federation proposal.
async fn create_federation_proposal_impl<E: GovernanceEventEmitter>(
    ctx: &GovernanceContext<E>,
    proposer_did: Did,
    domain_id: String,
    title: String,
    description: String,
    fed_proposal: FederationProposal,
) -> Result<HttpResponse, ApiError> {
    // Validate federation proposal fields
    fed_proposal.validate().map_err(err_bad)?;

    let payload = ProposalPayload::Federation(fed_proposal);
    let gov_domain_id = GovernanceDomainId(domain_id.clone());
    let suggested_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));

    let proposal_id = ctx
        .manager
        .create_proposal(
            suggested_id,
            gov_domain_id,
            proposer_did.clone(),
            title.clone(),
            description.clone(),
            payload,
            ProposalScope::Local,
        )
        .await
        .map_err(anyhow_to_api)?;

    ctx.emitter.emit_proposal_created(
        &proposal_id.0,
        &domain_id,
        &proposer_did.to_string(),
        &title,
        "federation",
    );

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Proposal creation succeeded but proposal not found"))?;

    Ok(HttpResponse::Created().json(proposal))
}

fn parse_data_sharing_level(s: &str) -> Result<DataSharingLevel, ApiError> {
    match s {
        "none" => Ok(DataSharingLevel::None),
        "metadata_only" => Ok(DataSharingLevel::MetadataOnly),
        "full" => Ok(DataSharingLevel::Full),
        _ => Err(err_internal(format!(
            "Invalid data_sharing_level after validation: '{s}'"
        ))),
    }
}

fn parse_dispute_resolution(s: &str) -> Result<DisputeResolutionMethod, ApiError> {
    if s.starts_with("arbitrator:") {
        let arbitrator_id = s.strip_prefix("arbitrator:").unwrap_or("").to_string();
        return Ok(DisputeResolutionMethod::ArbitratorCooperative { arbitrator_id });
    }
    match s {
        "federation_mediation" => Ok(DisputeResolutionMethod::FederationMediation),
        "federation_vote" => Ok(DisputeResolutionMethod::FederationVote),
        _ => Err(err_internal(format!(
            "Invalid dispute_resolution after validation: '{s}'"
        ))),
    }
}

fn parse_settlement_interval(s: &str) -> Result<SettlementInterval, ApiError> {
    match s {
        "daily" => Ok(SettlementInterval::Daily),
        "weekly" => Ok(SettlementInterval::Weekly),
        "monthly" => Ok(SettlementInterval::Monthly),
        "manual" => Ok(SettlementInterval::Manual),
        _ => Err(err_internal(format!(
            "Invalid settlement_interval after validation: '{s}'"
        ))),
    }
}

// ============================================================================
// Domain handlers
// ============================================================================

/// POST /gov/domains — Create a new governance domain.
pub async fn create_domain<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<CreateDomainRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let creator_did = parse_did(&claims.sub, "Invalid DID in token")?;

    val::validate_domain_id(&req.id)?;
    val::validate_domain_name(&req.name)?;
    val::validate_governance_model(&req.profile)?;
    val::validate_domain_members(&req.members)?;

    const MAX_VOTING_PERIOD_DAYS: u64 = val::MAX_VOTING_PERIOD_SECONDS / 86400;
    if req.voting_period_days == 0 {
        return Err(err_bad("Voting period must be greater than 0 days"));
    }
    if req.voting_period_days > MAX_VOTING_PERIOD_DAYS {
        return Err(err_bad(format!(
            "Voting period exceeds maximum of {MAX_VOTING_PERIOD_DAYS} days (1 year)"
        )));
    }

    let voting_period_seconds = req.voting_period_days * 86400;
    val::validate_governance_params(
        req.quorum_percent,
        req.approval_percent,
        voting_period_seconds,
    )?;

    let members: Result<Vec<Did>, ApiError> = req
        .members
        .iter()
        .map(|s| parse_did(s, "Invalid member DID"))
        .collect();
    let members = members?;

    let membership = MembershipConfig::static_list(members);
    let params = GovernanceParams::new(
        req.quorum_percent,
        req.approval_percent,
        voting_period_seconds,
    );
    let domain_id = GovernanceDomainId(req.id.clone());

    ctx.manager
        .create_domain(
            domain_id.clone(),
            req.name.clone(),
            req.profile.clone(),
            params,
            membership,
        )
        .await
        .map_err(anyhow_to_api)?;

    ctx.emitter
        .emit_domain_created(&domain_id.0, &req.name, &creator_did.to_string());

    let domain = ctx
        .manager
        .get_domain(&domain_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Domain creation succeeded but domain not found"))?;

    Ok(HttpResponse::Created().json(domain))
}

/// GET /gov/domains — List all governance domains (with pagination).
pub async fn list_domains<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    query: web::Query<ListQuery>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let query = query.into_inner().validate();
    query.validate_filters().map_err(err_bad)?;

    let mut domains = ctx.manager.list_domains().await.map_err(anyhow_to_api)?;

    if let Some(name_filter) = query.filter("name") {
        let lower = name_filter.to_lowercase();
        domains.retain(|d| d.name.to_lowercase().contains(&lower));
    }

    let sort_fields = query.sort_fields();
    if sort_fields.is_empty() {
        domains.sort_by(|a, b| a.name.cmp(&b.name));
    } else {
        let sort = &sort_fields[0];
        const VALID: &[&str] = &["name", "created_at"];
        if !VALID.contains(&sort.field.as_str()) {
            return Err(err_bad(format!(
                "Invalid sort field '{}'. Valid fields: {}",
                sort.field,
                VALID.join(", ")
            )));
        }
        match sort.field.as_str() {
            "name" => {
                if sort.ascending {
                    domains.sort_by(|a, b| a.name.cmp(&b.name));
                } else {
                    domains.sort_by(|a, b| b.name.cmp(&a.name));
                }
            }
            "created_at" => {
                if sort.ascending {
                    domains.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                } else {
                    domains.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                }
            }
            _ => unreachable!(),
        }
    }

    let limit = query.limit;
    let total = domains.len();
    let offset: usize = query.parse_offset_cursor().map_err(err_bad)?.min(total);
    let paginated: Vec<_> = domains.into_iter().skip(offset).take(limit).collect();
    let count = paginated.len();
    let next_offset = offset + count;
    let has_more = next_offset < total;

    let pagination = if has_more {
        ListPagination::with_cursor(format!("offset:{next_offset}"), count).with_total(total)
    } else {
        ListPagination::last_page(count).with_total(total)
    };
    Ok(HttpResponse::Ok().json(ListResponse::new(paginated, pagination)))
}

/// GET /gov/domains/{domain_id} — Get a specific governance domain.
pub async fn get_domain<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = GovernanceDomainId(domain_id.into_inner());
    let domain = ctx
        .manager
        .get_domain(&id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Domain not found: {}", id.0)))?;

    Ok(HttpResponse::Ok().json(domain))
}

/// POST /gov/domains/{domain_id}/members — Add a member to a domain.
pub async fn add_domain_member<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    body: web::Json<AddDomainMemberRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let caller_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let domain_id_str = domain_id.into_inner();
    val::validate_domain_id(&domain_id_str)?;

    let member_did = parse_did(&body.did, "Invalid member DID")?;
    let gov_domain_id = GovernanceDomainId(domain_id_str.clone());

    let domain = ctx
        .manager
        .get_domain(&gov_domain_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Domain not found: {domain_id_str}")))?;

    let caller_is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(&caller_did),
        icn_governance::MembershipSource::TrustThreshold(_) => true,
    };
    if !caller_is_member {
        return Err(err_forbidden(format!(
            "Only domain members can add members to domain '{domain_id_str}'"
        )));
    }

    ctx.manager
        .update_domain_membership(gov_domain_id, member_did.clone(), MembershipAction::Add)
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "member_added",
        "domain_id": domain_id_str,
        "member_did": member_did.to_string()
    })))
}

/// DELETE /gov/domains/{domain_id}/members — Remove a member from a domain.
pub async fn remove_domain_member<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    body: web::Json<RemoveDomainMemberRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let caller_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let domain_id_str = domain_id.into_inner();
    val::validate_domain_id(&domain_id_str)?;

    let member_did = parse_did(&body.did, "Invalid member DID")?;
    let gov_domain_id = GovernanceDomainId(domain_id_str.clone());

    let domain = ctx
        .manager
        .get_domain(&gov_domain_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Domain not found: {domain_id_str}")))?;

    let caller_is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(&caller_did),
        icn_governance::MembershipSource::TrustThreshold(_) => true,
    };
    if !caller_is_member {
        return Err(err_forbidden(format!(
            "Only domain members can remove members from domain '{domain_id_str}'"
        )));
    }

    ctx.manager
        .update_domain_membership(gov_domain_id, member_did.clone(), MembershipAction::Remove)
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "member_removed",
        "domain_id": domain_id_str,
        "member_did": member_did.to_string()
    })))
}

// ============================================================================
// Proposal handlers
// ============================================================================

/// POST /gov/proposals — Create a new proposal.
pub async fn create_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<CreateProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let proposer_did = parse_did(&claims.sub, "Invalid DID in token")?;

    val::validate_domain_id(&req.domain_id)?;
    val::validate_proposal_title(&req.title)?;
    val::validate_proposal_description(&req.description)?;

    let payload = match &req.payload {
        ProposalPayloadRequest::Text { body } => {
            if body.is_empty() || body.trim().is_empty() {
                return Err(err_bad(
                    "Proposal text body cannot be empty or whitespace-only",
                ));
            }
            if body.len() > val::MAX_PROPOSAL_DESCRIPTION_LEN {
                return Err(err_bad(format!(
                    "Proposal text body exceeds maximum length of {} characters",
                    val::MAX_PROPOSAL_DESCRIPTION_LEN
                )));
            }
            ProposalPayload::Text { body: body.clone() }
        }
        ProposalPayloadRequest::Budget {
            amount,
            recipient,
            currency,
            purpose,
        } => {
            val::validate_payment_amount(*amount)?;
            val::validate_currency(currency)?;
            if purpose.is_empty() || purpose.trim().is_empty() {
                return Err(err_bad("Budget purpose cannot be empty or whitespace-only"));
            }
            if purpose.len() > val::MAX_PROPOSAL_DESCRIPTION_LEN {
                return Err(err_bad(format!(
                    "Budget purpose exceeds maximum length of {} characters",
                    val::MAX_PROPOSAL_DESCRIPTION_LEN
                )));
            }
            let recipient_did = parse_did(recipient, "Invalid recipient DID")?;
            ProposalPayload::Budget {
                amount: *amount,
                recipient: recipient_did,
                currency: currency.clone(),
                purpose: purpose.clone(),
            }
        }
        ProposalPayloadRequest::Membership { action, did } => {
            let member_did = parse_did(did, "Invalid member DID")?;
            let membership_action = match action.to_lowercase().as_str() {
                "add" => MembershipAction::Add,
                "remove" => MembershipAction::Remove,
                _ => return Err(err_bad(format!("Invalid action: {action}"))),
            };
            ProposalPayload::Membership {
                action: membership_action,
                member: member_did,
            }
        }
        ProposalPayloadRequest::ConfigChange { key, value } => {
            if key.is_empty() || key.trim().is_empty() {
                return Err(err_bad("Config key cannot be empty or whitespace-only"));
            }
            if key.len() > val::MAX_GOVERNANCE_MODEL_LEN {
                return Err(err_bad(format!(
                    "Config key exceeds maximum length of {} characters",
                    val::MAX_GOVERNANCE_MODEL_LEN
                )));
            }
            if value.is_empty() || value.trim().is_empty() {
                return Err(err_bad("Config value cannot be empty or whitespace-only"));
            }
            if value.len() > val::MAX_PROPOSAL_DESCRIPTION_LEN {
                return Err(err_bad(format!(
                    "Config value exceeds maximum length of {} characters",
                    val::MAX_PROPOSAL_DESCRIPTION_LEN
                )));
            }
            let new_config = serde_json::json!({ key: value }).to_string();
            ProposalPayload::ConfigChange { new_config }
        }
        ProposalPayloadRequest::Charter {
            charter_id,
            charter_yaml,
        } => {
            if charter_id.is_empty() || charter_id.trim().is_empty() {
                return Err(err_bad("Charter ID cannot be empty"));
            }
            if charter_yaml.is_empty() || charter_yaml.trim().is_empty() {
                return Err(err_bad("Charter YAML cannot be empty"));
            }
            ProposalPayload::Charter {
                charter_id: charter_id.clone(),
                charter_yaml: charter_yaml.clone(),
            }
        }
    };

    let scope = match req.scope {
        Some(ProposalScopeRequest::Federation { ref federation_id }) => {
            ProposalScope::Federation(federation_id.clone())
        }
        Some(ProposalScopeRequest::Local) | None => ProposalScope::Local,
    };

    let domain_id = GovernanceDomainId(req.domain_id.clone());
    let suggested_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));

    let proposal_id = ctx
        .manager
        .create_proposal(
            suggested_id,
            domain_id,
            proposer_did.clone(),
            req.title.clone(),
            req.description.clone(),
            payload,
            scope,
        )
        .await
        .map_err(anyhow_to_api)?;

    let payload_type = match &req.payload {
        ProposalPayloadRequest::Text { .. } => "text",
        ProposalPayloadRequest::Budget { .. } => "budget",
        ProposalPayloadRequest::Membership { .. } => "membership",
        ProposalPayloadRequest::ConfigChange { .. } => "config_change",
        ProposalPayloadRequest::Charter { .. } => "charter",
    };

    ctx.emitter.emit_proposal_created(
        &proposal_id.0,
        &req.domain_id,
        &proposer_did.to_string(),
        &req.title,
        payload_type,
    );

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Proposal creation succeeded but proposal not found"))?;

    Ok(HttpResponse::Created().json(proposal))
}

/// GET /gov/proposals — List proposals (with pagination and filters).
pub async fn list_proposals<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    query: web::Query<ListQuery>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let query = query.into_inner().validate();
    query.validate_filters().map_err(err_bad)?;

    let mut proposals = ctx.manager.list_proposals().await.map_err(anyhow_to_api)?;

    if let Some(domain_id) = query.filter("domain_id") {
        let filter_domain = GovernanceDomainId(domain_id.to_string());
        proposals.retain(|p| p.domain_id == filter_domain);
    }

    if let Some(state) = query.filter("state") {
        match state {
            "draft" => {
                proposals.retain(|p| matches!(p.state, icn_governance::ProposalState::Draft))
            }
            "open" => {
                proposals.retain(|p| matches!(p.state, icn_governance::ProposalState::Open { .. }))
            }
            "closed" => proposals.retain(|p| {
                matches!(
                    p.state,
                    icn_governance::ProposalState::Accepted { .. }
                        | icn_governance::ProposalState::Rejected { .. }
                        | icn_governance::ProposalState::NoQuorum { .. }
                )
            }),
            _ => {
                return Err(err_bad(format!(
                    "Invalid state filter: {state}. Valid: draft, open, closed"
                )))
            }
        }
    }

    if let Some(scope) = query.filter("scope") {
        match scope {
            "local" => proposals.retain(|p| matches!(p.scope, icn_governance::ProposalScope::Local)),
            "federation" => proposals.retain(|p| matches!(p.scope, icn_governance::ProposalScope::Federation(_))),
            other => proposals.retain(|p| {
                matches!(&p.scope, icn_governance::ProposalScope::Federation(id) if id.as_str() == other)
            }),
        }
    }

    let sort_fields = query.sort_fields();
    if sort_fields.is_empty() {
        proposals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    } else {
        let sort = &sort_fields[0];
        const VALID: &[&str] = &["created_at", "title"];
        if !VALID.contains(&sort.field.as_str()) {
            return Err(err_bad(format!(
                "Invalid sort field '{}'. Valid fields: {}",
                sort.field,
                VALID.join(", ")
            )));
        }
        match sort.field.as_str() {
            "created_at" => {
                if sort.ascending {
                    proposals.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                } else {
                    proposals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                }
            }
            "title" => {
                if sort.ascending {
                    proposals.sort_by(|a, b| a.title.cmp(&b.title));
                } else {
                    proposals.sort_by(|a, b| b.title.cmp(&a.title));
                }
            }
            _ => unreachable!(),
        }
    }

    let limit = query.limit;
    let total = proposals.len();
    let offset: usize = query.parse_offset_cursor().map_err(err_bad)?.min(total);
    let paginated: Vec<_> = proposals.into_iter().skip(offset).take(limit).collect();
    let count = paginated.len();
    let next_offset = offset + count;
    let has_more = next_offset < total;

    let pagination = if has_more {
        ListPagination::with_cursor(format!("offset:{next_offset}"), count).with_total(total)
    } else {
        ListPagination::last_page(count).with_total(total)
    };
    Ok(HttpResponse::Ok().json(ListResponse::new(paginated, pagination)))
}

/// GET /gov/proposals/{proposal_id} — Get a specific proposal.
pub async fn get_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());
    let proposal = ctx
        .manager
        .get_proposal(&id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Proposal not found: {}", id.0)))?;

    Ok(HttpResponse::Ok().json(proposal))
}

/// POST /gov/proposals/{proposal_id}/open — Open a proposal for voting.
pub async fn open_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
    req: web::Json<OpenProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let requester_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let proposal_id = ProposalId(proposal_id.into_inner());

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Proposal not found: {}", proposal_id.0)))?;

    let domain = ctx
        .manager
        .get_domain(&proposal.domain_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal(format!("Domain not found: {}", proposal.domain_id.0)))?;

    let is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(&requester_did),
        icn_governance::MembershipSource::TrustThreshold(_) => true,
    };
    if !is_member {
        return Err(err_forbidden(format!(
            "Only domain members can open proposals (not a member of domain '{}')",
            proposal.domain_id.0
        )));
    }

    let voting_period_seconds = if let Some(period) = req.voting_period_seconds {
        if period == 0 {
            return Err(err_bad("Voting period must be greater than 0"));
        }
        if period > val::MAX_VOTING_PERIOD_SECONDS {
            return Err(err_bad(format!(
                "Voting period exceeds maximum of {} seconds (1 year)",
                val::MAX_VOTING_PERIOD_SECONDS
            )));
        }
        period
    } else {
        86400 * 7 // Default 7 days
    };

    ctx.manager
        .open_proposal(proposal_id.clone(), voting_period_seconds)
        .await
        .map_err(anyhow_to_api)?;

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Proposal opening succeeded but proposal not found"))?;

    let closes_at = if let icn_governance::ProposalState::Open {
        opened_at: _,
        closes_at,
    } = proposal.state
    {
        closes_at
    } else {
        current_time_secs() + voting_period_seconds
    };

    ctx.emitter
        .emit_proposal_opened(&proposal_id.0, &proposal.domain_id.0, closes_at);

    Ok(HttpResponse::Ok().json(proposal))
}

/// POST /gov/proposals/{proposal_id}/close — Close a proposal and finalize voting.
pub async fn close_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let requester_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let proposal_id = ProposalId(proposal_id.into_inner());

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Proposal not found: {}", proposal_id.0)))?;

    let domain = ctx
        .manager
        .get_domain(&proposal.domain_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal(format!("Domain not found: {}", proposal.domain_id.0)))?;

    let is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(&requester_did),
        icn_governance::MembershipSource::TrustThreshold(_) => true,
    };
    if !is_member {
        return Err(err_forbidden(format!(
            "Only domain members can close proposals (not a member of domain '{}')",
            proposal.domain_id.0
        )));
    }

    ctx.manager
        .close_proposal(proposal_id.clone())
        .await
        .map_err(anyhow_to_api)?;

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Proposal closing succeeded but proposal not found"))?;

    let outcome = match &proposal.state {
        icn_governance::ProposalState::Accepted { .. } => "accepted",
        icn_governance::ProposalState::Rejected { .. } => "rejected",
        icn_governance::ProposalState::NoQuorum { .. } => "no_quorum",
        _ => "unknown",
    };

    // On charter acceptance: deploy the document to the charter policy oracle.
    if outcome == "accepted" {
        if let icn_governance::ProposalPayload::Charter {
            ref charter_id,
            ref charter_yaml,
        } = proposal.payload
        {
            if let Some(hook) = &ctx.on_charter_accepted {
                hook(charter_id.clone(), charter_yaml.clone());
            }
        }

        // General acceptance hook: translate to GovernanceEffect here so the
        // gateway never needs to import icn_governance types (meaning-firewall).
        if let Some(hook) = &ctx.on_proposal_accepted {
            let effect = match &proposal.payload {
                icn_governance::ProposalPayload::FreezeMember {
                    member,
                    reason,
                    duration_seconds,
                } => GovernanceEffect::FreezeMember {
                    proposal_id: proposal.id.0.clone(),
                    domain_id: proposal.domain_id.0.clone(),
                    member: member.clone(),
                    reason: reason.clone(),
                    duration_seconds: *duration_seconds,
                },
                icn_governance::ProposalPayload::UnfreezeMember { member, reason } => {
                    GovernanceEffect::UnfreezeMember {
                        proposal_id: proposal.id.0.clone(),
                        domain_id: proposal.domain_id.0.clone(),
                        member: member.clone(),
                        reason: reason.clone(),
                    }
                }
                _ => GovernanceEffect::Unhandled {
                    proposal_id: proposal.id.0.clone(),
                    payload_type: proposal.payload.type_name().to_owned(),
                },
            };
            hook(effect);
        }
    }

    ctx.emitter
        .emit_proposal_closed(&proposal_id.0, &proposal.domain_id.0, outcome);

    Ok(HttpResponse::Ok().json(proposal))
}

/// POST /gov/proposals/{proposal_id}/vote — Cast a vote on a proposal.
pub async fn cast_vote<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
    req: web::Json<CastVoteRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let voter_did = parse_did(&claims.sub, "Invalid DID in token")?;

    val::validate_vote_comment(&req.comment)?;

    let choice = match req.choice.to_lowercase().as_str() {
        "for" => VoteChoice::For,
        "against" => VoteChoice::Against,
        "abstain" => VoteChoice::Abstain,
        _ => return Err(err_bad(format!("Invalid vote choice: {}", req.choice))),
    };

    let proposal_id = ProposalId(proposal_id.into_inner());
    ctx.manager
        .cast_vote(
            proposal_id.clone(),
            voter_did.clone(),
            choice,
            req.comment.clone(),
        )
        .await
        .map_err(anyhow_to_api)?;

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Vote cast succeeded but proposal not found"))?;

    ctx.emitter.emit_vote_cast(
        &proposal_id.0,
        &proposal.domain_id.0,
        &voter_did.to_string(),
        &req.choice,
    );

    Ok(HttpResponse::Ok().json(proposal))
}

/// GET /gov/proposals/{proposal_id}/tally — Get vote tally.
pub async fn get_vote_tally<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());
    let _ = ctx
        .manager
        .get_proposal(&id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Proposal not found: {}", id.0)))?;

    let tally = ctx
        .manager
        .get_vote_tally(&id)
        .await
        .map_err(anyhow_to_api)?;

    #[derive(serde::Serialize)]
    struct VoteTallyResponse {
        for_votes: usize,
        against_votes: usize,
        abstain_votes: usize,
        total_votes: usize,
    }

    Ok(HttpResponse::Ok().json(VoteTallyResponse {
        for_votes: tally.for_votes,
        against_votes: tally.against_votes,
        abstain_votes: tally.abstain_votes,
        total_votes: tally.total_votes(),
    }))
}

/// GET /gov/proposals/{proposal_id}/chain — Get the full provenance chain for a proposal.
///
/// Returns the governance decision receipt and any linked allocation receipts,
/// enabling independent verification of the governance→economics binding (INV-5).
///
/// Response includes:
/// - `governance_receipt`: The decision receipt (present if proposal is closed)
/// - `allocations`: AllocationReceipts created when the proposal was accepted
/// - `chain_complete`: Whether the chain is complete for this proposal type
///
/// A `chain_complete: false` response means the system cannot answer
/// "what economic effect did this decision have?" — a provenance gap.
pub async fn get_chain<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());
    let chain = ctx.manager.get_chain(&id).await.map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(chain))
}

/// GET /gov/proposals/{proposal_id}/proof — Get cryptographic proof of proposal outcome.
pub async fn get_proof<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());
    let proof = ctx
        .manager
        .get_proof(&id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("No proof found for proposal: {}", id.0)))?;

    if !proof.verify_receipt() {
        tracing::warn!("Invalid proof binding for proposal {} — not serving", id.0);
        return Err(err_not_found(format!(
            "No valid proof found for proposal: {}",
            id.0
        )));
    }
    if proof.attestations.is_empty() {
        tracing::warn!(
            "Proof for proposal {} has no attestations — not serving",
            id.0
        );
        return Err(err_not_found(format!(
            "No valid proof found for proposal: {}",
            id.0
        )));
    }

    for attestation in &proof.attestations {
        if attestation.decision_hash != proof.receipt.decision_hash {
            tracing::warn!(
                "Proof for proposal {} has mismatched attestation decision hash — not serving",
                id.0
            );
            return Err(err_not_found(format!(
                "No valid proof found for proposal: {}",
                id.0
            )));
        }
        let verifying_key = attestation
            .signer_did
            .parse::<Did>()
            .and_then(|did| did.to_verifying_key())
            .map_err(|_| err_not_found(format!("No valid proof found for proposal: {}", id.0)))?;
        if !attestation.verify(&verifying_key) {
            tracing::warn!(
                "Proof for proposal {} has invalid attestation signature — not serving",
                id.0
            );
            return Err(err_not_found(format!(
                "No valid proof found for proposal: {}",
                id.0
            )));
        }
    }

    Ok(HttpResponse::Ok().json(proof))
}

// ============================================================================
// Discussion handlers
// ============================================================================

/// GET /gov/proposals/{proposal_id}/discussion — Get full discussion.
pub async fn get_discussion<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());
    let discussion = ctx
        .manager
        .get_discussion(&id)
        .await
        .map_err(anyhow_to_api)?;

    match discussion {
        Some(d) => {
            let comments = d.comments.into_iter().map(comment_to_response).collect();
            Ok(HttpResponse::Ok().json(DiscussionResponse {
                proposal_id: d.proposal_id.0,
                comments,
                participant_count: d.participant_count,
                last_activity_at: d.last_activity_at,
            }))
        }
        None => Ok(HttpResponse::Ok().json(DiscussionResponse {
            proposal_id: id.0,
            comments: vec![],
            participant_count: 0,
            last_activity_at: 0,
        })),
    }
}

/// POST /gov/proposals/{proposal_id}/discussion/comments — Add a comment.
pub async fn add_comment<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
    req: web::Json<AddCommentRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let author = parse_did(&claims.sub, "Invalid DID in token")?;

    if req.content.trim().is_empty() {
        return Err(err_bad("Comment content cannot be empty"));
    }
    if req.content.len() > 10_000 {
        return Err(err_bad(
            "Comment content exceeds maximum length of 10000 characters",
        ));
    }

    let id = ProposalId(proposal_id.into_inner());
    let mut comment = icn_governance::Comment::new(id.clone(), author, req.content.clone());
    if let Some(ref parent_id) = req.parent_id {
        comment.parent_id = Some(icn_governance::CommentId(parent_id.clone()));
    }

    let comment_id = ctx
        .manager
        .add_comment(comment.clone())
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().json(CommentResponse {
        id: comment_id.0,
        proposal_id: comment.proposal_id.0,
        author: comment.author.to_string(),
        content: comment.content,
        parent_id: comment.parent_id.map(|p| p.0),
        created_at: comment.created_at,
        updated_at: comment.updated_at,
        reactions: std::collections::HashMap::new(),
        is_edited: comment.is_edited,
        is_deleted: comment.is_deleted,
    }))
}

/// GET /gov/proposals/{proposal_id}/discussion/comments — List comments.
pub async fn list_comments<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
    query: web::Query<ListCommentsQuery>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let comments = ctx
        .manager
        .list_comments(&id, limit, offset)
        .await
        .map_err(anyhow_to_api)?;
    let total = ctx
        .manager
        .count_comments(&id)
        .await
        .map_err(anyhow_to_api)?;

    let responses: Vec<CommentResponse> = comments.into_iter().map(comment_to_response).collect();

    Ok(HttpResponse::Ok().json(ListCommentsResponse {
        comments: responses,
        total,
        limit,
        offset,
    }))
}

/// PUT /gov/proposals/{proposal_id}/discussion/comments/{comment_id} — Edit a comment.
pub async fn edit_comment<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<EditCommentRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let editor = parse_did(&claims.sub, "Invalid DID in token")?;

    let (_proposal_id, comment_id_str) = path.into_inner();

    if req.content.trim().is_empty() {
        return Err(err_bad("Comment content cannot be empty"));
    }

    let comment_id = icn_governance::CommentId(comment_id_str);
    ctx.manager
        .edit_comment(&comment_id, req.content.clone(), &editor)
        .await
        .map_err(anyhow_to_api)?;

    let updated = ctx
        .manager
        .get_comment(&comment_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Comment not found after update"))?;

    Ok(HttpResponse::Ok().json(comment_to_response(updated)))
}

/// DELETE /gov/proposals/{proposal_id}/discussion/comments/{comment_id} — Delete a comment.
pub async fn delete_comment<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let deleter = parse_did(&claims.sub, "Invalid DID in token")?;

    let (_proposal_id, comment_id_str) = path.into_inner();
    let comment_id = icn_governance::CommentId(comment_id_str);

    ctx.manager
        .delete_comment(&comment_id, &deleter)
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::NoContent().finish())
}

/// POST /gov/proposals/{proposal_id}/discussion/comments/{comment_id}/reactions — Add reaction.
pub async fn add_reaction<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<AddReactionRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let reactor = parse_did(&claims.sub, "Invalid DID in token")?;

    let (_proposal_id, comment_id_str) = path.into_inner();

    if req.emoji.is_empty() || req.emoji.chars().count() > 10 {
        return Err(err_bad("Invalid emoji: must be 1-10 characters"));
    }

    let comment_id = icn_governance::CommentId(comment_id_str);
    ctx.manager
        .add_reaction(&comment_id, &reactor, &req.emoji)
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().finish())
}

/// DELETE /gov/proposals/{proposal_id}/discussion/comments/{comment_id}/reactions — Remove reaction.
pub async fn remove_reaction<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<RemoveReactionRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let reactor = parse_did(&claims.sub, "Invalid DID in token")?;

    let (_proposal_id, comment_id_str) = path.into_inner();
    let comment_id = icn_governance::CommentId(comment_id_str);

    ctx.manager
        .remove_reaction(&comment_id, &reactor, &req.emoji)
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::NoContent().finish())
}

// ============================================================================
// Delegation handlers
// ============================================================================

/// POST /gov/delegations — Create a new vote delegation.
pub async fn create_delegation<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<CreateDelegationRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let delegator_did = parse_did(&claims.sub, "Invalid DID in token")?;
    let delegate_did = parse_did(&req.delegate, "Invalid delegate DID")?;
    let scope = parse_delegation_scope(&req.scope)?;

    let mut delegation = Delegation::new(delegator_did, delegate_did, scope);
    if let Some(expires_at) = req.expires_at {
        delegation = delegation.with_expiry(expires_at);
    }

    ctx.manager
        .create_delegation(delegation.clone())
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().json(delegation_to_response(&delegation)))
}

/// GET /gov/delegations — List delegations for the authenticated user.
pub async fn list_delegations<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    query: web::Query<ListDelegationsQuery>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let caller_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let given = ctx
        .manager
        .get_delegations_from(&caller_did)
        .await
        .map_err(anyhow_to_api)?;
    let received = ctx
        .manager
        .get_delegations_to(&caller_did)
        .await
        .map_err(anyhow_to_api)?;

    let include_revoked = query.include_revoked;
    let filter = |d: &Delegation| include_revoked || d.revoked_at.is_none();

    Ok(HttpResponse::Ok().json(DelegationListResponse {
        given: given
            .iter()
            .filter(|d| filter(d))
            .map(delegation_to_response)
            .collect(),
        received: received
            .iter()
            .filter(|d| filter(d))
            .map(delegation_to_response)
            .collect(),
    }))
}

/// DELETE /gov/delegations/{delegation_id} — Revoke a delegation.
pub async fn revoke_delegation<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    delegation_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let caller_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let id = DelegationId(delegation_id.into_inner());
    let delegation = ctx
        .manager
        .get_delegation(&id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Delegation not found: {}", id.0)))?;

    if delegation.delegator != caller_did {
        return Err(err_forbidden("Only the delegator can revoke a delegation"));
    }

    let now = current_time_secs();
    ctx.manager
        .revoke_delegation(&id, now)
        .await
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::NoContent().finish())
}

// ============================================================================
// Action item handlers
// ============================================================================

/// POST /gov/domains/{domain_id}/action-items — Create an action item.
pub async fn create_action_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    req: web::Json<CreateActionItemRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let creator_did = parse_did(&claims.sub, "Invalid DID in token")?;
    let domain = GovernanceDomainId(domain_id.into_inner());

    check_domain_membership(&ctx.manager, &domain, &creator_did).await?;

    // Validate inputs
    val::validate_action_item_title(&req.title)?;
    val::validate_action_item_description(&req.description)?;
    val::validate_tags(&req.tags)?;
    val::validate_meeting_context(&req.meeting_context)?;
    val::validate_due_date(req.due_date, false)?;

    let assignee: Option<Did> = if let Some(ref s) = req.assignee {
        Some(parse_did(s, "Invalid assignee DID")?)
    } else {
        None
    };
    let linked_proposal = req.linked_proposal.as_ref().map(ProposalId::new);
    let priority = parse_priority(&req.priority)?;

    let item = ctx
        .manager
        .create_action_item(
            domain,
            req.title.clone(),
            req.description.clone(),
            creator_did,
            assignee,
            req.due_date,
            priority,
            linked_proposal,
            req.meeting_context.clone(),
            req.tags.clone(),
        )
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().json(action_item_to_response(&item)))
}

/// GET /gov/domains/{domain_id}/action-items — List action items.
pub async fn list_action_items<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    query: web::Query<ActionItemFilterParams>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let domain = GovernanceDomainId(domain_id.into_inner());
    let filter = build_action_item_filter(&query)?;

    let items = ctx
        .manager
        .list_action_items(&domain, &filter)
        .map_err(anyhow_to_api)?;

    let responses: Vec<ActionItemResponse> = items.iter().map(action_item_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

/// GET /gov/domains/{domain_id}/action-items/{item_id} — Get a specific action item.
pub async fn get_action_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    let item = ctx
        .manager
        .get_action_item(&domain, &id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Action item not found"))?;

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// PUT /gov/domains/{domain_id}/action-items/{item_id} — Update an action item.
pub async fn update_action_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<UpdateActionItemRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let user_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx.manager, &domain, &user_did).await?;

    let mut item = ctx
        .manager
        .get_action_item(&domain, &id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Action item not found"))?;

    // Ownership check before validation to prevent probing
    if item.created_by != user_did {
        return Err(ApiError::Forbidden(
            "Only the action item creator can update it".into(),
        ));
    }

    // Validate
    if let Some(ref title) = req.title {
        val::validate_action_item_title(title)?;
        item.title = title.clone();
    }
    val::validate_action_item_description(&req.description)?;
    if let Some(ref desc) = req.description {
        item.description = Some(desc.clone());
    }
    if let Some(ref assignee_str) = req.assignee {
        if assignee_str.is_empty() {
            item.assignee = None;
        } else {
            item.assignee = Some(parse_did(assignee_str, "Invalid assignee DID")?);
        }
    }
    if let Some(due_date) = req.due_date {
        val::validate_due_date(Some(due_date), due_date == 0)?;
        item.due_date = if due_date == 0 { None } else { Some(due_date) };
    }
    if let Some(ref priority) = req.priority {
        item.priority = parse_priority(priority)?;
    }
    if let Some(ref status) = req.status {
        item.status = parse_status(status)?;
    }
    if let Some(ref tags) = req.tags {
        val::validate_tags(tags)?;
        item.tags = tags.clone();
    }

    item.updated_at = current_time_secs();

    ctx.manager
        .update_action_item(&item)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// DELETE /gov/domains/{domain_id}/action-items/{item_id} — Delete an action item.
pub async fn delete_action_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let user_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx.manager, &domain, &user_did).await?;

    let item = ctx
        .manager
        .get_action_item(&domain, &id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Action item not found"))?;

    if item.created_by != user_did {
        return Err(ApiError::Forbidden(
            "Only the action item creator can delete it".into(),
        ));
    }

    ctx.manager
        .delete_action_item(&domain, &id)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::NoContent().finish())
}

/// PUT /gov/domains/{domain_id}/action-items/{item_id}/status — Update status.
pub async fn update_action_item_status<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<StatusUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let user_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx.manager, &domain, &user_did).await?;

    let existing = ctx
        .manager
        .get_action_item(&domain, &id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Action item not found"))?;

    let is_creator = existing.created_by == user_did;
    let is_assignee = existing.assignee.as_ref().is_some_and(|a| a == &user_did);
    if !is_creator && !is_assignee {
        return Err(ApiError::Forbidden(
            "Only the creator or assignee can update action item status".into(),
        ));
    }

    let new_status = parse_status(&req.status)?;
    let item = ctx
        .manager
        .update_action_item_status(&domain, &id, new_status)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// POST /gov/domains/{domain_id}/action-items/{item_id}/notes — Add a note.
pub async fn add_action_item_note<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<AddActionItemNoteRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let author_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx.manager, &domain, &author_did).await?;

    let item = ctx
        .manager
        .add_action_item_note(&domain, &id, author_did, req.content.clone())
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

// ============================================================================
// Federation proposal handlers
// ============================================================================

/// POST /gov/proposals/federation/join — Create a "join federation" proposal.
pub async fn create_join_federation_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<JoinFederationProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let (proposer_did, domain_id, title, description) =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    val::validate_federation_id(&req.federation_id)?;
    let data_sharing_level_str = val::validate_data_sharing_level(&req.terms.data_sharing_level)?;
    let dispute_resolution_str = val::validate_dispute_resolution(&req.terms.dispute_resolution)?;
    val::validate_trust_score(req.terms.min_trust_threshold)?;

    let data_sharing_level = parse_data_sharing_level(&data_sharing_level_str)?;
    let dispute_resolution = parse_dispute_resolution(&dispute_resolution_str)?;

    let terms = FederationTerms {
        min_trust_threshold: req.terms.min_trust_threshold,
        governance_binding: req.terms.governance_binding,
        data_sharing_level,
        dispute_resolution,
    };
    let fed_proposal = FederationProposal::JoinFederation {
        federation_id: req.federation_id.clone(),
        terms,
        sponsor_coop_id: req.sponsor_coop_id.clone(),
    };

    create_federation_proposal_impl(
        &ctx,
        proposer_did,
        domain_id,
        title,
        description,
        fed_proposal,
    )
    .await
}

/// POST /gov/proposals/federation/leave — Create a "leave federation" proposal.
pub async fn create_leave_federation_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<LeaveFederationProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let (proposer_did, domain_id, title, description) =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    val::validate_federation_id(&req.federation_id)?;
    val::validate_reason(&req.reason)?;
    val::validate_grace_period_days(req.grace_period_days)?;

    let fed_proposal = FederationProposal::LeaveFederation {
        federation_id: req.federation_id.clone(),
        reason: req.reason.clone(),
        grace_period_days: req.grace_period_days,
    };

    create_federation_proposal_impl(
        &ctx,
        proposer_did,
        domain_id,
        title,
        description,
        fed_proposal,
    )
    .await
}

/// POST /gov/proposals/federation/clearing/establish — Create an "establish clearing" proposal.
pub async fn create_establish_clearing_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<EstablishClearingProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let (proposer_did, domain_id, title, description) =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    val::validate_federation_id(&req.partner_coop_id)?;
    val::validate_max_imbalance(req.max_imbalance)?;
    val::validate_currency(&req.currency)?;
    let settlement_interval_str = val::validate_settlement_interval(&req.settlement_interval)?;

    let partner_coop_did = parse_did(&req.partner_coop_did, "Invalid partner_coop_did")?;
    let settlement_interval = parse_settlement_interval(&settlement_interval_str)?;

    let fed_proposal = FederationProposal::EstablishClearing {
        partner_coop_id: req.partner_coop_id.clone(),
        partner_coop_did,
        max_imbalance: req.max_imbalance,
        settlement_interval,
        currency: req.currency.clone(),
    };

    create_federation_proposal_impl(
        &ctx,
        proposer_did,
        domain_id,
        title,
        description,
        fed_proposal,
    )
    .await
}

/// POST /gov/proposals/federation/clearing/terminate — Create a "terminate clearing" proposal.
pub async fn create_terminate_clearing_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<TerminateClearingProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let (proposer_did, domain_id, title, description) =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    val::validate_federation_id(&req.partner_coop_id)?;
    val::validate_reason(&req.reason)?;

    let fed_proposal = FederationProposal::TerminateClearing {
        partner_coop_id: req.partner_coop_id.clone(),
        reason: req.reason.clone(),
    };

    create_federation_proposal_impl(
        &ctx,
        proposer_did,
        domain_id,
        title,
        description,
        fed_proposal,
    )
    .await
}

/// POST /gov/proposals/federation/vouch — Create a "vouch for cooperative" proposal.
pub async fn create_vouch_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<VouchProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let (proposer_did, domain_id, title, description) =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    val::validate_federation_id(&req.target_coop_id)?;
    val::validate_trust_score(req.trust_score)?;
    val::validate_context(&req.context)?;
    val::validate_evidence(&req.evidence)?;

    let target_coop_did = parse_did(&req.target_coop_did, "Invalid target_coop_did")?;

    let fed_proposal = FederationProposal::VouchForCooperative {
        target_coop_id: req.target_coop_id.clone(),
        target_coop_did,
        trust_score: req.trust_score,
        context: req.context.clone(),
        evidence: req.evidence.clone(),
    };

    create_federation_proposal_impl(
        &ctx,
        proposer_did,
        domain_id,
        title,
        description,
        fed_proposal,
    )
    .await
}

/// POST /gov/proposals/federation/vouch/revoke — Create a "revoke vouch" proposal.
pub async fn create_revoke_vouch_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<RevokeVouchProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let (proposer_did, domain_id, title, description) =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    val::validate_federation_id(&req.target_coop_id)?;
    val::validate_reason(&req.reason)?;

    let fed_proposal = FederationProposal::RevokeVouch {
        target_coop_id: req.target_coop_id.clone(),
        reason: req.reason.clone(),
    };

    create_federation_proposal_impl(
        &ctx,
        proposer_did,
        domain_id,
        title,
        description,
        fed_proposal,
    )
    .await
}

/// POST /gov/proposals/federation/policy — Create an "update federation policy" proposal.
pub async fn create_update_federation_policy_proposal<
    E: GovernanceEventEmitter + Clone + 'static,
>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<UpdateFederationPolicyProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let (proposer_did, domain_id, title, description) =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    val::validate_auto_accept_threshold(req.auto_accept_vouch_threshold)?;
    val::validate_trust_decay_factor(req.trust_decay_factor)?;
    val::validate_max_attestations_per_minute(req.max_attestations_per_minute)?;

    if req.auto_accept_vouch_threshold.is_none()
        && req.trust_decay_factor.is_none()
        && req.max_attestations_per_minute.is_none()
    {
        return Err(err_bad("At least one policy field must be provided"));
    }

    let fed_proposal = FederationProposal::UpdateFederationPolicy {
        auto_accept_vouch_threshold: req.auto_accept_vouch_threshold,
        trust_decay_factor: req.trust_decay_factor,
        max_attestations_per_minute: req.max_attestations_per_minute,
    };

    create_federation_proposal_impl(
        &ctx,
        proposer_did,
        domain_id,
        title,
        description,
        fed_proposal,
    )
    .await
}

// ============================================================================
// Tests — governance → execution bridge
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use actix_web::dev::Service as _;
    use actix_web::{test, web, App, HttpMessage};
    use icn_governance::{
        GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource, ProposalId,
        ProposalPayload, ProposalScope, VoteChoice,
    };
    use icn_http_kit::auth::BasicClaims;
    use icn_identity::Did;

    use super::*;
    use crate::events::NoopEventEmitter;
    use crate::http::configure::{GovernanceContext, GovernanceEffect, ProposalAcceptedHook};
    use crate::manager::GovernanceManager;

    fn test_did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    /// Build a test app that injects the given claims into every request
    /// extension — bypasses JWT validation without touching production code.
    macro_rules! test_app {
        ($ctx:expr, $caller_did:expr) => {{
            let ctx_data = web::Data::new($ctx);
            let caller_did_str = $caller_did.to_string();
            test::init_service(
                App::new()
                    .app_data(ctx_data)
                    .wrap_fn(move |req, srv| {
                        req.extensions_mut().insert(BasicClaims {
                            sub: caller_did_str.clone(),
                            scope: Some("governance:write".to_string()),
                        });
                        srv.call(req)
                    })
                    .route(
                        "/proposals/{proposal_id}/close",
                        web::post().to(close_proposal::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    /// Helper: create domain + FreezeMember proposal and open it, returning
    /// `(manager, proposal_id)`.
    async fn setup_freeze_proposal(
        coop_id: &str,
        member_did: Did,
        target_did: Did,
        params: GovernanceParams,
    ) -> (Arc<GovernanceManager>, ProposalId) {
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId(coop_id.to_string());

        mgr.create_domain(
            domain_id.clone(),
            format!("{coop_id} coop"),
            "cooperative_default".to_string(),
            params,
            MembershipConfig {
                source: MembershipSource::StaticList(vec![member_did.clone()]),
            },
        )
        .await
        .unwrap();

        let proposal_id = ProposalId("freeze-proposal-1".to_string());
        mgr.create_proposal(
            proposal_id.clone(),
            domain_id.clone(),
            member_did,
            "Freeze disruptive member".to_string(),
            "Emergency action — account compromise suspected".to_string(),
            ProposalPayload::FreezeMember {
                member: target_did,
                reason: "account compromise suspected".to_string(),
                duration_seconds: Some(604_800),
            },
            ProposalScope::Local,
        )
        .await
        .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86_400)
            .await
            .unwrap();
        (mgr, proposal_id)
    }

    /// Proves: when `close_proposal` finalises a `FreezeMember` proposal as
    /// `Accepted`, the `on_proposal_accepted` hook fires and delivers the
    /// correct member DID, reason, and duration.
    ///
    /// Uses actix-web's test runtime so the actual HTTP handler executes,
    /// including auth extraction, domain-membership guard, and hook dispatch.
    #[tokio::test]
    async fn hook_fires_with_correct_payload_on_acceptance() {
        let captured: Arc<Mutex<Vec<GovernanceEffect>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        let hook: ProposalAcceptedHook = Arc::new(move |effect| {
            captured_clone.lock().unwrap().push(effect);
        });

        let member_did = test_did(1);
        let target_did = test_did(2);

        // GovernanceParams(quorum=0, approval=0) → any close is Accepted
        let (mgr, proposal_id) = setup_freeze_proposal(
            "alpha",
            member_did.clone(),
            target_did.clone(),
            GovernanceParams::new(0, 0, 86_400),
        )
        .await;

        let ctx = GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: Some(hook),
        };

        let app = test_app!(ctx, member_did);

        let req = test::TestRequest::post()
            .uri(&format!("/proposals/{}/close", proposal_id.0))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(
            resp.status().as_u16(),
            200,
            "close_proposal should return 200 OK"
        );

        let captured = captured.lock().unwrap();
        assert_eq!(
            captured.len(),
            1,
            "hook must fire exactly once on acceptance"
        );

        match &captured[0] {
            GovernanceEffect::FreezeMember {
                member,
                reason,
                duration_seconds,
                ..
            } => {
                assert_eq!(member, &target_did, "hook must receive correct target DID");
                assert_eq!(reason, "account compromise suspected");
                assert_eq!(*duration_seconds, Some(604_800));
            }
            other => panic!("expected GovernanceEffect::FreezeMember, got {other:?}"),
        }
    }

    /// Proves: hook is NOT fired when the proposal closes as `Rejected`.
    ///
    /// GovernanceParams(quorum=0, approval=100) with a vote AGAINST → Rejected.
    #[tokio::test]
    async fn hook_not_fired_on_rejection() {
        let fired = Arc::new(Mutex::new(false));
        let fired_clone = fired.clone();

        let hook: ProposalAcceptedHook = Arc::new(move |_| {
            *fired_clone.lock().unwrap() = true;
        });

        let member_did = test_did(3);
        let target_did = test_did(4);

        // approval=100 and one "Against" vote → Rejected
        let (mgr, proposal_id) = setup_freeze_proposal(
            "beta",
            member_did.clone(),
            target_did,
            GovernanceParams::new(0, 100, 86_400),
        )
        .await;

        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::Against,
            None,
        )
        .await
        .unwrap();

        let ctx = GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: Some(hook),
        };

        let app = test_app!(ctx, member_did);

        let req = test::TestRequest::post()
            .uri(&format!("/proposals/{}/close", proposal_id.0))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status().as_u16(), 200);
        assert!(
            !*fired.lock().unwrap(),
            "hook must NOT fire when proposal is rejected"
        );
    }
}
