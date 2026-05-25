//! `accepted-is-not-applied` ReconciliationStatus invariants — integration
//! tests that lock the contract `apps/governance/src/dispatch_evidence.rs`
//! defines, exercised through a `GovernanceReceiptBackend` round-trip and a
//! wire-format round-trip.
//!
//! The unit tests inside `dispatch_evidence.rs` (in `#[cfg(test)] mod tests`)
//! already prove the pure-function behavior of `derive_reconciliation_status`.
//! This integration suite locks the *additional* invariants that the abuse-
//! case hardening doctrine (`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`
//! §2.4 "Accepted is not applied", §3.5 "receipt halo effect") relies on:
//!
//! 1. **Failure-stickiness survives the backend round-trip.** Even when a
//!    later success entry exists, listing-then-deriving against the
//!    `GovernanceReceiptBackend` must still yield `ExecutionFailed`.
//! 2. **`max_by_key(recorded_at)` precedence is preserved through the query
//!    path.** Insertion-order-last is not the same as recorded-at-latest;
//!    derivation must use the latter, even when the backend yields evidence
//!    in insertion order.
//! 3. **`EffectOutcome` is orthogonal to status derivation.** Every
//!    `EffectOutcome` × `success` combination yields a status determined by
//!    `success` alone. Catches a regression where someone "improves"
//!    derivation by also reading `outcome`.
//! 4. **Multi-record independence.** Two records, two evidence chains; each
//!    derives independently from the others' evidence.
//! 5. **Wire-format round-trip.** Every variant serializes via
//!    `serde_json::to_value` and deserializes back via
//!    `serde_json::from_value` to itself.
//! 6. **Wire-tag stability.** The closed snake_case taxonomy
//!    (`emitted_only`, `execution_evidenced`, `execution_failed`) is locked
//!    against serde renames. Member-shell-v0 and steward-cockpit-v0 both
//!    consume these labels; a rename would propagate silently to UI layers.
//! 7. **Field-level forward-compat.** A legacy `EffectDispatchEvidence`
//!    payload that omits the optional `outcome` / `receipt_ref` /
//!    `error_message` fields deserializes cleanly under the
//!    `#[serde(default, skip_serializing_if = "Option::is_none")]` shape.
//!    Note: this test does **not** claim that unknown enum-tag values
//!    deserialize safely — Rust's `#[serde(tag = "status")]` deserializer
//!    rejects unknown tags by design, which is intentional.
//! 8. **Empty backend yields `EmittedOnly`.** The "accepted but no dispatch
//!    evidence has landed yet" surface — the literal `accepted ≠ applied`
//!    state — is locked at integration level.
//!
//! Tests are read-only against production code; no `dispatch_evidence.rs`,
//! no `receipt_backend.rs`, no handler, no other crate is modified.
//!
//! Refs #1873.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_governance::GovernanceDecisionReceipt;
use icn_governance_actor::{
    dispatch_evidence::{
        derive_reconciliation_status, reconciliation_label, EffectDispatchEvidence,
        ReconciliationStatus,
    },
    institutional_effect::InstitutionalEffectRecord,
    receipt_backend::GovernanceReceiptBackend,
};
use icn_kernel_api::{effects::EffectOutcome, AllocationReceipt, Hash};
use std::sync::Mutex;

/// In-memory `GovernanceReceiptBackend` specialized for reconciliation-status
/// tests. Stores `InstitutionalEffectRecord` and `EffectDispatchEvidence`;
/// every other trait method that lacks a usable default is stubbed.
///
/// Mirrors the `MemoryReceiptBackend` pattern from
/// `actor_path_dispatch_evidence_sink.rs`; the difference is that this one
/// is intentionally narrower — it does not pretend to support governance
/// receipts, allocations, mandates, or grants. Tests that need those use
/// the sibling file's broader backend.
struct ReconBackend {
    effects: Mutex<Vec<InstitutionalEffectRecord>>,
    evidence: Mutex<Vec<EffectDispatchEvidence>>,
}

impl ReconBackend {
    fn new() -> Self {
        Self {
            effects: Mutex::new(vec![]),
            evidence: Mutex::new(vec![]),
        }
    }
}

