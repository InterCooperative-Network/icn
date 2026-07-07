//! Governance HTTP handlers.
//!
//! All handlers are generic over `E: GovernanceEventEmitter` and receive
//! shared context via `web::Data<GovernanceContext<E>>`. Route registration
//! lives in `super::configure`.

use actix_web::{web, HttpRequest, HttpResponse};
use icn_federation::SettlementInterval;
use icn_governance::{
    ActionItemFilter, ActionItemId, ActionItemPriority, ActionItemStatus, ActivityId, ActivityKind,
    ActivityStatus, AttendanceStatus, CharterId, DataSharingLevel, Delegation, DelegationId,
    DelegationScope, DisputeResolutionMethod, DomainPolicy, FederationProposal, FederationTerms,
    GovernanceDomain, GovernanceDomainId, GovernanceParams, MeetingId, MeetingRole, MeetingStatus,
    MembershipAction, MembershipConfig, MilestoneId, MilestoneStatus, ProgramId, ProgramKind,
    ProposalId, ProposalPayload, ProposalScope, StructureId, StructureKind, StructureStatus,
    VoteChoice,
};
use icn_http_kit::{
    auth::{require_any_scope, require_any_scope_matched, require_scope, BasicClaims},
    error::ApiError,
    pagination::{ListPagination, ListQuery, ListResponse},
};
use icn_identity::Did;

use super::configure::{DispatchEvidenceSpec, GovernanceContext, GovernanceEffect};
use super::models::*;
use super::validation as val;
use crate::domain_policy_adoption::{
    AdoptDomainPolicyError, DeclareInstitutionalDomainError, DomainPolicyAdoptionError,
    InstitutionalDomainStoreError,
};
use crate::events::GovernanceEventEmitter;
use crate::fallback_observation_emitter::{
    observe_charter_domain_admission, NoopFallbackObservationEmitter,
};

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
///
/// Delegates to [`resolve_caller_membership`] so that `TrustThreshold` domains
/// are resolved through the configured `membership_resolver` rather than being
/// treated as unconditional members (issue #1913 — the same fail-open #1870
/// closed in the direct add/remove member handlers, here on the ~20 read/write
/// endpoints this shared gate backs). Unresolved standing fails closed under
/// `Production` posture (surfacing `unresolved_standing`); `Bootstrap`/`Test`
/// stay permissive for dev/devnet.
async fn check_domain_membership<E>(
    ctx: &GovernanceContext<E>,
    domain_id: &GovernanceDomainId,
    user_did: &Did,
) -> Result<(), ApiError> {
    let domain = ctx
        .manager
        .get_domain(domain_id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Domain not found: {}", domain_id.0)))?;

    if !resolve_caller_membership(ctx, &domain, user_did)? {
        return Err(err_forbidden(format!(
            "Only domain members can perform this action (not a member of domain '{}')",
            domain_id.0
        )));
    }
    Ok(())
}

/// Resolve whether `caller` holds membership standing in `domain`.
///
/// This is the single shared resolver-backed standing check used by both the
/// direct add/remove member handlers (#1870) and [`check_domain_membership`]
/// (#1913), so the two never drift into divergent authorization policies.
///
/// This closes the #1870 fail-open. For `TrustThreshold` domains membership is
/// derived from a trust graph that lives outside the governance layer, so the
/// caller's standing is resolved through the configured [`MembershipResolver`]
/// instead of being assumed. Outcomes:
///
/// * `Ok(true)` — caller holds standing; the handler may proceed.
/// * `Ok(false)` — caller is provably not a member; the handler returns its
///   own context-specific `403 Forbidden`.
/// * `Err(Forbidden)` — standing could not be resolved (the resolver failed,
///   or no resolver is wired under a posture that refuses to fall open). The
///   error carries the explicit `unresolved_standing` code and must be
///   propagated — it must never be downgraded to a permissive allow.
///
/// `StaticList` domains are resolved directly from the source, unchanged.
fn resolve_caller_membership<E>(
    ctx: &GovernanceContext<E>,
    domain: &GovernanceDomain,
    caller_did: &Did,
) -> Result<bool, ApiError> {
    match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => Ok(members.contains(caller_did)),
        icn_governance::MembershipSource::TrustThreshold(_) => match &ctx.membership_resolver {
            Some(resolver) => resolver.is_member(domain, caller_did).map_err(|e| {
                err_forbidden(format!(
                    "Cannot resolve membership standing for caller in TrustThreshold domain \
                     '{}' (unresolved_standing): {e}",
                    domain.id.0
                ))
            }),
            None => {
                if ctx.build_mode.rejects_unresolved_standing() {
                    Err(err_forbidden(format!(
                        "Membership resolver is not configured for TrustThreshold domain '{}' \
                         (unresolved_standing); refusing domain-scoped operation under production \
                         posture",
                        domain.id.0
                    )))
                } else {
                    // Bootstrap/Test posture preserves the historical permissive
                    // behavior so existing dev/devnet flows keep working. Only
                    // Production fails closed (issue #1870).
                    Ok(true)
                }
            }
        },
    }
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
    // #1868 step 5: federation-class capability with accepted-also fallback to
    // the legacy broad `governance:write` until that scope is retired. This
    // shared helper gates all seven federation-proposal handlers, so narrowing
    // it here migrates the whole family in one place.
    let claims = require_any_scope::<BasicClaims>(
        http_req,
        &["governance:federation:write", "governance:write"],
    )?;
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
    // #1868 step 3: charter-class capability with accepted-also fallback to the
    // legacy broad `governance:write` until that scope is retired.
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:charter:write", "governance:write"],
    )?;
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
                    domains.sort_by_key(|a| a.created_at);
                } else {
                    domains.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
    // #1868 step 3: charter-class capability with accepted-also fallback to the
    // legacy broad `governance:write` until that scope is retired.
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:charter:write", "governance:write"],
    )?;
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

    if !resolve_caller_membership(&ctx, &domain, &caller_did)? {
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
    // #1868 step 3: charter-class capability with accepted-also fallback to the
    // legacy broad `governance:write` until that scope is retired.
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:charter:write", "governance:write"],
    )?;
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

    if !resolve_caller_membership(&ctx, &domain, &caller_did)? {
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
// Charter activation handlers
// ============================================================================

/// Stable, recognizable proposal_id used when the direct charter-activation
/// path emits a [`GovernanceEffect::DeployCharter`]. The dispatch site
/// (`server.rs`) keys commons registration on `charter_id` and ignores
/// `proposal_id`, so a synthetic value is safe and makes the provenance
/// origin obvious in logs.
fn direct_activation_pseudo_proposal_id(charter_id: &str) -> String {
    format!("direct-activation:{charter_id}")
}

/// POST /gov/charters — Activate a CCL charter directly.
///
/// Bootstrap-side counterpart to the governance-ratification path: hands a
/// validated CCL document to the same downstream pair the accepted-proposal
/// path uses.
///
/// ## Equivalence with the accepted-proposal path
///
/// `close_proposal` (handlers.rs) does two things on charter acceptance:
///
/// 1. Calls `on_charter_accepted(charter_id, charter_yaml)` — wired by the
///    daemon to `CharterPolicyOracle::deploy_charter`, so the kernel starts
///    enforcing the charter's CCL constraints immediately.
/// 2. Calls `on_proposal_accepted(GovernanceEffect::DeployCharter)` — wired
///    by the gateway to `commons.store_charter(...)`, registering the domain
///    in the commons charter store so downstream `register_steward` /
///    jurisdiction-binding flows will recognize it.
///
/// This handler invokes **both** hooks. Without the second dispatch the
/// endpoint would silently leave commons unaware of the domain, breaking
/// SDIS steward registration (which validates that the jurisdiction's
/// charter exists in commons).
///
/// ## Out of scope (intentional)
///
/// - **Institutional effect record / decision-hash provenance.** The
///   ratified path persists an `InstitutionalEffectRecord` keyed by a real
///   proposal_id and bound to the governance receipt's `decision_hash`.
///   Direct activation has no proposal/decision, so no such record is
///   emitted. Provenance for this path is the `tracing` audit trail plus
///   the synthetic `direct-activation:<charter_id>` proposal_id appearing
///   in commons logs. Adding a synthetic proposal record would require a
///   broader institutional-history redesign — tracked elsewhere.
/// - **Dispatch evidence (`on_proposal_accepted_with_evidence`).** The
///   evidence sink writes rows keyed by a real `proposal_id`; firing it
///   here would create orphan rows. Skipped intentionally.
///
/// ## Idempotency
///
/// `CharterPolicyOracle::deploy_charter` overwrites in place per
/// `charter_id`, and the commons dispatch treats "already exists" as a
/// no-op (`server.rs` DeployCharter handler). Reactivating the same
/// `charter_id` therefore succeeds with a fresh `activated_at`. Callers
/// that need single-shot semantics must dedupe upstream.
pub async fn activate_charter<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    req: web::Json<ActivateCharterRequest>,
) -> Result<HttpResponse, ApiError> {
    // #1868 step 3: charter-class capability with accepted-also fallback to the
    // legacy broad `governance:write` until that scope is retired.
    require_any_scope::<BasicClaims>(&http_req, &["governance:charter:write", "governance:write"])?;

    val::validate_charter_id(&req.charter_id)?;
    val::validate_charter_yaml(&req.charter_yaml)?;

    // Parse and structurally validate at the boundary so malformed input
    // returns 400 instead of being silently dropped by the fire-and-forget
    // hook.
    let doc = icn_ccl::schema::CclDocument::from_yaml(&req.charter_yaml)
        .map_err(|e| err_bad(format!("Invalid CCL YAML: {e}")))?;
    doc.validate()
        .map_err(|e| err_bad(format!("Charter validation failed: {e}")))?;

    let charter_hook = ctx
        .on_charter_accepted
        .as_ref()
        .ok_or_else(|| err_internal("Charter activation hook is not wired"))?;
    charter_hook(req.charter_id.clone(), req.charter_yaml.clone());

    // Mirror the accepted-proposal close path: emit DeployCharter so the
    // gateway registers the domain in the commons charter store. Without
    // this, downstream `register_steward` / jurisdiction-binding checks
    // (which read the commons charter table) will not see the domain even
    // though the charter is live in the policy oracle.
    if let Some(proposal_hook) = &ctx.on_proposal_accepted {
        proposal_hook(GovernanceEffect::DeployCharter {
            proposal_id: direct_activation_pseudo_proposal_id(&req.charter_id),
            charter_id: req.charter_id.clone(),
        });
    } else {
        // Defensive: missing the proposal hook means commons registration
        // will not happen and the success response would be misleading.
        // The daemon wires both hooks unconditionally; this branch only
        // fires in misconfigured deployments or test envs that opt out.
        return Err(err_internal(
            "Charter activation rejected: governance acceptance hook is not wired, \
             so commons registration cannot run and the charter would be only partially active",
        ));
    }

    Ok(HttpResponse::Created().json(ActivateCharterResponse {
        charter_id: req.charter_id.clone(),
        status: "active".to_string(),
        activated_at: current_time_secs(),
        // Direct activation is a bootstrap / direct-administrative path, not a
        // ratified/mandate activation. Surface that lineage at the API boundary
        // so clients cannot mistake it for ordinary ratified authority. The
        // artifact-level marker (the synthetic direct-activation provenance
        // string on the emitted effect) is unchanged.
        activation_path: ActivationPath::Bootstrap,
    }))
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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
        proposals.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
                    proposals.sort_by_key(|a| a.created_at);
                } else {
                    proposals.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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
    // #1868 (A0): capture the scope class that actually authorized this close so
    // the process-authorized v3 receipt records evidence derived from the auth
    // decision rather than a hardcoded constant. The narrowed
    // `governance:proposal:write` is listed first so it is preferred when both
    // are present; the broad `governance:write` remains an accepted-also fallback
    // until that scope is retired. `require_any_scope_matched` returns whichever
    // candidate was actually matched, so the receipt records the truth (the
    // narrow class when held, the broad scope only when the fallback was used).
    let (claims, presented_scope) = require_any_scope_matched::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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
        .close_proposal_with_suspension(
            proposal_id.clone(),
            eligible_voters,
            excluded_delegators,
            // #1868: record the scope class that actually authorized this close
            // (from `require_any_scope_matched` above) as the v3 receipt's
            // `capability_scope_presented` — evidence derived from the auth
            // decision, not a hardcoded constant.
            Some(presented_scope),
        )
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

    // On accepted: persist an institutional effect record (non-economic
    // effects only — economic effects are recorded as AllocationReceipts in
    // the manager). This runs regardless of dispatch path (standalone vs
    // actor-backed) so the durable artifact is available via
    // GET /gov/proposals/{id}/effects and GET /gov/proposals/{id}/deliberation.
    if outcome == "accepted" {
        // Best-effort decision_hash lookup to bind the record into the
        // INV-5 provenance chain. If the governance receipt is not yet
        // available (e.g. actor-path timing), record with decision_hash=None
        // — the proposal_id still locates the record.
        let decision_hash = ctx
            .manager
            .get_chain(&proposal_id)
            .await
            .ok()
            .and_then(|chain| chain.governance_receipt.map(|r| r.decision_hash));

        // Canonical emission path: same method called from the actor's
        // force-close accept branch. Idempotent on (proposal_id, effect_kind)
        // so when the actor has already emitted (actor-backed normal close),
        // this call returns AlreadyEmitted instead of duplicating.
        if let Err(e) =
            ctx.manager
                .apply_acceptance_effects(&proposal, decision_hash, current_time_secs())
        {
            tracing::error!(
                proposal_id = %proposal.id.0,
                error = %e,
                "apply_acceptance_effects failed; proposal acceptance recorded but no institutional artifact",
            );
        }
    }

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
                // SDIS steward proposals: translate to concrete authority effects so the
                // evidence-returning hook (wired by the gateway to commons.register_steward
                // / commons.revoke_steward) can produce real dispatch evidence. Other SDIS
                // variants fall through to Unhandled — they are carried by the actor event
                // system (KernelGovernanceExecutor → SdisServiceImpl) and produce no
                // in-process evidence from this handler.
                icn_governance::ProposalPayload::Sdis { proposal: sdis } => match sdis {
                    icn_governance::sdis::SdisProposal::AppointSteward {
                        candidate,
                        region,
                        bond_amount,
                        term_length,
                        ..
                    } => GovernanceEffect::AppointSteward {
                        proposal_id: proposal.id.0.clone(),
                        domain_id: proposal.domain_id.0.clone(),
                        candidate: candidate.clone(),
                        region: region.clone(),
                        bond_amount: *bond_amount,
                        term_length_seconds: *term_length,
                    },
                    icn_governance::sdis::SdisProposal::RemoveSteward {
                        steward, reason, ..
                    } => GovernanceEffect::RevokeSteward {
                        proposal_id: proposal.id.0.clone(),
                        steward: steward.clone(),
                        reason: reason.clone(),
                    },
                    _ => GovernanceEffect::Unhandled {
                        proposal_id: proposal.id.0.clone(),
                        payload_type: proposal.payload.type_name().to_owned(),
                    },
                },
                _ => GovernanceEffect::Unhandled {
                    proposal_id: proposal.id.0.clone(),
                    payload_type: proposal.payload.type_name().to_owned(),
                },
            };
            // Fire the fire-and-forget hook first.
            hook(effect.clone());

            // Evidence-returning hook: when wired, persist the returned
            // dispatch evidence against the just-emitted institutional
            // effect record. Effect kind must match one of our translated
            // labels — for Unhandled we skip (no record was emitted).
            if let Some(ev_hook) = &ctx.on_proposal_accepted_with_evidence {
                if let Some(spec) = ev_hook(&effect).await {
                    let effect_kind = match &effect {
                        GovernanceEffect::FreezeMember { .. } => Some("freeze_member"),
                        GovernanceEffect::UnfreezeMember { .. } => Some("unfreeze_member"),
                        GovernanceEffect::DeployCharter { .. } => Some("deploy_charter"),
                        GovernanceEffect::AppointSteward { .. } => Some("appoint_steward"),
                        GovernanceEffect::RevokeSteward { .. } => Some("revoke_steward"),
                        GovernanceEffect::Unhandled { .. } => None,
                    };
                    if let Some(kind) = effect_kind {
                        record_hook_dispatch_evidence(
                            &ctx,
                            &proposal.id.0,
                            kind,
                            spec,
                            current_time_secs(),
                        )
                        .await;
                    }
                }
            }
        }

        // SDIS service dispatch (test path only — daemon uses actor event system).
        // When sdis_service is wired and the accepted payload is an SDIS proposal,
        // call the service directly so tests can prove steward creation without an
        // actor runtime.
        //
        // The SDIS service returns structured {success, state_change_hash, error},
        // so this is the one dispatch path in close_proposal that yields real
        // in-process evidence. After each call we record an
        // `EffectDispatchEvidence` tied to the institutional effect record
        // persisted earlier in this handler. Other dispatch paths (charter/
        // freeze hooks) are fire-and-forget and remain `emitted_only`.
        //
        // Double-dispatch guard: for steward authority proposals, the
        // evidence-returning hook (`on_proposal_accepted_with_evidence`, wired
        // by the gateway to commons.register_steward / revoke_steward) is now
        // canonical. If both are configured, running the sdis_service block
        // below would re-enter register/revoke and produce conflicting
        // evidence. When the evidence hook is wired, skip sdis_service for
        // AppointSteward/RemoveSteward variants — other SDIS variants still
        // flow through sdis_service unchanged.
        let evidence_hook_handles_steward = ctx.on_proposal_accepted_with_evidence.is_some();
        if let Some(ref svc) = ctx.sdis_service {
            use icn_kernel_api::{AppointStewardRequest, RevokeStewardRequest};
            if let icn_governance::ProposalPayload::Sdis {
                proposal: ref sdis_proposal,
            } = proposal.payload
            {
                // Look up the institutional effect record persisted earlier
                // in this handler. We match by effect_kind to disambiguate if
                // historical records exist — for accept/revoke that is a
                // singleton per proposal in normal flow.
                let effect_kind_needed = match sdis_proposal {
                    icn_governance::sdis::SdisProposal::AppointSteward { .. } => {
                        Some("appoint_steward")
                    }
                    icn_governance::sdis::SdisProposal::RemoveSteward { .. } => {
                        Some("revoke_steward")
                    }
                    _ => None,
                };
                let target_record_id: Option<String> = match effect_kind_needed {
                    Some(kind) => ctx
                        .manager
                        .list_institutional_effects(&proposal_id)
                        .ok()
                        .and_then(|records| {
                            records
                                .into_iter()
                                .find(|r| r.effect_kind == kind)
                                .map(|r| r.record_id)
                        }),
                    None => None,
                };

                match sdis_proposal {
                    icn_governance::sdis::SdisProposal::AppointSteward { .. }
                    | icn_governance::sdis::SdisProposal::RemoveSteward { .. }
                        if evidence_hook_handles_steward =>
                    {
                        // Evidence hook is canonical for steward variants — skip
                        // sdis_service to avoid double-dispatch.
                    }
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
                        let (success, receipt_ref, error) = match svc.appoint_steward(req) {
                            Ok(result) => {
                                if !result.success {
                                    tracing::warn!(
                                        proposal_id = %proposal.id.0,
                                        error = ?result.error,
                                        "SDIS test-path: appoint_steward returned success=false"
                                    );
                                }
                                (result.success, Some(result.state_change_hash), result.error)
                            }
                            Err(e) => {
                                tracing::warn!(
                                    proposal_id = %proposal.id.0,
                                    error = %e,
                                    "SDIS test-path: appoint_steward call failed"
                                );
                                (false, None, Some(e.to_string()))
                            }
                        };
                        if let Some(record_id) = target_record_id.as_ref() {
                            // AppointSteward has no NoOp / Partial semantics at
                            // the service layer: success ⇒ Applied, failure ⇒
                            // Failed (classification mirrors the kernel
                            // executor, see supervisor::governance_executor).
                            let outcome = if success {
                                Some(icn_kernel_api::EffectOutcome::Applied)
                            } else {
                                Some(icn_kernel_api::EffectOutcome::Failed)
                            };
                            let evidence = crate::dispatch_evidence::EffectDispatchEvidence::new(
                                record_id.clone(),
                                proposal.id.0.clone(),
                                "sdis",
                                receipt_ref,
                                success,
                                error,
                                outcome,
                                current_time_secs(),
                            );
                            if let Err(e) = ctx.manager.record_dispatch_evidence(&evidence) {
                                tracing::error!(
                                    proposal_id = %proposal.id.0,
                                    effect_record_id = %record_id,
                                    error = %e,
                                    "Failed to persist SDIS appoint_steward dispatch evidence",
                                );
                            }
                        }
                    }
                    icn_governance::sdis::SdisProposal::RemoveSteward {
                        steward, reason, ..
                    } => {
                        let req = RevokeStewardRequest {
                            steward_did: steward.to_string(),
                            reason: reason.clone(),
                        };
                        let (success, receipt_ref, error) = match svc.revoke_steward(req) {
                            Ok(result) => {
                                if !result.success {
                                    tracing::warn!(
                                        proposal_id = %proposal.id.0,
                                        error = ?result.error,
                                        "SDIS test-path: revoke_steward returned success=false"
                                    );
                                }
                                (result.success, Some(result.state_change_hash), result.error)
                            }
                            Err(e) => {
                                tracing::warn!(
                                    proposal_id = %proposal.id.0,
                                    error = %e,
                                    "SDIS test-path: revoke_steward call failed"
                                );
                                (false, None, Some(e.to_string()))
                            }
                        };
                        if let Some(record_id) = target_record_id.as_ref() {
                            // RevokeSteward can be a NoOp when no active record
                            // exists for the target — but the HTTP test-path
                            // collapses that into `success = true` without
                            // surfacing the receipt_ref signal used by the
                            // kernel executor. Classify conservatively: mark
                            // `Applied` on success (may over-count NoOp in this
                            // legacy test-path; the kernel-owned executor is
                            // canonical for the NoOp distinction) and `Failed`
                            // on failure. The service here does not expose
                            // Partial mutations.
                            let outcome = if success {
                                Some(icn_kernel_api::EffectOutcome::Applied)
                            } else {
                                Some(icn_kernel_api::EffectOutcome::Failed)
                            };
                            let evidence = crate::dispatch_evidence::EffectDispatchEvidence::new(
                                record_id.clone(),
                                proposal.id.0.clone(),
                                "sdis",
                                receipt_ref,
                                success,
                                error,
                                outcome,
                                current_time_secs(),
                            );
                            if let Err(e) = ctx.manager.record_dispatch_evidence(&evidence) {
                                tracing::error!(
                                    proposal_id = %proposal.id.0,
                                    effect_record_id = %record_id,
                                    error = %e,
                                    "Failed to persist SDIS revoke_steward dispatch evidence",
                                );
                            }
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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

/// Record downstream dispatch evidence returned by the
/// evidence-returning hook against the institutional effect record for
/// `(proposal_id, effect_kind)`. Looks up the emitted record by scanning
/// the proposal's records for a matching `effect_kind`. Logs and skips
/// on any failure — dispatch evidence is best-effort reconciliation
/// metadata, not a correctness condition.
async fn record_hook_dispatch_evidence<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: &web::Data<GovernanceContext<E>>,
    proposal_id: &str,
    effect_kind: &str,
    spec: DispatchEvidenceSpec,
    now: u64,
) {
    use crate::dispatch_evidence::EffectDispatchEvidence;

    let records = match ctx
        .manager
        .list_institutional_effects(&icn_governance::ProposalId(proposal_id.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                proposal_id = %proposal_id,
                error = %e,
                "dispatch-evidence hook: failed to list institutional effects; skipping"
            );
            return;
        }
    };
    let Some(rec) = records.iter().find(|r| r.effect_kind == effect_kind) else {
        tracing::warn!(
            proposal_id = %proposal_id,
            effect_kind = %effect_kind,
            "dispatch-evidence hook returned Some but no matching effect record exists; skipping"
        );
        return;
    };

    let evidence = EffectDispatchEvidence::new(
        rec.record_id.clone(),
        proposal_id.to_string(),
        spec.subsystem,
        spec.receipt_ref,
        spec.success,
        spec.error_message,
        spec.outcome,
        now,
    );
    if let Err(e) = ctx.manager.record_dispatch_evidence(&evidence) {
        tracing::error!(
            proposal_id = %proposal_id,
            effect_kind = %effect_kind,
            error = %e,
            "Failed to persist hook-returned dispatch evidence"
        );
    }
}

/// GET /gov/proposals/{proposal_id}/deliberation — Reverse read-model.
///
/// Returns the proposal's deliberation trail: every meeting in the proposal's
/// domain whose agenda linked back to this proposal, with the agenda item's
/// discussion notes, outcome, and generated action items. When the proposal
/// is closed, the governance decision receipt is included. The `effect_kind`
/// field labels the [`crate::http::configure::GovernanceEffect`] variant the
/// payload would translate into on acceptance (`"freeze_member"`, etc., or
/// `"unhandled"`). It is a shape label only — dispatch evidence lives in
/// `.../chain`, and cryptographic evidence in `.../proof`.
///
/// This endpoint is NOT an activity/progress tracker. It answers:
/// "what deliberation produced the institutional decision on this proposal?"
pub async fn get_proposal_deliberation<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());
    let trail = ctx
        .manager
        .get_deliberation(&id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Proposal not found: {}", id.0)))?;

    // Only surface a governance decision when we have *both* a receipt and a
    // terminal `decided_at`. A receipt without a decided_at would otherwise
    // serialize as `decided_at: 0` (Unix epoch), which is a misleading
    // timestamp for consumers in the rare receipt-store/state-skew case.
    let governance_decision = trail.governance_receipt.as_ref().and_then(|r| {
        trail
            .decided_at
            .map(|decided_at| DeliberationDecisionResponse {
                outcome: r.outcome.to_string(),
                decided_at,
                decision_hash: hex::encode(r.decision_hash),
            })
    });

    let deliberations = trail
        .deliberations
        .into_iter()
        .map(|e| DeliberationMeetingResponse {
            meeting_id: e.meeting_id.0.clone(),
            meeting_title: e.meeting_title,
            meeting_status: meeting_status_str(&e.meeting_status).to_string(),
            scheduled_at: e.scheduled_at,
            started_at: e.started_at,
            ended_at: e.ended_at,
            agenda_item_id: e.agenda_item_id.0.to_string(),
            agenda_item_title: e.agenda_item_title,
            presenter: e.presenter,
            discussion_notes: e.discussion_notes,
            outcome: e.outcome,
            generated_action_items: e
                .generated_action_items
                .into_iter()
                .map(|aid| aid.0.to_string())
                .collect(),
        })
        .collect();

    let emitted_effects = trail
        .emitted_effects
        .iter()
        .map(reconciled_effect_to_response)
        .collect();

    Ok(HttpResponse::Ok().json(ProposalDeliberationResponse {
        proposal_id: trail.proposal_id.0,
        domain_id: trail.domain_id.0,
        payload_type: trail.payload_type.to_string(),
        state: trail.state_label.to_string(),
        effect_kind: trail.effect_kind.to_string(),
        deliberations,
        governance_decision,
        emitted_effects,
    }))
}

/// Translate a persisted effect record into its HTTP response shape.
fn effect_record_to_response(
    r: &crate::institutional_effect::InstitutionalEffectRecord,
) -> InstitutionalEffectResponse {
    InstitutionalEffectResponse {
        record_id: r.record_id.clone(),
        proposal_id: r.proposal_id.clone(),
        domain_id: r.domain_id.clone(),
        decision_hash: r.decision_hash.map(hex::encode),
        effect_kind: r.effect_kind.clone(),
        target_did: r.target_did.clone(),
        target_ref: r.target_ref.clone(),
        reason: r.reason.clone(),
        recorded_at: r.recorded_at,
        payload: r.payload.clone(),
    }
}

/// Translate a dispatch evidence record into its HTTP response shape.
fn dispatch_evidence_to_response(
    e: &crate::dispatch_evidence::EffectDispatchEvidence,
) -> DispatchEvidenceResponse {
    DispatchEvidenceResponse {
        evidence_id: e.evidence_id.clone(),
        effect_record_id: e.effect_record_id.clone(),
        proposal_id: e.proposal_id.clone(),
        subsystem: e.subsystem.clone(),
        receipt_ref: e.receipt_ref.clone(),
        success: e.success,
        error_message: e.error_message.clone(),
        recorded_at: e.recorded_at,
    }
}

/// Render a reconciled effect entry (record + evidence + derived status).
fn reconciled_effect_to_response(
    entry: &crate::manager::ReconciledEffectEntry,
) -> ReconciledEffectResponse {
    use crate::dispatch_evidence::{reconciliation_label, ReconciliationStatus};
    let reconciliation_error = match &entry.reconciliation_status {
        ReconciliationStatus::ExecutionFailed { error } => error.clone(),
        _ => None,
    };
    ReconciledEffectResponse {
        record: effect_record_to_response(&entry.record),
        reconciliation_status: reconciliation_label(&entry.reconciliation_status).to_string(),
        reconciliation_error,
        dispatch_evidence: entry
            .dispatch_evidence
            .iter()
            .map(dispatch_evidence_to_response)
            .collect(),
    }
}

/// GET /gov/proposals/{proposal_id}/effects — List persisted institutional
/// effect records emitted at acceptance.
///
/// Returns an empty list when the proposal was not accepted, when the
/// payload translated to `Unhandled`, or when no receipt store is wired.
/// Records are oldest-first.
///
/// Durable counterpart to the `effect_kind` shape label on `/deliberation`:
/// this endpoint answers "what governance artifact was actually created?"
/// rather than "what shape would this imply?"
pub async fn list_proposal_effects<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    proposal_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;

    let id = ProposalId(proposal_id.into_inner());

    // 404 when the proposal itself is unknown — distinguishes a missing
    // proposal from an accepted-but-no-effect proposal (which returns []).
    let _ = ctx
        .manager
        .get_proposal(&id)
        .await
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found(format!("Proposal not found: {}", id.0)))?;

    let records = ctx
        .manager
        .list_institutional_effects(&id)
        .map_err(anyhow_to_api)?;

    let effects = records
        .iter()
        .map(|record| {
            let evidence = ctx
                .manager
                .list_dispatch_evidence(&record.record_id)
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        record_id = %record.record_id,
                        error = %e,
                        "list_proposal_effects: evidence read failed; treating as empty",
                    );
                    vec![]
                });
            let reconciliation_status =
                crate::dispatch_evidence::derive_reconciliation_status(record, &evidence);
            crate::manager::ReconciledEffectEntry {
                record: record.clone(),
                dispatch_evidence: evidence,
                reconciliation_status,
            }
        })
        .map(|entry| reconciled_effect_to_response(&entry))
        .collect();

    Ok(HttpResponse::Ok().json(ProposalEffectsResponse {
        proposal_id: id.0,
        effects,
    }))
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:comment:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:comment:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:comment:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:comment:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:comment:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let creator_did = parse_did(&claims.sub, "Invalid DID in token")?;
    let domain = GovernanceDomainId(domain_id.into_inner());

    check_domain_membership(&ctx, &domain, &creator_did).await?;

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
    // Record the scope that actually authorized the request (evidence) — used
    // when this update transitions the item into Completed. Narrowest-first so
    // the narrow class scope is preferred when both are present.
    let (claims, presented_scope) = require_any_scope_matched::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let user_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx, &domain, &user_did).await?;

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

    // Parse the requested status (if any) BEFORE mutating anything else,
    // so a status string error rejects the request cleanly.
    let requested_status = if let Some(ref s) = req.status {
        Some(parse_status(s)?)
    } else {
        None
    };
    let prior_status = item.status;

    // Validate + apply non-status fields.
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
    if let Some(ref tags) = req.tags {
        val::validate_tags(tags)?;
        item.tags = tags.clone();
    }
    // Note: status is intentionally NOT applied here — see below.

    item.updated_at = current_time_secs();

    ctx.manager
        .update_action_item(&item)
        .map_err(anyhow_to_api)?;

    // If the caller requested a status change, route it through the same
    // receipt-emitting path the dedicated `/status` endpoint uses. This
    // closes the bypass: a full-update setting status=Completed cannot
    // commit without producing the ADR-0026 Layer 2
    // ActionItemCompletionReceipt that the action card's
    // `receipt_expected: true` advertises.
    let final_item = if let Some(new_status) = requested_status {
        if new_status != prior_status {
            ctx.manager
                .update_action_item_status(&domain, &id, new_status, &user_did, &presented_scope)
                .map_err(anyhow_to_api)?
        } else {
            item
        }
    } else {
        item
    };

    Ok(HttpResponse::Ok().json(action_item_to_response(&final_item)))
}

