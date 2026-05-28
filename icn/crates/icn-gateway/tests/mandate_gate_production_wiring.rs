//! Regression test for PR #1927 / #1868 startup-guard wiring.
//!
//! Proves the gateway's daemon-backed production path can construct a real
//! `DefaultMandateGate` over the existing `ReceiptStore` and that the resulting
//! `GovernanceContext::validate()` accepts Production mode. Companion assertion
//! shows the guard still fails closed when `mandate_gate` is `None` — so this
//! test catches both regressions: forgetting to wire the gate, and silently
//! weakening the guard.
//!
//! The validate() pathway never calls `MandateGate::require()`; handler wiring
//! is out of scope for this PR (#1868 step 7). The point here is purely that
//! the field is populated with a real backend-backed gate, exactly as
//! `server.rs` does in the daemon-backed path.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gateway::ReceiptStore;
use icn_governance::{MembershipResolver, StaticMembershipResolver};
use icn_governance_actor::events::NoopEventEmitter;
use icn_governance_actor::http::{
    configure::{
        GovernanceContext, MemberStandingChecker, StewardStandingChecker, SuspensionChecker,
    },
    GovernanceContextBuildMode,
};
use icn_governance_actor::{DefaultMandateGate, GovernanceManager, MandateGate};

fn temp_db() -> sled::Db {
    sled::Config::new()
        .temporary(true)
        .open()
        .expect("sled temp open")
}

fn dummy_member_checker() -> MemberStandingChecker {
    Arc::new(|_did, _domain| Box::pin(async { true }))
}

fn dummy_steward_checker() -> StewardStandingChecker {
    Arc::new(|_did| Box::pin(async { true }))
}

fn dummy_suspension_checker() -> SuspensionChecker {
    Arc::new(|_did, _domain| Box::pin(async { false }))
}

fn dummy_membership_resolver() -> Arc<dyn MembershipResolver> {
    Arc::new(StaticMembershipResolver::new())
}

/// Build a `GovernanceContext` that mirrors the shape `server.rs` constructs
/// on the daemon-backed path: all standing/resolver deps wired, and the
/// `mandate_gate` derived from the gateway's real `ReceiptStore`.
fn production_like_ctx(
    receipt_store: Arc<ReceiptStore>,
    mandate_gate: Option<Arc<dyn MandateGate + Send + Sync>>,
) -> GovernanceContext<NoopEventEmitter> {
    // Touch `receipt_store` so the test fails to compile if the gateway ever
    // moves the field off the context shape this test depends on.
    let _: &ReceiptStore = &receipt_store;

    GovernanceContext {
        manager: Arc::new(GovernanceManager::new()),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: Some(dummy_member_checker()),
        steward_checker: Some(dummy_steward_checker()),
        suspension_checker: Some(dummy_suspension_checker()),
        membership_resolver: Some(dummy_membership_resolver()),
        sdis_service: None,
        mandate_gate,
        build_mode: GovernanceContextBuildMode::Production,
    }
}

#[test]
fn production_gateway_context_validates_when_mandate_gate_wired_from_receipt_store() {
    // Mirror the daemon-backed path in `server.rs`: build the `ReceiptStore`
    // first, then construct a `DefaultMandateGate` over it. The `Arc<ReceiptStore>`
    // coerces to `Arc<dyn GovernanceReceiptBackend>` at the call site because
    // `ReceiptStore` implements `GovernanceReceiptBackend`.
    let db = temp_db();
    let receipt_store = Arc::new(ReceiptStore::new(db));

    let mandate_gate: Arc<dyn MandateGate + Send + Sync> =
        Arc::new(DefaultMandateGate::new(receipt_store.clone()));

    let ctx = production_like_ctx(receipt_store, Some(mandate_gate));
    ctx.validate()
        .expect("Production gateway context with a real DefaultMandateGate must validate");
}

#[test]
fn production_gateway_context_still_rejects_when_mandate_gate_missing() {
    // The companion direction: even when every other production dep is wired,
    // a missing `mandate_gate` must fail closed. This is the guard the
    // PR-#1927 fix preserves; weakening validate() to make the wiring test
    // pass would silently retire this rejection. Keep both assertions in the
    // same file so any future weakening shows up as a test diff here.
    let db = temp_db();
    let receipt_store = Arc::new(ReceiptStore::new(db));

    let ctx = production_like_ctx(receipt_store, None);
    let err = ctx
        .validate()
        .expect_err("Production gateway context without mandate_gate must reject");
    assert_eq!(err.missing, vec!["mandate_gate"]);
    assert_eq!(err.mode, GovernanceContextBuildMode::Production);

    let msg = err.to_string();
    assert!(
        msg.contains("mandate_gate"),
        "error message must name the missing dep: {msg}"
    );
}
