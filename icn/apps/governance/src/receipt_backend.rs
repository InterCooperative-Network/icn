//! Abstraction over gateway's ReceiptStore so GovernanceManager can live
//! in this crate without a circular dependency on icn-gateway.

use icn_governance::{
    ActionItemCompletionReceipt, ActionItemCompletionReceiptV2, ActivationCrossedReceipt,
    AuthorityGrant, AuthorityGrantId, DecisionRecordedReceipt, DeliberationEntryRecordedReceipt,
    EvidencePacketExportPreparedReceipt, EvidencePacketProducedReceipt, GovernanceDecisionReceipt,
    GovernanceDecisionReceiptV3, Grantee, Mandate, MeetingAttendanceReceipt,
    MeetingAttendanceReceiptV2, MutationAppliedReceipt, MutationPlanRecordedReceipt,
    ProcessGateKind, ProcessGateResultReceipt, ProcessSessionOpenedReceipt, Timestamp,
};
use icn_kernel_api::{AllocationReceipt, Hash};

use crate::dispatch_evidence::EffectDispatchEvidence;
use crate::institutional_effect::InstitutionalEffectRecord;

/// Class string for `ProcessGateResultReceipt` opaque storage. Chosen
/// in the apps layer so the gateway never sees the typed name.
const PROCESS_GATE_RESULT_CLASS: &str = "process_gate_result";

/// Class string for `ProcessSessionOpenedReceipt` opaque storage (#2275
/// session-anchor contract). Chosen in the apps layer so the gateway never
/// sees the typed name.
const PROCESS_SESSION_OPENED_CLASS: &str = "process_session_opened";

/// Outcome of persisting a [`ProcessSessionOpenedReceipt`] through
/// [`GovernanceReceiptBackend::put_process_session_opened`].
///
/// The backend enforces at most one opening per `(domain_id, session_id)`
/// **atomically at the storage layer** (per the #2275 contract). The caller
/// (the manager) turns `Existing` into either a same-opener idempotent
/// success or a different-opener fail-closed conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOpenedPersist {
    /// This call won the insert: the receipt is now the one persisted
    /// opening for its `(domain_id, session_id)`.
    Inserted,
    /// An opening already existed for this `(domain_id, session_id)`;
    /// nothing was written. Carries the original persisted receipt
    /// (original `opened_at` / `record_hash` — never restamped).
    Existing(ProcessSessionOpenedReceipt),
}

/// Class string for `DeliberationEntryRecordedReceipt` opaque storage
/// (#2277 contract + #2278 Q3 decision). Chosen in the apps layer so the
/// gateway never sees the typed name.
const DELIBERATION_ENTRY_RECORDED_CLASS: &str = "deliberation_entry_recorded";

/// Build the injective composite `key1` binding a deliberation entry to its
/// `(domain_id, session_id)` anchor in opaque storage.
///
/// Deliberation entries need three logical key dimensions
/// (`domain_id`, `session_id`, `entry_id`) over the opaque store's two-key
/// surface, so `key1` composes the anchor pair and `key2` carries
/// `entry_id`. The encoding is netstring-style — the decimal length of
/// `domain_id`, a colon, then the two ids concatenated — which is
/// **injective**: the length prefix delimits `domain_id` exactly, so
/// `("ab","c")` and `("a","bc")` can never produce the same key (bare
/// concatenation would alias them). This also makes cross-domain mixing
/// impossible by construction: two domains sharing a `session_id` scan
/// different `key1` prefixes.
pub fn deliberation_entry_composite_key1(domain_id: &str, session_id: &str) -> String {
    format!("{}:{domain_id}{session_id}", domain_id.len())
}

/// Outcome of persisting a [`DeliberationEntryRecordedReceipt`] through
/// [`GovernanceReceiptBackend::put_deliberation_entry_recorded`].
///
/// The backend enforces at most one entry per
/// `(domain_id, session_id, entry_id)` **atomically at the storage layer**
/// (same `put_opaque_if_absent` primitive as the #2276 session anchor). The
/// caller (the manager) turns `Existing` into either a same-identity
/// idempotent success or a fail-closed conflict — stable identity includes
/// `author`, `body_hash`, **and `entry_kind`** (per the #2278 decision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliberationEntryPersist {
    /// This call won the insert: the receipt is now the one persisted entry
    /// for its `(domain_id, session_id, entry_id)`.
    Inserted,
    /// An entry already existed for this triple; nothing was written.
    /// Carries the original persisted receipt (original `recorded_at` /
    /// `record_hash` — never restamped).
    Existing(DeliberationEntryRecordedReceipt),
}

/// Class string for `DecisionRecordedReceipt` opaque storage (#2280
/// contract + #2281 Q4 decision). Chosen in the apps layer so the gateway
/// never sees the typed name. Deliberately distinct from the proposal/vote
/// lineage's typed persistence and from `GOVERNANCE_DECISION_V3_CLASS` —
/// the two decision lineages never share stores.
const DECISION_RECORDED_CLASS: &str = "decision_recorded";

/// Build the injective composite `key1` binding a recorded decision to its
/// `(domain_id, session_id)` anchor in opaque storage.
///
/// Sibling of [`deliberation_entry_composite_key1`] with the identical
/// netstring-style encoding (decimal length of `domain_id`, a colon, then
/// the two ids concatenated), kept as a separate function so each class's
/// key derivation is independently anchored by its own anti-aliasing test.
/// The length prefix delimits `domain_id` exactly, so `("ab","c")` and
/// `("a","bc")` can never produce the same key, and two domains sharing a
/// `session_id` scan different `key1` prefixes.
pub fn decision_recorded_composite_key1(domain_id: &str, session_id: &str) -> String {
    format!("{}:{domain_id}{session_id}", domain_id.len())
}

/// Outcome of persisting a [`DecisionRecordedReceipt`] through
/// [`GovernanceReceiptBackend::put_decision_recorded`].
///
/// The backend enforces at most one decision per
/// `(domain_id, session_id, decision_id)` **atomically at the storage
/// layer** (same `put_opaque_if_absent` primitive as the sibling process
/// classes). The caller (the manager) turns `Existing` into either a
/// same-identity idempotent success or a fail-closed conflict — stable
/// identity is `recorded_by` + `body_hash` (per #2280 §4 as confirmed by
/// #2281; `recorded_at`/`record_hash` are NOT identity inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionRecordedPersist {
    /// This call won the insert: the receipt is now the one persisted
    /// decision for its `(domain_id, session_id, decision_id)`.
    Inserted,
    /// A decision already existed for this triple; nothing was written.
    /// Carries the original persisted receipt (original `recorded_at` /
    /// `record_hash` — never restamped).
    Existing(DecisionRecordedReceipt),
}

/// Class string for `ActivationCrossedReceipt` opaque storage (#2294
/// contract + #2295 B1/B2/B3 decision rung). Chosen in the apps layer so the
/// gateway never sees the typed name. Distinct from every other process
/// class store.
const ACTIVATION_CROSSED_CLASS: &str = "activation_crossed";

