//! Runtime-slice integration tests for the ninth process/evidence receipt
//! class, `EvidencePacketExportPreparedReceipt` (#2322 boundary contract +
//! #2324 EX1–EX8 decision rung, issue #2325).
//!
//! These exercise `GovernanceManager::record_evidence_packet_export_prepared`
//! end to end through an opaque-backed test store (the analog of the
//! production gateway-backed `ReceiptStore`), proving:
//!
//! 1. it requires an already-opened session and a verified produced
//!    predecessor (`EvidencePacketProducedReceipt`) in the same
//!    `(domain, session)` under `packet_id`;
//! 2. the echoed `packet_hash` is verified against the stored produced
//!    receipt (the lane's first verified echoed content field) — a mismatch
//!    fails closed;
//! 3. persistence is idempotent on stable identity and fail-closed on a
//!    conflicting re-record; multiple exports per packet (distinct
//!    `export_id`s) succeed;
//! 4. no packet/policy body, contact data, or transmission/acceptance claim
//!    is ever stored — prepared is not delivered, received, or accepted.
//!
//! Mirrors `evidence_packet_produced_receipt_runtime_slice.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{
    EvidencePacketSourceRef, GovernanceDecisionReceipt, GovernanceDomainId, ProcessGateKind,
    ProcessGateResult,
};
use icn_governance_actor::manager::{
    ActivationCrossedOutcome, DecisionRecordOutcome, EvidencePacketExportPreparedOutcome,
    EvidencePacketProducedOutcome, GovernanceManager, MutationAppliedOutcome,
    MutationPlanRecordedOutcome, ProcessSessionOpenOutcome,
};
use icn_governance_actor::receipt_backend::{
    evidence_packet_export_prepared_composite_key1, GovernanceReceiptBackend,
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

/// Backend that persists everything except the export-prepared insert — proves
/// fail-closed export persistence with all preconditions satisfied (the full
/// produced chain is recorded through the same store first).
#[derive(Default)]
struct FailingEvidencePacketExportPreparedBackend {
    inner: OpaqueUniqueBackend,
}

impl GovernanceReceiptBackend for FailingEvidencePacketExportPreparedBackend {
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
        if class == "evidence_packet_export_prepared" {
            return Err("simulated evidence-packet-export-prepared backend failure".to_string());
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

fn exp_key1(domain: &str, session: &str) -> String {
    evidence_packet_export_prepared_composite_key1(domain, session)
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
/// its `record_hash`. The activation/plan ids are derived from
/// `application_id`. Mirrors the produced slice's helper.
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

/// The packet-artifact fingerprint every produced fixture in this file uses.
const PACKET_HASH: [u8; 32] = [5u8; 32];

/// Record the full chain through a recorded `EvidencePacketProducedReceipt`
/// for `packet_id` and return the produced receipt's `record_hash` — the EX5
/// predecessor link an export preparation references. The produced receipt's
/// stored `packet_hash` is [`PACKET_HASH`].
fn setup_produced(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    packet_id: &str,
    by: &Did,
) -> [u8; 32] {
    let application_id = format!("application-for-{packet_id}");
    let ah = setup_applied(mgr, domain, session, &application_id, by);
    let source = vec![EvidencePacketSourceRef {
        ladder_position: 6,
        record_hash: ah,
    }];
    match mgr
        .record_evidence_packet_produced(
            domain,
            session,
            packet_id,
            &application_id,
            ah,
            &source,
            PACKET_HASH,
            [9u8; 32],
            by,
        )
        .unwrap()
    {
        EvidencePacketProducedOutcome::Produced(r)
        | EvidencePacketProducedOutcome::AlreadyProduced(r) => r.record_hash,
    }
}

// ============================================================================
// Happy path — construct, persist, retrieve, verify EX5 links
// ============================================================================

#[test]
fn export_prepared_persists_and_returns_receipt_with_verified_links() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);

    let outcome = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            ph,
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect("first export preparation must succeed");
    let EvidencePacketExportPreparedOutcome::Prepared(receipt) = outcome else {
        panic!("first export preparation must be Prepared, got {outcome:?}");
    };
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.export_id, "export-001");
    assert_eq!(receipt.packet_id, "packet-001");
    assert_eq!(receipt.recipient_scope_id, "scope-partner-review");
    assert_eq!(receipt.prepared_by, actor.to_string());
    assert_eq!(receipt.export_policy_hash, [3u8; 32]);
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");
    // EX5: packet_produced_record_hash equals the persisted produced receipt
    // hash, and packet_hash is the verified echo of its artifact fingerprint.
    assert_eq!(
        receipt.packet_produced_record_hash, ph,
        "EX5 proof link binds the recorded packet's real record_hash"
    );
    assert_eq!(
        receipt.packet_hash, PACKET_HASH,
        "EX5 echo binds the produced receipt's stored packet_hash"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_export_prepared",
            &exp_key1("coop:test", "session-001"),
            Some("export-001"),
        ),
        1,
        "exactly one persisted export preparation"
    );
    let read = mgr
        .get_evidence_packet_export_prepared(&domain, "session-001", "export-001")
        .unwrap()
        .expect("point read must hydrate");
    assert_eq!(read, receipt);
}

