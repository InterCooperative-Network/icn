//! Governance API endpoints
//!
//! RESTful API for managing governance domains, proposals, and votes.

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::Result;
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::governance_mgr::GovernanceManager;
use crate::middleware::{get_claims, require_scope};
use crate::models::{
    ActionItemFilterParams, ActionItemNoteResponse, ActionItemResponse, AddActionItemNoteRequest,
    CastVoteRequest, CreateActionItemRequest, CreateDelegationRequest, CreateDomainRequest,
    CreateProposalRequest, DelegationListResponse, DelegationResponse,
    EstablishClearingProposalRequest, JoinFederationProposalRequest,
    LeaveFederationProposalRequest, OpenProposalRequest, ProposalPayloadRequest,
    RevokeVouchProposalRequest, TerminateClearingProposalRequest, UpdateActionItemRequest,
    UpdateFederationPolicyProposalRequest, VouchProposalRequest,
};
use crate::pagination::{ListPagination, ListQuery, ListResponse};
use crate::validation;
use icn_federation::SettlementInterval;
use icn_governance::{
    ActionItemFilter, ActionItemId, ActionItemPriority, ActionItemStatus, DataSharingLevel,
    Delegation, DelegationScope, DisputeResolutionMethod, FederationProposal, FederationTerms,
    GovernanceDomainId, GovernanceParams, MembershipConfig, ProposalId, ProposalPayload,
    VoteChoice,
};
use icn_identity::Did;
use icn_obs::metrics::{action_items, gateway};

// ============================================================================
// Domain Endpoints
// ============================================================================

/// POST /gov/domains - Create a new governance domain
#[post("/domains")]
pub async fn create_domain(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<CreateDomainRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let creator_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate inputs
    validation::validate_domain_id(&req.id)?;
    validation::validate_domain_name(&req.name)?;
    validation::validate_governance_model(&req.profile)?;
    validation::validate_domain_members(&req.members)?;

    // Validate voting period BEFORE multiplication to prevent overflow
    // Max safe value: MAX_VOTING_PERIOD_SECONDS / 86400 = 365 days
    const MAX_VOTING_PERIOD_DAYS: u64 = validation::MAX_VOTING_PERIOD_SECONDS / 86400;
    if req.voting_period_days == 0 {
        return Err(crate::error::GatewayError::BadRequest(
            "Voting period must be greater than 0 days".to_string(),
        ));
    }
    if req.voting_period_days > MAX_VOTING_PERIOD_DAYS {
        return Err(crate::error::GatewayError::BadRequest(format!(
            "Voting period exceeds maximum of {MAX_VOTING_PERIOD_DAYS} days (1 year)"
        )));
    }

    // Safe to multiply now - voting_period_days <= 365
    let voting_period_seconds = req.voting_period_days * 86400; // days -> seconds
    validation::validate_governance_params(
        req.quorum_percent,
        req.approval_percent,
        voting_period_seconds,
    )?;

    // Convert member DIDs
    let members: Result<Vec<Did>> = req
        .members
        .iter()
        .map(|s| {
            s.parse().map_err(|e| {
                crate::error::GatewayError::BadRequest(format!("Invalid member DID: {e}"))
            })
        })
        .collect();
    let members = members?;

    // Build membership config
    let membership = MembershipConfig::static_list(members);

    // Build governance params
    let params = GovernanceParams::new(
        req.quorum_percent,
        req.approval_percent,
        voting_period_seconds,
    );

    // Create domain via governance manager
    let domain_id = GovernanceDomainId(req.id.clone());
    gov_mgr
        .create_domain(
            domain_id.clone(),
            req.name.clone(),
            req.profile.clone(),
            params,
            membership,
        )
        .await?;

    // Track domain creation
    gateway::governance_domains_created_inc();

    // Broadcast event to WebSocket subscribers
    event_broadcaster
        .broadcast(
            &domain_id.0,
            GatewayEvent::GovernanceDomainCreated {
                domain_id: domain_id.0.clone(),
                name: req.name.clone(),
                creator: creator_did.to_string(),
            },
        )
        .await;

    // Return created domain
    let domain = gov_mgr.get_domain(&domain_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError(
            "Domain creation succeeded but domain not found".to_string(),
        )
    })?;

    Ok(HttpResponse::Created().json(domain))
}

/// GET /gov/domains - List all governance domains
///
/// Supports cursor-based pagination and filtering:
/// - `cursor`: Offset-based cursor (e.g., "offset:20")
/// - `limit`: Max items per page (default: 20, max: 100)
/// - `filter[name]`: Filter by domain name (partial match)
/// - `sort`: Sort field (`name`, `-name`, `created_at`, `-created_at`)
#[get("/domains")]
pub async fn list_domains(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    query: web::Query<ListQuery>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let query = query.into_inner().validate();

    // Validate filter lengths to prevent DoS
    query
        .validate_filters()
        .map_err(crate::error::GatewayError::BadRequest)?;

    let limit = query.limit;

    // NOTE: We load all domains for filtering/sorting because:
    // 1. The API contract requires default sorting by name
    // 2. Storage-layer pagination returns items in storage order (by ID)
    // 3. Sorting after pagination breaks cursor semantics
    //
    // TODO(performance): For very large domain counts (10K+), consider:
    // - Adding a name-sorted secondary index in storage
    // - Offering an unsorted API variant for raw pagination
    let mut domains = gov_mgr.list_domains().await?;

    // Apply filters
    if let Some(name_filter) = query.filter("name") {
        let filter_lower = name_filter.to_lowercase();
        domains.retain(|d| d.name.to_lowercase().contains(&filter_lower));
    }

    // Apply sorting
    // Valid sort fields for domains
    const VALID_DOMAIN_SORT_FIELDS: &[&str] = &["name", "created_at"];

    let sort_fields = query.sort_fields();
    if sort_fields.is_empty() {
        // Default: sort by name ascending
        domains.sort_by(|a, b| a.name.cmp(&b.name));
    } else {
        // Validate sort field
        let sort = &sort_fields[0];
        if !VALID_DOMAIN_SORT_FIELDS.contains(&sort.field.as_str()) {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Invalid sort field '{}'. Valid fields: {}",
                sort.field,
                VALID_DOMAIN_SORT_FIELDS.join(", ")
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
            _ => unreachable!(), // Already validated above
        }
    }

    // Apply pagination
    let total = domains.len();

    // Parse cursor to get offset (format: "offset:<number>")
    let offset: usize = query
        .parse_offset_cursor()
        .map_err(crate::error::GatewayError::BadRequest)?
        .min(total);

    let paginated: Vec<_> = domains.into_iter().skip(offset).take(limit).collect();
    let count = paginated.len();
    let next_offset = offset + count;
    let has_more = next_offset < total;

    // Build pagination metadata
    let pagination = if has_more {
        ListPagination::with_cursor(format!("offset:{next_offset}"), count).with_total(total)
    } else {
        ListPagination::last_page(count).with_total(total)
    };

    let response: ListResponse<_> = ListResponse::new(paginated, pagination);
    Ok(HttpResponse::Ok().json(response))
}

/// GET /gov/domains/{id} - Get a specific governance domain
#[get("/domains/{id}")]
pub async fn get_domain(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let domain_id = GovernanceDomainId(id.into_inner());
    let domain = gov_mgr.get_domain(&domain_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Domain not found: {}", domain_id.0))
    })?;

    Ok(HttpResponse::Ok().json(domain))
}

/// POST /gov/domains/{domain_id}/members - Add a member to a governance domain
#[post("/domains/{domain_id}/members")]
pub async fn add_domain_member(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    path: web::Path<String>,
    body: web::Json<crate::models::AddDomainMemberRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;
    let caller_did: icn_identity::Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let domain_id_str = path.into_inner();
    validation::validate_domain_id(&domain_id_str)?;

    let member_did: icn_identity::Did = body
        .did
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid member DID: {e}")))?;

    let domain_id = GovernanceDomainId(domain_id_str.clone());

    // Verify domain exists and caller is a member
    let domain = gov_mgr.get_domain(&domain_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Domain not found: {domain_id_str}"))
    })?;

    let caller_is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(&caller_did),
        icn_governance::MembershipSource::TrustThreshold(_) => true,
    };
    if !caller_is_member {
        return Err(crate::error::GatewayError::AuthorizationFailed(format!(
            "Only domain members can add members to domain '{domain_id_str}'"
        )));
    }

    // Add the member
    gov_mgr
        .update_domain_membership(
            domain_id,
            member_did.clone(),
            icn_governance::MembershipAction::Add,
        )
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "member_added",
        "domain_id": domain_id_str,
        "member_did": member_did.to_string()
    })))
}

// ============================================================================
// Proposal Endpoints
// ============================================================================

/// POST /gov/proposals - Create a new proposal
#[post("/proposals")]
pub async fn create_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<CreateProposalRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let proposer_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate inputs
    validation::validate_domain_id(&req.domain_id)?;
    validation::validate_proposal_title(&req.title)?;
    validation::validate_proposal_description(&req.description)?;

    // Convert payload
    let payload = match &req.payload {
        ProposalPayloadRequest::Text { body } => {
            // Validate text body
            if body.is_empty() || body.trim().is_empty() {
                return Err(crate::error::GatewayError::BadRequest(
                    "Proposal text body cannot be empty or whitespace-only".to_string(),
                ));
            }
            if body.len() > validation::MAX_PROPOSAL_DESCRIPTION_LEN {
                return Err(crate::error::GatewayError::BadRequest(format!(
                    "Proposal text body exceeds maximum length of {} characters",
                    validation::MAX_PROPOSAL_DESCRIPTION_LEN
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
            // Validate budget amount
            validation::validate_payment_amount(*amount)?;

            // Validate currency
            validation::validate_currency(currency)?;

            // Validate purpose
            if purpose.is_empty() || purpose.trim().is_empty() {
                return Err(crate::error::GatewayError::BadRequest(
                    "Budget purpose cannot be empty or whitespace-only".to_string(),
                ));
            }
            if purpose.len() > validation::MAX_PROPOSAL_DESCRIPTION_LEN {
                return Err(crate::error::GatewayError::BadRequest(format!(
                    "Budget purpose exceeds maximum length of {} characters",
                    validation::MAX_PROPOSAL_DESCRIPTION_LEN
                )));
            }

            let recipient_did: Did = recipient.parse().map_err(|e| {
                crate::error::GatewayError::BadRequest(format!("Invalid recipient DID: {e}"))
            })?;
            ProposalPayload::Budget {
                amount: *amount,
                recipient: recipient_did,
                currency: currency.clone(),
                purpose: purpose.clone(),
            }
        }
        ProposalPayloadRequest::Membership { action, did } => {
            let member_did: Did = did.parse().map_err(|e| {
                crate::error::GatewayError::BadRequest(format!("Invalid member DID: {e}"))
            })?;

            // Parse action string to MembershipAction
            use icn_governance::MembershipAction;
            let membership_action = match action.to_lowercase().as_str() {
                "add" => MembershipAction::Add,
                "remove" => MembershipAction::Remove,
                _ => {
                    return Err(crate::error::GatewayError::BadRequest(format!(
                        "Invalid action: {action}"
                    )))
                }
            };

            ProposalPayload::Membership {
                action: membership_action,
                member: member_did,
            }
        }
        ProposalPayloadRequest::ConfigChange { key, value } => {
            // Validate config key
            if key.is_empty() || key.trim().is_empty() {
                return Err(crate::error::GatewayError::BadRequest(
                    "Config key cannot be empty or whitespace-only".to_string(),
                ));
            }
            if key.len() > validation::MAX_GOVERNANCE_MODEL_LEN {
                return Err(crate::error::GatewayError::BadRequest(format!(
                    "Config key exceeds maximum length of {} characters",
                    validation::MAX_GOVERNANCE_MODEL_LEN
                )));
            }

            // Validate config value
            if value.is_empty() || value.trim().is_empty() {
                return Err(crate::error::GatewayError::BadRequest(
                    "Config value cannot be empty or whitespace-only".to_string(),
                ));
            }
            if value.len() > validation::MAX_PROPOSAL_DESCRIPTION_LEN {
                return Err(crate::error::GatewayError::BadRequest(format!(
                    "Config value exceeds maximum length of {} characters",
                    validation::MAX_PROPOSAL_DESCRIPTION_LEN
                )));
            }

            // Combine key-value into JSON config
            let new_config = serde_json::json!({ key: value }).to_string();
            ProposalPayload::ConfigChange { new_config }
        }
    };

    // Create proposal via governance manager
    // In actor-backed mode, the actor generates the proposal ID
    // In standalone mode, we provide one (but it may be overridden)
    let domain_id = GovernanceDomainId(req.domain_id.clone());
    let suggested_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));

    let proposal_id = gov_mgr
        .create_proposal(
            suggested_id,
            domain_id,
            proposer_did.clone(),
            req.title.clone(),
            req.description.clone(),
            payload,
        )
        .await?;

    // Track proposal creation
    gateway::governance_proposals_created_inc();

    // Determine payload type for event
    let payload_type = match &req.payload {
        ProposalPayloadRequest::Text { .. } => "text",
        ProposalPayloadRequest::Budget { .. } => "budget",
        ProposalPayloadRequest::Membership { .. } => "membership",
        ProposalPayloadRequest::ConfigChange { .. } => "config_change",
    };

    // Broadcast event to WebSocket subscribers
    event_broadcaster
        .broadcast(
            &req.domain_id,
            GatewayEvent::GovernanceProposalCreated {
                proposal_id: proposal_id.0.clone(),
                domain_id: req.domain_id.clone(),
                proposer: proposer_did.to_string(),
                title: req.title.clone(),
                payload_type: payload_type.to_string(),
            },
        )
        .await;

    // Return created proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError(
            "Proposal creation succeeded but proposal not found".to_string(),
        )
    })?;

    Ok(HttpResponse::Created().json(proposal))
}

