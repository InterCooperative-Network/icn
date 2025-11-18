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
    GovernanceDomainId, GovernanceParams, MembershipConfig, ProposalId, ProposalPayload,
    VoteChoice,
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
    let claims = get_claims(&http_req)
        .ok_or_else(|| crate::error::GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let creator_did: Did = claims.sub.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Validate inputs
    validation::validate_domain_id(&req.id)?;
    validation::validate_domain_name(&req.name)?;

    // Convert member DIDs
    let members: Result<Vec<Did>> = req.members.iter()
        .map(|s| s.parse().map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid member DID: {e}"))))
        .collect();
    let members = members?;

    // Build membership config
    let membership = MembershipConfig::static_list(members);

    // Build governance params
    let params = GovernanceParams::new(
        req.quorum_percent,
        req.approval_percent,
        req.voting_period_days * 86400, // days -> seconds
    );

    // Create domain via governance manager
    let domain_id = GovernanceDomainId(req.id.clone());
    gov_mgr.create_domain(
        domain_id.clone(),
        req.name.clone(),
        req.profile.clone(),
        params,
        membership,
    ).await?;

    // Track domain creation
    gateway::governance_domains_created_inc();

    // Broadcast event to WebSocket subscribers
    event_broadcaster.broadcast(
        &domain_id.0,
        GatewayEvent::GovernanceDomainCreated {
            domain_id: domain_id.0.clone(),
            name: req.name.clone(),
            creator: creator_did.to_string(),
        },
    ).await;

    // Return created domain
    let domain = gov_mgr.get_domain(&domain_id).await?
        .ok_or_else(|| crate::error::GatewayError::InternalError("Domain creation succeeded but domain not found".to_string()))?;

    Ok(HttpResponse::Created().json(domain))
}

/// GET /gov/domains - List all governance domains
#[get("/domains")]
pub async fn list_domains(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:read")?;

    let domains = gov_mgr.list_domains().await?;
    Ok(HttpResponse::Ok().json(domains))
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
    let domain = gov_mgr.get_domain(&domain_id).await?
        .ok_or_else(|| crate::error::GatewayError::NotFound(format!("Domain not found: {}", domain_id.0)))?;

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
    let claims = get_claims(&http_req)
        .ok_or_else(|| crate::error::GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let proposer_did: Did = claims.sub.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Validate inputs
    validation::validate_domain_id(&req.domain_id)?;
    if req.title.is_empty() || req.title.len() > 200 {
        return Err(crate::error::GatewayError::BadRequest("Title must be 1-200 characters".to_string()));
    }
    if req.description.len() > 5000 {
        return Err(crate::error::GatewayError::BadRequest("Description must be ≤5000 characters".to_string()));
    }

    // Convert payload
    let payload = match &req.payload {
        ProposalPayloadRequest::Text { body } => ProposalPayload::Text { body: body.clone() },
        ProposalPayloadRequest::Budget { amount, recipient, currency, purpose } => {
            let recipient_did: Did = recipient.parse()
                .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid recipient DID: {e}")))?;
            ProposalPayload::Budget {
                amount: *amount,
                recipient: recipient_did,
                currency: currency.clone(),
                purpose: purpose.clone(),
            }
        },
        ProposalPayloadRequest::Membership { action, did } => {
            let member_did: Did = did.parse()
                .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid member DID: {e}")))?;

            // Parse action string to MembershipAction
            use icn_governance::MembershipAction;
            let membership_action = match action.to_lowercase().as_str() {
                "add" => MembershipAction::Add,
                "remove" => MembershipAction::Remove,
                _ => return Err(crate::error::GatewayError::BadRequest(format!("Invalid action: {action}"))),
            };

            ProposalPayload::Membership {
                action: membership_action,
                member: member_did,
            }
        },
        ProposalPayloadRequest::ConfigChange { key, value } => {
            // Combine key-value into JSON config
            let new_config = serde_json::json!({ key: value }).to_string();
            ProposalPayload::ConfigChange {
                new_config,
            }
        },
    };

    // Create proposal via governance actor
    let domain_id = GovernanceDomainId(req.domain_id.clone());
    let proposal_id = ProposalId(format!("prop-{}", uuid::Uuid::new_v4()));

    gov_mgr.create_proposal(
        proposal_id.clone(),
        domain_id,
        proposer_did.clone(),
        req.title.clone(),
        req.description.clone(),
        payload,
    ).await?;

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
    event_broadcaster.broadcast(
        &req.domain_id,
        GatewayEvent::GovernanceProposalCreated {
            proposal_id: proposal_id.0.clone(),
            domain_id: req.domain_id.clone(),
            proposer: proposer_did.to_string(),
            title: req.title.clone(),
            payload_type: payload_type.to_string(),
        },
    ).await;

    // Return created proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?
        .ok_or_else(|| crate::error::GatewayError::InternalError("Proposal creation succeeded but proposal not found".to_string()))?;

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
            "draft" => proposals.retain(|p| matches!(p.state, icn_governance::ProposalState::Draft)),
            "open" => proposals.retain(|p| matches!(p.state, icn_governance::ProposalState::Open { .. })),
            "closed" => proposals.retain(|p| matches!(
                p.state,
                icn_governance::ProposalState::Accepted { .. }
                | icn_governance::ProposalState::Rejected { .. }
                | icn_governance::ProposalState::NoQuorum { .. }
            )),
            _ => return Err(crate::error::GatewayError::BadRequest(format!("Invalid state filter: {state}"))),
        }
    }

    Ok(HttpResponse::Ok().json(proposals))
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
    let proposal = gov_mgr.get_proposal(&proposal_id).await?
        .ok_or_else(|| crate::error::GatewayError::NotFound(format!("Proposal not found: {}", proposal_id.0)))?;

    Ok(HttpResponse::Ok().json(proposal))
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

    let proposal_id = ProposalId(id.into_inner());

    // Use custom voting period or get from domain config
    let voting_period_seconds = if let Some(period) = req.voting_period_seconds {
        period
    } else {
        // Get default from domain (would need to fetch proposal and domain)
        86400 * 7 // Default 7 days if not specified
    };

    gov_mgr.open_proposal(proposal_id.clone(), voting_period_seconds).await?;

    // Track proposal opening
    gateway::governance_proposals_opened_inc();

    // Return updated proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?
        .ok_or_else(|| crate::error::GatewayError::InternalError("Proposal opening succeeded but proposal not found".to_string()))?;

    // Calculate closes_at timestamp from proposal state
    let closes_at = if let icn_governance::ProposalState::Open { opened_at: _, closes_at } = proposal.state {
        closes_at
    } else {
        // Fallback to current time + voting period if state doesn't match
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() + voting_period_seconds
    };

    // Broadcast event to WebSocket subscribers
    event_broadcaster.broadcast(
        &proposal.domain_id.0,
        GatewayEvent::GovernanceProposalOpened {
            proposal_id: proposal_id.0.clone(),
            domain_id: proposal.domain_id.0.clone(),
            closes_at,
        },
    ).await;

    Ok(HttpResponse::Ok().json(proposal))
}