/// DELETE /gov/domains/{domain_id}/action-items/{item_id} — Delete an action item.
pub async fn delete_action_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let user_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx, &domain, &user_did).await?;

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
    // Record the scope that actually authorized the request (evidence): the
    // narrowed class scope when present, else the legacy broad scope during
    // the compatibility period. Listed narrowest-first so it is preferred.
    let (claims, presented_scope) = require_any_scope_matched::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let user_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx, &domain, &user_did).await?;

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
        .update_action_item_status(&domain, &id, new_status, &user_did, &presented_scope)
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let author_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    check_domain_membership(&ctx, &domain, &author_did).await?;

    let item = ctx
        .manager
        .add_action_item_note(&domain, &id, author_did, req.content.clone())
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// GET /gov/domains/{domain_id}/action-items/{item_id}/completion-receipt
/// — Retrieve the latest [`ActionItemCompletionReceipt`] persisted for an
/// action item, if any.
///
/// Closes the proof loop documented in `docs/dev/NYCN_K3S_PROOF_PATH.md`
/// and `docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md`: a holder shell that
/// completed an `action_item / complete` action card can read the
/// `ActionItemCompletionReceipt` back via HTTP, instead of relying on
/// in-process tests or on-disk Sled inspection.
///
/// Authorization mirrors the rest of the action-item read surface:
/// `governance:read` scope plus domain membership for the caller. The
/// receipt's bound `domain_id` is also asserted to match the path
/// parameter so a holder cannot probe across domain boundaries.
///
/// Returns:
/// - 200 with the receipt JSON when a completion receipt has been
///   persisted for the item under this domain.
/// - 404 when no receipt exists, when the item id does not match the
///   stored receipt's `domain_id`, or when the manager has no receipt
///   store wired (the receipt simply does not exist from the caller's
///   point of view).
#[utoipa::path(
    get,
    path = "/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt",
    tag = "governance",
    params(
        ("domain_id" = String, Path,
            description = "Governance domain the action item lives under"),
        ("item_id" = String, Path,
            description = "Action item id (UUID). This is the same string an \
                `action_item`-sourced action card carries in `source_id` — it links \
                the card to its receipt. Accepts any valid UUID form; canonicalized \
                before lookup.")
    ),
    responses(
        (status = 200,
            description = "Latest completion receipt persisted for the action item \
                under this domain. `record_hash` is the blake3 canonical record hash \
                binding the receipt fields, serialized as a JSON array of 32 bytes.",
            body = icn_governance::ActionItemCompletionReceipt),
        (status = 400, description = "Malformed action item id (not a valid UUID)"),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403,
            description = "Token lacks the `governance:read` scope, or the caller is \
                not a member of the domain"),
        (status = 404,
            description = "No completion receipt persisted for this item, the receipt \
                is bound to a different domain (cross-domain existence is not leaked), \
                or the domain does not exist")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_action_item_completion_receipt<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let caller_did = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    // Validate the item id format before any DB lookup so a malformed
    // path returns 400, not a missing-record 404. The receipt store
    // indexes by the canonical lowercase-hyphenated UUID string that
    // the manager wrote at receipt-emission time
    // (`ActionItemCompletionReceipt::new(item.id.to_string(), ...)`),
    // so we canonicalize the parsed id back to a string here. Without
    // this normalization, a caller passing the same UUID in
    // alternative-but-valid forms (uppercase hex, URN form,
    // braces-wrapped) would parse OK but miss the index entry and
    // receive a spurious 404.
    let canonical_item_id = parse_action_item_id(&item_id)?.to_string();

    check_domain_membership(&ctx, &domain, &caller_did).await?;

    let receipt = ctx
        .manager
        .get_action_item_completion_by_item(&canonical_item_id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| {
            err_not_found(format!(
                "No completion receipt found for action item: {canonical_item_id}"
            ))
        })?;

    if receipt.domain_id != domain.0 {
        // Receipt exists for the item id, but under a different
        // governance domain. From this caller's perspective the receipt
        // does not exist; do not leak cross-domain existence.
        return Err(err_not_found(format!(
            "No completion receipt found for action item: {canonical_item_id}"
        )));
    }

    Ok(HttpResponse::Ok().json(receipt))
}

/// POST /gov/domains/{domain_id}/process-sessions/{session_id}/gate-results
/// — Record one process-gate result and return its persisted
/// [`icn_governance::ProcessGateResultReceipt`] (issue #2144).
///
/// Mounts the existing `GovernanceManager::record_process_gate_result`
/// runtime slice — the first `ProcessTransitionReceipt` class for the
/// `idea-0019` Institutional Process Substrate (ADR-0026 Layer 2) — over
/// HTTP. The manager constructs a deterministic blake3 receipt and persists
/// it through the receipt backend *before* returning, so the response body
/// carries the persisted receipt and its `record_hash` is the verification
/// evidence. This adds no new receipt type and no process runtime.
///
/// Authorization accepts `governance:process:write` first and retains
/// `governance:write` as a compatibility fallback, then requires domain
/// membership for the caller. The manager performs no authority check itself — charter
/// enforcement of "who may record a gate result for this domain/session"
/// is upstream of this slice.
///
/// Returns:
/// - 200 with the persisted `ProcessGateResultReceipt` JSON.
/// - 400 when `result`/`gate_kind` is outside its closed taxonomy
///   (deserialization) or `session_id` is empty/whitespace.
/// - 401 when the bearer token is missing/invalid.
/// - 403 when the token lacks both accepted write scopes or the caller is not
///   a member of the domain.
/// - 404 when the domain does not exist.
pub async fn record_process_gate_result<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<RecordProcessGateResultRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:process:write", "governance:write"],
    )?;
    let recorded_by = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, session_id) = path.into_inner();
    if session_id.trim().is_empty() {
        return Err(err_bad("session_id must be a non-empty path segment"));
    }
    let domain = GovernanceDomainId(domain_id);

    // Reuse the existing domain-membership gate; do not introduce a new
    // authorization regime.
    check_domain_membership(&ctx, &domain, &recorded_by).await?;

    let receipt = ctx
        .manager
        .record_process_gate_result(
            &domain,
            &session_id,
            req.gate_kind,
            req.result,
            &recorded_by,
        )
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(receipt))
}

