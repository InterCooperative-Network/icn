//! Runtime-slice integration tests for the eighth `ProcessTransitionReceipt`
//! class, `EvidencePacketProducedReceipt` (#2314 design/audit contract + #2316
//! EP1–EP5 decision rung, issue #2317).
//!
//! These exercise `GovernanceManager::record_evidence_packet_produced` end to
//! end through an opaque-backed test store (the analog of the production
//! gateway-backed `ReceiptStore`), proving:
//!
//! 1. it requires an already-opened session and a verified immediate
//!    predecessor (`MutationAppliedReceipt`) in the same `(domain, session)`;
//! 2. the source set is canonicalized (order-independent), rejects duplicates,
//!    and must include the immediate predecessor;
//! 3. persistence is idempotent on stable identity and fail-closed on a
//!    conflicting re-record;
//! 4. no packet/profile/source body is ever stored.
//!
//! Mirrors `mutation_applied_receipt_runtime_slice.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{
    EvidencePacketSourceRef, GovernanceDecisionReceipt, GovernanceDomainId, ProcessGateKind,
    ProcessGateResult,
};
use icn_governance_actor::manager::{
    ActivationCrossedOutcome, DecisionRecordOutcome, EvidencePacketProducedOutcome,
    GovernanceManager, MutationAppliedOutcome, MutationPlanRecordedOutcome,
    ProcessSessionOpenOutcome,
};
use icn_governance_actor::receipt_backend::{
    evidence_packet_produced_composite_key1, GovernanceReceiptBackend,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-backed test store — exercises the trait's typed defaults end-to-end.
// ============================================================================

type ChainKey = (String, String, Option<String>);
type ChainEntry = (u64, [u8; 32], Vec<u8>);

#[derive(Default)]
struct OpaqueUniqueBackend {
    chains: Mutex<HashMap<ChainKey, Vec<ChainEntry>>>,
    unique: Mutex<HashMap<ChainKey, [u8; 32]>>,
}

impl OpaqueUniqueBackend {
    fn chain_len(&self, class: &str, key1: &str, key2: Option<&str>) -> usize {
        self.chains
            .lock()
            .unwrap()
            .get(&(class.to_string(), key1.to_string(), key2.map(String::from)))
            .map_or(0, Vec::len)
    }

    fn raw_payload(&self, class: &str, key1: &str, key2: Option<&str>) -> Option<Vec<u8>> {
        self.chains
            .lock()
            .unwrap()
            .get(&(class.to_string(), key1.to_string(), key2.map(String::from)))
            .and_then(|chain| chain.first().map(|(_, _, p)| p.clone()))
    }
}

impl GovernanceReceiptBackend for OpaqueUniqueBackend {
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

    fn put_opaque(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
        recorded_at: u64,
        record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<(), String> {
        let key = (class.to_string(), key1.to_string(), key2.map(String::from));
        self.chains.lock().unwrap().entry(key).or_default().push((
            recorded_at,
            record_hash,
            payload.to_vec(),
        ));
        Ok(())
    }

    fn put_opaque_if_absent(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
        recorded_at: u64,
        record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<Option<[u8; 32]>, String> {
        let key = (class.to_string(), key1.to_string(), key2.map(String::from));
        let mut unique = self.unique.lock().unwrap();
        if let Some(winner) = unique.get(&key) {
            return Ok(Some(*winner));
        }
        unique.insert(key.clone(), record_hash);
        self.chains.lock().unwrap().entry(key).or_default().push((
            recorded_at,
            record_hash,
            payload.to_vec(),
        ));
        Ok(None)
    }

    fn get_latest_opaque(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
    ) -> Result<Option<Vec<u8>>, String> {
        let key = (class.to_string(), key1.to_string(), key2.map(String::from));
        Ok(self.chains.lock().unwrap().get(&key).and_then(|chain| {
            chain
                .iter()
                .max_by_key(|(t, h, _)| (*t, *h))
                .map(|(_, _, p)| p.clone())
        }))
    }

    fn list_opaque_for(&self, class: &str, key1: &str) -> Result<Vec<Vec<u8>>, String> {
        let chains = self.chains.lock().unwrap();
        let mut hits: Vec<ChainEntry> = chains
            .iter()
            .filter(|((c, k1, _), _)| c == class && k1 == key1)
            .flat_map(|(_, chain)| chain.iter().cloned())
            .collect();
        hits.sort_by_key(|(t, h, _)| (*t, *h));
        Ok(hits.into_iter().map(|(_, _, p)| p).collect())
    }
}

/// Backend that persists everything except the evidence-packet-produced insert
/// — proves fail-closed packet persistence with all preconditions satisfied
/// (the full applied chain is recorded through the same store first).
#[derive(Default)]
struct FailingEvidencePacketProducedBackend {
    inner: OpaqueUniqueBackend,
}

impl GovernanceReceiptBackend for FailingEvidencePacketProducedBackend {
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
    fn put_opaque(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
        recorded_at: u64,
        record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<(), String> {
        self.inner
            .put_opaque(class, key1, key2, recorded_at, record_hash, payload)
    }
    fn put_opaque_if_absent(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
        recorded_at: u64,
        record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<Option<[u8; 32]>, String> {
        if class == "evidence_packet_produced" {
            return Err("simulated evidence-packet-produced backend failure".to_string());
        }
        self.inner
            .put_opaque_if_absent(class, key1, key2, recorded_at, record_hash, payload)
    }
    fn get_latest_opaque(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
    ) -> Result<Option<Vec<u8>>, String> {
        self.inner.get_latest_opaque(class, key1, key2)
    }
    fn list_opaque_for(&self, class: &str, key1: &str) -> Result<Vec<Vec<u8>>, String> {
        self.inner.list_opaque_for(class, key1)
    }
}

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

fn make_manager() -> (GovernanceManager, Arc<OpaqueUniqueBackend>) {
    let store = Arc::new(OpaqueUniqueBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    (mgr, store)
}

fn coop_test() -> GovernanceDomainId {
    GovernanceDomainId::new("coop:test")
}

fn pkt_key1(domain: &str, session: &str) -> String {
    evidence_packet_produced_composite_key1(domain, session)
}

fn open_session(mgr: &GovernanceManager, domain: &GovernanceDomainId, session: &str, by: &Did) {
    match mgr
        .record_process_session_opened(domain, session, by)
        .unwrap()
    {
        ProcessSessionOpenOutcome::Opened(_) | ProcessSessionOpenOutcome::AlreadyOpened(_) => {}
    }
}

/// Record the full chain up to a recorded `MutationAppliedReceipt` and return
/// its `record_hash` — the EP1 immediate-predecessor link a produced packet
/// references. The activation/plan ids are derived from `application_id`.
fn setup_applied(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    application_id: &str,
    by: &Did,
) -> [u8; 32] {
    let plan_id = format!("plan-for-{application_id}");
    let activation_id = format!("activation-for-{plan_id}");
    open_session(mgr, domain, session, by);
    let decision_id = format!("decision-for-{activation_id}");
    let decision_hash = match mgr
        .record_decision(domain, session, &decision_id, by, [9u8; 32])
        .unwrap()
    {
        DecisionRecordOutcome::Recorded(r) | DecisionRecordOutcome::AlreadyRecorded(r) => {
            r.record_hash
        }
    };
    let gate_hash = mgr
        .record_process_gate_result(
            domain,
            session,
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            by,
        )
        .unwrap()
        .record_hash;
    let activation_hash = match mgr
        .record_activation_crossed(
            domain,
            session,
            &activation_id,
            &decision_id,
            decision_hash,
            &[gate_hash],
            by,
        )
        .unwrap()
    {
        ActivationCrossedOutcome::Crossed(r) | ActivationCrossedOutcome::AlreadyCrossed(r) => {
            r.record_hash
        }
    };
    let plan_hash = match mgr
        .record_mutation_plan_recorded(
            domain,
            session,
            &plan_id,
            &activation_id,
            activation_hash,
            [5u8; 32],
            by,
        )
        .unwrap()
    {
        MutationPlanRecordedOutcome::Recorded(r)
        | MutationPlanRecordedOutcome::AlreadyRecorded(r) => r.record_hash,
    };
    match mgr
        .record_mutation_applied(
            domain,
            session,
            application_id,
            &plan_id,
            plan_hash,
            [4u8; 32],
            by,
        )
        .unwrap()
    {
        MutationAppliedOutcome::Applied(r) | MutationAppliedOutcome::AlreadyApplied(r) => {
            r.record_hash
        }
    }
}

/// Minimal valid source set: the immediate predecessor (MutationApplied,
/// ladder 6). Callers may extend it.
fn source_set(applied_hash: [u8; 32]) -> Vec<EvidencePacketSourceRef> {
    vec![EvidencePacketSourceRef {
        ladder_position: 6,
        record_hash: applied_hash,
    }]
}

// ============================================================================
// Happy path — construct, persist, retrieve, verify EP1 link
// ============================================================================

#[test]
fn packet_persists_and_returns_receipt_with_verified_predecessor() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);

    let outcome = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &source_set(ah),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect("first packet must succeed");
    let EvidencePacketProducedOutcome::Produced(receipt) = outcome else {
        panic!("first packet must be Produced, got {outcome:?}");
    };
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.packet_id, "packet-001");
    assert_eq!(receipt.mutation_application_id, "application-001");
    assert_eq!(receipt.produced_by, actor.to_string());
    assert_eq!(receipt.packet_hash, [5u8; 32]);
    assert_eq!(receipt.redaction_profile_hash, [9u8; 32]);
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");
    assert_ne!(receipt.receipt_set_hash, [0u8; 32], "real set hash");
    // EP1: mutation_applied_record_hash equals the persisted applied receipt hash.
    assert_eq!(
        receipt.mutation_applied_record_hash, ah,
        "EP1 proof link binds the recorded application's real record_hash"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_produced",
            &pkt_key1("coop:test", "session-001"),
            Some("packet-001"),
        ),
        1,
        "exactly one persisted packet"
    );
    let read = mgr
        .get_evidence_packet_produced(&domain, "session-001", "packet-001")
        .unwrap()
        .expect("point read must hydrate");
    assert_eq!(read, receipt);
}

// ============================================================================
// Idempotency + conflict
// ============================================================================

#[test]
fn same_identity_retry_returns_original_never_restamped() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);

    let first = match mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &source_set(ah),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .unwrap()
    {
        EvidencePacketProducedOutcome::Produced(r) => r,
        other => panic!("expected Produced, got {other:?}"),
    };
    // Retry with byte-identical inputs — same stable identity, no restamp.
    let retry = match mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &source_set(ah),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .unwrap()
    {
        EvidencePacketProducedOutcome::AlreadyProduced(r) => r,
        other => panic!("expected AlreadyProduced, got {other:?}"),
    };
    assert_eq!(retry, first, "retry returns the original, never restamped");
    assert_eq!(retry.produced_at, first.produced_at);
    assert_eq!(retry.record_hash, first.record_hash);
}

