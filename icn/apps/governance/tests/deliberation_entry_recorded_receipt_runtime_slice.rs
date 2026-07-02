//! Runtime proof for the third `ProcessTransitionReceipt` class —
//! [`DeliberationEntryRecordedReceipt`] — per the merged #2277 contract
//! (`docs/design/deliberation-entry-recorded-receipt.md`) and the #2278 Q3
//! taxonomy decision (`docs/design/deliberation-entry-kind-taxonomy.md`).
//!
//! Pins the merged contract:
//!
//! 1. `GovernanceManager::record_deliberation_entry` requires an
//!    already-opened session (stable prefix
//!    `deliberation_entry_session_not_opened`; nothing written; no silent
//!    session creation), constructs a receipt with a real blake3
//!    `record_hash`, persists it through the backend's atomic
//!    insert-if-absent BEFORE returning, and returns `Recorded(receipt)`.
//! 2. Retry with identical stable identity (`author` + `body_hash` +
//!    `entry_kind`) returns the ORIGINAL receipt unchanged
//!    (`AlreadyRecorded`) — never restamped, no second record.
//! 3. Same `entry_id` with a different `author`, `body_hash`, or
//!    `entry_kind` fails closed with the stable
//!    `deliberation_entry_conflict` prefix; the original is untouched.
//! 4. Empty ids rejected; a missing receipt store is an error; backend
//!    failure fails closed.
//! 5. Concurrent duplicate records serialize to exactly one persisted
//!    entry (atomic uniqueness at the storage layer).
//! 6. The composite storage key is injective: `("ab","c")` vs `("a","bc")`
//!    domain/session pairs never alias, and two domains sharing a
//!    `session_id` never mix entries.
//! 7. List order is the store's deterministic `(recorded_at, record_hash)`
//!    chronological order — NOT arrival order.
//! 8. Existing session-open and gate-result behavior is unchanged.
//! 9. The receipt carries only `body_hash` — no body content field — and
//!    no prohibited vocabulary.
//!
//! This is **not** a discussion system: no stored `DeliberationThread`, no
//! lifecycle, no decision/mutation-plan/evidence-packet objects, no
//! moderation. A receipt records an institutional fact and grants zero
//! authority.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{
    DeliberationEntryKind, DeliberationEntryRecordedReceipt, GovernanceDecisionReceipt,
    GovernanceDomainId, ProcessGateKind, ProcessGateResult,
};
use icn_governance_actor::manager::{
    DeliberationEntryRecordOutcome, GovernanceManager, ProcessSessionOpenOutcome,
};
use icn_governance_actor::receipt_backend::{
    deliberation_entry_composite_key1, GovernanceReceiptBackend,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-backed test store — the test analog of the production
// gateway-backed ReceiptStore: it implements the opaque primitives
// (including the atomic `put_opaque_if_absent`) and NO typed overrides,
// so the trait's typed deliberation-entry defaults are exercised
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
/// deliberation-entry insert — proves fail-closed entry persistence with
/// the session precondition satisfied.
#[derive(Default)]
struct FailingEntryBackend {
    inner: OpaqueUniqueBackend,
}

impl GovernanceReceiptBackend for FailingEntryBackend {
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
        if class == "deliberation_entry_recorded" {
            return Err("simulated deliberation-entry backend failure".to_string());
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

/// Open a session so the entry precondition is satisfied.
fn open_session(mgr: &GovernanceManager, domain: &GovernanceDomainId, session: &str, by: &Did) {
    match mgr
        .record_process_session_opened(domain, session, by)
        .unwrap()
    {
        ProcessSessionOpenOutcome::Opened(_) | ProcessSessionOpenOutcome::AlreadyOpened(_) => {}
    }
}

/// The composite key the entry chain lives under (for chain_len asserts).
fn entry_key1(domain: &str, session: &str) -> String {
    deliberation_entry_composite_key1(domain, session)
}

// ============================================================================
// Record — happy path, retry idempotency, conflicts
// ============================================================================

#[test]
fn record_persists_and_returns_receipt() {
    let (mgr, store) = make_manager();
    let author = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &author);

    let outcome = mgr
        .record_deliberation_entry(
            &domain,
            "session-001",
            "entry-001",
            &author,
            DeliberationEntryKind::Question,
            [7u8; 32],
        )
        .expect("first record must succeed");
    let DeliberationEntryRecordOutcome::Recorded(receipt) = outcome else {
        panic!("first record must be Recorded, got {outcome:?}");
    };
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.entry_id, "entry-001");
    assert_eq!(receipt.author, author.to_string());
    assert_eq!(receipt.entry_kind, DeliberationEntryKind::Question);
    assert_eq!(receipt.body_hash, [7u8; 32]);
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");
    // Persist-before-return: durable under the injective composite key.
    assert_eq!(
        store.chain_len(
            "deliberation_entry_recorded",
            &entry_key1("coop:test", "session-001"),
            Some("entry-001")
        ),
        1
    );
    // Readable back through the manager (point read + session list).
    let read = mgr
        .get_deliberation_entry(&domain, "session-001", "entry-001")
        .unwrap()
        .expect("entry readable after persist");
    assert_eq!(read, receipt);
    let listed = mgr
        .list_deliberation_entries_in_domain(&domain, "session-001")
        .unwrap();
    assert_eq!(listed, vec![receipt]);
}

#[test]
fn same_identity_retry_returns_original_unchanged() {
    let (mgr, store) = make_manager();
    let author = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-retry", &author);

    let first = match mgr
        .record_deliberation_entry(
            &domain,
            "session-retry",
            "entry-r",
            &author,
            DeliberationEntryKind::Concern,
            [9u8; 32],
        )
        .unwrap()
    {
        DeliberationEntryRecordOutcome::Recorded(r) => r,
        other => panic!("expected Recorded, got {other:?}"),
    };

    // Retry with identical stable identity (author + body_hash +
    // entry_kind) — the contract requires the ORIGINAL receipt back,
    // never a restamp.
    let second = match mgr
        .record_deliberation_entry(
            &domain,
            "session-retry",
            "entry-r",
            &author,
            DeliberationEntryKind::Concern,
            [9u8; 32],
        )
        .unwrap()
    {
        DeliberationEntryRecordOutcome::AlreadyRecorded(r) => r,
        other => panic!("expected AlreadyRecorded, got {other:?}"),
    };
    assert_eq!(second.record_hash, first.record_hash, "never restamped");
    assert_eq!(
        second.recorded_at, first.recorded_at,
        "original recorded_at"
    );
    assert_eq!(
        store.chain_len(
            "deliberation_entry_recorded",
            &entry_key1("coop:test", "session-retry"),
            Some("entry-r")
        ),
        1,
        "no second record persisted"
    );
}

#[test]
fn different_author_conflicts() {
    let (mgr, _store) = make_manager();
    let author_a = fresh_did();
    let author_b = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-ca", &author_a);

    let original = match mgr
        .record_deliberation_entry(
            &domain,
            "session-ca",
            "entry-c",
            &author_a,
            DeliberationEntryKind::Objection,
            [1u8; 32],
        )
        .unwrap()
    {
        DeliberationEntryRecordOutcome::Recorded(r) => r,
        other => panic!("expected Recorded, got {other:?}"),
    };

    let err = mgr
        .record_deliberation_entry(
            &domain,
            "session-ca",
            "entry-c",
            &author_b,
            DeliberationEntryKind::Objection,
            [1u8; 32],
        )
        .expect_err("different author must fail closed");
    assert!(
        err.to_string().contains("deliberation_entry_conflict"),
        "stable conflict prefix, got: {err}"
    );
    let read = mgr
        .get_deliberation_entry(&domain, "session-ca", "entry-c")
        .unwrap()
        .unwrap();
    assert_eq!(read, original, "original untouched");
}

#[test]
fn different_body_hash_conflicts() {
    let (mgr, _store) = make_manager();
    let author = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-cb", &author);

    mgr.record_deliberation_entry(
        &domain,
        "session-cb",
        "entry-c",
        &author,
        DeliberationEntryKind::Amendment,
        [1u8; 32],
    )
    .unwrap();

    let err = mgr
        .record_deliberation_entry(
            &domain,
            "session-cb",
            "entry-c",
            &author,
            DeliberationEntryKind::Amendment,
            [2u8; 32],
        )
        .expect_err("different body_hash must fail closed");
    assert!(err.to_string().contains("deliberation_entry_conflict"));
}

#[test]
fn different_entry_kind_conflicts() {
    // The #2278 rule: entry_kind participates in duplicate identity — a
    // same-entry_id retry with a different kind is a conflict, never a
    // silent original-receipt return (its canonical hash differs).
    let (mgr, _store) = make_manager();
    let author = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-ck", &author);

    mgr.record_deliberation_entry(
        &domain,
        "session-ck",
        "entry-c",
        &author,
        DeliberationEntryKind::Question,
        [1u8; 32],
    )
    .unwrap();

    let err = mgr
        .record_deliberation_entry(
            &domain,
            "session-ck",
            "entry-c",
            &author,
            DeliberationEntryKind::Blocker,
            [1u8; 32],
        )
        .expect_err("different entry_kind must fail closed");
    assert!(err.to_string().contains("deliberation_entry_conflict"));
}

// ============================================================================
// Session precondition — fail closed, no silent creation
// ============================================================================

#[test]
fn missing_opened_session_fails_closed_and_writes_nothing() {
    let (mgr, store) = make_manager();
    let author = fresh_did();
    let domain = coop_test();

    let err = mgr
        .record_deliberation_entry(
            &domain,
            "session-unopened",
            "entry-x",
            &author,
            DeliberationEntryKind::RecordOnly,
            [3u8; 32],
        )
        .expect_err("entry against an unopened session must fail closed");
    assert!(
        err.to_string()
            .contains("deliberation_entry_session_not_opened"),
        "stable precondition prefix, got: {err}"
    );
    // Nothing persisted — no entry, and NO silently created session.
    assert_eq!(
        store.chain_len(
            "deliberation_entry_recorded",
            &entry_key1("coop:test", "session-unopened"),
            Some("entry-x")
        ),
        0
    );
    assert!(mgr
        .get_process_session_opened(&domain, "session-unopened")
        .unwrap()
        .is_none());
}

// ============================================================================
// Validation + fail-closed persistence
// ============================================================================

#[test]
fn empty_ids_rejected() {
    let (mgr, _store) = make_manager();
    let author = fresh_did();
    let kind = DeliberationEntryKind::Question;

    let err = mgr
        .record_deliberation_entry(&coop_test(), "", "entry-x", &author, kind, [0u8; 32])
        .expect_err("empty session_id must be rejected");
    assert!(err.to_string().contains("session_id must be non-empty"));

    // Whitespace-only ids are rejected at the manager layer too — callers
    // outside the HTTP handler must not mint receipts with visually-empty
    // identifiers or whitespace storage keys.
    let err = mgr
        .record_deliberation_entry(&coop_test(), "   ", "entry-x", &author, kind, [0u8; 32])
        .expect_err("whitespace session_id must be rejected");
    assert!(err.to_string().contains("session_id must be non-empty"));
    let err = mgr
        .record_deliberation_entry(&coop_test(), "session-x", "   ", &author, kind, [0u8; 32])
        .expect_err("whitespace entry_id must be rejected");
    assert!(err.to_string().contains("entry_id must be non-empty"));
    let err = mgr
        .record_deliberation_entry(
            &GovernanceDomainId::new("   "),
            "session-x",
            "entry-x",
            &author,
            kind,
            [0u8; 32],
        )
        .expect_err("whitespace domain_id must be rejected");
    assert!(err.to_string().contains("domain_id must be non-empty"));

    let err = mgr
        .record_deliberation_entry(&coop_test(), "session-x", "", &author, kind, [0u8; 32])
        .expect_err("empty entry_id must be rejected");
    assert!(err.to_string().contains("entry_id must be non-empty"));

    let err = mgr
        .record_deliberation_entry(
            &GovernanceDomainId::new(""),
            "session-x",
            "entry-x",
            &author,
            kind,
            [0u8; 32],
        )
        .expect_err("empty domain_id must be rejected");
    assert!(err.to_string().contains("domain_id must be non-empty"));
}

#[test]
fn missing_receipt_store_is_an_error() {
    let mgr = GovernanceManager::new();
    let err = mgr
        .record_deliberation_entry(
            &coop_test(),
            "session-y",
            "entry-y",
            &fresh_did(),
            DeliberationEntryKind::Question,
            [0u8; 32],
        )
        .expect_err("no receipt store must be an error");
    assert!(err.to_string().contains("receipt store is required"));
}

#[test]
fn backend_failure_fails_closed() {
    let mgr = GovernanceManager::new().with_receipt_store(
        Arc::new(FailingEntryBackend::default()) as Arc<dyn GovernanceReceiptBackend>
    );
    let author = fresh_did();
    let domain = coop_test();
    // Session opens persist fine on this backend; only the entry insert
    // fails — proving the entry path itself fails closed.
    open_session(&mgr, &domain, "session-z", &author);
    let err = mgr
        .record_deliberation_entry(
            &domain,
            "session-z",
            "entry-z",
            &author,
            DeliberationEntryKind::Question,
            [0u8; 32],
        )
        .expect_err("backend failure must surface as an error");
    assert!(err
        .to_string()
        .contains("simulated deliberation-entry backend failure"));
}

// ============================================================================
// Concurrency — atomic uniqueness end-to-end through the manager
// ============================================================================

#[test]
fn concurrent_same_identity_records_yield_exactly_one_persisted_entry() {
    let store = Arc::new(OpaqueUniqueBackend::default());
    let mgr = Arc::new(
        GovernanceManager::new()
            .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>),
    );
    let author = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-race", &author);

    let mut handles = Vec::new();
    for _ in 0..8 {
        let mgr = Arc::clone(&mgr);
        let author = author.clone();
        let domain = domain.clone();
        handles.push(std::thread::spawn(move || {
            mgr.record_deliberation_entry(
                &domain,
                "session-race",
                "entry-race",
                &author,
                DeliberationEntryKind::Question,
                [5u8; 32],
            )
        }));
    }
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Identical stable identity everywhere → every call succeeds
    // (Recorded or AlreadyRecorded); exactly one record persisted.
    let mut recorded = 0;
    for o in outcomes {
        match o.expect("same-identity race must never error") {
            DeliberationEntryRecordOutcome::Recorded(_) => recorded += 1,
            DeliberationEntryRecordOutcome::AlreadyRecorded(_) => {}
        }
    }
    assert_eq!(recorded, 1, "exactly one thread may win the record");
    assert_eq!(
        store.chain_len(
            "deliberation_entry_recorded",
            &entry_key1("coop:test", "session-race"),
            Some("entry-race")
        ),
        1,
        "exactly one persisted entry"
    );
}