/// GET /gov/proposals - List all proposals (optionally filtered by domain)
///
/// Supports cursor-based pagination and filtering:
/// - `cursor`: Opaque cursor for pagination
/// - `limit`: Max items per page (default: 20, max: 100)
/// - `filter[domain_id]`: Filter by domain ID
/// - `filter[state]`: Filter by state (draft, open, closed)
/// - `sort`: Sort field (`created_at`, `-created_at`, `title`)
#[get("/proposals")]
pub async fn list_proposals(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    query: web::Query<ListQuery>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let query = query.into_inner().validate();

    // Validate filter lengths to prevent DoS
    query
        .validate_filters()
        .map_err(crate::error::GatewayError::BadRequest)?;

    let mut proposals = gov_mgr.list_proposals().await?;

    // Filter by domain if requested
    if let Some(domain_id) = query.filter("domain_id") {
        let filter_domain = GovernanceDomainId(domain_id.to_string());
        proposals.retain(|p| p.domain_id == filter_domain);
    }

    // Filter by state if requested
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
                return Err(crate::error::GatewayError::BadRequest(format!(
                    "Invalid state filter: {state}. Valid: draft, open, closed"
                )))
            }
        }
    }

    // Filter by scope if requested
    if let Some(scope) = query.filter("scope") {
        match scope {
            "local" => {
                proposals.retain(|p| matches!(p.scope, icn_governance::ProposalScope::Local));
            }
            "federation" => {
                proposals
                    .retain(|p| matches!(p.scope, icn_governance::ProposalScope::Federation(_)));
            }
            other => {
                // Treat as federation ID filter
                proposals.retain(|p| {
                    matches!(&p.scope, icn_governance::ProposalScope::Federation(id) if id.as_str() == other)
                });
            }
        }
    }

    // Apply sorting
    // Valid sort fields for proposals
    const VALID_PROPOSAL_SORT_FIELDS: &[&str] = &["created_at", "title"];

    let sort_fields = query.sort_fields();
    if sort_fields.is_empty() {
        // Default: sort by creation time (newest first)
        proposals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    } else {
        // Validate sort field
        let sort = &sort_fields[0];
        if !VALID_PROPOSAL_SORT_FIELDS.contains(&sort.field.as_str()) {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Invalid sort field '{}'. Valid fields: {}",
                sort.field,
                VALID_PROPOSAL_SORT_FIELDS.join(", ")
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
            _ => unreachable!(), // Already validated above
        }
    }

    // Apply pagination
    let limit = query.limit;
    let total = proposals.len();

    // Parse cursor to get offset (format: "offset:<number>")
    let offset: usize = query
        .parse_offset_cursor()
        .map_err(crate::error::GatewayError::BadRequest)?
        .min(total);

    let paginated: Vec<_> = proposals.into_iter().skip(offset).take(limit).collect();
    let count = paginated.len();
    let next_offset = offset + count;
    let has_more = next_offset < total;

    // Build pagination metadata
    let pagination = if has_more {
        ListPagination::with_cursor(format!("offset:{next_offset}"), count).with_total(total)
    } else {
        ListPagination::last_page(count).with_total(total)
    };

    let response: ListResponse<_> = ListResponse::new(paginated, pagination);
    Ok(HttpResponse::Ok().json(response))
}

/// GET /gov/proposals/{id} - Get a specific proposal
#[get("/proposals/{id}")]
pub async fn get_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let proposal_id = ProposalId(id.into_inner());
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Proposal not found: {}", proposal_id.0))
    })?;

    Ok(HttpResponse::Ok().json(proposal))
}

/// GET /gov/proposals/{id}/votes - Get vote tally for a proposal
#[get("/proposals/{id}/votes")]
pub async fn get_votes(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let proposal_id = ProposalId(id.into_inner());

    // Verify proposal exists
    let _ = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Proposal not found: {}", proposal_id.0))
    })?;

    let tally = gov_mgr.get_vote_tally(&proposal_id).await?;

    #[derive(serde::Serialize)]
    struct VoteTallyResponse {
        for_votes: usize,
        against_votes: usize,
        abstain_votes: usize,
        total_votes: usize,
    }

    let response = VoteTallyResponse {
        for_votes: tally.for_votes,
        against_votes: tally.against_votes,
        abstain_votes: tally.abstain_votes,
        total_votes: tally.total_votes(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /gov/proposals/{id}/proof - Get cryptographic proof of proposal outcome
#[get("/proposals/{id}/proof")]
pub async fn get_proposal_proof(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:read")?;

    let proposal_id = ProposalId(id.into_inner());

    let proof = gov_mgr.get_proof(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!(
            "No proof found for proposal: {}",
            proposal_id.0
        ))
    })?;

    // Verify cryptographic binding before serving to clients
    if !proof.verify_binding() {
        tracing::warn!(
            "Invalid proof binding for proposal {} — not serving",
            proposal_id.0
        );
        return Err(crate::error::GatewayError::NotFound(format!(
            "No valid proof found for proposal: {}",
            proposal_id.0
        )));
    }

    Ok(HttpResponse::Ok().json(proof))
}

/// POST /gov/proposals/{id}/open - Open a proposal for voting
#[post("/proposals/{id}/open")]
pub async fn open_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
    req: web::Json<OpenProposalRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let requester_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let proposal_id = ProposalId(id.into_inner());

    // CRITICAL: Verify requester is a member of the proposal's domain
    // Fetch proposal to get domain_id
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Proposal not found: {}", proposal_id.0))
    })?;

    // Fetch domain to check membership
    let domain = gov_mgr
        .get_domain(&proposal.domain_id)
        .await?
        .ok_or_else(|| {
            crate::error::GatewayError::InternalError(format!(
                "Domain not found: {}",
                proposal.domain_id.0
            ))
        })?;

    // Check if requester is a domain member
    let is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(&requester_did),
        icn_governance::MembershipSource::TrustThreshold(_) => {
            // For trust-based membership, we'd need trust graph integration
            // For now, allow (will be enforced by daemon integration)
            true
        }
    };

    if !is_member {
        return Err(crate::error::GatewayError::AuthorizationFailed(format!(
            "Only domain members can open proposals (you are not a member of domain '{}')",
            proposal.domain_id.0
        )));
    }

    // Use custom voting period or get from domain config
    let voting_period_seconds = if let Some(period) = req.voting_period_seconds {
        // Validate custom voting period
        if period == 0 {
            return Err(crate::error::GatewayError::BadRequest(
                "Voting period must be greater than 0".to_string(),
            ));
        }
        if period > validation::MAX_VOTING_PERIOD_SECONDS {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Voting period exceeds maximum of {} seconds (1 year)",
                validation::MAX_VOTING_PERIOD_SECONDS
            )));
        }
        period
    } else {
        // Get default from domain (would need to fetch proposal and domain)
        86400 * 7 // Default 7 days if not specified
    };

    gov_mgr
        .open_proposal(proposal_id.clone(), voting_period_seconds)
        .await?;

    // Track proposal opening
    gateway::governance_proposals_opened_inc();

    // Return updated proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError(
            "Proposal opening succeeded but proposal not found".to_string(),
        )
    })?;

    // Calculate closes_at timestamp from proposal state
    let closes_at = if let icn_governance::ProposalState::Open {
        opened_at: _,
        closes_at,
    } = proposal.state
    {
        closes_at
    } else {
        // Fallback to current time + voting period if state doesn't match
        icn_time::current_timestamp_secs() + voting_period_seconds
    };

    // Broadcast event to WebSocket subscribers
    event_broadcaster
        .broadcast(
            &proposal.domain_id.0,
            GatewayEvent::GovernanceProposalOpened {
                proposal_id: proposal_id.0.clone(),
                domain_id: proposal.domain_id.0.clone(),
                closes_at,
            },
        )
        .await;

    Ok(HttpResponse::Ok().json(proposal))
}

/// POST /gov/proposals/{id}/close - Close a proposal and finalize voting
#[post("/proposals/{id}/close")]
pub async fn close_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    notification_service: web::Data<Arc<crate::notifications::NotificationService>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let requester_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let proposal_id = ProposalId(id.into_inner());

    // CRITICAL: Verify requester is a member of the proposal's domain
    // Fetch proposal to get domain_id
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Proposal not found: {}", proposal_id.0))
    })?;

    // Fetch domain to check membership
    let domain = gov_mgr
        .get_domain(&proposal.domain_id)
        .await?
        .ok_or_else(|| {
            crate::error::GatewayError::InternalError(format!(
                "Domain not found: {}",
                proposal.domain_id.0
            ))
        })?;

    // Check if requester is a domain member
    let is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(&requester_did),
        icn_governance::MembershipSource::TrustThreshold(_) => {
            // For trust-based membership, we'd need trust graph integration
            // For now, allow (will be enforced by daemon integration)
            true
        }
    };

    if !is_member {
        return Err(crate::error::GatewayError::AuthorizationFailed(format!(
            "Only domain members can close proposals (you are not a member of domain '{}')",
            proposal.domain_id.0
        )));
    }

    gov_mgr.close_proposal(proposal_id.clone()).await?;

    // Track proposal closing
    gateway::governance_proposals_closed_inc();

    // Return updated proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError(
            "Proposal closing succeeded but proposal not found".to_string(),
        )
    })?;

    // Determine outcome from proposal state
    let outcome = match &proposal.state {
        icn_governance::ProposalState::Accepted { .. } => "accepted",
        icn_governance::ProposalState::Rejected { .. } => "rejected",
        icn_governance::ProposalState::NoQuorum { .. } => "no_quorum",
        _ => "unknown", // Shouldn't happen after close, but handle gracefully
    };

    // Broadcast event to WebSocket subscribers
    event_broadcaster
        .broadcast(
            &proposal.domain_id.0,
            GatewayEvent::GovernanceProposalClosed {
                proposal_id: proposal_id.0.clone(),
                domain_id: proposal.domain_id.0.clone(),
                outcome: outcome.to_string(),
            },
        )
        .await;

    // Send notifications to all voters about the outcome
    if let Ok(voters) = gov_mgr.get_voter_dids(&proposal_id).await {
        let notif = crate::notifications::NotificationService::proposal_result_notification(
            &proposal_id.0,
            &proposal.title,
            outcome,
        );
        let voter_count = voters.len();
        for voter in &voters {
            if let Err(e) = notification_service.send_to_did(voter, notif.clone()).await {
                tracing::warn!(
                    "Failed to send proposal result notification to {}: {}",
                    voter,
                    e
                );
            }
        }
        tracing::info!(
            "Sent proposal result notifications to {} voters for proposal {}",
            voter_count,
            proposal_id.0
        );
    }

    Ok(HttpResponse::Ok().json(proposal))
}

// ============================================================================
// Federation Proposal Endpoints
// ============================================================================

// ============================================================================
// Federation Proposal Helpers
// ============================================================================

/// Common fields for federation proposals
struct FederationProposalCommon {
    domain_id: String,
    title: String,
    description: String,
    proposer_did: Did,
}

