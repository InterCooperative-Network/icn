//! ExecutionReceiptGate — guards ledger allocations behind verified governance decisions.
//!
//! Invariant 7: every resource-allocation journal entry must be linked to a
//! `GovernanceDecisionReceipt` with outcome `Accepted` and a verified decision hash.
//!
//! # Usage
//!
//! ```rust,ignore
//! use icn_governance::gate::{check_execution_gate, GateError};
//!
//! let receipt = GovernanceDecisionReceipt::new(...);
//! check_execution_gate(&receipt)?;                     // gate enforced here
//! let allocation = create_budget_allocation(receipt.decision_hash, ...);
//! ```
//!
//! The gate is intentionally **pure and stateless** — it performs no I/O and
//! holds no mutable state.  The double-execution guard (preventing re-use of a
//! decision hash that was already executed) is handled separately by
//! `ExecutionRecord::is_terminal()` in the execution store.
//!
//! # Why not `InvariantGate`?
//!
//! [`crate::invariant_gate::InvariantGate`] checks `EffectManifest` + `PowerDiff`
//! *before* a governance decision is made (Invariants 4–6, proposal phase).
//! This gate operates *after* the vote, on the resulting `GovernanceDecisionReceipt`,
//! guarding the ledger allocation write-path.  Different pipeline stage, different
//! input type — intentionally separate.

use crate::{proof::ProofOutcome, GovernanceDecisionReceipt};
use thiserror::Error;

/// Reasons a governance decision receipt was rejected by the gate.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum GateError {
    /// The receipt's outcome is not `Accepted`; allocations require acceptance.
    #[error("decision outcome is {actual:?}, expected Accepted")]
    OutcomeNotAccepted { actual: ProofOutcome },

    /// The stored `decision_hash` does not match the receipt's canonical fields.
    ///
    /// This indicates the receipt was tampered with or corrupted after construction.
    #[error("decision_hash verification failed — receipt fields do not match stored hash")]
    DecisionHashInvalid,
}

/// Verify that a `GovernanceDecisionReceipt` satisfies the execution gate.
///
/// Returns `Ok(())` only when:
/// 1. `receipt.outcome == Accepted`
/// 2. `receipt.verify()` passes (hash is internally consistent)
///
/// # Errors
///
/// Returns `Err(GateError::OutcomeNotAccepted)` if the vote was rejected or
/// lacked quorum, and `Err(GateError::DecisionHashInvalid)` if the receipt's
/// internal hash is inconsistent.
///
/// # TODO
///
/// Wire this call into the ledger allocation write-path before creating any
/// budget allocation entry (e.g. `icn_ledger::BudgetAllocator`).  Until that
/// integration lands, Invariant 7 is defined but not enforced on the hot path.
pub fn check_execution_gate(receipt: &GovernanceDecisionReceipt) -> Result<(), GateError> {
    if receipt.outcome != ProofOutcome::Accepted {
        return Err(GateError::OutcomeNotAccepted {
            actual: receipt.outcome,
        });
    }

    if !receipt.verify() {
        return Err(GateError::DecisionHashInvalid);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GovernanceDecisionReceipt, VoteTally};

    fn accepted_receipt() -> GovernanceDecisionReceipt {
        GovernanceDecisionReceipt::new(
            "prop-001".to_string(),
            "domain-tools".to_string(),
            ProofOutcome::Accepted,
            VoteTally {
                for_votes: 3,
                against_votes: 0,
                abstain_votes: 0,
            },
            &[],
        )
    }

    /// Gate passes for a valid Accepted receipt.
    #[test]
    fn test_gate_passes_accepted() {
        let receipt = accepted_receipt();
        assert!(check_execution_gate(&receipt).is_ok());
    }

    /// Gate rejects a Rejected outcome.
    #[test]
    fn test_gate_rejects_rejected_outcome() {
        let receipt = GovernanceDecisionReceipt::new(
            "prop-002".to_string(),
            "domain-tools".to_string(),
            ProofOutcome::Rejected,
            VoteTally {
                for_votes: 0,
                against_votes: 3,
                abstain_votes: 0,
            },
            &[],
        );
        let err = check_execution_gate(&receipt).unwrap_err();
        assert_eq!(
            err,
            GateError::OutcomeNotAccepted {
                actual: ProofOutcome::Rejected
            }
        );
    }

    /// Gate rejects a NoQuorum outcome.
    #[test]
    fn test_gate_rejects_no_quorum() {
        let receipt = GovernanceDecisionReceipt::new(
            "prop-003".to_string(),
            "domain-tools".to_string(),
            ProofOutcome::NoQuorum,
            VoteTally {
                for_votes: 0,
                against_votes: 0,
                abstain_votes: 1,
            },
            &[],
        );
        let err = check_execution_gate(&receipt).unwrap_err();
        assert_eq!(
            err,
            GateError::OutcomeNotAccepted {
                actual: ProofOutcome::NoQuorum
            }
        );
    }

    /// Gate catches a tampered decision hash.
    ///
    /// Mutating any field after construction causes `verify()` to fail because
    /// the stored `decision_hash` no longer matches the recomputed value.
    #[test]
    fn test_gate_rejects_tampered_hash() {
        let mut receipt = accepted_receipt();
        // Mutate the proposal_id — the decision_hash is now stale.
        receipt.proposal_id = "evil-prop".to_string();

        let err = check_execution_gate(&receipt).unwrap_err();
        assert_eq!(err, GateError::DecisionHashInvalid);
    }

    /// Error messages are human-readable.
    #[test]
    fn test_error_display() {
        let err = GateError::OutcomeNotAccepted {
            actual: ProofOutcome::Rejected,
        };
        let msg = err.to_string();
        assert!(msg.contains("Accepted"), "expected 'Accepted' in: {msg}");

        let err2 = GateError::DecisionHashInvalid;
        let msg2 = err2.to_string();
        assert!(
            msg2.contains("decision_hash"),
            "expected 'decision_hash' in: {msg2}"
        );
    }
}
