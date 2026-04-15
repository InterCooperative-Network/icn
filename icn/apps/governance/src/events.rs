//! Domain-owned event emitter trait for governance HTTP handlers.
//!
//! `GovernanceEventEmitter` is implemented by gateway's `GatewayEventAdapter`
//! (which forwards to `EventBroadcaster`) and by `NoopEventEmitter` for
//! testing / standalone mode.

/// Emitter for governance lifecycle events.
///
/// All methods take primitive arguments (strings and integers) so that
/// kernel-layer implementations (`GatewayEventAdapter` in `icn-gateway`)
/// do not need to import domain types from `icn-governance`.  Callers in
/// the app layer extract the relevant fields before calling the emitter.
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
    fn emit_action_item_created(
        &self,
        item_id: &str,
        domain_id: &str,
        parent_proposal: Option<&str>,
        assignee: Option<&str>,
        created_at: u64,
    );

    /// Fire when a meeting is scheduled (`POST /gov/domains/{domain}/meetings`).
    fn emit_meeting_scheduled(
        &self,
        meeting_id: &str,
        domain_id: &str,
        title: &str,
        scheduled_at: Option<u64>,
        created_by: &str,
    );

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
    fn emit_action_item_created(&self, _: &str, _: &str, _: Option<&str>, _: Option<&str>, _: u64) {
    }
    fn emit_meeting_scheduled(&self, _: &str, _: &str, _: &str, _: Option<u64>, _: &str) {}
    fn emit_meeting_started(&self, _: &str, _: &str, _: u64) {}
    fn emit_meeting_ended(&self, _: &str, _: &str, _: u64) {}
}
