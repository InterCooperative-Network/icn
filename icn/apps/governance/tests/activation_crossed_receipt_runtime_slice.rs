//! Runtime proof for the fifth `ProcessTransitionReceipt` class —
//! [`ActivationCrossedReceipt`] — per the merged #2294 contract
//! (`docs/design/activation-crossed-receipt-runtime-dogfood.md`) and the
//! #2295 B1/B2/B3 decision rung
//! (`docs/design/activation-crossed-receipt-decision-rung.md`).
//!
//! Pins the merged contract:
//!
//! 1. `GovernanceManager::record_activation_crossed` requires an
//!    already-opened session, a **recorded decision** to reference (B1), and
//!    a **non-empty basis of passed gate results** (B2). It constructs a
//!    receipt with a real blake3 `record_hash`, persists it through the
//!    backend's atomic insert-if-absent BEFORE returning, and returns
//!    `Crossed(receipt)`.
//! 2. B1: the crossing names the decision by both `decision_id` and
//!    `decision_record_hash`; the reference is **verified** — a missing,
//!    wrong-session, wrong-domain, or hash-mismatched decision fails closed
//!    and persists nothing.
//! 3. B2: `gate_basis` is a fingerprint over the sorted, de-duplicated
//!    passed gate-result `record_hash`es; a `Fail`, absent, wrong-domain, or
//!    wrong-session gate refuses the crossing; an empty basis refuses; basis
//!    input ordering does not matter and duplicates de-dupe.
//! 4. B3: a single caller-supplied `recorded_at`, hashed but excluded from
//!    identity — a retry with identical stable identity returns the ORIGINAL
//!    receipt unchanged, never restamped.
//! 5. Same `activation_id` with a different decision reference, gate basis,
//!    or crosser fails closed with the stable `activation_crossed_conflict`
//!    prefix; the original is untouched.
//! 6. Empty/whitespace ids rejected; a missing receipt store is an error;
//!    backend failure fails closed; concurrent duplicate records serialize
//!    to exactly one persisted crossing.
//! 7. The composite storage key is injective: `("ab","c")` vs `("a","bc")`
//!    domain/session pairs never alias, and two domains sharing a
//!    `session_id` never mix crossings.
//!
//! This records a **process fact and grants zero authority.** "Activation"
//! here is a local/dev/fixture institutional fact — a recorded decision
//! accepted as ready to drive a later action-planning step, conditioned on
//! its required gates observing `pass`. It is the gate, not the mutation; it
//! is not production activation, mutation planning, or mutation application.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{
    ActivationCrossedReceipt, GovernanceDecisionReceipt, GovernanceDomainId, ProcessGateKind,
    ProcessGateResult,
};
use icn_governance_actor::manager::{
    ActivationCrossedOutcome, DecisionRecordOutcome, GovernanceManager, ProcessSessionOpenOutcome,
};
use icn_governance_actor::receipt_backend::{
    activation_crossed_composite_key1, GovernanceReceiptBackend,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-backed test store — the test analog of the production
// gateway-backed ReceiptStore: it implements the opaque primitives
// (including the atomic `put_opaque_if_absent` and the append `put_opaque`)
// and NO typed overrides, so the trait's typed activation-crossed defaults
// are exercised end-to-end.
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

/// Backend that persists everything except the activation-crossed insert —
/// proves fail-closed activation persistence with all preconditions
/// satisfied.
#[derive(Default)]
struct FailingActivationBackend {
    inner: OpaqueUniqueBackend,
}

impl GovernanceReceiptBackend for FailingActivationBackend {
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
        if class == "activation_crossed" {
            return Err("simulated activation-crossed backend failure".to_string());
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

/// The composite key the activation chain lives under (for chain_len asserts).
fn activation_key1(domain: &str, session: &str) -> String {
    activation_crossed_composite_key1(domain, session)
}

/// Open a session so the activation precondition is satisfied.
fn open_session(mgr: &GovernanceManager, domain: &GovernanceDomainId, session: &str, by: &Did) {
    match mgr
        .record_process_session_opened(domain, session, by)
        .unwrap()
    {
        ProcessSessionOpenOutcome::Opened(_) | ProcessSessionOpenOutcome::AlreadyOpened(_) => {}
    }
}

/// Record a decision and return its `record_hash` (the B1 proof link).
fn record_decision(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    decision_id: &str,
    by: &Did,
    body: [u8; 32],
) -> [u8; 32] {
    match mgr
        .record_decision(domain, session, decision_id, by, body)
        .unwrap()
    {
        DecisionRecordOutcome::Recorded(r) | DecisionRecordOutcome::AlreadyRecorded(r) => {
            r.record_hash
        }
    }
}

/// Record a passed gate result and return its `record_hash` (a basis input).
fn record_pass_gate(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    kind: ProcessGateKind,
    by: &Did,
) -> [u8; 32] {
    mgr.record_process_gate_result(domain, session, kind, ProcessGateResult::Pass, by)
        .unwrap()
        .record_hash
}

/// Record a failed gate result and return its `record_hash`.
fn record_fail_gate(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    kind: ProcessGateKind,
    by: &Did,
) -> [u8; 32] {
    mgr.record_process_gate_result(domain, session, kind, ProcessGateResult::Fail, by)
        .unwrap()
        .record_hash
}

// ============================================================================
// Happy path — construct, persist, retrieve, verify B1 + B2 links
// ============================================================================

#[test]
fn cross_persists_and_returns_receipt_with_verified_links() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );
    let g2 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::AccessibilityReview,
        &actor,
    );

    let outcome = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1, g2],
            &actor,
        )
        .expect("first crossing must succeed");
    let ActivationCrossedOutcome::Crossed(receipt) = outcome else {
        panic!("first crossing must be Crossed, got {outcome:?}");
    };
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.activation_id, "activation-001");
    assert_eq!(receipt.decision_id, "decision-001");
    assert_eq!(receipt.crossed_by, actor.to_string());
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");

    // B1: decision_record_hash equals the persisted decision receipt hash.
    assert_eq!(
        receipt.decision_record_hash, decision_hash,
        "B1 proof link binds the recorded decision's real record_hash"
    );
    // B2: gate_basis equals an independent sorted/de-duped fingerprint of the
    // passed gate-result hashes.
    assert_eq!(
        receipt.gate_basis,
        ActivationCrossedReceipt::compute_gate_basis(&[g1, g2]),
        "B2 gate_basis is the fingerprint of the declared passed gate results"
    );

    // Persist-before-return: durable under the injective composite key.
    assert_eq!(
        store.chain_len(
            "activation_crossed",
            &activation_key1("coop:test", "session-001"),
            Some("activation-001"),
        ),
        1,
        "exactly one persisted crossing"
    );
    // Point read hydrates the same receipt.
    let read = mgr
        .get_activation_crossed(&domain, "session-001", "activation-001")
        .unwrap()
        .expect("point read must hydrate");
    assert_eq!(read, receipt);
}