/// POST /gov/domains/{domain_id}/process-sessions/{session_id}/open
/// — Record that a process session was opened and return the persisted
/// [`icn_governance::ProcessSessionOpenedReceipt`] (the #2275 session
/// anchor; second `ProcessTransitionReceipt` class, ADR-0026 Layer 2).
///
/// Mounts `GovernanceManager::record_process_session_opened` over HTTP.
/// The backend enforces at most one opening per `(domain_id, session_id)`
/// atomically at the storage layer; a retry by the same authenticated
/// opener returns the ORIGINAL receipt unchanged (idempotent — the
/// response is byte-identical to the first success), and an open by a
/// different actor is refused with 409 (fail-closed; the original stays
/// untouched). Opening records institutional process context and grants
/// zero authority; `opened_by` is the authenticated caller as actor
/// evidence, not proof of permission.
///
/// Authorization mirrors the gate-result write surface exactly (per the
/// #2275 contract): process-class-first scope admission with broad fallback,
/// plus domain membership for the caller — sticky duplicate-open semantics would
/// otherwise let an authenticated outsider preempt a session id in
/// someone else's domain.
///
/// Returns:
/// - 200 with the persisted `ProcessSessionOpenedReceipt` JSON (first
///   open AND same-opener retry).
/// - 400 when `session_id` is empty/whitespace.
/// - 401 when the bearer token is missing/invalid.
/// - 403 when the token lacks both accepted write scopes or the caller is not
///   a member of the domain.
/// - 404 when the domain does not exist.
/// - 409 when the session was already opened by a different actor.
pub async fn open_process_session<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:process:write", "governance:write"],
    )?;
    let opened_by = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, session_id) = path.into_inner();
    if session_id.trim().is_empty() {
        return Err(err_bad("session_id must be a non-empty path segment"));
    }
    let domain = GovernanceDomainId(domain_id);

    // Reuse the existing domain-membership gate; do not introduce a new
    // authorization regime.
    check_domain_membership(&ctx, &domain, &opened_by).await?;

    match ctx
        .manager
        .record_process_session_opened(&domain, &session_id, &opened_by)
    {
        Ok(crate::manager::ProcessSessionOpenOutcome::Opened(receipt))
        | Ok(crate::manager::ProcessSessionOpenOutcome::AlreadyOpened(receipt)) => {
            Ok(HttpResponse::Ok().json(receipt))
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("process_session_open_conflict") {
                Err(ApiError::Conflict(msg))
            } else {
                Err(anyhow_to_api(e))
            }
        }
    }
}

/// POST /gov/domains/{domain_id}/process-sessions/{session_id}/deliberation-entries/{entry_id}/record
/// — Record one deliberation entry against an already-opened process
/// session and return the persisted
/// [`icn_governance::DeliberationEntryRecordedReceipt`] (the third
/// `ProcessTransitionReceipt` class, ADR-0026 Layer 2; #2277 contract +
/// #2278 Q3 taxonomy decision).
///
/// Mounts `GovernanceManager::record_deliberation_entry` over HTTP. The
/// request carries only the closed-taxonomy `entry_kind` and the 64-hex
/// `body_hash` fingerprint — **the deliberation body is never sent to,
/// stored by, or returned from this surface**. The backend enforces at
/// most one entry per `(domain_id, session_id, entry_id)` atomically at
/// the storage layer; a retry with identical stable identity (`author` +
/// `body_hash` + `entry_kind`) returns the ORIGINAL receipt unchanged
/// (idempotent — byte-identical response), and any mismatch is refused
/// with 409 (fail-closed; the original stays untouched). Recording grants
/// zero authority; `author` is the authenticated caller as actor
/// evidence, not proof of entitlement.
///
/// Authorization mirrors the session-open surface exactly:
/// process-class-first scope admission with broad fallback, plus domain
/// membership for the caller.
///
/// Returns:
/// - 200 with the persisted `DeliberationEntryRecordedReceipt` JSON
///   (first record AND same-identity retry).
/// - 400 when `domain_id`/`session_id`/`entry_id` is empty/whitespace,
///   `entry_kind` is outside the closed taxonomy (serde rejects it), or
///   `body_hash` is not 64 hex characters.
/// - 401 when the bearer token is missing/invalid.
/// - 403 when the token lacks both accepted write scopes or the caller is not
///   a member of the domain.
/// - 404 when the domain does not exist, or when the session has no
///   recorded opening (`deliberation_entry_session_not_opened`) — the
///   referenced anchor is absent, mirroring the existing missing-domain
///   mapping; nothing is written and no session is silently created.
/// - 409 when the entry was already recorded with a different author,
///   body hash, or entry kind.
pub async fn record_deliberation_entry<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String, String)>,
    req: web::Json<RecordDeliberationEntryRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:process:write", "governance:write"],
    )?;
    let author = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, session_id, entry_id) = path.into_inner();
    if domain_id.trim().is_empty() {
        return Err(err_bad("domain_id must be a non-empty path segment"));
    }
    if session_id.trim().is_empty() {
        return Err(err_bad("session_id must be a non-empty path segment"));
    }
    if entry_id.trim().is_empty() {
        return Err(err_bad("entry_id must be a non-empty path segment"));
    }
    // body_hash: exactly 32 bytes, hex-encoded. The body itself never
    // crosses this boundary.
    let body_hash_bytes = hex::decode(req.body_hash.trim())
        .map_err(|e| err_bad(format!("body_hash must be hex: {e}")))?;
    let body_hash: [u8; 32] = body_hash_bytes
        .try_into()
        .map_err(|_| err_bad("body_hash must be exactly 32 bytes (64 hex characters)"))?;
    let domain = GovernanceDomainId(domain_id);

    // Reuse the existing domain-membership gate; do not introduce a new
    // authorization regime.
    check_domain_membership(&ctx, &domain, &author).await?;

    match ctx.manager.record_deliberation_entry(
        &domain,
        &session_id,
        &entry_id,
        &author,
        req.entry_kind,
        body_hash,
    ) {
        Ok(crate::manager::DeliberationEntryRecordOutcome::Recorded(receipt))
        | Ok(crate::manager::DeliberationEntryRecordOutcome::AlreadyRecorded(receipt)) => {
            Ok(HttpResponse::Ok().json(receipt))
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("deliberation_entry_conflict") {
                Err(ApiError::Conflict(msg))
            } else if msg.starts_with("deliberation_entry_session_not_opened") {
                // The referenced session anchor is absent — map to 404 like
                // the missing-domain case (absent referenced resource), not
                // 409 (which this surface reserves for identity conflicts).
                Err(ApiError::NotFound(msg))
            } else {
                Err(anyhow_to_api(e))
            }
        }
    }
}

/// POST /gov/domains/{domain_id}/process-sessions/{session_id}/decisions/{decision_id}/record
/// — Record one decision against an already-opened process session and
/// return the persisted [`icn_governance::DecisionRecordedReceipt`] (the
/// fourth `ProcessTransitionReceipt` class, ADR-0026 Layer 2; #2280
/// contract + #2281 Q4 decision).
///
/// Mounts `GovernanceManager::record_decision` over HTTP. The request
/// carries only the 64-hex `body_hash` fingerprint — **the decision body
/// is never sent to, stored by, or returned from this surface**, and per
/// the Q4 decision there is no kind, outcome, tally, or deciding-body
/// field. This surface records a generic recorded-decision fact; it is
/// parallel to and never touches the proposal/vote decision endpoints,
/// their typed store, effect dispatch, or action cards. The backend
/// enforces at most one decision per
/// `(domain_id, session_id, decision_id)` atomically at the storage
/// layer; a retry with identical stable identity (`recorded_by` +
/// `body_hash`) returns the ORIGINAL receipt unchanged (idempotent —
/// byte-identical response), and any mismatch is refused with 409
/// (fail-closed; the original stays untouched). Recording grants zero
/// authority; `recorded_by` is the authenticated caller as recorder
/// evidence — not decider identity, not proof of entitlement.
///
/// Authorization mirrors the sibling process-receipt surfaces exactly:
/// process-class-first scope admission with broad fallback, plus domain
/// membership for the caller.
///
/// Returns:
/// - 200 with the persisted `DecisionRecordedReceipt` JSON (first record
///   AND same-identity retry).
/// - 400 when `domain_id`/`session_id`/`decision_id` is empty/whitespace
///   or `body_hash` is not 64 hex characters.
/// - 401 when the bearer token is missing/invalid.
/// - 403 when the token lacks both accepted write scopes or the caller is not
///   a member of the domain.
/// - 404 when the domain does not exist, or when the session has no
///   recorded opening (`decision_recorded_session_not_opened`) — the
///   referenced anchor is absent, mirroring the existing missing-domain
///   mapping; nothing is written and no session is silently created.
/// - 409 when the decision was already recorded with a different
///   recorder or body hash.
pub async fn record_decision<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String, String)>,
    req: web::Json<RecordDecisionRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:process:write", "governance:write"],
    )?;
    let recorded_by = parse_did(&claims.sub, "Invalid DID in token")?;

    let (domain_id, session_id, decision_id) = path.into_inner();
    if domain_id.trim().is_empty() {
        return Err(err_bad("domain_id must be a non-empty path segment"));
    }
    if session_id.trim().is_empty() {
        return Err(err_bad("session_id must be a non-empty path segment"));
    }
    if decision_id.trim().is_empty() {
        return Err(err_bad("decision_id must be a non-empty path segment"));
    }
    // body_hash: exactly 32 bytes, hex-encoded. The body itself never
    // crosses this boundary.
    let body_hash_bytes = hex::decode(req.body_hash.trim())
        .map_err(|e| err_bad(format!("body_hash must be hex: {e}")))?;
    let body_hash: [u8; 32] = body_hash_bytes
        .try_into()
        .map_err(|_| err_bad("body_hash must be exactly 32 bytes (64 hex characters)"))?;
    let domain = GovernanceDomainId(domain_id);

    // Reuse the existing domain-membership gate; do not introduce a new
    // authorization regime.
    check_domain_membership(&ctx, &domain, &recorded_by).await?;

    match ctx
        .manager
        .record_decision(&domain, &session_id, &decision_id, &recorded_by, body_hash)
    {
        Ok(crate::manager::DecisionRecordOutcome::Recorded(receipt))
        | Ok(crate::manager::DecisionRecordOutcome::AlreadyRecorded(receipt)) => {
            Ok(HttpResponse::Ok().json(receipt))
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("decision_recorded_conflict") {
                Err(ApiError::Conflict(msg))
            } else if msg.starts_with("decision_recorded_session_not_opened") {
                // The referenced session anchor is absent — map to 404 like
                // the missing-domain case (absent referenced resource), not
                // 409 (which this surface reserves for identity conflicts).
                Err(ApiError::NotFound(msg))
            } else {
                Err(anyhow_to_api(e))
            }
        }
    }
}

/// Map the persisted domain-policy adoption seam error onto the governance
/// HTTP error taxonomy. Fail-closed: only a clearly-authorization or
/// clearly-client problem maps to 403/404; everything else (missing store,
/// backend read/write failure, unreachable `AlreadyDeclared`) surfaces as 500.
fn institutional_domain_store_err_to_api(e: InstitutionalDomainStoreError) -> ApiError {
    use AdoptDomainPolicyError as Gate;
    use DomainPolicyAdoptionError as Adopt;
    use InstitutionalDomainStoreError as Store;
    let msg = e.to_string();
    match e {
        // No `InstitutionalDomain` declared for this domain id.
        Store::NotDeclared => err_not_found(msg),
        // Authority refusal — the gate denied the actor/domain/act/lifecycle,
        // or the pure-core structural commit rejected the adoption.
        Store::Adopt(Adopt::Gated(Gate::Unauthorized(_) | Gate::Core(_))) => err_forbidden(msg),
        // Misconfiguration or backend failure. `AlreadyDeclared` is unreachable
        // on the adopt path; fail closed to 500 if it ever surfaces.
        Store::MissingDomainStore
        | Store::AlreadyDeclared
        | Store::Store(_)
        | Store::Adopt(Adopt::MissingReceiptBackend)
        | Store::Adopt(Adopt::Gated(Gate::Backend(_))) => err_internal(msg),
    }
}

/// POST /gov/domains/{domain_id}/domain-policy/adopt
/// — Adopt a `DomainPolicy` as the persisted `InstitutionalDomain`'s current
/// policy, gated by the app-side `DefaultMandateGate` (issue #2142).
///
/// Drives the existing persisted adoption seam end-to-end over HTTP:
///
/// ```text
/// POST → governance:charter:write class scope (or broad compatibility fallback)
///      → domain-membership gate
///      → GovernanceManager::adopt_domain_policy_persisted
///        → load InstitutionalDomain
///        → DefaultMandateGate authority resolution (actor → active grants →
///          domain-scoped Execution `domain_policy:adopt` grant + live mandate)
///        → pure-core InstitutionalDomain::adopt_policy() (defense-in-depth)
///        → save InstitutionalDomain
///      → adopted DomainPolicyRef
/// ```
///
/// The adopting actor is the authenticated caller (`sub`), never a body field.
/// The candidate policy is content-addressed server-side from `policy_content`
/// and bound to the path `domain_id`, so the policy↔domain identity is correct
/// by construction (the pure-core `PolicyForOtherDomain` rejection is therefore
/// unreachable from this route). The manager seam serializes concurrent
/// same-domain adoptions through a per-domain critical section.
///
/// This adds **no** declare/create route — declaring a governed domain is not
/// mandate-gated yet, so exposing it would be an authority bypass — and no CCL
/// runtime, policy registry, service-binding runtime, or auth-model change.
///
/// Returns:
/// - 200 with the adopted policy ref projection (`{policy_id, domain_id}`).
/// - 400 when `policy_content` is empty/whitespace or exceeds
///   `MAX_DOMAIN_POLICY_CONTENT_BYTES`, or the body/DID is malformed.
/// - 401 when the bearer token is missing/invalid.
/// - 403 when the token lacks both accepted write scopes, the caller is not a
///   domain member, or the `DefaultMandateGate` refuses adoption authority.
/// - 404 when no `InstitutionalDomain` is declared for `domain_id` (or the
///   `GovernanceDomain` membership target does not exist).
/// - 500 when no domain/receipt store is wired, or a backend read/write fails.
pub async fn adopt_domain_policy<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<String>,
    req: web::Json<AdoptDomainPolicyRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:charter:write", "governance:write"],
    )?;
    // Observe-only (#2352): record how the *unchanged* charter gate was
    // satisfied (class vs. broad fallback) via the no-op emission seam. Returns
    // unit and defaults to the no-op emitter, so it cannot affect this request.
    observe_charter_domain_admission(&NoopFallbackObservationEmitter, &claims);
    let actor = parse_did(&claims.sub, "Invalid DID in token")?;

    // Reject empty/whitespace and oversize bodies before content-addressing
    // (blake3-hashing) the candidate — an unbounded body is a DoS vector.
    val::validate_domain_policy_content(&req.policy_content)?;
    let domain = GovernanceDomainId(path.into_inner());

    // Reuse the existing coarse domain-membership gate (the same gate the rest
    // of the domain-scoped write surface uses). The authoritative adopt check
    // is the DefaultMandateGate inside the manager seam below; membership is
    // strictly additive and fail-closed, not a substitute for it.
    check_domain_membership(&ctx, &domain, &actor).await?;

    // Content-address the candidate policy server-side and bind it to the path
    // domain, so the adopted ref can neither mismatch its id nor target a
    // foreign domain.
    let policy = DomainPolicy::new(domain.clone(), req.policy_content.as_bytes());

    // Persist the policy body alongside the adopted ref (#2180), passing the
    // same bytes the id was content-addressed from, so the adopted ref can be
    // resolved back to its text. The manager stores the body only after the gate
    // resolves and before the current_policy update, so a rejection persists
    // neither.
    let policy_ref = ctx
        .manager
        .adopt_domain_policy_persisted_with_body(
            &domain,
            &policy,
            Some(req.policy_content.as_bytes()),
            &actor,
            current_time_secs(),
        )
        .map_err(institutional_domain_store_err_to_api)?;

    Ok(HttpResponse::Ok().json(AdoptDomainPolicyResponse {
        policy_id: policy_ref.id.to_hex(),
        domain_id: policy_ref.domain_id.0,
    }))
}

/// Parse the optional hex `charter_id` request field into a [`CharterId`].
/// `None` stays `None`; a present value must be exactly 32 bytes (64 hex chars).
fn parse_optional_charter_id(charter_id: &Option<String>) -> Result<Option<CharterId>, ApiError> {
    let Some(s) = charter_id.as_ref() else {
        return Ok(None);
    };
    let trimmed = s.trim();
    // A `CharterId` is exactly 32 bytes (64 hex chars). Validate length BEFORE
    // decoding so an oversize body is never hex-decoded, and a wrong-length
    // value gets a precise 400 rather than a generic decode error.
    if trimmed.len() != 64 {
        return Err(err_bad(
            "charter_id must be exactly 64 hex characters (32 bytes)",
        ));
    }
    let bytes = hex::decode(trimmed)
        .map_err(|e| err_bad(format!("charter_id must be hex-encoded: {e}")))?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| err_bad("charter_id must be 32 bytes (64 hex characters)"))?;
    Ok(Some(CharterId::new(arr)))
}

/// Map the gated-declaration seam error onto the governance HTTP error
/// taxonomy. Fail-closed: only a clearly-authorization or already-declared
/// problem maps to 403/409; everything else (missing backend/store, gate
/// backend fault, unreachable variants) surfaces as 500.
fn declare_institutional_domain_err_to_api(e: DeclareInstitutionalDomainError) -> ApiError {
    use DeclareInstitutionalDomainError as D;
    use InstitutionalDomainStoreError as S;
    let msg = e.to_string();
    match e {
        // Gate denial (wrong actor/domain/act/class, expired/revoked) → 403.
        D::Unauthorized(_) => err_forbidden(msg),
        // A record already exists for this domain id → conflict.
        D::Store(S::AlreadyDeclared) => ApiError::Conflict(msg),
        // Misconfiguration / infrastructure faults → 500. `NotDeclared` and
        // `Adopt(_)` are unreachable on the declare path; fail closed to 500.
        D::MissingReceiptBackend
        | D::Backend(_)
        | D::Store(S::MissingDomainStore)
        | D::Store(S::Store(_))
        | D::Store(S::NotDeclared)
        | D::Store(S::Adopt(_)) => err_internal(msg),
    }
}

