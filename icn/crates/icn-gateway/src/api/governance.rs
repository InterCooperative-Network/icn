//! Governance API endpoints
//!
//! RESTful API for managing governance domains, proposals, and votes.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::Result;
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::governance_mgr::GovernanceManager;
use crate::middleware::{get_claims, require_scope};
use crate::models::{
    CastVoteRequest, CreateDomainRequest, CreateProposalRequest, OpenProposalRequest,
    ProposalPayloadRequest,
};
use crate::validation;
use icn_governance::{
    GovernanceDomainId, GovernanceParams, MembershipConfig, ProposalId, ProposalPayload, VoteChoice,
};
use icn_identity::Did;
use icn_obs::metrics::gateway;

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
#[get("/domains")]
pub async fn list_domains(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let mut domains = gov_mgr.list_domains().await?;

    // Apply pagination
    let limit = if let Some(limit_str) = query.get("limit") {
        let limit: usize = limit_str.parse().map_err(|_| {
            crate::error::GatewayError::BadRequest("Invalid limit parameter".to_string())
        })?;
        validation::validate_history_limit(limit)?
    } else {
        validation::DEFAULT_HISTORY_LIMIT
    };

    let offset = if let Some(offset_str) = query.get("offset") {
        let offset: usize = offset_str.parse().map_err(|_| {
            crate::error::GatewayError::BadRequest("Invalid offset parameter".to_string())
        })?;
        validation::validate_history_offset(offset)?
    } else {
        0
    };

    // Sort by name for consistent pagination
    domains.sort_by(|a, b| a.name.cmp(&b.name));

    // Apply pagination slice
    let total = domains.len();
    let paginated: Vec<_> = domains.into_iter().skip(offset).take(limit).collect();

    // Return with pagination metadata
    let response = serde_json::json!({
        "data": paginated,
        "pagination": {
            "total": total,
            "offset": offset,
            "limit": limit,
            "returned": paginated.len(),
        }
    });

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

    // Create proposal via governance actor
    let domain_id = GovernanceDomainId(req.domain_id.clone());
    let proposal_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));

    gov_mgr
        .create_proposal(
            proposal_id.clone(),
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
#[get("/proposals")]
pub async fn list_proposals(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let mut proposals = gov_mgr.list_proposals().await?;

    // Filter by domain if requested
    if let Some(domain_id) = query.get("domain_id") {
        let filter_domain = GovernanceDomainId(domain_id.clone());
        proposals.retain(|p| p.domain_id == filter_domain);
    }

    // Filter by state if requested
    if let Some(state) = query.get("state") {
        match state.as_str() {
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
                    "Invalid state filter: {state}"
                )))
            }
        }
    }

    // Apply pagination
    let limit = if let Some(limit_str) = query.get("limit") {
        let limit: usize = limit_str.parse().map_err(|_| {
            crate::error::GatewayError::BadRequest("Invalid limit parameter".to_string())
        })?;
        validation::validate_history_limit(limit)?
    } else {
        validation::DEFAULT_HISTORY_LIMIT
    };

    let offset = if let Some(offset_str) = query.get("offset") {
        let offset: usize = offset_str.parse().map_err(|_| {
            crate::error::GatewayError::BadRequest("Invalid offset parameter".to_string())
        })?;
        validation::validate_history_offset(offset)?
    } else {
        0
    };

    // Sort by creation time (newest first) for consistent pagination
    proposals.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Apply pagination slice
    let total = proposals.len();
    let paginated: Vec<_> = proposals.into_iter().skip(offset).take(limit).collect();

    // Return with pagination metadata
    let response = serde_json::json!({
        "data": paginated,
        "pagination": {
            "total": total,
            "offset": offset,
            "limit": limit,
            "returned": paginated.len(),
        }
    });

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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| {
                crate::error::GatewayError::InternalError(format!("System clock error: {e}"))
            })?
            .as_secs();
        now + voting_period_seconds
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
        let notification_service = Arc::new(crate::notifications::NotificationService::new(None));
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
                GovernanceParams {
                    quorum_percentage: 80, // Need 4 out of 5 members to vote
                    approval_threshold_percentage: 66,
                    voting_period_seconds: 86400,
                },
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
        assert!(result.unwrap_err().to_string().contains("Domain not found"));
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

        let notification_service = Arc::new(crate::notifications::NotificationService::new(None));
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
}
