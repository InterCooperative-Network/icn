//! Runtime-slice integration tests for the tenth process/evidence receipt
//! class, `EvidencePacketMadeAvailableReceipt` (#2330 access/made-available/
//! disclosure decision rung, issue #2332).
//!
//! These exercise `GovernanceManager::record_evidence_packet_made_available`
//! end to end through an opaque-backed test store (the analog of the
//! production gateway-backed `ReceiptStore`), proving:
//!
//! 1. it requires an already-opened session and a verified export-prepared
//!    predecessor (`EvidencePacketExportPreparedReceipt`) in the same
//!    `(domain, session)` under `export_id`;
//! 2. the echoed `packet_id`, `packet_hash`, and `recipient_scope_id` are
//!    verified against the stored export-prepared receipt (availability must be
//!    to the scope the export was prepared for) — a mismatch fails closed;
//! 3. persistence is idempotent on stable identity and fail-closed on a
//!    conflicting re-record; multiple availabilities per export (distinct
//!    `availability_id`s) succeed;
//! 4. no packet/policy/method body, custody location, or transmission/access
//!    claim is ever stored — made available is not retrieved, accessed,
//!    delivered, received, or accepted.
//!
//! Mirrors `evidence_packet_export_prepared_receipt_runtime_slice.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{
    EvidencePacketSourceRef, GovernanceDecisionReceipt, GovernanceDomainId, ProcessGateKind,
    ProcessGateResult,
};
use icn_governance_actor::manager::{
    ActivationCrossedOutcome, DecisionRecordOutcome, EvidencePacketExportPreparedOutcome,
    EvidencePacketMadeAvailableOutcome, EvidencePacketProducedOutcome, GovernanceManager,
    MutationAppliedOutcome, MutationPlanRecordedOutcome, ProcessSessionOpenOutcome,
};
use icn_governance_actor::receipt_backend::{
    evidence_packet_made_available_composite_key1, GovernanceReceiptBackend,
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

/// Backend that persists everything except the made-available insert — proves
/// fail-closed availability persistence with all preconditions satisfied (the
/// full produced + export-prepared chain is recorded through the same store
/// first).
#[derive(Default)]
struct FailingEvidencePacketMadeAvailableBackend {
    inner: OpaqueUniqueBackend,
}

impl GovernanceReceiptBackend for FailingEvidencePacketMadeAvailableBackend {
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
        if class == "evidence_packet_made_available" {
            return Err("simulated evidence-packet-made-available backend failure".to_string());
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

fn mav_key1(domain: &str, session: &str) -> String {
    evidence_packet_made_available_composite_key1(domain, session)
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
/// its `record_hash`. Mirrors the sibling slices' helper.
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

/// The packet-artifact fingerprint every produced fixture in this file uses —
/// echoed transitively into the export-prepared and made-available receipts.
const PACKET_HASH: [u8; 32] = [5u8; 32];
/// The export policy fingerprint every export-prepared fixture uses.
const EXPORT_POLICY_HASH: [u8; 32] = [3u8; 32];
/// The disclosure policy fingerprint made-available fixtures use (R6).
const DISCLOSURE_POLICY_HASH: [u8; 32] = [11u8; 32];
/// The availability method fingerprint made-available fixtures use (R6).
const AVAILABILITY_METHOD_HASH: [u8; 32] = [12u8; 32];

/// Record the full chain through a recorded `EvidencePacketProducedReceipt` for
/// `packet_id` and return its `record_hash`.
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

/// Record the full chain through a recorded `EvidencePacketExportPreparedReceipt`
/// for `(export_id, packet_id, scope)` and return the export-prepared receipt's
/// `record_hash` — the D4 predecessor link a made-available receipt references.
fn setup_export_prepared(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    export_id: &str,
    packet_id: &str,
    scope: &str,
    by: &Did,
) -> [u8; 32] {
    let ph = setup_produced(mgr, domain, session, packet_id, by);
    match mgr
        .record_evidence_packet_export_prepared(
            domain,
            session,
            export_id,
            packet_id,
            ph,
            PACKET_HASH,
            EXPORT_POLICY_HASH,
            scope,
            by,
        )
        .unwrap()
    {
        EvidencePacketExportPreparedOutcome::Prepared(r)
        | EvidencePacketExportPreparedOutcome::AlreadyPrepared(r) => r.record_hash,
    }
}

// ============================================================================
// Happy path — construct, persist, retrieve, verify D4 links
// ============================================================================

#[test]
fn made_available_persists_and_returns_receipt_with_verified_links() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );

    let outcome = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect("first availability must succeed");
    let EvidencePacketMadeAvailableOutcome::MadeAvailable(receipt) = outcome else {
        panic!("first availability must be MadeAvailable, got {outcome:?}");
    };
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.availability_id, "availability-001");
    assert_eq!(receipt.export_id, "export-001");
    assert_eq!(receipt.packet_id, "packet-001");
    assert_eq!(receipt.recipient_scope_id, "scope-partner-review");
    assert_eq!(receipt.disclosure_policy_hash, DISCLOSURE_POLICY_HASH);
    assert_eq!(receipt.availability_method_hash, AVAILABILITY_METHOD_HASH);
    assert_eq!(receipt.made_available_by, actor.to_string());
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");
    // D4: export_prepared_record_hash equals the persisted export-prepared
    // receipt hash; packet_hash is the verified echo of its artifact fingerprint.
    assert_eq!(
        receipt.export_prepared_record_hash, eph,
        "D4 proof link binds the recorded export's real record_hash"
    );
    assert_eq!(
        receipt.packet_hash, PACKET_HASH,
        "D4 echo binds the export-prepared receipt's packet_hash"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_made_available",
            &mav_key1("coop:test", "session-001"),
            Some("availability-001"),
        ),
        1,
        "exactly one persisted availability"
    );
    let read = mgr
        .get_evidence_packet_made_available(&domain, "session-001", "availability-001")
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
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );

    let first = match mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .unwrap()
    {
        EvidencePacketMadeAvailableOutcome::MadeAvailable(r) => r,
        other => panic!("expected MadeAvailable, got {other:?}"),
    };
    // Retry with byte-identical inputs — same stable identity, no restamp.
    let retry = match mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .unwrap()
    {
        EvidencePacketMadeAvailableOutcome::AlreadyMadeAvailable(r) => r,
        other => panic!("expected AlreadyMadeAvailable, got {other:?}"),
    };
    assert_eq!(retry, first, "retry returns the original, never restamped");
    assert_eq!(retry.made_available_at, first.made_available_at);
    assert_eq!(retry.record_hash, first.record_hash);
}

