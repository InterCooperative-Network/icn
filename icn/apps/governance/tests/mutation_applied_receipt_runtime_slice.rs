//! Runtime proof for the seventh `ProcessTransitionReceipt` class —
//! [`MutationAppliedReceipt`] — per the merged #2307 contract
//! (`docs/design/mutation-applied-receipt.md`) and the #2308 A1/A2/A3/A4
//! decision rung (`docs/design/mutation-applied-receipt-decision-rung.md`).
//!
//! Pins the merged decisions:
//!
//! 1. `GovernanceManager::record_mutation_applied` requires an already-opened
//!    session and a **recorded plan** to reference (A1), constructs a receipt
//!    with a real blake3 `record_hash`, persists it through the backend's
//!    atomic insert-if-absent BEFORE returning, and returns `Applied(receipt)`.
//! 2. A1: the application names the plan by both `plan_id` and
//!    `plan_record_hash`; the reference is **verified** — a missing,
//!    wrong-session, wrong-domain, or hash-mismatched plan fails closed and
//!    persists nothing. Activation + decision + gate basis are inherited
//!    transitively (not re-referenced).
//! 3. A2: `result_hash`-only; the persisted payload carries no applied-result
//!    body / plan body / operation list / target / effect payload.
//! 4. A3: a single caller-supplied `applied_at`, hashed but excluded from
//!    identity — a retry with identical stable identity returns the ORIGINAL
//!    receipt unchanged, never restamped.
//! 5. A4: recording an application persists only the receipt (no domain-state
//!    mutation); `applied_by` is recorder/apply-witness evidence, zero
//!    authority. A same `application_id` with a different plan reference,
//!    recorder, or result hash fails closed with the stable
//!    `mutation_applied_conflict` prefix; the original is untouched.
//! 6. Empty/whitespace ids rejected; missing receipt store is an error;
//!    backend failure fails closed; concurrent duplicates serialize to one
//!    winner; composite key injective; two domains sharing a `session_id`
//!    never mix.
//!
//! This records a **process fact and grants zero authority.** It is not a
//! mutation-application engine.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, ProcessGateKind, ProcessGateResult,
};
use icn_governance_actor::manager::{
    ActivationCrossedOutcome, DecisionRecordOutcome, GovernanceManager, MutationAppliedOutcome,
    MutationPlanRecordedOutcome, ProcessSessionOpenOutcome,
};
use icn_governance_actor::receipt_backend::{
    mutation_applied_composite_key1, GovernanceReceiptBackend,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Opaque-backed test store — the test analog of the production gateway-backed
// ReceiptStore: it implements the opaque primitives (atomic
// `put_opaque_if_absent` + append `put_opaque`) and NO typed overrides, so
// the trait's typed defaults are exercised end-to-end.
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

/// Backend that persists everything except the mutation-applied insert —
/// proves fail-closed application persistence with all preconditions satisfied
/// (the full plan chain is recorded through the same store first).
#[derive(Default)]
struct FailingMutationAppliedBackend {
    inner: OpaqueUniqueBackend,
}

impl GovernanceReceiptBackend for FailingMutationAppliedBackend {
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
        if class == "mutation_applied" {
            return Err("simulated mutation-applied backend failure".to_string());
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

fn app_key1(domain: &str, session: &str) -> String {
    mutation_applied_composite_key1(domain, session)
}

fn open_session(mgr: &GovernanceManager, domain: &GovernanceDomainId, session: &str, by: &Did) {
    match mgr
        .record_process_session_opened(domain, session, by)
        .unwrap()
    {
        ProcessSessionOpenOutcome::Opened(_) | ProcessSessionOpenOutcome::AlreadyOpened(_) => {}
    }
}

/// Record the full activation chain (session opened, decision, Pass gate,
/// activation crossing) and return the `ActivationCrossedReceipt.record_hash`.
/// Each call uses an `activation_id`-derived decision id so distinct
/// activations are distinct.
fn setup_activation(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    activation_id: &str,
    by: &Did,
) -> [u8; 32] {
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
    match mgr
        .record_activation_crossed(
            domain,
            session,
            activation_id,
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
    }
}

/// Record the full chain up to a recorded mutation plan (activation chain +
/// plan) and return the `MutationPlanRecordedReceipt.record_hash` — the A1
/// proof link a mutation application references. The activation id is derived
/// from `plan_id` so distinct plans reference distinct activations.
fn setup_plan(
    mgr: &GovernanceManager,
    domain: &GovernanceDomainId,
    session: &str,
    plan_id: &str,
    by: &Did,
) -> [u8; 32] {
    let activation_id = format!("activation-for-{plan_id}");
    let ah = setup_activation(mgr, domain, session, &activation_id, by);
    match mgr
        .record_mutation_plan_recorded(domain, session, plan_id, &activation_id, ah, [5u8; 32], by)
        .unwrap()
    {
        MutationPlanRecordedOutcome::Recorded(r)
        | MutationPlanRecordedOutcome::AlreadyRecorded(r) => r.record_hash,
    }
}

// ============================================================================
// Happy path — construct, persist, retrieve, verify A1 link
// ============================================================================

#[test]
fn application_persists_and_returns_receipt_with_verified_plan_link() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);

    let outcome = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &actor,
        )
        .expect("first application must succeed");
    let MutationAppliedOutcome::Applied(receipt) = outcome else {
        panic!("first application must be Applied, got {outcome:?}");
    };
    assert_eq!(receipt.domain_id, "coop:test");
    assert_eq!(receipt.session_id, "session-001");
    assert_eq!(receipt.application_id, "application-001");
    assert_eq!(receipt.plan_id, "plan-001");
    assert_eq!(receipt.applied_by, actor.to_string());
    assert_eq!(receipt.result_hash, [4u8; 32]);
    assert_ne!(receipt.record_hash, [0u8; 32], "real blake3 hash");
    // A1: plan_record_hash equals the persisted plan receipt hash.
    assert_eq!(
        receipt.plan_record_hash, ph,
        "A1 proof link binds the recorded plan's real record_hash"
    );
    assert_eq!(
        store.chain_len(
            "mutation_applied",
            &app_key1("coop:test", "session-001"),
            Some("application-001"),
        ),
        1,
        "exactly one persisted application"
    );
    let read = mgr
        .get_mutation_applied(&domain, "session-001", "application-001")
        .unwrap()
        .expect("point read must hydrate");
    assert_eq!(read, receipt);
}