/// POST /gov/proposals/{id}/close - Close a proposal and finalize voting
#[post("/proposals/{id}/close")]
pub async fn close_proposal(
    http_req: HttpRequest,
    gov_mgr: web::Data<Arc<GovernanceManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    let proposal_id = ProposalId(id.into_inner());
    gov_mgr.close_proposal(proposal_id.clone()).await?;

    // Track proposal closing
    gateway::governance_proposals_closed_inc();

    // Return updated proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?
        .ok_or_else(|| crate::error::GatewayError::InternalError("Proposal closing succeeded but proposal not found".to_string()))?;

    // Determine outcome from proposal state
    let outcome = match &proposal.state {
        icn_governance::ProposalState::Accepted { .. } => "accepted",
        icn_governance::ProposalState::Rejected { .. } => "rejected",
        icn_governance::ProposalState::NoQuorum { .. } => "no_quorum",
        _ => "unknown", // Shouldn't happen after close, but handle gracefully
    };

    // Broadcast event to WebSocket subscribers
    event_broadcaster.broadcast(
        &proposal.domain_id.0,
        GatewayEvent::GovernanceProposalClosed {
            proposal_id: proposal_id.0.clone(),
            domain_id: proposal.domain_id.0.clone(),
            outcome: outcome.to_string(),
        },
    ).await;

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
    id: web::Path<String>,
    req: web::Json<CastVoteRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "gov:write")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| crate::error::GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let voter_did: Did = claims.sub.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Parse vote choice
    let choice = match req.choice.to_lowercase().as_str() {
        "for" => VoteChoice::For,
        "against" => VoteChoice::Against,
        "abstain" => VoteChoice::Abstain,
        _ => return Err(crate::error::GatewayError::BadRequest(format!("Invalid vote choice: {}", req.choice))),
    };

    let proposal_id = ProposalId(id.into_inner());
    gov_mgr.cast_vote(proposal_id.clone(), voter_did.clone(), choice, req.comment.clone()).await?;

    // Track vote
    gateway::governance_votes_cast_inc();

    // Return updated proposal
    let proposal = gov_mgr.get_proposal(&proposal_id).await?
        .ok_or_else(|| crate::error::GatewayError::InternalError("Vote cast succeeded but proposal not found".to_string()))?;

    // Broadcast event to WebSocket subscribers
    event_broadcaster.broadcast(
        &proposal.domain_id.0,
        GatewayEvent::GovernanceVoteCast {
            proposal_id: proposal_id.0.clone(),
            domain_id: proposal.domain_id.0.clone(),
            voter: voter_did.to_string(),
            choice: req.choice.clone(),
        },
    ).await;

    Ok(HttpResponse::Ok().json(proposal))
}
