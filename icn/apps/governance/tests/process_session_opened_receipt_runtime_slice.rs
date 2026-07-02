//! Runtime proof for the second `ProcessTransitionReceipt` class —
//! [`ProcessSessionOpenedReceipt`] — the #2275 session anchor
//! (`docs/design/process-session-receipt-anchor.md`).
//!
//! Pins the merged contract:
//!
//! 1. `GovernanceManager::record_process_session_opened` constructs a
//!    receipt with a real blake3 `record_hash`, persists it through the
//!    backend's atomic insert-if-absent BEFORE returning, and returns
//!    `Opened(receipt)`.
//! 2. Same-opener retry (later timestamp) returns the ORIGINAL receipt
//!    unchanged (`AlreadyOpened`) — original `opened_at` / `record_hash`
//!    preserved, never restamped, and no second record persisted.
//! 3. Different-opener duplicate fails closed with the stable
//!    `process_session_open_conflict` prefix; the original is untouched.
//! 4. Empty `session_id` / `domain_id` rejected; a missing receipt store
//!    is an error (uniqueness cannot be faked in memory).
//! 5. Backend/storage failure fails closed — no receipt returned.
//! 6. Concurrent duplicate opens serialize to exactly one persisted
//!    opening (atomic uniqueness at the storage layer).
//! 7. Existing `record_process_gate_result` behavior is unchanged: gate
//!    results neither require nor silently create an opened session.
//! 8. Domain-scoped gate-result reads: two domains reusing the same
//!    `session_id` never mix evidence; legacy per-session reads are
//!    untouched.
//! 9. Regulatory-safe vocabulary preserved in the serialized receipt.
//!
//! This is **not** a full process runtime: no stored `ProcessSession`,
//! no lifecycle, no deliberation/decision/mutation-plan/evidence-packet
//! objects, no charter/CCL gating. A receipt records an institutional
//! fact and grants zero authority.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, ProcessGateKind, ProcessGateResult,
    ProcessSessionOpenedReceipt,
};
use icn_governance_actor::manager::{GovernanceManager, ProcessSessionOpenOutcome};
use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-backed test store — the test analog of the production
// gateway-backed ReceiptStore: it implements the opaque primitives
// (including the atomic `put_opaque_if_absent`) and NO typed overrides,
// so the trait's typed session-open defaults are exercised end-to-end
// (typed default → opaque insert-if-absent → map).
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
        // critical section, so concurrent opens serialize.
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

/// Backend whose insert-if-absent always rejects — proves fail-closed
/// persistence (no receipt returned on storage failure).
#[derive(Default)]
struct FailingOpenBackend;

impl GovernanceReceiptBackend for FailingOpenBackend {
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
        _class: &str,
        _key1: &str,
        _key2: Option<&str>,
        _recorded_at: u64,
        _record_hash: [u8; 32],
        _payload: &[u8],
    ) -> Result<Option<[u8; 32]>, String> {
        Err("simulated session-open backend failure".to_string())
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

// ============================================================================
// Session open — happy path, retry idempotency, conflict
// ============================================================================

#[test]
fn open_persists_and_returns_receipt() {
    let (mgr, store) = make_manager();
    let opener = fresh_did();
    let domain = coop_test();

    let outcome = mgr
        .record_process_session_opened(&domain, "session-001", &opener)
        .expect("first open must succeed");
    let ProcessSessionOpenOutcome::Opened(receipt) = outcome else {
        panic!("first open must be Opened, got {outcome:?}");
    };
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.opened_by, opener.to_string());
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");
    // Persist-before-return: the opening is durable in the backend.
    assert_eq!(
        store.chain_len("process_session_opened", "coop:test", Some("session-001")),
        1
    );
    // Readable back through the manager.
    let read = mgr
        .get_process_session_opened(&domain, "session-001")
        .unwrap()
        .expect("opening readable after persist");
    assert_eq!(read, receipt);
}

#[test]
fn same_opener_retry_returns_original_unchanged() {
    let (mgr, store) = make_manager();
    let opener = fresh_did();
    let domain = coop_test();

    let first = match mgr
        .record_process_session_opened(&domain, "session-retry", &opener)
        .unwrap()
    {
        ProcessSessionOpenOutcome::Opened(r) => r,
        other => panic!("expected Opened, got {other:?}"),
    };

    // Retry — even in a later second the manager restamps opened_at, but
    // the contract requires the ORIGINAL receipt back, never a restamp.
    let second = match mgr
        .record_process_session_opened(&domain, "session-retry", &opener)
        .unwrap()
    {
        ProcessSessionOpenOutcome::AlreadyOpened(r) => r,
        other => panic!("expected AlreadyOpened, got {other:?}"),
    };
    assert_eq!(second.record_hash, first.record_hash, "never restamped");
    assert_eq!(second.opened_at, first.opened_at, "original opened_at");
    assert_eq!(
        store.chain_len("process_session_opened", "coop:test", Some("session-retry")),
        1,
        "no second record persisted"
    );
}

#[test]
fn different_opener_duplicate_fails_closed() {
    let (mgr, store) = make_manager();
    let opener_a = fresh_did();
    let opener_b = fresh_did();
    let domain = coop_test();

    let original = match mgr
        .record_process_session_opened(&domain, "session-conflict", &opener_a)
        .unwrap()
    {
        ProcessSessionOpenOutcome::Opened(r) => r,
        other => panic!("expected Opened, got {other:?}"),
    };

    let err = mgr
        .record_process_session_opened(&domain, "session-conflict", &opener_b)
        .expect_err("different opener must fail closed");
    assert!(
        err.to_string().contains("process_session_open_conflict"),
        "stable conflict prefix, got: {err}"
    );
    // Original untouched; still exactly one record.
    assert_eq!(
        store.chain_len(
            "process_session_opened",
            "coop:test",
            Some("session-conflict")
        ),
        1
    );
    let read = mgr
        .get_process_session_opened(&domain, "session-conflict")
        .unwrap()
        .unwrap();
    assert_eq!(read, original);
}