// ============================================================================
// Session precondition
// ============================================================================

#[test]
fn unopened_session_fails_closed_and_creates_nothing() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    // No open_session call.

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-ghost",
            "activation-001",
            "decision-001",
            [1u8; 32],
            &[[2u8; 32]],
            &actor,
        )
        .expect_err("unopened session must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_session_not_opened"),
        "stable precondition prefix, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "activation_crossed",
            &activation_key1("coop:test", "session-ghost"),
            Some("activation-001"),
        ),
        0,
        "nothing persisted"
    );
}

// ============================================================================
// B1 — decision reference preconditions (verified, not asserted)
// ============================================================================

#[test]
fn missing_decision_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );
    // No decision recorded.

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-404",
            [7u8; 32],
            &[g1],
            &actor,
        )
        .expect_err("missing decision must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_decision_not_found"),
        "stable prefix, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "activation_crossed",
            &activation_key1("coop:test", "session-001"),
            Some("activation-001"),
        ),
        0
    );
}

#[test]
fn wrong_session_decision_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-a", &actor);
    open_session(&mgr, &domain, "session-b", &actor);
    // Decision recorded in session-a only.
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-a",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-b",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    // Activation in session-b cites the session-a decision — invisible under
    // session-b's composite key.
    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-b",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1],
            &actor,
        )
        .expect_err("wrong-session decision must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_decision_not_found"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn wrong_domain_decision_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    open_session(&mgr, &d1, "session-shared", &actor);
    open_session(&mgr, &d2, "session-shared", &actor);
    // Decision recorded in domain d1 only.
    let decision_hash = record_decision(
        &mgr,
        &d1,
        "session-shared",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &d2,
        "session-shared",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    // Activation in d2 cites the d1 decision — invisible under d2's composite
    // key.
    let err = mgr
        .record_activation_crossed(
            &d2,
            "session-shared",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1],
            &actor,
        )
        .expect_err("wrong-domain decision must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_decision_not_found"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn decision_id_hash_mismatch_fails_closed() {
    // The supplied decision_id references a real decision, but the supplied
    // decision_record_hash belongs to a DIFFERENT decision — the reference is
    // verified against the recorded decision and refused on mismatch.
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let other_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-002",
        &actor,
        [8u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            other_hash, // hash of decision-002, not decision-001
            &[g1],
            &actor,
        )
        .expect_err("decision_id/record_hash mismatch must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_decision_mismatch"),
        "stable prefix, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "activation_crossed",
            &activation_key1("coop:test", "session-001"),
            Some("activation-001"),
        ),
        0
    );
}