/// POST /gov/domains/{domain_id}/institutional-domain/declare
/// — Declare the `InstitutionalDomain` authority record for an existing
/// `GovernanceDomainId`, gated by the app-side `DefaultMandateGate` (issue
/// #2142).
///
/// Drives the mandate-gated declaration seam end-to-end over HTTP:
///
/// ```text
/// POST → governance:charter:write class scope (or broad compatibility fallback)
///      → GovernanceManager::declare_institutional_domain_gated
///        → DefaultMandateGate authority resolution (actor → active grants →
///          domain-scoped Execution `institutional_domain:declare` grant +
///          live mandate)
///        → (only on pass) persist a freshly-declared InstitutionalDomain
///      → declared-domain projection
/// ```
///
/// The declaring actor is the authenticated caller (`sub`), never a body field.
/// The route calls the **gated** seam, never the ungated bootstrap
/// `declare_institutional_domain` — authority is resolved before any store
/// write, so an unauthorized caller persists nothing and cannot learn whether a
/// domain already exists (the gate refuses before the duplicate check).
///
/// **Membership gate intentionally omitted.** Unlike the domain-scoped *write*
/// surface (which reuses `check_domain_membership`), declaration must not
/// require prior membership: the actor authorized to *establish* a governed
/// domain (by a domain-scoped declare grant) may not yet be a member of it, so a
/// membership precondition could wrongly block the first authorized declaration.
/// The `DefaultMandateGate` declare grant is the authoritative — and strictly
/// stronger — authority check here.
///
/// This adds no new authority primitive, no CCL runtime, no policy registry, and
/// no auth-model change.
///
/// Returns:
/// - 200 with the declared-domain projection.
/// - 400 when `entity_type` is out of taxonomy, `charter_id` is malformed, or
///   the body/DID is malformed.
/// - 401 when the bearer token is missing/invalid.
/// - 403 when the token lacks both accepted write scopes or the
///   `DefaultMandateGate` refuses declaration authority.
/// - 409 when an `InstitutionalDomain` is already declared for `domain_id`.
/// - 500 when no receipt/domain store is wired, or a backend read/write fails.
pub async fn declare_institutional_domain<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<String>,
    req: web::Json<DeclareInstitutionalDomainRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:charter:write", "governance:write"],
    )?;
    // Observe-only (#2352): record how the *unchanged* charter gate was
    // satisfied (class vs. broad fallback) via the no-op emission seam. Returns
    // unit and defaults to the no-op emitter, so it cannot affect this request.
    observe_charter_domain_admission(&NoopFallbackObservationEmitter, &claims);
    let actor = parse_did(&claims.sub, "Invalid DID in token")?;

    let domain = GovernanceDomainId(path.into_inner());
    let charter_ref = parse_optional_charter_id(&req.charter_id)?;

    // Authoritative check is the DefaultMandateGate inside the gated seam, which
    // resolves authority BEFORE any store write. The ungated bootstrap seam is
    // never reached from this route.
    let declared = ctx
        .manager
        .declare_institutional_domain_gated(
            domain,
            req.entity_type,
            charter_ref,
            &actor,
            current_time_secs(),
        )
        .map_err(declare_institutional_domain_err_to_api)?;

    Ok(HttpResponse::Ok().json(DeclareInstitutionalDomainResponse {
        domain_id: declared.domain_id.0,
        owning_entity_class: declared.owning_entity_class,
        charter_id: declared.charter_ref.map(|c| c.to_hex()),
        current_policy_id: declared.current_policy.map(|p| p.id.to_hex()),
    }))
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:proposal:write", "governance:write"],
    )?;
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
        authority_scope: r.authority_scope.clone(),
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
    require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
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
    // #1868 step 4: steward-class capability with accepted-also fallback to the
    // legacy broad `governance:write` until that scope is retired. Direct role
    // assignment grants steward authority, so it carries its own narrow scope;
    // steward *proposals* stay under the proposal-write family (a later rung).
    require_any_scope::<BasicClaims>(&http_req, &["governance:steward:write", "governance:write"])?;
    let sid = StructureId(structure_id.into_inner());
    let person_did = parse_did(&req.did, "Invalid DID in request")?;

    let assignment = ctx
        .manager
        .assign_role(
            sid,
            person_did,
            req.role.clone(),
            req.authority_scope.clone(),
        )
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
    require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
    let entity = entity_id.into_inner();
    let kind = parse_activity_kind(&req.kind)?;

    let parent_program_id = req
        .parent_program_id
        .as_deref()
        .map(|s| icn_governance::program::ProgramId(s.to_string()));
    let linked_structures = req
        .linked_structures
        .iter()
        .map(|id| StructureId(id.clone()))
        .collect();
    let a = ctx
        .manager
        .create_activity_with_links(
            entity,
            kind,
            req.name.clone(),
            req.description.clone(),
            req.start_date,
            req.end_date,
            linked_structures,
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let domain = GovernanceDomainId(domain_id.into_inner());

    check_domain_membership(
        &ctx,
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let id = MeetingId(meeting_id.into_inner());

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    check_domain_membership(
        &ctx,
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let id = MeetingId(meeting_id.into_inner());

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    check_domain_membership(
        &ctx,
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
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
///
/// Routes through `GovernanceManager::update_meeting_attendance` so a
/// transition into `Present` / `Remote` emits an
/// `icn_governance::MeetingAttendanceReceipt` (ADR-0026 Layer 2). The
/// authenticated caller is recorded as `recorded_by`; the request's
/// `did` is recorded as `attendee_did`. The two may differ —
/// attendance is steward-recorded, not self-attestation only.
pub async fn mark_attendance<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
    req: web::Json<MarkAttendanceRequest>,
) -> Result<HttpResponse, ApiError> {
    // Record the scope that actually authorized the request (evidence): the
    // narrowed class scope when present, else the legacy broad scope during the
    // compatibility period. Listed narrowest-first so it is preferred.
    let (claims, presented_scope) = require_any_scope_matched::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let id = MeetingId(meeting_id.into_inner());
    let status = parse_attendance_status(&req.status)?;
    let recorded_by = parse_did(&claims.sub, "Invalid DID in token")?;

    let m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    // Domain-membership check: only members of the meeting's governance domain
    // may mutate its state. `governance:write` alone is insufficient.
    check_domain_membership(&ctx, &GovernanceDomainId(m.domain_id.clone()), &recorded_by).await?;

    if matches!(
        m.status,
        MeetingStatus::Completed | MeetingStatus::Cancelled
    ) {
        return Ok(HttpResponse::UnprocessableEntity()
            .json(serde_json::json!({"error": "Cannot modify a completed or cancelled meeting"})));
    }

    if !m.attendees.iter().any(|a| a.did == req.did) {
        return Err(err_not_found("Attendee not found in this meeting"));
    }

    let updated = ctx
        .manager
        .update_meeting_attendance(&id, &req.did, status, &recorded_by, &presented_scope)
        .map_err(anyhow_to_api)?;
    Ok(HttpResponse::Ok().json(meeting_to_response(&updated)))
}

/// POST /gov/meetings/{meeting_id}/agenda — Add an agenda item.
pub async fn add_agenda_item<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    meeting_id: web::Path<String>,
    req: web::Json<AddAgendaItemRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
    let id = MeetingId(meeting_id.into_inner());

    let mut m = ctx
        .manager
        .get_meeting(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Meeting not found"))?;

    check_domain_membership(
        &ctx,
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:meeting:write", "governance:write"],
    )?;
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
        &ctx,
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
    MilestoneResponse {
        id: m.id.0.clone(),
        program_id: m.program_id.0.clone(),
        name: m.name.clone(),
        description: m.description.clone(),
        phase_index: m.phase_index,
        target_date: m.target_date,
        status: milestone_status_str(&m.status).to_string(),
        completion_criteria: m.completion_criteria.clone(),
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
    let domain = GovernanceDomainId(domain_id.into_inner());

    check_domain_membership(
        &ctx,
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

// ── Milestone endpoints ──────────────────────────────────────────────────────

/// POST /gov/programs/{program_id}/milestones — Create a milestone.
pub async fn create_milestone<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    program_id: web::Path<String>,
    req: web::Json<CreateMilestoneRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
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
        &ctx,
        &prog.domain_id,
        &parse_did(&claims.sub, "Invalid DID in token")?,
    )
    .await?;

    let m = ctx
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
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
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
    check_domain_membership(&ctx, &prog.domain_id, &actor).await?;

    let m = ctx
        .manager
        .update_milestone_status(&id, status, &actor)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(milestone_to_response(&m)))
}

// ── Program ↔ Activity relationship management ────────────────────────────────

/// PUT /gov/programs/{program_id}/activities/{activity_id}
///
/// Link an activity to a program, keeping `Program.activities` and
/// `Activity.parent_program_id` in sync. Idempotent — calling this when the
/// relationship already exists returns 204 without error.
///
/// Requires `governance:write` scope. The caller must be a member of the
/// program's governance domain.
pub async fn link_activity_to_program<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
    let (program_id_str, activity_id_str) = path.into_inner();
    let pid = ProgramId(program_id_str);
    let aid = ActivityId(activity_id_str);
    let actor = parse_did(&claims.sub, "Invalid DID in token")?;

    let prog = ctx
        .manager
        .get_program(&pid)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found"))?;
    check_domain_membership(&ctx, &prog.domain_id, &actor).await?;

    // Pre-check the activity exists so we return a deterministic 404 rather
    // than relying on string-matching the manager's error message.
    ctx.manager
        .get_activity(&aid)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Activity not found"))?;

    ctx.manager
        .link_activity_to_program(&pid, &aid)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::NoContent().finish())
}

/// DELETE /gov/programs/{program_id}/activities/{activity_id}
///
/// Remove the link between a program and an activity. Clears both
/// `Program.activities` and `Activity.parent_program_id`. Idempotent —
/// returns 204 whether or not the relationship existed.
///
/// Requires `governance:write` scope. The caller must be a member of the
/// program's governance domain.
pub async fn unlink_activity_from_program<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
    let (program_id_str, activity_id_str) = path.into_inner();
    let pid = ProgramId(program_id_str);
    let aid = ActivityId(activity_id_str);
    let actor = parse_did(&claims.sub, "Invalid DID in token")?;

    let prog = ctx
        .manager
        .get_program(&pid)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found"))?;
    check_domain_membership(&ctx, &prog.domain_id, &actor).await?;

    ctx.manager
        .unlink_activity_from_program(&pid, &aid)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::NoContent().finish())
}

// ── Milestone preview (read-only readiness) ───────────────────────────────────

/// GET /gov/milestones/{milestone_id}/preview — Read-only milestone readiness.
///
/// Reports whether the milestone is currently ready to advance, based on the
/// same observable ordering state a human operator would inspect before
/// marking it complete:
///
/// - the milestone is open (not `completed` / `skipped`) and not `blocked`,
/// - every earlier-phase milestone in the same program is `completed` or
///   `skipped`.
///
/// This endpoint performs **no mutation**, no status transition, and emits
/// no events. It deliberately does **not** evaluate `completion_criteria` —
/// the governance core treats criteria as free-form declarative text whose
/// interpretation belongs to an institution package. The criteria are
/// surfaced verbatim for caller inspection only.
///
/// Requires `governance:read`.
///
/// Returns 404 if the milestone does not exist. Also returns 404 if the
/// milestone's program is missing (orphaned milestone — should not occur,
/// but handled defensively rather than panicking).
pub async fn preview_milestone<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    milestone_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = MilestoneId(milestone_id.into_inner());

    // 1. Load the milestone (404 when absent).
    let milestone = ctx
        .manager
        .get_milestone(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Milestone not found"))?;

    // 2. Load the enclosing program for its status (informational context).
    //    An orphaned milestone (program deleted under it) returns 404 rather
    //    than crashing or fabricating a status.
    let program = ctx
        .manager
        .get_program(&milestone.program_id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found for milestone"))?;

    // 3. Load siblings ordered by phase_index (store guarantees ordering).
    let siblings = ctx
        .manager
        .list_milestones_by_program(&milestone.program_id)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(build_milestone_preview(&milestone, &program, &siblings)))
}

/// Pure function composing a [`MilestonePreviewResponse`] from the three
/// pieces of observable state. Split out so the readiness logic can be unit
/// tested without spinning up an HTTP stack.
fn build_milestone_preview(
    milestone: &icn_governance::Milestone,
    program: &icn_governance::Program,
    siblings: &[icn_governance::Milestone],
) -> MilestonePreviewResponse {
    // Collect earlier-phase milestones that are not yet terminal.
    let blocking: Vec<BlockingMilestoneSummary> = siblings
        .iter()
        .filter(|m| {
            m.phase_index < milestone.phase_index
                && !matches!(
                    m.status,
                    MilestoneStatus::Completed | MilestoneStatus::Skipped
                )
        })
        .map(|m| BlockingMilestoneSummary {
            id: m.id.0.clone(),
            name: m.name.clone(),
            phase_index: m.phase_index,
            status: milestone_status_str(&m.status).to_string(),
        })
        .collect();

    let earlier_complete = blocking.is_empty();
    let is_open = milestone.is_open();

    // Readiness: open, not blocked, earlier phases clear.
    let (ready, reason) = match milestone.status {
        MilestoneStatus::Completed => (false, Some("milestone is already completed".to_string())),
        MilestoneStatus::Skipped => (false, Some("milestone is skipped".to_string())),
        MilestoneStatus::Blocked => (false, Some("milestone is blocked".to_string())),
        MilestoneStatus::Pending | MilestoneStatus::InProgress => {
            if earlier_complete {
                (true, None)
            } else {
                let first = &blocking[0];
                (
                    false,
                    Some(format!(
                        "earlier milestone '{}' (phase {}) is {}",
                        first.name, first.phase_index, first.status
                    )),
                )
            }
        }
    };

    MilestonePreviewResponse {
        milestone_id: milestone.id.0.clone(),
        program_id: milestone.program_id.0.clone(),
        name: milestone.name.clone(),
        phase_index: milestone.phase_index,
        status: milestone_status_str(&milestone.status).to_string(),
        is_open,
        program_status: program_status_str(&program.status).to_string(),
        earlier_milestones_complete: earlier_complete,
        blocking_milestones: blocking,
        criteria_count: milestone.completion_criteria.len(),
        completion_criteria: milestone.completion_criteria.clone(),
        ready_to_advance: ready,
        reason,
    }
}

// ── Milestone history ──────────────────────────────────────────────────────────

/// GET /gov/milestones/{milestone_id}/history — Read-only lifecycle bookmark view.
///
/// Returns the observable status-transition history for a milestone.
///
/// **Coverage limitation**: the governance store records only two temporal
/// facts about a milestone:
/// - `created_at` — when the milestone was first persisted (initial status is
///   always `pending`).
/// - `completed_at` + `completed_by` — set only when the milestone moves into
///   `Completed`.
///
/// Intermediate transitions (`pending → in_progress`, `→ blocked`, etc.) are
/// **not** persisted. They will not appear in this response. The `coverage`
/// field in the response is always `"lifecycle_bookmarks"` in the current
/// implementation; callers must treat the entry list as a partial view, not a
/// complete audit log.
///
/// Requires `governance:read`.
///
/// Returns 404 if the milestone does not exist.
pub async fn get_milestone_history<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    milestone_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let id = MilestoneId(milestone_id.into_inner());

    let milestone = ctx
        .manager
        .get_milestone(&id)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Milestone not found"))?;

    // Load event log entries (empty Vec when no log is configured).
    let events = ctx
        .manager
        .list_milestone_events(&id)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(build_milestone_history(&milestone, &events)))
}

/// Pure function assembling a [`MilestoneHistoryResponse`] from a milestone
/// record and an optional list of recorded transition events.
///
/// **Two paths:**
///
/// 1. **Transition log** (`events` is non-empty): entries are built directly
///    from the event log. Coverage is `"transition_log"`. Each entry has
///    `from_status`, `to_status`, `changed_by`, and `changed_at` from the
///    recorded event. A synthetic creation entry is prepended (from
///    `milestone.created_at`) because the event log records status *changes*
///    only, not the initial creation.
///
/// 2. **Lifecycle bookmarks** (`events` is empty — no log configured or no
///    transitions recorded yet): same fallback as the previous implementation.
///    Coverage is `"lifecycle_bookmarks"`. Returns creation bookmark + optional
///    completion bookmark from milestone struct fields.
///
/// Entries are always ordered oldest-to-newest.
fn build_milestone_history(
    milestone: &icn_governance::Milestone,
    events: &[crate::manager::MilestoneEvent],
) -> MilestoneHistoryResponse {
    if events.is_empty() {
        // ── Fallback: lifecycle bookmarks (no event log available) ──────────
        let mut entries = Vec::new();

        // Creation (always present; initial status is always pending).
        entries.push(MilestoneHistoryEntry {
            changed_at: milestone.created_at,
            changed_by: None, // creator not recorded on milestone struct
            to_status: "pending".to_string(),
            source: "creation".to_string(),
        });

        // Completion (only when the milestone has completed).
        if let Some(completed_at) = milestone.completed_at {
            entries.push(MilestoneHistoryEntry {
                changed_at: completed_at,
                changed_by: milestone.completed_by.as_ref().map(|d| d.to_string()),
                to_status: "completed".to_string(),
                source: "completion_record".to_string(),
            });
        }

        MilestoneHistoryResponse {
            milestone_id: milestone.id.0.clone(),
            coverage: "lifecycle_bookmarks".to_string(),
            entries,
        }
    } else {
        // ── Full path: event log entries ────────────────────────────────────
        //
        // Prepend a synthetic creation entry. The event log records transitions
        // only; the creation itself is not emitted as a log entry. The creation
        // bookmark is always accurate (from milestone.created_at, initial status
        // is always pending) and provides the anchor for the entry sequence.
        let mut entries = Vec::with_capacity(events.len() + 1);
        entries.push(MilestoneHistoryEntry {
            changed_at: milestone.created_at,
            changed_by: None,
            to_status: "pending".to_string(),
            source: "creation".to_string(),
        });

        for event in events {
            entries.push(MilestoneHistoryEntry {
                changed_at: event.changed_at,
                changed_by: Some(event.changed_by.to_string()),
                to_status: milestone_status_str(&event.to_status).to_string(),
                source: "transition_log".to_string(),
            });
        }

        MilestoneHistoryResponse {
            milestone_id: milestone.id.0.clone(),
            coverage: "transition_log".to_string(),
            entries,
        }
    }
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

fn meeting_status_str(s: &MeetingStatus) -> &'static str {
    match s {
        MeetingStatus::Scheduled => "scheduled",
        MeetingStatus::InProgress => "in_progress",
        MeetingStatus::Completed => "completed",
        MeetingStatus::Cancelled => "cancelled",
    }
}

fn dashboard_meeting_summary(
    m: &icn_governance::Meeting,
    program_activity_ids: &std::collections::HashSet<&str>,
) -> DashboardMeetingSummary {
    DashboardMeetingSummary {
        id: m.id.0.clone(),
        title: m.title.clone(),
        status: meeting_status_str(&m.status).to_string(),
        scheduled_at: m.scheduled_at,
        // Only expose activity IDs that belong to THIS program. A meeting
        // may legitimately be linked to unrelated activities; those IDs are
        // intentionally omitted from the program-scoped dashboard summary.
        linked_activity_ids: m
            .linked_activities
            .iter()
            .filter(|a| program_activity_ids.contains(a.0.as_str()))
            .map(|a| a.0.clone())
            .collect(),
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
        meetings: {
            let program_activity_ids: std::collections::HashSet<&str> =
                d.activities.iter().map(|a| a.id.0.as_str()).collect();
            d.meetings
                .iter()
                .map(|m| dashboard_meeting_summary(m, &program_activity_ids))
                .collect()
        },
    };

    Ok(HttpResponse::Ok().json(resp))
}

// ── Program summary ───────────────────────────────────────────────────────────

/// GET /gov/programs/{program_id}/summary — Read-only milestone progression view.
///
/// Returns a lightweight roll-up of a program's current progression state:
/// - program status
/// - milestone counts by status
/// - all milestones ordered by `phase_index`
/// - the first open (not `completed` / `skipped`) milestone — i.e. where
///   work currently needs to go — as `next_unfinished_milestone`
/// - the `phase_index` of that milestone as `current_phase_index`
///
/// This is a focused alternative to `/dashboard`, which includes activities
/// and action items. The summary answers "where is this program in its
/// lifecycle?" without loading the heavier composite surfaces.
///
/// **Progression basis**: milestones are ordered by `phase_index` (the only
/// programmatic ordering signal in the current model). Milestone names and
/// `completion_criteria` text are surfaced verbatim and are **not** interpreted
/// — semantic meaning belongs in the institution package layer.
///
/// Requires `governance:read`. No mutation; no event emission.
///
/// Returns 404 if the program does not exist.
pub async fn get_program_summary<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    program_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let pid = ProgramId(program_id.into_inner());

    let program = ctx
        .manager
        .get_program(&pid)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found"))?;

    // Milestones come back ordered by phase_index (store contract).
    let milestones = ctx
        .manager
        .list_milestones_by_program(&pid)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(build_program_summary(&program, &milestones)))
}