#[test]
fn caller_source_order_cannot_fork_receipt_set_hash() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    let applied = mgr
        .get_mutation_applied(&domain, "session-001", "application-001")
        .unwrap()
        .unwrap();
    let forward = vec![
        EvidencePacketSourceRef {
            ladder_position: 6,
            record_hash: ah,
        },
        EvidencePacketSourceRef {
            ladder_position: 5,
            record_hash: applied.plan_record_hash,
        },
    ];
    let mut reversed = forward.clone();
    reversed.reverse();

    let r1 = match mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &forward,
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .unwrap()
    {
        EvidencePacketProducedOutcome::Produced(r) => r,
        other => panic!("expected Produced, got {other:?}"),
    };
    // Re-record the same packet with the source set in reversed input order —
    // the canonical receipt_set_hash is identical, so this is a same-identity
    // idempotent retry, NOT a conflict.
    let r2 = match mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &reversed,
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .unwrap()
    {
        EvidencePacketProducedOutcome::AlreadyProduced(r) => r,
        other => panic!("reordered source set must not fork identity; got {other:?}"),
    };
    assert_eq!(r1.receipt_set_hash, r2.receipt_set_hash);
    assert_eq!(r1, r2);
}

#[test]
fn conflicting_identity_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);

    mgr.record_evidence_packet_produced(
        &domain,
        "session-001",
        "packet-001",
        "application-001",
        ah,
        &source_set(ah),
        [5u8; 32],
        [9u8; 32],
        &actor,
    )
    .expect("first packet");
    // Same packet_id, different packet_hash → conflict, fail closed.
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &source_set(ah),
            [6u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("conflicting re-record must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_conflict"),
        "expected conflict sentinel, got: {err}"
    );
}