// ============================================================================
// B2 — gate-basis preconditions
// ============================================================================

#[test]
fn empty_gate_basis_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[],
            &actor,
        )
        .expect_err("empty basis must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_empty_gate_basis"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn absent_gate_hash_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    // No gate results recorded; cite a phantom hash.

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[[123u8; 32]],
            &actor,
        )
        .expect_err("absent gate hash must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_gate_not_found"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn failed_gate_hash_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let failed = record_fail_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[failed],
            &actor,
        )
        .expect_err("failed gate in basis must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_gate_not_passed"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn wrong_domain_gate_hash_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    open_session(&mgr, &d1, "session-shared", &actor);
    open_session(&mgr, &d2, "session-shared", &actor);
    let decision_hash = record_decision(
        &mgr,
        &d2,
        "session-shared",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    // Pass gate recorded in domain d1 only (same session id).
    let g_d1 = record_pass_gate(
        &mgr,
        &d1,
        "session-shared",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    // Activation in d2 cites the d1 gate — the domain-scoped read excludes it.
    let err = mgr
        .record_activation_crossed(
            &d2,
            "session-shared",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g_d1],
            &actor,
        )
        .expect_err("wrong-domain gate must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_gate_not_found"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn wrong_session_gate_hash_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-a", &actor);
    open_session(&mgr, &domain, "session-b", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-b",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    // Pass gate recorded in session-a only.
    let g_a = record_pass_gate(
        &mgr,
        &domain,
        "session-a",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-b",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g_a],
            &actor,
        )
        .expect_err("wrong-session gate must fail closed");
    assert!(
        err.to_string()
            .starts_with("activation_crossed_gate_not_found"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn basis_ordering_does_not_matter() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );
    let g2 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::AccessibilityReview,
        &actor,
    );

    let ActivationCrossedOutcome::Crossed(a) = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "act-a",
            "decision-001",
            decision_hash,
            &[g1, g2],
            &actor,
        )
        .unwrap()
    else {
        panic!("act-a must cross");
    };
    let ActivationCrossedOutcome::Crossed(b) = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "act-b",
            "decision-001",
            decision_hash,
            &[g2, g1],
            &actor,
        )
        .unwrap()
    else {
        panic!("act-b must cross");
    };
    assert_eq!(
        a.gate_basis, b.gate_basis,
        "gate_basis is order-independent for the same declared set"
    );
}

#[test]
fn duplicated_basis_hashes_dedupe_deterministically() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );
    let g2 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::AccessibilityReview,
        &actor,
    );

    let ActivationCrossedOutcome::Crossed(dup) = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "act-dup",
            "decision-001",
            decision_hash,
            &[g1, g1, g2, g1],
            &actor,
        )
        .unwrap()
    else {
        panic!("act-dup must cross");
    };
    assert_eq!(
        dup.gate_basis,
        ActivationCrossedReceipt::compute_gate_basis(&[g1, g2]),
        "duplicated basis hashes de-dupe to the same fingerprint as the unique set"
    );
}

