//! Governance HTTP handlers.
//!
//! All handlers are generic over `E: GovernanceEventEmitter` and receive
//! shared context via `web::Data<GovernanceContext<E>>`. Route registration
//! lives in `super::configure`.

use actix_web::{web, HttpRequest, HttpResponse};
use icn_federation::SettlementInterval;
use icn_governance::{
    ActionItemFilter, ActionItemId, ActionItemPriority, ActionItemStatus, ActivityId, ActivityKind,
    ActivityStatus, AttendanceStatus, DataSharingLevel, Delegation, DelegationId, DelegationScope,
    DisputeResolutionMethod, FederationProposal, FederationTerms, GovernanceDomainId,
    GovernanceParams, MeetingId, MeetingRole, MeetingStatus, MembershipAction, MembershipConfig,
    ProposalId, ProposalPayload, ProposalScope, StructureId, StructureKind, StructureStatus,
    VoteChoice,
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
    let mut params = GovernanceParams::new(
        req.quorum_percent,
        req.approval_percent,
        voting_period_seconds,
    );
    if let Some(ref mode) = req.decision_mode {
        params.decision_mode = match mode.as_str() {
            "consent" => icn_governance::DecisionMode::Consent,
            _ => icn_governance::DecisionMode::Majority,
        };
    }
    params.max_objections = req.max_objections;
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

    // Commons standing gate: if a checker is wired, the proposer must hold active
    // Member status in the target domain's jurisdiction. This enforces the principle
    // that institutional authority constrains governance participation — not just
    // the reverse. The checker is `None` in test setups that don't wire commons.
    if let Some(ref checker) = ctx.member_checker {
        if !checker(proposer_did.clone(), req.domain_id.clone()).await {
            return Err(err_forbidden(format!(
                "proposer {} does not have active Member standing in domain {}; \
                 join the jurisdiction before submitting proposals",
                proposer_did, req.domain_id
            )));
        }
    }

    // Suspension gate: proposer must not be suspended. An accepted FreezeMember
    // proposal sets MemberStatus::Suspended in CoopStore. Suspended members may
    // retain commons standing (member_checker can pass) but may not initiate
    // governance actions until unfrozen.
    if let Some(ref checker) = ctx.suspension_checker {
        if checker(proposer_did.clone(), req.domain_id.clone()).await {
            return Err(err_forbidden(format!(
                "proposer {} is suspended in domain {}; \
                 suspended members may not submit proposals",
                proposer_did, req.domain_id
            )));
        }
    }

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

    // Convert action item specs from API model to domain model, applying the
    // same validation as the action-item endpoints.
    let action_specs: Vec<icn_governance::ActionItemSpec> = req
        .action_items_on_accept
        .iter()
        .map(|s| -> Result<icn_governance::ActionItemSpec, ApiError> {
            val::validate_action_item_title(&s.title)?;
            val::validate_action_item_description(&s.description)?;
            val::validate_tags(&s.tags)?;

            let priority = match s.priority.as_deref() {
                Some(p) => parse_priority(p)?,
                None => icn_governance::ActionItemPriority::Medium,
            };

            let assignee = match s.assignee.as_deref() {
                Some(d) => Some(parse_did(d, "action_items_on_accept[].assignee")?),
                None => None,
            };

            Ok(icn_governance::ActionItemSpec {
                title: s.title.clone(),
                description: s.description.clone(),
                assignee,
                due_offset_seconds: s.due_offset_seconds,
                priority,
                tags: s.tags.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let proposal_id = ctx
        .manager
        .create_proposal_with_actions(
            suggested_id,
            domain_id,
            proposer_did.clone(),
            req.title.clone(),
            req.description.clone(),
            payload,
            scope,
            action_specs,
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

    // Suspension gate: suspended members may not advance governance proceedings
    // by opening a proposal's voting period. This prevents a frozen member from
    // controlling vote timing even on proposals they authored before suspension.
    if let Some(ref checker) = ctx.suspension_checker {
        if checker(requester_did.clone(), proposal.domain_id.0.clone()).await {
            return Err(err_forbidden(format!(
                "requester {} is suspended in domain {}; \
                 suspended members may not open proposals",
                requester_did, proposal.domain_id.0
            )));
        }
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

    // Close-time standing revalidation: if a member_checker is configured,
    // revalidate each voter's current commons standing before tallying.
    // Votes from members who lost standing (Suspended/Candidate) after casting
    // are excluded from the effective tally — standing must hold at resolution,
    // not just at vote-cast time. This converts entry-only gating to lifecycle
    // legitimacy: the constitutional guarantee holds across the full decision arc.
    let eligible_voters: Option<std::collections::HashSet<Did>> =
        if let Some(ref checker) = ctx.member_checker {
            let voter_dids = ctx
                .manager
                .get_voter_dids(&proposal_id)
                .await
                .map_err(anyhow_to_api)?;
            let domain_id = proposal.domain_id.0.clone();
            let mut eligible = std::collections::HashSet::new();
            for did in voter_dids {
                if checker(did.clone(), domain_id.clone()).await {
                    eligible.insert(did);
                }
            }
            Some(eligible)
        } else {
            None
        };

    // Suspension-based delegation exclusion: if a suspension_checker is configured,
    // identify suspended members across all domain members (not just voters) so their
    // vote weight cannot flow via pre-existing delegations at close time. This closes
    // the indirect-influence loophole: a suspended member cannot proxy their governance
    // weight through a delegate they appointed before being frozen.
    //
    // Only StaticList membership domains can enumerate members here; TrustThreshold
    // domains cannot, so their delegation suspension exclusion is a known gap.
    let excluded_delegators: Option<std::collections::HashSet<Did>> =
        if let Some(ref checker) = ctx.suspension_checker {
            match &domain.config.membership.source {
                icn_governance::MembershipSource::StaticList(members) => {
                    let domain_id = proposal.domain_id.0.clone();
                    let mut excluded = std::collections::HashSet::new();
                    for member in members {
                        if checker(member.clone(), domain_id.clone()).await {
                            excluded.insert(member.clone());
                        }
                    }
                    if excluded.is_empty() {
                        None
                    } else {
                        Some(excluded)
                    }
                }
                icn_governance::MembershipSource::TrustThreshold(_) => {
                    // TrustThreshold domains cannot enumerate members from the
                    // MembershipSource directly. If a resolver is wired into the
                    // context, ask it for the current eligible set and check each
                    // member against the suspension gate. Fail-open on resolver
                    // errors: a spurious 403 is worse than a missed exclusion, and
                    // a production resolver must be reliable.
                    if let Some(ref resolver) = ctx.membership_resolver {
                        match resolver.resolve_members(&domain) {
                            Ok(members) => {
                                let domain_id = proposal.domain_id.0.clone();
                                let mut excluded = std::collections::HashSet::new();
                                for member in &members {
                                    if checker(member.clone(), domain_id.clone()).await {
                                        excluded.insert(member.clone());
                                    }
                                }
                                if excluded.is_empty() {
                                    None
                                } else {
                                    Some(excluded)
                                }
                            }
                            Err(_) => None,
                        }
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        };

    ctx.manager
        .close_proposal_with_suspension(proposal_id.clone(), eligible_voters, excluded_delegators)
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
                // Charter acceptance: emit DeployCharter so the gateway registers
                // the domain in the commons charter store. domain_id == charter_id
                // per governance convention.
                icn_governance::ProposalPayload::Charter { charter_id, .. } => {
                    GovernanceEffect::DeployCharter {
                        proposal_id: proposal.id.0.clone(),
                        charter_id: charter_id.clone(),
                    }
                }
                // SDIS steward proposals are handled via ctx.sdis_service when set (test
                // path). In daemon mode sdis_service is None — the actor event system
                // handles execution (KernelGovernanceExecutor → SdisServiceImpl). Emit
                // Unhandled so the hook is a no-op for SDIS in both paths.
                icn_governance::ProposalPayload::Sdis { .. } => GovernanceEffect::Unhandled {
                    proposal_id: proposal.id.0.clone(),
                    payload_type: proposal.payload.type_name().to_owned(),
                },
                _ => GovernanceEffect::Unhandled {
                    proposal_id: proposal.id.0.clone(),
                    payload_type: proposal.payload.type_name().to_owned(),
                },
            };
            hook(effect);
        }

        // SDIS service dispatch (test path only — daemon uses actor event system).
        // When sdis_service is wired and the accepted payload is an SDIS proposal,
        // call the service directly so tests can prove steward creation without an
        // actor runtime.
        if let Some(ref svc) = ctx.sdis_service {
            use icn_kernel_api::{AppointStewardRequest, RevokeStewardRequest};
            if let icn_governance::ProposalPayload::Sdis {
                proposal: ref sdis_proposal,
            } = proposal.payload
            {
                match sdis_proposal {
                    icn_governance::sdis::SdisProposal::AppointSteward {
                        candidate,
                        bond_amount,
                        term_length,
                        ..
                    } => {
                        let req = AppointStewardRequest {
                            steward_did: candidate.to_string(),
                            jurisdiction_id: proposal.domain_id.0.clone(),
                            term_length_seconds: *term_length as i64,
                            bond_amount: *bond_amount,
                            region: None,
                            proposal_id: proposal.id.0.clone(),
                        };
                        match svc.appoint_steward(req) {
                            Ok(result) if !result.success => {
                                tracing::warn!(
                                    proposal_id = %proposal.id.0,
                                    error = ?result.error,
                                    "SDIS test-path: appoint_steward returned success=false"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    proposal_id = %proposal.id.0,
                                    error = %e,
                                    "SDIS test-path: appoint_steward call failed"
                                );
                            }
                            Ok(_) => {}
                        }
                    }
                    icn_governance::sdis::SdisProposal::RemoveSteward {
                        steward, reason, ..
                    } => {
                        let req = RevokeStewardRequest {
                            steward_did: steward.to_string(),
                            reason: reason.clone(),
                        };
                        match svc.revoke_steward(req) {
                            Ok(result) if !result.success => {
                                tracing::warn!(
                                    proposal_id = %proposal.id.0,
                                    error = ?result.error,
                                    "SDIS test-path: revoke_steward returned success=false"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    proposal_id = %proposal.id.0,
                                    error = %e,
                                    "SDIS test-path: revoke_steward call failed"
                                );
                            }
                            Ok(_) => {}
                        }
                    }
                    _ => {}
                }
            }
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

    // Fetch the proposal first so we can extract its domain_id for the standing
    // check. The domain is not available from the URL — it lives on the proposal.
    // We 404 here (not after the vote) so a bad proposal_id gives the right status.
    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Proposal not found: {}", proposal_id.0)))?;

    // Commons standing gate: voter must hold active Member standing in the
    // proposal's domain jurisdiction, same rule as proposal submission.
    if let Some(ref checker) = ctx.member_checker {
        if !checker(voter_did.clone(), proposal.domain_id.0.clone()).await {
            return Err(err_forbidden(format!(
                "voter {} does not have active Member standing in domain {}; \
                 join the jurisdiction before voting",
                voter_did, proposal.domain_id.0
            )));
        }
    }

    // Suspension gate: voter must not be suspended (MemberStatus::Suspended) in
    // the proposal's domain. Suspension is set by an accepted FreezeMember proposal.
    // A suspended member may not influence governance decisions until unfrozen.
    if let Some(ref checker) = ctx.suspension_checker {
        if checker(voter_did.clone(), proposal.domain_id.0.clone()).await {
            return Err(err_forbidden(format!(
                "voter {} is suspended in domain {}; \
                 suspended members may not cast votes",
                voter_did, proposal.domain_id.0
            )));
        }
    }

    ctx.manager
        .cast_vote(
            proposal_id.clone(),
            voter_did.clone(),
            choice,
            req.comment.clone(),
        )
        .await
        .map_err(anyhow_to_api)?;

    // Re-fetch so the response reflects the vote just cast (tally updated).
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

    // Suspension gate: suspended members may not proxy their voting influence
    // by creating delegations in domains where they are frozen.
    //
    // Scope resolution:
    // - Domain(id)   → domain is known directly; suspension checked against it.
    // - Proposal(id) → domain resolved via manager lookup; suspension checked.
    // - Blanket       → applies to all domains globally; the gate enumerates
    //                   every StaticList domain the delegator belongs to and
    //                   denies if suspended in any. TrustThreshold domains are
    //                   intentionally skipped (member set is not enumerable
    //                   without the trust graph at this layer).
    if let Some(ref checker) = ctx.suspension_checker {
        match &scope {
            DelegationScope::Blanket => {
                // A blanket delegation affects all domains globally. Enumerate
                // every domain the delegator belongs to via StaticList and deny
                // if suspended in any. Errors from list_domains are fail-open:
                // if enumeration fails, we do not deny (prevents spurious 403s
                // and preserves behaviour for non-suspended callers).
                if let Ok(all_domains) = ctx.manager.list_domains().await {
                    for domain in &all_domains {
                        if domain
                            .config
                            .membership
                            .source
                            .contains_static(&delegator_did)
                        {
                            let domain_id = domain.id.0.clone();
                            if checker(delegator_did.clone(), domain_id.clone()).await {
                                return Err(err_forbidden(format!(
                                    "delegator {} is suspended in domain {}; \
                                     suspended members may not create blanket delegations",
                                    delegator_did, domain_id
                                )));
                            }
                        }
                    }
                }
            }
            DelegationScope::Domain(d) => {
                let domain_id = d.0.clone();
                if checker(delegator_did.clone(), domain_id.clone()).await {
                    return Err(err_forbidden(format!(
                        "delegator {} is suspended in domain {}; \
                         suspended members may not create delegations",
                        delegator_did, domain_id
                    )));
                }
            }
            DelegationScope::Proposal(p) => {
                if let Some(domain_id) = ctx
                    .manager
                    .get_proposal(p)
                    .await
                    .unwrap_or(None)
                    .map(|prop| prop.domain_id.0.clone())
                {
                    if checker(delegator_did.clone(), domain_id.clone()).await {
                        return Err(err_forbidden(format!(
                            "delegator {} is suspended in domain {}; \
                             suspended members may not create delegations",
                            delegator_did, domain_id
                        )));
                    }
                }
            }
        }
    }

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
        source_agreement_id: None,
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
// SDIS proposal endpoints
// ============================================================================

/// POST /gov/proposals/sdis/appoint-steward — Propose to appoint a new steward.
///
/// Requires the proposer to be an active steward (checked via
/// `GovernanceContext::steward_checker`). Only existing stewards may nominate
/// new stewards — this separates generic governance participation (Member) from
/// steward-office governance (active steward required).
pub async fn create_appoint_steward_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<AppointStewardProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let proposer_did = parse_did(&claims.sub, "Invalid DID in token")?;

    val::validate_domain_id(&req.domain_id)?;
    val::validate_proposal_title(&req.title)?;
    val::validate_proposal_description(&req.description)?;

    // Steward standing gate: only active stewards may propose steward appointments.
    if let Some(ref checker) = ctx.steward_checker {
        if !checker(proposer_did.clone()).await {
            return Err(err_forbidden(format!(
                "proposer {} is not an active steward; only stewards may propose \
                 steward appointments",
                proposer_did
            )));
        }
    }

    let candidate = parse_did(&req.candidate, "Invalid candidate DID")?;
    let sponsors = req
        .sponsors
        .iter()
        .map(|s| parse_did(s, "Invalid sponsor DID"))
        .collect::<Result<Vec<_>, _>>()?;

    let payload = ProposalPayload::Sdis {
        proposal: icn_governance::sdis::SdisProposal::AppointSteward {
            candidate,
            sponsors,
            region: req.region.clone(),
            bond_amount: req.bond_amount,
            term_length: req.term_length_seconds,
        },
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
            ProposalScope::Local,
        )
        .await
        .map_err(anyhow_to_api)?;

    ctx.emitter.emit_proposal_created(
        &proposal_id.0,
        &req.domain_id,
        &proposer_did.to_string(),
        &req.title,
        "sdis_appoint_steward",
    );

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Proposal creation succeeded but proposal not found"))?;

    Ok(HttpResponse::Created().json(proposal))
}

/// POST /gov/proposals/sdis/remove-steward — Propose to remove an existing steward.
///
/// Requires the proposer to be an active steward (checked via
/// `GovernanceContext::steward_checker`). Steward removal is a steward-office
/// action, not a general membership action.
pub async fn create_remove_steward_proposal<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<RemoveStewardProposalRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let proposer_did = parse_did(&claims.sub, "Invalid DID in token")?;

    val::validate_domain_id(&req.domain_id)?;
    val::validate_proposal_title(&req.title)?;
    val::validate_proposal_description(&req.description)?;

    // Steward standing gate: only active stewards may propose steward removals.
    if let Some(ref checker) = ctx.steward_checker {
        if !checker(proposer_did.clone()).await {
            return Err(err_forbidden(format!(
                "proposer {} is not an active steward; only stewards may propose \
                 steward removals",
                proposer_did
            )));
        }
    }

    let steward_did = parse_did(&req.steward, "Invalid steward DID")?;

    let payload = ProposalPayload::Sdis {
        proposal: icn_governance::sdis::SdisProposal::RemoveSteward {
            steward: steward_did,
            reason: req.reason.clone(),
            return_bond: req.return_bond,
        },
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
            ProposalScope::Local,
        )
        .await
        .map_err(anyhow_to_api)?;

    ctx.emitter.emit_proposal_created(
        &proposal_id.0,
        &req.domain_id,
        &proposer_did.to_string(),
        &req.title,
        "sdis_remove_steward",
    );

    let proposal = ctx
        .manager
        .get_proposal(&proposal_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_internal("Proposal creation succeeded but proposal not found"))?;

    Ok(HttpResponse::Created().json(proposal))
}

// ============================================================================
// Structure and Activity helpers
// ============================================================================

fn parse_structure_kind(s: &str) -> Result<StructureKind, ApiError> {
    match s {
        "committee" => Ok(StructureKind::Committee),
        "working_group" => Ok(StructureKind::WorkingGroup),
        "team" => Ok(StructureKind::Team),
        "office" => Ok(StructureKind::Office),
        _ => Err(err_bad(format!(
            "Unknown structure kind: '{s}'. Must be one of: committee, working_group, team, office"
        ))),
    }
}

fn parse_activity_kind(s: &str) -> Result<ActivityKind, ApiError> {
    match s {
        "event" => Ok(ActivityKind::Event),
        "program" => Ok(ActivityKind::Program),
        "project" => Ok(ActivityKind::Project),
        "initiative" => Ok(ActivityKind::Initiative),
        _ => Err(err_bad(format!(
            "Unknown activity kind: '{s}'. Must be one of: event, program, project, initiative"
        ))),
    }
}

fn structure_to_response(s: &icn_governance::Structure) -> StructureResponse {
    StructureResponse {
        id: s.id.0.clone(),
        entity_id: s.parent_entity_id.clone(),
        kind: match s.kind {
            StructureKind::Committee => "committee",
            StructureKind::WorkingGroup => "working_group",
            StructureKind::Team => "team",
            StructureKind::Office => "office",
        }
        .to_string(),
        name: s.name.clone(),
        description: s.mandate.clone(),
        status: match s.status {
            StructureStatus::Active => "active",
            StructureStatus::Suspended => "suspended",
            StructureStatus::Dissolved => "dissolved",
        }
        .to_string(),
        created_at: s.created_at,
    }
}

fn role_to_response(r: &icn_governance::RoleAssignment) -> RoleAssignmentResponse {
    RoleAssignmentResponse {
        id: r.id.to_string(),
        structure_id: r.structure_id.0.clone(),
        person_did: r.person_did.to_string(),
        role: r.role.clone(),
        start_date: r.start_date,
        end_date: r.end_date,
    }
}

fn activity_to_response(a: &icn_governance::Activity) -> ActivityResponse {
    ActivityResponse {
        id: a.id.0.clone(),
        entity_id: a.parent_entity_id.clone(),
        kind: match a.kind {
            ActivityKind::Event => "event",
            ActivityKind::Program => "program",
            ActivityKind::Project => "project",
            ActivityKind::Initiative => "initiative",
        }
        .to_string(),
        name: a.name.clone(),
        description: a.description.clone(),
        status: match a.status {
            ActivityStatus::Planned => "planned",
            ActivityStatus::Active => "active",
            ActivityStatus::Completed => "completed",
            ActivityStatus::Cancelled => "cancelled",
        }
        .to_string(),
        start_date: a.start_date,
        end_date: a.end_date,
        linked_structures: a.linked_structures.iter().map(|s| s.0.clone()).collect(),
        created_at: a.created_at,
    }
}

// ── Structure endpoints ──────────────────────────────────────────────────────

/// POST /gov/entities/{entity_id}/structures — Create a structure.
pub async fn create_structure<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    entity_id: web::Path<String>,
    req: web::Json<CreateStructureRequest>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let entity = entity_id.into_inner();
    let kind = parse_structure_kind(&req.kind)?;

    let s = ctx
        .manager
        .create_structure(entity, kind, req.name.clone(), req.description.clone())
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().json(structure_to_response(&s)))
}

/// GET /gov/entities/{entity_id}/structures — List structures for an entity.
pub async fn list_structures<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    entity_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let entity = entity_id.into_inner();

    let structures = ctx
        .manager
        .list_structures(&entity)
        .map_err(anyhow_to_api)?;

    let responses: Vec<StructureResponse> = structures.iter().map(structure_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

/// GET /gov/structures/{structure_id} — Get a specific structure.
pub async fn get_structure<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    structure_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = StructureId(structure_id.into_inner());

    let s = ctx
        .manager
        .get_structure(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Structure not found"))?;

    Ok(HttpResponse::Ok().json(structure_to_response(&s)))
}

/// POST /gov/structures/{structure_id}/roles — Assign a role.
pub async fn assign_role<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    structure_id: web::Path<String>,
    req: web::Json<AssignRoleRequest>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let sid = StructureId(structure_id.into_inner());
    let person_did = parse_did(&req.did, "Invalid DID in request")?;

    let assignment = ctx
        .manager
        .assign_role(sid, person_did, req.role.clone())
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().json(role_to_response(&assignment)))
}

/// GET /gov/structures/{structure_id}/roles — List roles for a structure.
pub async fn list_roles<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    structure_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = StructureId(structure_id.into_inner());

    let roles = ctx.manager.list_roles(&id).map_err(anyhow_to_api)?;

    let responses: Vec<RoleAssignmentResponse> = roles.iter().map(role_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

// ── Activity endpoints ───────────────────────────────────────────────────────

/// POST /gov/entities/{entity_id}/activities — Create an activity.
pub async fn create_activity<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    entity_id: web::Path<String>,
    req: web::Json<CreateActivityRequest>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let entity = entity_id.into_inner();
    let kind = parse_activity_kind(&req.kind)?;

    let a = ctx
        .manager
        .create_activity(
            entity,
            kind,
            req.name.clone(),
            req.description.clone(),
            req.start_date,
            req.end_date,
        )
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().json(activity_to_response(&a)))
}

/// GET /gov/entities/{entity_id}/activities — List activities for an entity.
pub async fn list_activities<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    entity_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let entity = entity_id.into_inner();

    let activities = ctx
        .manager
        .list_activities(&entity)
        .map_err(anyhow_to_api)?;

    let responses: Vec<ActivityResponse> = activities.iter().map(activity_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

/// GET /gov/activities/{activity_id} — Get a specific activity.
pub async fn get_activity<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    activity_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = ActivityId(activity_id.into_inner());

    let a = ctx
        .manager
        .get_activity(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Activity not found"))?;

    Ok(HttpResponse::Ok().json(activity_to_response(&a)))
}

// ── Meeting helpers ──────────────────────────────────────────────────────────

fn attendance_status_str(s: &AttendanceStatus) -> &'static str {
    match s {
        AttendanceStatus::Invited => "invited",
        AttendanceStatus::Present => "present",
        AttendanceStatus::Absent => "absent",
        AttendanceStatus::Remote => "remote",
    }
}

fn meeting_role_str(r: &MeetingRole) -> &'static str {
    match r {
        MeetingRole::Facilitator => "facilitator",
        MeetingRole::NoteTaker => "note_taker",
        MeetingRole::Participant => "participant",
        MeetingRole::Observer => "observer",
    }
}

fn parse_attendance_status(s: &str) -> Result<AttendanceStatus, ApiError> {
    match s {
        "invited" => Ok(AttendanceStatus::Invited),
        "present" => Ok(AttendanceStatus::Present),
        "absent" => Ok(AttendanceStatus::Absent),
        "remote" => Ok(AttendanceStatus::Remote),
        _ => Err(err_bad(format!("Unknown attendance status: {s}"))),
    }
}

fn parse_meeting_role(s: &str) -> Result<MeetingRole, ApiError> {
    match s {
        "facilitator" => Ok(MeetingRole::Facilitator),
        "note_taker" | "notetaker" => Ok(MeetingRole::NoteTaker),
        "participant" => Ok(MeetingRole::Participant),
        "observer" => Ok(MeetingRole::Observer),
        _ => Err(err_bad(format!("Unknown meeting role: {s}"))),
    }
}

fn meeting_to_response(m: &icn_governance::Meeting) -> MeetingResponse {
    MeetingResponse {
        id: m.id.0.clone(),
        domain_id: m.domain_id.clone(),
        title: m.title.clone(),
        description: m.description.clone(),
        status: match m.status {
            MeetingStatus::Scheduled => "scheduled",
            MeetingStatus::InProgress => "in_progress",
            MeetingStatus::Completed => "completed",
            MeetingStatus::Cancelled => "cancelled",
        }
        .to_string(),
        scheduled_at: m.scheduled_at,
        started_at: m.started_at,
        ended_at: m.ended_at,
        attendees: m
            .attendees
            .iter()
            .map(|a| MeetingAttendeeResponse {
                did: a.did.clone(),
                status: attendance_status_str(&a.status).to_string(),
                meeting_role: meeting_role_str(&a.meeting_role).to_string(),
            })
            .collect(),
        agenda: m
            .agenda
            .iter()
            .map(|item| AgendaItemResponse {
                id: item.id.to_string(),
                title: item.title.clone(),
                description: item.description.clone(),
                presenter: item.presenter.clone(),
                linked_proposal: item.linked_proposal.as_ref().map(|p| p.0.clone()),
                discussion_notes: item.discussion_notes.clone(),
                outcome: item.outcome.clone(),
                generated_action_items: item
                    .generated_action_items
                    .iter()
                    .map(|id| id.to_string())
                    .collect(),
            })
            .collect(),
        linked_structures: m.linked_structures.iter().map(|s| s.0.clone()).collect(),
        linked_activities: m.linked_activities.iter().map(|a| a.0.clone()).collect(),
        notes_doc_id: m.notes_doc_id.clone(),
        created_by: m.created_by.clone(),
        created_at: m.created_at,
        present_count: m.present_count(),
    }
}

// ── Meeting endpoints ────────────────────────────────────────────────────────

/// POST /gov/domains/{domain_id}/meetings — Create a meeting.
pub async fn create_meeting<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    req: web::Json<CreateMeetingRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let domain = GovernanceDomainId(domain_id.into_inner());

    check_domain_membership(
        &ctx.manager,
        &domain,
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

    let m = ctx
        .manager
        .create_meeting(
            domain.0,
            req.title.clone(),
            req.description.clone(),
            req.scheduled_at,
            claims.sub.clone(),
        )
        .map_err(anyhow_to_api)?;

    // TODO: emit MeetingCreated event when GovernanceEventEmitter gains meeting methods
    Ok(HttpResponse::Created().json(meeting_to_response(&m)))
}

/// GET /gov/domains/{domain_id}/meetings — List meetings in a domain.
pub async fn list_meetings<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let domain = domain_id.into_inner();

    let meetings = ctx.manager.list_meetings(&domain).map_err(anyhow_to_api)?;
    let responses: Vec<MeetingResponse> = meetings.iter().map(meeting_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

/// GET /gov/meetings/{meeting_id} — Get a specific meeting.
pub async fn get_meeting<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = MeetingId(meeting_id.into_inner());

    let m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
}

/// POST /gov/meetings/{meeting_id}/start — Transition meeting to InProgress.
pub async fn start_meeting<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MeetingId(meeting_id.into_inner());

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    if m.created_by != claims.sub {
        return Err(ApiError::Forbidden(
            "Only the meeting creator can start it".into(),
        ));
    }
    if !matches!(m.status, MeetingStatus::Scheduled) {
        return Err(err_bad("Meeting is not in Scheduled status"));
    }

    m.status = MeetingStatus::InProgress;
    m.started_at = Some(current_time_secs());
    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;

    // TODO: emit MeetingStarted event when GovernanceEventEmitter gains meeting methods
    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
}

/// POST /gov/meetings/{meeting_id}/end — Transition meeting to Completed.
pub async fn end_meeting<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MeetingId(meeting_id.into_inner());

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    if m.created_by != claims.sub {
        return Err(ApiError::Forbidden(
            "Only the meeting creator can end it".into(),
        ));
    }
    if !matches!(m.status, MeetingStatus::InProgress) {
        return Err(err_bad("Meeting is not InProgress"));
    }

    m.status = MeetingStatus::Completed;
    m.ended_at = Some(current_time_secs());
    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;

    // TODO: emit MeetingEnded event when GovernanceEventEmitter gains meeting methods
    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
}

