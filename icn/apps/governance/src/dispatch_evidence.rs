//! Downstream dispatch evidence for a persisted institutional effect record.
//!
//! An `InstitutionalEffectRecord` proves governance emitted an intent. It
//! does NOT prove the intended downstream action actually ran. This module
//! adds the missing half: an append-only log of dispatch evidence keyed to
//! the emitted record, plus a pure derivation of reconciliation status.
//!
//! The semantics are strict. A record with no evidence is `emitted_only`;
//! a record with evidence where `success == true` is `execution_evidenced`;
//! one with `success == false` is `execution_failed`. "Evidenced" means a
//! downstream subsystem reported back synchronously — it does NOT mean the
//! action is externally observable or that side effects are durable in
//! downstream storage. Callers that require stronger guarantees must follow
//! the `receipt_ref` into the downstream subsystem's own audit surface.
//!
//! Not every dispatch path produces evidence today. The charter-deploy hook
//! and commons freeze/unfreeze hooks are `Fn(...)` returning nothing; those
//! effects will remain `emitted_only` until their surfaces are upgraded.
//! This module does not pretend otherwise.

use serde::{Deserialize, Serialize};

use crate::institutional_effect::InstitutionalEffectRecord;

/// Append-only evidence that a downstream subsystem dispatched the action
/// implied by an emitted institutional effect record.
///
/// `subsystem` names the downstream system that reported back (e.g. `"sdis"`,
/// `"commons"`). `receipt_ref` is an opaque identifier produced by that
/// subsystem — consumers that need the downstream receipt itself must look
/// it up in the subsystem's own store. `success` is the subsystem's own
/// bool; this module does not re-validate it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDispatchEvidence {
    /// Stable identifier (uuidv4).
    pub evidence_id: String,
    /// Institutional effect record this evidence attaches to.
    pub effect_record_id: String,
    /// Proposal whose acceptance emitted the record. Duplicated for
    /// cheaper indexing; must equal the effect record's proposal_id.
    pub proposal_id: String,
    /// Downstream subsystem name. Lowercase, stable: `"sdis"`,
    /// `"commons"`, `"ledger"`, …
    pub subsystem: String,
    /// Opaque receipt reference minted by the downstream subsystem.
    ///
    /// For SDIS `AppointSteward` / `RevokeSteward` / `ReconfirmSteward`,
    /// this is the content-addressed `StewardId::to_hex()` published by
    /// the commons layer — *not* a steward-store `state_change_hash`.
    ///
    /// `None` means the executing service had no downstream handle to
    /// attribute: a failure, a no-op (e.g. revoke against a DID with no
    /// active record), or an effect family that does not yet produce a
    /// downstream receipt. Consumers that need the full downstream
    /// receipt must look it up in the subsystem's own store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_ref: Option<String>,
    /// Whether the subsystem reported success.
    pub success: bool,
    /// Error message surfaced by the subsystem on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Unix seconds when this evidence was persisted.
    pub recorded_at: u64,
}

impl EffectDispatchEvidence {
    pub fn new(
        effect_record_id: impl Into<String>,
        proposal_id: impl Into<String>,
        subsystem: impl Into<String>,
        receipt_ref: Option<String>,
        success: bool,
        error_message: Option<String>,
        recorded_at: u64,
    ) -> Self {
        Self {
            evidence_id: uuid::Uuid::new_v4().to_string(),
            effect_record_id: effect_record_id.into(),
            proposal_id: proposal_id.into(),
            subsystem: subsystem.into(),
            receipt_ref,
            success,
            error_message,
            recorded_at,
        }
    }
}

/// Derived reconciliation status of an emitted effect against its dispatch
/// evidence. Pure function of inputs — no storage, no I/O.
///
/// Ordering of precedence when multiple evidence entries exist:
/// - Any `success: false` entry → `ExecutionFailed`. A later success does
///   not erase an earlier failure; the audit trail retains both.
/// - Otherwise, at least one `success: true` → `ExecutionEvidenced`.
/// - Empty list → `EmittedOnly`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ReconciliationStatus {
    /// The governance app has an emitted record but no downstream evidence.
    /// Either dispatch has not yet happened, the dispatch surface is
    /// fire-and-forget, or the subsystem never reported back.
    EmittedOnly,
    /// A downstream subsystem reported successful dispatch.
    ExecutionEvidenced,
    /// A downstream subsystem reported failure. The error message from the
    /// most recent failed evidence entry is surfaced.
    ExecutionFailed { error: Option<String> },
}

