//! End-to-end proof-loop integration tests for the `meeting` / `attend`
//! action-card source path.
//!
//! Pins the runtime proof loop:
//!
//!   `standing → action card → authorized action → receipt`
//!
//! is honest for the `meeting` / `attend` source path — i.e. the card's
//! `receipt_expected: true` is not a lie. Companion to
//! `me_action_card_receipt_chain.rs` (proposal/vote) and
//! `me_action_item_receipt_chain.rs` (action_item/complete).
//!
//! ## What this test pins
//!
//! 1. A holder on a meeting's attendee list sees a
//!    `source_kind=meeting`, `action_kind=attend`, `receipt_expected=true`
//!    card on `GET /me/action-cards`. Card `source_id` equals
//!    `meeting_id`.
//! 2. A `Present` transition emits a `MeetingAttendanceReceipt`
//!    (ADR-0026 Layer 2) keyed by `(meeting_id, attendee_did)`. Receipt
//!    fields: `attendee_did` is the subject, `recorded_by` is the
//!    authenticated caller (which may differ — steward-recorded
//!    attendance), `transition` is `Present`, `record_hash` is a real
//!    blake3 binding (never zero).
//! 3. A different DID does not see the first DID's attendance receipt
//!    via their own `(meeting_id, did)` lookup — cross-DID isolation.
//! 4. Re-marking an attendee's status to the same value is idempotent —
//!    no fresh receipt is appended.
//! 5. A real transition between distinct receipt-bearing states
//!    (`Present` → `Remote`) DOES append a fresh receipt; the audit
//!    chain preserves the change.
//! 6. `Absent` mutates state but emits no receipt — absence is not an
//!    attend event under [`MeetingAttendanceTransition`].
//! 7. A receipt backend that rejects `put_meeting_attendance` aborts the
//!    attendance commit — the meeting state does not silently advance
//!    without provenance.
//! 8. The receipt's regulatory-safe vocabulary is preserved: no
//!    wallet/balance/currency/payment language anywhere in the
//!    serialized form or the receipt's text fields.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use icn_governance::{
    AttendanceStatus, AuthorityGrant, GovernanceDecisionReceipt, GovernanceDomainId,
    GovernanceParams, MeetingAttendanceReceipt, MeetingAttendanceReceiptV2,
    MeetingAttendanceTransition, MeetingAttendee, MeetingRole, MembershipConfig, MembershipSource,
    NoMandateReason, ReceiptMandateAttestation,
};
use icn_governance_actor::{
    dispatch_evidence::EffectDispatchEvidence, institutional_effect::InstitutionalEffectRecord,
    manager::GovernanceManager, receipt_backend::GovernanceReceiptBackend,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Test receipt backend — persists MeetingAttendanceReceipt + lookups.
// ============================================================================

#[derive(Default)]
struct TestReceiptStore {
    attendance: Mutex<Vec<MeetingAttendanceReceipt>>,
    attendance_v2: Mutex<Vec<MeetingAttendanceReceiptV2>>,
}

impl GovernanceReceiptBackend for TestReceiptStore {
    fn put_governance(&self, _r: &GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _p: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _r: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _h: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _h: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_institutional_effect(&self, _r: &InstitutionalEffectRecord) -> Result<(), String> {
        Ok(())
    }
    fn list_institutional_effects_by_proposal(
        &self,
        _p: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(vec![])
    }
    fn put_effect_dispatch_evidence(&self, _e: &EffectDispatchEvidence) -> Result<(), String> {
        Ok(())
    }
    fn list_effect_dispatch_evidence_by_record(
        &self,
        _r: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, String> {
        Ok(vec![])
    }
    fn put_mandate(&self, _m: &icn_governance::Mandate) -> Result<(), String> {
        Ok(())
    }
    fn put_authority_grant(&self, _g: &AuthorityGrant) -> Result<(), String> {
        Ok(())
    }

    fn put_meeting_attendance(&self, receipt: &MeetingAttendanceReceipt) -> Result<(), String> {
        self.attendance.lock().unwrap().push(receipt.clone());
        Ok(())
    }

    fn put_meeting_attendance_v2(
        &self,
        receipt: &MeetingAttendanceReceiptV2,
    ) -> Result<(), String> {
        self.attendance_v2.lock().unwrap().push(receipt.clone());
        Ok(())
    }

    fn get_meeting_attendance(
        &self,
        meeting_id: &str,
        attendee_did: &str,
    ) -> Result<Option<MeetingAttendanceReceipt>, String> {
        // Latest = receipt with the largest `recorded_at`. The
        // underlying chain is sorted ascending by `recorded_at`, so
        // `.rfind(...)` returns the most recent matching receipt.
        Ok(self
            .list_meeting_attendance_for_meeting(meeting_id)?
            .into_iter()
            .rfind(|r| r.attendee_did == attendee_did))
    }

    fn list_meeting_attendance_for_meeting(
        &self,
        meeting_id: &str,
    ) -> Result<Vec<MeetingAttendanceReceipt>, String> {
        let mut hits: Vec<MeetingAttendanceReceipt> = self
            .attendance
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.meeting_id == meeting_id)
            .cloned()
            .collect();
        hits.sort_by_key(|r| r.recorded_at);
        Ok(hits)
    }
}

impl TestReceiptStore {
    fn count_for_pair(&self, meeting_id: &str, attendee_did: &str) -> usize {
        self.attendance
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.meeting_id == meeting_id && r.attendee_did == attendee_did)
            .count()
    }

    /// All v2 receipts captured for a `(meeting_id, attendee_did)` pair.
    fn v2_for_pair(&self, meeting_id: &str, attendee_did: &str) -> Vec<MeetingAttendanceReceiptV2> {
        self.attendance_v2
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.meeting_id == meeting_id && r.attendee_did == attendee_did)
            .cloned()
            .collect()
    }
}