/// Extract common authorization and validation for federation proposals
fn extract_federation_common(
    http_req: &HttpRequest,
    domain_id: &str,
    title: &str,
    description: &str,
) -> Result<FederationProposalCommon> {
    // Check authorization
    require_scope(http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let proposer_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate common fields
    validation::validate_domain_id(domain_id)?;
    validation::validate_proposal_title(title)?;
    validation::validate_proposal_description(description)?;

    Ok(FederationProposalCommon {
        domain_id: domain_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        proposer_did,
    })
}

/// Create a federation proposal with the common workflow
async fn create_federation_proposal_impl(
    gov_mgr: &GovernanceManager,
    event_broadcaster: &EventBroadcaster,
    common: FederationProposalCommon,
    fed_proposal: FederationProposal,
) -> Result<HttpResponse> {
    // Validate the federation proposal itself
    fed_proposal
        .validate()
        .map_err(crate::error::GatewayError::BadRequest)?;

    // Wrap in ProposalPayload
    let payload = ProposalPayload::Federation(fed_proposal);

    // Create proposal via governance manager
    let domain_id = GovernanceDomainId(common.domain_id.clone());
    let suggested_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));

    let proposal_id = gov_mgr
        .create_proposal(
            suggested_id,
            domain_id,
            common.proposer_did.clone(),
            common.title.clone(),
            common.description.clone(),
            payload,
        )
        .await?;

    // Track proposal creation
    gateway::governance_proposals_created_inc();

    // Broadcast event to WebSocket subscribers
    event_broadcaster
        .broadcast(
            &common.domain_id,
            GatewayEvent::GovernanceProposalCreated {
                proposal_id: proposal_id.0.clone(),
                domain_id: common.domain_id.clone(),
                proposer: common.proposer_did.to_string(),
                title: common.title.clone(),
                payload_type: "federation".to_string(),
            },
        )
        .await;

    // Return created proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError(
            "Proposal creation succeeded but proposal not found".to_string(),
        )
    })?;

    Ok(HttpResponse::Created().json(proposal))
}

/// Parse data sharing level string to enum
fn parse_data_sharing_level(s: &str) -> Result<DataSharingLevel> {
    match s {
        "none" => Ok(DataSharingLevel::None),
        "metadata_only" => Ok(DataSharingLevel::MetadataOnly),
        "full" => Ok(DataSharingLevel::Full),
        _ => Err(crate::error::GatewayError::InternalError(format!(
            "Invalid data_sharing_level after validation: '{s}'"
        ))),
    }
}

/// Parse dispute resolution string to enum
fn parse_dispute_resolution(s: &str) -> Result<DisputeResolutionMethod> {
    if s.starts_with("arbitrator:") {
        let arbitrator_id = s.strip_prefix("arbitrator:").unwrap_or("").to_string();
        Ok(DisputeResolutionMethod::ArbitratorCooperative { arbitrator_id })
    } else {
        match s {
            "federation_mediation" => Ok(DisputeResolutionMethod::FederationMediation),
            "federation_vote" => Ok(DisputeResolutionMethod::FederationVote),
            _ => Err(crate::error::GatewayError::InternalError(format!(
                "Invalid dispute_resolution after validation: '{s}'"
            ))),
        }
    }
}

/// Parse settlement interval string to enum
fn parse_settlement_interval(s: &str) -> Result<SettlementInterval> {
    match s {
        "daily" => Ok(SettlementInterval::Daily),
        "weekly" => Ok(SettlementInterval::Weekly),
        "monthly" => Ok(SettlementInterval::Monthly),
        "manual" => Ok(SettlementInterval::Manual),
        _ => Err(crate::error::GatewayError::InternalError(format!(
            "Invalid settlement_interval after validation: '{s}'"
        ))),
    }
}

// ============================================================================
// Federation Proposal Endpoints
// ============================================================================

/// POST /gov/proposals/federation/join - Create a "join federation" proposal
#[post("/proposals/federation/join")]
pub async fn create_join_federation_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<JoinFederationProposalRequest>,
) -> Result<HttpResponse> {
    let common =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    // Validate federation-specific fields
    validation::validate_federation_id(&req.federation_id)?;
    // Validate trust score (f64 is validated by gateway, not during deserialization)
    let data_sharing_level_str =
        validation::validate_data_sharing_level(&req.terms.data_sharing_level)?;
    let dispute_resolution_str =
        validation::validate_dispute_resolution(&req.terms.dispute_resolution)?;

    // Convert string types to enums
    let data_sharing_level = parse_data_sharing_level(&data_sharing_level_str)?;
    let dispute_resolution = parse_dispute_resolution(&dispute_resolution_str)?;

    // Validate trust score
    validation::validate_trust_score(req.terms.min_trust_threshold)?;

    // Build FederationTerms (governance types now use f64 directly)
    let terms = FederationTerms {
        min_trust_threshold: req.terms.min_trust_threshold,
        governance_binding: req.terms.governance_binding,
        data_sharing_level,
        dispute_resolution,
    };

    // Build FederationProposal
    let fed_proposal = FederationProposal::JoinFederation {
        federation_id: req.federation_id.clone(),
        terms,
        sponsor_coop_id: req.sponsor_coop_id.clone(),
    };

    create_federation_proposal_impl(&gov_mgr, &event_broadcaster, common, fed_proposal).await
}

/// POST /gov/proposals/federation/leave - Create a "leave federation" proposal
#[post("/proposals/federation/leave")]
pub async fn create_leave_federation_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<LeaveFederationProposalRequest>,
) -> Result<HttpResponse> {
    let common =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    // Validate federation-specific fields
    validation::validate_federation_id(&req.federation_id)?;
    validation::validate_reason(&req.reason)?;
    validation::validate_grace_period_days(req.grace_period_days)?;

    // Build FederationProposal
    let fed_proposal = FederationProposal::LeaveFederation {
        federation_id: req.federation_id.clone(),
        reason: req.reason.clone(),
        grace_period_days: req.grace_period_days,
    };

    create_federation_proposal_impl(&gov_mgr, &event_broadcaster, common, fed_proposal).await
}

/// POST /gov/proposals/federation/clearing - Create an "establish clearing" proposal
#[post("/proposals/federation/clearing")]
pub async fn create_establish_clearing_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<EstablishClearingProposalRequest>,
) -> Result<HttpResponse> {
    let common =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    // Validate federation-specific fields
    validation::validate_federation_id(&req.partner_coop_id)?;
    validation::validate_max_imbalance(req.max_imbalance)?;
    validation::validate_currency(&req.currency)?;
    let settlement_interval_str =
        validation::validate_settlement_interval(&req.settlement_interval)?;

    // Parse partner DID
    let partner_coop_did: Did = req.partner_coop_did.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid partner_coop_did: {e}"))
    })?;

    // Convert settlement interval string to enum
    let settlement_interval = parse_settlement_interval(&settlement_interval_str)?;

    // Build FederationProposal
    let fed_proposal = FederationProposal::EstablishClearing {
        partner_coop_id: req.partner_coop_id.clone(),
        partner_coop_did,
        max_imbalance: req.max_imbalance,
        settlement_interval,
        currency: req.currency.clone(),
    };

    create_federation_proposal_impl(&gov_mgr, &event_broadcaster, common, fed_proposal).await
}

/// POST /gov/proposals/federation/clearing/terminate - Create a "terminate clearing" proposal
#[post("/proposals/federation/clearing/terminate")]
pub async fn create_terminate_clearing_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<TerminateClearingProposalRequest>,
) -> Result<HttpResponse> {
    let common =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    // Validate federation-specific fields
    validation::validate_federation_id(&req.partner_coop_id)?;
    validation::validate_reason(&req.reason)?;

    // Build FederationProposal
    let fed_proposal = FederationProposal::TerminateClearing {
        partner_coop_id: req.partner_coop_id.clone(),
        reason: req.reason.clone(),
    };

    create_federation_proposal_impl(&gov_mgr, &event_broadcaster, common, fed_proposal).await
}

/// POST /gov/proposals/federation/vouch - Create a "vouch for cooperative" proposal
#[post("/proposals/federation/vouch")]
pub async fn create_vouch_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<VouchProposalRequest>,
) -> Result<HttpResponse> {
    let common =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    // Validate federation-specific fields
    validation::validate_federation_id(&req.target_coop_id)?;
    validation::validate_trust_score(req.trust_score)?;
    validation::validate_context(&req.context)?;
    validation::validate_evidence(&req.evidence)?;

    // Parse target DID
    let target_coop_did: Did = req.target_coop_did.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid target_coop_did: {e}"))
    })?;

    // Build FederationProposal
    let fed_proposal = FederationProposal::VouchForCooperative {
        target_coop_id: req.target_coop_id.clone(),
        target_coop_did,
        trust_score: req.trust_score,
        context: req.context.clone(),
        evidence: req.evidence.clone(),
    };

    create_federation_proposal_impl(&gov_mgr, &event_broadcaster, common, fed_proposal).await
}

/// POST /gov/proposals/federation/vouch/revoke - Create a "revoke vouch" proposal
#[post("/proposals/federation/vouch/revoke")]
pub async fn create_revoke_vouch_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<RevokeVouchProposalRequest>,
) -> Result<HttpResponse> {
    let common =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    // Validate federation-specific fields
    validation::validate_federation_id(&req.target_coop_id)?;
    validation::validate_reason(&req.reason)?;

    // Build FederationProposal
    let fed_proposal = FederationProposal::RevokeVouch {
        target_coop_id: req.target_coop_id.clone(),
        reason: req.reason.clone(),
    };

    create_federation_proposal_impl(&gov_mgr, &event_broadcaster, common, fed_proposal).await
}

/// POST /gov/proposals/federation/policy - Create an "update federation policy" proposal
#[post("/proposals/federation/policy")]
pub async fn create_update_federation_policy_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<UpdateFederationPolicyProposalRequest>,
) -> Result<HttpResponse> {
    let common =
        extract_federation_common(&http_req, &req.domain_id, &req.title, &req.description)?;

    // Validate federation-specific fields
    validation::validate_auto_accept_threshold(req.auto_accept_vouch_threshold)?;
    validation::validate_trust_decay_factor(req.trust_decay_factor)?;
    validation::validate_max_attestations_per_minute(req.max_attestations_per_minute)?;

    // Ensure at least one policy field is provided
    if req.auto_accept_vouch_threshold.is_none()
        && req.trust_decay_factor.is_none()
        && req.max_attestations_per_minute.is_none()
    {
        return Err(crate::error::GatewayError::BadRequest(
            "At least one policy field must be provided".to_string(),
        ));
    }

    // Build FederationProposal (governance types now use f64 directly)
    let fed_proposal = FederationProposal::UpdateFederationPolicy {
        auto_accept_vouch_threshold: req.auto_accept_vouch_threshold,
        trust_decay_factor: req.trust_decay_factor,
        max_attestations_per_minute: req.max_attestations_per_minute,
    };

    create_federation_proposal_impl(&gov_mgr, &event_broadcaster, common, fed_proposal).await
}

// ============================================================================
// Vote Endpoints
// ============================================================================

/// POST /gov/proposals/{id}/vote - Cast a vote on a proposal
#[post("/proposals/{id}/vote")]
pub async fn cast_vote(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    notification_service: web::Data<Arc<crate::notifications::NotificationService>>,
    id: web::Path<String>,
    req: web::Json<CastVoteRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let voter_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate comment length
    validation::validate_vote_comment(&req.comment)?;

    // Parse vote choice
    let choice = match req.choice.to_lowercase().as_str() {
        "for" => VoteChoice::For,
        "against" => VoteChoice::Against,
        "abstain" => VoteChoice::Abstain,
        _ => {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Invalid vote choice: {}",
                req.choice
            )))
        }
    };

    let proposal_id = ProposalId(id.into_inner());
    gov_mgr
        .cast_vote(
            proposal_id.clone(),
            voter_did.clone(),
            choice,
            req.comment.clone(),
        )
        .await?;

    // Track vote
    gateway::governance_votes_cast_inc();

    // Return updated proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError(
            "Vote cast succeeded but proposal not found".to_string(),
        )
    })?;

    // Broadcast event to WebSocket subscribers
    let vote_event = GatewayEvent::GovernanceVoteCast {
        proposal_id: proposal_id.0.clone(),
        domain_id: proposal.domain_id.0.clone(),
        voter: voter_did.to_string(),
        choice: req.choice.clone(),
    };

    event_broadcaster
        .broadcast(&proposal.domain_id.0, vote_event.clone())
        .await;

    // Send push notification (async, don't block response)
    let notif_service = notification_service.into_inner();
    tokio::spawn(async move {
        use crate::notification_listener::handle_event_for_notifications;
        handle_event_for_notifications(&vote_event, &notif_service).await;
    });

    Ok(HttpResponse::Ok().json(proposal))
}

// ============================================================================
// Delegation Endpoints
// ============================================================================

/// Helper to convert a Delegation to DelegationResponse
fn delegation_to_response(d: &Delegation) -> DelegationResponse {
    let scope_str = match &d.scope {
        DelegationScope::Blanket => "blanket".to_string(),
        DelegationScope::Domain(domain_id) => format!("domain:{}", domain_id.0),
        DelegationScope::Proposal(proposal_id) => format!("proposal:{}", proposal_id.0),
    };
    let now = icn_time::current_timestamp_secs();
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

/// POST /gov/delegations - Create a new vote delegation
#[post("/delegations")]
pub async fn create_delegation(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    req: web::Json<CreateDelegationRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let delegator_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Parse delegate DID
    let delegate_did: Did = req.delegate.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid delegate DID: {e}"))
    })?;

    // Parse scope
    let scope = parse_delegation_scope(&req.scope)?;

    // Create the delegation
    let mut delegation = Delegation::new(delegator_did, delegate_did, scope);
    if let Some(expires_at) = req.expires_at {
        delegation = delegation.with_expiry(expires_at);
    }

    // Store the delegation via governance manager
    gov_mgr.create_delegation(delegation.clone()).await?;

    Ok(HttpResponse::Created().json(delegation_to_response(&delegation)))
}