/// POST /gov/meetings/{meeting_id}/attendees — Add or update an attendee.
pub async fn add_attendee<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
    req: web::Json<AddAttendeeRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MeetingId(meeting_id.into_inner());
    let role = parse_meeting_role(&req.meeting_role)?;

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    if matches!(
        m.status,
        MeetingStatus::Completed | MeetingStatus::Cancelled
    ) {
        return Ok(HttpResponse::UnprocessableEntity()
            .json(serde_json::json!({"error": "Cannot modify a completed or cancelled meeting"})));
    }

    if let Some(ref checker) = ctx.member_checker {
        let caller_did = parse_did(&claims.sub, "Invalid DID in token")?;
        if !checker(caller_did, m.domain_id.clone()).await {
            return Ok(
                HttpResponse::Forbidden().json(serde_json::json!({"error": "Not a domain member"}))
            );
        }
    }

    // Upsert: update existing entry or append
    if let Some(existing) = m.attendees.iter_mut().find(|a| a.did == req.did) {
        existing.meeting_role = role;
    } else {
        m.attendees.push(icn_governance::MeetingAttendee {
            did: req.did.clone(),
            status: icn_governance::AttendanceStatus::Invited,
            meeting_role: role,
        });
    }

    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;
    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
}

/// PUT /gov/meetings/{meeting_id}/attendance — Mark attendance for a participant.
pub async fn mark_attendance<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
    req: web::Json<MarkAttendanceRequest>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MeetingId(meeting_id.into_inner());
    let status = parse_attendance_status(&req.status)?;

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    if matches!(
        m.status,
        MeetingStatus::Completed | MeetingStatus::Cancelled
    ) {
        return Ok(HttpResponse::UnprocessableEntity()
            .json(serde_json::json!({"error": "Cannot modify a completed or cancelled meeting"})));
    }

    let attendee = m
        .attendees
        .iter_mut()
        .find(|a| a.did == req.did)
        .ok_or_else(|| err_not_found("Attendee not found in this meeting"))?;

    attendee.status = status;
    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;
    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
}