/// Build the injective composite `key1` binding an activation crossing to
/// its `(domain_id, session_id)` anchor in opaque storage.
///
/// Sibling of [`decision_recorded_composite_key1`] with the identical
/// netstring-style encoding (decimal length of `domain_id`, a colon, then
/// the two ids concatenated), kept as a separate function so this class's
/// key derivation is independently anchored by its own anti-aliasing test.
/// The length prefix delimits `domain_id` exactly, so `("ab","c")` and
/// `("a","bc")` can never produce the same key, and two domains sharing a
/// `session_id` scan different `key1` prefixes.
pub fn activation_crossed_composite_key1(domain_id: &str, session_id: &str) -> String {
    format!("{}:{domain_id}{session_id}", domain_id.len())
}

/// Outcome of persisting an [`ActivationCrossedReceipt`] through
/// [`GovernanceReceiptBackend::put_activation_crossed`].
///
/// The backend enforces at most one crossing per
/// `(domain_id, session_id, activation_id)` **atomically at the storage
/// layer** (same `put_opaque_if_absent` primitive as the sibling process
/// classes). The caller (the manager) turns `Existing` into either a
/// same-identity idempotent success or a fail-closed conflict — stable
/// identity is `decision_id` + `decision_record_hash` + `gate_basis` +
/// `crossed_by` (per the #2295 rung §7; `recorded_at`/`record_hash` are NOT
/// identity inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationCrossedPersist {
    /// This call won the insert: the receipt is now the one persisted
    /// crossing for its `(domain_id, session_id, activation_id)`.
    Inserted,
    /// A crossing already existed for this triple; nothing was written.
    /// Carries the original persisted receipt (original `recorded_at` /
    /// `record_hash` — never restamped). Boxed because
    /// [`ActivationCrossedReceipt`] is the largest process-receipt payload
    /// (five ids plus two 32-byte hashes), so an unboxed variant would make
    /// this enum needlessly large next to the unit `Inserted`.
    Existing(Box<ActivationCrossedReceipt>),
}

/// Class string for `MutationPlanRecordedReceipt` opaque storage (#2300
/// contract + #2302 M1/M2/M3 decision rung). Chosen in the apps layer so the
/// gateway never sees the typed name. Distinct from every other process
/// class store.
const MUTATION_PLAN_RECORDED_CLASS: &str = "mutation_plan_recorded";

/// Build the injective composite `key1` binding a recorded mutation plan to
/// its `(domain_id, session_id)` anchor in opaque storage.
///
/// Sibling of [`activation_crossed_composite_key1`] with the identical
/// netstring-style encoding (decimal length of `domain_id`, a colon, then
/// the two ids concatenated), kept as a separate function so this class's
/// key derivation is independently anchored by its own anti-aliasing test.
/// The length prefix delimits `domain_id` exactly, so `("ab","c")` and
/// `("a","bc")` can never produce the same key, and two domains sharing a
/// `session_id` scan different `key1` prefixes.
pub fn mutation_plan_recorded_composite_key1(domain_id: &str, session_id: &str) -> String {
    format!("{}:{domain_id}{session_id}", domain_id.len())
}

/// Outcome of persisting a [`MutationPlanRecordedReceipt`] through
/// [`GovernanceReceiptBackend::put_mutation_plan_recorded`].
///
/// The backend enforces at most one plan per `(domain_id, session_id,
/// plan_id)` **atomically at the storage layer** (same `put_opaque_if_absent`
/// primitive as the sibling process classes). The caller (the manager) turns
/// `Existing` into either a same-identity idempotent success or a fail-closed
/// conflict — stable identity is `activation_id` + `activation_record_hash` +
/// `recorded_by` + `body_hash` (per the #2302 rung §7; `recorded_at`/
/// `record_hash` are NOT identity inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationPlanRecordedPersist {
    /// This call won the insert: the receipt is now the one persisted plan
    /// for its `(domain_id, session_id, plan_id)`.
    Inserted,
    /// A plan already existed for this triple; nothing was written. Carries
    /// the original persisted receipt (original `recorded_at` / `record_hash`
    /// — never restamped). Boxed because [`MutationPlanRecordedReceipt`] is a
    /// large payload (five string fields plus three 32-byte hashes —
    /// `activation_record_hash`, `body_hash`, and `record_hash`), so an
    /// unboxed variant would make this enum needlessly large next to the unit
    /// `Inserted`.
    Existing(Box<MutationPlanRecordedReceipt>),
}

/// Class string for `MutationAppliedReceipt` opaque storage (#2307 contract +
/// #2308 A1/A2/A3/A4 decision rung). Chosen in the apps layer so the gateway
/// never sees the typed name. Distinct from every other process class store.
const MUTATION_APPLIED_CLASS: &str = "mutation_applied";

/// Build the injective composite `key1` binding a recorded mutation
/// application to its `(domain_id, session_id)` anchor in opaque storage.
///
/// Sibling of [`mutation_plan_recorded_composite_key1`] with the identical
/// netstring-style encoding (decimal length of `domain_id`, a colon, then the
/// two ids concatenated), kept as a separate function so this class's key
/// derivation is independently anchored by its own anti-aliasing test. The
/// length prefix delimits `domain_id` exactly, so `("ab","c")` and `("a","bc")`
/// can never produce the same key, and two domains sharing a `session_id` scan
/// different `key1` prefixes.
pub fn mutation_applied_composite_key1(domain_id: &str, session_id: &str) -> String {
    format!("{}:{domain_id}{session_id}", domain_id.len())
}

/// Outcome of persisting a [`MutationAppliedReceipt`] through
/// [`GovernanceReceiptBackend::put_mutation_applied`].
///
/// The backend enforces at most one application per `(domain_id, session_id,
/// application_id)` **atomically at the storage layer** (same
/// `put_opaque_if_absent` primitive as the sibling process classes). The caller
/// (the manager) turns `Existing` into either a same-identity idempotent
/// success or a fail-closed conflict — stable identity is `plan_id` +
/// `plan_record_hash` + `applied_by` + `result_hash` (per the #2308 rung §8;
/// `applied_at`/`record_hash` are NOT identity inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationAppliedPersist {
    /// This call won the insert: the receipt is now the one persisted
    /// application for its `(domain_id, session_id, application_id)`.
    Inserted,
    /// An application already existed for this triple; nothing was written.
    /// Carries the original persisted receipt (original `applied_at` /
    /// `record_hash` — never restamped). Boxed because [`MutationAppliedReceipt`]
    /// is a large payload (five string fields plus three 32-byte hashes —
    /// `plan_record_hash`, `result_hash`, and `record_hash`), so an unboxed
    /// variant would make this enum needlessly large next to the unit
    /// `Inserted`.
    Existing(Box<MutationAppliedReceipt>),
}

/// Class string for `EvidencePacketProducedReceipt` opaque storage (#2314
/// contract + #2316 EP1–EP5 decision rung). Chosen in the apps layer so the
/// gateway never sees the typed name. Distinct from every other process class
/// store.
const EVIDENCE_PACKET_PRODUCED_CLASS: &str = "evidence_packet_produced";

