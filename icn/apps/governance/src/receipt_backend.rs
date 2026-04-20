//! Abstraction over gateway's ReceiptStore so GovernanceManager can live
//! in this crate without a circular dependency on icn-gateway.

use icn_governance::{GovernanceDecisionReceipt, Mandate};
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

    /// Persist a [`Mandate`] minted at proposal acceptance time.
    ///
    /// A mandate is the constitutional mediation layer between an
    /// accepted decision and its execution (ADR-0014). It is
    /// **upstream** of evidence-side records (`InstitutionalEffectRecord`,
    /// `EffectDispatchEvidence`) and **distinct** from them: a mandate
    /// records *authority to act*, evidence records document *what was
    /// done*.
    ///
    /// Append-only; implementations treat a same-`MandateId` re-write as
    /// idempotent. Default impl is a no-op so downstream backends can
    /// opt in without breaking.
    ///
    /// **Override status (ADR-0014 bootstrap):** The in-memory test
    /// backend overrides. The production sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore) does
    /// **not** yet override — a follow-up tranche is required to add
    /// sled column families for mandate storage. Until then, the
    /// gateway's acceptance path records the mandate in tracing logs
    /// (via the `GovernanceManager` error path when a backend returns
    /// an error) but the sled backend simply no-ops. Operators that
    /// need durable mandate records today must provide their own
    /// overriding backend.
    fn put_mandate(&self, _mandate: &Mandate) -> Result<(), String> {
        Ok(())
    }

    /// Retrieve the mandate minted for a proposal, if any.
    ///
    /// Returns `Ok(None)` when no mandate exists or when the backend
    /// does not implement mandate storage (default).
    fn get_mandate_by_proposal(&self, _proposal_id: &str) -> Result<Option<Mandate>, String> {
        Ok(None)
    }

    /// Retrieve all mandates anchored to a governance decision hash,
    /// oldest-first by `issued_at`.
    ///
    /// A single decision usually yields one mandate, but this returns a
    /// `Vec` to keep the API symmetric with
    /// [`Self::list_allocations_by_decision`] and to permit future
    /// multi-mandate decisions without a migration.
    fn list_mandates_by_decision(&self, _decision_hash: &Hash) -> Result<Vec<Mandate>, String> {
        Ok(vec![])
    }
}
