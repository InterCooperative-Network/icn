//! Gateway adapter that bridges `GovernanceEventEmitter` → `EventBroadcaster`.
//!
//! `GatewayEventAdapter` implements the domain-owned `GovernanceEventEmitter`
//! trait and forwards each governance lifecycle event to the gateway's
//! WebSocket `EventBroadcaster` as the corresponding `GatewayEvent` variant.
//!
//! Because `GovernanceEventEmitter` methods are synchronous (fire-and-forget),
//! each emission spawns a detached Tokio task so the caller is never blocked
//! by the broadcast channel's back-pressure.

use crate::events::{EventBroadcaster, GatewayEvent};
use icn_governance_actor::events::GovernanceEventEmitter;
use std::sync::Arc;

/// Adapts the gateway's `EventBroadcaster` to the `GovernanceEventEmitter`
/// interface expected by `apps/governance` HTTP handlers.
#[derive(Clone)]
pub struct GatewayEventAdapter {
    broadcaster: Arc<EventBroadcaster>,
}

impl GatewayEventAdapter {
    pub fn new(broadcaster: Arc<EventBroadcaster>) -> Self {
        Self { broadcaster }
    }
}

impl GovernanceEventEmitter for GatewayEventAdapter {
    fn emit_domain_created(&self, domain_id: &str, name: &str, creator: &str) {
        let b = self.broadcaster.clone();
        let domain_id = domain_id.to_owned();
        let name = name.to_owned();
        let creator = creator.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceDomainCreated {
                    domain_id: domain_id.clone(),
                    name,
                    creator,
                },
            )
            .await;
        });
    }

    fn emit_proposal_created(
        &self,
        proposal_id: &str,
        domain_id: &str,
        proposer: &str,
        title: &str,
        payload_type: &str,
    ) {
        let b = self.broadcaster.clone();
        let proposal_id = proposal_id.to_owned();
        let domain_id = domain_id.to_owned();
        let proposer = proposer.to_owned();
        let title = title.to_owned();
        let payload_type = payload_type.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceProposalCreated {
                    proposal_id,
                    domain_id: domain_id.clone(),
                    proposer,
                    title,
                    payload_type,
                },
            )
            .await;
        });
    }

    fn emit_proposal_opened(&self, proposal_id: &str, domain_id: &str, closes_at: u64) {
        let b = self.broadcaster.clone();
        let proposal_id = proposal_id.to_owned();
        let domain_id = domain_id.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceProposalOpened {
                    proposal_id,
                    domain_id: domain_id.clone(),
                    closes_at,
                },
            )
            .await;
        });
    }

    fn emit_proposal_closed(&self, proposal_id: &str, domain_id: &str, outcome: &str) {
        let b = self.broadcaster.clone();
        let proposal_id = proposal_id.to_owned();
        let domain_id = domain_id.to_owned();
        let outcome = outcome.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceProposalClosed {
                    proposal_id,
                    domain_id: domain_id.clone(),
                    outcome,
                },
            )
            .await;
        });
    }

    fn emit_vote_cast(&self, proposal_id: &str, domain_id: &str, voter: &str, choice: &str) {
        let b = self.broadcaster.clone();
        let proposal_id = proposal_id.to_owned();
        let domain_id = domain_id.to_owned();
        let voter = voter.to_owned();
        let choice = choice.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceVoteCast {
                    proposal_id,
                    domain_id: domain_id.clone(),
                    voter,
                    choice,
                },
            )
            .await;
        });
    }

    fn emit_action_item_created(
        &self,
        item_id: &str,
        domain_id: &str,
        parent_proposal: Option<&str>,
        assignee: Option<&str>,
        created_at: u64,
    ) {
        let b = self.broadcaster.clone();
        let item_id = item_id.to_owned();
        let domain_id = domain_id.to_owned();
        let parent_proposal = parent_proposal.map(str::to_owned);
        let assignee = assignee.map(str::to_owned);
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceActionItemCreated {
                    item_id,
                    domain_id: domain_id.clone(),
                    parent_proposal,
                    assignee,
                    created_at,
                },
            )
            .await;
        });
    }

    fn emit_meeting_scheduled(
        &self,
        meeting_id: &str,
        domain_id: &str,
        title: &str,
        scheduled_at: Option<u64>,
        created_by: &str,
    ) {
        let b = self.broadcaster.clone();
        let meeting_id = meeting_id.to_owned();
        let domain_id = domain_id.to_owned();
        let title = title.to_owned();
        let created_by = created_by.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceMeetingScheduled {
                    meeting_id,
                    domain_id: domain_id.clone(),
                    title,
                    scheduled_at,
                    created_by,
                },
            )
            .await;
        });
    }

    fn emit_meeting_started(&self, meeting_id: &str, domain_id: &str, started_at: u64) {
        let b = self.broadcaster.clone();
        let meeting_id = meeting_id.to_owned();
        let domain_id = domain_id.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceMeetingStarted {
                    meeting_id,
                    domain_id: domain_id.clone(),
                    started_at,
                },
            )
            .await;
        });
    }

    fn emit_meeting_ended(&self, meeting_id: &str, domain_id: &str, ended_at: u64) {
        let b = self.broadcaster.clone();
        let meeting_id = meeting_id.to_owned();
        let domain_id = domain_id.to_owned();
        tokio::spawn(async move {
            b.broadcast(
                &domain_id,
                GatewayEvent::GovernanceMeetingEnded {
                    meeting_id,
                    domain_id: domain_id.clone(),
                    ended_at,
                },
            )
            .await;
        });
    }
}