/// Build the injective composite `key1` binding a produced evidence packet to
/// its `(domain_id, session_id)` anchor in opaque storage.
///
/// Sibling of [`mutation_applied_composite_key1`] with the identical
/// netstring-style encoding (decimal length of `domain_id`, a colon, then the
/// two ids concatenated), kept as a separate function so this class's key
/// derivation is independently anchored by its own anti-aliasing test. The
/// length prefix delimits `domain_id` exactly, so `("ab","c")` and `("a","bc")`
/// can never produce the same key, and two domains sharing a `session_id` scan
/// different `key1` prefixes.
pub fn evidence_packet_produced_composite_key1(domain_id: &str, session_id: &str) -> String {
    format!("{}:{domain_id}{session_id}", domain_id.len())
}

/// Outcome of persisting an [`EvidencePacketProducedReceipt`] through
/// [`GovernanceReceiptBackend::put_evidence_packet_produced`].
///
/// The backend enforces at most one packet per `(domain_id, session_id,
/// packet_id)` **atomically at the storage layer** (same `put_opaque_if_absent`
/// primitive as the sibling process classes). The caller (the manager) turns
/// `Existing` into either a same-identity idempotent success or a fail-closed
/// conflict — stable identity is `mutation_application_id` +
/// `mutation_applied_record_hash` + `receipt_set_hash` + `packet_hash` +
/// `redaction_profile_hash` + `produced_by` (per the #2316 rung §9;
/// `produced_at`/`record_hash` are NOT identity inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidencePacketProducedPersist {
    /// This call won the insert: the receipt is now the one persisted packet for
    /// its `(domain_id, session_id, packet_id)`.
    Inserted,
    /// A packet already existed for this triple; nothing was written. Carries
    /// the original persisted receipt (original `produced_at` / `record_hash` —
    /// never restamped). Boxed because [`EvidencePacketProducedReceipt`] is a
    /// large payload (five string fields plus four 32-byte hashes), so an
    /// unboxed variant would make this enum needlessly large next to the unit
    /// `Inserted`.
    Existing(Box<EvidencePacketProducedReceipt>),
}

/// Class string for `EvidencePacketExportPreparedReceipt` opaque storage
/// (#2322 boundary contract + #2324 EX1–EX8 decision rung). Chosen in the apps
/// layer so the gateway never sees the typed name. Distinct from every other
/// process class store.
const EVIDENCE_PACKET_EXPORT_PREPARED_CLASS: &str = "evidence_packet_export_prepared";

/// Build the injective composite `key1` binding a prepared evidence-packet
/// export to its `(domain_id, session_id)` anchor in opaque storage.
///
/// Sibling of [`evidence_packet_produced_composite_key1`] with the identical
/// netstring-style encoding (decimal length of `domain_id`, a colon, then the
/// two ids concatenated), kept as a separate function so this class's key
/// derivation is independently anchored by its own anti-aliasing test. The
/// length prefix delimits `domain_id` exactly, so `("ab","c")` and `("a","bc")`
/// can never produce the same key, and two domains sharing a `session_id` scan
/// different `key1` prefixes.
pub fn evidence_packet_export_prepared_composite_key1(domain_id: &str, session_id: &str) -> String {
    format!("{}:{domain_id}{session_id}", domain_id.len())
}

/// Outcome of persisting an [`EvidencePacketExportPreparedReceipt`] through
/// [`GovernanceReceiptBackend::put_evidence_packet_export_prepared`].
///
/// The backend enforces at most one export preparation per `(domain_id,
/// session_id, export_id)` **atomically at the storage layer** (same
/// `put_opaque_if_absent` primitive as the sibling process classes). The
/// caller (the manager) turns `Existing` into either a same-identity
/// idempotent success or a fail-closed conflict — stable identity is
/// `packet_id` + `packet_produced_record_hash` + `packet_hash` +
/// `export_policy_hash` + `recipient_scope_id` + `prepared_by` (per the #2324
/// rung §12; `prepared_at`/`record_hash` are NOT identity inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidencePacketExportPreparedPersist {
    /// This call won the insert: the receipt is now the one persisted export
    /// preparation for its `(domain_id, session_id, export_id)`.
    Inserted,
    /// An export preparation already existed for this triple; nothing was
    /// written. Carries the original persisted receipt (original `prepared_at`
    /// / `record_hash` — never restamped). Boxed because
    /// [`EvidencePacketExportPreparedReceipt`] is a large payload (six string
    /// fields plus three 32-byte hashes), so an unboxed variant would make
    /// this enum needlessly large next to the unit `Inserted`.
    Existing(Box<EvidencePacketExportPreparedReceipt>),
}

/// Class string for `MeetingAttendanceReceiptV2` opaque storage (#1868).
/// Chosen in the apps layer so the gateway persists v2 attendance evidence
/// without importing the typed name (preserves the meaning firewall).
const MEETING_ATTENDANCE_V2_CLASS: &str = "meeting_attendance_v2";

/// Class string for `ActionItemCompletionReceiptV2` opaque storage (#1868).
/// Chosen in the apps layer so the gateway persists v2 completion evidence
/// without importing the typed name (preserves the meaning firewall).
const ACTION_ITEM_COMPLETION_V2_CLASS: &str = "action_item_completion_v2";