/// Derive a reconciliation status from one effect record and its evidence.
///
/// `record` is not required to compute the label but is included so future
/// policy (e.g. "failure older than N days escalates") has a hook point.
pub fn derive_reconciliation_status(
    _record: &InstitutionalEffectRecord,
    evidence: &[EffectDispatchEvidence],
) -> ReconciliationStatus {
    if let Some(failed) = evidence
        .iter()
        .filter(|e| !e.success)
        .max_by_key(|e| e.recorded_at)
    {
        return ReconciliationStatus::ExecutionFailed {
            error: failed.error_message.clone(),
        };
    }
    if evidence.iter().any(|e| e.success) {
        return ReconciliationStatus::ExecutionEvidenced;
    }
    ReconciliationStatus::EmittedOnly
}

/// Short lowercase label for a reconciliation status — used in wire
/// responses that prefer a flat string over a tagged enum.
pub fn reconciliation_label(status: &ReconciliationStatus) -> &'static str {
    match status {
        ReconciliationStatus::EmittedOnly => "emitted_only",
        ReconciliationStatus::ExecutionEvidenced => "execution_evidenced",
        ReconciliationStatus::ExecutionFailed { .. } => "execution_failed",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn rec() -> InstitutionalEffectRecord {
        InstitutionalEffectRecord::new(
            "prop-1",
            "coop-a",
            None,
            "appoint_steward",
            Some("did:icn:s".into()),
            Some("us-east".into()),
            None,
            10,
            serde_json::json!({}),
        )
    }

    fn ev(success: bool, err: Option<&str>, at: u64) -> EffectDispatchEvidence {
        EffectDispatchEvidence::new(
            "rec-1",
            "prop-1",
            "sdis",
            Some("state-hash-x".into()),
            success,
            err.map(String::from),
            at,
        )
    }

    #[test]
    fn empty_evidence_yields_emitted_only() {
        assert_eq!(
            derive_reconciliation_status(&rec(), &[]),
            ReconciliationStatus::EmittedOnly
        );
    }

    #[test]
    fn single_success_yields_execution_evidenced() {
        let evidence = vec![ev(true, None, 10)];
        assert_eq!(
            derive_reconciliation_status(&rec(), &evidence),
            ReconciliationStatus::ExecutionEvidenced
        );
    }

    #[test]
    fn single_failure_yields_execution_failed_with_error() {
        let evidence = vec![ev(false, Some("boom"), 10)];
        assert_eq!(
            derive_reconciliation_status(&rec(), &evidence),
            ReconciliationStatus::ExecutionFailed {
                error: Some("boom".into())
            }
        );
    }

    #[test]
    fn later_success_does_not_erase_earlier_failure() {
        // Audit-discipline: any failure sticks.
        let evidence = vec![ev(false, Some("boom"), 10), ev(true, None, 20)];
        match derive_reconciliation_status(&rec(), &evidence) {
            ReconciliationStatus::ExecutionFailed { error } => {
                assert_eq!(error.as_deref(), Some("boom"));
            }
            other => panic!("expected ExecutionFailed, got {other:?}"),
        }
    }

    #[test]
    fn most_recent_failure_message_wins_when_multiple_failures() {
        let evidence = vec![ev(false, Some("older"), 10), ev(false, Some("newer"), 20)];
        match derive_reconciliation_status(&rec(), &evidence) {
            ReconciliationStatus::ExecutionFailed { error } => {
                assert_eq!(error.as_deref(), Some("newer"));
            }
            other => panic!("expected ExecutionFailed, got {other:?}"),
        }
    }

    #[test]
    fn labels_are_stable_snake_case() {
        assert_eq!(
            reconciliation_label(&ReconciliationStatus::EmittedOnly),
            "emitted_only"
        );
        assert_eq!(
            reconciliation_label(&ReconciliationStatus::ExecutionEvidenced),
            "execution_evidenced"
        );
        assert_eq!(
            reconciliation_label(&ReconciliationStatus::ExecutionFailed { error: None }),
            "execution_failed"
        );
    }
}
