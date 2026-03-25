//! Domain-owned event emitter trait for governance HTTP handlers.
//!
//! `GovernanceEventEmitter` is implemented by gateway's `GatewayEventAdapter`
//! (which forwards to `EventBroadcaster`) and by `NoopEventEmitter` for
//! testing / standalone mode.

/// Emitter for the five governance lifecycle events.
///
/// Implement this in the hosting process (e.g. gateway) and pass it into
/// [`configure`] so handlers can broadcast without depending on gateway types.
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
}