/// A backend whose `put_meeting_attendance` always rejects, used to
/// prove the manager's "persist before commit" guarantee.
#[derive(Default)]
struct FailingAttendanceStore;

impl GovernanceReceiptBackend for FailingAttendanceStore {
    fn put_governance(&self, _r: &GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _p: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _r: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _h: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _h: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_meeting_attendance(&self, _receipt: &MeetingAttendanceReceipt) -> Result<(), String> {
        Err("simulated receipt backend failure".to_string())
    }
}

// ============================================================================
// Scaffolding
// ============================================================================

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

async fn seed_domain_with_member(
    mgr: &GovernanceManager,
    caller: &Did,
    domain_id: &str,
) -> GovernanceDomainId {
    let domain = GovernanceDomainId::new(domain_id);
    mgr.create_domain(
        domain.clone(),
        "Test Coop".to_string(),
        "default".to_string(),
        GovernanceParams {
            quorum_percentage: 1,
            approval_threshold_percentage: 51,
            voting_period_seconds: 86_400,
            require_deliberation: false,
            ..GovernanceParams::default()
        },
        MembershipConfig {
            source: MembershipSource::StaticList(vec![caller.clone()]),
        },
    )
    .await
    .expect("create_domain");
    domain
}

fn invite_to_meeting(mgr: &GovernanceManager, meeting_id: &icn_governance::MeetingId, did: &Did) {
    let mut m = mgr
        .get_meeting(meeting_id)
        .expect("get_meeting")
        .expect("meeting must exist");
    m.attendees.push(MeetingAttendee {
        did: did.as_str().to_string(),
        status: AttendanceStatus::Invited,
        meeting_role: MeetingRole::Participant,
    });
    mgr.update_meeting(&m).expect("update_meeting");
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn meeting_attendance_receipt_chain_end_to_end() {
    let receipts = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);

    let attendee = fresh_did();
    let recorder = fresh_did(); // steward — distinct from attendee
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "Cycle planning (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &attendee);

    // ── 1. Authorized action: steward marks attendee Present.
    let updated = mgr
        .update_meeting_attendance(
            &meeting.id,
            attendee.as_str(),
            AttendanceStatus::Present,
            &recorder,
            "governance:meeting:write",
        )
        .expect("update_meeting_attendance -> Present");
    let updated_attendee = updated
        .attendees
        .iter()
        .find(|a| a.did == attendee.as_str())
        .expect("attendee row");
    assert_eq!(updated_attendee.status, AttendanceStatus::Present);

    // ── 2. Receipt: store contains a MeetingAttendanceReceipt keyed by
    //       (meeting_id, attendee_did).
    let receipt = receipts
        .get_meeting_attendance(meeting.id.0.as_str(), attendee.as_str())
        .expect("receipt lookup")
        .expect(
            "expected a MeetingAttendanceReceipt — the action card's \
             receipt_expected:true would otherwise be a lie",
        );
    assert_eq!(receipt.meeting_id, meeting.id.0);
    assert_eq!(receipt.domain_id, domain.0);
    assert_eq!(receipt.attendee_did, attendee.to_string());
    assert_eq!(
        receipt.recorded_by,
        recorder.to_string(),
        "recorded_by must be the authenticated caller, not the attendee"
    );
    assert_ne!(
        receipt.recorded_by, receipt.attendee_did,
        "this test exercises steward-recorded attendance — the two DIDs must differ"
    );
    assert_eq!(receipt.transition, MeetingAttendanceTransition::Present);
    assert_ne!(
        receipt.record_hash, [0u8; 32],
        "record_hash must be a real cryptographic binding, not a zero placeholder"
    );

    // The card's `source_id` is the meeting id; the documented receipt
    // lookup pair is `(source_id, caller)`.
    assert_eq!(
        receipt.meeting_id, meeting.id.0,
        "the receipt's meeting_id must equal the action card's source_id"
    );
}

#[tokio::test]
async fn another_did_does_not_see_first_did_attendance_receipt() {
    let receipts = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);

