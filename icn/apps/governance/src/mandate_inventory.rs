//! Read-only standing of a proposal's stored mandates against its governance
//! record (ADR-0019 pre-deployment inventory).
//!
//! The receipt store enumerates mandates per proposal (storage); deciding what
//! a proposal's state means for that authority is governance meaning and lives
//! here. Nothing is written, deleted or chosen.

use icn_governance::proof::ProofOutcome;
use icn_governance::{ProposalId, ProposalOutcome, ProposalState};

use crate::receipt_backend::GovernanceReceiptBackend;
use crate::state_store::GovernanceStateStore;

/// Where a proposal's stored mandates stand against the proposal's governance
/// record. Report only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MandateStanding {
    /// The proposal is in an accepted terminal state and every mandate is
    /// bound to the decision hash its record names (the governance receipt,
    /// or a pending close intent).
    Consistent,
    /// The proposal is accepted but no stored decision hash is available to
    /// compare against (e.g. a forced accept, which stores no v1 receipt).
    AcceptedUnverified,
    /// The proposal is accepted but a mandate is bound to another decision.
    HashMismatch,
    /// A close of the proposal is durably decided (pending intent) but not
    /// yet completed; compared against the intent's decision hash.
    PendingClose,
    /// The proposal is not in an accepted terminal state (open, vetoed,
    /// rejected, ...) and has no pending accepted close, yet authority exists
    /// for it: an orphan, e.g. from a pre-intent preflight of an older binary.
    Orphan,
    /// The proposal record is missing from the state store.
    ProposalMissing,
}

fn accepted(state: &ProposalState) -> bool {
    matches!(
        state,
        ProposalState::Accepted { .. }
            | ProposalState::ForceClosed {
                outcome: ProposalOutcome::Accepted,
                ..
            }
    )
}

/// Classify the standing of `proposal_id`'s mandates, bound to the hex
/// decision hashes `mandate_decision_hashes`, against the proposal record,
/// its close intent (`state`) and its governance receipt (`receipts`).
///
/// Returns the standing and the hex decision hash the governance record
/// names, if any.
pub fn classify_mandate_standing(
    proposal_id: &str,
    mandate_decision_hashes: &[String],
    state: &dyn GovernanceStateStore,
    receipts: &dyn GovernanceReceiptBackend,
) -> Result<(MandateStanding, Option<String>), String> {
    let pid = ProposalId(proposal_id.to_string());
    let proposal = state
        .get_proposal(&pid)
        .map_err(|e| format!("read proposal '{}': {e}", pid.0))?;
    let intent = state
        .get_close_intent(&pid)
        .map_err(|e| format!("read close intent '{}': {e}", pid.0))?;
    let receipt_hash = receipts
        .get_governance_by_proposal(&pid.0)?
        .filter(|r| matches!(r.outcome, ProofOutcome::Accepted))
        .map(|r| hex::encode(r.decision_hash));
    let matches_all = |h: &str| mandate_decision_hashes.iter().all(|x| x == h);
    Ok(match (&proposal, &intent) {
        (None, _) => (MandateStanding::ProposalMissing, None),
        (Some(_), Some(i)) if accepted(&i.proposal.state) => {
            let h = i.governance_decision_hash.clone();
            match &h {
                Some(h) if !matches_all(h) => (MandateStanding::HashMismatch, Some(h.clone())),
                _ => (MandateStanding::PendingClose, h),
            }
        }
        (Some(p), _) if accepted(&p.state) => match &receipt_hash {
            Some(h) if matches_all(h) => (MandateStanding::Consistent, Some(h.clone())),
            Some(h) => (MandateStanding::HashMismatch, Some(h.clone())),
            None => (MandateStanding::AcceptedUnverified, None),
        },
        (Some(_), _) => (MandateStanding::Orphan, receipt_hash),
    })
}