// ============================================================================
// Predecessor verification (EP1) fail-closed
// ============================================================================

#[test]
fn missing_predecessor_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    // No MutationAppliedReceipt recorded.
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-404",
            [7u8; 32],
            &source_set([7u8; 32]),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("must fail closed when the predecessor is absent");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_predecessor_not_found"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_produced",
            &pkt_key1("coop:test", "session-001"),
            Some("packet-001"),
        ),
        0,
        "nothing persisted"
    );
}

#[test]
fn predecessor_hash_mismatch_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    // Supply a wrong mutation_applied_record_hash for a real application id.
    let wrong = [0xEEu8; 32];
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            wrong,
            &source_set(wrong),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("mismatched predecessor hash must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_predecessor_mismatch"),
        "got: {err}"
    );
    let _ = ah;
}

#[test]
fn wrong_session_predecessor_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    // Applied recorded in session-A; try to produce a packet in session-B.
    let ah = setup_applied(&mgr, &domain, "session-A", "application-001", &actor);
    open_session(&mgr, &domain, "session-B", &actor);
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-B",
            "packet-001",
            "application-001",
            ah,
            &source_set(ah),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("cross-session predecessor must fail closed as not found");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_predecessor_not_found"),
        "got: {err}"
    );
}

// ============================================================================
// Source-set validation (EP1/EP2) fail-closed
// ============================================================================

