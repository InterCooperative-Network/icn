//! Runtime proof for the fourth `ProcessTransitionReceipt` class —
//! [`DecisionRecordedReceipt`] — per the merged #2280 contract
//! (`docs/design/decision-recorded-receipt.md`) and the #2281 Q4 decision
//! (`docs/design/decision-recorded-q4-decision.md`).
//!
//! Pins the merged contract:
//!
//! 1. `GovernanceManager::record_decision` requires an already-opened
//!    session (stable prefix `decision_recorded_session_not_opened`;
//!    nothing written; no silent session creation), constructs a receipt
//!    with a real blake3 `record_hash`, persists it through the backend's
//!    atomic insert-if-absent BEFORE returning, and returns
//!    `Recorded(receipt)`.
//! 2. Retry with identical stable identity (`recorded_by` + `body_hash`)
//!    returns the ORIGINAL receipt unchanged (`AlreadyRecorded`) — never
//!    restamped, no second record. `recorded_at`/`record_hash` are NOT
//!    identity inputs.
//! 3. Same `decision_id` with a different `recorded_by` or `body_hash`
//!    fails closed with the stable `decision_recorded_conflict` prefix;
//!    the original is untouched.
//! 4. Empty ids rejected; a missing receipt store is an error; backend
//!    failure fails closed.
//! 5. Concurrent duplicate records serialize to exactly one persisted
//!    decision (atomic uniqueness at the storage layer).
//! 6. The composite storage key is injective: `("ab","c")` vs `("a","bc")`
//!    domain/session pairs never alias, and two domains sharing a
//!    `session_id` never mix decisions.
//! 7. List order is the store's deterministic `(recorded_at, record_hash)`
//!    chronological order — NOT arrival order; multiple decisions per
//!    session are permitted at the substrate layer.
//! 8. Existing session-open and deliberation-entry behavior is unchanged
//!    alongside decision recording.
//! 9. The persisted payload carries exactly the v1 field set — no body
//!    content, and none of the proposal/vote lineage's fields (no
//!    outcome, tally, vote hash, proposal id, or mandate attestation).
//!
//! This records a **generic recorded-decision fact** — parallel to and
//! never touching the proposal/vote `GovernanceDecisionReceipt` lineage
//! (#2281 Axis B). A receipt records an institutional fact and grants
//! zero authority; `recorded_by` is recorder evidence, not decider
//! identity.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{DeliberationEntryKind, GovernanceDecisionReceipt, GovernanceDomainId};
use icn_governance_actor::manager::{
    DecisionRecordOutcome, DeliberationEntryRecordOutcome, GovernanceManager,
    ProcessSessionOpenOutcome,
};
use icn_governance_actor::receipt_backend::{
    decision_recorded_composite_key1, GovernanceReceiptBackend,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-backed test store — the test analog of the production
// gateway-backed ReceiptStore: it implements the opaque primitives
// (including the atomic `put_opaque_if_absent`) and NO typed overrides,
// so the trait's typed decision-recorded defaults are exercised
// end-to-end (typed default → opaque insert-if-absent → map).
// ============================================================================

type ChainKey = (String, String, Option<String>);

/// One persisted opaque entry: `(recorded_at, record_hash, payload)`.
type ChainEntry = (u64, [u8; 32], Vec<u8>);

#[derive(Default)]
struct OpaqueUniqueBackend {
    /// `(class, key1, key2)` → chain of `(recorded_at, record_hash, payload)`.
    chains: Mutex<HashMap<ChainKey, Vec<ChainEntry>>>,
    /// `(class, key1, key2)` → winning `record_hash` (unique marker).
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
        // One lock guards marker + chain, mirroring the production sled
        // transaction's atomicity: check-and-set happens under the same
        // critical section, so concurrent records serialize.
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
        // Mirrors the production store's documented deterministic order:
        // chronological by recorded_at with a record_hash tiebreak.
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

/// Backend that persists session opens fine but rejects the
/// decision-recorded insert — proves fail-closed decision persistence with
/// the session precondition satisfied.
#[derive(Default)]
struct FailingDecisionBackend {
    inner: OpaqueUniqueBackend,
}

impl GovernanceReceiptBackend for FailingDecisionBackend {
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
    fn put_opaque_if_absent(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
        recorded_at: u64,
        record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<Option<[u8; 32]>, String> {
        if class == "decision_recorded" {
            return Err("simulated decision-recorded backend failure".to_string());
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

/// Open a session so the decision precondition is satisfied.
fn open_session(mgr: &GovernanceManager, domain: &GovernanceDomainId, session: &str, by: &Did) {
    match mgr
        .record_process_session_opened(domain, session, by)
        .unwrap()
    {
        ProcessSessionOpenOutcome::Opened(_) | ProcessSessionOpenOutcome::AlreadyOpened(_) => {}
    }
}

/// The composite key the decision chain lives under (for chain_len asserts).
fn decision_key1(domain: &str, session: &str) -> String {
    decision_recorded_composite_key1(domain, session)
}

// ============================================================================
// Record — happy path, retry idempotency, conflicts
// ============================================================================

#[test]
fn record_persists_and_returns_receipt() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    let outcome = mgr
        .record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .expect("first record must succeed");
    let DecisionRecordOutcome::Recorded(receipt) = outcome else {
        panic!("first record must be Recorded, got {outcome:?}");
    };
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.decision_id, "decision-001");
    assert_eq!(receipt.recorded_by, recorder.to_string());
    assert_eq!(receipt.body_hash, [9u8; 32]);
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");
    // Persist-before-return: durable under the injective composite key.
    assert_eq!(
        store.chain_len(
            "decision_recorded",
            &decision_key1("coop:test", "session-001"),
            Some("decision-001"),
        ),
        1,
        "exactly one persisted decision"
    );
    // Point read hydrates the same receipt.
    let read = mgr
        .get_decision_recorded(&domain, "session-001", "decision-001")
        .unwrap()
        .expect("point read must hydrate");
    assert_eq!(read, receipt);
}

#[test]
fn same_identity_retry_returns_original_never_restamped() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    let DecisionRecordOutcome::Recorded(original) = mgr
        .record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .unwrap()
    else {
        panic!("first record must be Recorded");
    };

    // Retry with identical stable identity (recorded_by + body_hash). The
    // manager stamps a fresh recorded_at internally on every attempt —
    // proving that recorded_at/record_hash are NOT identity inputs: the
    // retry still resolves to the ORIGINAL receipt, byte-identical.
    let outcome = mgr
        .record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .expect("same-identity retry must succeed");
    let DecisionRecordOutcome::AlreadyRecorded(returned) = outcome else {
        panic!("retry must be AlreadyRecorded, got {outcome:?}");
    };
    assert_eq!(
        returned.recorded_at, original.recorded_at,
        "never restamped"
    );
    assert_eq!(returned.record_hash, original.record_hash);
    assert_eq!(returned, original);
    assert_eq!(
        store.chain_len(
            "decision_recorded",
            &decision_key1("coop:test", "session-001"),
            Some("decision-001"),
        ),
        1,
        "retry must not append a second record"
    );
}

#[test]
fn different_recorder_conflicts_fail_closed() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let other = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    let DecisionRecordOutcome::Recorded(original) = mgr
        .record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .unwrap()
    else {
        panic!("first record must be Recorded");
    };

    let err = mgr
        .record_decision(&domain, "session-001", "decision-001", &other, [9u8; 32])
        .expect_err("different recorder must conflict");
    assert!(
        err.to_string().starts_with("decision_recorded_conflict"),
        "stable conflict prefix, got: {err}"
    );
    // Original untouched; no second record.
    let read = mgr
        .get_decision_recorded(&domain, "session-001", "decision-001")
        .unwrap()
        .unwrap();
    assert_eq!(read, original);
    assert_eq!(
        store.chain_len(
            "decision_recorded",
            &decision_key1("coop:test", "session-001"),
            Some("decision-001"),
        ),
        1
    );
}

#[test]
fn different_body_hash_conflicts_fail_closed() {
    let (mgr, _store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    let DecisionRecordOutcome::Recorded(original) = mgr
        .record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .unwrap()
    else {
        panic!("first record must be Recorded");
    };

    let err = mgr
        .record_decision(
            &domain,
            "session-001",
            "decision-001",
            &recorder,
            [10u8; 32],
        )
        .expect_err("different body hash must conflict");
    assert!(
        err.to_string().starts_with("decision_recorded_conflict"),
        "stable conflict prefix, got: {err}"
    );
    let read = mgr
        .get_decision_recorded(&domain, "session-001", "decision-001")
        .unwrap()
        .unwrap();
    assert_eq!(read, original, "original untouched after conflict");
}

// ============================================================================
// Session precondition — fail closed, no silent creation
// ============================================================================

#[test]
fn unopened_session_fails_closed_and_creates_nothing() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    // No open_session call.

    let err = mgr
        .record_decision(
            &domain,
            "session-ghost",
            "decision-001",
            &recorder,
            [9u8; 32],
        )
        .expect_err("unopened session must fail closed");
    assert!(
        err.to_string()
            .starts_with("decision_recorded_session_not_opened"),
        "stable precondition prefix, got: {err}"
    );
    // Nothing persisted — no decision, and NO silently-created session.
    assert_eq!(
        store.chain_len(
            "decision_recorded",
            &decision_key1("coop:test", "session-ghost"),
            Some("decision-001"),
        ),
        0
    );
    assert!(
        mgr.list_decisions_recorded_in_domain(&domain, "session-ghost")
            .unwrap()
            .is_empty(),
        "no decision visible via manager list"
    );
    assert_eq!(
        store.chain_len("process_session_opened", "coop:test", Some("session-ghost")),
        0,
        "no session silently created"
    );
}

// ============================================================================
// Input validation, missing store, backend failure
// ============================================================================

#[test]
fn empty_and_whitespace_ids_rejected_before_persistence() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    for (d, s, id) in [
        ("", "session-001", "decision-001"),
        ("   ", "session-001", "decision-001"),
        ("coop:test", "", "decision-001"),
        ("coop:test", "  \t", "decision-001"),
        ("coop:test", "session-001", ""),
        ("coop:test", "session-001", " \n "),
    ] {
        let err = mgr
            .record_decision(&GovernanceDomainId::new(d), s, id, &recorder, [9u8; 32])
            .expect_err("empty/whitespace ids must be rejected");
        assert!(
            err.to_string().contains("non-empty"),
            "id-validation error, got: {err}"
        );
    }
    assert_eq!(
        store.chain_len(
            "decision_recorded",
            &decision_key1("coop:test", "session-001"),
            Some("decision-001"),
        ),
        0,
        "nothing persisted by rejected calls"
    );
}

#[test]
fn missing_receipt_store_is_an_error() {
    let mgr = GovernanceManager::new(); // no receipt store wired
    let recorder = fresh_did();
    let err = mgr
        .record_decision(
            &coop_test(),
            "session-001",
            "decision-001",
            &recorder,
            [9u8; 32],
        )
        .expect_err("missing store must be an error");
    assert!(
        err.to_string().contains("receipt store is required"),
        "store-required error, got: {err}"
    );
}

#[test]
fn backend_failure_fails_closed() {
    let store = Arc::new(FailingDecisionBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    let err = mgr
        .record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .expect_err("backend failure must surface");
    assert!(
        err.to_string()
            .contains("simulated decision-recorded backend failure"),
        "backend error surfaced, got: {err}"
    );
    assert!(
        mgr.get_decision_recorded(&domain, "session-001", "decision-001")
            .unwrap()
            .is_none(),
        "nothing half-written"
    );
}

// ============================================================================
// Concurrency — atomic uniqueness under racing duplicates
// ============================================================================

#[test]
fn concurrent_duplicate_records_serialize_to_one_decision() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    let mgr = Arc::new(mgr);
    let mut handles = Vec::new();
    for _ in 0..8 {
        let mgr = mgr.clone();
        let recorder = recorder.clone();
        let domain = domain.clone();
        handles.push(std::thread::spawn(move || {
            mgr.record_decision(
                &domain,
                "session-001",
                "decision-race",
                &recorder,
                [9u8; 32],
            )
        }));
    }
    let mut receipts = Vec::new();
    for h in handles {
        let outcome = h.join().unwrap().expect("same-identity race must succeed");
        match outcome {
            DecisionRecordOutcome::Recorded(r) | DecisionRecordOutcome::AlreadyRecorded(r) => {
                receipts.push(r)
            }
        }
    }
    // Exactly one persisted record; every thread observed the same winner.
    assert_eq!(
        store.chain_len(
            "decision_recorded",
            &decision_key1("coop:test", "session-001"),
            Some("decision-race"),
        ),
        1,
        "atomic uniqueness: one winner"
    );
    let first = &receipts[0];
    for r in &receipts {
        assert_eq!(r, first, "all threads observe the single winner");
    }
}

// ============================================================================
// Key injectivity, cross-domain isolation, list semantics
// ============================================================================

#[test]
fn composite_key_is_injective_no_aliasing() {
    assert_ne!(
        decision_recorded_composite_key1("ab", "c"),
        decision_recorded_composite_key1("a", "bc"),
        "netstring length prefix must prevent domain/session aliasing"
    );

    let (mgr, _store) = make_manager();
    let recorder = fresh_did();
    let d_ab = GovernanceDomainId::new("ab");
    let d_a = GovernanceDomainId::new("a");
    open_session(&mgr, &d_ab, "c", &recorder);
    // Only ("ab","c") is opened; ("a","bc") must NOT see it through key
    // aliasing — the precondition fails closed for the unopened pair.
    let err = mgr
        .record_decision(&d_a, "bc", "decision-001", &recorder, [9u8; 32])
        .expect_err("aliased pair must not inherit the opened session");
    assert!(err
        .to_string()
        .starts_with("decision_recorded_session_not_opened"));
    // The genuinely opened pair records fine.
    mgr.record_decision(&d_ab, "c", "decision-001", &recorder, [9u8; 32])
        .expect("opened pair records");
}

#[test]
fn two_domains_sharing_session_id_never_mix() {
    let (mgr, _store) = make_manager();
    let recorder = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    open_session(&mgr, &d1, "shared-session", &recorder);
    open_session(&mgr, &d2, "shared-session", &recorder);

    mgr.record_decision(&d1, "shared-session", "decision-1", &recorder, [1u8; 32])
        .unwrap();
    mgr.record_decision(&d2, "shared-session", "decision-2", &recorder, [2u8; 32])
        .unwrap();

    let list1 = mgr
        .list_decisions_recorded_in_domain(&d1, "shared-session")
        .unwrap();
    let list2 = mgr
        .list_decisions_recorded_in_domain(&d2, "shared-session")
        .unwrap();
    assert_eq!(list1.len(), 1);
    assert_eq!(list2.len(), 1);
    assert_eq!(list1[0].decision_id, "decision-1");
    assert_eq!(list2[0].decision_id, "decision-2");
    assert_eq!(list1[0].domain_id, "coop:one");
    assert_eq!(list2[0].domain_id, "coop:two");
}

#[test]
fn multiple_decisions_per_session_list_deterministically() {
    let (mgr, _store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    // Multiple decisions per session are permitted at the substrate layer
    // (decision_id is the unit of uniqueness, not the session).
    for id in ["decision-a", "decision-b", "decision-c"] {
        mgr.record_decision(&domain, "session-001", id, &recorder, [3u8; 32])
            .expect("each distinct decision_id records");
    }
    let list = mgr
        .list_decisions_recorded_in_domain(&domain, "session-001")
        .unwrap();
    assert_eq!(list.len(), 3);
    // Deterministic chronological order — (recorded_at, record_hash) —
    // stable across re-reads.
    let again = mgr
        .list_decisions_recorded_in_domain(&domain, "session-001")
        .unwrap();
    assert_eq!(list, again, "list order stable across re-reads");
    let mut sorted = list.clone();
    sorted.sort_by_key(|r| (r.recorded_at, r.record_hash));
    assert_eq!(
        list, sorted,
        "chronological (recorded_at, record_hash) order"
    );
}

// ============================================================================
// Payload audit + sibling classes unchanged
// ============================================================================

#[test]
fn persisted_payload_carries_only_v1_fields_no_lineage_fields() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);
    mgr.record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .unwrap();

    let payload = store
        .raw_payload(
            "decision_recorded",
            &decision_key1("coop:test", "session-001"),
            Some("decision-001"),
        )
        .expect("payload persisted");
    let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    let obj = value.as_object().unwrap();
    let expected: std::collections::BTreeSet<&str> = [
        "domain_id",
        "session_id",
        "decision_id",
        "recorded_by",
        "recorded_at",
        "body_hash",
        "record_hash",
    ]
    .into_iter()
    .collect();
    let actual: std::collections::BTreeSet<&str> = obj.keys().map(|k| k.as_str()).collect();
    assert_eq!(
        actual, expected,
        "persisted payload must carry exactly the v1 field set — no body, \
         no outcome/tally/vote/proposal/mandate field (#2281 Axis A/B)"
    );
}