// ============================================================================
// Idempotency, conflict, multiplicity
// ============================================================================

#[test]
fn same_identity_retry_returns_original_never_restamped() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);

    let first = match mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            ph,
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .unwrap()
    {
        EvidencePacketExportPreparedOutcome::Prepared(r) => r,
        other => panic!("expected Prepared, got {other:?}"),
    };
    // Retry with byte-identical inputs — same stable identity, no restamp.
    let retry = match mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            ph,
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .unwrap()
    {
        EvidencePacketExportPreparedOutcome::AlreadyPrepared(r) => r,
        other => panic!("expected AlreadyPrepared, got {other:?}"),
    };
    assert_eq!(retry, first, "retry returns the original, never restamped");
    assert_eq!(retry.prepared_at, first.prepared_at);
    assert_eq!(retry.record_hash, first.record_hash);
}

#[test]
fn conflicting_identity_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);

    mgr.record_evidence_packet_export_prepared(
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        ph,
        PACKET_HASH,
        [3u8; 32],
        "scope-partner-review",
        &actor,
    )
    .expect("first export preparation");
    // Same export_id, different recipient scope → conflict, fail closed.
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            ph,
            PACKET_HASH,
            [3u8; 32],
            "scope-other",
            &actor,
        )
        .expect_err("conflicting re-record must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_conflict"),
        "expected conflict sentinel, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_export_prepared",
            &exp_key1("coop:test", "session-001"),
            Some("export-001"),
        ),
        1,
        "the original preparation remains the only persisted record"
    );
}

#[test]
fn multiple_exports_per_packet_with_distinct_export_ids_succeed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);

    // Same packet, two scopes, two export ids — both recordable (how many is
    // charter policy, not substrate policy).
    for (export_id, scope) in [
        ("export-001", "scope-partner-review"),
        ("export-002", "scope-member-summary"),
    ] {
        let outcome = mgr
            .record_evidence_packet_export_prepared(
                &domain,
                "session-001",
                export_id,
                "packet-001",
                ph,
                PACKET_HASH,
                [3u8; 32],
                scope,
                &actor,
            )
            .expect("each distinct export_id must succeed");
        assert!(matches!(
            outcome,
            EvidencePacketExportPreparedOutcome::Prepared(_)
        ));
    }
    let listed = mgr
        .list_evidence_packet_export_prepared_in_domain(&domain, "session-001")
        .unwrap();
    assert_eq!(listed.len(), 2, "both preparations listed");
}

// ============================================================================
// Predecessor + echo verification (EX5) fail-closed
// ============================================================================

#[test]
fn missing_predecessor_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    // No EvidencePacketProducedReceipt recorded.
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-404",
            [6u8; 32],
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("must fail closed when the predecessor is absent");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_predecessor_not_found"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_export_prepared",
            &exp_key1("coop:test", "session-001"),
            Some("export-001"),
        ),
        0,
        "nothing persisted"
    );
}

#[test]
fn predecessor_hash_mismatch_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let _ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);
    // Supply a wrong packet_produced_record_hash for a real packet id.
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            [0xEEu8; 32],
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("mismatched predecessor hash must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_predecessor_mismatch"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_export_prepared",
            &exp_key1("coop:test", "session-001"),
            Some("export-001"),
        ),
        0,
        "nothing persisted"
    );
}

#[test]
fn cross_packet_reference_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    // Two real packets in the same session; supply packet B's record_hash
    // under packet A's id — the fetched receipt is A, so the link mismatches.
    let _ph_a = setup_produced(&mgr, &domain, "session-001", "packet-A", &actor);
    let ph_b = setup_produced(&mgr, &domain, "session-001", "packet-B", &actor);
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-A",
            ph_b,
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("cross-packet id/hash swap must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_predecessor_mismatch"),
        "got: {err}"
    );
}

#[test]
fn packet_hash_mismatch_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);
    // Correct predecessor record_hash, wrong echoed packet_hash — the verified
    // echo must fail closed (EX5).
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            ph,
            [0xAAu8; 32],
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("mismatched packet_hash echo must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_packet_hash_mismatch"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_export_prepared",
            &exp_key1("coop:test", "session-001"),
            Some("export-001"),
        ),
        0,
        "nothing persisted"
    );
}

#[test]
fn wrong_session_predecessor_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    // Packet produced in session-A; try to prepare an export in session-B.
    let ph = setup_produced(&mgr, &domain, "session-A", "packet-001", &actor);
    open_session(&mgr, &domain, "session-B", &actor);
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-B",
            "export-001",
            "packet-001",
            ph,
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("cross-session predecessor must fail closed as not found");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_predecessor_not_found"),
        "got: {err}"
    );
}