// ============================================================================
// Key injectivity + domain scoping
// ============================================================================

#[test]
fn composite_key_is_injective_no_aliasing() {
    // ("ab","c") and ("a","bc") must produce distinct storage keys — bare
    // concatenation would alias them into the same entry chain.
    assert_ne!(
        deliberation_entry_composite_key1("ab", "c"),
        deliberation_entry_composite_key1("a", "bc")
    );
    assert_ne!(
        deliberation_entry_composite_key1("a:b", "c"),
        deliberation_entry_composite_key1("a", "b:c")
    );

    // End-to-end: both pairs record the SAME entry_id independently —
    // two Recorded outcomes, two separate persisted entries.
    let (mgr, store) = make_manager();
    let author = fresh_did();
    let domain_ab = GovernanceDomainId::new("ab");
    let domain_a = GovernanceDomainId::new("a");
    open_session(&mgr, &domain_ab, "c", &author);
    open_session(&mgr, &domain_a, "bc", &author);

    let first = mgr
        .record_deliberation_entry(
            &domain_ab,
            "c",
            "entry-alias",
            &author,
            DeliberationEntryKind::Question,
            [1u8; 32],
        )
        .unwrap();
    let second = mgr
        .record_deliberation_entry(
            &domain_a,
            "bc",
            "entry-alias",
            &author,
            DeliberationEntryKind::Concern,
            [2u8; 32],
        )
        .unwrap();
    assert!(matches!(first, DeliberationEntryRecordOutcome::Recorded(_)));
    assert!(
        matches!(second, DeliberationEntryRecordOutcome::Recorded(_)),
        "aliased keys would have made this a duplicate; got {second:?}"
    );
    assert_eq!(
        store.chain_len(
            "deliberation_entry_recorded",
            &entry_key1("ab", "c"),
            Some("entry-alias")
        ),
        1
    );
    assert_eq!(
        store.chain_len(
            "deliberation_entry_recorded",
            &entry_key1("a", "bc"),
            Some("entry-alias")
        ),
        1
    );
}