#[test]
fn same_identity_retry_returns_original_never_restamped() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);

    let MutationAppliedOutcome::Applied(original) = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &actor,
        )
        .unwrap()
    else {
        panic!("first application must be Applied");
    };

    let outcome = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &actor,
        )
        .expect("same-identity retry must succeed");
    let MutationAppliedOutcome::AlreadyApplied(returned) = outcome else {
        panic!("retry must be AlreadyApplied, got {outcome:?}");
    };
    assert_eq!(returned.applied_at, original.applied_at, "never restamped");
    assert_eq!(returned.record_hash, original.record_hash);
    assert_eq!(returned, original);
    assert_eq!(
        store.chain_len(
            "mutation_applied",
            &app_key1("coop:test", "session-001"),
            Some("application-001"),
        ),
        1,
        "retry must not append a second record"
    );
}

// ============================================================================
// A1 — plan reference preconditions (verified, not asserted)
// ============================================================================

#[test]
fn unopened_session_fails_closed_and_creates_nothing() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    // No session opened, no plan.
    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-ghost",
            "application-001",
            "plan-001",
            [1u8; 32],
            [4u8; 32],
            &actor,
        )
        .expect_err("unopened session must fail closed");
    assert!(
        err.to_string()
            .starts_with("mutation_applied_session_not_opened"),
        "stable precondition prefix, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "mutation_applied",
            &app_key1("coop:test", "session-ghost"),
            Some("application-001"),
        ),
        0
    );
}

#[test]
fn missing_plan_fails_closed() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    open_session(&mgr, &domain, "session-001", &actor);
    // Session opened but no plan recorded.
    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-404",
            [1u8; 32],
            [4u8; 32],
            &actor,
        )
        .expect_err("missing plan must fail closed");
    assert!(
        err.to_string()
            .starts_with("mutation_applied_plan_not_found"),
        "stable prefix, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "mutation_applied",
            &app_key1("coop:test", "session-001"),
            Some("application-001"),
        ),
        0
    );
}

#[test]
fn wrong_session_plan_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    // Plan recorded in session-a; application attempted in session-b.
    let ph = setup_plan(&mgr, &domain, "session-a", "plan-001", &actor);
    open_session(&mgr, &domain, "session-b", &actor);
    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-b",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &actor,
        )
        .expect_err("wrong-session plan must fail closed");
    assert!(
        err.to_string()
            .starts_with("mutation_applied_plan_not_found"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn wrong_domain_plan_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d1 = GovernanceDomainId::new("coop:one");
    let d2 = GovernanceDomainId::new("coop:two");
    let ph = setup_plan(&mgr, &d1, "session-shared", "plan-001", &actor);
    open_session(&mgr, &d2, "session-shared", &actor);
    let err = mgr
        .record_mutation_applied(
            &d2,
            "session-shared",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &actor,
        )
        .expect_err("wrong-domain plan must fail closed");
    assert!(
        err.to_string()
            .starts_with("mutation_applied_plan_not_found"),
        "stable prefix, got: {err}"
    );
}

#[test]
fn plan_hash_mismatch_fails_closed() {
    // The supplied plan_id references a real plan, but the supplied
    // plan_record_hash does not match it.
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);
    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-001",
            [123u8; 32], // wrong hash for plan-001
            [4u8; 32],
            &actor,
        )
        .expect_err("plan hash mismatch must fail closed");
    assert!(
        err.to_string()
            .starts_with("mutation_applied_plan_mismatch"),
        "stable prefix, got: {err}"
    );
    assert_eq!(
        store.chain_len(
            "mutation_applied",
            &app_key1("coop:test", "session-001"),
            Some("application-001"),
        ),
        0
    );
}