/// Query parameters for listing delegations
#[derive(Debug, serde::Deserialize)]
pub struct ListDelegationsQuery {
    /// Include revoked delegations (default: false)
    /// By default, only active delegations are shown to protect privacy and reduce noise.
    #[serde(default)]
    include_revoked: bool,
}

/// GET /gov/delegations - List delegations for the authenticated user
///
/// By default, only active (non-revoked) delegations are returned.
/// Use `?include_revoked=true` to also see revoked delegations.
#[get("/delegations")]
pub async fn list_delegations(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    query: web::Query<ListDelegationsQuery>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let caller_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Get delegations given by and received by this user
    let given = gov_mgr.get_delegations_from(&caller_did).await?;
    let received = gov_mgr.get_delegations_to(&caller_did).await?;

    // Filter out revoked delegations unless explicitly requested
    let filter_revoked = |d: &Delegation| query.include_revoked || d.revoked_at.is_none();

    let response = DelegationListResponse {
        given: given
            .iter()
            .filter(|d| filter_revoked(d))
            .map(delegation_to_response)
            .collect(),
        received: received
            .iter()
            .filter(|d| filter_revoked(d))
            .map(delegation_to_response)
            .collect(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /gov/delegations/{id} - Get a specific delegation
///
/// Only the delegator or delegate can view a delegation to protect privacy.
#[get("/delegations/{id}")]
pub async fn get_delegation(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let caller_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let delegation_id = icn_governance::DelegationId(id.into_inner());
    let delegation = gov_mgr
        .get_delegation(&delegation_id)
        .await?
        .ok_or_else(|| {
            crate::error::GatewayError::NotFound(format!(
                "Delegation not found: {}",
                delegation_id.0
            ))
        })?;

    // Privacy: Only allow the delegator or delegate to view the delegation
    if delegation.delegator != caller_did && delegation.delegate != caller_did {
        return Err(crate::error::GatewayError::AuthorizationFailed(
            "You can only view delegations you are involved in".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(delegation_to_response(&delegation)))
}

/// DELETE /gov/delegations/{id} - Revoke a delegation
#[delete("/delegations/{id}")]
pub async fn revoke_delegation(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let caller_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let delegation_id = icn_governance::DelegationId(id.into_inner());

    // Verify the delegation exists and belongs to the caller
    let delegation = gov_mgr
        .get_delegation(&delegation_id)
        .await?
        .ok_or_else(|| {
            crate::error::GatewayError::NotFound(format!(
                "Delegation not found: {}",
                delegation_id.0
            ))
        })?;

    // Only the delegator can revoke their delegation
    if delegation.delegator != caller_did {
        return Err(crate::error::GatewayError::AuthorizationFailed(
            "Only the delegator can revoke a delegation".to_string(),
        ));
    }

    // Revoke the delegation
    let now = icn_time::current_timestamp_secs();
    gov_mgr.revoke_delegation(&delegation_id, now).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// Parse a delegation scope string (e.g., "blanket", "domain:coop-id", "proposal:prop-id")
fn parse_delegation_scope(scope: &str) -> Result<DelegationScope> {
    if scope == "blanket" {
        return Ok(DelegationScope::Blanket);
    }

    if let Some(domain_id) = scope.strip_prefix("domain:") {
        if domain_id.is_empty() {
            return Err(crate::error::GatewayError::BadRequest(
                "Domain ID cannot be empty in scope".to_string(),
            ));
        }
        return Ok(DelegationScope::Domain(GovernanceDomainId(
            domain_id.to_string(),
        )));
    }

    if let Some(proposal_id) = scope.strip_prefix("proposal:") {
        if proposal_id.is_empty() {
            return Err(crate::error::GatewayError::BadRequest(
                "Proposal ID cannot be empty in scope".to_string(),
            ));
        }
        return Ok(DelegationScope::Proposal(ProposalId(
            proposal_id.to_string(),
        )));
    }

    Err(crate::error::GatewayError::BadRequest(format!(
        "Invalid delegation scope: '{scope}'. Must be 'blanket', 'domain:<id>', or 'proposal:<id>'"
    )))
}

// ============================================================================
// Deliberation Endpoints
// ============================================================================

/// POST /gov/proposals/{id}/deliberation/start - Start deliberation period
#[post("/proposals/{id}/deliberation/start")]
pub async fn start_deliberation(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
    req: web::Json<StartDeliberationRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let caller_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let proposal_id = ProposalId(id.into_inner());

    // Get the proposal to verify authorization
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Proposal not found: {}", proposal_id.0))
    })?;

    // Only the proposer can start deliberation
    if proposal.proposer != caller_did {
        return Err(crate::error::GatewayError::AuthorizationFailed(
            "Only the proposer can start deliberation".to_string(),
        ));
    }

    // Use the provided period or get default from domain config
    let period = req.deliberation_period_seconds.unwrap_or(7 * 24 * 60 * 60); // Default: 7 days

    gov_mgr.start_deliberation(&proposal_id, period).await?;

    // Get updated proposal
    let updated_proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError("Proposal disappeared after update".to_string())
    })?;

    Ok(HttpResponse::Ok().json(updated_proposal))
}

/// POST /gov/proposals/{id}/deliberation/end - End deliberation and open voting
#[post("/proposals/{id}/deliberation/end")]
pub async fn end_deliberation(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
    req: web::Json<EndDeliberationRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let caller_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let proposal_id = ProposalId(id.into_inner());

    // Get the proposal to verify authorization
    let proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Proposal not found: {}", proposal_id.0))
    })?;

    // Only the proposer can end deliberation
    if proposal.proposer != caller_did {
        return Err(crate::error::GatewayError::AuthorizationFailed(
            "Only the proposer can end deliberation".to_string(),
        ));
    }

    let voting_period = req.voting_period_seconds.unwrap_or(7 * 24 * 60 * 60); // Default: 7 days

    gov_mgr
        .end_deliberation_and_open(&proposal_id, voting_period)
        .await?;

    // Get updated proposal
    let updated_proposal = gov_mgr.get_proposal(&proposal_id).await?.ok_or_else(|| {
        crate::error::GatewayError::InternalError("Proposal disappeared after update".to_string())
    })?;

    Ok(HttpResponse::Ok().json(updated_proposal))
}

// ============================================================================
// Discussion Endpoints
// ============================================================================

/// POST /gov/proposals/{id}/comments - Add a comment to a proposal
#[post("/proposals/{id}/comments")]
pub async fn add_comment(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
    req: web::Json<AddCommentRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let author: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let proposal_id = ProposalId(id.into_inner());

    // Validate content
    if req.content.trim().is_empty() {
        return Err(crate::error::GatewayError::BadRequest(
            "Comment content cannot be empty".to_string(),
        ));
    }

    if req.content.len() > 10000 {
        return Err(crate::error::GatewayError::BadRequest(
            "Comment content exceeds maximum length of 10000 characters".to_string(),
        ));
    }

    // Create the comment
    let mut comment =
        icn_governance::Comment::new(proposal_id.clone(), author, req.content.clone());

    // Set parent if this is a reply
    if let Some(parent_id) = &req.parent_id {
        comment.parent_id = Some(icn_governance::CommentId(parent_id.clone()));
    }

    let comment_id = gov_mgr.add_comment(comment.clone()).await?;

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

/// GET /gov/proposals/{id}/comments - List comments on a proposal
#[get("/proposals/{id}/comments")]
pub async fn list_comments(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
    query: web::Query<ListCommentsQuery>,
) -> Result<HttpResponse> {
    // Check authorization (read access)
    require_scope(&http_req, "gov:read")?;

    let proposal_id = ProposalId(id.into_inner());

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let comments = gov_mgr.list_comments(&proposal_id, limit, offset).await?;
    let total = gov_mgr.count_comments(&proposal_id).await?;

    let responses: Vec<CommentResponse> = comments
        .into_iter()
        .map(|c| CommentResponse {
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
        })
        .collect();

    Ok(HttpResponse::Ok().json(ListCommentsResponse {
        comments: responses,
        total,
        limit,
        offset,
    }))
}

/// GET /gov/proposals/{id}/discussion - Get full discussion with metadata
#[get("/proposals/{id}/discussion")]
pub async fn get_discussion(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization (read access)
    require_scope(&http_req, "gov:read")?;

    let proposal_id = ProposalId(id.into_inner());

    let discussion = gov_mgr.get_discussion(&proposal_id).await?;

    match discussion {
        Some(d) => {
            let comments: Vec<CommentResponse> = d
                .comments
                .into_iter()
                .map(|c| CommentResponse {
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
                })
                .collect();

            Ok(HttpResponse::Ok().json(DiscussionResponse {
                proposal_id: d.proposal_id.0,
                comments,
                participant_count: d.participant_count,
                last_activity_at: d.last_activity_at,
            }))
        }
        None => Ok(HttpResponse::Ok().json(DiscussionResponse {
            proposal_id: proposal_id.0,
            comments: vec![],
            participant_count: 0,
            last_activity_at: 0,
        })),
    }
}

/// PUT /gov/comments/{id} - Edit a comment
#[put("/comments/{id}")]
pub async fn edit_comment(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
    req: web::Json<EditCommentRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let editor: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let comment_id = icn_governance::CommentId(id.into_inner());

    // Validate content
    if req.content.trim().is_empty() {
        return Err(crate::error::GatewayError::BadRequest(
            "Comment content cannot be empty".to_string(),
        ));
    }

    gov_mgr
        .edit_comment(&comment_id, req.content.clone(), &editor)
        .await?;

    // Get updated comment
    let updated = gov_mgr.get_comment(&comment_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound("Comment not found after update".to_string())
    })?;

    Ok(HttpResponse::Ok().json(CommentResponse {
        id: updated.id.0,
        proposal_id: updated.proposal_id.0,
        author: updated.author.to_string(),
        content: updated.content,
        parent_id: updated.parent_id.map(|p| p.0),
        created_at: updated.created_at,
        updated_at: updated.updated_at,
        reactions: updated
            .reactions
            .into_iter()
            .map(|(k, v)| (k, v.len()))
            .collect(),
        is_edited: updated.is_edited,
        is_deleted: updated.is_deleted,
    }))
}