// ============================================================================
// B3 / idempotency — retry returns original, never restamped
// ============================================================================

#[test]
fn same_identity_retry_returns_original_never_restamped() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    let ActivationCrossedOutcome::Crossed(original) = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1],
            &actor,
        )
        .unwrap()
    else {
        panic!("first crossing must be Crossed");
    };

    // Retry with identical stable identity. The manager stamps a fresh
    // recorded_at internally on every attempt — proving recorded_at /
    // record_hash are NOT identity inputs: the retry still resolves to the
    // ORIGINAL receipt, byte-identical.
    let outcome = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1],
            &actor,
        )
        .expect("same-identity retry must succeed");
    let ActivationCrossedOutcome::AlreadyCrossed(returned) = outcome else {
        panic!("retry must be AlreadyCrossed, got {outcome:?}");
    };
    assert_eq!(
        returned.recorded_at, original.recorded_at,
        "never restamped"
    );
    assert_eq!(returned.record_hash, original.record_hash);
    assert_eq!(returned, original);
    assert_eq!(
        store.chain_len(
            "activation_crossed",
            &activation_key1("coop:test", "session-001"),
            Some("activation-001"),
        ),
        1,
        "retry must not append a second record"
    );
}

// ============================================================================
// Conflicts — same activation_id, different identity fields
// ============================================================================

#[test]
fn conflict_on_different_decision_reference_fails_closed() {
    // decision_id and decision_record_hash are both stable-identity inputs.
    // Through the validated path they co-vary (each decision_id maps to one
    // record_hash; a same-id/wrong-hash reference is caught earlier as
    // `activation_crossed_decision_mismatch`, tested above). Referencing a
    // DIFFERENT recorded decision for the same activation_id conflicts.
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let d1 = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let d2 = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-002",
        &actor,
        [8u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    mgr.record_activation_crossed(
        &domain,
        "session-001",
        "activation-001",
        "decision-001",
        d1,
        &[g1],
        &actor,
    )
    .expect("first crossing");

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-002",
            d2,
            &[g1],
            &actor,
        )
        .expect_err("different decision reference must conflict");
    assert!(
        err.to_string().starts_with("activation_crossed_conflict"),
        "stable conflict prefix, got: {err}"
    );
    let read = mgr
        .get_activation_crossed(&domain, "session-001", "activation-001")
        .unwrap()
        .unwrap();
    assert_eq!(read.decision_id, "decision-001", "original untouched");
    assert_eq!(
        store.chain_len(
            "activation_crossed",
            &activation_key1("coop:test", "session-001"),
            Some("activation-001"),
        ),
        1
    );
}

#[test]
fn conflict_on_different_gate_basis_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );
    let g2 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::AccessibilityReview,
        &actor,
    );

    mgr.record_activation_crossed(
        &domain,
        "session-001",
        "activation-001",
        "decision-001",
        decision_hash,
        &[g1],
        &actor,
    )
    .expect("first crossing with basis [g1]");

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1, g2],
            &actor,
        )
        .expect_err("different gate basis must conflict");
    assert!(
        err.to_string().starts_with("activation_crossed_conflict"),
        "stable conflict prefix, got: {err}"
    );
}

#[test]
fn conflict_on_different_crosser_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let other = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    mgr.record_activation_crossed(
        &domain,
        "session-001",
        "activation-001",
        "decision-001",
        decision_hash,
        &[g1],
        &actor,
    )
    .expect("first crossing");

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1],
            &other,
        )
        .expect_err("different crosser must conflict");
    assert!(
        err.to_string().starts_with("activation_crossed_conflict"),
        "stable conflict prefix, got: {err}"
    );
}

