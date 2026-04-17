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
    MilestoneId, MilestoneStatus, ProgramId, ProgramKind, ProposalId, ProposalPayload,
    ProposalScope, StructureId, StructureKind, StructureStatus, VoteChoice,
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

/// Build an [`ActionItemFilter`] from `/me/work` query parameters.
///
/// Identical to [`build_action_item_filter`] except it accepts [`MyWorkFilterParams`]
/// (no `assignee` field — the caller's DID is always the implicit assignee).
fn build_my_work_filter(
    query: &MyWorkFilterParams,
) -> Result<icn_governance::ActionItemFilter, ApiError> {
    let mut filter = icn_governance::ActionItemFilter::default();
    if let Some(ref status) = query.status {
        filter.status = Some(parse_status(status)?);
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

    // Decision→action bridge: for accepted proposals, the actor materializes
    // action items from `action_items_on_accept` specs. Query those freshly
    // created items (filtered by linked_proposal) and emit
    // GovernanceActionItemCreated for each — matches the existing pattern
    // where the HTTP handler (not the actor) emits gateway-layer events.
    if outcome == "accepted" && !proposal.action_items_on_accept.is_empty() {
        let filter = icn_governance::ActionItemFilter {
            linked_proposal: Some(proposal.id.clone()),
            ..Default::default()
        };
        match ctx.manager.list_action_items(&proposal.domain_id, &filter) {
            Ok(items) => {
                for item in &items {
                    ctx.emitter.emit_action_item_created(
                        &item.id.to_string(),
                        &item.domain_id.0,
                        item.linked_proposal.as_ref().map(|p| p.0.as_str()),
                        item.assignee.as_ref().map(|d| d.as_str()),
                        item.created_at,
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    proposal_id = %proposal.id.0,
                    error = %e,
                    "Failed to list materialized action items for event emission"
                );
            }
        }
    }

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
// Notification digest handler
// ============================================================================

/// GET /gov/digest — Return a DID-scoped notification digest.
///
/// The `sub` claim in the token is used as the DID. Returns:
/// - `pending_votes`: Open proposals the caller has not yet voted on.
/// - `overdue_items`: Action items assigned to the caller that are past due.
/// - `upcoming_meetings`: Meetings scheduled in the next 48 h where the
///   caller is listed as an attendee.
pub async fn get_digest<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let did = parse_did(&claims.sub, "Invalid DID in token")?;
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let digest = ctx.manager.generate_digest(&did, now_secs).await;
    Ok(HttpResponse::Ok().json(digest))
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
        parent_program_id: a.parent_program_id.as_ref().map(|p| p.0.clone()),
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

    let parent_program_id = req
        .parent_program_id
        .as_deref()
        .map(|s| icn_governance::program::ProgramId(s.to_string()));
    let a = ctx
        .manager
        .create_activity(
            entity,
            kind,
            req.name.clone(),
            req.description.clone(),
            req.start_date,
            req.end_date,
            parent_program_id,
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

    ctx.emitter.emit_meeting_scheduled(
        &m.id.0,
        &m.domain_id,
        &m.title,
        m.scheduled_at,
        &m.created_by,
    );
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
///
/// Authorization (Phase 3 scope):
/// - Caller must hold the `governance:write` scope.
/// - Caller must be a member of the meeting's governance domain.
/// - Caller must be the `created_by` DID of the meeting.
///
/// Longer-term (Tranche 6, operational role delegation): authorization will
/// derive from `RoleAssignment.authority_scope` capability strings plus a
/// PolicyOracle rule — e.g., holders of `meeting-admin` on the linked structure.
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

    check_domain_membership(
        &ctx.manager,
        &GovernanceDomainId(m.domain_id.clone()),
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

    if m.created_by != claims.sub {
        return Err(ApiError::Forbidden(
            "Only the meeting creator can start it".into(),
        ));
    }
    if !matches!(m.status, MeetingStatus::Scheduled) {
        return Err(err_bad("Meeting is not in Scheduled status"));
    }

    let started_at = current_time_secs();
    m.status = MeetingStatus::InProgress;
    m.started_at = Some(started_at);
    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;

    ctx.emitter
        .emit_meeting_started(&m.id.0, &m.domain_id, started_at);
    Ok(HttpResponse::Ok().json(meeting_to_response(&m)))
}

/// POST /gov/meetings/{meeting_id}/end — Transition meeting to Completed.
///
/// Authorization: same as `start_meeting` (see docs there). Creator-only plus
/// domain membership is the Phase 3 policy; richer role-based authorization
/// lands with Tranche 6.
///
/// Note: this handler does not yet materialize action items from unresolved
/// agenda items. `AgendaItem.generated_action_items` is supported in the
/// schema but the closure trigger is a Tranche 1 follow-up — for now, action
/// items are created separately via the action-item API with `meeting_id` set.
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

    check_domain_membership(
        &ctx.manager,
        &GovernanceDomainId(m.domain_id.clone()),
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

    if m.created_by != claims.sub {
        return Err(ApiError::Forbidden(
            "Only the meeting creator can end it".into(),
        ));
    }
    if !matches!(m.status, MeetingStatus::InProgress) {
        return Err(err_bad("Meeting is not InProgress"));
    }

    let ended_at = current_time_secs();
    m.status = MeetingStatus::Completed;
    m.ended_at = Some(ended_at);
    ctx.manager.update_meeting(&m).map_err(anyhow_to_api)?;

    ctx.emitter
        .emit_meeting_ended(&m.id.0, &m.domain_id, ended_at);
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
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MeetingId(meeting_id.into_inner());
    let status = parse_attendance_status(&req.status)?;

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    // Domain-membership check: only members of the meeting's governance domain
    // may mutate its state. `governance:write` alone is insufficient.
    check_domain_membership(
        &ctx.manager,
        &GovernanceDomainId(m.domain_id.clone()),
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

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
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MeetingId(meeting_id.into_inner());

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    check_domain_membership(
        &ctx.manager,
        &GovernanceDomainId(m.domain_id.clone()),
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

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
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
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

    check_domain_membership(
        &ctx.manager,
        &GovernanceDomainId(m.domain_id.clone()),
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

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

// ── Program helpers ──────────────────────────────────────────────────────────

fn parse_program_kind(s: &str) -> Result<ProgramKind, ApiError> {
    match s {
        "cycle" => Ok(ProgramKind::Cycle),
        "campaign" => Ok(ProgramKind::Campaign),
        "initiative" => Ok(ProgramKind::Initiative),
        "series" => Ok(ProgramKind::Series),
        other if !other.is_empty() => Ok(ProgramKind::Custom(other.to_string())),
        _ => Err(err_bad("program kind must not be empty")),
    }
}

fn program_kind_str(k: &ProgramKind) -> String {
    match k {
        ProgramKind::Cycle => "cycle".to_string(),
        ProgramKind::Campaign => "campaign".to_string(),
        ProgramKind::Initiative => "initiative".to_string(),
        ProgramKind::Series => "series".to_string(),
        ProgramKind::Custom(s) => s.clone(),
    }
}

fn program_status_str(s: &icn_governance::ProgramStatus) -> &'static str {
    use icn_governance::ProgramStatus;
    match s {
        ProgramStatus::Draft => "draft",
        ProgramStatus::ActivePlanning => "active_planning",
        ProgramStatus::PublicLaunch => "public_launch",
        ProgramStatus::InExecution => "in_execution",
        ProgramStatus::Closed => "closed",
        ProgramStatus::Archived => "archived",
    }
}

fn milestone_status_str(s: &MilestoneStatus) -> &'static str {
    match s {
        MilestoneStatus::Pending => "pending",
        MilestoneStatus::InProgress => "in_progress",
        MilestoneStatus::Completed => "completed",
        MilestoneStatus::Blocked => "blocked",
        MilestoneStatus::Skipped => "skipped",
    }
}

fn parse_milestone_status(s: &str) -> Result<MilestoneStatus, ApiError> {
    match s {
        "pending" => Ok(MilestoneStatus::Pending),
        "in_progress" => Ok(MilestoneStatus::InProgress),
        "completed" => Ok(MilestoneStatus::Completed),
        "blocked" => Ok(MilestoneStatus::Blocked),
        "skipped" => Ok(MilestoneStatus::Skipped),
        _ => Err(err_bad(format!("Unknown milestone status: {s}"))),
    }
}

fn program_to_response(p: &icn_governance::Program) -> ProgramResponse {
    ProgramResponse {
        id: p.id.0.clone(),
        domain_id: p.domain_id.0.clone(),
        parent_entity_id: p.parent_entity_id.clone(),
        kind: program_kind_str(&p.kind),
        name: p.name.clone(),
        description: p.description.clone(),
        status: program_status_str(&p.status).to_string(),
        start_at: p.start_at,
        end_at: p.end_at,
        milestones: p.milestones.iter().map(|m| m.0.clone()).collect(),
        activities: p.activities.iter().map(|a| a.0.clone()).collect(),
        created_at: p.created_at,
        closed_at: p.closed_at,
        created_by_decision: p.created_by_decision.as_ref().map(|d| d.0.clone()),
    }
}

fn milestone_to_response(m: &icn_governance::Milestone) -> MilestoneResponse {
    // Deserialize the JSON-encoded gate string back to a Value for HTTP clients.
    // An unparseable stored gate is treated as absent rather than surfaced as an
    // error here — parse failures are surfaced at write time.
    let completion_gate = m
        .completion_gate
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    MilestoneResponse {
        id: m.id.0.clone(),
        program_id: m.program_id.0.clone(),
        name: m.name.clone(),
        description: m.description.clone(),
        phase_index: m.phase_index,
        target_date: m.target_date,
        status: milestone_status_str(&m.status).to_string(),
        completion_criteria: m.completion_criteria.clone(),
        completion_gate,
        completed_at: m.completed_at,
        completed_by: m.completed_by.as_ref().map(|d| d.to_string()),
        created_at: m.created_at,
    }
}

// ── Program endpoints ────────────────────────────────────────────────────────

/// POST /gov/domains/{domain_id}/programs — Create a program.
pub async fn create_program<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    req: web::Json<CreateProgramRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let domain = GovernanceDomainId(domain_id.into_inner());

    check_domain_membership(
        &ctx.manager,
        &domain,
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

    if req.name.trim().is_empty() {
        return Err(err_bad("Program name must not be empty"));
    }

    let kind = parse_program_kind(&req.kind)?;
    let created_by_decision = req
        .created_by_decision
        .as_deref()
        .map(|s| icn_governance::ProposalId(s.to_string()));

    let p = ctx
        .manager
        .create_program(
            domain,
            req.parent_entity_id.clone(),
            kind,
            req.name.clone(),
            req.description.clone(),
            req.start_at,
            req.end_at,
            created_by_decision,
        )
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Created().json(program_to_response(&p)))
}

/// GET /gov/domains/{domain_id}/programs — List programs in a domain.
pub async fn list_programs_by_domain<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let domain = GovernanceDomainId(domain_id.into_inner());

    let programs = ctx
        .manager
        .list_programs_by_domain(&domain)
        .map_err(anyhow_to_api)?;

    let responses: Vec<ProgramResponse> = programs.iter().map(program_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

/// GET /gov/programs/{program_id} — Get a specific program.
pub async fn get_program<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    program_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = ProgramId(program_id.into_inner());

    let p = ctx
        .manager
        .get_program(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found"))?;

    Ok(HttpResponse::Ok().json(program_to_response(&p)))
}

// ── Milestone gate helpers ───────────────────────────────────────────────────

/// Parse and validate a `serde_json::Value` as a `icn_ccl::Expr`.
///
/// Used at HTTP write boundaries to reject malformed gate expressions before
/// they are persisted.  The round-trip is:
///   HTTP Value → `serde_json::to_string` → `parse_gate` → `icn_ccl::Expr`
///
/// Returns `ApiError::BadRequest` on any parse failure.
fn parse_gate_value(value: &serde_json::Value) -> Result<icn_ccl::Expr, ApiError> {
    let json_str = serde_json::to_string(value)
        .map_err(|e| err_bad(format!("Invalid gate expression JSON: {e}")))?;
    icn_governance::milestone_gate::parse_gate(&json_str)
        .map_err(|e| err_bad(format!("Invalid gate expression: {e}")))
}

// ── Milestone endpoints ──────────────────────────────────────────────────────

/// POST /gov/programs/{program_id}/milestones — Create a milestone.
pub async fn create_milestone<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    program_id: web::Path<String>,
    req: web::Json<CreateMilestoneRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let pid = ProgramId(program_id.into_inner());

    if req.name.trim().is_empty() {
        return Err(err_bad("Milestone name must not be empty"));
    }

    // Resolve the program's governance domain so membership can be verified.
    // A caller with governance:write on an unrelated domain must not be able
    // to add milestones to any known program.
    let prog = ctx
        .manager
        .get_program(&pid)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found"))?;

    check_domain_membership(
        &ctx.manager,
        &prog.domain_id,
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

    let mut m = ctx
        .manager
        .create_milestone(
            pid,
            req.name.clone(),
            req.description.clone(),
            req.phase_index,
            req.target_date,
            req.completion_criteria.clone(),
        )
        .map_err(anyhow_to_api)?;

    // If a gate was included in the create request, validate and store it now.
    if let Some(gate_value) = &req.completion_gate {
        let gate_expr = parse_gate_value(gate_value)?;
        m = ctx
            .manager
            .set_milestone_gate(&m.id, Some(gate_expr))
            .map_err(anyhow_to_api)?;
    }

    Ok(HttpResponse::Created().json(milestone_to_response(&m)))
}

/// GET /gov/milestones/{milestone_id} — Get a specific milestone.
pub async fn get_milestone<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    milestone_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = MilestoneId(milestone_id.into_inner());

    let m = ctx
        .manager
        .get_milestone(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Milestone not found"))?;

    Ok(HttpResponse::Ok().json(milestone_to_response(&m)))
}

/// GET /gov/programs/{program_id}/milestones — List milestones for a program.
pub async fn list_milestones_by_program<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    program_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let pid = ProgramId(program_id.into_inner());

    let milestones = ctx
        .manager
        .list_milestones_by_program(&pid)
        .map_err(anyhow_to_api)?;

    let responses: Vec<MilestoneResponse> = milestones.iter().map(milestone_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

/// PATCH /gov/milestones/{milestone_id} — Update milestone status.
pub async fn update_milestone_status<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    milestone_id: web::Path<String>,
    req: web::Json<UpdateMilestoneStatusRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MilestoneId(milestone_id.into_inner());
    let status = parse_milestone_status(&req.status)?;
    let actor = parse_did(&claims.sub, "Invalid DID in token")?;

    // Resolve governance domain via milestone → program → domain for membership check.
    let milestone = ctx
        .manager
        .get_milestone(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Milestone not found"))?;
    let prog = ctx
        .manager
        .get_program(&milestone.program_id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found for milestone"))?;
    check_domain_membership(&ctx.manager, &prog.domain_id, &actor).await?;

    let m = ctx
        .manager
        .update_milestone_status(&id, status, &actor)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(milestone_to_response(&m)))
}

/// PUT /gov/milestones/{milestone_id}/gate — Set or clear the CCL completion gate.
///
/// Accepts `{ "completion_gate": <Expr JSON> }` to install a gate, or
/// `{ "completion_gate": null }` / `{}` to remove any existing gate.
///
/// The gate expression is validated immediately; a malformed expression is
/// rejected with 400 before anything is persisted.
pub async fn set_milestone_gate<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    milestone_id: web::Path<String>,
    req: web::Json<SetMilestoneGateRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:write")?;
    let id = MilestoneId(milestone_id.into_inner());
    let actor = parse_did(&claims.sub, "Invalid DID in token")?;

    // Resolve governance domain via milestone → program for membership check.
    let milestone = ctx
        .manager
        .get_milestone(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Milestone not found"))?;
    let prog = ctx
        .manager
        .get_program(&milestone.program_id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found for milestone"))?;
    check_domain_membership(&ctx.manager, &prog.domain_id, &actor).await?;

    // Parse and validate the gate expression (if any) before persisting.
    let gate_expr = req
        .completion_gate
        .as_ref()
        .map(parse_gate_value)
        .transpose()?;

    let m = ctx
        .manager
        .set_milestone_gate(&id, gate_expr)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(milestone_to_response(&m)))
}

// ── Program dashboard ─────────────────────────────────────────────────────────

fn dashboard_milestone_summary(m: &icn_governance::Milestone) -> DashboardMilestoneSummary {
    DashboardMilestoneSummary {
        id: m.id.0.clone(),
        name: m.name.clone(),
        phase_index: m.phase_index,
        status: milestone_status_str(&m.status).to_string(),
        target_date: m.target_date,
        completed_at: m.completed_at,
    }
}

fn dashboard_activity_summary(a: &icn_governance::Activity) -> DashboardActivitySummary {
    DashboardActivitySummary {
        id: a.id.0.clone(),
        name: a.name.clone(),
        kind: match a.kind {
            ActivityKind::Event => "event",
            ActivityKind::Program => "program",
            ActivityKind::Project => "project",
            ActivityKind::Initiative => "initiative",
        }
        .to_string(),
        status: match a.status {
            ActivityStatus::Planned => "planned",
            ActivityStatus::Active => "active",
            ActivityStatus::Completed => "completed",
            ActivityStatus::Cancelled => "cancelled",
        }
        .to_string(),
    }
}

/// GET /gov/programs/{program_id}/dashboard — Composite program view.
///
/// Returns a compact read surface combining:
/// - program metadata
/// - ordered milestones with status counts
/// - linked activity summaries
/// - action item counts scoped to the program's activities
///
/// Requires `governance:read` scope. No write gates — this is a pure read.
///
/// **Meetings** are absent from this response. `Meeting` records link to
/// activities via `linked_activities` but there is no activity-keyed index
/// on the meeting store. Resolving meetings would require a full domain scan.
/// Deferred until a `list_by_activity` index is available.
pub async fn get_program_dashboard<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    program_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let pid = ProgramId(program_id.into_inner());

    let d = ctx
        .manager
        .get_program_dashboard(&pid)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found"))?;

    // Build milestone counts
    let mut mc = DashboardMilestoneCounts {
        total: d.milestones.len(),
        ..Default::default()
    };
    for m in &d.milestones {
        match m.status {
            MilestoneStatus::Completed => mc.completed += 1,
            MilestoneStatus::InProgress => mc.in_progress += 1,
            MilestoneStatus::Blocked => mc.blocked += 1,
            MilestoneStatus::Pending => mc.pending += 1,
            MilestoneStatus::Skipped => mc.skipped += 1,
        }
    }

    let ai = &d.action_item_counts;
    let resp = ProgramDashboardResponse {
        program_id: d.program.id.0.clone(),
        domain_id: d.program.domain_id.0.clone(),
        parent_entity_id: d.program.parent_entity_id.clone(),
        name: d.program.name.clone(),
        kind: program_kind_str(&d.program.kind),
        status: program_status_str(&d.program.status).to_string(),
        description: d.program.description.clone(),
        start_at: d.program.start_at,
        end_at: d.program.end_at,
        milestones: d
            .milestones
            .iter()
            .map(dashboard_milestone_summary)
            .collect(),
        milestone_counts: mc,
        activities: d
            .activities
            .iter()
            .map(dashboard_activity_summary)
            .collect(),
        action_item_counts: DashboardActionItemCounts {
            pending: ai.pending,
            in_progress: ai.in_progress,
            completed: ai.completed,
            deferred: ai.deferred,
            cancelled: ai.cancelled,
            total: ai.total(),
        },
    };

    Ok(HttpResponse::Ok().json(resp))
}

// ── /me endpoints ─────────────────────────────────────────────────────────────

/// GET /gov/me/scopes — Return all role assignments held by the authenticated DID.
///
/// The response is a flat list of [`RoleAssignmentResponse`] across every
/// structure in the system where the caller holds a role. This is the primary
/// entry point for an organizer or member to discover their authority scope
/// without knowing which structures they belong to in advance.
pub async fn get_my_scopes<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let caller = parse_did(&claims.sub, "Invalid DID in token")?;

    let roles = ctx
        .manager
        .list_roles_for_person(&caller)
        .map_err(anyhow_to_api)?;

    let responses: Vec<RoleAssignmentResponse> = roles.iter().map(role_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
}

/// GET /gov/me/work — Return action items assigned to the authenticated DID.
///
/// Returns items across **all** governance domains, sorted oldest-first
/// (`created_at` ascending) so the longest-outstanding work surfaces at the top.
///
/// **Query parameters** (all optional):
/// - `status` — filter by status: `pending`, `in_progress`, `completed`, `deferred`, `cancelled`
/// - `priority` — filter by priority: `low`, `medium`, `high`, `critical`
/// - `overdue` — if `true`, return only items whose `due_date` is in the past
/// - `tag` — filter to items that include this tag string
///
/// When no parameters are provided all items in any status are returned.
/// Typical usage: `?status=pending` or `?overdue=true&priority=high`.
pub async fn get_my_work<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    query: web::Query<MyWorkFilterParams>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let caller = parse_did(&claims.sub, "Invalid DID in token")?;

    let filter = build_my_work_filter(&query)?;

    let items = ctx
        .manager
        .list_work_for_person(&caller, &filter)
        .map_err(anyhow_to_api)?;

    let responses: Vec<ActionItemResponse> = items.iter().map(action_item_to_response).collect();
    Ok(HttpResponse::Ok().json(responses))
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

    // ── /me endpoint tests ──────────────────────────────────────────────────

    /// Build a minimal test app wired to both /me/scopes and /me/work.
    macro_rules! me_test_app {
        ($ctx:expr, $caller_did:expr) => {{
            let ctx_data = web::Data::new($ctx);
            let caller_did_str = $caller_did.to_string();
            test::init_service(
                App::new()
                    .app_data(ctx_data)
                    .wrap_fn(move |req, srv| {
                        req.extensions_mut().insert(BasicClaims {
                            sub: caller_did_str.clone(),
                            scope: Some("governance:read".to_string()),
                        });
                        srv.call(req)
                    })
                    .route(
                        "/me/scopes",
                        web::get().to(get_my_scopes::<NoopEventEmitter>),
                    )
                    .route("/me/work", web::get().to(get_my_work::<NoopEventEmitter>)),
            )
            .await
        }};
    }

    #[actix_web::test]
    async fn get_my_scopes_empty() {
        let caller = test_did(10);
        let mgr = Arc::new(GovernanceManager::new());
        let ctx = GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
        };

        let app = me_test_app!(ctx, caller);
        let resp = test::call_service(
            &app,
            test::TestRequest::get().uri("/me/scopes").to_request(),
        )
        .await;

        assert_eq!(resp.status().as_u16(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body, serde_json::json!([]));
    }

    #[actix_web::test]
    async fn get_my_scopes_returns_assigned_roles() {
        let caller = test_did(11);
        let other = test_did(12);
        let mgr = Arc::new(GovernanceManager::new());

        // Create a structure and assign roles
        let s = mgr
            .create_structure(
                "entity-abc".to_string(),
                icn_governance::StructureKind::Committee,
                "Tech Committee".to_string(),
                None,
            )
            .unwrap();

        mgr.assign_role(s.id.clone(), caller.clone(), "coordinator".to_string())
            .unwrap();
        mgr.assign_role(s.id.clone(), other.clone(), "member".to_string())
            .unwrap();

        let ctx = GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
        };

        let app = me_test_app!(ctx, caller.clone());
        let resp = test::call_service(
            &app,
            test::TestRequest::get().uri("/me/scopes").to_request(),
        )
        .await;

        assert_eq!(resp.status().as_u16(), 200);
        let body: Vec<serde_json::Value> = test::read_body_json(resp).await;
        // Only the caller's role should appear, not `other`'s
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["person_did"], caller.to_string());
        assert_eq!(body[0]["role"], "coordinator");
    }

    #[actix_web::test]
    async fn get_my_work_empty() {
        let caller = test_did(10); // seed 10 produces a valid Ed25519 point
        let mgr = Arc::new(GovernanceManager::new());
        let ctx = GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
        };

        let app = me_test_app!(ctx, caller);
        let resp =
            test::call_service(&app, test::TestRequest::get().uri("/me/work").to_request()).await;

        assert_eq!(resp.status().as_u16(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body, serde_json::json!([]));
    }

    #[actix_web::test]
    async fn get_my_work_returns_assigned_items_across_domains() {
        let caller = test_did(11); // seed 11 produces a valid Ed25519 point
        let mgr = Arc::new(GovernanceManager::new());

        // Create domains and action items
        for domain_str in ["alpha", "beta"] {
            let domain_id = GovernanceDomainId(domain_str.to_string());
            mgr.create_domain(
                domain_id.clone(),
                format!("{domain_str} coop"),
                "cooperative_default".to_string(),
                GovernanceParams::new(0, 51, 86_400),
                MembershipConfig {
                    source: MembershipSource::StaticList(vec![caller.clone()]),
                },
            )
            .await
            .unwrap();

            mgr.create_action_item(
                domain_id,
                format!("Task in {domain_str}"),
                None,
                caller.clone(),       // created_by
                Some(caller.clone()), // assignee
                None,
                icn_governance::ActionItemPriority::Medium,
                None,
                None,
                vec![],
            )
            .unwrap();
        }

        let ctx = GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
        };

        let app = me_test_app!(ctx, caller.clone());
        let resp =
            test::call_service(&app, test::TestRequest::get().uri("/me/work").to_request()).await;

        assert_eq!(resp.status().as_u16(), 200);
        let body: Vec<serde_json::Value> = test::read_body_json(resp).await;
        assert_eq!(
            body.len(),
            2,
            "should return items from both alpha and beta domains"
        );
        // All items belong to the caller
        for item in &body {
            assert_eq!(item["assignee"], caller.to_string());
        }
    }

    // ── /me/work filter tests ────────────────────────────────────────────────

    /// Build a context + test app for /me/work filter tests.
    macro_rules! work_app {
        ($mgr:expr, $caller:expr) => {{
            let ctx = GovernanceContext {
                manager: $mgr,
                emitter: NoopEventEmitter,
                on_proposal_accepted: None,
                on_charter_accepted: None,
                member_checker: None,
                steward_checker: None,
                suspension_checker: None,
                membership_resolver: None,
                sdis_service: None,
            };
            me_test_app!(ctx, $caller)
        }};
    }

    /// Create a single-domain governance manager with the caller as a member.
    async fn single_domain_mgr(
        caller: &Did,
        domain_str: &str,
    ) -> (Arc<GovernanceManager>, GovernanceDomainId) {
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId(domain_str.to_string());
        mgr.create_domain(
            domain_id.clone(),
            format!("{domain_str}-coop"),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 51, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![caller.clone()]),
            },
        )
        .await
        .unwrap();
        (mgr, domain_id)
    }

    #[actix_web::test]
    async fn my_work_filter_by_status() {
        let caller = test_did(11); // seed 11 produces a valid Ed25519 point
        let (mgr, domain_id) = single_domain_mgr(&caller, "filter-status").await;

        // One pending, one completed item.
        let pending = mgr
            .create_action_item(
                domain_id.clone(),
                "Pending task".to_string(),
                None,
                caller.clone(),
                Some(caller.clone()),
                None,
                icn_governance::ActionItemPriority::Medium,
                None,
                None,
                vec![],
            )
            .unwrap();
        let mut completed = mgr
            .create_action_item(
                domain_id.clone(),
                "Completed task".to_string(),
                None,
                caller.clone(),
                Some(caller.clone()),
                None,
                icn_governance::ActionItemPriority::Medium,
                None,
                None,
                vec![],
            )
            .unwrap();
        completed.status = icn_governance::ActionItemStatus::Completed;
        mgr.update_action_item(&completed).unwrap();

        let app = work_app!(mgr, caller.clone());

        // ?status=pending → 1 item
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/me/work?status=pending")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);
        let body: Vec<serde_json::Value> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1, "only pending items");
        assert_eq!(body[0]["id"], pending.id.to_string());

        // ?status=completed → 1 item
        let resp2 = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/me/work?status=completed")
                .to_request(),
        )
        .await;
        assert_eq!(resp2.status().as_u16(), 200);
        let body2: Vec<serde_json::Value> = test::read_body_json(resp2).await;
        assert_eq!(body2.len(), 1, "only completed items");
        assert_eq!(body2[0]["id"], completed.id.to_string());
    }

    #[actix_web::test]
    async fn my_work_filter_by_priority() {
        let caller = test_did(12); // seed 12 produces a valid Ed25519 point
        let (mgr, domain_id) = single_domain_mgr(&caller, "filter-prio").await;

        mgr.create_action_item(
            domain_id.clone(),
            "Low task".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            icn_governance::ActionItemPriority::Low,
            None,
            None,
            vec![],
        )
        .unwrap();
        let high = mgr
            .create_action_item(
                domain_id.clone(),
                "High task".to_string(),
                None,
                caller.clone(),
                Some(caller.clone()),
                None,
                icn_governance::ActionItemPriority::High,
                None,
                None,
                vec![],
            )
            .unwrap();

        let app = work_app!(mgr, caller.clone());

        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/me/work?priority=high")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);
        let body: Vec<serde_json::Value> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1, "only high-priority items");
        assert_eq!(body[0]["id"], high.id.to_string());
    }

    #[actix_web::test]
    async fn my_work_filter_by_tag() {
        let caller = test_did(22);
        let (mgr, domain_id) = single_domain_mgr(&caller, "filter-tag").await;

        mgr.create_action_item(
            domain_id.clone(),
            "Untagged".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            icn_governance::ActionItemPriority::Medium,
            None,
            None,
            vec![],
        )
        .unwrap();
        let tagged = mgr
            .create_action_item(
                domain_id.clone(),
                "Tagged outreach".to_string(),
                None,
                caller.clone(),
                Some(caller.clone()),
                None,
                icn_governance::ActionItemPriority::Medium,
                None,
                None,
                vec!["outreach".to_string()],
            )
            .unwrap();

        let app = work_app!(mgr, caller.clone());

        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/me/work?tag=outreach")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);
        let body: Vec<serde_json::Value> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1, "only items tagged 'outreach'");
        assert_eq!(body[0]["id"], tagged.id.to_string());
    }

    #[actix_web::test]
    async fn my_work_filter_overdue() {
        let caller = test_did(23);
        let (mgr, domain_id) = single_domain_mgr(&caller, "filter-overdue").await;

        // Past due_date (10s in the past).
        let now = icn_time::current_timestamp_secs();
        let mut overdue = mgr
            .create_action_item(
                domain_id.clone(),
                "Overdue task".to_string(),
                None,
                caller.clone(),
                Some(caller.clone()),
                None,
                icn_governance::ActionItemPriority::High,
                None,
                None,
                vec![],
            )
            .unwrap();
        overdue.due_date = Some(now.saturating_sub(10));
        mgr.update_action_item(&overdue).unwrap();

        // No due_date — not overdue.
        mgr.create_action_item(
            domain_id.clone(),
            "No deadline".to_string(),
            None,
            caller.clone(),
            Some(caller.clone()),
            None,
            icn_governance::ActionItemPriority::Low,
            None,
            None,
            vec![],
        )
        .unwrap();

        let app = work_app!(mgr, caller.clone());

        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/me/work?overdue=true")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);
        let body: Vec<serde_json::Value> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1, "only overdue items");
        assert_eq!(body[0]["id"], overdue.id.to_string());
    }

    #[actix_web::test]
    async fn my_work_sorted_oldest_first() {
        let caller = test_did(10); // seed 10 produces a valid Ed25519 point
        let (mgr, domain_id) = single_domain_mgr(&caller, "filter-sort").await;

        let mut first = mgr
            .create_action_item(
                domain_id.clone(),
                "Old task".to_string(),
                None,
                caller.clone(),
                Some(caller.clone()),
                None,
                icn_governance::ActionItemPriority::Medium,
                None,
                None,
                vec![],
            )
            .unwrap();
        let mut second = mgr
            .create_action_item(
                domain_id.clone(),
                "New task".to_string(),
                None,
                caller.clone(),
                Some(caller.clone()),
                None,
                icn_governance::ActionItemPriority::Medium,
                None,
                None,
                vec![],
            )
            .unwrap();

        // Force deterministic timestamps independent of wall clock.
        first.created_at = 1_000;
        second.created_at = 2_000;
        mgr.update_action_item(&first).unwrap();
        mgr.update_action_item(&second).unwrap();

        let app = work_app!(mgr, caller.clone());
        let resp =
            test::call_service(&app, test::TestRequest::get().uri("/me/work").to_request()).await;
        assert_eq!(resp.status().as_u16(), 200);
        let body: Vec<serde_json::Value> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 2);
        assert_eq!(
            body[0]["id"],
            first.id.to_string(),
            "oldest item must be first"
        );
        assert_eq!(
            body[1]["id"],
            second.id.to_string(),
            "newest item must be last"
        );
    }

    // ── Governance-to-execution bridge tests ─────────────────────────────────

    /// Proves: when a proposal with `action_items_on_accept` is accepted via
    /// `close_proposal`, the specs are materialized as concrete `ActionItem`s
    /// with correct provenance (`linked_proposal` set to the proposal ID).
    ///
    /// This tests the standalone/in-memory path (no actor). The actor path is
    /// tested separately in `actor.rs`. Both paths must produce the same result.
    #[tokio::test]
    async fn accepted_proposal_materializes_action_items() {
        // Seeds 1 and 2 produce valid Ed25519 compressed points.
        let member_did = test_did(1);
        let assignee_did = test_did(2);
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId("bridge-test-coop".to_string());

        // quorum=0, approval=0 → any close is Accepted.
        mgr.create_domain(
            domain_id.clone(),
            "Bridge Test Coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 0, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![member_did.clone()]),
            },
        )
        .await
        .unwrap();

        // Proposal with two obligation specs.
        let proposal_id = icn_governance::ProposalId("obligation-proposal-1".to_string());
        let specs = vec![
            icn_governance::ActionItemSpec {
                title: "Send meeting recap to members".to_string(),
                description: Some("Within 48h of decision".to_string()),
                assignee: Some(assignee_did.clone()),
                due_offset_seconds: Some(48 * 3600),
                priority: icn_governance::ActionItemPriority::High,
                tags: vec!["comms".to_string()],
            },
            icn_governance::ActionItemSpec {
                title: "Update website".to_string(),
                description: None,
                assignee: None,
                due_offset_seconds: None,
                priority: icn_governance::ActionItemPriority::Medium,
                tags: vec![],
            },
        ];

        mgr.create_proposal_with_actions(
            proposal_id.clone(),
            domain_id.clone(),
            member_did.clone(),
            "Accepted Proposal".to_string(),
            String::new(),
            icn_governance::ProposalPayload::Text {
                body: "obligation test".to_string(),
            },
            icn_governance::ProposalScope::Local,
            specs,
        )
        .await
        .unwrap();

        // Open → Vote For → close (quorum=0, approval=0 → Accepted).
        mgr.open_proposal(proposal_id.clone(), 86_400)
            .await
            .unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            icn_governance::VoteChoice::For,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        // Verify action items were materialized.
        let filter = icn_governance::ActionItemFilter {
            linked_proposal: Some(proposal_id.clone()),
            ..Default::default()
        };
        let items = mgr.list_action_items(&domain_id, &filter).unwrap();
        assert_eq!(items.len(), 2, "two specs → two ActionItems");

        // Find the "recap" item by title.
        let recap = items
            .iter()
            .find(|i| i.title.contains("recap"))
            .expect("recap item must exist");

        assert_eq!(recap.linked_proposal.as_ref(), Some(&proposal_id));
        assert_eq!(recap.assignee.as_ref(), Some(&assignee_did));
        assert_eq!(recap.priority, icn_governance::ActionItemPriority::High);
        assert!(recap.tags.contains(&"comms".to_string()));
        // due_date = creation time + 48h offset
        assert_eq!(
            recap.due_date,
            Some(recap.created_at.saturating_add(48 * 3600)),
            "due_date must equal created_at + due_offset_seconds"
        );
    }

    /// Proves: a rejected proposal does NOT create action items.
    #[tokio::test]
    async fn rejected_proposal_does_not_materialize_action_items() {
        let member_did = test_did(3);
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId("reject-test-coop".to_string());

        // quorum=0, approval=100 → any close is Rejected.
        mgr.create_domain(
            domain_id.clone(),
            "Reject Test Coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 100, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![member_did.clone()]),
            },
        )
        .await
        .unwrap();

        let proposal_id = icn_governance::ProposalId("reject-proposal-1".to_string());
        let specs = vec![icn_governance::ActionItemSpec {
            title: "This must not appear".to_string(),
            description: None,
            assignee: None,
            due_offset_seconds: None,
            priority: icn_governance::ActionItemPriority::Medium,
            tags: vec![],
        }];

        mgr.create_proposal_with_actions(
            proposal_id.clone(),
            domain_id.clone(),
            member_did.clone(),
            "Rejected Proposal".to_string(),
            String::new(),
            icn_governance::ProposalPayload::Text {
                body: "rejection test".to_string(),
            },
            icn_governance::ProposalScope::Local,
            specs,
        )
        .await
        .unwrap();

        // Open → Vote Against → close (quorum=0, approval=100 → Rejected).
        mgr.open_proposal(proposal_id.clone(), 86_400)
            .await
            .unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            icn_governance::VoteChoice::Against,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        let filter = icn_governance::ActionItemFilter {
            linked_proposal: Some(proposal_id.clone()),
            ..Default::default()
        };
        let items = mgr.list_action_items(&domain_id, &filter).unwrap();
        assert_eq!(
            items.len(),
            0,
            "rejected proposal must not produce ActionItems"
        );
    }

    // ── Program dashboard tests ────────────────────────────────────────────

    macro_rules! dashboard_app {
        ($ctx:expr) => {{
            let ctx_data = web::Data::new($ctx);
            test::init_service(
                App::new()
                    .app_data(ctx_data)
                    .wrap_fn(move |req, srv| {
                        req.extensions_mut().insert(BasicClaims {
                            sub: "did:icn:reader".to_string(),
                            scope: Some("governance:read".to_string()),
                        });
                        srv.call(req)
                    })
                    .route(
                        "/programs/{program_id}/dashboard",
                        web::get().to(get_program_dashboard::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    fn make_dashboard_ctx(mgr: Arc<GovernanceManager>) -> GovernanceContext<NoopEventEmitter> {
        GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
        }
    }

    /// 404 when program does not exist.
    #[actix_web::test]
    async fn dashboard_404_for_missing_program() {
        let mgr = Arc::new(GovernanceManager::new());
        let app = dashboard_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/programs/nonexistent/dashboard")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 404);
    }

    /// Empty program returns zero counts and empty lists.
    #[actix_web::test]
    async fn dashboard_empty_program() {
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId("dom-a".to_string());
        mgr.create_domain(
            domain_id.clone(),
            "Domain A".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![]),
            },
        )
        .await
        .unwrap();

        let prog = mgr
            .create_program(
                domain_id,
                "entity-a".to_string(),
                icn_governance::ProgramKind::Cycle,
                "Summit Cycle 2026".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let app = dashboard_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/programs/{}/dashboard", prog.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["program_id"], prog.id.0.as_str());
        assert_eq!(body["name"], "Summit Cycle 2026");
        assert_eq!(body["status"], "draft");
        assert_eq!(body["kind"], "cycle");
        assert_eq!(body["milestones"], serde_json::json!([]));
        assert_eq!(body["activities"], serde_json::json!([]));
        assert_eq!(body["milestone_counts"]["total"], 0);
        assert_eq!(body["action_item_counts"]["total"], 0);
    }

    /// Milestones are returned ordered by phase_index; counts are correct.
    #[actix_web::test]
    async fn dashboard_milestones_ordered_and_counted() {
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId("dom-b".to_string());
        mgr.create_domain(
            domain_id.clone(),
            "Domain B".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![]),
            },
        )
        .await
        .unwrap();

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "entity-b".to_string(),
                icn_governance::ProgramKind::Campaign,
                "Q3 Campaign".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        // Add milestones out of phase order
        mgr.create_milestone(
            prog.id.clone(),
            "Phase 2 work".to_string(),
            None,
            2,
            None,
            vec![],
        )
        .unwrap();
        let m0 = mgr
            .create_milestone(
                prog.id.clone(),
                "Phase 0 kickoff".to_string(),
                None,
                0,
                None,
                vec![],
            )
            .unwrap();
        let m1 = mgr
            .create_milestone(
                prog.id.clone(),
                "Phase 1 plan".to_string(),
                None,
                1,
                None,
                vec![],
            )
            .unwrap();

        // Mark m0 completed, m1 blocked
        let caller = test_did(1);
        mgr.update_milestone_status(&m0.id, MilestoneStatus::Completed, &caller)
            .unwrap();
        mgr.update_milestone_status(&m1.id, MilestoneStatus::Blocked, &caller)
            .unwrap();

        let app = dashboard_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/programs/{}/dashboard", prog.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        let milestones = body["milestones"].as_array().unwrap();
        assert_eq!(milestones.len(), 3);
        // Phase order preserved (snake_case field names — no camelCase rename on summary)
        assert_eq!(milestones[0]["phase_index"], 0);
        assert_eq!(milestones[1]["phase_index"], 1);
        assert_eq!(milestones[2]["phase_index"], 2);
        assert_eq!(milestones[0]["status"], "completed");
        assert_eq!(milestones[1]["status"], "blocked");

        let mc = &body["milestone_counts"];
        assert_eq!(mc["total"], 3);
        assert_eq!(mc["completed"], 1);
        assert_eq!(mc["blocked"], 1);
        assert_eq!(mc["pending"], 1);
    }

    /// Action items attached to program activities appear in counts;
    /// domain items not attached to a program activity are excluded.
    #[actix_web::test]
    async fn dashboard_action_item_counts_scoped_to_program_activities() {
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId("dom-c".to_string());
        let creator = test_did(2);

        mgr.create_domain(
            domain_id.clone(),
            "Domain C".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![creator.clone()]),
            },
        )
        .await
        .unwrap();

        // Create program and link an activity
        let prog = mgr
            .create_program(
                domain_id.clone(),
                "entity-c".to_string(),
                icn_governance::ProgramKind::Cycle,
                "Cycle".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        // Activity declares its own parent_program_id — dashboard discovers
        // it via the reverse-link scan (activity → program direction).
        let activity = mgr
            .create_activity(
                "entity-c".to_string(),
                icn_governance::ActivityKind::Event,
                "Summit Event".to_string(),
                None,
                None,
                None,
                Some(prog.id.clone()),
            )
            .unwrap();

        // Action item attached to the program's activity (should be counted)
        let mut item_in = mgr
            .create_action_item(
                domain_id.clone(),
                "In-scope task".to_string(),
                None,
                creator.clone(),
                None,
                None,
                icn_governance::ActionItemPriority::Medium,
                None,
                None,
                vec![],
            )
            .unwrap();
        item_in.parent = Some(icn_governance::InstitutionalParent::Activity {
            id: activity.id.clone(),
        });
        mgr.update_action_item(&item_in).unwrap();

        // Domain action item with no parent (should NOT be counted)
        mgr.create_action_item(
            domain_id.clone(),
            "Unattached task".to_string(),
            None,
            creator.clone(),
            None,
            None,
            icn_governance::ActionItemPriority::Medium,
            None,
            None,
            vec![],
        )
        .unwrap();

        let app = dashboard_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/programs/{}/dashboard", prog.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        let ai = &body["action_item_counts"];
        assert_eq!(ai["pending"], 1, "only the attached item should be counted");
        assert_eq!(ai["total"], 1);

        let activities = body["activities"].as_array().unwrap();
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0]["name"], "Summit Event");
    }

    // =========================================================================
    // Milestone gate HTTP surface tests
    // =========================================================================

    /// Build a `GovernanceContext` wired for milestone gate tests.
    /// Uses `governance:write` scope so the same context works for both reads
    /// and writes (write scope is a superset in these tests).
    fn make_gate_ctx(mgr: Arc<GovernanceManager>) -> GovernanceContext<NoopEventEmitter> {
        GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
        }
    }

    /// Actix test app for milestone gate endpoints.
    ///
    /// Exposes:
    ///   POST /programs/{program_id}/milestones
    ///   GET  /milestones/{milestone_id}
    ///   PUT  /milestones/{milestone_id}/gate
    macro_rules! gate_app {
        ($ctx:expr, $caller_did:expr) => {{
            let ctx_data = web::Data::new($ctx);
            let caller_did_str = $caller_did.to_string();
            test::init_service(
                App::new()
                    .app_data(ctx_data)
                    .wrap_fn(move |req, srv| {
                        req.extensions_mut().insert(BasicClaims {
                            sub: caller_did_str.clone(),
                            // Space-separated: satisfies both governance:read
                            // (GET milestone) and governance:write (POST/PUT).
                            scope: Some("governance:read governance:write".to_string()),
                        });
                        srv.call(req)
                    })
                    .route(
                        "/programs/{program_id}/milestones",
                        web::post().to(create_milestone::<NoopEventEmitter>),
                    )
                    .route(
                        "/milestones/{milestone_id}",
                        web::get().to(get_milestone::<NoopEventEmitter>),
                    )
                    .route(
                        "/milestones/{milestone_id}/gate",
                        web::put().to(set_milestone_gate::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    /// Set up a domain + member + program and return (mgr, program_id, member_did).
    async fn setup_gate_fixture() -> (Arc<GovernanceManager>, String, Did) {
        use icn_governance::{
            GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource,
        };

        let mgr = Arc::new(GovernanceManager::new());
        // Use a real keypair: parse_did validates Ed25519 key validity, so a fake
        // anchor-id DID (test_did) causes a 400 when the handler calls parse_did.
        let did = icn_identity::KeyPair::generate().unwrap().did().clone();
        let domain_id = GovernanceDomainId("gate-coop".into());

        mgr.create_domain(
            domain_id.clone(),
            "Gate Coop".into(),
            "cooperative_default".into(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![did.clone()]),
            },
        )
        .await
        .unwrap();

        let prog = mgr
            .create_program(
                domain_id,
                "ent-1".into(),
                icn_governance::ProgramKind::Cycle,
                "Test Program".into(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        (mgr, prog.id.0.clone(), did)
    }

    /// A valid `icn_ccl::Expr` as a JSON Value.
    ///
    /// Uses `Literal(Bool(true))` — the simplest well-typed expression.  Its
    /// serde form is `{"Literal":{"Bool":true}}` which roundtrips cleanly
    /// through `parse_gate_value`.
    fn valid_gate_json() -> serde_json::Value {
        // Derive the canonical JSON from the actual Expr type so this stays
        // in sync if the serde representation ever changes.
        let expr = icn_ccl::Expr::Literal(icn_ccl::types::Value::Bool(true));
        serde_json::to_value(&expr).expect("Expr must serialize")
    }

    /// Creating a milestone with a valid `completion_gate` succeeds and the
    /// response body includes the gate.
    #[actix_web::test]
    async fn http_create_milestone_with_gate_roundtrips() {
        let (mgr, prog_id, did) = setup_gate_fixture().await;
        let app = gate_app!(make_gate_ctx(mgr), did);

        let body = serde_json::json!({
            "name": "M1",
            "completion_gate": valid_gate_json(),
        });

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/programs/{prog_id}/milestones"))
                .set_json(&body)
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 201, "create should succeed");

        let json: serde_json::Value = test::read_body_json(resp).await;
        assert!(
            json["completionGate"].is_object(),
            "response must include completionGate as a JSON object, got: {json}"
        );
    }

    /// GET /gov/milestones/{id} includes `completionGate` when a gate is set.
    #[actix_web::test]
    async fn http_get_milestone_returns_gate() {
        let (mgr, prog_id, did) = setup_gate_fixture().await;

        // Create milestone then set gate directly via manager.
        let m = mgr
            .create_milestone(
                icn_governance::ProgramId(prog_id.clone()),
                "M2".into(),
                None,
                0,
                None,
                vec![],
            )
            .unwrap();
        let gate = icn_ccl::Expr::Literal(icn_ccl::types::Value::Bool(true));
        mgr.set_milestone_gate(&m.id, Some(gate)).unwrap();

        let app = gate_app!(make_gate_ctx(mgr), did);
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let json: serde_json::Value = test::read_body_json(resp).await;
        assert!(
            !json["completionGate"].is_null() && json["completionGate"] != serde_json::Value::Null,
            "completionGate should be present when gate is set, got: {json}"
        );
    }

    /// PUT /gov/milestones/{id}/gate sets a new gate; response reflects the change.
    #[actix_web::test]
    async fn http_put_gate_sets_gate() {
        let (mgr, prog_id, did) = setup_gate_fixture().await;
        let m = mgr
            .create_milestone(
                icn_governance::ProgramId(prog_id),
                "M3".into(),
                None,
                0,
                None,
                vec![],
            )
            .unwrap();

        let app = gate_app!(make_gate_ctx(mgr), did);
        let resp = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/milestones/{}/gate", m.id.0))
                .set_json(serde_json::json!({ "completion_gate": valid_gate_json() }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200, "PUT gate should succeed");

        let json: serde_json::Value = test::read_body_json(resp).await;
        assert!(
            json["completionGate"].is_object(),
            "response must include updated completionGate"
        );
    }

    /// PUT with `completion_gate: null` removes an existing gate.
    #[actix_web::test]
    async fn http_put_gate_null_clears_gate() {
        let (mgr, prog_id, did) = setup_gate_fixture().await;
        let m = mgr
            .create_milestone(
                icn_governance::ProgramId(prog_id),
                "M4".into(),
                None,
                0,
                None,
                vec![],
            )
            .unwrap();

        // Set a gate first.
        let gate = icn_ccl::Expr::Literal(icn_ccl::types::Value::Bool(true));
        mgr.set_milestone_gate(&m.id, Some(gate)).unwrap();

        let app = gate_app!(make_gate_ctx(mgr), did);
        let resp = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/milestones/{}/gate", m.id.0))
                .set_json(serde_json::json!({ "completion_gate": null }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200, "clearing gate should succeed");

        let json: serde_json::Value = test::read_body_json(resp).await;
        assert!(
            json.get("completionGate").is_none() || json["completionGate"].is_null(),
            "completionGate should be absent after clearing, got: {json}"
        );
    }

    /// PUT with a malformed gate expression returns 400.
    #[actix_web::test]
    async fn http_put_gate_malformed_returns_400() {
        let (mgr, prog_id, did) = setup_gate_fixture().await;
        let m = mgr
            .create_milestone(
                icn_governance::ProgramId(prog_id),
                "M5".into(),
                None,
                0,
                None,
                vec![],
            )
            .unwrap();

        let app = gate_app!(make_gate_ctx(mgr), did);
        let resp = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/milestones/{}/gate", m.id.0))
                .set_json(serde_json::json!({ "completion_gate": { "NotAnExpr": true } }))
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status().as_u16(),
            400,
            "malformed gate must be rejected with 400"
        );
    }
}