// ============================================================================
// Conflicts — same application_id, different identity fields
// ============================================================================

#[test]
fn conflict_on_different_plan_reference_fails_closed() {
    // plan_id and plan_record_hash are both stable-identity inputs; through the
    // validated path they co-vary (a same-id/wrong-hash reference is caught
    // earlier as plan_mismatch). Referencing a DIFFERENT recorded plan for the
    // same application_id conflicts.
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let p1 = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);
    let p2 = setup_plan(&mgr, &domain, "session-001", "plan-002", &actor);

    mgr.record_mutation_applied(
        &domain,
        "session-001",
        "application-001",
        "plan-001",
        p1,
        [4u8; 32],
        &actor,
    )
    .expect("first application");

    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-002",
            p2,
            [4u8; 32],
            &actor,
        )
        .expect_err("different plan reference must conflict");
    assert!(
        err.to_string().starts_with("mutation_applied_conflict"),
        "stable conflict prefix, got: {err}"
    );
    let read = mgr
        .get_mutation_applied(&domain, "session-001", "application-001")
        .unwrap()
        .unwrap();
    assert_eq!(read.plan_id, "plan-001", "original untouched");
    assert_eq!(
        store.chain_len(
            "mutation_applied",
            &app_key1("coop:test", "session-001"),
            Some("application-001"),
        ),
        1
    );
}

#[test]
fn conflict_on_different_result_hash_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);

    mgr.record_mutation_applied(
        &domain,
        "session-001",
        "application-001",
        "plan-001",
        ph,
        [4u8; 32],
        &actor,
    )
    .expect("first application");

    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-001",
            ph,
            [10u8; 32],
            &actor,
        )
        .expect_err("different result hash must conflict");
    assert!(
        err.to_string().starts_with("mutation_applied_conflict"),
        "stable conflict prefix, got: {err}"
    );
}

#[test]
fn conflict_on_different_applier_fails_closed() {
    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let other = fresh_did();
    let domain = coop_test();
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);

    mgr.record_mutation_applied(
        &domain,
        "session-001",
        "application-001",
        "plan-001",
        ph,
        [4u8; 32],
        &actor,
    )
    .expect("first application");

    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &other,
        )
        .expect_err("different applier must conflict");
    assert!(
        err.to_string().starts_with("mutation_applied_conflict"),
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
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);

    for (d, s, a, p) in [
        ("", "session-001", "application-001", "plan-001"),
        ("   ", "session-001", "application-001", "plan-001"),
        ("coop:test", "", "application-001", "plan-001"),
        ("coop:test", "  \t", "application-001", "plan-001"),
        ("coop:test", "session-001", "", "plan-001"),
        ("coop:test", "session-001", " \n ", "plan-001"),
        ("coop:test", "session-001", "application-001", ""),
        ("coop:test", "session-001", "application-001", "  "),
    ] {
        let err = mgr
            .record_mutation_applied(&GovernanceDomainId::new(d), s, a, p, ph, [4u8; 32], &actor)
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
        .record_mutation_applied(
            &coop_test(),
            "session-001",
            "application-001",
            "plan-001",
            [1u8; 32],
            [4u8; 32],
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
    let store = Arc::new(FailingMutationAppliedBackend::default());
    let mgr = GovernanceManager::new()
        .with_receipt_store(store.clone() as Arc<dyn GovernanceReceiptBackend>);
    let actor = fresh_did();
    let domain = coop_test();
    // The full plan chain records fine (the backend only rejects the
    // mutation_applied class); the application insert then fails closed.
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);

    let err = mgr
        .record_mutation_applied(
            &domain,
            "session-001",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &actor,
        )
        .expect_err("backend failure must surface");
    assert!(
        err.to_string()
            .contains("simulated mutation-applied backend failure"),
        "backend error surfaced, got: {err}"
    );
    assert!(
        mgr.get_mutation_applied(&domain, "session-001", "application-001")
            .unwrap()
            .is_none(),
        "nothing half-written"
    );
}