/// Pure function composing a [`ProgramSummaryResponse`] from the program record
/// and its ordered milestones. Split out for unit-testability.
///
/// **`current_phase_index`**: the `phase_index` of the first open milestone
/// (lowest phase_index where status is not `Completed` / `Skipped`). If all
/// milestones are terminal, falls back to the highest `phase_index` present
/// (program likely closing / archived). `None` when there are no milestones.
fn build_program_summary(
    program: &icn_governance::Program,
    milestones: &[icn_governance::Milestone],
) -> ProgramSummaryResponse {
    // Count by status.
    let mut counts = DashboardMilestoneCounts {
        total: milestones.len(),
        ..Default::default()
    };
    for m in milestones {
        match m.status {
            MilestoneStatus::Completed => counts.completed += 1,
            MilestoneStatus::InProgress => counts.in_progress += 1,
            MilestoneStatus::Blocked => counts.blocked += 1,
            MilestoneStatus::Pending => counts.pending += 1,
            MilestoneStatus::Skipped => counts.skipped += 1,
        }
    }

    // First open milestone by phase_index (milestones already sorted ascending).
    let next_unfinished: Option<NextUnfinishedMilestone> = milestones
        .iter()
        .find(|m| m.is_open())
        .map(|m| NextUnfinishedMilestone {
            milestone_id: m.id.0.clone(),
            name: m.name.clone(),
            phase_index: m.phase_index,
            status: milestone_status_str(&m.status).to_string(),
        });

    // current_phase_index: open milestone's phase, falling back to highest
    // terminal phase when all milestones are done.
    let current_phase_index: Option<u32> = next_unfinished
        .as_ref()
        .map(|n| n.phase_index)
        .or_else(|| milestones.last().map(|m| m.phase_index));

    ProgramSummaryResponse {
        program_id: program.id.0.clone(),
        name: program.name.clone(),
        program_status: program_status_str(&program.status).to_string(),
        milestone_counts: counts,
        milestones: milestones.iter().map(dashboard_milestone_summary).collect(),
        next_unfinished_milestone: next_unfinished,
        current_phase_index,
        progress_basis: "phase_index_ordering".to_string(),
    }
}

fn parse_program_status(s: &str) -> Result<icn_governance::ProgramStatus, ApiError> {
    use icn_governance::ProgramStatus;
    match s {
        "draft" => Ok(ProgramStatus::Draft),
        "active_planning" => Ok(ProgramStatus::ActivePlanning),
        "public_launch" => Ok(ProgramStatus::PublicLaunch),
        "in_execution" => Ok(ProgramStatus::InExecution),
        "closed" => Ok(ProgramStatus::Closed),
        "archived" => Ok(ProgramStatus::Archived),
        _ => Err(err_bad(format!("Unknown program status: {s}"))),
    }
}

/// PATCH /gov/programs/{program_id}/status — Update a program's lifecycle status.
///
/// Records a [`ProgramEvent`] in the append-only event log when the status
/// actually changes (no-op transitions are silently ignored). The event log is
/// non-fatal: a write failure is logged but does not fail the request.
///
/// Requires `governance:write`. Caller must be a member of the program's domain.
///
/// Returns 404 if the program does not exist.
pub async fn update_program_status<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    program_id: web::Path<String>,
    req: web::Json<UpdateProgramStatusRequest>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_any_scope::<BasicClaims>(
        &http_req,
        &["governance:activity:write", "governance:write"],
    )?;
    let pid = ProgramId(program_id.into_inner());
    let status = parse_program_status(&req.status)?;
    let actor = parse_did(&claims.sub, "Invalid DID in token")?;

    // Load program first to resolve domain for membership check.
    let prog = ctx
        .manager
        .get_program(&pid)
        .map_err(anyhow_to_api)?
        .ok_or_else(|| err_not_found("Program not found"))?;
    check_domain_membership(&ctx, &prog.domain_id, &actor).await?;

    let p = ctx
        .manager
        .update_program_status(&pid, status, &actor)
        .map_err(anyhow_to_api)?;

    Ok(HttpResponse::Ok().json(program_to_response(&p)))
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