/// DELETE /gov/comments/{id} - Delete a comment
#[delete("/comments/{id}")]
pub async fn delete_comment(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let deleter: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let comment_id = icn_governance::CommentId(id.into_inner());

    gov_mgr.delete_comment(&comment_id, &deleter).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// POST /gov/comments/{id}/reactions - Add a reaction to a comment
#[post("/comments/{id}/reactions")]
pub async fn add_reaction(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    id: web::Path<String>,
    req: web::Json<AddReactionRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let reactor: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let comment_id = icn_governance::CommentId(id.into_inner());

    // Validate emoji
    if req.emoji.is_empty() || req.emoji.chars().count() > 10 {
        return Err(crate::error::GatewayError::BadRequest(
            "Invalid emoji: must be 1-10 characters".to_string(),
        ));
    }

    gov_mgr
        .add_reaction(&comment_id, &reactor, &req.emoji)
        .await?;

    Ok(HttpResponse::Created().finish())
}

/// DELETE /gov/comments/{id}/reactions/{emoji} - Remove a reaction from a comment
#[delete("/comments/{id}/reactions/{emoji}")]
pub async fn remove_reaction(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let reactor: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let (comment_id_str, emoji) = path.into_inner();
    let comment_id = icn_governance::CommentId(comment_id_str);

    gov_mgr
        .remove_reaction(&comment_id, &reactor, &emoji)
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

// ============================================================================
// Request/Response Types for Deliberation & Discussion
// ============================================================================

/// Request to start deliberation
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StartDeliberationRequest {
    /// Duration in seconds (optional, defaults to domain config or 7 days)
    pub deliberation_period_seconds: Option<u64>,
}

/// Request to end deliberation
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EndDeliberationRequest {
    /// Voting period in seconds (optional, defaults to domain config or 7 days)
    pub voting_period_seconds: Option<u64>,
}

/// Request to add a comment
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AddCommentRequest {
    /// Comment content
    pub content: String,
    /// Parent comment ID (if this is a reply)
    pub parent_id: Option<String>,
}

/// Request to edit a comment
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EditCommentRequest {
    /// New content
    pub content: String,
}

/// Request to add a reaction
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AddReactionRequest {
    /// Emoji reaction
    pub emoji: String,
}

/// Query parameters for listing comments
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ListCommentsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Response for a single comment
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommentResponse {
    pub id: String,
    pub proposal_id: String,
    pub author: String,
    pub content: String,
    pub parent_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub reactions: std::collections::HashMap<String, usize>,
    pub is_edited: bool,
    pub is_deleted: bool,
}

/// Response for listing comments
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListCommentsResponse {
    pub comments: Vec<CommentResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Response for a discussion
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscussionResponse {
    pub proposal_id: String,
    pub comments: Vec<CommentResponse>,
    pub participant_count: usize,
    pub last_activity_at: u64,
}

// ============================================================================
// Action Item Endpoints
// ============================================================================

/// Check if a user is a member of a governance domain
async fn check_domain_membership(
    gov_mgr: &GovernanceManager,
    domain_id: &GovernanceDomainId,
    user_did: &Did,
) -> Result<()> {
    let domain = gov_mgr.get_domain(domain_id).await?.ok_or_else(|| {
        crate::error::GatewayError::NotFound(format!("Domain not found: {}", domain_id.0))
    })?;

    let is_member = match &domain.config.membership.source {
        icn_governance::MembershipSource::StaticList(members) => members.contains(user_did),
        icn_governance::MembershipSource::TrustThreshold(_) => {
            // For trust-based membership, trust graph integration required
            // For now, allow (will be enforced by daemon integration)
            true
        }
    };

    if !is_member {
        return Err(crate::error::GatewayError::AuthorizationFailed(format!(
            "Only domain members can perform this action (you are not a member of domain '{}')",
            domain_id.0
        )));
    }

    Ok(())
}

/// Validation constants for action items
const MAX_TITLE_LENGTH: usize = 200;
const MAX_DESCRIPTION_LENGTH: usize = 5000;
const MAX_TAGS: usize = 10;
const MAX_TAG_LENGTH: usize = 50;
const MAX_MEETING_CONTEXT_LENGTH: usize = 500;

/// Validate action item input
fn validate_action_item_input(req: &CreateActionItemRequest) -> Result<()> {
    // Title validation
    if req.title.is_empty() {
        return Err(crate::error::GatewayError::BadRequest(
            "Title cannot be empty".to_string(),
        ));
    }
    if req.title.len() > MAX_TITLE_LENGTH {
        return Err(crate::error::GatewayError::BadRequest(format!(
            "Title exceeds maximum length of {MAX_TITLE_LENGTH} characters"
        )));
    }

    // Description validation
    if let Some(ref desc) = req.description {
        if desc.len() > MAX_DESCRIPTION_LENGTH {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Description exceeds maximum length of {MAX_DESCRIPTION_LENGTH} characters"
            )));
        }
    }

    // Tags validation
    if req.tags.len() > MAX_TAGS {
        return Err(crate::error::GatewayError::BadRequest(format!(
            "Too many tags (maximum {MAX_TAGS})"
        )));
    }
    for tag in &req.tags {
        if tag.len() > MAX_TAG_LENGTH {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Tag '{tag}' exceeds maximum length of {MAX_TAG_LENGTH} characters"
            )));
        }
        if tag.is_empty() {
            return Err(crate::error::GatewayError::BadRequest(
                "Tags cannot be empty strings".to_string(),
            ));
        }
    }

    // Meeting context validation
    if let Some(ref context) = req.meeting_context {
        if context.len() > MAX_MEETING_CONTEXT_LENGTH {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Meeting context exceeds maximum length of {MAX_MEETING_CONTEXT_LENGTH} characters"
            )));
        }
    }

    // Due date validation - can't be in the past (allow 5 minute grace period)
    if let Some(due_date) = req.due_date {
        let now = icn_time::current_timestamp_secs();
        let grace_period = 5 * 60; // 5 minutes
        if due_date < now.saturating_sub(grace_period) {
            return Err(crate::error::GatewayError::BadRequest(
                "Due date cannot be in the past".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate action item update input
fn validate_action_item_update(req: &UpdateActionItemRequest) -> Result<()> {
    // Title validation
    if let Some(ref title) = req.title {
        if title.is_empty() {
            return Err(crate::error::GatewayError::BadRequest(
                "Title cannot be empty".to_string(),
            ));
        }
        if title.len() > MAX_TITLE_LENGTH {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Title exceeds maximum length of {MAX_TITLE_LENGTH} characters"
            )));
        }
    }

    // Description validation
    if let Some(ref desc) = req.description {
        if desc.len() > MAX_DESCRIPTION_LENGTH {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Description exceeds maximum length of {MAX_DESCRIPTION_LENGTH} characters"
            )));
        }
    }

    // Tags validation
    if let Some(ref tags) = req.tags {
        if tags.len() > MAX_TAGS {
            return Err(crate::error::GatewayError::BadRequest(format!(
                "Too many tags (maximum {MAX_TAGS})"
            )));
        }
        for tag in tags {
            if tag.len() > MAX_TAG_LENGTH {
                return Err(crate::error::GatewayError::BadRequest(format!(
                    "Tag '{tag}' exceeds maximum length of {MAX_TAG_LENGTH} characters"
                )));
            }
            if tag.is_empty() {
                return Err(crate::error::GatewayError::BadRequest(
                    "Tags cannot be empty strings".to_string(),
                ));
            }
        }
    }

    // Due date validation - if provided and non-zero, can't be in the past
    if let Some(due_date) = req.due_date {
        if due_date != 0 {
            let now = icn_time::current_timestamp_secs();
            let grace_period = 5 * 60; // 5 minutes
            if due_date < now.saturating_sub(grace_period) {
                return Err(crate::error::GatewayError::BadRequest(
                    "Due date cannot be in the past".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// POST /gov/domains/{domain_id}/action-items - Create a new action item
#[post("/domains/{domain_id}/action-items")]
pub async fn create_action_item(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    domain_id: web::Path<String>,
    req: web::Json<CreateActionItemRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:write")?;

    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let creator_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let domain = GovernanceDomainId(domain_id.into_inner());

    // Verify domain membership
    check_domain_membership(&gov_mgr, &domain, &creator_did).await?;

    // Input validation
    validate_action_item_input(&req)?;

    // Parse assignee DID if provided
    let assignee: Option<Did> = if let Some(ref assignee_str) = req.assignee {
        Some(assignee_str.parse().map_err(|e| {
            crate::error::GatewayError::BadRequest(format!("Invalid assignee DID: {e}"))
        })?)
    } else {
        None
    };

    // Parse linked proposal if provided
    let linked_proposal = req.linked_proposal.as_ref().map(ProposalId::new);

    // Parse priority
    let priority = parse_priority(&req.priority)?;

    let item = gov_mgr
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
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    // Record metrics
    action_items::action_items_created_inc(&format!("{priority:?}"));

    Ok(HttpResponse::Created().json(action_item_to_response(&item)))
}

/// GET /gov/domains/{domain_id}/action-items - List action items
#[get("/domains/{domain_id}/action-items")]
pub async fn list_action_items(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    domain_id: web::Path<String>,
    query: web::Query<ActionItemFilterParams>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:read")?;

    let domain = GovernanceDomainId(domain_id.into_inner());

    // Build filter from query parameters
    let filter = build_action_item_filter(&query)?;

    let items = gov_mgr
        .list_action_items(&domain, &filter)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    let responses: Vec<ActionItemResponse> = items.iter().map(action_item_to_response).collect();

    Ok(HttpResponse::Ok().json(responses))
}

/// GET /gov/domains/{domain_id}/action-items/{item_id} - Get a specific action item
#[get("/domains/{domain_id}/action-items/{item_id}")]
pub async fn get_action_item(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:read")?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    let item = gov_mgr
        .get_action_item(&domain, &id)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| crate::error::GatewayError::NotFound("Action item not found".to_string()))?;

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// PUT /gov/domains/{domain_id}/action-items/{item_id} - Update an action item
#[put("/domains/{domain_id}/action-items/{item_id}")]
pub async fn update_action_item(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    path: web::Path<(String, String)>,
    req: web::Json<UpdateActionItemRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:write")?;

    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;
    let user_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    // Verify domain membership
    check_domain_membership(&gov_mgr, &domain, &user_did).await?;

    // Get existing item FIRST (before validation to prevent information leakage)
    let mut item = gov_mgr
        .get_action_item(&domain, &id)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| crate::error::GatewayError::NotFound("Action item not found".to_string()))?;

    // Ownership check BEFORE validation (prevents probing via validation errors)
    if item.created_by != user_did {
        return Err(crate::error::GatewayError::Forbidden(
            "Only the action item creator can update it".to_string(),
        ));
    }

    // Input validation (safe to reveal validation errors now that ownership is confirmed)
    validate_action_item_update(&req)?;

    // Capture old status for metrics tracking
    let old_status = item.status;

    // Apply updates
    if let Some(ref title) = req.title {
        item.title = title.clone();
    }
    if let Some(ref description) = req.description {
        item.description = Some(description.clone());
    }
    if let Some(ref assignee_str) = req.assignee {
        if assignee_str.is_empty() {
            item.assignee = None;
        } else {
            item.assignee = Some(assignee_str.parse().map_err(|e| {
                crate::error::GatewayError::BadRequest(format!("Invalid assignee DID: {e}"))
            })?);
        }
    }
    if let Some(due_date) = req.due_date {
        item.due_date = if due_date == 0 { None } else { Some(due_date) };
    }
    if let Some(ref priority) = req.priority {
        item.priority = parse_priority(priority)?;
    }
    if let Some(ref status) = req.status {
        item.status = parse_status(status)?;
    }
    if let Some(ref tags) = req.tags {
        item.tags = tags.clone();
    }

    item.updated_at = icn_time::current_timestamp_secs();

    gov_mgr
        .update_action_item(&item)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    // Record metrics if status changed
    if old_status != item.status {
        action_items::action_items_status_change_inc(
            &format!("{old_status:?}"),
            &format!("{:?}", item.status),
        );

        // Track completed/deferred status transitions
        if matches!(item.status, ActionItemStatus::Completed) {
            action_items::action_items_completed_inc();
        } else if matches!(item.status, ActionItemStatus::Deferred) {
            action_items::action_items_deferred_inc();
        }
    }

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// DELETE /gov/domains/{domain_id}/action-items/{item_id} - Delete an action item
#[delete("/domains/{domain_id}/action-items/{item_id}")]
pub async fn delete_action_item(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:write")?;

    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;
    let user_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    // Verify domain membership
    check_domain_membership(&gov_mgr, &domain, &user_did).await?;

    // Get the item to verify ownership before deleting
    let item = gov_mgr
        .get_action_item(&domain, &id)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| crate::error::GatewayError::NotFound("Action item not found".to_string()))?;

    // Ownership check: only the creator can delete the action item
    if item.created_by != user_did {
        return Err(crate::error::GatewayError::Forbidden(
            "Only the action item creator can delete it".to_string(),
        ));
    }

    gov_mgr
        .delete_action_item(&domain, &id)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    // Record metrics
    action_items::action_items_deleted_inc();

    Ok(HttpResponse::NoContent().finish())
}

/// POST /gov/domains/{domain_id}/action-items/{item_id}/notes - Add a note to an action item
#[post("/domains/{domain_id}/action-items/{item_id}/notes")]
pub async fn add_action_item_note(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    path: web::Path<(String, String)>,
    req: web::Json<AddActionItemNoteRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:write")?;

    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let author_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    // Verify domain membership
    check_domain_membership(&gov_mgr, &domain, &author_did).await?;

    let item = gov_mgr
        .add_action_item_note(&domain, &id, author_did, req.content.clone())
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// PUT /gov/domains/{domain_id}/action-items/{item_id}/status - Update action item status
#[put("/domains/{domain_id}/action-items/{item_id}/status")]
pub async fn update_action_item_status(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    path: web::Path<(String, String)>,
    req: web::Json<StatusUpdateRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "gov:write")?;

    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;
    let user_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    let (domain_id, item_id) = path.into_inner();
    let domain = GovernanceDomainId(domain_id);
    let id = parse_action_item_id(&item_id)?;

    // Verify domain membership
    check_domain_membership(&gov_mgr, &domain, &user_did).await?;

    // Get the item to check permissions
    let existing = gov_mgr
        .get_action_item(&domain, &id)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| crate::error::GatewayError::NotFound("Action item not found".to_string()))?;

    // Permission check: only creator or assignee can update status
    let is_creator = existing.created_by == user_did;
    let is_assignee = existing.assignee.as_ref().is_some_and(|a| a == &user_did);
    if !is_creator && !is_assignee {
        return Err(crate::error::GatewayError::Forbidden(
            "Only the creator or assignee can update action item status".to_string(),
        ));
    }

    let new_status = parse_status(&req.status)?;
    let old_status = existing.status;

    let item = gov_mgr
        .update_action_item_status(&domain, &id, new_status)
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    // Record metrics for status change
    action_items::action_items_status_change_inc(
        &format!("{old_status:?}"),
        &format!("{new_status:?}"),
    );

    // Track completed/deferred status transitions
    if matches!(new_status, ActionItemStatus::Completed) {
        action_items::action_items_completed_inc();
    } else if matches!(new_status, ActionItemStatus::Deferred) {
        action_items::action_items_deferred_inc();
    }

    Ok(HttpResponse::Ok().json(action_item_to_response(&item)))
}

/// Request for status update
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StatusUpdateRequest {
    pub status: String,
}

// Helper functions for action items

fn parse_action_item_id(s: &str) -> Result<ActionItemId> {
    uuid::Uuid::parse_str(s)
        .map(ActionItemId::from_uuid)
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid action item ID: {e}")))
}

fn parse_priority(s: &str) -> Result<ActionItemPriority> {
    match s.to_lowercase().as_str() {
        "low" => Ok(ActionItemPriority::Low),
        "medium" => Ok(ActionItemPriority::Medium),
        "high" => Ok(ActionItemPriority::High),
        "critical" => Ok(ActionItemPriority::Critical),
        _ => Err(crate::error::GatewayError::BadRequest(format!(
            "Invalid priority: {s}. Must be one of: low, medium, high, critical"
        ))),
    }
}

fn parse_status(s: &str) -> Result<ActionItemStatus> {
    match s.to_lowercase().as_str() {
        "pending" => Ok(ActionItemStatus::Pending),
        "in_progress" | "inprogress" => Ok(ActionItemStatus::InProgress),
        "completed" => Ok(ActionItemStatus::Completed),
        "deferred" => Ok(ActionItemStatus::Deferred),
        "cancelled" | "canceled" => Ok(ActionItemStatus::Cancelled),
        _ => Err(crate::error::GatewayError::BadRequest(format!(
            "Invalid status: {s}. Must be one of: pending, in_progress, completed, deferred, cancelled"
        ))),
    }
}

fn build_action_item_filter(query: &ActionItemFilterParams) -> Result<ActionItemFilter> {
    let mut filter = ActionItemFilter::default();

    if let Some(ref status) = query.status {
        filter.status = Some(parse_status(status)?);
    }
    if let Some(ref assignee) = query.assignee {
        let did: Did = assignee.parse().map_err(|e| {
            crate::error::GatewayError::BadRequest(format!("Invalid assignee DID: {e}"))
        })?;
        filter.assignee = Some(did);
    }
    if let Some(ref priority) = query.priority {
        filter.priority = Some(parse_priority(priority)?);
    }
    if query.overdue == Some(true) {
        filter.overdue_only = Some(icn_time::current_timestamp_secs());
    }
    if let Some(ref tag) = query.tag {
        filter.tag = Some(tag.clone());
    }

    Ok(filter)
}

fn action_item_to_response(item: &icn_governance::ActionItem) -> ActionItemResponse {
    let now = icn_time::current_timestamp_secs();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use crate::events::EventBroadcaster;
    use actix_web::{test, App, HttpMessage};
    use icn_governance::{
        GovernanceDomain, GovernanceDomainId, GovernanceParams, MembershipConfig, MembershipSource,
        Proposal, ProposalState,
    };
    use icn_identity::IdentityBundle;

    fn create_test_claims(did: &str, scopes: Vec<&str>) -> TokenClaims {
        TokenClaims {
            sub: did.to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            exp: 9999999999,
        }
    }

    #[actix_web::test]
    async fn test_create_and_get_domain() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(
                    web::scope("/gov")
                        .service(create_domain)
                        .service(get_domain),
                ),
        )
        .await;

        // Create domain
        let req_body = CreateDomainRequest {
            id: "coop:food".to_string(),
            name: "Food Cooperative".to_string(),
            profile: "cooperative".to_string(),
            quorum_percent: 50,
            approval_percent: 66,
            voting_period_days: 7,
            members: vec![alice.did().to_string()],
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/domains")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Get domain
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/domains/coop:food")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: GovernanceDomain = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.name, "Food Cooperative");
    }

    #[actix_web::test]
    async fn test_list_domains() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(
                    web::scope("/gov")
                        .service(create_domain)
                        .service(list_domains),
                ),
        )
        .await;

        // Create two domains
        for (id, name) in [("coop:food", "Food Coop"), ("coop:tech", "Tech Coop")] {
            let req_body = CreateDomainRequest {
                id: id.to_string(),
                name: name.to_string(),
                profile: "cooperative".to_string(),
                quorum_percent: 50,
                approval_percent: 66,
                voting_period_days: 7,
                members: vec![alice.did().to_string()],
            };

            let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
            let req = test::TestRequest::post()
                .uri("/gov/domains")
                .set_json(&req_body)
                .to_request();
            req.extensions_mut().insert(claims);
            test::call_service(&app, req).await;
        }

        // List all domains
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get().uri("/gov/domains").to_request();
        req.extensions_mut().insert(claims);

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let domains: Vec<GovernanceDomain> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(domains.len(), 2);
        assert_eq!(resp["pagination"]["total"], 2);
    }

    #[actix_web::test]
    async fn test_create_proposal_text() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        // Create domain first
        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_proposal)),
        )
        .await;

        // Create text proposal
        let req_body = CreateProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Approve new supplier".to_string(),
            description: "We should partner with Local Farms Inc.".to_string(),
            payload: ProposalPayloadRequest::Text {
                body: "Detailed proposal text...".to_string(),
            },
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Approve new supplier");
        assert!(matches!(resp.state, icn_governance::ProposalState::Draft));
    }

    #[actix_web::test]
    async fn test_proposal_lifecycle() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service =
            Arc::new(crate::notifications::NotificationService::new(store, None));
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        // Create domain
        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone(), bob.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .app_data(web::Data::new(notification_service.clone()))
                .service(
                    web::scope("/gov")
                        .service(create_proposal)
                        .service(open_proposal)
                        .service(cast_vote)
                        .service(close_proposal)
                        .service(get_proposal),
                ),
        )
        .await;

        // 1. Create proposal
        let req_body = CreateProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Test Proposal".to_string(),
            description: "Testing lifecycle".to_string(),
            payload: ProposalPayloadRequest::Text {
                body: "Test".to_string(),
            },
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let proposal: Proposal = test::call_and_read_body_json(&app, req).await;
        let proposal_id = proposal.id.0.clone();

        // 2. Open proposal
        let req_body = OpenProposalRequest {
            voting_period_seconds: Some(86400),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri(&format!("/gov/proposals/{proposal_id}/open"))
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // 3. Cast votes
        for (voter, choice) in [(alice.did(), "for"), (bob.did(), "against")] {
            let req_body = CastVoteRequest {
                choice: choice.to_string(),
                comment: None,
            };

            let claims = create_test_claims(&voter.to_string(), vec!["gov:write"]);
            let req = test::TestRequest::post()
                .uri(&format!("/gov/proposals/{proposal_id}/vote"))
                .set_json(&req_body)
                .to_request();
            req.extensions_mut().insert(claims);

            let resp = test::call_service(&app, req).await;
            assert!(resp.status().is_success());
        }

        // 4. Close proposal
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri(&format!("/gov/proposals/{proposal_id}/close"))
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // 5. Verify final state (should be rejected: 1 for, 1 against, simple majority fails)
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri(&format!("/gov/proposals/{proposal_id}"))
            .to_request();
        req.extensions_mut().insert(claims);

        let final_proposal: Proposal = test::call_and_read_body_json(&app, req).await;
        assert!(matches!(
            final_proposal.state,
            icn_governance::ProposalState::Rejected { .. }
        ));
    }

    #[actix_web::test]
    async fn test_list_proposals_filtering() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        // Create two domains
        for domain_id in ["coop:food", "coop:tech"] {
            gov_mgr
                .create_domain(
                    GovernanceDomainId(domain_id.to_string()),
                    format!("{domain_id} Coop"),
                    "cooperative".to_string(),
                    GovernanceParams::new(50, 66, 7 * 86400),
                    MembershipConfig::static_list(vec![alice.did().clone()]),
                )
                .await
                .unwrap();
        }

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(
                    web::scope("/gov")
                        .service(create_proposal)
                        .service(list_proposals),
                ),
        )
        .await;

        // Create proposals in different domains
        for (domain_id, title) in [
            ("coop:food", "Proposal 1"),
            ("coop:food", "Proposal 2"),
            ("coop:tech", "Proposal 3"),
        ] {
            let req_body = CreateProposalRequest {
                domain_id: domain_id.to_string(),
                title: title.to_string(),
                description: "Test".to_string(),
                payload: ProposalPayloadRequest::Text {
                    body: "Test".to_string(),
                },
            };

            let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
            let req = test::TestRequest::post()
                .uri("/gov/proposals")
                .set_json(&req_body)
                .to_request();
            req.extensions_mut().insert(claims);
            test::call_service(&app, req).await;
        }

        // List all proposals
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get().uri("/gov/proposals").to_request();
        req.extensions_mut().insert(claims);

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let all_proposals: Vec<Proposal> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(all_proposals.len(), 3);
        assert_eq!(resp["pagination"]["total"], 3);

        // Filter by domain
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/proposals?domain_id=coop:food")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let food_proposals: Vec<Proposal> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(food_proposals.len(), 2);
        assert_eq!(resp["pagination"]["total"], 2);

        // Filter by state
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/proposals?state=draft")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let draft_proposals: Vec<Proposal> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(draft_proposals.len(), 3);
        assert_eq!(resp["pagination"]["total"], 3);
    }

    #[actix_web::test]
    async fn test_list_domains_cursor_pagination() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();

        // Create 5 domains
        for i in 0..5 {
            gov_mgr
                .create_domain(
                    GovernanceDomainId(format!("coop:domain{i}")),
                    format!("Domain {i}"),
                    "cooperative".to_string(),
                    GovernanceParams::new(50, 66, 7 * 86400),
                    MembershipConfig::static_list(vec![alice.did().clone()]),
                )
                .await
                .unwrap();
        }

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .service(web::scope("/gov").service(list_domains)),
        )
        .await;

        // First page: limit=2
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/domains?limit=2")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let page1: Vec<serde_json::Value> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(resp["pagination"]["has_more"], true);
        assert_eq!(resp["pagination"]["total"], 5);
        let cursor = resp["pagination"]["cursor"].as_str().unwrap();
        assert_eq!(cursor, "offset:2");

        // Second page using cursor
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri(&format!("/gov/domains?limit=2&cursor={cursor}"))
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let page2: Vec<serde_json::Value> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(resp["pagination"]["has_more"], true);
        let cursor = resp["pagination"]["cursor"].as_str().unwrap();
        assert_eq!(cursor, "offset:4");

        // Third page (last, only 1 item)
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri(&format!("/gov/domains?limit=2&cursor={cursor}"))
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let page3: Vec<serde_json::Value> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(page3.len(), 1);
        assert_eq!(resp["pagination"]["has_more"], false);
        assert!(resp["pagination"]["cursor"].is_null());

        // Verify no duplicates across pages
        let all_names: Vec<String> = page1
            .iter()
            .chain(page2.iter())
            .chain(page3.iter())
            .map(|d| d["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(all_names.len(), 5);
        // Should all be unique
        let unique: std::collections::HashSet<_> = all_names.iter().collect();
        assert_eq!(unique.len(), 5);
    }

    #[actix_web::test]
    async fn test_authorization_gov_read_scope() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .service(web::scope("/gov").service(list_domains)),
        )
        .await;

        // Try without gov:read scope (should fail)
        let claims = create_test_claims(&alice.did().to_string(), vec!["ledger:read"]);
        let req = test::TestRequest::get().uri("/gov/domains").to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403); // Forbidden
    }

    #[actix_web::test]
    async fn test_authorization_gov_write_scope() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_domain)),
        )
        .await;

        let req_body = CreateDomainRequest {
            id: "coop:food".to_string(),
            name: "Food Coop".to_string(),
            profile: "cooperative".to_string(),
            quorum_percent: 50,
            approval_percent: 66,
            voting_period_days: 7,
            members: vec![alice.did().to_string()],
        };

        // Try without gov:write scope (should fail)
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::post()
            .uri("/gov/domains")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403); // Forbidden
    }

    #[actix_web::test]
    async fn test_get_nonexistent_domain() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .service(web::scope("/gov").service(get_domain)),
        )
        .await;

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/domains/nonexistent")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404); // Not Found
    }

    #[actix_web::test]
    async fn test_open_nonexistent_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(open_proposal)),
        )
        .await;

        let req_body = OpenProposalRequest {
            voting_period_seconds: Some(86400),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/nonexistent/open")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404); // Not Found (proper error code for nonexistent proposal)
    }

    #[actix_web::test]
    async fn test_proposal_no_quorum_outcome() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();
        let carol = IdentityBundle::generate().unwrap();
        let dave = IdentityBundle::generate().unwrap();
        let eve = IdentityBundle::generate().unwrap();

        let domain_id = GovernanceDomainId("coop:test".to_string());

        // Create domain with 5 members and 80% quorum requirement
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(
                    80,    // quorum_percentage - Need 4 out of 5 members to vote
                    66,    // approval_threshold_percentage
                    86400, // voting_period_seconds
                ),
                MembershipConfig {
                    source: MembershipSource::StaticList(vec![
                        alice.did().clone(),
                        bob.did().clone(),
                        carol.did().clone(),
                        dave.did().clone(),
                        eve.did().clone(),
                    ]),
                },
            )
            .await
            .unwrap();

        // Create proposal
        let proposal_id = ProposalId("prop-123".to_string());
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id.clone(),
                alice.did().clone(),
                "Test Proposal".to_string(),
                "A proposal to test quorum".to_string(),
                ProposalPayload::Text {
                    body: "Should fail due to insufficient quorum".to_string(),
                },
            )
            .await
            .unwrap();

        // Open the proposal
        gov_mgr
            .open_proposal(proposal_id.clone(), 86400)
            .await
            .unwrap();

        // Only 3 out of 5 members vote (60% participation, below 80% quorum)
        gov_mgr
            .cast_vote(
                proposal_id.clone(),
                alice.did().clone(),
                VoteChoice::For,
                None,
            )
            .await
            .unwrap();

        gov_mgr
            .cast_vote(
                proposal_id.clone(),
                bob.did().clone(),
                VoteChoice::For,
                None,
            )
            .await
            .unwrap();

        gov_mgr
            .cast_vote(
                proposal_id.clone(),
                carol.did().clone(),
                VoteChoice::Against,
                None,
            )
            .await
            .unwrap();

        // Close the proposal
        gov_mgr.close_proposal(proposal_id.clone()).await.unwrap();

        // Verify outcome is NoQuorum despite approval being met (2/3 = 66%)
        let proposal = gov_mgr.get_proposal(&proposal_id).await.unwrap().unwrap();
        match proposal.state {
            ProposalState::NoQuorum { .. } => {
                // Success - quorum not met
            }
            other => {
                panic!("Expected NoQuorum state, got {other:?}");
            }
        }
    }

    #[actix_web::test]
    async fn test_duplicate_vote_prevention() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("coop:test".to_string());

        // Create domain with Alice as member
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        // Create and open proposal
        let proposal_id = ProposalId("prop-123".to_string());
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id,
                alice.did().clone(),
                "Test".to_string(),
                "Test".to_string(),
                ProposalPayload::Text {
                    body: "Test".to_string(),
                },
            )
            .await
            .unwrap();
        gov_mgr
            .open_proposal(proposal_id.clone(), 86400)
            .await
            .unwrap();

        // First vote succeeds
        gov_mgr
            .cast_vote(
                proposal_id.clone(),
                alice.did().clone(),
                VoteChoice::For,
                None,
            )
            .await
            .unwrap();

        // Second vote from same DID should fail
        let result = gov_mgr
            .cast_vote(
                proposal_id.clone(),
                alice.did().clone(),
                VoteChoice::Against,
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already voted"));
    }

    #[actix_web::test]
    async fn test_vote_on_draft_proposal_fails() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("coop:test".to_string());

        // Create domain and proposal (but don't open it)
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let proposal_id = ProposalId("prop-123".to_string());
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id,
                alice.did().clone(),
                "Test".to_string(),
                "Test".to_string(),
                ProposalPayload::Text {
                    body: "Test".to_string(),
                },
            )
            .await
            .unwrap();

        // Try to vote on Draft proposal
        let result = gov_mgr
            .cast_vote(proposal_id, alice.did().clone(), VoteChoice::For, None)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not open for voting"));
    }

    #[actix_web::test]
    async fn test_vote_on_closed_proposal_fails() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("coop:test".to_string());

        // Create domain, proposal, open, and close it
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let proposal_id = ProposalId("prop-123".to_string());
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id,
                alice.did().clone(),
                "Test".to_string(),
                "Test".to_string(),
                ProposalPayload::Text {
                    body: "Test".to_string(),
                },
            )
            .await
            .unwrap();
        gov_mgr
            .open_proposal(proposal_id.clone(), 86400)
            .await
            .unwrap();
        gov_mgr.close_proposal(proposal_id.clone()).await.unwrap();

        // Try to vote on closed proposal
        let result = gov_mgr
            .cast_vote(proposal_id, alice.did().clone(), VoteChoice::For, None)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not open for voting"));
    }

    #[actix_web::test]
    async fn test_non_member_vote_fails() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("coop:test".to_string());

        // Create domain with only Alice as member
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        // Create and open proposal
        let proposal_id = ProposalId("prop-123".to_string());
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id,
                alice.did().clone(),
                "Test".to_string(),
                "Test".to_string(),
                ProposalPayload::Text {
                    body: "Test".to_string(),
                },
            )
            .await
            .unwrap();
        gov_mgr
            .open_proposal(proposal_id.clone(), 86400)
            .await
            .unwrap();

        // Try to vote as Bob (non-member)
        let result = gov_mgr
            .cast_vote(proposal_id, bob.did().clone(), VoteChoice::For, None)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a member"));
    }

    #[actix_web::test]
    async fn test_create_proposal_for_nonexistent_domain_fails() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("nonexistent".to_string());

        // Try to create proposal for non-existent domain
        let result = gov_mgr
            .create_proposal(
                ProposalId("prop-123".to_string()),
                domain_id,
                alice.did().clone(),
                "Test".to_string(),
                "Test".to_string(),
                ProposalPayload::Text {
                    body: "Test".to_string(),
                },
            )
            .await;

        assert!(result.is_err());
        // Check for the improved error message that includes domain ID and action suggestion
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not found"),
            "Expected 'not found' in error message but got: {err_msg}",
        );
    }

    #[actix_web::test]
    async fn test_toctou_vote_close_race_condition() {
        // This test verifies the TOCTOU (Time-of-Check-Time-of-Use) race condition fix
        // in cast_vote(). The fix re-checks the proposal state after acquiring the votes
        // lock to ensure the proposal is still open.

        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("coop:test".to_string());

        // Create domain with two members
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone(), bob.did().clone()]),
            )
            .await
            .unwrap();

        // Create and open proposal
        let proposal_id = ProposalId("prop-123".to_string());
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id,
                alice.did().clone(),
                "Test".to_string(),
                "Test".to_string(),
                ProposalPayload::Text {
                    body: "Test".to_string(),
                },
            )
            .await
            .unwrap();
        gov_mgr
            .open_proposal(proposal_id.clone(), 86400)
            .await
            .unwrap();

        // Alice votes successfully
        gov_mgr
            .cast_vote(
                proposal_id.clone(),
                alice.did().clone(),
                VoteChoice::For,
                None,
            )
            .await
            .unwrap();

        // Spawn concurrent operations to trigger potential race condition
        let gov_mgr_clone = gov_mgr.clone();
        let proposal_id_clone = proposal_id.clone();
        let bob_did = bob.did().clone();

        // Use tokio::join! to run close and vote concurrently
        let (close_result, vote_result) = tokio::join!(
            // Thread 1: Close the proposal
            async move { gov_mgr_clone.close_proposal(proposal_id_clone).await },
            // Thread 2: Bob tries to vote (should fail if close happens first)
            async move {
                // Add tiny delay to increase chance that close happens first
                tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
                gov_mgr
                    .cast_vote(proposal_id.clone(), bob_did, VoteChoice::Against, None)
                    .await
            }
        );

        // Close should succeed
        assert!(close_result.is_ok(), "Close operation should succeed");

        // Vote should fail because proposal was closed
        // The TOCTOU fix ensures that even if the vote passed the initial check,
        // it will be rejected when re-checked after acquiring the votes lock
        assert!(
            vote_result.is_err(),
            "Vote should fail when proposal is closed"
        );
        let error_msg = vote_result.unwrap_err().to_string();
        assert!(
            error_msg.contains("not open for voting")
                || error_msg.contains("was closed during vote submission"),
            "Error should indicate proposal is not open, got: {error_msg}"
        );
    }

    #[actix_web::test]
    async fn test_duplicate_domain_id_prevention() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("coop:food".to_string());

        // Create first domain
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Food Coop v1".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        // Try to create domain with same ID but different name
        let result = gov_mgr
            .create_domain(
                domain_id.clone(),
                "Food Coop v2 (OVERWRITE)".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(60, 75, 172800),
                MembershipConfig::static_list(vec![]),
            )
            .await;

        // Should fail - duplicate domain ID not allowed
        assert!(result.is_err(), "Duplicate domain ID should be rejected");
        assert!(result.unwrap_err().to_string().contains("already exists"));

        // Verify original domain still exists with original name
        let domain = gov_mgr.get_domain(&domain_id).await.unwrap().unwrap();
        assert_eq!(domain.name, "Food Coop v1");
        // Verify params weren't overwritten
        assert_eq!(domain.config.params.quorum_percentage, 50);
    }

    #[actix_web::test]
    async fn test_duplicate_proposal_id_prevention() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let domain_id = GovernanceDomainId("coop:food".to_string());

        // Create domain first
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let proposal_id = ProposalId("prop-123".to_string());

        // Create first proposal
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id.clone(),
                alice.did().clone(),
                "Original Proposal".to_string(),
                "Original description".to_string(),
                ProposalPayload::Text {
                    body: "Original body".to_string(),
                },
            )
            .await
            .unwrap();

        // Try to create proposal with same ID but different content
        let result = gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id,
                alice.did().clone(),
                "Malicious Overwrite".to_string(),
                "Attempting to overwrite existing proposal".to_string(),
                ProposalPayload::Text {
                    body: "Malicious content".to_string(),
                },
            )
            .await;

        // Should fail - duplicate proposal ID not allowed
        assert!(result.is_err(), "Duplicate proposal ID should be rejected");
        assert!(result.unwrap_err().to_string().contains("already exists"));

        // Verify original proposal still exists with original content
        let proposal = gov_mgr.get_proposal(&proposal_id).await.unwrap().unwrap();
        assert_eq!(proposal.title, "Original Proposal");
        assert_eq!(proposal.description, "Original description");
    }

    #[actix_web::test]
    async fn test_voting_period_overflow_prevention() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_domain)),
        )
        .await;

        // Try to create domain with voting period that would overflow
        // MAX_VOTING_PERIOD_SECONDS / 86400 = 365 days
        let req_body = CreateDomainRequest {
            id: "coop:overflow".to_string(),
            name: "Overflow Test".to_string(),
            profile: "cooperative".to_string(),
            quorum_percent: 50,
            approval_percent: 66,
            voting_period_days: 366, // 1 day over the limit
            members: vec![alice.did().to_string()],
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/domains")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request

        // Try with zero days
        let req_body_zero = CreateDomainRequest {
            id: "coop:zero".to_string(),
            name: "Zero Period Test".to_string(),
            profile: "cooperative".to_string(),
            quorum_percent: 50,
            approval_percent: 66,
            voting_period_days: 0,
            members: vec![alice.did().to_string()],
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/domains")
            .set_json(&req_body_zero)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request

        // Valid voting period (365 days exactly) should work
        let req_body_valid = CreateDomainRequest {
            id: "coop:valid".to_string(),
            name: "Valid Period Test".to_string(),
            profile: "cooperative".to_string(),
            quorum_percent: 50,
            approval_percent: 66,
            voting_period_days: 365, // Exactly 1 year
            members: vec![alice.did().to_string()],
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/domains")
            .set_json(&req_body_valid)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201); // Created
    }

    #[actix_web::test]
    async fn test_non_member_cannot_open_close_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap(); // Not a member
        let domain_id = GovernanceDomainId("coop:test".to_string());

        // Create domain with only Alice as member
        gov_mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        // Alice creates proposal (allowed - she's a member)
        let proposal_id = ProposalId("prop-123".to_string());
        gov_mgr
            .create_proposal(
                proposal_id.clone(),
                domain_id,
                alice.did().clone(),
                "Test Proposal".to_string(),
                "Test".to_string(),
                ProposalPayload::Text {
                    body: "Test".to_string(),
                },
            )
            .await
            .unwrap();

        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service =
            Arc::new(crate::notifications::NotificationService::new(store, None));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .app_data(web::Data::new(notification_service))
                .service(
                    web::scope("/gov")
                        .service(open_proposal)
                        .service(close_proposal),
                ),
        )
        .await;

        // Bob (non-member) tries to open proposal
        let req_body = OpenProposalRequest {
            voting_period_seconds: Some(86400),
        };
        let claims = create_test_claims(&bob.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri(&format!("/gov/proposals/{}/open", proposal_id.0))
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403); // Forbidden - not a member

        // Alice (member) opens proposal successfully
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri(&format!("/gov/proposals/{}/open", proposal_id.0))
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200); // Success

        // Bob (non-member) tries to close proposal
        let claims = create_test_claims(&bob.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri(&format!("/gov/proposals/{}/close", proposal_id.0))
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403); // Forbidden - not a member

        // Alice (member) can close proposal successfully
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri(&format!("/gov/proposals/{}/close", proposal_id.0))
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200); // Success
    }

    // ============================================================================
    // Federation Proposal Tests
    // ============================================================================

    #[actix_web::test]
    async fn test_create_join_federation_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        // Create domain first
        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_join_federation_proposal)),
        )
        .await;

        // Create join federation proposal
        let req_body = JoinFederationProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Join Regional Food Federation".to_string(),
            description: "Proposal to join the regional food cooperative federation".to_string(),
            federation_id: "regional-food-fed".to_string(),
            terms: crate::models::FederationTermsRequest {
                min_trust_threshold: 0.6,
                governance_binding: true,
                data_sharing_level: "metadata_only".to_string(),
                dispute_resolution: "federation_mediation".to_string(),
            },
            sponsor_coop_id: Some("organic-farms-coop".to_string()),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/join")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Join Regional Food Federation");
        assert!(matches!(resp.state, ProposalState::Draft));
        assert!(matches!(
            resp.payload,
            icn_governance::ProposalPayload::Federation(
                icn_governance::FederationProposal::JoinFederation { .. }
            )
        ));
    }

    #[actix_web::test]
    async fn test_create_leave_federation_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_leave_federation_proposal)),
        )
        .await;

        let req_body = LeaveFederationProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Leave Regional Federation".to_string(),
            description: "Strategic realignment requires leaving federation".to_string(),
            federation_id: "regional-food-fed".to_string(),
            reason: "Strategic realignment".to_string(),
            grace_period_days: 30,
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/leave")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Leave Regional Federation");
        assert!(matches!(
            resp.payload,
            icn_governance::ProposalPayload::Federation(
                icn_governance::FederationProposal::LeaveFederation { .. }
            )
        ));
    }

    #[actix_web::test]
    async fn test_create_establish_clearing_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let partner = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_establish_clearing_proposal)),
        )
        .await;

        let req_body = EstablishClearingProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Establish Clearing with Partner Coop".to_string(),
            description: "Setup bilateral clearing for cross-coop transactions".to_string(),
            partner_coop_id: "partner-coop".to_string(),
            partner_coop_did: partner.did().to_string(),
            max_imbalance: 10000,
            settlement_interval: "weekly".to_string(),
            currency: "HOURS".to_string(),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/clearing")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Establish Clearing with Partner Coop");
        assert!(matches!(
            resp.payload,
            icn_governance::ProposalPayload::Federation(
                icn_governance::FederationProposal::EstablishClearing { .. }
            )
        ));
    }

    #[actix_web::test]
    async fn test_create_terminate_clearing_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_terminate_clearing_proposal)),
        )
        .await;

        let req_body = TerminateClearingProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Terminate Clearing Agreement".to_string(),
            description: "End the clearing agreement with partner coop".to_string(),
            partner_coop_id: "partner-coop".to_string(),
            reason: "Partnership no longer beneficial".to_string(),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/clearing/terminate")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Terminate Clearing Agreement");
        assert!(matches!(
            resp.payload,
            icn_governance::ProposalPayload::Federation(
                icn_governance::FederationProposal::TerminateClearing { .. }
            )
        ));
    }

    #[actix_web::test]
    async fn test_create_vouch_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let target = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_vouch_proposal)),
        )
        .await;

        let req_body = VouchProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Vouch for New Worker Coop".to_string(),
            description: "Recommend this cooperative for federation membership".to_string(),
            target_coop_id: "new-worker-coop".to_string(),
            target_coop_did: target.did().to_string(),
            trust_score: 0.75,
            context: "trade".to_string(),
            evidence: Some("Six months of successful trading".to_string()),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/vouch")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Vouch for New Worker Coop");
        assert!(matches!(
            resp.payload,
            icn_governance::ProposalPayload::Federation(
                icn_governance::FederationProposal::VouchForCooperative { .. }
            )
        ));
    }

    #[actix_web::test]
    async fn test_create_revoke_vouch_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_revoke_vouch_proposal)),
        )
        .await;

        let req_body = RevokeVouchProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Revoke Vouch for Problem Coop".to_string(),
            description: "Remove our endorsement due to trust violations".to_string(),
            target_coop_id: "problem-coop".to_string(),
            reason: "Repeated contract violations".to_string(),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/vouch/revoke")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Revoke Vouch for Problem Coop");
        assert!(matches!(
            resp.payload,
            icn_governance::ProposalPayload::Federation(
                icn_governance::FederationProposal::RevokeVouch { .. }
            )
        ));
    }

    #[actix_web::test]
    async fn test_create_update_federation_policy_proposal() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_update_federation_policy_proposal)),
        )
        .await;

        let req_body = UpdateFederationPolicyProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Update Federation Policy".to_string(),
            description: "Adjust auto-accept threshold and rate limits".to_string(),
            auto_accept_vouch_threshold: Some(0.7),
            trust_decay_factor: Some(0.05),
            max_attestations_per_minute: Some(30),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/policy")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Update Federation Policy");
        assert!(matches!(
            resp.payload,
            icn_governance::ProposalPayload::Federation(
                icn_governance::FederationProposal::UpdateFederationPolicy { .. }
            )
        ));
    }

    #[actix_web::test]
    async fn test_federation_proposal_invalid_trust_score() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_join_federation_proposal)),
        )
        .await;

        // Trust score too high (> 1.0) - use raw JSON to test deserialization rejection
        let invalid_json = serde_json::json!({
            "domain_id": "coop:food",
            "title": "Join Federation",
            "description": "Test proposal",
            "federation_id": "test-fed",
            "terms": {
                "min_trust_threshold": 1.5,  // Invalid - too high
                "governance_binding": true,
                "data_sharing_level": "metadata_only",
                "dispute_resolution": "federation_mediation"
            },
            "sponsor_coop_id": null
        });

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/join")
            .set_json(&invalid_json)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request - trust score validation rejects 1.5
    }

    #[actix_web::test]
    async fn test_federation_proposal_invalid_grace_period() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_leave_federation_proposal)),
        )
        .await;

        // Grace period too long (> 365)
        let req_body = LeaveFederationProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Leave Federation".to_string(),
            description: "Test proposal".to_string(),
            federation_id: "test-fed".to_string(),
            reason: "Test reason".to_string(),
            grace_period_days: 500, // Invalid - too long
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/leave")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request
    }

    #[actix_web::test]
    async fn test_federation_proposal_invalid_settlement_interval() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let partner = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_establish_clearing_proposal)),
        )
        .await;

        let req_body = EstablishClearingProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Establish Clearing".to_string(),
            description: "Test proposal".to_string(),
            partner_coop_id: "partner-coop".to_string(),
            partner_coop_did: partner.did().to_string(),
            max_imbalance: 10000,
            settlement_interval: "yearly".to_string(), // Invalid
            currency: "HOURS".to_string(),
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/clearing")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request
    }

    #[actix_web::test]
    async fn test_federation_proposal_requires_auth() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_join_federation_proposal)),
        )
        .await;

        let req_body = JoinFederationProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Join Federation".to_string(),
            description: "Test proposal".to_string(),
            federation_id: "test-fed".to_string(),
            terms: crate::models::FederationTermsRequest {
                min_trust_threshold: 0.5,
                governance_binding: true,
                data_sharing_level: "metadata_only".to_string(),
                dispute_resolution: "federation_mediation".to_string(),
            },
            sponsor_coop_id: None,
        };

        // Request without claims (no auth)
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/join")
            .set_json(&req_body)
            .to_request();
        // Note: no claims inserted

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401); // Unauthorized - no auth token
    }

    #[actix_web::test]
    async fn test_federation_proposal_requires_write_scope() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_join_federation_proposal)),
        )
        .await;

        let req_body = JoinFederationProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Join Federation".to_string(),
            description: "Test proposal".to_string(),
            federation_id: "test-fed".to_string(),
            terms: crate::models::FederationTermsRequest {
                min_trust_threshold: 0.5,
                governance_binding: true,
                data_sharing_level: "metadata_only".to_string(),
                dispute_resolution: "federation_mediation".to_string(),
            },
            sponsor_coop_id: None,
        };

        // Request with only read scope (not write)
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/join")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403); // Forbidden - wrong scope
    }

    #[actix_web::test]
    async fn test_update_policy_requires_at_least_one_field() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_update_federation_policy_proposal)),
        )
        .await;

        // No policy fields provided
        let req_body = UpdateFederationPolicyProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Update Policy".to_string(),
            description: "Test proposal".to_string(),
            auto_accept_vouch_threshold: None,
            trust_decay_factor: None,
            max_attestations_per_minute: None,
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/policy")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request - no fields provided
    }

    #[actix_web::test]
    async fn test_federation_proposal_arbitrator_dispute_resolution() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/gov").service(create_join_federation_proposal)),
        )
        .await;

        // Test arbitrator dispute resolution format
        let req_body = JoinFederationProposalRequest {
            domain_id: "coop:food".to_string(),
            title: "Join Federation with Arbitrator".to_string(),
            description: "Using arbitrator for dispute resolution".to_string(),
            federation_id: "test-fed".to_string(),
            terms: crate::models::FederationTermsRequest {
                min_trust_threshold: 0.5,
                governance_binding: true,
                data_sharing_level: "full".to_string(),
                dispute_resolution: "arbitrator:neutral-coop".to_string(),
            },
            sponsor_coop_id: None,
        };

        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:write"]);
        let req = test::TestRequest::post()
            .uri("/gov/proposals/federation/join")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp: Proposal = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.title, "Join Federation with Arbitrator");
    }

    #[actix_web::test]
    async fn test_list_proposals_scope_filter() {
        let gov_mgr = Arc::new(GovernanceManager::new());
        let alice = IdentityBundle::generate().unwrap();

        // Create a domain
        gov_mgr
            .create_domain(
                GovernanceDomainId("coop:food".to_string()),
                "Food Coop".to_string(),
                "cooperative".to_string(),
                GovernanceParams::new(50, 66, 7 * 86400),
                MembershipConfig::static_list(vec![alice.did().clone()]),
            )
            .await
            .unwrap();

        // Insert proposals with different scopes using the test helper
        let local_proposal = Proposal::new(
            GovernanceDomainId("coop:food".to_string()),
            alice.did().clone(),
            "Local Proposal".to_string(),
            "A local proposal".to_string(),
            icn_governance::ProposalPayload::Text {
                body: "local stuff".to_string(),
            },
        );
        gov_mgr.insert_test_proposal(local_proposal);

        let fed_proposal = Proposal::new(
            GovernanceDomainId("coop:food".to_string()),
            alice.did().clone(),
            "Federation Proposal".to_string(),
            "A federation proposal".to_string(),
            icn_governance::ProposalPayload::Text {
                body: "federation stuff".to_string(),
            },
        )
        .with_scope(icn_governance::ProposalScope::Federation(
            "my-fed".to_string(),
        ));
        gov_mgr.insert_test_proposal(fed_proposal);

        let fed_proposal_2 = Proposal::new(
            GovernanceDomainId("coop:food".to_string()),
            alice.did().clone(),
            "Other Fed Proposal".to_string(),
            "Another fed proposal".to_string(),
            icn_governance::ProposalPayload::Text {
                body: "other fed".to_string(),
            },
        )
        .with_scope(icn_governance::ProposalScope::Federation(
            "other-fed".to_string(),
        ));
        gov_mgr.insert_test_proposal(fed_proposal_2);

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(gov_mgr.clone()))
                .service(web::scope("/gov").service(list_proposals)),
        )
        .await;

        // Filter scope=local => 1 result
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/proposals?scope=local")
            .to_request();
        req.extensions_mut().insert(claims);
        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let proposals: Vec<Proposal> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].title, "Local Proposal");

        // Filter scope=federation => 2 results
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/proposals?scope=federation")
            .to_request();
        req.extensions_mut().insert(claims);
        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let proposals: Vec<Proposal> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(proposals.len(), 2);

        // Filter scope=my-fed => 1 result (specific federation ID)
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get()
            .uri("/gov/proposals?scope=my-fed")
            .to_request();
        req.extensions_mut().insert(claims);
        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let proposals: Vec<Proposal> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].title, "Federation Proposal");

        // No scope filter => all 3
        let claims = create_test_claims(&alice.did().to_string(), vec!["gov:read"]);
        let req = test::TestRequest::get().uri("/gov/proposals").to_request();
        req.extensions_mut().insert(claims);
        let resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let proposals: Vec<Proposal> = serde_json::from_value(resp["data"].clone()).unwrap();
        assert_eq!(proposals.len(), 3);
    }
}