// ============================================================================
// Input validation, missing store, backend failure
// ============================================================================

#[test]
fn empty_and_whitespace_ids_rejected_before_persistence() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    for (d, s, a, dec) in [
        ("", "session-001", "activation-001", "decision-001"),
        ("   ", "session-001", "activation-001", "decision-001"),
        ("coop:test", "", "activation-001", "decision-001"),
        ("coop:test", "  \t", "activation-001", "decision-001"),
        ("coop:test", "session-001", "", "decision-001"),
        ("coop:test", "session-001", " \n ", "decision-001"),
        ("coop:test", "session-001", "activation-001", ""),
        ("coop:test", "session-001", "activation-001", "  "),
    ] {
        let err = mgr
            .record_activation_crossed(
                &GovernanceDomainId::new(d),
                s,
                a,
                dec,
                decision_hash,
                &[g1],
                &actor,
            )
            .expect_err("empty/whitespace ids must be rejected");
        assert!(
            err.to_string().contains("non-empty"),
            "id-validation error, got: {err}"
        );
    }
}

#[test]
fn missing_receipt_store_is_an_error() {
    let mgr = GovernanceManager::new(); // no receipt store wired
    let actor = fresh_did();
    let err = mgr
        .record_activation_crossed(
            &coop_test(),
            "session-001",
            "activation-001",
            "decision-001",
            [1u8; 32],
            &[[2u8; 32]],
            &actor,
        )
        .expect_err("missing store must be an error");
    assert!(
        err.to_string().contains("receipt store is required"),
        "store-required error, got: {err}"
    );
}

#[test]
fn backend_failure_fails_closed() {
    let store = Arc::new(FailingActivationBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    let err = mgr
        .record_activation_crossed(
            &domain,
            "session-001",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1],
            &actor,
        )
        .expect_err("backend failure must surface");
    assert!(
        err.to_string()
            .contains("simulated activation-crossed backend failure"),
        "backend error surfaced, got: {err}"
    );
    assert!(
        mgr.get_activation_crossed(&domain, "session-001", "activation-001")
            .unwrap()
            .is_none(),
        "nothing half-written"
    );
}

// ============================================================================
// Concurrency — atomic uniqueness under racing duplicates
// ============================================================================