// ============================================================================
// Concurrency — atomic uniqueness under racing duplicates
// ============================================================================

#[test]
fn concurrent_duplicate_records_serialize_to_one_application() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);

    let mgr = Arc::new(mgr);
    let mut handles = Vec::new();
    for _ in 0..8 {
        let mgr = mgr.clone();
        let actor = actor.clone();
        let domain = domain.clone();
        handles.push(std::thread::spawn(move || {
            mgr.record_mutation_applied(
                &domain,
                "session-001",
                "application-race",
                "plan-001",
                ph,
                [4u8; 32],
                &actor,
            )
        }));
    }
    let mut receipts = Vec::new();
    for h in handles {
        let outcome = h.join().unwrap().expect("same-identity race must succeed");
        match outcome {
            MutationAppliedOutcome::Applied(r) | MutationAppliedOutcome::AlreadyApplied(r) => {
                receipts.push(r)
            }
        }
    }
    assert_eq!(
        store.chain_len(
            "mutation_applied",
            &app_key1("coop:test", "session-001"),
            Some("application-race"),
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
        mutation_applied_composite_key1("ab", "c"),
        mutation_applied_composite_key1("a", "bc"),
        "netstring length prefix must prevent domain/session aliasing"
    );

    let (mgr, _store) = make_manager();
    let actor = fresh_did();
    let d_ab = GovernanceDomainId::new("ab");
    let d_a = GovernanceDomainId::new("a");
    let ph = setup_plan(&mgr, &d_ab, "c", "plan-001", &actor);
    // Only ("ab","c") is opened; ("a","bc") must NOT see it through key
    // aliasing — the precondition fails closed for the unopened pair.
    let err = mgr
        .record_mutation_applied(
            &d_a,
            "bc",
            "application-001",
            "plan-001",
            ph,
            [4u8; 32],
            &actor,
        )
        .expect_err("aliased pair must not inherit the opened session");
    assert!(err
        .to_string()
        .starts_with("mutation_applied_session_not_opened"));
    // The genuinely opened pair records fine.
    mgr.record_mutation_applied(
        &d_ab,
        "c",
        "application-001",
        "plan-001",
        ph,
        [4u8; 32],
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
    let p1 = setup_plan(&mgr, &d1, "shared-session", "plan-1", &actor);
    let p2 = setup_plan(&mgr, &d2, "shared-session", "plan-2", &actor);

    mgr.record_mutation_applied(
        &d1,
        "shared-session",
        "application-1",
        "plan-1",
        p1,
        [1u8; 32],
        &actor,
    )
    .unwrap();
    mgr.record_mutation_applied(
        &d2,
        "shared-session",
        "application-2",
        "plan-2",
        p2,
        [2u8; 32],
        &actor,
    )
    .unwrap();

    let list1 = mgr
        .list_mutation_applied_in_domain(&d1, "shared-session")
        .unwrap();
    let list2 = mgr
        .list_mutation_applied_in_domain(&d2, "shared-session")
        .unwrap();
    assert_eq!(list1.len(), 1);
    assert_eq!(list2.len(), 1);
    assert_eq!(list1[0].application_id, "application-1");
    assert_eq!(list2[0].application_id, "application-2");
    assert_eq!(list1[0].domain_id, "coop:one");
    assert_eq!(list2[0].domain_id, "coop:two");
}

#[test]
fn persisted_payload_carries_only_v1_fields_no_result_body() {
    let (mgr, store) = make_manager();
    let actor = fresh_did();
    let domain = coop_test();
    let ph = setup_plan(&mgr, &domain, "session-001", "plan-001", &actor);
    mgr.record_mutation_applied(
        &domain,
        "session-001",
        "application-001",
        "plan-001",
        ph,
        [4u8; 32],
        &actor,
    )
    .unwrap();

    let payload = store
        .raw_payload(
            "mutation_applied",
            &app_key1("coop:test", "session-001"),
            Some("application-001"),
        )
        .expect("payload persisted");
    let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    let obj = value.as_object().unwrap();
    let expected: std::collections::BTreeSet<&str> = [
        "domain_id",
        "session_id",
        "application_id",
        "plan_id",
        "plan_record_hash",
        "applied_by",
        "result_hash",
        "applied_at",
        "record_hash",
    ]
    .into_iter()
    .collect();
    let actual: std::collections::BTreeSet<&str> = obj.keys().map(|k| k.as_str()).collect();
    assert_eq!(
        actual, expected,
        "persisted payload must carry exactly the v1 field set — no applied-result body, \
         plan body, operation list, target list, or effect payload"
    );
}
