//! Abstraction over gateway's ReceiptStore so GovernanceManager can live
//! in this crate without a circular dependency on icn-gateway.

use icn_governance::{
    ActionItemCompletionReceipt, AuthorityGrant, AuthorityGrantId, GovernanceDecisionReceipt,
    Grantee, Mandate, MeetingAttendanceReceipt, ProcessGateKind, ProcessGateResultReceipt,
    Timestamp,
};
use icn_kernel_api::{AllocationReceipt, Hash};

use crate::dispatch_evidence::EffectDispatchEvidence;
use crate::institutional_effect::InstitutionalEffectRecord;

/// Class string for `ProcessGateResultReceipt` opaque storage. Chosen
/// in the apps layer so the gateway never sees the typed name.
const PROCESS_GATE_RESULT_CLASS: &str = "process_gate_result";

/// Stable string mapping for `ProcessGateKind` used as the
/// opaque-storage `key2`. Distinct from serde wire form so the
/// storage key is unambiguous and free of any future enum-rename
/// risk on the wire.
fn process_gate_kind_key(kind: ProcessGateKind) -> &'static str {
    match kind {
        ProcessGateKind::PrivacyReview => "privacy_review",
        ProcessGateKind::AccessibilityReview => "accessibility_review",
        ProcessGateKind::RepoSafetyReview => "repo_safety_review",
        ProcessGateKind::ScopeConfirmation => "scope_confirmation",
        ProcessGateKind::NoMutationCheck => "no_mutation_check",
        ProcessGateKind::SecondReviewerSignoff => "second_reviewer_signoff",
    }
}

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
    /// Backends that store multiple receipts per item (e.g. for a
    /// reopen/re-complete cycle that produces an append-only chain of
    /// completions) must return the receipt with the largest
    /// `completed_at`.
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

    /// List **all** [`ActionItemCompletionReceipt`]s ever persisted for an
    /// action item id, oldest-first by `completed_at`.
    ///
    /// The append-only contract: a reopen/re-complete cycle on the same
    /// item produces a new receipt; previous receipts are not
    /// overwritten. This method exposes the full chain so audits can see
    /// every completion event for the item.
    ///
    /// Default impl returns an empty vector. Backends that override
    /// [`Self::put_action_item_completion`] should also override this
    /// method to keep the audit chain readable.
    fn list_action_item_completions_by_item(
        &self,
        _item_id: &str,
    ) -> Result<Vec<ActionItemCompletionReceipt>, String> {
        Ok(vec![])
    }

    /// Persist a [`MeetingAttendanceReceipt`] emitted when an attendee
    /// transitions into `Present` or `Remote` via an authenticated
    /// caller.
    ///
    /// Append-only: implementations should treat a same-`record_hash`
    /// re-write as idempotent and must not overwrite prior receipts on
    /// repeated transitions for the same `(meeting_id, attendee_did)`
    /// pair.
    ///
    /// Default impl is a no-op so backends that do not yet durably
    /// persist these receipts inherit a truthful "attendance receipt not
    /// persisted" behavior. Test backends and the sled-backed
    /// [`ReceiptStore`](icn_gateway::receipt_store::ReceiptStore)
    /// override.
    fn put_meeting_attendance(&self, _receipt: &MeetingAttendanceReceipt) -> Result<(), String> {
        Ok(())
    }

    /// Retrieve the latest [`MeetingAttendanceReceipt`] for a
    /// `(meeting_id, attendee_did)` pair, or `None` when no attendance
    /// has been recorded for that attendee in that meeting.
    ///
    /// Backends that store multiple receipts per pair (e.g. a
    /// `Present → Remote` transition appending a fresh record) must
    /// return the receipt with the largest `recorded_at`.
    ///
    /// Default impl returns `Ok(None)` so backends that do not implement
    /// attendance-receipt storage are indistinguishable from "no
    /// attendance recorded".
    fn get_meeting_attendance(
        &self,
        _meeting_id: &str,
        _attendee_did: &str,
    ) -> Result<Option<MeetingAttendanceReceipt>, String> {
        Ok(None)
    }

    /// List **all** [`MeetingAttendanceReceipt`]s persisted for a
    /// meeting, oldest-first by `recorded_at`. Spans every attendee.
    ///
    /// Default impl returns an empty vector. Backends that override
    /// [`Self::put_meeting_attendance`] should also override this method
    /// to keep the per-meeting audit chain readable.
    fn list_meeting_attendance_for_meeting(
        &self,
        _meeting_id: &str,
    ) -> Result<Vec<MeetingAttendanceReceipt>, String> {
        Ok(vec![])
    }

    /// Persist a [`ProcessGateResultReceipt`] emitted when the runtime
    /// records a single named process gate result for a process
    /// session.
    ///
    /// First `ProcessTransitionReceipt` class (per the `idea-0019`
    /// Institutional Process Substrate framing brief) emitted by the
    /// runtime. Append-only: implementations should treat a same-
    /// `record_hash` re-write as idempotent and must not overwrite
    /// prior receipts on repeated transitions for the same
    /// `(session_id, gate_kind)` pair.
    ///
    /// **Default routes through opaque storage.** As of Stage 1d of
    /// the architecture orchestration plan, the default body
    /// serialises the typed receipt to JSON and delegates to
    /// [`Self::put_opaque`] under class `"process_gate_result"` with
    /// `key1 = receipt.session_id`,
    /// `key2 = Some(stringified gate_kind)`. A backend that
    /// implements opaque storage (e.g. the production gateway-backed
    /// `ReceiptStore` via the Stage 1b override) gets durable
    /// persistence for free. A backend that does NOT implement
    /// opaque storage falls back to the opaque method's own
    /// fail-closed default (`opaque_storage_not_implemented`), so
    /// the persist-or-error semantics first introduced in #1755 are
    /// preserved end-to-end. Backends that want custom typed
    /// persistence (e.g. an in-memory test store with extra
    /// counters) can still override this method directly.
    fn put_process_gate_result(&self, receipt: &ProcessGateResultReceipt) -> Result<(), String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize ProcessGateResultReceipt: {e}"))?;
        self.put_opaque(
            PROCESS_GATE_RESULT_CLASS,
            &receipt.session_id,
            Some(process_gate_kind_key(receipt.gate_kind)),
            receipt.recorded_at,
            receipt.record_hash,
            &payload,
        )
    }

    /// Retrieve the latest [`ProcessGateResultReceipt`] for a
    /// `(session_id, gate_kind)` pair, or `None` when no result has
    /// been recorded for that gate in that session.
    ///
    /// Backends that store multiple receipts per pair (a re-record at
    /// a strictly later second appending a fresh receipt) must return
    /// the receipt with the largest `recorded_at`. Implementations
    /// must compute the latest by `recorded_at`, not by insertion
    /// order — out-of-order writes (e.g. a backend that batches or
    /// reorders inserts) must still return the highest-`recorded_at`
    /// receipt for the pair.
    ///
    /// **Default routes through opaque storage.** Same convention as
    /// [`Self::put_process_gate_result`]: class
    /// `"process_gate_result"`, `key1 = session_id`,
    /// `key2 = Some(stringified gate_kind)`. The opaque store's
    /// `get_latest_opaque` already returns the entry with the
    /// largest `recorded_at` (per its trait contract), so the
    /// "max-recorded-at" semantics here come for free. The opaque
    /// payload is JSON-deserialised back into the typed receipt.
    fn get_latest_process_gate_result(
        &self,
        session_id: &str,
        gate_kind: ProcessGateKind,
    ) -> Result<Option<ProcessGateResultReceipt>, String> {
        let payload = self.get_latest_opaque(
            PROCESS_GATE_RESULT_CLASS,
            session_id,
            Some(process_gate_kind_key(gate_kind)),
        )?;
        match payload {
            Some(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize ProcessGateResultReceipt: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// List **all** [`ProcessGateResultReceipt`]s persisted for a
    /// process session, oldest-first by `recorded_at`. Spans every
    /// gate kind for that session.
    ///
    /// **Default routes through opaque storage.** Class
    /// `"process_gate_result"`, `key1 = session_id`. The opaque
    /// store's `list_opaque_for` returns payloads ordered ascending
    /// by `recorded_at` across all `key2` values (i.e. across every
    /// gate kind for that session) — the chronological audit-chain
    /// shape callers expect. Each payload is JSON-deserialised back
    /// into the typed receipt; a deserialise failure aborts the
    /// list with an error rather than silently dropping the bad
    /// entry.
    fn list_process_gate_results_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<ProcessGateResultReceipt>, String> {
        let payloads = self.list_opaque_for(PROCESS_GATE_RESULT_CLASS, session_id)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            let r: ProcessGateResultReceipt = serde_json::from_slice(&bytes)
                .map_err(|e| format!("deserialize ProcessGateResultReceipt: {e}"))?;
            out.push(r);
        }
        Ok(out)
    }

    // ========================================================================
    // Opaque storage primitive (Stage 1b of architecture orchestration plan)
    //
    // The gateway-side ReceiptStore exposes a meaning-blind storage shape:
    // store payloads under (class, key1, key2_opt) keyed by record_hash and
    // ordered by recorded_at. The runtime layer (this crate) owns the typed
    // envelope and the serde adapter; the gateway sees only opaque bytes.
    //
    // These trait methods exist so a `GovernanceReceiptBackend` can be
    // routed through opaque storage WITHOUT adding new typed receipt types
    // to the gateway's existing imports — the meaning-firewall ratchet
    // stays where it is.
    //
    // Stage 1c will rewire the typed default impls above to delegate
    // through these opaque methods, so adding a new receipt class becomes
    // a one-file change in apps with no gateway touch. Stage 1d will then
    // flip ProcessGateResultReceipt's fail-closed default to a routing
    // default that uses opaque storage.
    //
    // Default impls below are FAIL-CLOSED. A backend that does not opt
    // in to opaque storage surfaces the gap as an explicit error (stable
    // sentinel `opaque_storage_not_implemented`) rather than as a silent
    // commit-without-persistence. Same discipline as the existing
    // `put_process_gate_result` default landed in #1755.
    // ========================================================================

    /// Persist an opaque receipt payload under a `(class, key1, key2_opt)`
    /// key, tagged with the receipt's canonical `record_hash` and
    /// ordered by `recorded_at`. See the gateway-side
    /// `ReceiptStore::put_opaque` docstring for the layout contract;
    /// this trait method is the indirection so the runtime layer can
    /// call into the gateway storage without typing the receipt at the
    /// boundary.
    ///
    /// **Fail-closed default.** A backend that has not opted in to
    /// opaque storage returns `Err` with stable sentinel
    /// `opaque_storage_not_implemented`. Callers must either handle
    /// the error or wire a backend that overrides this method.
    fn put_opaque(
        &self,
        _class: &str,
        _key1: &str,
        _key2: Option<&str>,
        _recorded_at: u64,
        _record_hash: [u8; 32],
        _payload: &[u8],
    ) -> Result<(), String> {
        Err("opaque_storage_not_implemented: \
             this backend inherits the fail-closed default for \
             put_opaque. Override the method to opt in to durable \
             opaque storage, or handle this error at the caller."
            .to_string())
    }

    /// Retrieve the latest opaque payload for a
    /// `(class, key1, key2_opt)` triple — the entry with the largest
    /// `recorded_at`. Returns `Ok(None)` when the backend supports
    /// opaque storage but has no entry for the triple. Returns
    /// `Err(opaque_storage_not_implemented)` from the default impl
    /// for backends that have not opted in.
    fn get_latest_opaque(
        &self,
        _class: &str,
        _key1: &str,
        _key2: Option<&str>,
    ) -> Result<Option<Vec<u8>>, String> {
        Err("opaque_storage_not_implemented: \
             this backend inherits the fail-closed default for \
             get_latest_opaque. Override the method to opt in to \
             durable opaque storage, or handle this error at the \
             caller."
            .to_string())
    }

    /// List all opaque payloads under a given `(class, key1)`
    /// regardless of `key2`, oldest-first by `recorded_at`. Returns
    /// `Err(opaque_storage_not_implemented)` from the default impl
    /// for backends that have not opted in.
    fn list_opaque_for(&self, _class: &str, _key1: &str) -> Result<Vec<Vec<u8>>, String> {
        Err("opaque_storage_not_implemented: \
             this backend inherits the fail-closed default for \
             list_opaque_for. Override the method to opt in to \
             durable opaque storage, or handle this error at the \
             caller."
            .to_string())
    }
}
