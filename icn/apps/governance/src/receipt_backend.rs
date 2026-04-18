//! Abstraction over gateway's ReceiptStore so GovernanceManager can live
//! in this crate without a circular dependency on icn-gateway.

use icn_governance::GovernanceDecisionReceipt;
use icn_kernel_api::{AllocationReceipt, Hash};

use crate::dispatch_evidence::EffectDispatchEvidence;
use crate::institutional_effect::InstitutionalEffectRecord;

/// Minimal receipt-storage interface required by [`GovernanceManager`].
///
/// Gateway implements this for its [`ReceiptStore`]; tests can provide a
/// simple in-memory stand-in.
///
/// [`GovernanceManager`]: crate::manager::GovernanceManager
/// [`ReceiptStore`]: icn_gateway::receipt_store::ReceiptStore
pub trait GovernanceReceiptBackend: Send + Sync {
    /// Persist a governance decision receipt.
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String>;

    /// Retrieve a governance decision receipt by proposal ID.
    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String>;

    /// Persist an allocation receipt linked to a governance decision.
    ///
    /// Called by the governance manager when a budget/treasury proposal
    /// is accepted, creating the governance→economics binding.
    fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String>;

    /// Retrieve a governance decision receipt by decision hash (cross-node canonical).
    fn get_governance_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String>;

    /// Retrieve all allocation receipts linked to a governance decision.
    fn list_allocations_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String>;

    /// Persist an institutional effect record emitted at proposal acceptance.
    ///
    /// Called once per accepted proposal whose payload translates to a
    /// structured `GovernanceEffect` variant (i.e. not `Unhandled`). The
    /// backend is append-only — callers must not attempt to update an
    /// existing record. Implementations should be tolerant of benign
    /// duplicate writes (same `record_id`) but should treat differing
    /// records with the same `record_id` as an error.
    ///
    /// Default impl is a no-op so downstream backends can opt in without
    /// breaking. The in-memory test backend and the production sled-backed
    /// `ReceiptStore` both override.
    fn put_institutional_effect(&self, _record: &InstitutionalEffectRecord) -> Result<(), String> {
        Ok(())
    }

    /// Retrieve all institutional effect records emitted for a proposal,
    /// oldest-first.
    ///
    /// Returns `Ok(vec![])` when no records exist or when the backend does
    /// not implement effect storage (default). Callers interpret an empty
    /// list as "no structured effect recorded" — which is indistinguishable
    /// from "backend not wired"; operators should check backend capability
    /// out-of-band when this matters.
    fn list_institutional_effects_by_proposal(
        &self,
        _proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(vec![])
    }

    /// Persist downstream dispatch evidence for a previously emitted
    /// institutional effect record. Append-only; implementations treat a
    /// same-`evidence_id` re-write as idempotent.
    ///
    /// Default impl is a no-op so downstream backends can opt in without
    /// breaking. The in-memory test backend and the sled-backed
    /// `ReceiptStore` both override.
    fn put_effect_dispatch_evidence(
        &self,
        _evidence: &EffectDispatchEvidence,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Retrieve all dispatch evidence for an effect record, oldest-first.
    /// Returns an empty list when no evidence exists or when the backend
    /// does not implement evidence storage.
    fn list_effect_dispatch_evidence_by_record(
        &self,
        _effect_record_id: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, String> {
        Ok(vec![])
    }
}