impl GovernanceReceiptBackend for ReconBackend {
    fn put_governance(&self, _: &GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_institutional_effect(&self, record: &InstitutionalEffectRecord) -> Result<(), String> {
        self.effects.lock().unwrap().push(record.clone());
        Ok(())
    }
    fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(self
            .effects
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.proposal_id == proposal_id)
            .cloned()
            .collect())
    }
    fn put_effect_dispatch_evidence(&self, ev: &EffectDispatchEvidence) -> Result<(), String> {
        self.evidence.lock().unwrap().push(ev.clone());
        Ok(())
    }
    fn list_effect_dispatch_evidence_by_record(
        &self,
        effect_record_id: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, String> {
        Ok(self
            .evidence
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.effect_record_id == effect_record_id)
            .cloned()
            .collect())
    }
}

fn fresh_record() -> InstitutionalEffectRecord {
    InstitutionalEffectRecord::new(
        "prop-recon-1",
        "did:icn:coop:reconciliation-test",
        None,
        "appoint_steward",
        Some("did:icn:steward-x".into()),
        None,
        None,
        100,
        serde_json::json!({}),
    )
}

fn evidence_for(
    record: &InstitutionalEffectRecord,
    success: bool,
    err: Option<&str>,
    outcome: Option<EffectOutcome>,
    recorded_at: u64,
) -> EffectDispatchEvidence {
    EffectDispatchEvidence::new(
        record.record_id.clone(),
        record.proposal_id.clone(),
        "sdis",
        Some("steward-id-hash".into()),
        success,
        err.map(String::from),
        outcome,
        recorded_at,
    )
}

#[test]
fn empty_evidence_list_after_backend_query_yields_emitted_only() {
    // "Accepted but no dispatch evidence has landed yet" surface: a record
    // is persisted; zero evidence rows exist; the backend's listing yields
    // an empty vec; derivation must yield `EmittedOnly`.
    let backend = ReconBackend::new();
    let record = fresh_record();
    backend.put_institutional_effect(&record).unwrap();

    let evidence = backend
        .list_effect_dispatch_evidence_by_record(&record.record_id)
        .unwrap();
    assert!(
        evidence.is_empty(),
        "no evidence was written for this record"
    );

    let status = derive_reconciliation_status(&record, &evidence);
    assert_eq!(status, ReconciliationStatus::EmittedOnly);
}

#[test]
fn failure_stickiness_survives_later_success_added_to_backend() {
    // A failure entry, then a later success entry; even after the backend
    // round-trip, derivation must yield `ExecutionFailed` and surface the
    // failure's error message. The unit test
    // `later_success_does_not_erase_earlier_failure` covers the pure
    // function; this locks the same invariant through the backend write/
    // read path.
    let backend = ReconBackend::new();
    let record = fresh_record();
    backend.put_institutional_effect(&record).unwrap();

    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &record,
            false,
            Some("boom"),
            Some(EffectOutcome::Failed),
            100,
        ))
        .unwrap();
    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &record,
            true,
            None,
            Some(EffectOutcome::Applied),
            200,
        ))
        .unwrap();

    let evidence = backend
        .list_effect_dispatch_evidence_by_record(&record.record_id)
        .unwrap();
    assert_eq!(evidence.len(), 2);

    match derive_reconciliation_status(&record, &evidence) {
        ReconciliationStatus::ExecutionFailed { error } => {
            assert_eq!(
                error.as_deref(),
                Some("boom"),
                "failure must stick — a later success does not erase the audit trail"
            );
        }
        other => panic!("expected ExecutionFailed (failure-stickiness invariant); got {other:?}"),
    }
}