#[test]
fn cross_domain_same_session_id_does_not_mix() {
    let (mgr, _store) = make_manager();
    let author = fresh_did();
    let domain_a = GovernanceDomainId::new("coop:alpha");
    let domain_b = GovernanceDomainId::new("coop:beta");
    // Two domains reuse the SAME session_id.
    open_session(&mgr, &domain_a, "shared-session", &author);
    open_session(&mgr, &domain_b, "shared-session", &author);

    mgr.record_deliberation_entry(
        &domain_a,
        "shared-session",
        "entry-1",
        &author,
        DeliberationEntryKind::Question,
        [1u8; 32],
    )
    .unwrap();
    mgr.record_deliberation_entry(
        &domain_b,
        "shared-session",
        "entry-2",
        &author,
        DeliberationEntryKind::Amendment,
        [2u8; 32],
    )
    .unwrap();

    let a = mgr
        .list_deliberation_entries_in_domain(&domain_a, "shared-session")
        .unwrap();
    assert_eq!(a.len(), 1, "domain A sees only its own entries");
    assert_eq!(a[0].domain_id, "coop:alpha");
    assert_eq!(a[0].entry_id, "entry-1");

    let b = mgr
        .list_deliberation_entries_in_domain(&domain_b, "shared-session")
        .unwrap();
    assert_eq!(b.len(), 1, "domain B sees only its own entries");
    assert_eq!(b[0].domain_id, "coop:beta");
    assert_eq!(b[0].entry_id, "entry-2");
}