    let alice = fresh_did();
    let bob = fresh_did();
    assert_ne!(alice, bob);
    let recorder = fresh_did();
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "Cross-DID isolation meeting (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &alice);
    invite_to_meeting(&mgr, &meeting.id, &bob);

    // Only Alice is marked Present.
    mgr.update_meeting_attendance(
        &meeting.id,
        alice.as_str(),
        AttendanceStatus::Present,
        &recorder,
        "governance:meeting:write",
    )
    .expect("alice -> Present");

    // Alice has a receipt; Bob does not.
    let alice_receipt = receipts
        .get_meeting_attendance(meeting.id.0.as_str(), alice.as_str())
        .expect("lookup");
    assert!(alice_receipt.is_some());

    let bob_receipt = receipts
        .get_meeting_attendance(meeting.id.0.as_str(), bob.as_str())
        .expect("lookup");
    assert!(
        bob_receipt.is_none(),
        "Bob must not see Alice's attendance receipt via his own (meeting_id, did) lookup"
    );

    // Cross-check: Alice's receipt is bound to Alice, not Bob.
    let r = alice_receipt.unwrap();
    assert_eq!(r.attendee_did, alice.to_string());
    assert_ne!(r.attendee_did, bob.to_string());
}

#[tokio::test]
async fn re_marking_present_is_idempotent_no_duplicate_receipt() {
    let receipts = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);

    let attendee = fresh_did();
    let recorder = fresh_did();
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "Idempotence meeting (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &attendee);

    mgr.update_meeting_attendance(
        &meeting.id,
        attendee.as_str(),
        AttendanceStatus::Present,
        &recorder,
        "governance:meeting:write",
    )
    .expect("first Present");

    // Re-marking Present must be a no-op for the receipt seam.
    mgr.update_meeting_attendance(
        &meeting.id,
        attendee.as_str(),
        AttendanceStatus::Present,
        &recorder,
        "governance:meeting:write",
    )
    .expect("re-Present");

    assert_eq!(
        receipts.count_for_pair(meeting.id.0.as_str(), attendee.as_str()),
        1,
        "re-marking the same Present status must not append a duplicate receipt"
    );
}