#[test]
fn failure_stickiness_with_out_of_order_recorded_at() {
    // Backend insertion order chronologically interleaves:
    //   t=100 success, t=300 fail-A (latest), t=400 success, t=200 fail-B.
    // Two failures present, neither is insertion-last; derivation must pick
    // the most-recent-by-`recorded_at` (fail-A at t=300), not insertion-
    // order-last (fail-B at t=200). The pure-function unit test
    // `most_recent_failure_message_wins_when_multiple_failures` covers this
    // shape; this locks it *through* `list_effect_dispatch_evidence_by_record`.
    let backend = ReconBackend::new();
    let record = fresh_record();
    backend.put_institutional_effect(&record).unwrap();

    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &record,
            true,
            None,
            Some(EffectOutcome::Applied),
            100,
        ))
        .unwrap();
    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &record,
            false,
            Some("fail-A-latest"),
            Some(EffectOutcome::Failed),
            300,
        ))
        .unwrap();
    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &record,
            true,
            None,
            Some(EffectOutcome::Applied),
            400,
        ))
        .unwrap();
    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &record,
            false,
            Some("fail-B-earlier"),
            Some(EffectOutcome::Failed),
            200,
        ))
        .unwrap();

    let evidence = backend
        .list_effect_dispatch_evidence_by_record(&record.record_id)
        .unwrap();
    assert_eq!(evidence.len(), 4);

    match derive_reconciliation_status(&record, &evidence) {
        ReconciliationStatus::ExecutionFailed { error } => {
            assert_eq!(
                error.as_deref(),
                Some("fail-A-latest"),
                "expected latest-by-recorded_at failure (t=300), not insertion-order-last failure (t=200)"
            );
        }
        other => panic!("expected ExecutionFailed, got {other:?}"),
    }
}

#[test]
fn outcome_is_orthogonal_to_status_derivation() {
    // Status derivation reads `success` only, never `outcome`. Locks the
    // boundary against a future regression that tries to "improve"
    // derivation by also reading the `outcome` field.
    let record = fresh_record();
    let outcomes: &[Option<EffectOutcome>] = &[
        None,
        Some(EffectOutcome::Applied),
        Some(EffectOutcome::NoOp),
        Some(EffectOutcome::Partial),
        Some(EffectOutcome::Failed),
    ];

    for outcome in outcomes.iter().copied() {
        for success in [true, false] {
            let err = if success { None } else { Some("err") };
            let ev = evidence_for(&record, success, err, outcome, 100);
            let status = derive_reconciliation_status(&record, &[ev]);

            if success {
                assert_eq!(
                    status,
                    ReconciliationStatus::ExecutionEvidenced,
                    "outcome={outcome:?}, success=true must yield ExecutionEvidenced regardless of outcome class"
                );
            } else {
                match status {
                    ReconciliationStatus::ExecutionFailed { .. } => {}
                    other => panic!(
                        "outcome={outcome:?}, success=false must yield ExecutionFailed; got {other:?}"
                    ),
                }
            }
        }
    }
}

#[test]
fn multi_record_independence() {
    // Two independent records, two independent evidence chains. Listing
    // one record's evidence must not cross-pollinate the other's
    // derivation. Catches a regression where a shared-by-proposal index
    // accidentally collapses status across records.
    let backend = ReconBackend::new();
    let rec_a = InstitutionalEffectRecord::new(
        "prop-A",
        "did:icn:coop:t",
        None,
        "appoint_steward",
        Some("did:icn:a".into()),
        None,
        None,
        100,
        serde_json::json!({}),
    );
    let rec_b = InstitutionalEffectRecord::new(
        "prop-B",
        "did:icn:coop:t",
        None,
        "appoint_steward",
        Some("did:icn:b".into()),
        None,
        None,
        100,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&rec_a).unwrap();
    backend.put_institutional_effect(&rec_b).unwrap();
    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &rec_a,
            true,
            None,
            Some(EffectOutcome::Applied),
            100,
        ))
        .unwrap();
    backend
        .put_effect_dispatch_evidence(&evidence_for(
            &rec_b,
            false,
            Some("b-fails"),
            Some(EffectOutcome::Failed),
            100,
        ))
        .unwrap();

    let ev_a = backend
        .list_effect_dispatch_evidence_by_record(&rec_a.record_id)
        .unwrap();
    let ev_b = backend
        .list_effect_dispatch_evidence_by_record(&rec_b.record_id)
        .unwrap();
    assert_eq!(ev_a.len(), 1, "record A should see only its own evidence");
    assert_eq!(ev_b.len(), 1, "record B should see only its own evidence");

    assert_eq!(
        derive_reconciliation_status(&rec_a, &ev_a),
        ReconciliationStatus::ExecutionEvidenced
    );
    match derive_reconciliation_status(&rec_b, &ev_b) {
        ReconciliationStatus::ExecutionFailed { error } => {
            assert_eq!(error.as_deref(), Some("b-fails"));
        }
        other => panic!("expected B to be ExecutionFailed; got {other:?}"),
    }
}

