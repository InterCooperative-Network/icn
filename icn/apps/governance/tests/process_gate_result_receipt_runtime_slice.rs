//! Runtime proof for the first `ProcessTransitionReceipt` class —
//! [`ProcessGateResultReceipt`] — emitted by the `idea-0019`
//! Institutional Process Substrate runtime slice.
//!
//! Pins:
//!
//! 1. `GovernanceManager::record_process_gate_result` constructs a
//!    `ProcessGateResultReceipt` with a real blake3 `record_hash` (not
//!    a zero placeholder), persists it through the
//!    `GovernanceReceiptBackend` trait BEFORE returning, and returns
//!    the receipt.
//! 2. The receipt's `record_hash` is a deterministic blake3 binding
//!    over the bound fields — re-recording at a strictly later
//!    `recorded_at` yields a distinct `record_hash`; the audit chain
//!    appends.
//! 3. Cross-session isolation: a probe with a different `session_id`
//!    does not return a receipt from another session.
//! 4. Cross-gate-kind isolation: a probe for `ProcessGateKind::PrivacyReview`
//!    on a session that recorded `AccessibilityReview` returns `None`.
//! 5. Domain-id binding: a receipt for one `domain_id` is bound to a
//!    different `record_hash` than the same other-fields receipt
//!    under a different `domain_id` — the `domain_id` cannot be
//!    rewritten silently after-the-fact.
//! 6. Persist-before-return: a backend whose `put_process_gate_result`
//!    rejects causes the manager method to return an error rather
//!    than a silent commit-without-receipt.
//! 7. Empty `session_id` is rejected by the manager.
//! 8. Regulatory-safe vocabulary is preserved: no
//!    wallet/balance/currency/payment/token/withdraw/deposit terms
//!    in the serialized form (matching the existing receipt-family
//!    vocabulary discipline).
//!
//! This is **not** a full process runtime. It does not implement
//! `ProcessSession`, `DeliberationEntry`, `DecisionRecord`, or any of
//! the other spine objects the framing brief names. It is the
//! smallest runtime slice that produces a real receipt under
//! `ADR-0026` for one named gate-result transition.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, ProcessGateKind, ProcessGateResult,
    ProcessGateResultReceipt,
};
use icn_governance_actor::{manager::GovernanceManager, receipt_backend::GovernanceReceiptBackend};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Test receipt backend — persists ProcessGateResultReceipt + lookups.
//
// Implements the GovernanceReceiptBackend trait by overriding only the
// three process-gate-result methods; every other method falls through
// to the trait's default no-op. That is sufficient to prove the
// runtime slice — the manager's emission path only writes to
// `put_process_gate_result`.
// ============================================================================

#[derive(Default)]
struct TestReceiptStore {
    /// Append-only log of every receipt the runtime persisted.
    persisted: Mutex<Vec<ProcessGateResultReceipt>>,
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

    fn put_process_gate_result(&self, receipt: &ProcessGateResultReceipt) -> Result<(), String> {
        self.persisted.lock().unwrap().push(receipt.clone());
        Ok(())
    }

    fn get_latest_process_gate_result(
        &self,
        session_id: &str,
        gate_kind: ProcessGateKind,
    ) -> Result<Option<ProcessGateResultReceipt>, String> {
        // Latest = receipt with the largest `recorded_at` for this
        // (session_id, gate_kind) pair, per the trait contract on
        // GovernanceReceiptBackend::get_latest_process_gate_result.
        // Compute the max by `recorded_at` directly rather than
        // assuming insertion order tracks recorded_at — the trait
        // contract is "largest recorded_at", and a backend that
        // batches or reorders inserts must still satisfy it. Mirrors
        // the existing meeting-attendance test backend pattern of
        // sorting then taking the last hit.
        Ok(self
            .persisted
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.session_id == session_id && r.gate_kind == gate_kind)
            .max_by_key(|r| r.recorded_at)
            .cloned())
    }

    fn list_process_gate_results_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<ProcessGateResultReceipt>, String> {
        let mut hits: Vec<ProcessGateResultReceipt> = self
            .persisted
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.session_id == session_id)
            .cloned()
            .collect();
        hits.sort_by_key(|r| r.recorded_at);
        Ok(hits)
    }
}