/// Class string for `GovernanceDecisionReceiptV3` opaque storage (#1868).
/// Chosen in the apps layer so the gateway persists v3 process-authorized
/// decision evidence without importing the typed name (preserves the meaning
/// firewall).
const GOVERNANCE_DECISION_V3_CLASS: &str = "governance_decision_v3";

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

    /// Force all buffered receipt-store writes durable (fsync).
    ///
    /// Default no-op for in-memory / non-durable backends. The gateway
    /// sled-backed `ReceiptStore` overrides it. The governance close write-ahead
    /// journal calls this (alongside the state and action-item stores) before
    /// clearing a completed close's journal entry, so the entry can never be
    /// cleared while a receipt is still un-fsynced in a separate sled DB — which
    /// would leave a terminal proposal with a missing v1/v3 receipt and break the
    /// audit chain the journal protects.
    fn flush(&self) -> Result<(), String> {
        Ok(())
    }

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

    /// Persist a [`GovernanceDecisionReceiptV3`] — the #1868 process-authorized
    /// decision receipt carrying `capability_scope_presented` and an explicit
    /// [`ReceiptMandateAttestation`](icn_governance::ReceiptMandateAttestation).
    /// Emitted **alongside** the existing v1 receipt / `GovernanceProofV2` by
    /// the migrated normal-close path (v1 emission and its consumers are
    /// unchanged this slice).
    ///
    /// Unlike the meeting/action-item v2 receipts, a decision receipt carries
    /// no timestamp field, so `recorded_at` (the proposal close time) is
    /// passed explicitly — it only orders the opaque entry, it is not part of
    /// the canonical `decision_hash`.
    ///
    /// **Default routes through opaque storage** (same pattern as
    /// [`Self::put_meeting_attendance_v2`] / [`Self::put_action_item_completion_v2`]):
    /// the typed receipt is serialised to JSON and delegated to
    /// [`Self::put_opaque`] under class `"governance_decision_v3"` with
    /// `key1 = receipt.proposal_id`, `key2 = None` (mirrors the v1 by-proposal
    /// index). The production gateway-backed `ReceiptStore` overrides
    /// `put_opaque`, so it gets durable persistence here **without** importing
    /// `GovernanceDecisionReceiptV3` — the meaning firewall stays put. A backend
    /// that does not implement opaque storage hits the `put_opaque` fail-closed
    /// default (`opaque_storage_not_implemented`) rather than silently dropping
    /// the v3 evidence. Backends wanting custom typed persistence (e.g. an
    /// in-memory test store) may still override this method directly.
    fn put_governance_decision_v3(
        &self,
        receipt: &GovernanceDecisionReceiptV3,
        recorded_at: u64,
    ) -> Result<(), String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize GovernanceDecisionReceiptV3: {e}"))?;
        self.put_opaque(
            GOVERNANCE_DECISION_V3_CLASS,
            &receipt.proposal_id,
            None,
            recorded_at,
            receipt.decision_hash,
            &payload,
        )
    }

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

    /// Persist an [`ActionItemCompletionReceiptV2`] — the #1868 v2 form
    /// carrying `capability_scope_presented` and an explicit
    /// [`ReceiptMandateAttestation`](icn_governance::ReceiptMandateAttestation).
    /// Emitted **alongside** the v1 receipt by the migrated
    /// `update_action_item_status` path (v1 emission + consumers unchanged).
    ///
    /// Append-only and idempotent on `record_hash`, same discipline as
    /// [`Self::put_action_item_completion`].
    ///
    /// **Default routes through opaque storage** (same pattern as
    /// [`Self::put_meeting_attendance_v2`] / [`Self::put_process_gate_result`]):
    /// the typed receipt is serialised to JSON and delegated to
    /// [`Self::put_opaque`] under class `"action_item_completion_v2"` with
    /// `key1 = receipt.item_id`, `key2 = None` (mirrors the v1 by-item index).
    /// The production gateway-backed `ReceiptStore` overrides `put_opaque`
    /// (Stage 1b), so it gets durable persistence here **without** importing
    /// `ActionItemCompletionReceiptV2` — the meaning firewall stays put. A
    /// backend that does not implement opaque storage hits the `put_opaque`
    /// fail-closed default (`opaque_storage_not_implemented`) rather than
    /// silently dropping the v2 evidence. Backends wanting custom typed
    /// persistence (e.g. an in-memory test store) may still override this
    /// method directly.
    fn put_action_item_completion_v2(
        &self,
        receipt: &ActionItemCompletionReceiptV2,
    ) -> Result<(), String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize ActionItemCompletionReceiptV2: {e}"))?;
        self.put_opaque(
            ACTION_ITEM_COMPLETION_V2_CLASS,
            &receipt.item_id,
            None,
            receipt.completed_at,
            receipt.record_hash,
            &payload,
        )
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

    /// Persist a [`MeetingAttendanceReceiptV2`] — the #1868 v2 form carrying
    /// `capability_scope_presented` and an explicit
    /// [`ReceiptMandateAttestation`](icn_governance::ReceiptMandateAttestation).
    /// Emitted **alongside** the v1 receipt by migrated handlers (the v1
    /// emission and its consumers are unchanged this slice).
    ///
    /// Append-only and idempotent on `record_hash`, same discipline as
    /// [`Self::put_meeting_attendance`].
    ///
    /// **Default routes through opaque storage** (same pattern as
    /// [`Self::put_process_gate_result`]): the typed receipt is serialised to
    /// JSON and delegated to [`Self::put_opaque`] under class
    /// `"meeting_attendance_v2"` with `key1 = receipt.meeting_id`,
    /// `key2 = Some(receipt.attendee_did)`. The production gateway-backed
    /// `ReceiptStore` overrides `put_opaque` (Stage 1b), so it gets durable
    /// persistence here **without** the gateway importing
    /// `MeetingAttendanceReceiptV2` — the meaning firewall stays put. A
    /// backend that does not implement opaque storage hits the
    /// `put_opaque` fail-closed default (`opaque_storage_not_implemented`)
    /// rather than silently dropping the v2 evidence. Backends wanting custom
    /// typed persistence (e.g. an in-memory test store) may still override
    /// this method directly.
    fn put_meeting_attendance_v2(
        &self,
        receipt: &MeetingAttendanceReceiptV2,
    ) -> Result<(), String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize MeetingAttendanceReceiptV2: {e}"))?;
        self.put_opaque(
            MEETING_ATTENDANCE_V2_CLASS,
            &receipt.meeting_id,
            Some(receipt.attendee_did.as_str()),
            receipt.recorded_at,
            receipt.record_hash,
            &payload,
        )
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

    /// List the [`ProcessGateResultReceipt`]s persisted for a process
    /// session **within one governance domain**, oldest-first by
    /// `recorded_at` (the #2275 domain-scoped read).
    ///
    /// The opaque gate-result index is keyed by `session_id` alone, so two
    /// domains reusing the same `session_id` share one storage chain. This
    /// read filters on the receipt's **hash-bound `domain_id`** so
    /// session-anchored consumers (future evidence export) can never mix
    /// evidence across domains. It does **not** assume globally unique
    /// session ids and does **not** alter the legacy
    /// [`Self::list_process_gate_results_for_session`] /
    /// [`Self::get_latest_process_gate_result`] reads, which remain
    /// byte-identical in behavior.
    fn list_process_gate_results_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<ProcessGateResultReceipt>, String> {
        let all = self.list_process_gate_results_for_session(session_id)?;
        Ok(all
            .into_iter()
            .filter(|r| r.domain_id == domain_id)
            .collect())
    }

    /// Persist a [`ProcessSessionOpenedReceipt`] — the #2275 session
    /// anchor — enforcing **at most one opening per
    /// `(domain_id, session_id)` atomically at the storage layer**.
    ///
    /// **Default routes through opaque storage** under class
    /// `"process_session_opened"` with `key1 = receipt.domain_id`,
    /// `key2 = Some(receipt.session_id)`, via
    /// [`Self::put_opaque_if_absent`] (an insert-if-absent primitive; the
    /// plain append-chain [`Self::put_opaque`] cannot enforce uniqueness —
    /// two racing opens with different `opened_at` values would both
    /// persist). On a lost insert the original persisted receipt is
    /// hydrated and returned as [`SessionOpenedPersist::Existing`]; the
    /// caller decides idempotency vs conflict from `opened_by`. A backend
    /// that does not implement opaque storage fails closed with the
    /// `opaque_storage_not_implemented` sentinel.
    ///
    /// A receipt records a fact; it grants no authority.
    fn put_process_session_opened(
        &self,
        receipt: &ProcessSessionOpenedReceipt,
    ) -> Result<SessionOpenedPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize ProcessSessionOpenedReceipt: {e}"))?;
        let existing = self.put_opaque_if_absent(
            PROCESS_SESSION_OPENED_CLASS,
            &receipt.domain_id,
            Some(&receipt.session_id),
            receipt.opened_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(SessionOpenedPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_process_session_opened(&receipt.domain_id, &receipt.session_id)?
                    .ok_or_else(|| {
                        // The unique marker says an opening exists but the
                        // payload cannot be hydrated — surface loudly
                        // rather than minting a second opening.
                        "process_session_opened_inconsistent: unique marker present \
                         but no persisted opening payload could be read"
                            .to_string()
                    })?;
                Ok(SessionOpenedPersist::Existing(original))
            }
        }
    }

    /// Retrieve the persisted [`ProcessSessionOpenedReceipt`] for a
    /// `(domain_id, session_id)`, or `None` when the session has no
    /// recorded opening.
    ///
    /// **Default routes through opaque storage.** Same key convention as
    /// [`Self::put_process_session_opened`]. At most one entry exists per
    /// pair (uniqueness is enforced on write), so "latest" and "the"
    /// opening coincide.
    fn get_process_session_opened(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Option<ProcessSessionOpenedReceipt>, String> {
        let payload =
            self.get_latest_opaque(PROCESS_SESSION_OPENED_CLASS, domain_id, Some(session_id))?;
        match payload {
            Some(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize ProcessSessionOpenedReceipt: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// Persist a [`DeliberationEntryRecordedReceipt`] — the third
    /// `ProcessTransitionReceipt` class (#2277 contract, #2278 Q3 decision)
    /// — enforcing **at most one entry per
    /// `(domain_id, session_id, entry_id)` atomically at the storage
    /// layer**.
    ///
    /// **Default routes through opaque storage** under class
    /// `"deliberation_entry_recorded"` with
    /// `key1 =` [`deliberation_entry_composite_key1`] (the injective
    /// `(domain_id, session_id)` composite) and `key2 = entry_id`, via
    /// [`Self::put_opaque_if_absent`]. On a lost insert the original
    /// persisted receipt is hydrated and returned as
    /// [`DeliberationEntryPersist::Existing`]; the caller decides
    /// idempotency vs conflict from `author` + `body_hash` + `entry_kind`.
    /// A backend that does not implement opaque storage fails closed with
    /// the `opaque_storage_not_implemented` sentinel.
    ///
    /// The session-open precondition (an entry may only be recorded against
    /// an already-opened session) is the manager's responsibility, not this
    /// method's — the backend persists what it is handed.
    ///
    /// A receipt records a fact; it grants no authority.
    fn put_deliberation_entry_recorded(
        &self,
        receipt: &DeliberationEntryRecordedReceipt,
    ) -> Result<DeliberationEntryPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize DeliberationEntryRecordedReceipt: {e}"))?;
        let key1 = deliberation_entry_composite_key1(&receipt.domain_id, &receipt.session_id);
        let existing = self.put_opaque_if_absent(
            DELIBERATION_ENTRY_RECORDED_CLASS,
            &key1,
            Some(&receipt.entry_id),
            receipt.recorded_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(DeliberationEntryPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_deliberation_entry_recorded(
                        &receipt.domain_id,
                        &receipt.session_id,
                        &receipt.entry_id,
                    )?
                    .ok_or_else(|| {
                        // The unique marker says an entry exists but the
                        // payload cannot be hydrated — surface loudly
                        // rather than minting a second entry.
                        "deliberation_entry_recorded_inconsistent: unique marker present \
                         but no persisted entry payload could be read"
                            .to_string()
                    })?;
                Ok(DeliberationEntryPersist::Existing(original))
            }
        }
    }

    /// Retrieve the persisted [`DeliberationEntryRecordedReceipt`] for a
    /// `(domain_id, session_id, entry_id)`, or `None` when no entry has
    /// been recorded. Same key convention as
    /// [`Self::put_deliberation_entry_recorded`]; at most one entry exists
    /// per triple (uniqueness is enforced on write).
    fn get_deliberation_entry_recorded(
        &self,
        domain_id: &str,
        session_id: &str,
        entry_id: &str,
    ) -> Result<Option<DeliberationEntryRecordedReceipt>, String> {
        let key1 = deliberation_entry_composite_key1(domain_id, session_id);
        let payload =
            self.get_latest_opaque(DELIBERATION_ENTRY_RECORDED_CLASS, &key1, Some(entry_id))?;
        match payload {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                format!("deserialize DeliberationEntryRecordedReceipt: {e}")
            })?)),
            None => Ok(None),
        }
    }

    /// List all deliberation-entry receipts recorded against one
    /// `(domain_id, session_id)` anchor.
    ///
    /// **Ordering is the opaque store's deterministic chronological order —
    /// sorted by `(recorded_at, record_hash)` — NOT arrival/insertion
    /// order** (per the #2277 contract as amended in review): entries
    /// sharing a `recorded_at` second order by `record_hash`. Because
    /// `key1` is the injective domain+session composite, two domains
    /// sharing a `session_id` can never mix here — no payload-side
    /// filtering is needed.
    fn list_deliberation_entries_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<DeliberationEntryRecordedReceipt>, String> {
        let key1 = deliberation_entry_composite_key1(domain_id, session_id);
        let payloads = self.list_opaque_for(DELIBERATION_ENTRY_RECORDED_CLASS, &key1)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            out.push(serde_json::from_slice(&bytes).map_err(|e| {
                format!("deserialize DeliberationEntryRecordedReceipt in list: {e}")
            })?);
        }
        Ok(out)
    }

    /// Persist a [`DecisionRecordedReceipt`] — the fourth
    /// `ProcessTransitionReceipt` class (#2280 contract, #2281 Q4
    /// decision) — enforcing **at most one decision per
    /// `(domain_id, session_id, decision_id)` atomically at the storage
    /// layer**.
    ///
    /// **Default routes through opaque storage** under class
    /// `"decision_recorded"` with
    /// `key1 =` [`decision_recorded_composite_key1`] (the injective
    /// `(domain_id, session_id)` composite) and `key2 = decision_id`, via
    /// [`Self::put_opaque_if_absent`]. On a lost insert the original
    /// persisted receipt is hydrated and returned as
    /// [`DecisionRecordedPersist::Existing`]; the caller decides
    /// idempotency vs conflict from `recorded_by` + `body_hash`. A backend
    /// that does not implement opaque storage fails closed with the
    /// `opaque_storage_not_implemented` sentinel.
    ///
    /// This class never touches the typed proposal/vote
    /// [`GovernanceDecisionReceipt`] persistence — the two decision
    /// lineages are parallel and share nothing (#2281 Axis B).
    ///
    /// The session-open precondition (a decision may only be recorded
    /// against an already-opened session) is the manager's responsibility,
    /// not this method's — the backend persists what it is handed.
    ///
    /// A receipt records a fact; it grants no authority.
    fn put_decision_recorded(
        &self,
        receipt: &DecisionRecordedReceipt,
    ) -> Result<DecisionRecordedPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize DecisionRecordedReceipt: {e}"))?;
        let key1 = decision_recorded_composite_key1(&receipt.domain_id, &receipt.session_id);
        let existing = self.put_opaque_if_absent(
            DECISION_RECORDED_CLASS,
            &key1,
            Some(&receipt.decision_id),
            receipt.recorded_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(DecisionRecordedPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_decision_recorded(
                        &receipt.domain_id,
                        &receipt.session_id,
                        &receipt.decision_id,
                    )?
                    .ok_or_else(|| {
                        // The unique marker says a decision exists but the
                        // payload cannot be hydrated — surface loudly
                        // rather than minting a second decision.
                        "decision_recorded_inconsistent: unique marker present \
                         but no persisted decision payload could be read"
                            .to_string()
                    })?;
                Ok(DecisionRecordedPersist::Existing(original))
            }
        }
    }

    /// Retrieve the persisted [`DecisionRecordedReceipt`] for a
    /// `(domain_id, session_id, decision_id)`, or `None` when no decision
    /// has been recorded. Same key convention as
    /// [`Self::put_decision_recorded`]; at most one decision exists per
    /// triple (uniqueness is enforced on write).
    fn get_decision_recorded(
        &self,
        domain_id: &str,
        session_id: &str,
        decision_id: &str,
    ) -> Result<Option<DecisionRecordedReceipt>, String> {
        let key1 = decision_recorded_composite_key1(domain_id, session_id);
        let payload = self.get_latest_opaque(DECISION_RECORDED_CLASS, &key1, Some(decision_id))?;
        match payload {
            Some(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize DecisionRecordedReceipt: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// List all decision receipts recorded against one
    /// `(domain_id, session_id)` anchor.
    ///
    /// **Ordering is the opaque store's deterministic chronological order —
    /// sorted by `(recorded_at, record_hash)` — NOT arrival/insertion
    /// order** (same contract as the sibling process classes). Because
    /// `key1` is the injective domain+session composite, two domains
    /// sharing a `session_id` can never mix here — no payload-side
    /// filtering is needed.
    fn list_decisions_recorded_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<DecisionRecordedReceipt>, String> {
        let key1 = decision_recorded_composite_key1(domain_id, session_id);
        let payloads = self.list_opaque_for(DECISION_RECORDED_CLASS, &key1)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            out.push(
                serde_json::from_slice(&bytes)
                    .map_err(|e| format!("deserialize DecisionRecordedReceipt in list: {e}"))?,
            );
        }
        Ok(out)
    }

    /// Persist an [`ActivationCrossedReceipt`] emitted when the runtime
    /// records that a decision crossed the activation boundary (#2294
    /// contract + #2295 rung).
    ///
    /// **Default routes through opaque storage.** Serialises the typed
    /// receipt to JSON and delegates to [`Self::put_opaque_if_absent`] under
    /// class `"activation_crossed"` with
    /// `key1 =` [`activation_crossed_composite_key1`] (the injective
    /// domain+session composite) and `key2 = activation_id`. On a lost
    /// insert the original persisted receipt is hydrated and returned
    /// (never restamped). A backend that has not opted in to opaque storage
    /// inherits the fail-closed `opaque_storage_not_implemented` default.
    fn put_activation_crossed(
        &self,
        receipt: &ActivationCrossedReceipt,
    ) -> Result<ActivationCrossedPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize ActivationCrossedReceipt: {e}"))?;
        let key1 = activation_crossed_composite_key1(&receipt.domain_id, &receipt.session_id);
        let existing = self.put_opaque_if_absent(
            ACTIVATION_CROSSED_CLASS,
            &key1,
            Some(&receipt.activation_id),
            receipt.recorded_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(ActivationCrossedPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_activation_crossed(
                        &receipt.domain_id,
                        &receipt.session_id,
                        &receipt.activation_id,
                    )?
                    .ok_or_else(|| {
                        // The unique marker says a crossing exists but the
                        // payload cannot be hydrated — surface loudly rather
                        // than minting a second crossing.
                        "activation_crossed_inconsistent: unique marker present \
                         but no persisted crossing payload could be read"
                            .to_string()
                    })?;
                Ok(ActivationCrossedPersist::Existing(Box::new(original)))
            }
        }
    }

    /// Retrieve the persisted [`ActivationCrossedReceipt`] for a
    /// `(domain_id, session_id, activation_id)`, or `None` when no crossing
    /// has been recorded. Same key convention as
    /// [`Self::put_activation_crossed`]; at most one crossing exists per
    /// triple (uniqueness is enforced on write).
    fn get_activation_crossed(
        &self,
        domain_id: &str,
        session_id: &str,
        activation_id: &str,
    ) -> Result<Option<ActivationCrossedReceipt>, String> {
        let key1 = activation_crossed_composite_key1(domain_id, session_id);
        let payload =
            self.get_latest_opaque(ACTIVATION_CROSSED_CLASS, &key1, Some(activation_id))?;
        match payload {
            Some(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize ActivationCrossedReceipt: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// List all activation-crossed receipts recorded against one
    /// `(domain_id, session_id)` anchor, in the store's deterministic
    /// chronological `(recorded_at, record_hash)` order. Because `key1` is
    /// the injective domain+session composite, two domains sharing a
    /// `session_id` can never mix here — no payload-side filtering is needed.
    fn list_activations_crossed_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<ActivationCrossedReceipt>, String> {
        let key1 = activation_crossed_composite_key1(domain_id, session_id);
        let payloads = self.list_opaque_for(ACTIVATION_CROSSED_CLASS, &key1)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            out.push(
                serde_json::from_slice(&bytes)
                    .map_err(|e| format!("deserialize ActivationCrossedReceipt in list: {e}"))?,
            );
        }
        Ok(out)
    }

    /// Persist a [`MutationPlanRecordedReceipt`] emitted when the runtime
    /// records that a mutation plan was recorded after an activation (#2300
    /// contract + #2302 rung).
    ///
    /// **Default routes through opaque storage.** Serialises the typed
    /// receipt to JSON and delegates to [`Self::put_opaque_if_absent`] under
    /// class `"mutation_plan_recorded"` with
    /// `key1 =` [`mutation_plan_recorded_composite_key1`] (the injective
    /// domain+session composite) and `key2 = plan_id`. On a lost insert the
    /// original persisted receipt is hydrated and returned (never restamped).
    /// A backend that has not opted in to opaque storage inherits the
    /// fail-closed `opaque_storage_not_implemented` default.
    fn put_mutation_plan_recorded(
        &self,
        receipt: &MutationPlanRecordedReceipt,
    ) -> Result<MutationPlanRecordedPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize MutationPlanRecordedReceipt: {e}"))?;
        let key1 = mutation_plan_recorded_composite_key1(&receipt.domain_id, &receipt.session_id);
        let existing = self.put_opaque_if_absent(
            MUTATION_PLAN_RECORDED_CLASS,
            &key1,
            Some(&receipt.plan_id),
            receipt.recorded_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(MutationPlanRecordedPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_mutation_plan_recorded(
                        &receipt.domain_id,
                        &receipt.session_id,
                        &receipt.plan_id,
                    )?
                    .ok_or_else(|| {
                        // The unique marker says a plan exists but the payload
                        // cannot be hydrated — surface loudly rather than
                        // minting a second plan.
                        "mutation_plan_recorded_inconsistent: unique marker present \
                         but no persisted plan payload could be read"
                            .to_string()
                    })?;
                Ok(MutationPlanRecordedPersist::Existing(Box::new(original)))
            }
        }
    }

    /// Retrieve the persisted [`MutationPlanRecordedReceipt`] for a
    /// `(domain_id, session_id, plan_id)`, or `None` when no plan has been
    /// recorded. Same key convention as
    /// [`Self::put_mutation_plan_recorded`]; at most one plan exists per
    /// triple (uniqueness is enforced on write).
    fn get_mutation_plan_recorded(
        &self,
        domain_id: &str,
        session_id: &str,
        plan_id: &str,
    ) -> Result<Option<MutationPlanRecordedReceipt>, String> {
        let key1 = mutation_plan_recorded_composite_key1(domain_id, session_id);
        let payload = self.get_latest_opaque(MUTATION_PLAN_RECORDED_CLASS, &key1, Some(plan_id))?;
        match payload {
            Some(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize MutationPlanRecordedReceipt: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// List all mutation-plan-recorded receipts recorded against one
    /// `(domain_id, session_id)` anchor, in the store's deterministic
    /// chronological `(recorded_at, record_hash)` order. Because `key1` is
    /// the injective domain+session composite, two domains sharing a
    /// `session_id` can never mix here — no payload-side filtering is needed.
    fn list_mutation_plans_recorded_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<MutationPlanRecordedReceipt>, String> {
        let key1 = mutation_plan_recorded_composite_key1(domain_id, session_id);
        let payloads = self.list_opaque_for(MUTATION_PLAN_RECORDED_CLASS, &key1)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            out.push(
                serde_json::from_slice(&bytes)
                    .map_err(|e| format!("deserialize MutationPlanRecordedReceipt in list: {e}"))?,
            );
        }
        Ok(out)
    }

    /// Persist a [`MutationAppliedReceipt`] emitted when the runtime records
    /// that a recorded mutation plan was applied (#2307 contract + #2308 rung).
    ///
    /// **Default routes through opaque storage.** Serialises the typed receipt
    /// to JSON and delegates to [`Self::put_opaque_if_absent`] under class
    /// `"mutation_applied"` with `key1 =` [`mutation_applied_composite_key1`]
    /// (the injective domain+session composite) and `key2 = application_id`. On
    /// a lost insert the original persisted receipt is hydrated and returned
    /// (never restamped). A backend that has not opted in to opaque storage
    /// inherits the fail-closed `opaque_storage_not_implemented` default.
    fn put_mutation_applied(
        &self,
        receipt: &MutationAppliedReceipt,
    ) -> Result<MutationAppliedPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize MutationAppliedReceipt: {e}"))?;
        let key1 = mutation_applied_composite_key1(&receipt.domain_id, &receipt.session_id);
        let existing = self.put_opaque_if_absent(
            MUTATION_APPLIED_CLASS,
            &key1,
            Some(&receipt.application_id),
            receipt.applied_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(MutationAppliedPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_mutation_applied(
                        &receipt.domain_id,
                        &receipt.session_id,
                        &receipt.application_id,
                    )?
                    .ok_or_else(|| {
                        // The unique marker says an application exists but the
                        // payload cannot be hydrated — surface loudly rather
                        // than minting a second application.
                        "mutation_applied_inconsistent: unique marker present \
                         but no persisted application payload could be read"
                            .to_string()
                    })?;
                Ok(MutationAppliedPersist::Existing(Box::new(original)))
            }
        }
    }

    /// Retrieve the persisted [`MutationAppliedReceipt`] for a `(domain_id,
    /// session_id, application_id)`, or `None` when no application has been
    /// recorded. Same key convention as [`Self::put_mutation_applied`]; at most
    /// one application exists per triple (uniqueness is enforced on write).
    fn get_mutation_applied(
        &self,
        domain_id: &str,
        session_id: &str,
        application_id: &str,
    ) -> Result<Option<MutationAppliedReceipt>, String> {
        let key1 = mutation_applied_composite_key1(domain_id, session_id);
        let payload =
            self.get_latest_opaque(MUTATION_APPLIED_CLASS, &key1, Some(application_id))?;
        match payload {
            Some(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize MutationAppliedReceipt: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// List all mutation-applied receipts recorded against one `(domain_id,
    /// session_id)` anchor, in the store's deterministic chronological
    /// `(recorded_at, record_hash)` order. Because `key1` is the injective
    /// domain+session composite, two domains sharing a `session_id` can never
    /// mix here — no payload-side filtering is needed.
    fn list_mutation_applied_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<MutationAppliedReceipt>, String> {
        let key1 = mutation_applied_composite_key1(domain_id, session_id);
        let payloads = self.list_opaque_for(MUTATION_APPLIED_CLASS, &key1)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            out.push(
                serde_json::from_slice(&bytes)
                    .map_err(|e| format!("deserialize MutationAppliedReceipt in list: {e}"))?,
            );
        }
        Ok(out)
    }

    /// Persist an [`EvidencePacketProducedReceipt`] emitted when the runtime
    /// records that a redacted evidence packet artifact was produced (#2314
    /// contract + #2316 rung).
    ///
    /// **Default routes through opaque storage.** Serialises the typed receipt
    /// to JSON and delegates to [`Self::put_opaque_if_absent`] under class
    /// `"evidence_packet_produced"` with `key1 =`
    /// [`evidence_packet_produced_composite_key1`] (the injective domain+session
    /// composite) and `key2 = packet_id`. On a lost insert the original
    /// persisted receipt is hydrated and returned (never restamped). A backend
    /// that has not opted in to opaque storage inherits the fail-closed
    /// `opaque_storage_not_implemented` default.
    fn put_evidence_packet_produced(
        &self,
        receipt: &EvidencePacketProducedReceipt,
    ) -> Result<EvidencePacketProducedPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize EvidencePacketProducedReceipt: {e}"))?;
        let key1 = evidence_packet_produced_composite_key1(&receipt.domain_id, &receipt.session_id);
        let existing = self.put_opaque_if_absent(
            EVIDENCE_PACKET_PRODUCED_CLASS,
            &key1,
            Some(&receipt.packet_id),
            receipt.produced_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(EvidencePacketProducedPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_evidence_packet_produced(
                        &receipt.domain_id,
                        &receipt.session_id,
                        &receipt.packet_id,
                    )?
                    .ok_or_else(|| {
                        // The unique marker says a packet exists but the payload
                        // cannot be hydrated — surface loudly rather than minting
                        // a second packet.
                        "evidence_packet_produced_inconsistent: unique marker present \
                         but no persisted packet payload could be read"
                            .to_string()
                    })?;
                Ok(EvidencePacketProducedPersist::Existing(Box::new(original)))
            }
        }
    }

    /// Retrieve the persisted [`EvidencePacketProducedReceipt`] for a
    /// `(domain_id, session_id, packet_id)`, or `None` when no packet has been
    /// recorded. Same key convention as [`Self::put_evidence_packet_produced`];
    /// at most one packet exists per triple (uniqueness is enforced on write).
    fn get_evidence_packet_produced(
        &self,
        domain_id: &str,
        session_id: &str,
        packet_id: &str,
    ) -> Result<Option<EvidencePacketProducedReceipt>, String> {
        let key1 = evidence_packet_produced_composite_key1(domain_id, session_id);
        let payload =
            self.get_latest_opaque(EVIDENCE_PACKET_PRODUCED_CLASS, &key1, Some(packet_id))?;
        match payload {
            Some(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize EvidencePacketProducedReceipt: {e}")
                })?))
            }
            None => Ok(None),
        }
    }

    /// List all evidence-packet-produced receipts recorded against one
    /// `(domain_id, session_id)` anchor, in the store's deterministic
    /// chronological `(produced_at, record_hash)` order. Because `key1` is the
    /// injective domain+session composite, two domains sharing a `session_id`
    /// can never mix here — no payload-side filtering is needed.
    fn list_evidence_packet_produced_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<EvidencePacketProducedReceipt>, String> {
        let key1 = evidence_packet_produced_composite_key1(domain_id, session_id);
        let payloads = self.list_opaque_for(EVIDENCE_PACKET_PRODUCED_CLASS, &key1)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            out.push(
                serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize EvidencePacketProducedReceipt in list: {e}")
                })?,
            );
        }
        Ok(out)
    }

    /// Persist an [`EvidencePacketExportPreparedReceipt`] emitted when the
    /// runtime records that an export of a produced evidence packet was
    /// prepared (#2322 contract + #2324 rung).
    ///
    /// **Default routes through opaque storage.** Serialises the typed receipt
    /// to JSON and delegates to [`Self::put_opaque_if_absent`] under class
    /// `"evidence_packet_export_prepared"` with `key1 =`
    /// [`evidence_packet_export_prepared_composite_key1`] (the injective
    /// domain+session composite) and `key2 = export_id`. On a lost insert the
    /// original persisted receipt is hydrated and returned (never restamped).
    /// A backend that has not opted in to opaque storage inherits the
    /// fail-closed `opaque_storage_not_implemented` default.
    fn put_evidence_packet_export_prepared(
        &self,
        receipt: &EvidencePacketExportPreparedReceipt,
    ) -> Result<EvidencePacketExportPreparedPersist, String> {
        let payload = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize EvidencePacketExportPreparedReceipt: {e}"))?;
        let key1 =
            evidence_packet_export_prepared_composite_key1(&receipt.domain_id, &receipt.session_id);
        let existing = self.put_opaque_if_absent(
            EVIDENCE_PACKET_EXPORT_PREPARED_CLASS,
            &key1,
            Some(&receipt.export_id),
            receipt.prepared_at,
            receipt.record_hash,
            &payload,
        )?;
        match existing {
            None => Ok(EvidencePacketExportPreparedPersist::Inserted),
            Some(_winning_hash) => {
                let original = self
                    .get_evidence_packet_export_prepared(
                        &receipt.domain_id,
                        &receipt.session_id,
                        &receipt.export_id,
                    )?
                    .ok_or_else(|| {
                        // The unique marker says a preparation exists but the
                        // payload cannot be hydrated — surface loudly rather
                        // than minting a second preparation.
                        "evidence_packet_export_prepared_inconsistent: unique marker present \
                         but no persisted export-prepared payload could be read"
                            .to_string()
                    })?;
                Ok(EvidencePacketExportPreparedPersist::Existing(Box::new(
                    original,
                )))
            }
        }
    }

    /// Retrieve the persisted [`EvidencePacketExportPreparedReceipt`] for a
    /// `(domain_id, session_id, export_id)`, or `None` when no export
    /// preparation has been recorded. Same key convention as
    /// [`Self::put_evidence_packet_export_prepared`]; at most one preparation
    /// exists per triple (uniqueness is enforced on write).
    fn get_evidence_packet_export_prepared(
        &self,
        domain_id: &str,
        session_id: &str,
        export_id: &str,
    ) -> Result<Option<EvidencePacketExportPreparedReceipt>, String> {
        let key1 = evidence_packet_export_prepared_composite_key1(domain_id, session_id);
        let payload = self.get_latest_opaque(
            EVIDENCE_PACKET_EXPORT_PREPARED_CLASS,
            &key1,
            Some(export_id),
        )?;
        match payload {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                format!("deserialize EvidencePacketExportPreparedReceipt: {e}")
            })?)),
            None => Ok(None),
        }
    }

    /// List all evidence-packet-export-prepared receipts recorded against one
    /// `(domain_id, session_id)` anchor, in the store's deterministic
    /// chronological `(prepared_at, record_hash)` order. Because `key1` is the
    /// injective domain+session composite, two domains sharing a `session_id`
    /// can never mix here — no payload-side filtering is needed.
    fn list_evidence_packet_export_prepared_for_session_in_domain(
        &self,
        domain_id: &str,
        session_id: &str,
    ) -> Result<Vec<EvidencePacketExportPreparedReceipt>, String> {
        let key1 = evidence_packet_export_prepared_composite_key1(domain_id, session_id);
        let payloads = self.list_opaque_for(EVIDENCE_PACKET_EXPORT_PREPARED_CLASS, &key1)?;
        let mut out = Vec::with_capacity(payloads.len());
        for bytes in payloads {
            out.push(serde_json::from_slice(&bytes).map_err(|e| {
                format!("deserialize EvidencePacketExportPreparedReceipt in list: {e}")
            })?);
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

    /// Persist an opaque receipt payload for a `(class, key1, key2_opt)`
    /// triple **only if no entry exists for that triple yet** — the
    /// insert-if-absent primitive the #2275 session-anchor contract
    /// requires. Returns `Ok(None)` when this call won the insert, or
    /// `Ok(Some(existing_record_hash))` when an entry already existed
    /// (in which case **nothing is written**).
    ///
    /// Unlike [`Self::put_opaque`] — whose chain deliberately appends
    /// distinct hashes under the same triple — implementations MUST
    /// enforce the absence check and the insert **atomically inside the
    /// storage transaction** (insert-if-absent / compare-and-swap or an
    /// equivalent unique index). A check-then-write above the store is
    /// not acceptable: two concurrent writers with different
    /// `recorded_at` values would both persist.
    ///
    /// **Fail-closed default.** A backend that has not opted in returns
    /// `Err` with stable sentinel `opaque_storage_not_implemented`.
    fn put_opaque_if_absent(
        &self,
        _class: &str,
        _key1: &str,
        _key2: Option<&str>,
        _recorded_at: u64,
        _record_hash: [u8; 32],
        _payload: &[u8],
    ) -> Result<Option<[u8; 32]>, String> {
        Err("opaque_storage_not_implemented: \
             this backend inherits the fail-closed default for \
             put_opaque_if_absent. Override the method to opt in to \
             durable opaque storage with atomic uniqueness, or handle \
             this error at the caller."
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