/// POST /gov/meetings/{meeting_id}/agenda — Add an agenda item.
pub async fn add_agenda_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
    req: web::Json<AddAgendaItemRequest>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MeetingId(meeting_id.into_inner());

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    if matches!(
        m.status,
        MeetingStatus::Completed | MeetingStatus::Cancelled
    ) {
        return Ok(HttpResponse::UnprocessableEntity()
            .json(serde_json::json!({"error": "Cannot modify a completed or cancelled meeting"})));
    }

    let mut item = icn_governance::AgendaItem::new(req.title.clone());
    item.description = req.description.clone();
    item.presenter = req.presenter.clone();
    item.linked_proposal = req.linked_proposal.as_ref().map(|p| ProposalId(p.clone()));
    m.agenda.push(item);

    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;
    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
}

/// PUT /gov/meetings/{meeting_id}/agenda/{item_id} — Update an agenda item outcome.
pub async fn update_agenda_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<UpdateAgendaItemRequest>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let (meeting_id_str, item_id_str) = path.into_inner();
    let meeting_id = MeetingId(meeting_id_str);
    let item_uuid = item_id_str
        .parse::<uuid::Uuid>()
        .map_err(|e| err_bad(format!("Invalid agenda item ID: {e}")))?;
    let item_id = icn_governance::AgendaItemId(item_uuid);

    let mut m = ctx
        .manager
        .get_meeting(&meeting_id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    if matches!(
        m.status,
        MeetingStatus::Completed | MeetingStatus::Cancelled
    ) {
        return Ok(HttpResponse::UnprocessableEntity()
            .json(serde_json::json!({"error": "Cannot modify a completed or cancelled meeting"})));
    }

    let item = m
        .get_agenda_item_mut(&item_id)
        .ok_or_else(|| err_not_found("Agenda item not found"))?;

    if let Some(ref notes) = req.discussion_notes {
        item.discussion_notes = Some(notes.clone());
    }
    if let Some(ref outcome) = req.outcome {
        item.outcome = Some(outcome.clone());
    }

    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;
    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
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
    use crate::http::configure::{
        GovernanceContext, GovernanceEffect, ProposalAcceptedHook, SuspensionChecker,
    };
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
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
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
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
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

    /// Proves: the `close_proposal` HTTP handler invokes `membership_resolver` for
    /// `TrustThreshold` domains and uses the resolved member list to build
    /// `excluded_delegators`. This exercises the code path added in Tranche 8.
    ///
    /// Note: the in-memory `GovernanceManager` does not expand delegations at close time
    /// (delegation expansion requires the full `GovernanceActor`). The behavioral
    /// proof that excluded members' weight does not flow is covered by
    /// `test_suspended_delegator_weight_excluded_at_close_time` in
    /// `crates/icn-core/tests/delegation_tally_integration.rs`. This test verifies
    /// the HTTP-layer integration: that the resolver is invoked and the handler succeeds.
    #[tokio::test]
    async fn trust_threshold_membership_resolver_invoked_at_close_time() {
        use icn_governance::{GovernanceDomain, MembershipResolver};
        use icn_identity::KeyPair;

        // Test-only resolver: tracks whether `resolve_members` was called.
        struct TrackingResolver {
            members: Vec<Did>,
            called: Arc<Mutex<bool>>,
        }
        impl MembershipResolver for TrackingResolver {
            fn resolve_members(&self, _domain: &GovernanceDomain) -> anyhow::Result<Vec<Did>> {
                *self.called.lock().unwrap() = true;
                Ok(self.members.clone())
            }
        }

        // Use real KeyPairs so parse_did succeeds (test_did(N) with high N can produce
        // 32-byte patterns that are not valid Ed25519 points).
        let alice_kp = KeyPair::generate().unwrap();
        let bob_kp = KeyPair::generate().unwrap();
        let alice_did = alice_kp.did().clone();
        let bob_did = bob_kp.did().clone();

        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId("tt-resolver-test".to_string());

        // TrustThreshold domain: members cannot be enumerated without a resolver
        mgr.create_domain(
            domain_id.clone(),
            "TrustThreshold Coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 0, 86_400), // quorum=0, approval=0 → always Accepted
            MembershipConfig {
                source: MembershipSource::TrustThreshold(0.3),
            },
        )
        .await
        .unwrap();

        let proposal_id = ProposalId("tt-resolver-proof-1".to_string());
        mgr.create_proposal(
            proposal_id.clone(),
            domain_id.clone(),
            alice_did.clone(),
            "TrustThreshold resolver test".to_string(),
            "Proves resolver is invoked for TrustThreshold domains".to_string(),
            ProposalPayload::FreezeMember {
                member: test_did(1), // target DID: use seed 1 which is a valid Ed25519 point
                reason: "test".to_string(),
                duration_seconds: Some(86_400),
            },
            ProposalScope::Local,
        )
        .await
        .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86_400)
            .await
            .unwrap();

        mgr.cast_vote(
            proposal_id.clone(),
            alice_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();

        // suspension_checker marks bob as suspended
        let bob_did_for_checker = bob_did.clone();
        let suspension_checker: SuspensionChecker = Arc::new(move |did, _domain_id| {
            let bob = bob_did_for_checker.clone();
            Box::pin(async move { did == bob })
        });

        let resolver_called = Arc::new(Mutex::new(false));
        let resolver: Arc<dyn MembershipResolver> = Arc::new(TrackingResolver {
            members: vec![alice_did.clone(), bob_did.clone()],
            called: resolver_called.clone(),
        });

        let ctx = GovernanceContext {
            manager: mgr.clone(),
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: Some(suspension_checker),
            membership_resolver: Some(resolver),
            sdis_service: None,
        };

        let app = test_app!(ctx, alice_did);
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/proposals/{}/close", proposal_id.0))
                .to_request(),
        )
        .await;

        assert_eq!(
            resp.status().as_u16(),
            200,
            "close_proposal with TrustThreshold + resolver must succeed"
        );
        assert!(
            *resolver_called.lock().unwrap(),
            "membership_resolver.resolve_members() must be invoked for TrustThreshold domains"
        );
    }
}