#[test]
fn source_set_missing_predecessor_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    // A non-empty source set that does NOT include the immediate predecessor.
    let bad = vec![EvidencePacketSourceRef {
        ladder_position: 5,
        record_hash: [1u8; 32],
    }];
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &bad,
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("source set without the predecessor must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_predecessor_not_in_set"),
        "got: {err}"
    );
}

#[test]
fn duplicate_source_record_hash_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    let dup = vec![
        EvidencePacketSourceRef {
            ladder_position: 6,
            record_hash: ah,
        },
        EvidencePacketSourceRef {
            ladder_position: 5,
            record_hash: ah,
        },
    ];
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &dup,
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("duplicate source member must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_duplicate_source"),
        "got: {err}"
    );
}

#[test]
fn empty_source_set_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    // An empty source set fails fast with the dedicated stable prefix, ahead of
    // the more specific predecessor-in-set check.
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &[],
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("empty source set must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_empty_source_set"),
        "got: {err}"
    );
}

// ============================================================================
// Session precondition / id validation / store wiring / backend failure
// ============================================================================

#[test]
fn unopened_session_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-unopened",
            "packet-001",
            "application-001",
            [7u8; 32],
            &source_set([7u8; 32]),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("unopened session must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_produced_session_not_opened"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_produced",
            &pkt_key1("coop:test", "session-unopened"),
            Some("packet-001"),
        ),
        0
    );
}