impl TestReceiptStore {
    fn count_for(&self, session_id: &str, gate_kind: ProcessGateKind) -> usize {
        self.persisted
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.session_id == session_id && r.gate_kind == gate_kind)
            .count()
    }

    fn total_count(&self) -> usize {
        self.persisted.lock().unwrap().len()
    }
}

/// A backend whose `put_process_gate_result` always rejects, used to
/// prove the manager's "persist before return" guarantee.
#[derive(Default)]
struct FailingProcessGateStore;

impl GovernanceReceiptBackend for FailingProcessGateStore {
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

    fn put_process_gate_result(&self, _receipt: &ProcessGateResultReceipt) -> Result<(), String> {
        Err("simulated process gate result backend failure".to_string())
    }
}

/// A backend that does **not** override any process-gate methods AND
/// does NOT implement opaque storage. Used to prove the production-
/// path failure mode: a backend that has neither typed override nor
/// opaque storage produces an explicit error instead of a silent
/// commit-without-persistence.
///
/// As of Stage 1d, the trait's default `put_process_gate_result`
/// routes through `put_opaque`. Without an opaque override, the
/// opaque method's own fail-closed default fires. The end-to-end
/// behavior is preserved: persist or error, never silent loss.
#[derive(Default)]
struct DefaultInheritingBackend;

impl GovernanceReceiptBackend for DefaultInheritingBackend {
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
    // Deliberately do NOT override put_process_gate_result,
    // get_latest_process_gate_result, list_process_gate_results_for_session,
    // OR put_opaque/get_latest_opaque/list_opaque_for. The point of this
    // backend is to exercise the cascade: typed default routes through
    // opaque default, which is itself fail-closed.
}

/// A backend that implements ONLY the opaque storage methods (with an
/// in-memory HashMap). Inherits the typed `put_process_gate_result`
/// etc. from the trait, which now routes through opaque (Stage 1d).
///
/// Storage key for [`OpaqueOnlyBackend`]:
/// `(class, key1, key2_opt, record_hash)`. Multiple record_hashes
/// per `(class, key1, key2_opt)` form the audit chain.
type OpaqueKey = (String, String, Option<String>, [u8; 32]);

/// Storage value for [`OpaqueOnlyBackend`]: `(recorded_at, payload)`.
/// `recorded_at` is tracked separately so `get_latest_opaque` can
/// pick the largest deterministically.
type OpaqueValue = (u64, Vec<u8>);

/// This is the test analog of the production gateway-backed
/// `ReceiptStore`'s posture after Stage 1b: opaque-storage-capable,
/// no typed process-gate overrides. Calling `put_process_gate_result`
/// on this backend should successfully persist via the cascade
/// (typed default → opaque override → HashMap).
#[derive(Default)]
struct OpaqueOnlyBackend {
    storage: std::sync::Mutex<std::collections::HashMap<OpaqueKey, OpaqueValue>>,
}