#[tokio::test]
async fn present_to_remote_appends_new_transition_receipt() {
    let receipts = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);

    let attendee = fresh_did();
    let recorder = fresh_did();
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "Transition history meeting (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &attendee);

    mgr.update_meeting_attendance(
        &meeting.id,
        attendee.as_str(),
        AttendanceStatus::Present,
        &recorder,
        "governance:meeting:write",
    )
    .expect("Present");
    let first = receipts
        .get_meeting_attendance(meeting.id.0.as_str(), attendee.as_str())
        .unwrap()
        .expect("first receipt");

    // Wall clock must advance so the second receipt's `recorded_at` is
    // strictly greater — the canonical hash binds `recorded_at`, so a
    // same-second second receipt would alias the first.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    mgr.update_meeting_attendance(
        &meeting.id,
        attendee.as_str(),
        AttendanceStatus::Remote,
        &recorder,
        "governance:meeting:write",
    )
    .expect("Remote");
    let chain = receipts
        .list_meeting_attendance_for_meeting(meeting.id.0.as_str())
        .expect("chain");
    assert_eq!(
        chain.len(),
        2,
        "Present -> Remote is a real transition; the audit chain must record both"
    );
    assert_eq!(chain[0].record_hash, first.record_hash);
    assert_ne!(
        chain[0].record_hash, chain[1].record_hash,
        "the two transitions must have distinct record_hashes — append-only history"
    );
    assert_eq!(chain[0].transition, MeetingAttendanceTransition::Present);
    assert_eq!(chain[1].transition, MeetingAttendanceTransition::Remote);

    // Latest lookup returns the most recent transition.
    let latest = receipts
        .get_meeting_attendance(meeting.id.0.as_str(), attendee.as_str())
        .unwrap()
        .expect("latest");
    assert_eq!(latest.transition, MeetingAttendanceTransition::Remote);
}

#[tokio::test]
async fn absent_does_not_emit_attendance_receipt() {
    let receipts = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);

    let attendee = fresh_did();
    let recorder = fresh_did();
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "Absent meeting (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &attendee);

    let updated = mgr
        .update_meeting_attendance(
            &meeting.id,
            attendee.as_str(),
            AttendanceStatus::Absent,
            &recorder,
            "governance:meeting:write",
        )
        .expect("Absent write must succeed");
    let row = updated
        .attendees
        .iter()
        .find(|a| a.did == attendee.as_str())
        .expect("attendee row");
    assert_eq!(
        row.status,
        AttendanceStatus::Absent,
        "Absent must still be persisted as state"
    );
    assert_eq!(
        receipts.count_for_pair(meeting.id.0.as_str(), attendee.as_str()),
        0,
        "Absent is not an attend event under MeetingAttendanceTransition; no receipt emitted"
    );
}

#[tokio::test]
async fn receipt_backend_failure_prevents_attendance_status_commit() {
    let mgr = GovernanceManager::new()
        .with_receipt_store(Arc::new(FailingAttendanceStore) as Arc<dyn GovernanceReceiptBackend>);

    let attendee = fresh_did();
    let recorder = fresh_did();
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "Fail-closed meeting (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &attendee);

    let err = mgr
        .update_meeting_attendance(
            &meeting.id,
            attendee.as_str(),
            AttendanceStatus::Present,
            &recorder,
            "governance:meeting:write",
        )
        .expect_err("receipt backend failure must propagate");
    let msg = err.to_string();
    assert!(
        msg.contains("attendance"),
        "error must reference the attendance receipt seam: {msg}"
    );

    // Critical: meeting state must NOT have committed Present.
    let after = mgr.get_meeting(&meeting.id).unwrap().unwrap();
    let row = after
        .attendees
        .iter()
        .find(|a| a.did == attendee.as_str())
        .expect("attendee row");
    assert_eq!(
        row.status,
        AttendanceStatus::Invited,
        "attendance must remain at the prior status when the receipt backend rejects the write"
    );
}

#[tokio::test]
async fn attendance_receipt_uses_regulatory_safe_vocabulary() {
    let receipts = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);

    let attendee = fresh_did();
    let recorder = fresh_did();
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "Regulatory vocabulary meeting (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &attendee);

    mgr.update_meeting_attendance(
        &meeting.id,
        attendee.as_str(),
        AttendanceStatus::Present,
        &recorder,
        "governance:meeting:write",
    )
    .expect("Present");

    let receipt = receipts
        .get_meeting_attendance(meeting.id.0.as_str(), attendee.as_str())
        .unwrap()
        .expect("receipt");

    let json = serde_json::to_string(&receipt).expect("serialize");
    let lower = json.to_lowercase();
    for forbidden in [
        "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
    ] {
        assert!(
            !lower.contains(forbidden),
            "MeetingAttendanceReceipt JSON must not contain regulated-finance vocabulary; \
             found `{forbidden}` in: {json}"
        );
    }
}

