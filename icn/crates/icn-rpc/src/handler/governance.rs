//! Governance-related RPC handlers
//!
//! # Coop Isolation
//!
//! TODO(#769): Add `ctx.require_coop()` enforcement when multi-coop governance is implemented.
//! Currently domains are global. When per-coop governance domains exist, handlers should:
//! 1. Require `ctx` to be `Some` for write operations (already done for vote.cast)
//! 2. Call `ctx.require_coop(domain_coop_id)` to validate access
//! 3. Route requests to the appropriate coop-scoped governance instance

use std::sync::Arc;

use icn_governance::{MembershipSource, ProposalPayload, ProposalState, VoteChoice};

use crate::context::RpcContext;
use crate::error_codes::{NOT_FOUND, RESOURCE_NOT_AVAILABLE};
use crate::server::RpcServer;
use crate::types::{
    CastVoteRequest, CloseProposalRequest, CreateDomainRequest, CreateProposalRequest,
    CreateProposalResponse, GovernanceDomainInfo, GovernanceParamsInfo, MembershipConfigInfo,
    OpenProposalRequest, ProposalInfo, ProposalPayloadInfo, RpcResponse,
};

/// Handle governance.domain.list RPC call - list all governance domains
/// Handle governance.domain.list RPC call - list all governance domains
pub async fn handle_governance_domain_list(
    id: u64,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.domain.list called"
        );
    }

    let governance_service = match state.governance_service() {
        Some(service) => service,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    match governance_service.list_domains().await {
        Ok(domains) => {
            let domain_infos: Vec<GovernanceDomainInfo> = domains
                .into_iter()
                .map(|d| GovernanceDomainInfo {
                    id: d.id.0,
                    name: d.name,
                    created_at: d.created_at,
                    profile: d.config.profile.0,
                    membership_type: match &d.config.membership.source {
                        MembershipSource::StaticList(_) => "static_list".to_string(),
                        MembershipSource::TrustThreshold(_) => "trust_threshold".to_string(),
                    },
                    params: GovernanceParamsInfo {
                        quorum_percentage: d.config.params.quorum_percentage,
                        approval_threshold_percentage: d
                            .config
                            .params
                            .approval_threshold_percentage,
                        voting_period_seconds: d.config.params.voting_period_seconds,
                    },
                })
                .collect();

            match serde_json::to_value(&domain_infos) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::internal_error(id, e),
            }
        }
        Err(e) => RpcResponse::error(id, e.to_rpc_code(), e.to_string()),
    }
}
/// Handle governance.domain.get RPC call - get a specific domain
pub async fn handle_governance_domain_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.domain.get called"
        );
    }

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct DomainGetParams {
        domain_id: String,
    }

    let domain_params: DomainGetParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let domain_id = icn_governance::GovernanceDomainId(domain_params.domain_id);

    match governance_handle.get_domain(&domain_id).await {
        Ok(Some(d)) => {
            let domain_info = GovernanceDomainInfo {
                id: d.id.0,
                name: d.name,
                created_at: d.created_at,
                profile: d.config.profile.0,
                membership_type: match &d.config.membership.source {
                    MembershipSource::StaticList(_) => "static_list".to_string(),
                    MembershipSource::TrustThreshold(_) => "trust_threshold".to_string(),
                },
                params: GovernanceParamsInfo {
                    quorum_percentage: d.config.params.quorum_percentage,
                    approval_threshold_percentage: d.config.params.approval_threshold_percentage,
                    voting_period_seconds: d.config.params.voting_period_seconds,
                },
            };

            match serde_json::to_value(&domain_info) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::internal_error(id, e),
            }
        }
        Ok(None) => RpcResponse::error(id, NOT_FOUND, "Domain not found".to_string()),
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle governance.proposal.list RPC call - list all proposals
pub async fn handle_governance_proposal_list(
    id: u64,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.proposal.list called"
        );
    }

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    match governance_handle.list_proposals().await {
        Ok(proposals) => {
            let proposal_infos: Vec<ProposalInfo> = proposals
                .into_iter()
                .map(|p| {
                    let (state_str, opened_at, closes_at, closed_at) = match &p.state {
                        ProposalState::Draft => ("draft".to_string(), None, None, None),
                        ProposalState::Deliberation {
                            started_at,
                            ends_at,
                        } => (
                            "deliberation".to_string(),
                            Some(*started_at),
                            Some(*ends_at),
                            None,
                        ),
                        ProposalState::Open {
                            opened_at,
                            closes_at,
                        } => ("open".to_string(), Some(*opened_at), Some(*closes_at), None),
                        ProposalState::Accepted { closed_at } => {
                            ("accepted".to_string(), None, None, Some(*closed_at))
                        }
                        ProposalState::Rejected { closed_at } => {
                            ("rejected".to_string(), None, None, Some(*closed_at))
                        }
                        ProposalState::NoQuorum { closed_at } => {
                            ("no_quorum".to_string(), None, None, Some(*closed_at))
                        }
                        ProposalState::Cancelled { cancelled_at } => {
                            ("cancelled".to_string(), None, None, Some(*cancelled_at))
                        }
                        ProposalState::Vetoed { vetoed_at, .. } => {
                            ("vetoed".to_string(), None, None, Some(*vetoed_at))
                        }
                        ProposalState::ForceClosed { closed_at, .. } => {
                            ("force_closed".to_string(), None, None, Some(*closed_at))
                        }
                    };

                    ProposalInfo {
                        id: p.id.0,
                        domain_id: p.domain_id.0,
                        proposer: p.proposer.to_string(),
                        title: p.title,
                        description: p.description,
                        state: state_str,
                        created_at: p.created_at,
                        updated_at: p.updated_at,
                        opened_at,
                        closes_at,
                        closed_at,
                    }
                })
                .collect();

            match serde_json::to_value(&proposal_infos) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::internal_error(id, e),
            }
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle governance.proposal.get RPC call - get a specific proposal
pub async fn handle_governance_proposal_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.proposal.get called"
        );
    }

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct ProposalGetParams {
        proposal_id: String,
    }

    let proposal_params: ProposalGetParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(proposal_params.proposal_id);

    match governance_handle.get_proposal(&proposal_id).await {
        Ok(Some(p)) => {
            let (state_str, opened_at, closes_at, closed_at) = match &p.state {
                ProposalState::Draft => ("draft".to_string(), None, None, None),
                ProposalState::Deliberation {
                    started_at,
                    ends_at,
                } => (
                    "deliberation".to_string(),
                    Some(*started_at),
                    Some(*ends_at),
                    None,
                ),
                ProposalState::Open {
                    opened_at,
                    closes_at,
                } => ("open".to_string(), Some(*opened_at), Some(*closes_at), None),
                ProposalState::Accepted { closed_at } => {
                    ("accepted".to_string(), None, None, Some(*closed_at))
                }
                ProposalState::Rejected { closed_at } => {
                    ("rejected".to_string(), None, None, Some(*closed_at))
                }
                ProposalState::NoQuorum { closed_at } => {
                    ("no_quorum".to_string(), None, None, Some(*closed_at))
                }
                ProposalState::Cancelled { cancelled_at } => {
                    ("cancelled".to_string(), None, None, Some(*cancelled_at))
                }
                ProposalState::Vetoed { vetoed_at, .. } => {
                    ("vetoed".to_string(), None, None, Some(*vetoed_at))
                }
                ProposalState::ForceClosed { closed_at, .. } => {
                    ("force_closed".to_string(), None, None, Some(*closed_at))
                }
            };

            let proposal_info = ProposalInfo {
                id: p.id.0,
                domain_id: p.domain_id.0,
                proposer: p.proposer.to_string(),
                title: p.title,
                description: p.description,
                state: state_str,
                created_at: p.created_at,
                updated_at: p.updated_at,
                opened_at,
                closes_at,
                closed_at,
            };

            match serde_json::to_value(&proposal_info) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::internal_error(id, e),
            }
        }
        Ok(None) => RpcResponse::error(id, NOT_FOUND, "Proposal not found".to_string()),
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle governance.domain.create RPC call - create a new domain
pub async fn handle_governance_domain_create(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.domain.create called"
        );
    }

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    let request: CreateDomainRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Build membership config from enum
    let membership = icn_governance::MembershipConfig {
        source: match &request.membership {
            MembershipConfigInfo::StaticList { members } => {
                let mut dids = Vec::new();
                for m in members {
                    match icn_identity::Did::from_str(m) {
                        Ok(d) => dids.push(d),
                        Err(e) => {
                            return RpcResponse::error(
                                id,
                                -32602,
                                format!("Invalid member DID '{m}': {e}"),
                            );
                        }
                    }
                }
                MembershipSource::StaticList(dids)
            }
            MembershipConfigInfo::TrustThreshold { threshold } => {
                MembershipSource::TrustThreshold(*threshold)
            }
        },
    };

    let params_config = icn_governance::GovernanceParams::new(
        request.params.quorum_percentage,
        request.params.approval_threshold_percentage,
        request.params.voting_period_seconds,
    );

    let domain_id = icn_governance::GovernanceDomainId(request.domain_id);

    match governance_handle
        .create_domain(
            domain_id,
            request.name,
            request.profile,
            params_config,
            membership,
        )
        .await
    {
        Ok(()) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle governance.proposal.create RPC call - create a new proposal
pub async fn handle_governance_proposal_create(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.proposal.create called"
        );
    }

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    let request: CreateProposalRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let domain_id = icn_governance::GovernanceDomainId(request.domain_id);

    // Build payload from request enum
    let payload = match &request.payload {
        ProposalPayloadInfo::Text { body } => ProposalPayload::Text { body: body.clone() },
        ProposalPayloadInfo::Budget {
            amount,
            currency,
            recipient,
            purpose,
        } => {
            let recipient_did = match icn_identity::Did::from_str(recipient) {
                Ok(d) => d,
                Err(e) => {
                    return RpcResponse::error(id, -32602, format!("Invalid recipient DID: {e}"));
                }
            };
            ProposalPayload::Budget {
                amount: *amount,
                currency: currency.clone(),
                recipient: recipient_did,
                purpose: purpose.clone(),
            }
        }
        ProposalPayloadInfo::ConfigChange { new_config } => ProposalPayload::ConfigChange {
            new_config: new_config.clone(),
        },
        ProposalPayloadInfo::Membership { action, member } => {
            let membership_action = match action.as_str() {
                "add" => icn_governance::MembershipAction::Add,
                "remove" => icn_governance::MembershipAction::Remove,
                _ => {
                    return RpcResponse::error(
                        id,
                        -32602,
                        format!("Invalid membership action: {action}"),
                    );
                }
            };
            let member_did = match icn_identity::Did::from_str(member) {
                Ok(d) => d,
                Err(e) => {
                    return RpcResponse::error(id, -32602, format!("Invalid member DID: {e}"));
                }
            };
            ProposalPayload::Membership {
                action: membership_action,
                member: member_did,
            }
        }
    };

    match governance_handle
        .create_proposal(domain_id, request.title, request.description, payload)
        .await
    {
        Ok(proposal_id) => {
            let response = CreateProposalResponse {
                proposal_id: proposal_id.0,
            };
            match serde_json::to_value(&response) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::internal_error(id, e),
            }
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle governance.proposal.open RPC call - open a proposal for voting
pub async fn handle_governance_proposal_open(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.proposal.open called"
        );
    }

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    let request: OpenProposalRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(request.proposal_id);

    match governance_handle
        .open_proposal(proposal_id, request.voting_period_seconds)
        .await
    {
        Ok(()) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle governance.vote.cast RPC call - cast a vote on a proposal
///
/// Requires scope: `governance:vote` or `governance:*`
pub async fn handle_governance_vote_cast(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    // Require authentication with governance:vote scope
    let ctx = match ctx {
        Some(c) => c,
        None => {
            return RpcResponse::error(
                id,
                crate::error_codes::AUTHENTICATION_REQUIRED,
                "Authentication required to cast votes".to_string(),
            );
        }
    };

    // Verify caller has required scope
    if !ctx.has_scope("governance:vote") {
        tracing::warn!(
            caller = %ctx.caller_did,
            "Vote attempt denied: missing governance:vote scope"
        );
        return RpcResponse::error(
            id,
            crate::error_codes::SCOPE_INSUFFICIENT,
            "Insufficient scope: governance:vote required".to_string(),
        );
    }

    tracing::debug!(
        caller = %ctx.caller_did,
        coop_id = ?ctx.coop_id,
        "governance.vote.cast called"
    );

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    let request: CastVoteRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(request.proposal_id);

    // Convert choice value
    let choice = match request.choice.as_str() {
        "for" => VoteChoice::For,
        "against" => VoteChoice::Against,
        "abstain" => VoteChoice::Abstain,
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                format!(
                    "Invalid vote choice: {}. Must be 'for', 'against', or 'abstain'",
                    request.choice
                ),
            );
        }
    };

    match governance_handle
        .cast_vote(proposal_id, choice, request.comment)
        .await
    {
        Ok(()) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle governance.proposal.close RPC call - close a proposal
pub async fn handle_governance_proposal_close(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "governance.proposal.close called"
        );
    }

    let governance_handle = match state.governance_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Governance not available".to_string(),
            );
        }
    };

    let request: CloseProposalRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let proposal_id = icn_governance::ProposalId(request.proposal_id);

    match governance_handle.close_proposal(proposal_id).await {
        Ok(()) => {
            let result = serde_json::json!({ "success": true });
            RpcResponse::success(id, result)
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}