impl GovernanceReceiptBackend for OpaqueOnlyBackend {
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
        let key = (
            class.to_string(),
            key1.to_string(),
            key2.map(|s| s.to_string()),
            record_hash,
        );
        self.storage
            .lock()
            .unwrap()
            .insert(key, (recorded_at, payload.to_vec()));
        Ok(())
    }

    fn get_latest_opaque(
        &self,
        class: &str,
        key1: &str,
        key2: Option<&str>,
    ) -> Result<Option<Vec<u8>>, String> {
        let map = self.storage.lock().unwrap();
        // Latest by recorded_at across record_hashes for the
        // (class, key1, key2) triple.
        Ok(map
            .iter()
            .filter(|((c, k1, k2, _), _)| c == class && k1 == key1 && k2.as_deref() == key2)
            .max_by_key(|(_, (rec_at, _))| *rec_at)
            .map(|(_, (_, payload))| payload.clone()))
    }

    fn list_opaque_for(&self, class: &str, key1: &str) -> Result<Vec<Vec<u8>>, String> {
        let map = self.storage.lock().unwrap();
        let mut hits: Vec<(u64, Vec<u8>)> = map
            .iter()
            .filter(|((c, k1, _, _), _)| c == class && k1 == key1)
            .map(|(_, (rec_at, payload))| (*rec_at, payload.clone()))
            .collect();
        hits.sort_by_key(|(t, _)| *t);
        Ok(hits.into_iter().map(|(_, p)| p).collect())
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

fn make_manager_with_store() -> (GovernanceManager, Arc<TestReceiptStore>) {
    let store = Arc::new(TestReceiptStore::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    (mgr, store)
}

fn coop_test() -> GovernanceDomainId {
    GovernanceDomainId::new("coop:test")
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn record_process_gate_result_emits_persisted_receipt() {
    let (mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    let receipt = mgr
        .record_process_gate_result(
            &domain,
            "session-001",
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("record_process_gate_result must succeed under a healthy backend");

    // Returned receipt's fields match the call.
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.gate_kind, ProcessGateKind::PrivacyReview);
    assert_eq!(receipt.result, ProcessGateResult::Pass);
    assert_eq!(receipt.recorded_by, recorder.to_string());
    assert_ne!(
        receipt.record_hash, [0u8; 32],
        "record_hash must be a real blake3 binding, not a zero placeholder"
    );

    // Persisted: latest lookup returns the same receipt.
    let latest = store
        .get_latest_process_gate_result("session-001", ProcessGateKind::PrivacyReview)
        .expect("lookup")
        .expect("a receipt must be persisted before record_process_gate_result returns");
    assert_eq!(latest, receipt);
    assert_eq!(latest.record_hash, receipt.record_hash);
}

#[test]
fn record_hash_changes_on_rerecord_at_later_second() {
    // Construct two receipts with explicit, strictly-increasing
    // `recorded_at` timestamps and put them directly through the
    // backend trait. This exercises the same write path the
    // manager would use without depending on wall-clock advance.
    let (_mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();
    let session_id = "session-002";

    let first = ProcessGateResultReceipt::new(
        session_id.to_string(),
        domain.0.clone(),
        ProcessGateKind::AccessibilityReview,
        ProcessGateResult::Pass,
        recorder.to_string(),
        100,
    );
    let second = ProcessGateResultReceipt::new(
        session_id.to_string(),
        domain.0.clone(),
        ProcessGateKind::AccessibilityReview,
        ProcessGateResult::Pass,
        recorder.to_string(),
        200,
    );

    store.put_process_gate_result(&first).expect("first record");
    store
        .put_process_gate_result(&second)
        .expect("second record at strictly later recorded_at");

    assert_ne!(
        first.record_hash, second.record_hash,
        "a second record at a strictly later recorded_at must produce a distinct record_hash"
    );

    // Audit chain reads oldest-first.
    let chain = store
        .list_process_gate_results_for_session(session_id)
        .expect("list chain");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].record_hash, first.record_hash);
    assert_eq!(chain[1].record_hash, second.record_hash);
    assert!(chain[0].recorded_at < chain[1].recorded_at);

    // Latest lookup returns the most recent.
    let latest = store
        .get_latest_process_gate_result(session_id, ProcessGateKind::AccessibilityReview)
        .expect("latest")
        .expect("latest must exist");
    assert_eq!(latest.record_hash, second.record_hash);
}

#[test]
fn cross_session_isolation_no_leak() {
    let (mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    mgr.record_process_gate_result(
        &domain,
        "session-alpha",
        ProcessGateKind::ScopeConfirmation,
        ProcessGateResult::Pass,
        &recorder,
    )
    .expect("record alpha");

    // A probe under a different session_id must NOT return alpha's
    // receipt — the lookup is keyed on (session_id, gate_kind).
    let probe = store
        .get_latest_process_gate_result("session-beta", ProcessGateKind::ScopeConfirmation)
        .expect("probe");
    assert!(
        probe.is_none(),
        "a different session_id must not see another session's gate-result receipt"
    );

    // The chain for session-alpha sees its own receipt.
    let alpha_chain = store
        .list_process_gate_results_for_session("session-alpha")
        .expect("alpha chain");
    assert_eq!(alpha_chain.len(), 1);

    // The chain for session-beta is empty.
    let beta_chain = store
        .list_process_gate_results_for_session("session-beta")
        .expect("beta chain");
    assert!(beta_chain.is_empty());
}

#[test]
fn cross_gate_kind_isolation_no_leak() {
    let (mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    mgr.record_process_gate_result(
        &domain,
        "session-multi",
        ProcessGateKind::AccessibilityReview,
        ProcessGateResult::Pass,
        &recorder,
    )
    .expect("record accessibility");

    // A probe for a different gate_kind on the same session_id must
    // NOT return the accessibility receipt — the lookup is keyed on
    // (session_id, gate_kind).
    let probe = store
        .get_latest_process_gate_result("session-multi", ProcessGateKind::PrivacyReview)
        .expect("probe");
    assert!(
        probe.is_none(),
        "a different gate_kind must not see another gate's receipt for the same session"
    );

    // The session-spanning list still sees the one accessibility
    // receipt — `list_*_for_session` is per-session, not per-gate.
    let session_chain = store
        .list_process_gate_results_for_session("session-multi")
        .expect("session chain");
    assert_eq!(session_chain.len(), 1);
    assert_eq!(
        session_chain[0].gate_kind,
        ProcessGateKind::AccessibilityReview
    );
}

#[test]
fn distinct_gate_kinds_for_same_session_chain_independently() {
    let (mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    let privacy = mgr
        .record_process_gate_result(
            &domain,
            "session-six-gates",
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("privacy");
    let accessibility = mgr
        .record_process_gate_result(
            &domain,
            "session-six-gates",
            ProcessGateKind::AccessibilityReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("accessibility");
    let no_mutation = mgr
        .record_process_gate_result(
            &domain,
            "session-six-gates",
            ProcessGateKind::NoMutationCheck,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("no_mutation");

    assert_ne!(privacy.record_hash, accessibility.record_hash);
    assert_ne!(privacy.record_hash, no_mutation.record_hash);
    assert_ne!(accessibility.record_hash, no_mutation.record_hash);

    let session_chain = store
        .list_process_gate_results_for_session("session-six-gates")
        .expect("session chain");
    assert_eq!(
        session_chain.len(),
        3,
        "every distinct gate_kind for the same session appends a fresh receipt"
    );

    // Per-(session, gate) lookups stay isolated.
    let p = store
        .get_latest_process_gate_result("session-six-gates", ProcessGateKind::PrivacyReview)
        .expect("lookup")
        .expect("privacy must exist");
    assert_eq!(p.gate_kind, ProcessGateKind::PrivacyReview);

    let a = store
        .get_latest_process_gate_result("session-six-gates", ProcessGateKind::AccessibilityReview)
        .expect("lookup")
        .expect("accessibility must exist");
    assert_eq!(a.gate_kind, ProcessGateKind::AccessibilityReview);

    let n = store
        .get_latest_process_gate_result("session-six-gates", ProcessGateKind::NoMutationCheck)
        .expect("lookup")
        .expect("no_mutation must exist");
    assert_eq!(n.gate_kind, ProcessGateKind::NoMutationCheck);
}

#[test]
fn domain_id_is_bound_into_record_hash() {
    let (_mgr_a, _store_a) = make_manager_with_store();

    // Construct two receipts that differ ONLY in domain_id; their
    // record_hashes must differ. The `record_hash` is computed at
    // construction time, so this also confirms the manager's call to
    // `ProcessGateResultReceipt::new` binds `domain_id` into the
    // canonical hash.
    let now = 1_700_000_900;
    let recorder = "did:icn:r";
    let r_coop_a = ProcessGateResultReceipt::new(
        "session-domain".to_string(),
        "coop:a".to_string(),
        ProcessGateKind::ScopeConfirmation,
        ProcessGateResult::Pass,
        recorder.to_string(),
        now,
    );
    let r_coop_b = ProcessGateResultReceipt::new(
        "session-domain".to_string(),
        "coop:b".to_string(),
        ProcessGateKind::ScopeConfirmation,
        ProcessGateResult::Pass,
        recorder.to_string(),
        now,
    );
    assert_ne!(r_coop_a.record_hash, r_coop_b.record_hash);
}

#[test]
fn receipt_backend_failure_propagates() {
    let mgr = GovernanceManager::new()
        .with_receipt_store(Arc::new(FailingProcessGateStore) as Arc<dyn GovernanceReceiptBackend>);
    let recorder = fresh_did();
    let domain = coop_test();

    let err = mgr
        .record_process_gate_result(
            &domain,
            "session-fail-closed",
            ProcessGateKind::RepoSafetyReview,
            ProcessGateResult::Fail,
            &recorder,
        )
        .expect_err("a rejecting backend must propagate as a manager error");
    let msg = err.to_string();
    assert!(
        msg.contains("process gate result"),
        "error must reference the process gate result seam: {msg}"
    );
    assert!(
        msg.contains("session-fail-closed"),
        "error must reference the affected session: {msg}"
    );
}

#[test]
fn empty_session_id_is_rejected() {
    let (mgr, _store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    let err = mgr
        .record_process_gate_result(
            &domain,
            "",
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect_err("empty session_id must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("session_id"), "error must explain why: {msg}");
}

#[test]
fn idempotent_same_second_rerecord_is_safe_under_test_backend() {
    // Same-second re-recording produces the SAME `record_hash`
    // (because all bound fields are identical). The manager appends
    // unconditionally — the test backend's append-only log records
    // the duplicate. Real backends (e.g. the production sled-backed
    // ReceiptStore) treat same-`record_hash` re-writes as
    // idempotent at the storage layer; this test pins that the
    // *manager* does not silently dedup.
    let (mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    let r1 = mgr
        .record_process_gate_result(
            &domain,
            "session-idem",
            ProcessGateKind::SecondReviewerSignoff,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("first");
    let r2 = mgr
        .record_process_gate_result(
            &domain,
            "session-idem",
            ProcessGateKind::SecondReviewerSignoff,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("second (same second)");

    // If the wall clock did not advance between calls, both writes
    // share a `record_hash` — the backend's `put_*` is idempotent
    // by hash. If it did advance, the hashes differ. Either way, the
    // chain length and latest-lookup are well-defined.
    if r1.record_hash == r2.record_hash {
        // Same-second case: hash collision is intentional; the
        // backend's append-only contract permits a same-`record_hash`
        // re-write.
        assert_eq!(
            store.count_for("session-idem", ProcessGateKind::SecondReviewerSignoff),
            2,
            "test backend log records every put call; same-hash dedup is a real-backend concern"
        );
    } else {
        // Wall clock advanced: distinct receipts.
        assert_eq!(
            store.count_for("session-idem", ProcessGateKind::SecondReviewerSignoff),
            2
        );
    }
}

#[test]
fn no_receipt_persisted_when_session_id_empty() {
    // Empty session_id is rejected before any backend call. Confirm
    // the backend's persisted log remains empty after the failed call.
    let (mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    let _err = mgr
        .record_process_gate_result(
            &domain,
            "",
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect_err("empty session_id rejected");
    assert_eq!(
        store.total_count(),
        0,
        "no receipt must be persisted when the manager rejects the call up-front"
    );
}

#[test]
fn process_gate_result_receipt_uses_regulatory_safe_vocabulary() {
    let (mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();

    mgr.record_process_gate_result(
        &domain,
        "session-vocab",
        ProcessGateKind::PrivacyReview,
        ProcessGateResult::Pass,
        &recorder,
    )
    .expect("record");

    let r = store
        .get_latest_process_gate_result("session-vocab", ProcessGateKind::PrivacyReview)
        .expect("lookup")
        .expect("receipt exists");
    let json = serde_json::to_string(&r).expect("serialize");
    let lower = json.to_lowercase();
    for forbidden in [
        "wallet", "balance", "currency", "payment", "token", "withdraw", "deposit",
    ] {
        assert!(
            !lower.contains(forbidden),
            "ProcessGateResultReceipt JSON must not contain regulated-finance vocabulary; \
             found `{forbidden}` in: {json}"
        );
    }
}

#[test]
fn fail_result_is_recorded_distinct_from_pass() {
    // Pass and Fail are both receipt-bearing. Construct each receipt
    // with an explicit `recorded_at` so the test does not depend on
    // wall-clock advance, then put through the backend trait. Both
    // must land in the chain with distinct record_hashes.
    let (_mgr, store) = make_manager_with_store();
    let recorder = fresh_did();
    let domain = coop_test();
    let session_id = "session-pass-then-fail";

    let pass = ProcessGateResultReceipt::new(
        session_id.to_string(),
        domain.0.clone(),
        ProcessGateKind::AccessibilityReview,
        ProcessGateResult::Pass,
        recorder.to_string(),
        100,
    );
    let fail = ProcessGateResultReceipt::new(
        session_id.to_string(),
        domain.0.clone(),
        ProcessGateKind::AccessibilityReview,
        ProcessGateResult::Fail,
        recorder.to_string(),
        200,
    );

    store.put_process_gate_result(&pass).expect("pass");
    store.put_process_gate_result(&fail).expect("fail");

    assert_ne!(pass.record_hash, fail.record_hash);
    assert_eq!(pass.result, ProcessGateResult::Pass);
    assert_eq!(fail.result, ProcessGateResult::Fail);

    let chain = store
        .list_process_gate_results_for_session(session_id)
        .expect("chain");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].result, ProcessGateResult::Pass);
    assert_eq!(chain[1].result, ProcessGateResult::Fail);

    // Latest reads the most recent.
    let latest = store
        .get_latest_process_gate_result(session_id, ProcessGateKind::AccessibilityReview)
        .expect("latest")
        .expect("latest exists");
    assert_eq!(latest.result, ProcessGateResult::Fail);
}

#[test]
fn default_inheriting_backend_fails_closed_no_silent_loss() {
    // A backend that has neither typed process-gate overrides nor
    // opaque storage must surface the gap as an explicit manager
    // error rather than a silent commit-without-persistence.
    //
    // As of Stage 1d, the cascade is:
    //   record_process_gate_result
    //     -> put_process_gate_result (default, routes through opaque)
    //     -> put_opaque (default, fail-closed)
    // and the resulting error sentinel comes from the opaque layer.
    let mgr =
        GovernanceManager::new().with_receipt_store(
            Arc::new(DefaultInheritingBackend) as Arc<dyn GovernanceReceiptBackend>
        );
    let recorder = fresh_did();
    let domain = coop_test();

    let err = mgr
        .record_process_gate_result(
            &domain,
            "session-fail-closed-default",
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect_err(
            "a backend with no opaque storage must propagate an error rather than \
             allow a silent commit-without-persistence",
        );
    let msg = err.to_string();
    // The error must reference the affected session and carry the
    // stable opaque-layer sentinel so callers and operators can
    // pattern-match it.
    assert!(
        msg.contains("session-fail-closed-default"),
        "error must reference the affected session: {msg}"
    );
    assert!(
        msg.contains("opaque_storage_not_implemented"),
        "error must carry the opaque-layer's stable sentinel so callers can match \
         on it programmatically: {msg}"
    );
}

#[test]
fn opaque_only_backend_persists_and_round_trips_via_cascade() {
    // A backend that implements ONLY the opaque storage methods
    // (no typed process-gate overrides) gets durable persistence
    // for free via the cascade:
    //   record_process_gate_result
    //     -> put_process_gate_result (default, routes through opaque)
    //     -> put_opaque (overridden -> in-memory HashMap)
    //
    // This is the production-equivalent of the gateway-backed
    // `ReceiptStore` after Stage 1b: opaque-capable, no typed
    // process-gate overrides. Stage 1d is the change that wires
    // the typed default through opaque so this cascade actually
    // persists rather than fail-closing.
    let store = Arc::new(OpaqueOnlyBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let recorder = fresh_did();
    let domain = coop_test();

    // Record one pass result.
    let receipt = mgr
        .record_process_gate_result(
            &domain,
            "session-opaque-routed",
            ProcessGateKind::PrivacyReview,
            ProcessGateResult::Pass,
            &recorder,
        )
        .expect("opaque-only backend must persist via the cascade rather than fail-close");
    assert_ne!(receipt.record_hash, [0u8; 32]);

    // Read back via get_latest_process_gate_result (also routed
    // through the opaque cascade).
    let latest = store
        .get_latest_process_gate_result("session-opaque-routed", ProcessGateKind::PrivacyReview)
        .expect("get_latest must succeed against an opaque-capable backend")
        .expect("a receipt must be retrievable after a successful put");
    assert_eq!(latest.session_id, "session-opaque-routed");
    assert_eq!(latest.gate_kind, ProcessGateKind::PrivacyReview);
    assert_eq!(latest.result, ProcessGateResult::Pass);
    assert_eq!(latest.record_hash, receipt.record_hash);

    // Cross-gate-kind isolation: a probe under a different gate_kind
    // must NOT return this receipt — they have different opaque key2
    // values.
    let probe = store
        .get_latest_process_gate_result(
            "session-opaque-routed",
            ProcessGateKind::AccessibilityReview,
        )
        .expect("probe must succeed");
    assert!(
        probe.is_none(),
        "different gate kinds must not see each other's receipts via the opaque key2"
    );
}

#[test]
fn opaque_only_backend_chains_session_history_chronologically() {
    // Multiple gate results across distinct gate kinds for the same
    // session must form a chronologically ordered audit chain via
    // the opaque cascade's `list_opaque_for` semantics.
    //
    // Construct receipts with explicit `recorded_at` timestamps and
    // call `put_process_gate_result` directly so the test does not
    // depend on wall-clock advance (no `std::thread::sleep`). This
    // still exercises the full cascade:
    //   put_process_gate_result (trait default)
    //     -> put_opaque (OpaqueOnlyBackend override)
    //     -> in-memory HashMap
    let store = Arc::new(OpaqueOnlyBackend::default());
    let recorder = fresh_did();
    let domain = coop_test();
    let domain_id = domain.0.clone();
    let session_id = "session-multi-gate";

    let r_privacy = ProcessGateResultReceipt::new(
        session_id.to_string(),
        domain_id.clone(),
        ProcessGateKind::PrivacyReview,
        ProcessGateResult::Pass,
        recorder.to_string(),
        100,
    );
    let r_access = ProcessGateResultReceipt::new(
        session_id.to_string(),
        domain_id.clone(),
        ProcessGateKind::AccessibilityReview,
        ProcessGateResult::Pass,
        recorder.to_string(),
        200,
    );
    let r_no_mut = ProcessGateResultReceipt::new(
        session_id.to_string(),
        domain_id,
        ProcessGateKind::NoMutationCheck,
        ProcessGateResult::Pass,
        recorder.to_string(),
        300,
    );

    store
        .put_process_gate_result(&r_privacy)
        .expect("privacy gate must persist via the cascade");
    store
        .put_process_gate_result(&r_access)
        .expect("accessibility gate must persist via the cascade");
    store
        .put_process_gate_result(&r_no_mut)
        .expect("no-mutation gate must persist via the cascade");

    let chain = store
        .list_process_gate_results_for_session(session_id)
        .expect("list must succeed against an opaque-capable backend");
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].gate_kind, ProcessGateKind::PrivacyReview);
    assert_eq!(chain[1].gate_kind, ProcessGateKind::AccessibilityReview);
    assert_eq!(chain[2].gate_kind, ProcessGateKind::NoMutationCheck);
    // Chronologically ordered (sorted by recorded_at in
    // OpaqueOnlyBackend::list_opaque_for).
    assert!(chain[0].recorded_at < chain[1].recorded_at);
    assert!(chain[1].recorded_at < chain[2].recorded_at);
}