// ============================================================================
// Validation + fail-closed persistence
// ============================================================================

#[test]
fn empty_session_id_rejected() {
    let (mgr, _store) = make_manager();
    let err = mgr
        .record_process_session_opened(&coop_test(), "", &fresh_did())
        .expect_err("empty session_id must be rejected");
    assert!(err.to_string().contains("session_id must be non-empty"));
}

#[test]
fn empty_domain_id_rejected() {
    let (mgr, _store) = make_manager();
    let err = mgr
        .record_process_session_opened(&GovernanceDomainId::new(""), "session-x", &fresh_did())
        .expect_err("empty domain_id must be rejected");
    assert!(err.to_string().contains("domain_id must be non-empty"));
}

#[test]
fn missing_receipt_store_is_an_error() {
    // Uniqueness is a durable property; without a store the manager must
    // refuse rather than return an unpersisted "anchor".
    let mgr = GovernanceManager::new();
    let err = mgr
        .record_process_session_opened(&coop_test(), "session-y", &fresh_did())
        .expect_err("no receipt store must be an error");
    assert!(err.to_string().contains("receipt store is required"));
}

#[test]
fn backend_failure_fails_closed() {
    let mgr = GovernanceManager::new()
        .with_receipt_store(Arc::new(FailingOpenBackend) as Arc<dyn GovernanceReceiptBackend>);
    let err = mgr
        .record_process_session_opened(&coop_test(), "session-z", &fresh_did())
        .expect_err("backend failure must surface as an error");
    assert!(err
        .to_string()
        .contains("simulated session-open backend failure"));
}

// ============================================================================
// Concurrency — atomic uniqueness end-to-end through the manager
// ============================================================================

#[test]
fn concurrent_same_opener_opens_yield_exactly_one_persisted_opening() {
    let store = Arc::new(OpaqueUniqueBackend::default());
    let mgr = Arc::new(
        GovernanceManager::new()
            .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>),
    );
    let opener = fresh_did();
    let domain = coop_test();

    let mut handles = Vec::new();
    for _ in 0..8 {
        let mgr = Arc::clone(&mgr);
        let opener = opener.clone();
        let domain = domain.clone();
        handles.push(std::thread::spawn(move || {
            mgr.record_process_session_opened(&domain, "session-race", &opener)
        }));
    }
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Same opener everywhere → every call succeeds (Opened or
    // AlreadyOpened); exactly one record persisted.
    let mut opened = 0;
    for o in outcomes {
        match o.expect("same-opener race must never error") {
            ProcessSessionOpenOutcome::Opened(_) => opened += 1,
            ProcessSessionOpenOutcome::AlreadyOpened(_) => {}
        }
    }
    assert_eq!(opened, 1, "exactly one thread may win the open");
    assert_eq!(
        store.chain_len("process_session_opened", "coop:test", Some("session-race")),
        1,
        "exactly one persisted opening"
    );
}

// ============================================================================
// Gate-result behavior unchanged + domain-scoped reads
// ============================================================================

#[test]
fn gate_results_do_not_require_or_create_sessions() {
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

    // ...and must NOT have silently created a session-open record.
    assert_eq!(
        store.chain_len(
            "process_session_opened",
            "coop:test",
            Some("session-unopened")
        ),
        0,
        "no silent session creation"
    );
    assert!(mgr
        .get_process_session_opened(&domain, "session-unopened")
        .unwrap()
        .is_none());
}

#[test]
fn domain_scoped_gate_reads_do_not_mix_domains() {
    let (mgr, _store) = make_manager();
    let recorder = fresh_did();
    let domain_a = GovernanceDomainId::new("coop:alpha");
    let domain_b = GovernanceDomainId::new("coop:beta");

    // Two domains reuse the SAME session_id.
    mgr.record_process_gate_result(
        &domain_a,
        "shared-session",
        ProcessGateKind::PrivacyReview,
        ProcessGateResult::Pass,
        &recorder,
    )
    .unwrap();
    mgr.record_process_gate_result(
        &domain_b,
        "shared-session",
        ProcessGateKind::AccessibilityReview,
        ProcessGateResult::Fail,
        &recorder,
    )
    .unwrap();

    let a = mgr
        .list_process_gate_results_in_domain(&domain_a, "shared-session")
        .unwrap();
    assert_eq!(a.len(), 1, "domain A sees only its own evidence");
    assert_eq!(a[0].domain_id, "coop:alpha");
    assert_eq!(a[0].gate_kind, ProcessGateKind::PrivacyReview);

    let b = mgr
        .list_process_gate_results_in_domain(&domain_b, "shared-session")
        .unwrap();
    assert_eq!(b.len(), 1, "domain B sees only its own evidence");
    assert_eq!(b[0].domain_id, "coop:beta");
    assert_eq!(b[0].gate_kind, ProcessGateKind::AccessibilityReview);
}

// ============================================================================
// Vocabulary discipline
// ============================================================================

#[test]
fn session_open_receipt_no_regulated_finance_vocabulary() {
    let r = ProcessSessionOpenedReceipt::new(
        "session-vocab".to_string(),
        "coop:test".to_string(),
        "did:icn:someone".to_string(),
        1_750_000_000,
    );
    let json = serde_json::to_string(&r).unwrap().to_lowercase();
    for forbidden in [
        "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
    ] {
        assert!(
            !json.contains(forbidden),
            "serialized session-open receipt must not contain `{forbidden}`"
        );
    }
}
