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

use icn_governance::{
    Delegation, DelegationId, DelegationScope, MembershipSource, ProposalPayload, ProposalState,
    VoteChoice,
};

use crate::context::RpcContext;
use crate::error_codes::{
    AUTHENTICATION_REQUIRED, INVALID_PARAMS, NOT_FOUND, RESOURCE_NOT_AVAILABLE,
};
use crate::server::RpcServer;
use crate::types::{
    CastVoteRequest, CloseProposalRequest, CreateDelegationRequest, CreateDomainRequest,
    CreateProposalRequest, CreateProposalResponse, GovernanceDomainInfo, GovernanceParamsInfo,
    MembershipConfigInfo, OpenProposalRequest, ProposalInfo, ProposalPayloadInfo,
    RevokeDelegationRequest, RpcResponse,
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
        ProposalPayloadInfo::Allocation {
            pool_amount,
            unit,
            options,
            purpose,
        } => {
            let mut parsed_options = Vec::with_capacity(options.len());
            for opt in options {
                let recipient_did = match icn_identity::Did::from_str(&opt.recipient) {
                    Ok(d) => d,
                    Err(e) => {
                        return RpcResponse::error(
                            id,
                            -32602,
                            format!("Invalid option recipient DID: {e}"),
                        );
                    }
                };
                parsed_options.push(icn_governance::AllocationOption {
                    label: opt.label.clone(),
                    description: opt.description.clone(),
                    recipient: recipient_did,
                    requested_amount: opt.requested_amount,
                });
            }
            ProposalPayload::Allocation {
                pool_amount: *pool_amount,
                unit: unit.clone(),
                options: parsed_options,
                purpose: purpose.clone(),
            }
        }
    };

    match governance_handle
        .create_proposal(
            domain_id,
            request.title,
            request.description,
            payload,
            icn_governance::ProposalScope::Local,
        )
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
/// Scope authorization is enforced centrally by the JSON-RPC dispatcher
/// (`required_scopes_for_method` → `[governance:proposal:write,
/// governance:write]`), the same accepted-also gate the other proposal-family
/// methods use (#1868). This handler only requires an authenticated context so
/// the caller DID can be recorded as the voter; it does not re-check scopes.
pub async fn handle_governance_vote_cast(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    // The caller DID is the voter identity, so an authenticated context is
    // required. Scope authorization already happened at central dispatch.
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
        .cast_vote(proposal_id, ctx.caller_did.clone(), choice, request.comment)
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

// ============================================================================
// Vote delegation (issue #2113)
//
// These three handlers wire icnctl's `gov vote delegate/delegations/revoke`
// commands to the existing `GovernanceOps` delegation methods. The load-bearing
// part is the authorization gate, not the storage: a governance write-path must
// never let a caller act on another member's behalf.
//
//   create -> delegator is ALWAYS ctx.caller_did (never a params field)
//   list   -> only the caller's own outgoing/incoming delegations
//   revoke -> load first, revoke only if delegation.delegator == ctx.caller_did
//
// All three fail closed when the request is unauthenticated.
// ============================================================================

/// Parse a delegation scope string (`blanket` / `domain:<id>` / `proposal:<id>`).
fn parse_delegation_scope(scope: &str) -> Result<DelegationScope, String> {
    let scope = scope.trim();
    if scope.eq_ignore_ascii_case("blanket") {
        return Ok(DelegationScope::Blanket);
    }
    if let Some(domain) = scope.strip_prefix("domain:") {
        let domain = domain.trim();
        if domain.is_empty() {
            return Err("domain scope requires an id: 'domain:<id>'".to_string());
        }
        return Ok(DelegationScope::Domain(
            icn_governance::GovernanceDomainId::new(domain),
        ));
    }
    if let Some(proposal) = scope.strip_prefix("proposal:") {
        let proposal = proposal.trim();
        if proposal.is_empty() {
            return Err("proposal scope requires an id: 'proposal:<id>'".to_string());
        }
        return Ok(DelegationScope::Proposal(icn_governance::ProposalId(
            proposal.to_string(),
        )));
    }
    Err(format!(
        "invalid delegation scope '{scope}'. Expected 'blanket', 'domain:<id>', or 'proposal:<id>'"
    ))
}

/// Render a [`DelegationScope`] back to its wire string form.
fn delegation_scope_to_string(scope: &DelegationScope) -> String {
    match scope {
        DelegationScope::Blanket => "blanket".to_string(),
        DelegationScope::Domain(d) => format!("domain:{}", d.0),
        DelegationScope::Proposal(p) => format!("proposal:{}", p.0),
    }
}

/// Serialize a delegation into the JSON shape the icnctl client renders.
fn delegation_to_json(d: &Delegation, now: u64) -> serde_json::Value {
    serde_json::json!({
        "id": d.id.to_string(),
        "delegator": d.delegator.to_string(),
        "delegate": d.delegate.to_string(),
        "scope": delegation_scope_to_string(&d.scope),
        "is_active": d.is_active(now),
        "expires_at": d.expires_at,
    })
}

/// Handle `governance.delegation.create` — create a vote delegation.
///
/// The delegator is the authenticated caller (`ctx.caller_did`), never a params
/// field, so a caller can only ever delegate their own vote. Fail-closed when
/// unauthenticated.
pub async fn handle_governance_delegation_create(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    let ctx = match ctx {
        Some(c) => c,
        None => {
            return RpcResponse::error(
                id,
                AUTHENTICATION_REQUIRED,
                "Authentication required to create a delegation".to_string(),
            );
        }
    };

    tracing::debug!(caller = %ctx.caller_did, "governance.delegation.create called");

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

    let request: CreateDelegationRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid params: {e}")),
    };

    let delegate = match icn_identity::Did::from_str(&request.delegate) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid delegate DID: {e}"));
        }
    };

    let scope = match parse_delegation_scope(&request.scope) {
        Ok(s) => s,
        Err(e) => return RpcResponse::error(id, INVALID_PARAMS, e),
    };

    // SECURITY: delegator is the authenticated caller, never read from params.
    let mut delegation = Delegation::new(ctx.caller_did.clone(), delegate, scope);
    if let Some(expires_at) = request.expires_at {
        delegation = delegation.with_expiry(expires_at);
    }
    let delegation_id = delegation.id.to_string();

    match governance_handle.create_delegation(delegation).await {
        Ok(()) => RpcResponse::success(id, serde_json::json!({ "id": delegation_id })),
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle `governance.delegation.list` — list the caller's own delegations.
///
/// Returns only the authenticated caller's outgoing (`given`) and incoming
/// (`received`) delegations. No arbitrary-DID listing is accepted.
pub async fn handle_governance_delegation_list(
    id: u64,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    let ctx = match ctx {
        Some(c) => c,
        None => {
            return RpcResponse::error(
                id,
                AUTHENTICATION_REQUIRED,
                "Authentication required to list delegations".to_string(),
            );
        }
    };

    tracing::debug!(caller = %ctx.caller_did, "governance.delegation.list called");

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

    let caller = &ctx.caller_did;
    let given = match governance_handle.get_delegations_from(caller).await {
        Ok(v) => v,
        Err(e) => return RpcResponse::internal_error(id, e),
    };
    let received = match governance_handle.get_delegations_to(caller).await {
        Ok(v) => v,
        Err(e) => return RpcResponse::internal_error(id, e),
    };

    let now = icn_time::current_timestamp_secs();
    let given: Vec<_> = given.iter().map(|d| delegation_to_json(d, now)).collect();
    let received: Vec<_> = received
        .iter()
        .map(|d| delegation_to_json(d, now))
        .collect();

    RpcResponse::success(
        id,
        serde_json::json!({ "given": given, "received": received }),
    )
}

/// Handle `governance.delegation.revoke` — revoke one of the caller's delegations.
///
/// The delegation is loaded first and revoked ONLY if the caller is its delegator.
/// A delegation that does not exist OR is not owned by the caller returns the same
/// `NOT_FOUND` response, so the endpoint never reveals another member's delegation.
pub async fn handle_governance_delegation_revoke(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    let ctx = match ctx {
        Some(c) => c,
        None => {
            return RpcResponse::error(
                id,
                AUTHENTICATION_REQUIRED,
                "Authentication required to revoke a delegation".to_string(),
            );
        }
    };

    tracing::debug!(caller = %ctx.caller_did, "governance.delegation.revoke called");

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

    let request: RevokeDelegationRequest = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid params: {e}")),
    };

    let delegation_id = DelegationId::new(request.delegation_id);

    // Load first, then authorize: only the delegator may revoke. "Not found" and
    // "not owned by caller" return the SAME response so existence is never leaked.
    let existing = match governance_handle.get_delegation(&delegation_id).await {
        Ok(d) => d,
        Err(e) => return RpcResponse::internal_error(id, e),
    };
    match existing {
        Some(d) if d.delegator == ctx.caller_did => {}
        _ => return RpcResponse::error(id, NOT_FOUND, "Delegation not found".to_string()),
    }

    let now = icn_time::current_timestamp_secs();
    match governance_handle
        .revoke_delegation(&delegation_id, now)
        .await
    {
        Ok(()) => RpcResponse::success(id, serde_json::json!({ "success": true })),
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

#[cfg(test)]
mod delegation_auth_tests {
    use super::*;
    use icn_identity::KeyPair;
    use std::net::SocketAddr;
    use std::sync::Mutex;

    /// In-memory `GovernanceOps` test double. Only the delegation methods are
    /// functional (backed by a shared Vec the test can inspect); every other method
    /// is unused by these tests and panics if reached.
    #[derive(Clone, Default)]
    struct RecordingGovernance {
        delegations: Arc<Mutex<Vec<Delegation>>>,
    }

    #[async_trait::async_trait]
    impl icn_governance::GovernanceOps for RecordingGovernance {
        // ---- functional delegation methods ----
        async fn create_delegation(&self, delegation: Delegation) -> anyhow::Result<()> {
            self.delegations.lock().unwrap().push(delegation);
            Ok(())
        }
        async fn get_delegation(&self, id: &DelegationId) -> anyhow::Result<Option<Delegation>> {
            Ok(self
                .delegations
                .lock()
                .unwrap()
                .iter()
                .find(|d| &d.id == id)
                .cloned())
        }
        async fn get_delegations_from(
            &self,
            delegator: &icn_identity::Did,
        ) -> anyhow::Result<Vec<Delegation>> {
            Ok(self
                .delegations
                .lock()
                .unwrap()
                .iter()
                .filter(|d| &d.delegator == delegator)
                .cloned()
                .collect())
        }
        async fn get_delegations_to(
            &self,
            delegate: &icn_identity::Did,
        ) -> anyhow::Result<Vec<Delegation>> {
            Ok(self
                .delegations
                .lock()
                .unwrap()
                .iter()
                .filter(|d| &d.delegate == delegate)
                .cloned()
                .collect())
        }
        async fn revoke_delegation(
            &self,
            id: &DelegationId,
            revoked_at: icn_governance::Timestamp,
        ) -> anyhow::Result<()> {
            let mut g = self.delegations.lock().unwrap();
            if let Some(d) = g.iter_mut().find(|d| &d.id == id) {
                d.revoked_at = Some(revoked_at);
            }
            Ok(())
        }

        // ---- unused methods (never reached by delegation handlers) ----
        async fn list_domains(&self) -> anyhow::Result<Vec<icn_governance::GovernanceDomain>> {
            unimplemented!()
        }
        async fn list_domains_paginated(
            &self,
            _cursor: Option<&str>,
            _limit: usize,
        ) -> anyhow::Result<icn_governance::PaginatedResult<icn_governance::GovernanceDomain>>
        {
            unimplemented!()
        }
        async fn get_domain(
            &self,
            _id: &icn_governance::GovernanceDomainId,
        ) -> anyhow::Result<Option<icn_governance::GovernanceDomain>> {
            unimplemented!()
        }
        async fn list_proposals(&self) -> anyhow::Result<Vec<icn_governance::Proposal>> {
            unimplemented!()
        }
        async fn get_proposal(
            &self,
            _id: &icn_governance::ProposalId,
        ) -> anyhow::Result<Option<icn_governance::Proposal>> {
            unimplemented!()
        }
        async fn create_domain(
            &self,
            _domain_id: icn_governance::GovernanceDomainId,
            _name: String,
            _profile: String,
            _params: icn_governance::GovernanceParams,
            _membership: icn_governance::MembershipConfig,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn create_proposal(
            &self,
            _domain_id: icn_governance::GovernanceDomainId,
            _title: String,
            _description: String,
            _payload: icn_governance::ProposalPayload,
            _scope: icn_governance::ProposalScope,
        ) -> anyhow::Result<icn_governance::ProposalId> {
            unimplemented!()
        }
        async fn start_deliberation(
            &self,
            _proposal_id: icn_governance::ProposalId,
            _deliberation_period_seconds: u64,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn end_deliberation_and_open(
            &self,
            _proposal_id: icn_governance::ProposalId,
            _voting_period_seconds: u64,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn open_proposal(
            &self,
            _proposal_id: icn_governance::ProposalId,
            _voting_period_seconds: u64,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn cast_vote(
            &self,
            _proposal_id: icn_governance::ProposalId,
            _voter: icn_identity::Did,
            _choice: icn_governance::VoteChoice,
            _comment: Option<String>,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn close_proposal(
            &self,
            _proposal_id: icn_governance::ProposalId,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn update_domain_membership(
            &self,
            _domain_id: icn_governance::GovernanceDomainId,
            _member: icn_identity::Did,
            _action: icn_governance::MembershipAction,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn get_vote_tally(
            &self,
            _proposal_id: &icn_governance::ProposalId,
        ) -> anyhow::Result<icn_governance::VoteTally> {
            unimplemented!()
        }
        async fn get_voter_dids(
            &self,
            _proposal_id: &icn_governance::ProposalId,
        ) -> anyhow::Result<Vec<icn_identity::Did>> {
            unimplemented!()
        }
        async fn get_proof(
            &self,
            _proposal_id: &icn_governance::ProposalId,
        ) -> anyhow::Result<Option<icn_governance::GovernanceProofV2>> {
            unimplemented!()
        }
        async fn list_protocol_parameters(
            &self,
        ) -> anyhow::Result<Vec<icn_governance::ProtocolParameter>> {
            unimplemented!()
        }
        async fn get_protocol_parameter(
            &self,
            _id: &str,
        ) -> anyhow::Result<Option<icn_governance::ProtocolParameter>> {
            unimplemented!()
        }
        async fn get_effective_protocol_parameter(
            &self,
            _id: &str,
            _coop_id: Option<&str>,
            _fed_id: Option<&str>,
        ) -> anyhow::Result<Option<icn_governance::ProtocolParameter>> {
            unimplemented!()
        }
        async fn get_protocol_parameter_history(
            &self,
            _id: &str,
        ) -> anyhow::Result<Vec<icn_governance::ParameterChange>> {
            unimplemented!()
        }
    }

    fn test_state(gov: RecordingGovernance) -> Arc<RpcServer> {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut server = RpcServer::new(addr);
        server.set_governance_handle(gov);
        Arc::new(server)
    }

    fn new_did() -> icn_identity::Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[tokio::test]
    async fn create_binds_delegator_to_caller_ignoring_params() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov.clone());
        let alice = new_did();
        let bob = new_did();
        let mallory = new_did();
        let ctx = RpcContext::new(alice.clone(), None, vec![]);
        // A spurious "delegator" field must be ignored — the delegator is the caller.
        let params = serde_json::json!({
            "delegate": bob.to_string(),
            "scope": "blanket",
            "delegator": mallory.to_string(),
        });
        let resp = handle_governance_delegation_create(1, &params, &state, Some(&ctx)).await;
        assert!(
            resp.error.is_none(),
            "expected success, got {:?}",
            resp.error
        );
        let stored = gov.delegations.lock().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].delegator, alice,
            "delegator must be the authenticated caller, not a params value"
        );
        assert_eq!(stored[0].delegate, bob);
    }

    #[tokio::test]
    async fn create_requires_authentication() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov.clone());
        let bob = new_did();
        let params = serde_json::json!({ "delegate": bob.to_string(), "scope": "blanket" });
        let resp = handle_governance_delegation_create(1, &params, &state, None).await;
        assert_eq!(resp.error.unwrap().code, AUTHENTICATION_REQUIRED);
        assert!(gov.delegations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_returns_only_callers_own_delegations() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov.clone());
        let alice = new_did();
        let bob = new_did();
        let carol = new_did();
        let dave = new_did();

        let mk = |caller: &icn_identity::Did, delegate: &icn_identity::Did| {
            (
                RpcContext::new(caller.clone(), None, vec![]),
                serde_json::json!({ "delegate": delegate.to_string(), "scope": "blanket" }),
            )
        };
        let (ca, pa) = mk(&alice, &carol); // alice -> carol (given by alice)
        handle_governance_delegation_create(1, &pa, &state, Some(&ca)).await;
        let (cb, pb) = mk(&bob, &dave); // bob -> dave (unrelated to alice)
        handle_governance_delegation_create(2, &pb, &state, Some(&cb)).await;
        let (cd, pd) = mk(&dave, &alice); // dave -> alice (received by alice)
        handle_governance_delegation_create(3, &pd, &state, Some(&cd)).await;

        let ctx_alice = RpcContext::new(alice.clone(), None, vec![]);
        let resp = handle_governance_delegation_list(9, &state, Some(&ctx_alice)).await;
        let result = resp.result.expect("list result");
        let given = result["given"].as_array().unwrap();
        let received = result["received"].as_array().unwrap();

        assert_eq!(given.len(), 1, "alice gave exactly one delegation");
        assert_eq!(given[0]["delegator"], alice.to_string());
        assert_eq!(given[0]["delegate"], carol.to_string());
        assert_eq!(received.len(), 1, "alice received exactly one delegation");
        assert_eq!(received[0]["delegate"], alice.to_string());
        // Bob's unrelated delegation must not leak into alice's view at all.
        assert!(
            !result.to_string().contains(&bob.to_string()),
            "another member's delegation leaked into the caller's list"
        );
    }

    #[tokio::test]
    async fn list_requires_authentication() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov);
        let resp = handle_governance_delegation_list(1, &state, None).await;
        assert_eq!(resp.error.unwrap().code, AUTHENTICATION_REQUIRED);
    }

    #[tokio::test]
    async fn revoke_by_owner_succeeds() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov.clone());
        let alice = new_did();
        let bob = new_did();
        let ctx_alice = RpcContext::new(alice.clone(), None, vec![]);
        let create = serde_json::json!({ "delegate": bob.to_string(), "scope": "blanket" });
        let cresp = handle_governance_delegation_create(1, &create, &state, Some(&ctx_alice)).await;
        let dele_id = cresp.result.unwrap()["id"].as_str().unwrap().to_string();

        let revoke = serde_json::json!({ "delegation_id": dele_id });
        let rresp = handle_governance_delegation_revoke(2, &revoke, &state, Some(&ctx_alice)).await;
        assert!(
            rresp.error.is_none(),
            "owner revoke should succeed: {:?}",
            rresp.error
        );
        assert!(
            gov.delegations.lock().unwrap()[0].revoked_at.is_some(),
            "delegation should be revoked"
        );
    }

    #[tokio::test]
    async fn revoke_by_non_owner_is_rejected_and_not_leaking() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov.clone());
        let alice = new_did();
        let bob = new_did();
        let ctx_alice = RpcContext::new(alice.clone(), None, vec![]);
        let ctx_bob = RpcContext::new(bob.clone(), None, vec![]);
        let create = serde_json::json!({ "delegate": bob.to_string(), "scope": "blanket" });
        let cresp = handle_governance_delegation_create(1, &create, &state, Some(&ctx_alice)).await;
        let dele_id = cresp.result.unwrap()["id"].as_str().unwrap().to_string();

        // Bob is the delegate, NOT the delegator — he must not be able to revoke.
        let revoke = serde_json::json!({ "delegation_id": dele_id });
        let rresp = handle_governance_delegation_revoke(2, &revoke, &state, Some(&ctx_bob)).await;
        let err = rresp.error.expect("non-owner revoke must be rejected");
        assert_eq!(
            err.code, NOT_FOUND,
            "non-owner gets the same not-found as a missing id"
        );
        assert!(
            gov.delegations.lock().unwrap()[0].revoked_at.is_none(),
            "a non-owner must NOT be able to revoke another member's delegation"
        );
    }

    #[tokio::test]
    async fn revoke_requires_authentication() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov);
        let params = serde_json::json!({ "delegation_id": "anything" });
        let resp = handle_governance_delegation_revoke(1, &params, &state, None).await;
        assert_eq!(resp.error.unwrap().code, AUTHENTICATION_REQUIRED);
    }

    #[tokio::test]
    async fn revoke_unknown_id_returns_not_found() {
        let gov = RecordingGovernance::default();
        let state = test_state(gov);
        let alice = new_did();
        let ctx = RpcContext::new(alice, None, vec![]);
        let params = serde_json::json!({ "delegation_id": "no-such-delegation" });
        let resp = handle_governance_delegation_revoke(1, &params, &state, Some(&ctx)).await;
        assert_eq!(resp.error.unwrap().code, NOT_FOUND);
    }

    #[test]
    fn scope_parsing_accepts_known_forms_and_rejects_others() {
        assert!(matches!(
            parse_delegation_scope("blanket"),
            Ok(DelegationScope::Blanket)
        ));
        assert!(matches!(
            parse_delegation_scope("BLANKET"),
            Ok(DelegationScope::Blanket)
        ));
        assert!(matches!(
            parse_delegation_scope("domain:econ"),
            Ok(DelegationScope::Domain(_))
        ));
        assert!(matches!(
            parse_delegation_scope("proposal:p-1"),
            Ok(DelegationScope::Proposal(_))
        ));
        assert!(parse_delegation_scope("domain:").is_err());
        assert!(parse_delegation_scope("proposal:").is_err());
        assert!(parse_delegation_scope("nonsense").is_err());
    }
}