#[test]
fn wire_format_round_trip_all_three_variants() {
    // Every variant must serialize via `serde_json::to_value` and deserialize
    // back to itself via `serde_json::from_value`. Compares parsed `Value`s,
    // not raw strings — object key order is implementation-defined.
    let variants = [
        ReconciliationStatus::EmittedOnly,
        ReconciliationStatus::ExecutionEvidenced,
        ReconciliationStatus::ExecutionFailed { error: None },
        ReconciliationStatus::ExecutionFailed {
            error: Some("boom".into()),
        },
    ];

    for variant in &variants {
        let value: serde_json::Value =
            serde_json::to_value(variant).expect("serialize ReconciliationStatus");
        let parsed: ReconciliationStatus =
            serde_json::from_value(value.clone()).expect("deserialize ReconciliationStatus");
        assert_eq!(
            &parsed, variant,
            "round-trip mismatch on {variant:?}; intermediate value was {value}"
        );
    }
}

#[test]
fn wire_format_tag_strings_are_locked() {
    // Lock the closed snake_case taxonomy as a wire contract. Compare parsed
    // `serde_json::Value` field-by-field — never raw JSON strings, because
    // object key order is implementation-defined and a string compare would
    // be brittle.
    let cases: &[(ReconciliationStatus, &str, Option<&str>)] = &[
        (ReconciliationStatus::EmittedOnly, "emitted_only", None),
        (
            ReconciliationStatus::ExecutionEvidenced,
            "execution_evidenced",
            None,
        ),
        (
            ReconciliationStatus::ExecutionFailed {
                error: Some("err".into()),
            },
            "execution_failed",
            Some("err"),
        ),
        (
            ReconciliationStatus::ExecutionFailed { error: None },
            "execution_failed",
            None,
        ),
    ];

    for (variant, expected_tag, expected_error) in cases {
        let value = serde_json::to_value(variant).expect("serialize");
        let obj = value
            .as_object()
            .unwrap_or_else(|| panic!("{variant:?} must serialize as a JSON object"));

        assert_eq!(
            obj.get("status").and_then(|v| v.as_str()),
            Some(*expected_tag),
            "status tag mismatch for {variant:?}"
        );

        // The `error` field may be absent or null when None. Both are
        // acceptable; we only require that the *parsed* string value
        // matches what we expect.
        let actual_error = obj.get("error").and_then(|v| v.as_str());
        assert_eq!(
            actual_error, *expected_error,
            "error field mismatch for {variant:?}"
        );
    }

    // Integration-level confirmation that `reconciliation_label` is
    // publicly callable and stable across the closed taxonomy (the unit
    // test `labels_are_stable_snake_case` covers the same assertion at
    // unit level; this adds the public-API surface check).
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

#[test]
fn legacy_evidence_without_optional_fields_deserializes_cleanly() {
    // Field-level forward-compat: an `EffectDispatchEvidence` payload that
    // omits the optional `receipt_ref` / `error_message` / `outcome` fields
    // must deserialize cleanly under their
    // `#[serde(default, skip_serializing_if = "Option::is_none")]` shape
    // and still drive correct status derivation.
    //
    // NOTE: this test does NOT claim that unknown `ReconciliationStatus`
    // enum tags deserialize safely. The `#[serde(tag = "status")]`
    // deserializer rejects unknown tag values by design — that is
    // intentional, not a forward-compat gap, and is out of scope here.
    let legacy = serde_json::json!({
        "evidence_id": "11111111-1111-1111-1111-111111111111",
        "effect_record_id": "rec-legacy",
        "proposal_id": "prop-legacy",
        "subsystem": "sdis",
        "success": true,
        "recorded_at": 100u64
        // omitted on purpose: receipt_ref, error_message, outcome
    });

    let parsed: EffectDispatchEvidence =
        serde_json::from_value(legacy).expect("legacy payload must deserialize cleanly");
    assert_eq!(parsed.receipt_ref, None);
    assert_eq!(parsed.error_message, None);
    assert_eq!(parsed.outcome, None);
    assert!(parsed.success);
    assert_eq!(parsed.effect_record_id, "rec-legacy");

    let record = fresh_record();
    let status = derive_reconciliation_status(&record, &[parsed]);
    assert_eq!(
        status,
        ReconciliationStatus::ExecutionEvidenced,
        "legacy success-evidence must drive ExecutionEvidenced derivation"
    );
}