#[test]
fn session_open_and_deliberation_entry_behavior_unchanged_alongside_decisions() {
    let (mgr, _store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &recorder);

    // Deliberation entries still record with their landed semantics.
    let entry = mgr
        .record_deliberation_entry(
            &domain,
            "session-001",
            "entry-001",
            &recorder,
            DeliberationEntryKind::Question,
            [7u8; 32],
        )
        .expect("deliberation entry still records");
    assert!(matches!(entry, DeliberationEntryRecordOutcome::Recorded(_)));

    // A decision records against the same session without disturbing them.
    mgr.record_decision(&domain, "session-001", "decision-001", &recorder, [9u8; 32])
        .expect("decision records alongside");

    // Session-open retry is still idempotent (original receipt).
    let reopened = mgr
        .record_process_session_opened(&domain, "session-001", &recorder)
        .unwrap();
    assert!(matches!(
        reopened,
        ProcessSessionOpenOutcome::AlreadyOpened(_)
    ));

    // The two receipt families stay in separate stores/lists.
    let entries = mgr
        .list_deliberation_entries_in_domain(&domain, "session-001")
        .unwrap();
    let decisions = mgr
        .list_decisions_recorded_in_domain(&domain, "session-001")
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(decisions.len(), 1);
    assert_eq!(entries[0].entry_id, "entry-001");
    assert_eq!(decisions[0].decision_id, "decision-001");
}