#[test]
fn list_order_is_deterministic_recorded_at_then_hash() {
    let (mgr, _store) = make_manager();
    let author = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-order", &author);

    for (entry_id, byte) in [("entry-a", 1u8), ("entry-b", 2u8), ("entry-c", 3u8)] {
        mgr.record_deliberation_entry(
            &domain,
            "session-order",
            entry_id,
            &author,
            DeliberationEntryKind::RecordOnly,
            [byte; 32],
        )
        .unwrap();
    }

    let listed = mgr
        .list_deliberation_entries_in_domain(&domain, "session-order")
        .unwrap();
    assert_eq!(listed.len(), 3);
    // The documented order is (recorded_at, record_hash) — chronological
    // with a hash tiebreak, NOT arrival order — and stable across reads.
    let mut expected = listed.clone();
    expected.sort_by_key(|r| (r.recorded_at, r.record_hash));
    assert_eq!(
        listed, expected,
        "list order must be (recorded_at, record_hash)"
    );
    let again = mgr
        .list_deliberation_entries_in_domain(&domain, "session-order")
        .unwrap();
    assert_eq!(listed, again, "order stable across re-reads");
}

// ============================================================================
// Existing session-open + gate-result behavior unchanged
// ============================================================================

#[test]
fn session_open_behavior_unchanged_alongside_entries() {
    let (mgr, store) = make_manager();
    let opener = fresh_did();
    let other = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-so", &opener);
    mgr.record_deliberation_entry(
        &domain,
        "session-so",
        "entry-1",
        &opener,
        DeliberationEntryKind::Question,
        [1u8; 32],
    )
    .unwrap();

    // Same-opener retry still returns the original; different opener
    // still conflicts; still exactly one persisted opening.
    match mgr
        .record_process_session_opened(&domain, "session-so", &opener)
        .unwrap()
    {
        ProcessSessionOpenOutcome::AlreadyOpened(_) => {}
        other => panic!("expected AlreadyOpened, got {other:?}"),
    }
    let err = mgr
        .record_process_session_opened(&domain, "session-so", &other)
        .expect_err("different opener still fails closed");
    assert!(err.to_string().contains("process_session_open_conflict"));
    assert_eq!(
        store.chain_len("process_session_opened", "coop:test", Some("session-so")),
        1
    );
}