#[test]
fn concurrent_duplicate_crossings_serialize_to_one_winner() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    let mgr = Arc::new(mgr);
    let mut handles = Vec::new();
    for _ in 0..8 {
        let mgr = mgr.clone();
        let actor = actor.clone();
        let domain = domain.clone();
        handles.push(std::thread::spawn(move || {
            mgr.record_activation_crossed(
                &domain,
                "session-001",
                "activation-race",
                "decision-001",
                decision_hash,
                &[g1],
                &actor,
            )
        }));
    }
    let mut receipts = Vec::new();
    for h in handles {
        let outcome = h.join().unwrap().expect("same-identity race must succeed");
        match outcome {
            ActivationCrossedOutcome::Crossed(r) | ActivationCrossedOutcome::AlreadyCrossed(r) => {
                receipts.push(r)
            }
        }
    }
    assert_eq!(
        store.chain_len(
            "activation_crossed",
            &activation_key1("coop:test", "session-001"),
            Some("activation-race"),
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
// Key injectivity, cross-domain isolation, payload audit
// ============================================================================

#[test]
fn composite_key_is_injective_no_aliasing() {
    assert_ne!(
        activation_crossed_composite_key1("ab", "c"),
        activation_crossed_composite_key1("a", "bc"),
        "netstring length prefix must prevent domain/session aliasing"
    );

    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d_ab = GovernanceDomainId::new("ab");
    let d_a = GovernanceDomainId::new("a");
    open_session(&mgr, &d_ab, "c", &actor);
    let decision_hash = record_decision(&mgr, &d_ab, "c", "decision-001", &actor, [9u8; 32]);
    let g1 = record_pass_gate(&mgr, &d_ab, "c", ProcessGateKind::PrivacyReview, &actor);

    // Only ("ab","c") is opened; ("a","bc") must NOT see it through key
    // aliasing — the precondition fails closed for the unopened pair.
    let err = mgr
        .record_activation_crossed(
            &d_a,
            "bc",
            "activation-001",
            "decision-001",
            decision_hash,
            &[g1],
            &actor,
        )
        .expect_err("aliased pair must not inherit the opened session");
    assert!(err
        .to_string()
        .starts_with("activation_crossed_session_not_opened"));
    // The genuinely opened pair records fine.
    mgr.record_activation_crossed(
        &d_ab,
        "c",
        "activation-001",
        "decision-001",
        decision_hash,
        &[g1],
        &actor,
    )
    .expect("opened pair records");
}

#[test]
fn two_domains_sharing_session_id_never_mix() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    open_session(&mgr, &d1, "shared-session", &actor);
    open_session(&mgr, &d2, "shared-session", &actor);
    let h1 = record_decision(&mgr, &d1, "shared-session", "decision-1", &actor, [1u8; 32]);
    let h2 = record_decision(&mgr, &d2, "shared-session", "decision-2", &actor, [2u8; 32]);
    let g1 = record_pass_gate(
        &mgr,
        &d1,
        "shared-session",
        ProcessGateKind::PrivacyReview,
        &actor,
    );
    let g2 = record_pass_gate(
        &mgr,
        &d2,
        "shared-session",
        ProcessGateKind::PrivacyReview,
        &actor,
    );

    mgr.record_activation_crossed(
        &d1,
        "shared-session",
        "act-1",
        "decision-1",
        h1,
        &[g1],
        &actor,
    )
    .unwrap();
    mgr.record_activation_crossed(
        &d2,
        "shared-session",
        "act-2",
        "decision-2",
        h2,
        &[g2],
        &actor,
    )
    .unwrap();

    let list1 = mgr
        .list_activations_crossed_in_domain(&d1, "shared-session")
        .unwrap();
    let list2 = mgr
        .list_activations_crossed_in_domain(&d2, "shared-session")
        .unwrap();
    assert_eq!(list1.len(), 1);
    assert_eq!(list2.len(), 1);
    assert_eq!(list1[0].activation_id, "act-1");
    assert_eq!(list2[0].activation_id, "act-2");
    assert_eq!(list1[0].domain_id, "coop:one");
    assert_eq!(list2[0].domain_id, "coop:two");
}

#[test]
fn persisted_payload_carries_only_v1_fields() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    let decision_hash = record_decision(
        &mgr,
        &domain,
        "session-001",
        "decision-001",
        &actor,
        [9u8; 32],
    );
    let g1 = record_pass_gate(
        &mgr,
        &domain,
        "session-001",
        ProcessGateKind::PrivacyReview,
        &actor,
    );
    mgr.record_activation_crossed(
        &domain,
        "session-001",
        "activation-001",
        "decision-001",
        decision_hash,
        &[g1],
        &actor,
    )
    .unwrap();

    let payload = store
        .raw_payload(
            "activation_crossed",
            &activation_key1("coop:test", "session-001"),
            Some("activation-001"),
        )
        .expect("payload persisted");
    let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    let obj = value.as_object().unwrap();
    let expected: std::collections::BTreeSet<&str> = [
        "domain_id",
        "session_id",
        "activation_id",
        "decision_id",
        "decision_record_hash",
        "gate_basis",
        "crossed_by",
        "recorded_at",
        "record_hash",
    ]
    .into_iter()
    .collect();
    let actual: std::collections::BTreeSet<&str> = obj.keys().map(|k| k.as_str()).collect();
    assert_eq!(
        actual, expected,
        "persisted payload must carry exactly the v1 field set — no body, no \
         gate kind/result, no outcome/tally/vote/proposal/mandate field"
    );
}