#[test]
fn conflicting_identity_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );

    mgr.record_evidence_packet_made_available(
        &domain,
        "session-001",
        "availability-001",
        "export-001",
        "packet-001",
        eph,
        PACKET_HASH,
        "scope-partner-review",
        DISCLOSURE_POLICY_HASH,
        AVAILABILITY_METHOD_HASH,
        &actor,
    )
    .expect("first availability");
    // Same availability_id, different disclosure policy → conflict, fail closed.
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            [99u8; 32],
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("conflicting re-record must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_conflict"),
        "expected conflict sentinel, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_made_available",
            &mav_key1("coop:test", "session-001"),
            Some("availability-001"),
        ),
        1,
        "the original availability remains the only persisted record"
    );
}

#[test]
fn multiple_availabilities_per_export_with_distinct_ids_succeed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );

    // Same export, two availability ids (e.g. re-availability under a new
    // disclosure policy) — both recordable; how many is charter policy.
    for (availability_id, policy) in [
        ("availability-001", DISCLOSURE_POLICY_HASH),
        ("availability-002", [13u8; 32]),
    ] {
        let outcome = mgr
            .record_evidence_packet_made_available(
                &domain,
                "session-001",
                availability_id,
                "export-001",
                "packet-001",
                eph,
                PACKET_HASH,
                "scope-partner-review",
                policy,
                AVAILABILITY_METHOD_HASH,
                &actor,
            )
            .expect("distinct availability id must succeed");
        assert!(matches!(
            outcome,
            EvidencePacketMadeAvailableOutcome::MadeAvailable(_)
        ));
    }
}

// ============================================================================
// Fail-closed predecessor / echoed-field verification
// ============================================================================

#[test]
fn missing_predecessor_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    // Session opened, but NO export-prepared receipt recorded.
    open_session(&mgr, &domain, "session-001", &actor);
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            [7u8; 32],
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("missing predecessor must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_predecessor_not_found"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_made_available",
            &mav_key1("coop:test", "session-001"),
            Some("availability-001"),
        ),
        0
    );
}

#[test]
fn predecessor_hash_mismatch_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    // Correct export_id, wrong export_prepared_record_hash.
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            [99u8; 32],
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("predecessor hash mismatch must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_predecessor_mismatch"),
        "got: {err}"
    );
}

#[test]
fn packet_id_mismatch_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    // Correct predecessor hash, wrong packet_id echo.
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-999",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("packet_id echo mismatch must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_packet_id_mismatch"),
        "got: {err}"
    );
}

#[test]
fn packet_hash_mismatch_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    // Correct predecessor hash + packet_id, wrong packet_hash echo.
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            [99u8; 32],
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("packet_hash echo mismatch must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_packet_hash_mismatch"),
        "got: {err}"
    );
}

#[test]
fn recipient_scope_mismatch_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    // Correct predecessor hash + packet echoes, wrong recipient_scope_id — the
    // availability must be to the scope the export was prepared for.
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-other",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("recipient scope echo mismatch must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_recipient_scope_mismatch"),
        "got: {err}"
    );
}

#[test]
fn wrong_session_predecessor_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    // A different session has no such export-prepared receipt → not_found.
    open_session(&mgr, &domain, "session-002", &actor);
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-002",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("wrong-session predecessor must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_predecessor_not_found"),
        "got: {err}"
    );
}