#[test]
fn whitespace_ids_rejected() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    for (session, packet, app) in [
        ("   ", "packet-001", "application-001"),
        ("session-001", "  ", "application-001"),
        ("session-001", "packet-001", "   "),
    ] {
        let err = mgr
            .record_evidence_packet_produced(
                &domain,
                session,
                packet,
                app,
                ah,
                &source_set(ah),
                [5u8; 32],
                [9u8; 32],
                &actor,
            )
            .expect_err("whitespace id must be rejected");
        assert!(err.to_string().contains("non-whitespace"), "got: {err}");
    }
}

#[test]
fn no_receipt_store_fails_closed() {
    let mgr = GovernanceManager::new(); // no receipt store wired
    let actor = fresh_did();
    let domain = coop_test();
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            [7u8; 32],
            &source_set([7u8; 32]),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("a receipt store is required");
    assert!(
        err.to_string().contains("a receipt store is required"),
        "got: {err}"
    );
}

#[test]
fn backend_failure_propagates() {
    let store = Arc::new(FailingEvidencePacketProducedBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    let err = mgr
        .record_evidence_packet_produced(
            &domain,
            "session-001",
            "packet-001",
            "application-001",
            ah,
            &source_set(ah),
            [5u8; 32],
            [9u8; 32],
            &actor,
        )
        .expect_err("backend failure must propagate");
    assert!(
        err.to_string()
            .contains("simulated evidence-packet-produced backend failure"),
        "got: {err}"
    );
}

// ============================================================================
// Isolation, key injectivity, privacy audit
// ============================================================================

#[test]
fn two_domain_isolation() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    let ah1 = setup_applied(&mgr, &d1, "shared-session", "application-001", &actor);
    let ah2 = setup_applied(&mgr, &d2, "shared-session", "application-001", &actor);

    mgr.record_evidence_packet_produced(
        &d1,
        "shared-session",
        "packet-001",
        "application-001",
        ah1,
        &source_set(ah1),
        [5u8; 32],
        [9u8; 32],
        &actor,
    )
    .expect("d1 packet");
    mgr.record_evidence_packet_produced(
        &d2,
        "shared-session",
        "packet-001",
        "application-001",
        ah2,
        &source_set(ah2),
        [5u8; 32],
        [9u8; 32],
        &actor,
    )
    .expect("d2 packet");

    let r1 = mgr
        .get_evidence_packet_produced(&d1, "shared-session", "packet-001")
        .unwrap()
        .unwrap();
    let r2 = mgr
        .get_evidence_packet_produced(&d2, "shared-session", "packet-001")
        .unwrap()
        .unwrap();
    assert_eq!(r1.domain_id, "coop:one");
    assert_eq!(r2.domain_id, "coop:two");
    assert_ne!(r1.record_hash, r2.record_hash, "two domains never mix");
}

#[test]
fn composite_key1_is_injective() {
    // ("ab","c") and ("a","bc") must not alias.
    assert_ne!(
        evidence_packet_produced_composite_key1("ab", "c"),
        evidence_packet_produced_composite_key1("a", "bc"),
    );
}

#[test]
fn stored_payload_carries_no_bodies() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ah = setup_applied(&mgr, &domain, "session-001", "application-001", &actor);
    mgr.record_evidence_packet_produced(
        &domain,
        "session-001",
        "packet-001",
        "application-001",
        ah,
        &source_set(ah),
        [5u8; 32],
        [9u8; 32],
        &actor,
    )
    .expect("packet");
    let raw = store
        .raw_payload(
            "evidence_packet_produced",
            &pkt_key1("coop:test", "session-001"),
            Some("packet-001"),
        )
        .expect("payload present");
    let json = String::from_utf8(raw).expect("utf8");
    for forbidden in [
        "packet_body",
        "profile_body",
        "redaction_profile_id",
        "human_at_status",
        "source_receipt_bod",
        "plan_body",
        "applied_result_body",
        "operation_list",
        "target_list",
        "effect_payload",
    ] {
        assert!(
            !json.contains(forbidden),
            "stored payload must not carry `{forbidden}`; got: {json}"
        );
    }
}