#[test]
fn wrong_domain_predecessor_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    // Packet produced in d1; d2 opens the same session name but has no packet.
    let ph = setup_produced(&mgr, &d1, "shared-session", "packet-001", &actor);
    open_session(&mgr, &d2, "shared-session", &actor);
    let err = mgr
        .record_evidence_packet_export_prepared(
            &d2,
            "shared-session",
            "export-001",
            "packet-001",
            ph,
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("cross-domain predecessor must fail closed as not found");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_predecessor_not_found"),
        "got: {err}"
    );
}

// ============================================================================
// Session anchoring, input validation, store wiring
// ============================================================================

#[test]
fn unopened_session_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-unopened",
            "export-001",
            "packet-001",
            [6u8; 32],
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("unopened session must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_export_prepared_session_not_opened"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_export_prepared",
            &exp_key1("coop:test", "session-unopened"),
            Some("export-001"),
        ),
        0
    );
}

#[test]
fn whitespace_ids_rejected() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);
    for (session, export, packet, scope) in [
        ("   ", "export-001", "packet-001", "scope-partner-review"),
        ("session-001", "  ", "packet-001", "scope-partner-review"),
        ("session-001", "export-001", "   ", "scope-partner-review"),
        ("session-001", "export-001", "packet-001", "  "),
    ] {
        let err = mgr
            .record_evidence_packet_export_prepared(
                &domain,
                session,
                export,
                packet,
                ph,
                PACKET_HASH,
                [3u8; 32],
                scope,
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
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            [6u8; 32],
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
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
    let store = Arc::new(FailingEvidencePacketExportPreparedBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);
    let err = mgr
        .record_evidence_packet_export_prepared(
            &domain,
            "session-001",
            "export-001",
            "packet-001",
            ph,
            PACKET_HASH,
            [3u8; 32],
            "scope-partner-review",
            &actor,
        )
        .expect_err("backend failure must propagate");
    assert!(
        err.to_string()
            .contains("simulated evidence-packet-export-prepared backend failure"),
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
    let ph1 = setup_produced(&mgr, &d1, "shared-session", "packet-001", &actor);
    let ph2 = setup_produced(&mgr, &d2, "shared-session", "packet-001", &actor);

    mgr.record_evidence_packet_export_prepared(
        &d1,
        "shared-session",
        "export-001",
        "packet-001",
        ph1,
        PACKET_HASH,
        [3u8; 32],
        "scope-partner-review",
        &actor,
    )
    .expect("d1 export preparation");
    mgr.record_evidence_packet_export_prepared(
        &d2,
        "shared-session",
        "export-001",
        "packet-001",
        ph2,
        PACKET_HASH,
        [3u8; 32],
        "scope-partner-review",
        &actor,
    )
    .expect("d2 export preparation");

    let r1 = mgr
        .get_evidence_packet_export_prepared(&d1, "shared-session", "export-001")
        .unwrap()
        .unwrap();
    let r2 = mgr
        .get_evidence_packet_export_prepared(&d2, "shared-session", "export-001")
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
        evidence_packet_export_prepared_composite_key1("ab", "c"),
        evidence_packet_export_prepared_composite_key1("a", "bc"),
    );
}

#[test]
fn stored_payload_carries_no_bodies_or_contact_data() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_produced(&mgr, &domain, "session-001", "packet-001", &actor);
    mgr.record_evidence_packet_export_prepared(
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        ph,
        PACKET_HASH,
        [3u8; 32],
        "scope-partner-review",
        &actor,
    )
    .expect("export preparation");
    let raw = store
        .raw_payload(
            "evidence_packet_export_prepared",
            &exp_key1("coop:test", "session-001"),
            Some("export-001"),
        )
        .expect("payload present");
    let json = String::from_utf8(raw).expect("utf8");
    for forbidden in [
        // deliberately-absent semantics (EX2/EX6/EX7)
        "delivered",
        "transmitted",
        "received",
        "accepted",
        "audited",
        "certified",
        "availability",
        "custody",
        "vault",
        "location",
        "endpoint",
        "retrieval",
        "credential",
        "status",
        "superseded",
        "withdrawn",
        "challenged",
        // bodies (never stored — fingerprints only)
        "packet_body",
        "export_policy_body",
        "redaction_profile_body",
        "source_receipt_bod",
        // recipient contact data (EX3 — scope handle only)
        "recipient_did",
        "recipient_name",
        "recipient_email",
        "recipient_phone",
        "recipient_address",
        "recipient_list",
        "recipient_scope_hash",
        "scope_definition",
        "@",
        // human/AT status (excluded; #2041 stays open)
        "human_at_status",
    ] {
        assert!(
            !json.contains(forbidden),
            "stored payload must not carry `{forbidden}`; got: {json}"
        );
    }
}