#[test]
fn wrong_domain_predecessor_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    let eph = setup_export_prepared(
        &mgr,
        &d1,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    // The export-prepared lives in d1; d2 cannot reference it.
    open_session(&mgr, &d2, "session-001", &actor);
    let err = mgr
        .record_evidence_packet_made_available(
            &d2,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("wrong-domain predecessor must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_predecessor_not_found"),
        "got: {err}"
    );
}

// ============================================================================
// Precondition failures — session, ids, store, backend
// ============================================================================

#[test]
fn unopened_session_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-unopened",
            "availability-001",
            "export-001",
            "packet-001",
            [7u8; 32],
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("unopened session must fail closed");
    assert!(
        err.to_string()
            .starts_with("evidence_packet_made_available_session_not_opened"),
        "got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "evidence_packet_made_available",
            &mav_key1("coop:test", "session-unopened"),
            Some("availability-001"),
        ),
        0
    );
}

#[test]
fn whitespace_ids_rejected() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    for (session, availability, export, packet, scope) in [
        (
            "   ",
            "availability-001",
            "export-001",
            "packet-001",
            "scope-partner-review",
        ),
        (
            "session-001",
            "  ",
            "export-001",
            "packet-001",
            "scope-partner-review",
        ),
        (
            "session-001",
            "availability-001",
            "  ",
            "packet-001",
            "scope-partner-review",
        ),
        (
            "session-001",
            "availability-001",
            "export-001",
            "   ",
            "scope-partner-review",
        ),
        (
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            "  ",
        ),
    ] {
        let err = mgr
            .record_evidence_packet_made_available(
                &domain,
                session,
                availability,
                export,
                packet,
                eph,
                PACKET_HASH,
                scope,
                DISCLOSURE_POLICY_HASH,
                AVAILABILITY_METHOD_HASH,
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
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            [7u8; 32],
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
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
    let store = Arc::new(FailingEvidencePacketMadeAvailableBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    let err = mgr
        .record_evidence_packet_made_available(
            &domain,
            "session-001",
            "availability-001",
            "export-001",
            "packet-001",
            eph,
            PACKET_HASH,
            "scope-partner-review",
            DISCLOSURE_POLICY_HASH,
            AVAILABILITY_METHOD_HASH,
            &actor,
        )
        .expect_err("backend failure must propagate");
    assert!(
        err.to_string()
            .contains("simulated evidence-packet-made-available backend failure"),
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
    let eph1 = setup_export_prepared(
        &mgr,
        &d1,
        "shared-session",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    let eph2 = setup_export_prepared(
        &mgr,
        &d2,
        "shared-session",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );

    mgr.record_evidence_packet_made_available(
        &d1,
        "shared-session",
        "availability-001",
        "export-001",
        "packet-001",
        eph1,
        PACKET_HASH,
        "scope-partner-review",
        DISCLOSURE_POLICY_HASH,
        AVAILABILITY_METHOD_HASH,
        &actor,
    )
    .expect("d1 availability");
    mgr.record_evidence_packet_made_available(
        &d2,
        "shared-session",
        "availability-001",
        "export-001",
        "packet-001",
        eph2,
        PACKET_HASH,
        "scope-partner-review",
        DISCLOSURE_POLICY_HASH,
        AVAILABILITY_METHOD_HASH,
        &actor,
    )
    .expect("d2 availability");

    let r1 = mgr
        .get_evidence_packet_made_available(&d1, "shared-session", "availability-001")
        .unwrap()
        .unwrap();
    let r2 = mgr
        .get_evidence_packet_made_available(&d2, "shared-session", "availability-001")
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
        evidence_packet_made_available_composite_key1("ab", "c"),
        evidence_packet_made_available_composite_key1("a", "bc"),
    );
}

#[test]
fn stored_payload_carries_no_bodies_or_contact_data() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let eph = setup_export_prepared(
        &mgr,
        &domain,
        "session-001",
        "export-001",
        "packet-001",
        "scope-partner-review",
        &actor,
    );
    mgr.record_evidence_packet_made_available(
        &domain,
        "session-001",
        "availability-001",
        "export-001",
        "packet-001",
        eph,
        PACKET_HASH,
        "scope-partner-review",
        DISCLOSURE_POLICY_HASH,
        AVAILABILITY_METHOD_HASH,
        &actor,
    )
    .expect("availability");
    let raw = store
        .raw_payload(
            "evidence_packet_made_available",
            &mav_key1("coop:test", "session-001"),
            Some("availability-001"),
        )
        .expect("payload present");
    let json = String::from_utf8(raw).expect("utf8");
    // NOTE: "availability" is core vocabulary for this class — not forbidden.
    for forbidden in [
        // deliberately-absent semantics (R1/R6)
        "retrieved",
        "accessed",
        "delivered",
        "transmitted",
        "received",
        "accepted",
        "audited",
        "certified",
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
        "disclosure_policy_body",
        "availability_method_body",
        // recipient contact data (R6 — scope handle only)
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