#[test]
fn gate_results_still_neither_require_nor_create_sessions() {
    let (mgr, store) = make_manager();
    let recorder = fresh_did();
    let domain = coop_test();

    // No opened session — gate-result recording must succeed unchanged.
    let receipt = mgr
        .record_process_gate_result(
            &domain,
            "session-unopened",
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("gate results must not require an opened session");
    assert_eq!(receipt.session_id, "session-unopened");
    // ...and must NOT have silently created a session-open record or a
    // deliberation entry. The entry check goes through the manager's list
    // API (backed by list_opaque_for over the composite key1), which
    // covers EVERY entry_id under the session anchor — a point
    // chain_len(..., key2) probe could miss an entry stored under a
    // different entry_id.
    assert!(mgr
        .get_process_session_opened(&domain, "session-unopened")
        .unwrap()
        .is_none());
    assert!(mgr
        .list_deliberation_entries_in_domain(&domain, "session-unopened")
        .unwrap()
        .is_empty());
    let _ = &store; // chain-level probes are exercised by the other tests
}

// ============================================================================
// Privacy + vocabulary discipline
// ============================================================================

#[test]
fn receipt_carries_body_hash_only_and_clean_vocabulary() {
    let r = DeliberationEntryRecordedReceipt::new(
        "coop:test".to_string(),
        "session-vocab".to_string(),
        "entry-vocab".to_string(),
        "did:icn:someone".to_string(),
        DeliberationEntryKind::AccessibilityReview,
        1_750_000_000,
        [4u8; 32],
    );
    let json = serde_json::to_string(&r).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let obj = value.as_object().unwrap();
    assert!(obj.contains_key("body_hash"));
    for forbidden_field in ["body", "content", "text", "message"] {
        assert!(
            !obj.contains_key(forbidden_field),
            "receipt must not carry a `{forbidden_field}` field"
        );
    }
    let lower = json.to_lowercase();
    for forbidden in [
        "wallet", "balance", "currency", "payment", "withdraw", "deposit", "chat", "comment",
        "moderat", "approve", "vote", "outcome",
    ] {
        assert!(
            !lower.contains(forbidden),
            "serialized deliberation-entry receipt must not contain `{forbidden}`"
        );
    }
}
