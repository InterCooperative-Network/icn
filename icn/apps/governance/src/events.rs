//! Domain-owned event emitter trait for governance HTTP handlers.
//!
//! `GovernanceEventEmitter` is implemented by gateway's `GatewayEventAdapter`
//! (which forwards to `EventBroadcaster`) and by `NoopEventEmitter` for
//! testing / standalone mode.

use icn_governance::{ActionItem, Meeting};

/// Emitter for governance lifecycle events.
///
/// Implement this in the hosting process (e.g. gateway) and pass it into
/// [`configure`] so handlers can broadcast without depending on gateway types.
///
/// Method naming follows the canonical `Governance<Thing><Verb>` convention
/// (see docs/strategy/NYCN-Repo-Architecture-Spec.md §6).
///
/// [`configure`]: crate::http::configure
pub trait GovernanceEventEmitter: Send + Sync + 'static {
    fn emit_domain_created(&self, domain_id: &str, name: &str, creator: &str);
    fn emit_proposal_created(
        &self,
        proposal_id: &str,
        domain_id: &str,
        proposer: &str,
        title: &str,
        payload_type: &str,
    );
    fn emit_proposal_opened(&self, proposal_id: &str, domain_id: &str, closes_at: u64);
    fn emit_proposal_closed(&self, proposal_id: &str, domain_id: &str, outcome: &str);
    fn emit_vote_cast(&self, proposal_id: &str, domain_id: &str, voter: &str, choice: &str);

    /// Fire when an action item is materialized from an accepted proposal's
    /// `ActionItemSpec` (decision→action bridge).
    fn emit_action_item_created(&self, item: &ActionItem);

    /// Fire when a meeting is scheduled (`POST /gov/domains/{domain}/meetings`).
    fn emit_meeting_scheduled(&self, meeting: &Meeting);

    /// Fire when a meeting transitions to `InProgress`.
    fn emit_meeting_started(&self, meeting_id: &str, domain_id: &str, started_at: u64);

    /// Fire when a meeting transitions to `Completed`.
    fn emit_meeting_ended(&self, meeting_id: &str, domain_id: &str, ended_at: u64);
}

/// No-op emitter for tests and standalone mode.
#[derive(Clone)]
pub struct NoopEventEmitter;

impl GovernanceEventEmitter for NoopEventEmitter {
    fn emit_domain_created(&self, _: &str, _: &str, _: &str) {}
    fn emit_proposal_created(&self, _: &str, _: &str, _: &str, _: &str, _: &str) {}
    fn emit_proposal_opened(&self, _: &str, _: &str, _: u64) {}
    fn emit_proposal_closed(&self, _: &str, _: &str, _: &str) {}
    fn emit_vote_cast(&self, _: &str, _: &str, _: &str, _: &str) {}
    fn emit_action_item_created(&self, _: &ActionItem) {}
    fn emit_meeting_scheduled(&self, _: &Meeting) {}
    fn emit_meeting_started(&self, _: &str, _: &str, _: u64) {}
    fn emit_meeting_ended(&self, _: &str, _: &str, _: u64) {}
}