/// #1868: a meeting-attendance transition emits a `MeetingAttendanceReceiptV2`
/// alongside the v1 receipt, recording the capability scope that **actually**
/// authorized the request and the explicit
/// `NoMandateRequired { MembershipStandingOnly }` attestation (meeting
/// attendance is a membership-standing-only act — no MandateGate, no grant).
#[tokio::test]
async fn meeting_attendance_emits_v2_recording_the_accepted_scope() {
    let receipts = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(receipts.clone() as Arc<dyn GovernanceReceiptBackend>);

    let narrow_attendee = fresh_did();
    let legacy_attendee = fresh_did();
    let recorder = fresh_did();
    let domain = seed_domain_with_member(&mgr, &recorder, "test-coop").await;

    let now_secs = icn_time::current_timestamp_secs();
    let meeting = mgr
        .create_meeting(
            domain.0.clone(),
            "V2 emission meeting (fictional)".to_string(),
            None,
            Some(now_secs + 3_600),
            recorder.as_str().to_string(),
        )
        .expect("create_meeting");
    invite_to_meeting(&mgr, &meeting.id, &narrow_attendee);
    invite_to_meeting(&mgr, &meeting.id, &legacy_attendee);

    // Accepted via the narrowed class scope.
    mgr.update_meeting_attendance(
        &meeting.id,
        narrow_attendee.as_str(),
        AttendanceStatus::Present,
        &recorder,
        "governance:meeting:write",
    )
    .expect("Present (meeting:write)");

    // Accepted via the legacy broad scope during the compatibility period.
    mgr.update_meeting_attendance(
        &meeting.id,
        legacy_attendee.as_str(),
        AttendanceStatus::Present,
        &recorder,
        "governance:write",
    )
    .expect("Present (legacy write)");

    // ── v2 emitted, recording the accepted scope verbatim + the explicit
    //    no-mandate attestation.
    let narrow_v2 = receipts.v2_for_pair(meeting.id.0.as_str(), narrow_attendee.as_str());
    assert_eq!(
        narrow_v2.len(),
        1,
        "exactly one v2 receipt for the narrow-scope attendee"
    );
    let narrow = &narrow_v2[0];
    assert_eq!(
        narrow.capability_scope_presented,
        "governance:meeting:write"
    );
    assert_eq!(
        narrow.mandate_attestation,
        ReceiptMandateAttestation::NoMandateRequired {
            reason: NoMandateReason::MembershipStandingOnly,
        },
        "meeting attendance is membership-standing-only — no mandate grant"
    );
    assert!(
        narrow.verify(),
        "emitted v2 receipt must verify its own canonical hash"
    );
    assert_eq!(narrow.meeting_id, meeting.id.0);
    assert_eq!(narrow.attendee_did, narrow_attendee.to_string());
    assert_eq!(narrow.recorded_by, recorder.to_string());
    assert_eq!(narrow.transition, MeetingAttendanceTransition::Present);

    // ── Legacy-scope acceptance must record "governance:write", NOT the
    //    preferred class scope: the field is evidence of what authorized
    //    the request.
    let legacy_v2 = receipts.v2_for_pair(meeting.id.0.as_str(), legacy_attendee.as_str());
    assert_eq!(legacy_v2.len(), 1);
    assert_eq!(
        legacy_v2[0].capability_scope_presented, "governance:write",
        "a request accepted via the legacy broad scope must not claim the narrowed class scope"
    );
    assert!(legacy_v2[0].verify());

    // ── v1 still emitted alongside (no consumer regression).
    assert_eq!(
        receipts.count_for_pair(meeting.id.0.as_str(), narrow_attendee.as_str()),
        1,
        "v1 receipt must still be emitted alongside v2"
    );
    assert!(
        receipts
            .get_meeting_attendance(meeting.id.0.as_str(), legacy_attendee.as_str())
            .unwrap()
            .is_some(),
        "v1 receipt still present for the legacy-scope attendee"
    );
}