/// GET /gov/me/standing — Return the authenticated caller's institutional standing.
///
/// Composes existing governance state (domain membership + structure role
/// assignments) into one read model so member-facing UI does not have to
/// fan out to four endpoints.
///
/// ## What this returns
///
/// - The caller's DID (always — derived from the authenticated claims, not
///   from a query parameter).
/// - Each governance domain whose static member list contains the caller, or
///   whose membership is sourced from a trust threshold (status `"unverified"`
///   in that case — the trust graph is the source of truth, not this read
///   model).
/// - Each structure role assignment held by the caller, joined with cheap
///   structure metadata (name + parent entity) so the UI does not need a
///   second fetch per row.
/// - The deduplicated union of `authority_scope` across those role
///   assignments, as a convenience for UI.
///
/// ## Self-only
///
/// There is no `did` query parameter. The DID is always the caller's. A
/// caller cannot use this endpoint to inspect another DID's standing.
///
/// ## Optional filter
///
/// `?domain_id=<id>` narrows both `domains` and `roles` to assignments under
/// that domain. An unknown `domain_id` returns a valid empty standing rather
/// than a 404 — "no standing in that domain" is a meaningful answer.
///
/// ## Empty caller is not an error
///
/// A caller with no role assignments and no static membership receives a
/// 200 with empty `domains`, `roles`, and `authority_scopes`. UI should
/// render an onboarding affordance, not an error.
#[utoipa::path(
    get,
    path = "/gov/me/standing",
    tag = "governance",
    params(
        ("domain_id" = Option<String>, Query,
            description = "Narrow `domains` and `roles` to one governance domain. An \
                unknown `domain_id` returns a valid empty standing (200), not a 404 — \
                \"no standing in that domain\" is a meaningful answer.")
    ),
    responses(
        (status = 200,
            description = "The caller's institutional standing. Self-only: the DID is \
                always derived from the authenticated token; another DID's standing is \
                not reachable here. A caller with no memberships and no role assignments \
                receives 200 with empty `domains`, `roles`, and `authority_scopes` — \
                empty standing is not an error.",
            body = StandingResponse),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Token lacks the `governance:read` scope")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_my_standing<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    query: web::Query<StandingQuery>,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let caller = parse_did(&claims.sub, "Invalid DID in token")?;

    let domain_filter = query.domain_id.as_deref();

    // Domain memberships: scan all domains the manager knows about, keep
    // those where the caller is in the static list, mark trust-threshold
    // domains as "unverified" so the UI can prompt the user to look at the
    // trust-graph view rather than silently dropping them.
    let all_domains = ctx.manager.list_domains().await.map_err(anyhow_to_api)?;
    let mut domains: Vec<StandingDomainMembership> = all_domains
        .iter()
        .filter(|d| domain_filter.is_none_or(|id| d.id.0 == id))
        .filter_map(|d| match &d.config.membership.source {
            icn_governance::MembershipSource::StaticList(members) => {
                if members.contains(&caller) {
                    Some(StandingDomainMembership {
                        domain_id: d.id.0.clone(),
                        domain_name: d.name.clone(),
                        membership_source: "static_list".to_string(),
                        status: "member".to_string(),
                    })
                } else {
                    None
                }
            }
            icn_governance::MembershipSource::TrustThreshold(_) => Some(StandingDomainMembership {
                domain_id: d.id.0.clone(),
                domain_name: d.name.clone(),
                membership_source: "trust_threshold".to_string(),
                status: "unverified".to_string(),
            }),
        })
        .collect();
    domains.sort_by(|a, b| a.domain_id.cmp(&b.domain_id));

    // Role assignments: pull every role this caller holds, then join with
    // structure metadata. A genuinely-deleted structure (Ok(None)) still
    // surfaces as a row with `structure_name: None` so the caller can see
    // something to investigate. A storage/decode failure (Err) propagates
    // as 500 — silently dropping it would let a half-broken backend look
    // like a clean "structure deleted" answer.
    let role_records = ctx
        .manager
        .list_roles_for_person(&caller)
        .map_err(anyhow_to_api)?;
    let mut roles: Vec<StandingRoleAssignment> = Vec::with_capacity(role_records.len());
    for r in &role_records {
        let structure = ctx
            .manager
            .get_structure(&r.structure_id)
            .map_err(anyhow_to_api)?;
        let parent_entity_id = structure.as_ref().map(|s| s.parent_entity_id.clone());
        let structure_name = structure.as_ref().map(|s| s.name.clone());
        if let Some(filter_id) = domain_filter {
            // Domain filter currently restricts roles by parent entity id
            // when the entity id matches the requested domain id. This is
            // the loose-but-correct join while structures do not carry an
            // explicit governance-domain back-reference.
            if parent_entity_id.as_deref() != Some(filter_id) {
                continue;
            }
        }
        roles.push(StandingRoleAssignment {
            role_assignment_id: r.id.to_string(),
            structure_id: r.structure_id.0.clone(),
            structure_name,
            parent_entity_id,
            role: r.role.clone(),
            authority_scope: r.authority_scope.clone(),
            start_date: r.start_date,
            end_date: r.end_date,
        });
    }
    roles.sort_by(|a, b| {
        a.parent_entity_id
            .cmp(&b.parent_entity_id)
            .then_with(|| a.structure_id.cmp(&b.structure_id))
            .then_with(|| a.role.cmp(&b.role))
    });

    let mut authority_scopes: Vec<String> = roles
        .iter()
        .flat_map(|r| r.authority_scope.iter().cloned())
        .collect();
    authority_scopes.sort();
    authority_scopes.dedup();

    Ok(HttpResponse::Ok().json(StandingResponse {
        did: caller.to_string(),
        display_label: None,
        domains,
        roles,
        authority_scopes,
        generated_at: current_time_secs(),
    }))
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

/// GET /gov/me/action-cards — Return pending action cards for the caller.
///
/// Action cards are derived views — computed at request time from the caller's
/// `/me/standing` inputs plus open governance state. There is no mutation API;
/// the card carries a `source_id` and the holder acts on the underlying
/// object. See ADR-0027.
///
/// ## Sources currently emitted
///
/// - `proposal` / `vote` — Open proposals in domains where the caller has
///   voting standing and has not yet voted.
/// - `meeting` / `attend` — Meetings scheduled in the next 48 h where the
///   caller is on the attendee list.
/// - `action_item` / `complete` — Open action items assigned to the caller.
///
/// ## Sources reserved (not emitted today)
///
/// - `signal_rule` — pending icn#1631
/// - `obligation_lifecycle` — pending icn#1634
///
/// Per ADR-0027 the taxonomy is closed; new variants land alongside their
/// source-path implementation.
///
/// ## Self-only
///
/// There is no `did` query parameter. The caller's DID is always derived from
/// the authenticated claims; no card-set for a third party is reachable here.
#[utoipa::path(
    get,
    path = "/gov/me/action-cards",
    tag = "governance",
    responses(
        (status = 200,
            description = "Pending action cards for the caller, wrapped in \
                `ActionCardsResponse` (`{did, cards, generated_at}`) — **not** a bare \
                array. Cards are derived views computed at request time from the \
                caller's standing plus open governance state; there is no mutation API \
                on this surface — the holder acts on the underlying object named by \
                `source_id` (ADR-0027). `cards` may be empty. Source/action/scope/risk \
                fields use the closed enums `ActionCardSourceKind`, \
                `ActionCardActionKind`, `ActionCardScope`, and `ActionCardRiskLevel`.",
            body = ActionCardsResponse),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Token lacks the `governance:read` scope")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_my_action_cards<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let claims = require_scope::<BasicClaims>(&http_req, "governance:read")?;
    let caller = parse_did(&claims.sub, "Invalid DID in token")?;
    let now = current_time_secs();

    let mut cards: Vec<ActionCard> = Vec::new();

    // 1. Pending votes + upcoming meetings come from the existing digest
    //    pipeline. That pipeline already enforces "open proposal the caller
    //    has not yet voted on" and "meeting in the next 48 h where caller is
    //    on the attendee list" — exactly the membership/authority checks this
    //    surface needs.
    let digest = ctx.manager.generate_digest(&caller, now).await;

    for v in digest.pending_votes {
        cards.push(ActionCard {
            id: format!("card-proposal-{}-vote", v.proposal_id),
            source_kind: ActionCardSourceKind::Proposal,
            action_kind: ActionCardActionKind::Vote,
            scope: ActionCardScope::Entity,
            title: format!("Vote on proposal: {}", v.title),
            summary:
                "An open proposal awaits your vote in a domain where you have voting standing."
                    .to_string(),
            authority_basis: "domain_membership".to_string(),
            required_authority_scope: vec![format!("vote_in:{}", v.domain_id)],
            deadline: v.closes_at,
            risk_level: ActionCardRiskLevel::Normal,
            accessibility_hint:
                "Plain-language proposal summary available; alternate formats on request."
                    .to_string(),
            receipt_expected: true,
            source_id: v.proposal_id,
            domain_id: Some(v.domain_id),
        });
    }

    for m in digest.upcoming_meetings {
        cards.push(ActionCard {
            id: format!("card-meeting-{}-attend", m.meeting_id),
            source_kind: ActionCardSourceKind::Meeting,
            action_kind: ActionCardActionKind::Attend,
            scope: ActionCardScope::Structure,
            title: format!("Attend meeting: {}", m.title),
            summary: "A scheduled meeting includes you on the attendee list.".to_string(),
            authority_basis: "meeting_attendee".to_string(),
            required_authority_scope: Vec::new(),
            deadline: Some(m.scheduled_at),
            risk_level: ActionCardRiskLevel::Low,
            accessibility_hint:
                "Meeting accessibility provisions available on request from the host structure."
                    .to_string(),
            // Attending the meeting emits an `icn_governance::MeetingAttendanceReceipt`
            // (ADR-0026 Layer 2) keyed by `(meeting_id, caller_did)`. The card's
            // `source_id` is `meeting_id`; the `(source_id, caller)` pair is the
            // documented receipt lookup.
            receipt_expected: true,
            source_id: m.meeting_id,
            domain_id: Some(m.domain_id),
        });
    }

    // 2. Open assigned action items. `list_work_for_person` with the
    //    `assigned_to` filter (which sets `open_only: true`) covers the
    //    broader "any open assigned work" set — strictly larger than the
    //    digest's `overdue_items` subset.
    let work_filter = ActionItemFilter::assigned_to(caller.clone());
    let items = ctx
        .manager
        .list_work_for_person(&caller, &work_filter)
        .map_err(anyhow_to_api)?;

    for item in items {
        let item_id_str = item.id.to_string();
        cards.push(ActionCard {
            id: format!("card-action_item-{}-complete", item_id_str),
            source_kind: ActionCardSourceKind::ActionItem,
            action_kind: ActionCardActionKind::Complete,
            scope: ActionCardScope::Individual,
            title: format!("Complete: {}", item.title),
            summary: item
                .description
                .clone()
                .unwrap_or_else(|| "An assigned action item is awaiting your work.".to_string()),
            authority_basis: "assigned_action_item".to_string(),
            required_authority_scope: vec![format!("complete:{}", item_id_str)],
            deadline: item.due_date,
            risk_level: match item.priority {
                ActionItemPriority::Critical | ActionItemPriority::High => {
                    ActionCardRiskLevel::Elevated
                }
                ActionItemPriority::Medium => ActionCardRiskLevel::Normal,
                ActionItemPriority::Low => ActionCardRiskLevel::Low,
            },
            accessibility_hint:
                "Action items may be reassigned or accommodated; ask the owning structure."
                    .to_string(),
            receipt_expected: true,
            source_id: item_id_str,
            domain_id: Some(item.domain_id.0),
        });
    }

    // Stable order: deadline ascending (cards without a deadline last), then
    // id ascending. Deterministic so two identical requests return identical
    // payloads — required for the derived-view contract in ADR-0027.
    cards.sort_by(|a, b| match (a.deadline, b.deadline) {
        (Some(da), Some(db)) => da.cmp(&db).then_with(|| a.id.cmp(&b.id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });

    Ok(HttpResponse::Ok().json(ActionCardsResponse {
        did: caller.to_string(),
        cards,
        generated_at: now,
    }))
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
        GovernanceDomain, GovernanceDomainId, GovernanceParams, MembershipConfig,
        MembershipResolver, MembershipSource, ProposalId, ProposalPayload, ProposalScope,
        VoteChoice,
    };
    use icn_http_kit::auth::BasicClaims;
    use icn_identity::Did;

    use super::*;
    use crate::events::NoopEventEmitter;
    use crate::http::configure::{
        GovernanceContext, GovernanceContextBuildMode, GovernanceEffect, ProposalAcceptedHook,
        SuspensionChecker,
    };
    use crate::manager::{GovernanceManager, InMemoryMilestoneEventLog};

    fn test_did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    /// In-memory receipt backend usable from HTTP handler tests.
    ///
    /// Covers the minimum backend surface the HTTP close path touches:
    /// institutional effect records + dispatch evidence. Governance
    /// receipts and allocation receipts are no-ops here — these tests
    /// prove the canonical emission and evidence-hook behavior, not
    /// the economic chain.
    struct HandlerTestReceiptBackend {
        effects: Mutex<Vec<crate::institutional_effect::InstitutionalEffectRecord>>,
        evidence: Mutex<Vec<crate::dispatch_evidence::EffectDispatchEvidence>>,
    }

    impl HandlerTestReceiptBackend {
        fn new() -> Self {
            Self {
                effects: Mutex::new(vec![]),
                evidence: Mutex::new(vec![]),
            }
        }
    }

    impl crate::receipt_backend::GovernanceReceiptBackend for HandlerTestReceiptBackend {
        fn put_governance(
            &self,
            _receipt: &icn_governance::GovernanceDecisionReceipt,
        ) -> Result<(), String> {
            Ok(())
        }
        fn get_governance_by_proposal(
            &self,
            _proposal_id: &str,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn put_allocation(
            &self,
            _receipt: &icn_kernel_api::AllocationReceipt,
        ) -> Result<icn_kernel_api::Hash, String> {
            Ok([0u8; 32])
        }
        fn get_governance_by_decision(
            &self,
            _decision_hash: &icn_kernel_api::Hash,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn list_allocations_by_decision(
            &self,
            _decision_hash: &icn_kernel_api::Hash,
        ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
            Ok(vec![])
        }
        // Support the opaque seam so a scoped normal close can persist its
        // process-authorized v3 decision receipt (#1868). The production
        // gateway `ReceiptStore` overrides this; these handler tests don't
        // assert on v3 content, so accepting the write is sufficient to keep
        // the close from fail-closing on the v3 emission.
        fn put_opaque(
            &self,
            _class: &str,
            _key1: &str,
            _key2: Option<&str>,
            _recorded_at: u64,
            _record_hash: [u8; 32],
            _payload: &[u8],
        ) -> Result<(), String> {
            Ok(())
        }
        fn put_institutional_effect(
            &self,
            record: &crate::institutional_effect::InstitutionalEffectRecord,
        ) -> Result<(), String> {
            self.effects.lock().unwrap().push(record.clone());
            Ok(())
        }
        fn list_institutional_effects_by_proposal(
            &self,
            proposal_id: &str,
        ) -> Result<Vec<crate::institutional_effect::InstitutionalEffectRecord>, String> {
            let mut items: Vec<_> = self
                .effects
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.proposal_id == proposal_id)
                .cloned()
                .collect();
            items.sort_by_key(|r| r.recorded_at);
            Ok(items)
        }
        fn put_effect_dispatch_evidence(
            &self,
            evidence: &crate::dispatch_evidence::EffectDispatchEvidence,
        ) -> Result<(), String> {
            self.evidence.lock().unwrap().push(evidence.clone());
            Ok(())
        }
        fn list_effect_dispatch_evidence_by_record(
            &self,
            effect_record_id: &str,
        ) -> Result<Vec<crate::dispatch_evidence::EffectDispatchEvidence>, String> {
            let mut items: Vec<_> = self
                .evidence
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.effect_record_id == effect_record_id)
                .cloned()
                .collect();
            items.sort_by_key(|e| e.recorded_at);
            Ok(items)
        }
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

    /// Build a test app wiring the direct membership-mutation routes
    /// (`add_domain_member` / `remove_domain_member`) with the given caller
    /// injected as authenticated `governance:write` claims. Mirrors
    /// [`test_app`] but targets `/domains/{domain_id}/members` so the #1870
    /// TrustThreshold standing-resolution path is exercised through actix
    /// exactly as `configure` wires it in production.
    macro_rules! member_test_app {
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
                    .service(
                        web::resource("/domains/{domain_id}/members")
                            .route(web::post().to(add_domain_member::<NoopEventEmitter>))
                            .route(web::delete().to(remove_domain_member::<NoopEventEmitter>)),
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

    /// Same as `setup_freeze_proposal`, but with a receipt store wired so
    /// accepted execution-required payloads can persist closure artifacts.
    async fn setup_freeze_proposal_with_store(
        coop_id: &str,
        member_did: Did,
        target_did: Did,
        params: GovernanceParams,
    ) -> (Arc<GovernanceManager>, ProposalId) {
        let mgr = Arc::new(
            GovernanceManager::new().with_receipt_store(Arc::new(HandlerTestReceiptBackend::new())),
        );
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
        let (mgr, proposal_id) = setup_freeze_proposal_with_store(
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
            on_proposal_accepted_with_evidence: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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

    /// Proves the evidence-returning hook end-to-end through the HTTP
    /// `close_proposal` path:
    ///   normal accept
    ///   → canonical emission writes `InstitutionalEffectRecord`
    ///   → `on_proposal_accepted` fires
    ///   → `on_proposal_accepted_with_evidence` returns `DispatchEvidenceSpec`
    ///   → hook-returned evidence is persisted against the just-emitted record
    ///   → `list_dispatch_evidence` returns exactly that evidence entry
    ///   → derived `ReconciliationStatus` is `ExecutionEvidenced`.
    ///
    /// This is the narrow reconciliation bridge for the freeze dispatch lane
    /// — the lane was `emitted_only` before the evidence hook existed, and
    /// this test proves it is now wired through a supported path.
    #[tokio::test]
    async fn evidence_hook_persists_dispatch_evidence_on_accept() {
        use crate::dispatch_evidence::ReconciliationStatus;
        use crate::http::configure::{DispatchEvidenceSpec, ProposalDispatchEvidenceHook};

        let member_did = test_did(11);
        let target_did = test_did(12);

        // Build a manager wired with a receipt store so emission + evidence
        // are actually persistent (the base helper uses a bare manager).
        let mgr = Arc::new(
            GovernanceManager::new().with_receipt_store(Arc::new(HandlerTestReceiptBackend::new())),
        );
        let domain_id = GovernanceDomainId("evidence-coop".to_string());
        mgr.create_domain(
            domain_id.clone(),
            "evidence coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 0, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![member_did.clone()]),
            },
        )
        .await
        .unwrap();

        let proposal_id = ProposalId("evidence-freeze-1".to_string());
        mgr.create_proposal(
            proposal_id.clone(),
            domain_id.clone(),
            member_did.clone(),
            "Freeze for evidence test".to_string(),
            "audit".to_string(),
            ProposalPayload::FreezeMember {
                member: target_did.clone(),
                reason: "audit".to_string(),
                duration_seconds: None,
            },
            ProposalScope::Local,
        )
        .await
        .unwrap();
        mgr.open_proposal(proposal_id.clone(), 86_400)
            .await
            .unwrap();

        // The evidence hook simulates a downstream dispatcher reporting back
        // with a state_change_hash after successful side-effects.
        let ev_hook: ProposalDispatchEvidenceHook = Arc::new(move |effect| {
            let spec = match effect {
                GovernanceEffect::FreezeMember { .. } => Some(DispatchEvidenceSpec {
                    subsystem: "commons".to_string(),
                    receipt_ref: Some("state-hash-abc".to_string()),
                    success: true,
                    error_message: None,
                    outcome: Some(icn_kernel_api::EffectOutcome::Applied),
                }),
                _ => None,
            };
            Box::pin(async move { spec })
        });

        let ctx = GovernanceContext {
            manager: mgr.clone(),
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: Some(Arc::new(|_| {})), // fire-and-forget, also present
            on_proposal_accepted_with_evidence: Some(ev_hook),
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
        };

        let app = test_app!(ctx, member_did);
        let req = test::TestRequest::post()
            .uri(&format!("/proposals/{}/close", proposal_id.0))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);

        // Exactly one effect record was emitted.
        let records = mgr.list_institutional_effects(&proposal_id).unwrap();
        assert_eq!(records.len(), 1, "one emitted record expected");
        assert_eq!(records[0].effect_kind, "freeze_member");

        // Exactly one dispatch evidence entry was attached to that record.
        let evidence = mgr.list_dispatch_evidence(&records[0].record_id).unwrap();
        assert_eq!(evidence.len(), 1, "one dispatch evidence entry expected");
        assert_eq!(evidence[0].subsystem, "commons");
        assert_eq!(evidence[0].receipt_ref.as_deref(), Some("state-hash-abc"));
        assert!(evidence[0].success);

        // Reconciliation derives to ExecutionEvidenced on success.
        let status = crate::dispatch_evidence::derive_reconciliation_status(&records[0], &evidence);
        assert_eq!(status, ReconciliationStatus::ExecutionEvidenced);
    }

    /// Proves the `appoint_steward` dispatch lane produces real execution
    /// evidence on success:
    ///   SDIS AppointSteward proposal accepted
    ///   → `InstitutionalEffectRecord(effect_kind = "appoint_steward")`
    ///   → evidence hook awaits a simulated downstream steward registration,
    ///     returns `DispatchEvidenceSpec { success: true, receipt_ref: Some(id) }`
    ///   → dispatch evidence is persisted against the record
    ///   → derived `ReconciliationStatus` is `ExecutionEvidenced`.
    ///
    /// The simulated downstream is the async commons `register_steward` call
    /// in gateway/server.rs — same contract, substituted for a sync return
    /// here to avoid pulling in commons.
    #[tokio::test]
    async fn evidence_hook_persists_dispatch_evidence_on_appoint_steward_success() {
        use crate::dispatch_evidence::ReconciliationStatus;
        use crate::http::configure::{DispatchEvidenceSpec, ProposalDispatchEvidenceHook};
        use icn_governance::sdis::SdisProposal;

        let steward_did = test_did(11);
        let candidate_did = test_did(22);

        let mgr = Arc::new(
            GovernanceManager::new().with_receipt_store(Arc::new(HandlerTestReceiptBackend::new())),
        );
        let domain_id = GovernanceDomainId("steward-coop".to_string());
        mgr.create_domain(
            domain_id.clone(),
            "steward coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 0, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![steward_did.clone()]),
            },
        )
        .await
        .unwrap();

        let proposal_id = ProposalId("appoint-1".to_string());
        mgr.create_proposal(
            proposal_id.clone(),
            domain_id.clone(),
            steward_did.clone(),
            "Appoint candidate as steward".to_string(),
            "appoint_steward".to_string(),
            ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: candidate_did.clone(),
                    sponsors: vec![steward_did.clone()],
                    region: domain_id.0.clone(),
                    bond_amount: 1000,
                    term_length: 31_536_000, // 1 year
                },
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
            steward_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();

        let simulated_steward_id = "abcd1234".to_string();
        let sid_clone = simulated_steward_id.clone();
        let ev_hook: ProposalDispatchEvidenceHook = Arc::new(move |effect| {
            let sid = sid_clone.clone();
            let matched = matches!(effect, GovernanceEffect::AppointSteward { .. });
            Box::pin(async move {
                if matched {
                    Some(DispatchEvidenceSpec {
                        subsystem: "commons".to_string(),
                        receipt_ref: Some(sid),
                        success: true,
                        error_message: None,
                        outcome: Some(icn_kernel_api::EffectOutcome::Applied),
                    })
                } else {
                    None
                }
            })
        });

        let ctx = GovernanceContext {
            manager: mgr.clone(),
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: Some(Arc::new(|_| {})),
            on_proposal_accepted_with_evidence: Some(ev_hook),
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
        };

        let app = test_app!(ctx, steward_did);
        let req = test::TestRequest::post()
            .uri(&format!("/proposals/{}/close", proposal_id.0))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);

        let records = mgr.list_institutional_effects(&proposal_id).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].effect_kind, "appoint_steward");

        let evidence = mgr.list_dispatch_evidence(&records[0].record_id).unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].subsystem, "commons");
        assert_eq!(
            evidence[0].receipt_ref.as_deref(),
            Some(simulated_steward_id.as_str())
        );
        assert!(evidence[0].success);
        assert!(evidence[0].error_message.is_none());

        let status = crate::dispatch_evidence::derive_reconciliation_status(&records[0], &evidence);
        assert_eq!(status, ReconciliationStatus::ExecutionEvidenced);
    }

    /// Proves the `appoint_steward` dispatch lane records failures durably:
    ///   downstream register_steward fails (e.g. no holder record, unratified charter)
    ///   → `InstitutionalEffectRecord(effect_kind = "appoint_steward")`
    ///   → evidence hook returns `DispatchEvidenceSpec { success: false, error_message }`
    ///   → dispatch evidence is persisted
    ///   → derived `ReconciliationStatus` is `ExecutionFailed`.
    ///
    /// Failure must be durable, not silently swallowed — this is what makes
    /// reconciliation truthful. A later retry succeeding elsewhere must not
    /// overwrite this failure; that stickiness is enforced by the status
    /// derivation itself.
    #[tokio::test]
    async fn evidence_hook_persists_failure_on_appoint_steward_error() {
        use crate::dispatch_evidence::ReconciliationStatus;
        use crate::http::configure::{DispatchEvidenceSpec, ProposalDispatchEvidenceHook};
        use icn_governance::sdis::SdisProposal;

        let steward_did = test_did(11);
        let candidate_did = test_did(24);

        let mgr = Arc::new(
            GovernanceManager::new().with_receipt_store(Arc::new(HandlerTestReceiptBackend::new())),
        );
        let domain_id = GovernanceDomainId("steward-fail-coop".to_string());
        mgr.create_domain(
            domain_id.clone(),
            "steward fail coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 0, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![steward_did.clone()]),
            },
        )
        .await
        .unwrap();

        let proposal_id = ProposalId("appoint-fail-1".to_string());
        mgr.create_proposal(
            proposal_id.clone(),
            domain_id.clone(),
            steward_did.clone(),
            "Appoint — will fail downstream".to_string(),
            "appoint_steward".to_string(),
            ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: candidate_did.clone(),
                    sponsors: vec![steward_did.clone()],
                    region: domain_id.0.clone(),
                    bond_amount: 0,
                    term_length: 31_536_000,
                },
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
            steward_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();

        let ev_hook: ProposalDispatchEvidenceHook = Arc::new(move |effect| {
            let matched = matches!(effect, GovernanceEffect::AppointSteward { .. });
            Box::pin(async move {
                if matched {
                    Some(DispatchEvidenceSpec {
                        subsystem: "commons".to_string(),
                        receipt_ref: None,
                        success: false,
                        error_message: Some("candidate has no holder record".to_string()),
                        outcome: Some(icn_kernel_api::EffectOutcome::Failed),
                    })
                } else {
                    None
                }
            })
        });

        let ctx = GovernanceContext {
            manager: mgr.clone(),
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: Some(Arc::new(|_| {})),
            on_proposal_accepted_with_evidence: Some(ev_hook),
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
        };

        let app = test_app!(ctx, steward_did);
        let req = test::TestRequest::post()
            .uri(&format!("/proposals/{}/close", proposal_id.0))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);

        let records = mgr.list_institutional_effects(&proposal_id).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].effect_kind, "appoint_steward");

        let evidence = mgr.list_dispatch_evidence(&records[0].record_id).unwrap();
        assert_eq!(evidence.len(), 1);
        assert!(!evidence[0].success);
        assert_eq!(
            evidence[0].error_message.as_deref(),
            Some("candidate has no holder record"),
        );

        let status = crate::dispatch_evidence::derive_reconciliation_status(&records[0], &evidence);
        match status {
            ReconciliationStatus::ExecutionFailed { error } => {
                assert_eq!(error.as_deref(), Some("candidate has no holder record"));
            }
            other => panic!("expected ExecutionFailed, got {other:?}"),
        }
    }

    /// Proves the `revoke_steward` dispatch lane mirrors the appoint lane:
    /// on successful commons revocation, evidence is persisted with the
    /// commons steward-id as receipt_ref and reconciliation derives to
    /// `ExecutionEvidenced`.
    #[tokio::test]
    async fn evidence_hook_persists_dispatch_evidence_on_revoke_steward_success() {
        use crate::dispatch_evidence::ReconciliationStatus;
        use crate::http::configure::{DispatchEvidenceSpec, ProposalDispatchEvidenceHook};
        use icn_governance::sdis::SdisProposal;

        let steward_did = test_did(11);
        let target_steward = test_did(26);

        let mgr = Arc::new(
            GovernanceManager::new().with_receipt_store(Arc::new(HandlerTestReceiptBackend::new())),
        );
        let domain_id = GovernanceDomainId("revoke-coop".to_string());
        mgr.create_domain(
            domain_id.clone(),
            "revoke coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 0, 86_400),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![steward_did.clone()]),
            },
        )
        .await
        .unwrap();

        let proposal_id = ProposalId("revoke-1".to_string());
        mgr.create_proposal(
            proposal_id.clone(),
            domain_id.clone(),
            steward_did.clone(),
            "Revoke target steward".to_string(),
            "revoke_steward".to_string(),
            ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: target_steward.clone(),
                    reason: "performance".to_string(),
                    return_bond: true,
                },
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
            steward_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();

        let sid = "revoked-id-xyz".to_string();
        let sid_clone = sid.clone();
        let ev_hook: ProposalDispatchEvidenceHook = Arc::new(move |effect| {
            let sid = sid_clone.clone();
            let matched = matches!(effect, GovernanceEffect::RevokeSteward { .. });
            Box::pin(async move {
                if matched {
                    Some(DispatchEvidenceSpec {
                        subsystem: "commons".to_string(),
                        receipt_ref: Some(sid),
                        success: true,
                        error_message: None,
                        outcome: Some(icn_kernel_api::EffectOutcome::Applied),
                    })
                } else {
                    None
                }
            })
        });

        let ctx = GovernanceContext {
            manager: mgr.clone(),
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: Some(Arc::new(|_| {})),
            on_proposal_accepted_with_evidence: Some(ev_hook),
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
        };

        let app = test_app!(ctx, steward_did);
        let req = test::TestRequest::post()
            .uri(&format!("/proposals/{}/close", proposal_id.0))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 200);

        let records = mgr.list_institutional_effects(&proposal_id).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].effect_kind, "revoke_steward");

        let evidence = mgr.list_dispatch_evidence(&records[0].record_id).unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].receipt_ref.as_deref(), Some(sid.as_str()));
        assert!(evidence[0].success);

        let status = crate::dispatch_evidence::derive_reconciliation_status(&records[0], &evidence);
        assert_eq!(status, ReconciliationStatus::ExecutionEvidenced);
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
            on_proposal_accepted_with_evidence: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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

        let mgr = Arc::new(
            GovernanceManager::new().with_receipt_store(Arc::new(HandlerTestReceiptBackend::new())),
        );
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
            on_proposal_accepted_with_evidence: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: Some(suspension_checker),
            membership_resolver: Some(resolver),
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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

    // ── #1870: TrustThreshold direct membership-mutation standing resolution ──
    //
    // These tests exercise `add_domain_member` / `remove_domain_member` for
    // `TrustThreshold` domains through actix. They prove the fail-open closed
    // by #1870: the authorization gate must consult `membership_resolver`
    // rather than treating every caller as a member.
    //
    // Note on the "allowed" path: the in-memory `GovernanceManager` always
    // rejects direct member-list mutation of a `TrustThreshold` domain (it
    // returns "Cannot modify members of trust-based membership domain"). That
    // is a pre-existing, orthogonal safety net (HTTP 500), NOT the #1870
    // authorization fail-closed path (HTTP 403). So "authorization allowed"
    // is asserted as "not 403, request reaches the manager", and
    // "authorization rejected" is asserted as "403 + never reaches the
    // manager".

    /// Configurable test resolver. `Ok(members)` resolves a concrete eligible
    /// set; `Err(_)` simulates an unresolved trust graph (resolution failure).
    struct FakeMembershipResolver {
        outcome: Result<Vec<Did>, String>,
    }

    impl MembershipResolver for FakeMembershipResolver {
        fn resolve_members(&self, _domain: &GovernanceDomain) -> anyhow::Result<Vec<Did>> {
            self.outcome.clone().map_err(|e| anyhow::anyhow!(e))
        }
    }

    /// Create an in-memory manager owning a single `TrustThreshold` domain.
    async fn trust_threshold_domain(domain_id: &str) -> Arc<GovernanceManager> {
        let mgr = Arc::new(GovernanceManager::new());
        mgr.create_domain(
            GovernanceDomainId(domain_id.to_string()),
            "TrustThreshold Coop".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(0, 0, 86_400),
            MembershipConfig {
                source: MembershipSource::TrustThreshold(0.3),
            },
        )
        .await
        .unwrap();
        mgr
    }

    /// Build a governance context varying only the fields these tests need
    /// (`membership_resolver`, `build_mode`); everything else is unwired.
    fn membership_mutation_ctx(
        mgr: Arc<GovernanceManager>,
        membership_resolver: Option<Arc<dyn MembershipResolver>>,
        build_mode: GovernanceContextBuildMode,
    ) -> GovernanceContext<NoopEventEmitter> {
        GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_charter_accepted: None,
            on_proposal_accepted: None,
            on_proposal_accepted_with_evidence: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver,
            sdis_service: None,
            mandate_gate: None,
            build_mode,
        }
    }

    /// #1870 AC1: a TrustThreshold caller whose standing the configured
    /// resolver confirms is admitted by the authorization gate (it then hits
    /// the manager's TrustThreshold safety net, which is orthogonal).
    #[tokio::test]
    async fn add_member_trust_threshold_resolved_standing_allows() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();
        let new_member = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-add-ok").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Ok(vec![caller.clone()]),
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = member_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-add-ok/members")
                .set_json(serde_json::json!({ "did": new_member.to_string() }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_ne!(
            status, 403,
            "resolver-confirmed TrustThreshold caller must not be blocked by the membership gate; \
             got body: {body_str}"
        );
        assert!(
            !body_str.contains("unresolved_standing"),
            "confirmed standing must not be reported as unresolved; got: {body_str}"
        );
        assert!(
            body_str.contains("trust-based membership domain"),
            "request must reach the manager's TrustThreshold safety net (proving the membership \
             gate granted authorization), not be blocked at the gate; got status {status}, \
             body: {body_str}"
        );
    }

    /// #1870 AC2: when the resolver cannot determine the caller's standing
    /// (unresolved), the mutation is rejected outright with an explicit code
    /// rather than falling open. Pre-fix this fell through to the manager.
    #[tokio::test]
    async fn add_member_trust_threshold_unresolved_standing_rejected() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();
        let new_member = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-add-unresolved").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Err("trust graph unavailable".to_string()),
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = member_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-add-unresolved/members")
                .set_json(serde_json::json!({ "did": new_member.to_string() }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "unresolved TrustThreshold standing must be rejected, not allowed (fail-open #1870); \
             got body: {body_str}"
        );
        assert!(
            body_str.contains("unresolved_standing"),
            "rejection must carry the explicit unresolved_standing code; got: {body_str}"
        );
    }

    /// A TrustThreshold caller the resolver positively excludes (resolved
    /// non-member) is rejected with 403.
    #[tokio::test]
    async fn add_member_trust_threshold_resolved_non_member_rejected() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();
        let other = KeyPair::generate().unwrap().did().clone();
        let new_member = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-add-nonmember").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Ok(vec![other]), // caller is not in the resolved member set
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = member_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-add-nonmember/members")
                .set_json(serde_json::json!({ "did": new_member.to_string() }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "resolver-excluded TrustThreshold caller must not be able to add members; \
             got body: {body_str}"
        );
        assert!(
            body_str.contains("domain members"),
            "rejection should explain that only domain members may mutate; got: {body_str}"
        );
    }

    /// Remove-path parity: unresolved TrustThreshold standing rejects on the
    /// remove handler too (the fail-open existed identically in both).
    #[tokio::test]
    async fn remove_member_trust_threshold_unresolved_standing_rejected() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();
        let target = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-remove-unresolved").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Err("trust graph unavailable".to_string()),
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = member_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri("/domains/tt-remove-unresolved/members")
                .set_json(serde_json::json!({ "did": target.to_string() }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "unresolved TrustThreshold standing must reject member removal (fail-open #1870); \
             got body: {body_str}"
        );
        assert!(
            body_str.contains("unresolved_standing"),
            "rejection must carry the explicit unresolved_standing code; got: {body_str}"
        );
    }

    /// #1870 AC3: in Production posture an unconfigured resolver must reject
    /// the mutation outright rather than fall open. Reachable here because
    /// the test wires routes directly, bypassing the `configure()` startup
    /// guard that would otherwise panic on this configuration.
    #[tokio::test]
    async fn add_member_trust_threshold_production_without_resolver_rejected() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();
        let new_member = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-add-prod-noresolver").await;
        let ctx = membership_mutation_ctx(mgr, None, GovernanceContextBuildMode::Production);
        let app = member_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-add-prod-noresolver/members")
                .set_json(serde_json::json!({ "did": new_member.to_string() }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "production posture with no resolver must reject TrustThreshold mutation, not fall \
             open; got body: {body_str}"
        );
        assert!(
            body_str.contains("unresolved_standing"),
            "rejection must carry the explicit unresolved_standing code; got: {body_str}"
        );
    }

    /// Backward-compatibility guard: Bootstrap/dev posture with no resolver
    /// preserves the historical permissive behavior (the gate admits the
    /// caller, who then hits the manager's TrustThreshold safety net). Only
    /// Production is tightened by #1870, so existing dev/devnet flows are
    /// unaffected.
    #[tokio::test]
    async fn add_member_trust_threshold_bootstrap_without_resolver_still_permissive() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();
        let new_member = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-add-bootstrap-noresolver").await;
        let ctx = membership_mutation_ctx(mgr, None, GovernanceContextBuildMode::Bootstrap);
        let app = member_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-add-bootstrap-noresolver/members")
                .set_json(serde_json::json!({ "did": new_member.to_string() }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_ne!(
            status, 403,
            "Bootstrap posture without a resolver must remain permissive (no behavior change); \
             got body: {body_str}"
        );
        assert!(
            body_str.contains("trust-based membership domain"),
            "permissive gate must let the request reach the manager's TrustThreshold safety net; \
             got status {status}, body: {body_str}"
        );
    }

    // ── #1913: TrustThreshold standing resolution in check_domain_membership ──
    //
    // `check_domain_membership` is the shared gate behind ~20 read/write
    // governance endpoints. These tests exercise two representative consumers
    // (`create_action_item`, `create_meeting`) through actix for a
    // `TrustThreshold` domain, proving the same fail-open closed by #1870 in
    // the direct mutation handlers is also closed here: the gate must consult
    // `membership_resolver` rather than treating every caller as a member.
    //
    // Unlike the member-mutation handlers, these endpoints have no manager-side
    // TrustThreshold safety net, so "authorization allowed" is observable as a
    // clean `201 Created`, and "authorization rejected" as `403`.

    /// Wire the two representative `check_domain_membership` consumers
    /// (`POST /domains/{domain_id}/action-items`, `POST /domains/{domain_id}/meetings`)
    /// with the given caller injected as authenticated `governance:write` claims.
    macro_rules! domain_membership_test_app {
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
                    .service(
                        web::resource("/domains/{domain_id}/action-items")
                            .route(web::post().to(create_action_item::<NoopEventEmitter>)),
                    )
                    .service(
                        web::resource("/domains/{domain_id}/meetings")
                            .route(web::post().to(create_meeting::<NoopEventEmitter>)),
                    ),
            )
            .await
        }};
    }

    /// #1913 AC: unresolved standing must be rejected (not treated as member)
    /// on a representative read/write endpoint. Pre-fix this fell through to
    /// the manager and created the item.
    #[tokio::test]
    async fn create_action_item_trust_threshold_unresolved_standing_rejected() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-ai-unresolved").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Err("trust graph unavailable".to_string()),
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = domain_membership_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-ai-unresolved/action-items")
                .set_json(serde_json::json!({ "title": "Plan the rehearsal" }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "unresolved TrustThreshold standing must be rejected, not allowed (fail-open #1913); \
             got body: {body_str}"
        );
        assert!(
            body_str.contains("unresolved_standing"),
            "rejection must carry the explicit unresolved_standing code; got: {body_str}"
        );
    }

    /// A TrustThreshold caller the resolver positively excludes is rejected.
    #[tokio::test]
    async fn create_action_item_trust_threshold_resolved_non_member_rejected() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();
        let other = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-ai-nonmember").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Ok(vec![other]), // caller is not in the resolved member set
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = domain_membership_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-ai-nonmember/action-items")
                .set_json(serde_json::json!({ "title": "Plan the rehearsal" }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "resolver-excluded TrustThreshold caller must not pass the membership gate; \
             got body: {body_str}"
        );
    }

    /// A TrustThreshold caller whose standing the resolver confirms is admitted
    /// and the action item is created (`201`). Guards against the fix
    /// over-rejecting legitimate members.
    #[tokio::test]
    async fn create_action_item_trust_threshold_resolved_member_allows() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-ai-member").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Ok(vec![caller.clone()]),
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = domain_membership_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-ai-member/action-items")
                .set_json(serde_json::json!({ "title": "Plan the rehearsal" }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 201,
            "resolver-confirmed TrustThreshold member must be allowed to create an action item; \
             got body: {body_str}"
        );
    }

    /// Production posture with no resolver wired must fail closed on the shared
    /// gate (cannot resolve standing → refuse), matching the #1870/#1911 policy.
    #[tokio::test]
    async fn create_action_item_trust_threshold_production_without_resolver_fails_closed() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-ai-prod-noresolver").await;
        let ctx = membership_mutation_ctx(mgr, None, GovernanceContextBuildMode::Production);
        let app = domain_membership_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-ai-prod-noresolver/action-items")
                .set_json(serde_json::json!({ "title": "Plan the rehearsal" }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "Production posture without a resolver must fail closed on TrustThreshold standing; \
             got body: {body_str}"
        );
        assert!(
            body_str.contains("unresolved_standing"),
            "Production fail-closed rejection must carry unresolved_standing; got: {body_str}"
        );
    }

    /// Second representative endpoint: the shared gate also closes the fail-open
    /// for `create_meeting`. Unresolved standing is rejected.
    #[tokio::test]
    async fn create_meeting_trust_threshold_unresolved_standing_rejected() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-meeting-unresolved").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Err("trust graph unavailable".to_string()),
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = domain_membership_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-meeting-unresolved/meetings")
                .set_json(serde_json::json!({ "title": "Coop kickoff" }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 403,
            "unresolved TrustThreshold standing must be rejected on create_meeting too; \
             got body: {body_str}"
        );
        assert!(
            body_str.contains("unresolved_standing"),
            "rejection must carry the explicit unresolved_standing code; got: {body_str}"
        );
    }

    /// Second endpoint allow-path guard: a resolver-confirmed member can create
    /// a meeting (`201`), so the reject test above is not a spurious always-403.
    #[tokio::test]
    async fn create_meeting_trust_threshold_resolved_member_allows() {
        use icn_identity::KeyPair;
        let caller = KeyPair::generate().unwrap().did().clone();

        let mgr = trust_threshold_domain("tt-meeting-member").await;
        let resolver: Arc<dyn MembershipResolver> = Arc::new(FakeMembershipResolver {
            outcome: Ok(vec![caller.clone()]),
        });
        let ctx = membership_mutation_ctx(mgr, Some(resolver), GovernanceContextBuildMode::Test);
        let app = domain_membership_test_app!(ctx, caller);

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/domains/tt-meeting-member/meetings")
                .set_json(serde_json::json!({ "title": "Coop kickoff" }))
                .to_request(),
        )
        .await;

        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(
            status, 201,
            "resolver-confirmed TrustThreshold member must be allowed to create a meeting; \
             got body: {body_str}"
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
            on_proposal_accepted_with_evidence: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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

        mgr.assign_role(
            s.id.clone(),
            caller.clone(),
            "coordinator".to_string(),
            vec![],
        )
        .unwrap();
        mgr.assign_role(s.id.clone(), other.clone(), "member".to_string(), vec![])
            .unwrap();

        let ctx = GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_proposal_accepted_with_evidence: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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
            on_proposal_accepted_with_evidence: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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
            on_proposal_accepted_with_evidence: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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
                on_proposal_accepted_with_evidence: None,
                on_charter_accepted: None,
                member_checker: None,
                steward_checker: None,
                suspension_checker: None,
                membership_resolver: None,
                sdis_service: None,
                mandate_gate: None,
                build_mode: GovernanceContextBuildMode::Test,
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
            on_proposal_accepted_with_evidence: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
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

    /// Meetings linked to program activities are included in the dashboard.
    /// A meeting linked to two activities in the same program appears once
    /// (dedup). Meetings not linked to any program activity are excluded.
    #[actix_web::test]
    async fn dashboard_includes_meetings_linked_through_activities() {
        let mgr = Arc::new(GovernanceManager::new());
        let domain_id = GovernanceDomainId("dom-d".to_string());
        let creator = test_did(3);

        mgr.create_domain(
            domain_id.clone(),
            "Domain D".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![creator.clone()]),
            },
        )
        .await
        .unwrap();

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "entity-d".to_string(),
                icn_governance::ProgramKind::Initiative,
                "Initiative 2026".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        // Two activities in this program
        let act1 = mgr
            .create_activity(
                "entity-d".to_string(),
                icn_governance::ActivityKind::Event,
                "Kickoff Event".to_string(),
                None,
                None,
                None,
                Some(prog.id.clone()),
            )
            .unwrap();

        let act2 = mgr
            .create_activity(
                "entity-d".to_string(),
                icn_governance::ActivityKind::Event,
                "Review Session".to_string(),
                None,
                None,
                None,
                Some(prog.id.clone()),
            )
            .unwrap();

        // meeting_a: linked to act1 only, scheduled at 2000
        let mut meeting_a = mgr
            .create_meeting(
                domain_id.0.clone(),
                "Kickoff Meeting".to_string(),
                None,
                Some(2000),
                creator.to_string(),
            )
            .unwrap();
        meeting_a.linked_activities = vec![act1.id.clone()];
        mgr.update_meeting(&meeting_a).unwrap();

        // meeting_b: linked to BOTH act1 and act2, scheduled at 1000 (earlier)
        let mut meeting_b = mgr
            .create_meeting(
                domain_id.0.clone(),
                "Joint Meeting".to_string(),
                None,
                Some(1000),
                creator.to_string(),
            )
            .unwrap();
        meeting_b.linked_activities = vec![act1.id.clone(), act2.id.clone()];
        mgr.update_meeting(&meeting_b).unwrap();

        // meeting_c: linked to an activity NOT in this program — should be excluded
        let other_act = mgr
            .create_activity(
                "entity-d".to_string(),
                icn_governance::ActivityKind::Event,
                "Other Event".to_string(),
                None,
                None,
                None,
                None, // no parent_program_id
            )
            .unwrap();
        let mut meeting_c = mgr
            .create_meeting(
                domain_id.0.clone(),
                "Unrelated Meeting".to_string(),
                None,
                None,
                creator.to_string(),
            )
            .unwrap();
        meeting_c.linked_activities = vec![other_act.id.clone()];
        mgr.update_meeting(&meeting_c).unwrap();

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
        let meetings = body["meetings"].as_array().unwrap();

        // meeting_b (scheduled 1000) and meeting_a (scheduled 2000);
        // meeting_b also links to act2 — should NOT be duplicated.
        assert_eq!(meetings.len(), 2, "two distinct meetings in program");
        assert_eq!(
            meetings[0]["title"], "Joint Meeting",
            "earliest scheduled first"
        );
        assert_eq!(meetings[1]["title"], "Kickoff Meeting");

        // meeting_b is linked to both activities
        let joint_linked: Vec<&str> = meetings[0]["linked_activity_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(joint_linked.len(), 2);

        assert_eq!(meetings[0]["status"], "scheduled");
    }

    // ── Program summary tests ──────────────────────────────────────────────

    macro_rules! summary_app {
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
                        "/programs/{program_id}/summary",
                        web::get().to(get_program_summary::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    /// Helper: create domain + program, return program.
    async fn setup_summary_program(
        mgr: &Arc<GovernanceManager>,
        domain: &str,
        entity: &str,
    ) -> icn_governance::Program {
        let domain_id = GovernanceDomainId(domain.to_string());
        mgr.create_domain(
            domain_id.clone(),
            format!("{domain} domain"),
            "cooperative_default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![]),
            },
        )
        .await
        .unwrap();

        mgr.create_program(
            domain_id,
            entity.to_string(),
            icn_governance::ProgramKind::Cycle,
            "Test Cycle".to_string(),
            None,
            None,
            None,
            None,
        )
        .unwrap()
    }

    /// 404 when program does not exist.
    #[actix_web::test]
    async fn summary_404_for_missing_program() {
        let mgr = Arc::new(GovernanceManager::new());
        let app = summary_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/programs/nonexistent/summary")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 404);
    }

    /// Program with no milestones: zero counts, null next_unfinished and
    /// current_phase_index.
    #[actix_web::test]
    async fn summary_no_milestones() {
        let mgr = Arc::new(GovernanceManager::new());
        let prog = setup_summary_program(&mgr, "dom-sum0", "ent-sum0").await;

        let app = summary_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/programs/{}/summary", prog.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["progress_basis"], "phase_index_ordering");
        assert_eq!(body["milestone_counts"]["total"], 0);
        assert!(body["next_unfinished_milestone"].is_null());
        assert!(body["current_phase_index"].is_null());
        let milestones = body["milestones"].as_array().unwrap();
        assert!(milestones.is_empty());
    }

    /// Mixed milestone states: counts are correct; next_unfinished is the
    /// first open milestone by phase_index; current_phase_index matches.
    #[actix_web::test]
    async fn summary_mixed_milestone_states() {
        let mgr = Arc::new(GovernanceManager::new());
        let prog = setup_summary_program(&mgr, "dom-sum1", "ent-sum1").await;

        // phase 0: completed
        let m0 = mgr
            .create_milestone(
                prog.id.clone(),
                "Phase 0".to_string(),
                None,
                0,
                None,
                vec![],
            )
            .unwrap();
        let actor = Did::from_anchor_id(&[20u8; 32]);
        mgr.update_milestone_status(&m0.id, MilestoneStatus::Completed, &actor)
            .unwrap();

        // phase 1: skipped
        let m1 = mgr
            .create_milestone(
                prog.id.clone(),
                "Phase 1".to_string(),
                None,
                1,
                None,
                vec![],
            )
            .unwrap();
        mgr.update_milestone_status(&m1.id, MilestoneStatus::Skipped, &actor)
            .unwrap();

        // phase 2: in_progress (first open)
        let m2 = mgr
            .create_milestone(
                prog.id.clone(),
                "Phase 2".to_string(),
                None,
                2,
                None,
                vec![],
            )
            .unwrap();
        mgr.update_milestone_status(&m2.id, MilestoneStatus::InProgress, &actor)
            .unwrap();

        // phase 3: pending
        mgr.create_milestone(
            prog.id.clone(),
            "Phase 3".to_string(),
            None,
            3,
            None,
            vec![],
        )
        .unwrap();

        let app = summary_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/programs/{}/summary", prog.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["milestone_counts"]["total"], 4);
        assert_eq!(body["milestone_counts"]["completed"], 1);
        assert_eq!(body["milestone_counts"]["skipped"], 1);
        assert_eq!(body["milestone_counts"]["in_progress"], 1);
        assert_eq!(body["milestone_counts"]["pending"], 1);

        // next_unfinished is phase 2 (in_progress, lowest open phase_index)
        let nxt = &body["next_unfinished_milestone"];
        assert_eq!(nxt["phase_index"], 2);
        assert_eq!(nxt["status"], "in_progress");
        assert_eq!(body["current_phase_index"], 2);

        // milestones returned in phase_index order
        let ms = body["milestones"].as_array().unwrap();
        assert_eq!(ms.len(), 4);
        assert_eq!(ms[0]["phase_index"], 0);
        assert_eq!(ms[1]["phase_index"], 1);
        assert_eq!(ms[2]["phase_index"], 2);
        assert_eq!(ms[3]["phase_index"], 3);
    }

    /// All milestones terminal (completed/skipped): next_unfinished is null;
    /// current_phase_index is the highest phase_index.
    #[actix_web::test]
    async fn summary_all_milestones_terminal() {
        let mgr = Arc::new(GovernanceManager::new());
        let prog = setup_summary_program(&mgr, "dom-sum2", "ent-sum2").await;

        let actor = Did::from_anchor_id(&[21u8; 32]);
        for phase in 0..3u32 {
            let m = mgr
                .create_milestone(
                    prog.id.clone(),
                    format!("Phase {phase}"),
                    None,
                    phase,
                    None,
                    vec![],
                )
                .unwrap();
            mgr.update_milestone_status(&m.id, MilestoneStatus::Completed, &actor)
                .unwrap();
        }

        let app = summary_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/programs/{}/summary", prog.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["milestone_counts"]["completed"], 3);
        assert_eq!(body["milestone_counts"]["pending"], 0);
        assert!(body["next_unfinished_milestone"].is_null());
        // current_phase_index falls back to highest terminal phase_index
        assert_eq!(body["current_phase_index"], 2);
    }

    // ── Milestone preview tests ────────────────────────────────────────────

    macro_rules! preview_app {
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
                        "/milestones/{milestone_id}/preview",
                        web::get().to(preview_milestone::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    /// Helper: create a domain + program and return `(manager, program_id)`.
    async fn setup_preview_program(
        mgr: &Arc<GovernanceManager>,
        domain: &str,
        entity: &str,
    ) -> icn_governance::ProgramId {
        let domain_id = GovernanceDomainId(domain.to_string());
        mgr.create_domain(
            domain_id.clone(),
            format!("{domain} domain"),
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
                entity.to_string(),
                icn_governance::ProgramKind::Cycle,
                "Preview Cycle".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();
        prog.id
    }

    /// 404 when the milestone does not exist.
    #[actix_web::test]
    async fn preview_404_for_missing_milestone() {
        let mgr = Arc::new(GovernanceManager::new());
        let app = preview_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/milestones/nonexistent/preview")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 404);
    }

    /// Phase-0 milestone with no earlier phases → ready, reason null.
    #[actix_web::test]
    async fn preview_ready_when_first_phase_and_open() {
        let mgr = Arc::new(GovernanceManager::new());
        let pid = setup_preview_program(&mgr, "dom-ready", "ent-ready").await;

        let m = mgr
            .create_milestone(
                pid,
                "Kickoff".to_string(),
                None,
                0,
                None,
                vec!["plan drafted".to_string(), "team named".to_string()],
            )
            .unwrap();

        let app = preview_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/preview", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["milestone_id"], m.id.0.as_str());
        assert_eq!(body["phase_index"], 0);
        assert_eq!(body["status"], "pending");
        assert_eq!(body["is_open"], true);
        assert_eq!(body["earlier_milestones_complete"], true);
        assert_eq!(body["blocking_milestones"], serde_json::json!([]));
        assert_eq!(body["criteria_count"], 2);
        assert_eq!(
            body["completion_criteria"],
            serde_json::json!(["plan drafted", "team named"])
        );
        assert_eq!(body["ready_to_advance"], true);
        assert!(
            body.get("reason").is_none_or(|v| v.is_null()),
            "reason should be absent or null when ready: {body:?}"
        );
        assert_eq!(body["program_status"], "draft");
    }

    /// Later-phase milestone is blocked by an uncompleted earlier phase.
    /// `blocking_milestones` is ordered by `phase_index`; `reason` names the
    /// first blocker.
    #[actix_web::test]
    async fn preview_blocked_by_earlier_phase() {
        let mgr = Arc::new(GovernanceManager::new());
        let pid = setup_preview_program(&mgr, "dom-block", "ent-block").await;

        // Create out of order to prove ordering comes from phase_index.
        let m_launch = mgr
            .create_milestone(pid.clone(), "Launch".to_string(), None, 2, None, vec![])
            .unwrap();
        let _m_plan = mgr
            .create_milestone(pid.clone(), "Plan".to_string(), None, 1, None, vec![])
            .unwrap();
        let m_kick = mgr
            .create_milestone(pid, "Kickoff".to_string(), None, 0, None, vec![])
            .unwrap();

        // Complete phase-0 so phase-1 becomes the first blocker for phase-2.
        let caller = test_did(1);
        mgr.update_milestone_status(&m_kick.id, MilestoneStatus::Completed, &caller)
            .unwrap();

        let app = preview_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/preview", m_launch.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["ready_to_advance"], false);
        assert_eq!(body["earlier_milestones_complete"], false);
        let blocking = body["blocking_milestones"].as_array().unwrap();
        // Only phase-1 is still blocking (phase-0 is completed).
        assert_eq!(blocking.len(), 1);
        assert_eq!(blocking[0]["name"], "Plan");
        assert_eq!(blocking[0]["phase_index"], 1);
        assert_eq!(blocking[0]["status"], "pending");
        let reason = body["reason"].as_str().unwrap();
        assert!(
            reason.contains("Plan") && reason.contains("phase 1"),
            "reason should name the first blocker: {reason}"
        );
    }

    /// Skipped earlier milestones also clear the blocker.
    #[actix_web::test]
    async fn preview_skipped_earlier_phase_clears_blocker() {
        let mgr = Arc::new(GovernanceManager::new());
        let pid = setup_preview_program(&mgr, "dom-skip", "ent-skip").await;

        let m_a = mgr
            .create_milestone(pid.clone(), "A".to_string(), None, 0, None, vec![])
            .unwrap();
        let m_b = mgr
            .create_milestone(pid, "B".to_string(), None, 1, None, vec![])
            .unwrap();

        let caller = test_did(2);
        mgr.update_milestone_status(&m_a.id, MilestoneStatus::Skipped, &caller)
            .unwrap();

        let app = preview_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/preview", m_b.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["earlier_milestones_complete"], true);
        assert_eq!(body["ready_to_advance"], true);
        assert_eq!(body["blocking_milestones"], serde_json::json!([]));
    }

    /// Already-completed milestone reports not-ready with a descriptive reason
    /// and surfaces `is_open = false`.
    #[actix_web::test]
    async fn preview_already_completed_is_not_ready() {
        let mgr = Arc::new(GovernanceManager::new());
        let pid = setup_preview_program(&mgr, "dom-done", "ent-done").await;

        let m = mgr
            .create_milestone(pid, "Done".to_string(), None, 0, None, vec![])
            .unwrap();
        let caller = test_did(3);
        mgr.update_milestone_status(&m.id, MilestoneStatus::Completed, &caller)
            .unwrap();

        let app = preview_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/preview", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "completed");
        assert_eq!(body["is_open"], false);
        assert_eq!(body["ready_to_advance"], false);
        let reason = body["reason"].as_str().unwrap();
        assert!(
            reason.contains("completed"),
            "reason should explain completion: {reason}"
        );
    }

    /// Blocked status prevents readiness even when earlier phases are clear.
    #[actix_web::test]
    async fn preview_blocked_status_is_not_ready() {
        let mgr = Arc::new(GovernanceManager::new());
        let pid = setup_preview_program(&mgr, "dom-blk", "ent-blk").await;

        let m = mgr
            .create_milestone(pid, "Stuck".to_string(), None, 0, None, vec![])
            .unwrap();
        let caller = test_did(4);
        mgr.update_milestone_status(&m.id, MilestoneStatus::Blocked, &caller)
            .unwrap();

        let app = preview_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/preview", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "blocked");
        assert_eq!(body["earlier_milestones_complete"], true);
        assert_eq!(body["ready_to_advance"], false);
        let reason = body["reason"].as_str().unwrap();
        assert!(
            reason.contains("blocked"),
            "reason should mention block: {reason}"
        );
    }

    // ── Milestone history tests ────────────────────────────────────────────

    macro_rules! history_app {
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
                        "/milestones/{milestone_id}/history",
                        web::get().to(get_milestone_history::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    /// 404 when the milestone does not exist.
    #[actix_web::test]
    async fn history_404_for_missing_milestone() {
        let mgr = Arc::new(GovernanceManager::new());
        let app = history_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/milestones/ghost/history")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 404);
    }

    /// Open (pending) milestone has exactly one entry: the creation bookmark.
    /// `completed_at` and `completed_by` are absent → completion entry omitted.
    #[actix_web::test]
    async fn history_pending_milestone_has_one_entry() {
        let mgr = Arc::new(GovernanceManager::new());
        let pid = setup_preview_program(&mgr, "dom-h1", "ent-h1").await;

        let m = mgr
            .create_milestone(pid, "Phase 0".to_string(), None, 0, None, vec![])
            .unwrap();

        let app = history_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/history", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["coverage"], "lifecycle_bookmarks");
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(
            entries.len(),
            1,
            "only creation entry for pending milestone"
        );
        assert_eq!(entries[0]["to_status"], "pending");
        assert_eq!(entries[0]["source"], "creation");
        // changed_by is omitted (no creator recorded on milestone record)
        assert!(entries[0]["changed_by"].is_null());
    }

    /// Completed milestone has two entries: creation + completion.
    /// `completed_by` is present on the completion entry.
    #[actix_web::test]
    async fn history_completed_milestone_has_two_entries() {
        let mgr = Arc::new(GovernanceManager::new());
        let pid = setup_preview_program(&mgr, "dom-h2", "ent-h2").await;

        let m = mgr
            .create_milestone(pid, "Phase 0".to_string(), None, 0, None, vec![])
            .unwrap();

        let actor = Did::from_anchor_id(&[42u8; 32]);
        let actor_str = actor.to_string();
        mgr.update_milestone_status(&m.id, MilestoneStatus::Completed, &actor)
            .unwrap();

        let app = history_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/history", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["coverage"], "lifecycle_bookmarks");
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(
            entries.len(),
            2,
            "creation + completion for completed milestone"
        );

        assert_eq!(entries[0]["to_status"], "pending");
        assert_eq!(entries[0]["source"], "creation");
        assert!(entries[0]["changed_by"].is_null());

        assert_eq!(entries[1]["to_status"], "completed");
        assert_eq!(entries[1]["source"], "completion_record");
        assert_eq!(entries[1]["changed_by"], actor_str);
        // completion entry must be at or after creation
        let t0 = entries[0]["changed_at"].as_u64().unwrap();
        let t1 = entries[1]["changed_at"].as_u64().unwrap();
        assert!(t1 >= t0, "completion must not precede creation");
    }

    // ── Event log path tests ───────────────────────────────────────────────
    //
    // These tests use a GovernanceManager configured with an InMemoryMilestoneEventLog
    // so that status transitions are recorded. The history endpoint should return
    // coverage: "transition_log" and the actual recorded events.

    fn make_event_log_ctx(mgr: Arc<GovernanceManager>) -> GovernanceContext<NoopEventEmitter> {
        GovernanceContext {
            manager: mgr,
            emitter: NoopEventEmitter,
            on_proposal_accepted: None,
            on_proposal_accepted_with_evidence: None,
            on_charter_accepted: None,
            member_checker: None,
            steward_checker: None,
            suspension_checker: None,
            membership_resolver: None,
            sdis_service: None,
            mandate_gate: None,
            build_mode: GovernanceContextBuildMode::Test,
        }
    }

    macro_rules! event_log_history_app {
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
                        "/milestones/{milestone_id}/history",
                        web::get().to(get_milestone_history::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    /// Helper: build a GovernanceManager with an InMemoryMilestoneEventLog wired in.
    fn new_mgr_with_event_log() -> Arc<GovernanceManager> {
        let log = Arc::new(InMemoryMilestoneEventLog::new());
        Arc::new(GovernanceManager::new().with_milestone_event_log(log))
    }

    /// Status transitions are recorded; history returns transition_log coverage
    /// with one creation entry + one log entry per unique status change.
    #[actix_web::test]
    async fn event_log_single_transition_appears_in_history() {
        let mgr = new_mgr_with_event_log();
        let pid = setup_preview_program(&mgr, "dom-el1", "ent-el1").await;

        let m = mgr
            .create_milestone(pid, "Phase 0".to_string(), None, 0, None, vec![])
            .unwrap();

        let actor = Did::from_anchor_id(&[10u8; 32]);
        mgr.update_milestone_status(&m.id, MilestoneStatus::InProgress, &actor)
            .unwrap();

        let app = event_log_history_app!(make_event_log_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/history", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["coverage"], "transition_log");

        let entries = body["entries"].as_array().unwrap();
        // 1 creation + 1 logged transition
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["source"], "creation");
        assert_eq!(entries[0]["to_status"], "pending");
        assert!(entries[0]["changed_by"].is_null());

        assert_eq!(entries[1]["source"], "transition_log");
        assert_eq!(entries[1]["to_status"], "in_progress");
        assert_eq!(entries[1]["changed_by"], actor.to_string());
    }

    /// Multiple transitions appear in chronological order.
    #[actix_web::test]
    async fn event_log_multiple_transitions_ordered_oldest_first() {
        let mgr = new_mgr_with_event_log();
        let pid = setup_preview_program(&mgr, "dom-el2", "ent-el2").await;

        let m = mgr
            .create_milestone(pid, "Phase 0".to_string(), None, 0, None, vec![])
            .unwrap();

        let actor = Did::from_anchor_id(&[11u8; 32]);
        mgr.update_milestone_status(&m.id, MilestoneStatus::InProgress, &actor)
            .unwrap();
        mgr.update_milestone_status(&m.id, MilestoneStatus::Blocked, &actor)
            .unwrap();
        mgr.update_milestone_status(&m.id, MilestoneStatus::InProgress, &actor)
            .unwrap();
        mgr.update_milestone_status(&m.id, MilestoneStatus::Completed, &actor)
            .unwrap();

        let app = event_log_history_app!(make_event_log_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/history", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["coverage"], "transition_log");

        let entries = body["entries"].as_array().unwrap();
        // 1 creation + 4 transitions
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0]["source"], "creation");
        assert_eq!(entries[1]["to_status"], "in_progress");
        assert_eq!(entries[2]["to_status"], "blocked");
        assert_eq!(entries[3]["to_status"], "in_progress");
        assert_eq!(entries[4]["to_status"], "completed");

        // Chronological order: each changed_at >= previous
        let times: Vec<u64> = entries
            .iter()
            .map(|e| e["changed_at"].as_u64().unwrap())
            .collect();
        for pair in times.windows(2) {
            assert!(pair[1] >= pair[0], "entries not in chronological order");
        }
    }

    /// Re-marking an already-Completed milestone does not add a duplicate log entry.
    #[actix_web::test]
    async fn event_log_no_duplicate_on_same_status() {
        let mgr = new_mgr_with_event_log();
        let pid = setup_preview_program(&mgr, "dom-el3", "ent-el3").await;

        let m = mgr
            .create_milestone(pid, "Phase 0".to_string(), None, 0, None, vec![])
            .unwrap();

        let actor = Did::from_anchor_id(&[12u8; 32]);
        mgr.update_milestone_status(&m.id, MilestoneStatus::Completed, &actor)
            .unwrap();
        // Call again with same status — should not produce a second log entry.
        mgr.update_milestone_status(&m.id, MilestoneStatus::Completed, &actor)
            .unwrap();

        let app = event_log_history_app!(make_event_log_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/history", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        let entries = body["entries"].as_array().unwrap();
        // 1 creation + 1 completion — no duplicate
        assert_eq!(entries.len(), 2, "duplicate entry on same-status update");
    }

    /// A milestone without any transitions still has one entry (creation bookmark)
    /// and coverage "transition_log" (log is configured but empty for this milestone).
    /// Actually: no events → falls back to lifecycle_bookmarks.
    #[actix_web::test]
    async fn event_log_no_transitions_falls_back_to_lifecycle_bookmarks() {
        let mgr = new_mgr_with_event_log();
        let pid = setup_preview_program(&mgr, "dom-el4", "ent-el4").await;

        let m = mgr
            .create_milestone(pid, "Phase 0".to_string(), None, 0, None, vec![])
            .unwrap();
        // No status updates — event log for this milestone is empty.

        let app = event_log_history_app!(make_event_log_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/milestones/{}/history", m.id.0))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        // Empty event list → fallback path
        assert_eq!(body["coverage"], "lifecycle_bookmarks");
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["source"], "creation");
    }

    // ── Program status update tests ────────────────────────────────────────

    /// Helper: build a GovernanceManager with an InMemoryProgramEventLog wired in.
    fn new_mgr_with_program_event_log() -> Arc<GovernanceManager> {
        use crate::manager::InMemoryProgramEventLog;
        let log = Arc::new(InMemoryProgramEventLog::new());
        Arc::new(GovernanceManager::new().with_program_event_log(log))
    }

    /// Helper: create domain + program with TrustThreshold membership (all callers pass).
    ///
    /// Used by write-path tests that go through `check_domain_membership`. The
    /// threshold is 0.0 so every DID is considered a member; this avoids having
    /// to add specific test DIDs to the static list.
    async fn setup_writable_program(
        mgr: &Arc<GovernanceManager>,
        domain: &str,
        entity: &str,
    ) -> icn_governance::Program {
        let domain_id = GovernanceDomainId(domain.to_string());
        mgr.create_domain(
            domain_id.clone(),
            format!("{domain} domain"),
            "cooperative_default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::TrustThreshold(0.0),
            },
        )
        .await
        .unwrap();
        mgr.create_program(
            domain_id,
            entity.to_string(),
            icn_governance::ProgramKind::Cycle,
            "Test Cycle".to_string(),
            None,
            None,
            None,
            None,
        )
        .unwrap()
    }

    macro_rules! program_status_app {
        ($ctx:expr) => {{
            let ctx_data = web::Data::new($ctx);
            let caller_did_str = test_did(1).to_string();
            test::init_service(
                App::new()
                    .app_data(ctx_data)
                    .app_data(web::JsonConfig::default())
                    .wrap_fn(move |req, srv| {
                        req.extensions_mut().insert(BasicClaims {
                            sub: caller_did_str.clone(),
                            scope: Some("governance:write".to_string()),
                        });
                        srv.call(req)
                    })
                    .route(
                        "/programs/{program_id}/status",
                        web::patch().to(update_program_status::<NoopEventEmitter>),
                    ),
            )
            .await
        }};
    }

    /// PATCH /programs/{id}/status — basic transition: Draft → ActivePlanning.
    #[actix_web::test]
    async fn program_status_update_draft_to_active_planning() {
        let mgr = new_mgr_with_program_event_log();
        let prog = setup_writable_program(&mgr, "dom-ps1", "ent-ps1").await;
        assert_eq!(prog.status, icn_governance::ProgramStatus::Draft);

        let app = program_status_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/programs/{}/status", prog.id.0))
                .set_json(serde_json::json!({"status": "active_planning"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "active_planning");
    }

    /// Multiple ordered transitions are recorded in the event log.
    #[actix_web::test]
    async fn program_status_multiple_transitions_recorded() {
        let mgr = new_mgr_with_program_event_log();
        let prog = setup_summary_program(&mgr, "dom-ps2", "ent-ps2").await;
        let actor = Did::from_anchor_id(&[20u8; 32]);

        use icn_governance::ProgramStatus;
        mgr.update_program_status(&prog.id, ProgramStatus::ActivePlanning, &actor)
            .unwrap();
        mgr.update_program_status(&prog.id, ProgramStatus::InExecution, &actor)
            .unwrap();
        mgr.update_program_status(&prog.id, ProgramStatus::Closed, &actor)
            .unwrap();

        let events = mgr.list_program_events(&prog.id).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].from_status, ProgramStatus::Draft);
        assert_eq!(events[0].to_status, ProgramStatus::ActivePlanning);
        assert_eq!(events[1].from_status, ProgramStatus::ActivePlanning);
        assert_eq!(events[1].to_status, ProgramStatus::InExecution);
        assert_eq!(events[2].from_status, ProgramStatus::InExecution);
        assert_eq!(events[2].to_status, ProgramStatus::Closed);
    }

    /// Re-applying the same status does not produce a duplicate log entry.
    #[actix_web::test]
    async fn program_status_no_duplicate_on_same_status() {
        let mgr = new_mgr_with_program_event_log();
        let prog = setup_summary_program(&mgr, "dom-ps3", "ent-ps3").await;
        let actor = Did::from_anchor_id(&[21u8; 32]);

        use icn_governance::ProgramStatus;
        mgr.update_program_status(&prog.id, ProgramStatus::Closed, &actor)
            .unwrap();
        mgr.update_program_status(&prog.id, ProgramStatus::Closed, &actor)
            .unwrap();

        let events = mgr.list_program_events(&prog.id).unwrap();
        assert_eq!(events.len(), 1, "duplicate event on same-status update");
    }

    /// 404 when program does not exist.
    #[actix_web::test]
    async fn program_status_update_404_for_missing_program() {
        let mgr = new_mgr_with_program_event_log();
        let app = program_status_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri("/programs/nonexistent/status")
                .set_json(serde_json::json!({"status": "active_planning"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 404);
    }

    /// Unknown status string returns 400.
    #[actix_web::test]
    async fn program_status_update_400_for_unknown_status() {
        let mgr = new_mgr_with_program_event_log();
        let prog = setup_writable_program(&mgr, "dom-ps4", "ent-ps4").await;
        let app = program_status_app!(make_dashboard_ctx(mgr));
        let resp = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&format!("/programs/{}/status", prog.id.0))
                .set_json(serde_json::json!({"status": "flying_saucer"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 400);
    }
}
