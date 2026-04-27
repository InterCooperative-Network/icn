//! Abstraction over gateway's ReceiptStore so GovernanceManager can live
//! in this crate without a circular dependency on icn-gateway.

use icn_governance::{
    ActionItemCompletionReceipt, AuthorityGrant, AuthorityGrantId, GovernanceDecisionReceipt,
    Grantee, Mandate, Timestamp,
};
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
    /// **Override status:** The in-memory test backend and the
    /// production sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore) both
    /// override. The sled override uses a single transaction covering
    /// the mandate primary record and both the proposal and decision
    /// secondary indexes, so on-disk index skew from a partial write
    /// cannot happen. The default no-op here is only reached by
    /// backends that have not opted in to mandate storage; the
    /// acceptance seam treats such backends as having no durable
    /// mandate store and records the mandate in tracing logs only.
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

    /// Atomically persist a mandate and the authority grants it
    /// references, or leave the store unchanged.
    ///
    /// This is the **canonical write path** for the ADR-0014 acceptance
    /// seam when a derived grant set is non-empty: it exists so
    /// mandate→grant linkage cannot observe a partial-failure state
    /// where grants are durable but the mandate is not (orphan grants)
    /// or vice versa.
    ///
    /// **Default impl:** sequential per-grant `put_authority_grant` +
    /// read-after-write verification, then `put_mandate`. If any grant's
    /// round-trip fails (e.g. the backend inherits the no-op default
    /// for either put or get), the method returns an error whose string
    /// begins with the sentinel `grant_durability_not_supported` so the
    /// seam can recognize the case and fall back to a pending-grants
    /// mandate instead of leaving orphan grants. The default is **not
    /// atomic** across the full set of writes: if `put_authority_grant`
    /// has already written earlier grants and `put_mandate` then fails,
    /// the earlier grants are durable orphans. Backends that need true
    /// atomicity (the gateway sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore))
    /// **must override** this method with a real transaction.
    ///
    /// Callers must not assume per-grant writes landed individually when
    /// this method returns an error — the seam handles that invariant by
    /// recording a pending-grants mandate on the sentinel error.
    fn put_mandate_with_grants(
        &self,
        mandate: &Mandate,
        grants: &[AuthorityGrant],
    ) -> Result<(), String> {
        for grant in grants {
            self.put_authority_grant(grant)?;
            if self.get_authority_grant(&grant.id)?.is_none() {
                return Err(format!(
                    "grant_durability_not_supported: backend did not round-trip grant {}",
                    grant.id
                ));
            }
        }
        self.put_mandate(mandate)
    }

    /// Persist an [`AuthorityGrant`] minted at proposal acceptance time.
    ///
    /// Grants are derived by [`crate::grant_minting`] from a narrow,
    /// truthful subset of accepted proposal classes (today:
    /// steward-appointment and steward-reconfirmation). The grant sits
    /// on the **authorization** side of the chain, composed into the
    /// [`Mandate`] that records the underlying decision's bounded
    /// authority. It is **distinct from** downstream evidence records
    /// ([`InstitutionalEffectRecord`], [`EffectDispatchEvidence`]).
    ///
    /// Append-only; implementations treat a same-[`AuthorityGrantId`]
    /// re-write as idempotent. Default impl is a no-op so downstream
    /// backends can opt in without breaking.
    ///
    /// **Seam contract:** because the defaulted no-op returns `Ok(())`
    /// without actually storing anything, the ADR-0014 mandate seam in
    /// [`crate::grant_minting::mint_and_persist_for_accepted`] verifies
    /// each write with a follow-up [`Self::get_authority_grant`] call.
    /// Backends that opt into grant storage **must override both**
    /// `put_authority_grant` and `get_authority_grant` so the
    /// read-after-write check round-trips; otherwise the seam treats
    /// the grant as unpersisted and the mandate falls back to
    /// pending-grants semantics rather than referencing a grant ID that
    /// cannot be retrieved.
    ///
    /// **Override status:** The in-memory test backend and the
    /// production sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore) both
    /// override. The sled override uses a single transaction covering
    /// the grant primary record and (when `granted_by` is set) the
    /// decision-hash secondary index, so on-disk index skew cannot
    /// happen. The default no-op here is only reached by backends that
    /// have not opted in to grant storage; the acceptance seam's
    /// read-after-write check and the
    /// [`Self::put_mandate_with_grants`] sentinel ensure a non-durable
    /// grant never ends up referenced by a strict mandate.
    fn put_authority_grant(&self, _grant: &AuthorityGrant) -> Result<(), String> {
        Ok(())
    }

    /// Retrieve an [`AuthorityGrant`] by its stable identifier, if any.
    fn get_authority_grant(
        &self,
        _grant_id: &AuthorityGrantId,
    ) -> Result<Option<AuthorityGrant>, String> {
        Ok(None)
    }

    /// Retrieve all [`AuthorityGrant`]s whose `granted_by` provenance
    /// matches the given decision hash, oldest-first by `valid_from`.
    ///
    /// Returns `Ok(vec![])` when no grants exist or when the backend
    /// does not implement grant storage (default).
    fn list_authority_grants_by_decision(
        &self,
        _decision_hash: &Hash,
    ) -> Result<Vec<AuthorityGrant>, String> {
        Ok(vec![])
    }

    /// List authority grants for a [`Grantee`] that are active at `now`.
    ///
    /// "Active" is defined by [`AuthorityGrant::is_active_at`]: not
    /// revoked at or before `now`, `now >= valid_from`, and (if
    /// `valid_until` is set) `now < valid_until`.
    ///
    /// Used by the ADR-0014 acceptance seam to resolve a target steward
    /// or authority DID to its outstanding grants for revocation. The
    /// default impl returns an empty list so backends that do not
    /// durably persist grants produce the truthful "no grants to
    /// revoke" answer; the sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore)
    /// overrides via a dedicated by-grantee secondary index.
    fn list_active_authority_grants_by_grantee(
        &self,
        _grantee: &Grantee,
        _now: Timestamp,
    ) -> Result<Vec<AuthorityGrant>, String> {
        Ok(vec![])
    }

    /// List **all** authority grants ever issued to a [`Grantee`],
    /// including revoked and expired ones, ordered oldest-first by
    /// `valid_from`.
    ///
    /// Unlike [`Self::list_active_authority_grants_by_grantee`], this
    /// method does not filter on `is_active_at`: the reinstatement seam
    /// needs to inspect previously-revoked grants to clone class/scope/
    /// grantor context for the fresh grant. Uses the same by-grantee
    /// secondary index as the active-filtered variant; no new index is
    /// introduced.
    ///
    /// Default impl returns an empty list.
    fn list_authority_grants_by_grantee(
        &self,
        _grantee: &Grantee,
    ) -> Result<Vec<AuthorityGrant>, String> {
        Ok(vec![])
    }

    /// Revoke an [`AuthorityGrant`] by stamping `revoked_at` on its
    /// primary record.
    ///
    /// **Seam contract:**
    /// - First-write-wins: a grant already carrying `revoked_at: Some(_)`
    ///   is a no-op; the recorded timestamp never moves. This keeps
    ///   double-revocation safe and prevents a later proposal from
    ///   silently rewriting the original revocation time.
    /// - Missing grants are an error (`grant_not_found: …`). The
    ///   acceptance seam logs and continues rather than aborting the
    ///   entire decision.
    ///
    /// **Default impl:** no-op `Ok(())` so backends that do not durably
    /// persist grants inherit the truthful "revocation not persisted"
    /// behavior. Callers that need a real write-ack must use a backend
    /// that overrides this method (the sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore)
    /// overrides it with a transactional primary-record update).
    fn revoke_authority_grant(
        &self,
        _grant_id: &AuthorityGrantId,
        _revoked_at: Timestamp,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Persist an [`ActionItemCompletionReceipt`] emitted when an action
    /// item transitions to `Completed` via an authorized actor (assignee
    /// or creator).
    ///
    /// Append-only: implementations should treat a same-`item_id`
    /// re-write as idempotent. The runtime's
    /// `update_action_item_status` path emits at most one receipt per
    /// transition and only for transitions listed in
    /// [`icn_governance::ActionItemTransition`].
    ///
    /// Default impl is a no-op so backends that do not yet durably
    /// persist these receipts inherit a truthful "completion receipt
    /// not persisted" behavior. Test backends and the sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore)
    /// override.
    fn put_action_item_completion(
        &self,
        _receipt: &ActionItemCompletionReceipt,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Retrieve the latest [`ActionItemCompletionReceipt`] for an action
    /// item id (string form of `ActionItemId`), or `None` when no
    /// completion has been recorded.
    ///
    /// Default impl returns `Ok(None)` so backends that do not implement
    /// completion-receipt storage are indistinguishable from "no
    /// completion recorded". Callers that need to assert a receipt
    /// exists must use a backend that overrides this method.
    fn get_action_item_completion_by_item(
        &self,
        _item_id: &str,
    ) -> Result<Option<ActionItemCompletionReceipt>, String> {
        Ok(None)
    }
}
