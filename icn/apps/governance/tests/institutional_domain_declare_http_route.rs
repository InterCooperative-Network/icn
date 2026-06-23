//! HTTP route proof for gated `InstitutionalDomain` declaration — issue #2142.
//!
//! The gated declaration seam already exists and is proven at the manager layer
//! (`domain_policy_adoption.rs` unit tests): `MandateAct::DeclareInstitutionalDomain`,
//! `GovernanceManager::declare_institutional_domain_gated` (`gate → persist`,
//! gate-before-write), and the ungated bootstrap `declare_institutional_domain`.
//! This test pins the missing surface: **one governance HTTP route that drives
//! the gated seam end-to-end**:
//!
//!   `POST /gov/domains/{domain_id}/institutional-domain/declare`
//!     → governance:write scope
//!     → GovernanceManager::declare_institutional_domain_gated
//!       → DefaultMandateGate (domain-scoped `institutional_domain:declare` grant)
//!       → persist InstitutionalDomain (only after the gate passes)
//!     → declared-domain projection
//!
//! It adds NO new authority primitive and NEVER calls the ungated bootstrap
//! `declare_institutional_domain` seam.
//!
//! Pins:
//!
//! 1. The route is mounted and succeeds with an active domain-scoped declare grant.
//! 2. The declared `InstitutionalDomain` persists and reloads from the store.
//! 3. The route requires the existing `governance:write` write guard.
//! 4. The acting DID is the authenticated token subject (the grant is bound to
//!    that DID; a request with a different token subject is refused).
//! 5. A wrong actor is rejected via the gate (403) and persists nothing.
//! 6. A wrong-domain grant is rejected via the gate (403).
//! 7. A revoked backing mandate is rejected via the gate (403).
//! 8. Duplicate declaration (by the authorized actor) returns 409.
//! 9. **Gate-before-write / no existence leak**: an unauthorized actor declaring
//!    an already-declared domain gets 403 (the gate refuses), NOT 409.
//!    This is also the proof the route does not bypass the gate into the
//!    ungated seam — an ungated path would have persisted/succeeded.
//! 10. A malformed `charter_id` (non-hex / wrong length) is a 400.
//! 11. An out-of-taxonomy `entity_type` is a 400.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{body::to_bytes, http::StatusCode, test, App};
use icn_governance::{
    AuthorityClass, AuthorityGrant, AuthorityGrantId, BootstrapEntityType, DecisionProvenance,
    GovernanceDecisionReceipt, GovernanceDomainId, Grantee, GrantorEntityId, Mandate,
    MandateStatus, Timestamp, TypedScope,
};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    receipt_backend::GovernanceReceiptBackend,
    GovernanceStateStore, NoopEventEmitter, SledGovernanceStateStore,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};

// ============================================================================
// Gate-serving receipt backend (mirrors the adoption route test).
// ============================================================================

struct TestBackend {
    mandates: Vec<Mandate>,
    grants: Vec<AuthorityGrant>,
}

impl TestBackend {
    fn new(mandates: Vec<Mandate>, grants: Vec<AuthorityGrant>) -> Arc<Self> {
        Arc::new(Self { mandates, grants })
    }
}

impl GovernanceReceiptBackend for TestBackend {
    fn put_governance(&self, _: &GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn list_mandates_by_decision(&self, decision_hash: &Hash) -> Result<Vec<Mandate>, String> {
        Ok(self
            .mandates
            .iter()
            .filter(|m| &m.decision.decision_hash == decision_hash)
            .cloned()
            .collect())
    }
    fn get_authority_grant(
        &self,
        grant_id: &AuthorityGrantId,
    ) -> Result<Option<AuthorityGrant>, String> {
        Ok(self.grants.iter().find(|g| &g.id == grant_id).cloned())
    }
    fn list_active_authority_grants_by_grantee(
        &self,
        grantee: &Grantee,
        now: Timestamp,
    ) -> Result<Vec<AuthorityGrant>, String> {
        Ok(self
            .grants
            .iter()
            .filter(|g| &g.grantee == grantee && g.is_active_at(now))
            .cloned()
            .collect())
    }
}

// ============================================================================
// Fixtures
// ============================================================================

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

fn decision() -> DecisionProvenance {
    DecisionProvenance {
        proposal_id: "prop-1".into(),
        decision_hash: [7u8; 32],
    }
}

/// An Execution grant for `actor`, bound to `dom`, carrying the
/// `institutional_domain:declare` act token. `valid_until: None` so it is active
/// at wall-clock `now` (the handler resolves authority at `current_time_secs()`).
fn declare_grant(actor: Did, dom: &str, grant_id: AuthorityGrantId) -> AuthorityGrant {
    AuthorityGrant {
        id: grant_id,
        class: AuthorityClass::Execution,
        grantor: GrantorEntityId("coop:alpha".into()),
        grantee: Grantee::Person(actor),
        scope: TypedScope {
            domain: Some(GovernanceDomainId::new(dom)),
            action_kind: vec!["institutional_domain:declare".into()],
            ..TypedScope::default()
        },
        granted_by: Some(decision()),
        valid_from: 0,
        valid_until: None,
        revoked_at: None,
    }
}

fn backing_mandate(grant_id: AuthorityGrantId) -> Mandate {
    Mandate::new(decision(), [9u8; 32], vec![grant_id], None, None, 0)
        .expect("grant-bearing mandate is valid")
}

struct Harness {
    ctx: GovernanceContext<NoopEventEmitter>,
    store: Arc<dyn GovernanceStateStore>,
}

/// Manager wired with the gate backend + a temp Sled `InstitutionalDomain`
/// store, in `Test`-mode context. No `GovernanceDomain` is seeded: the declare
/// route does not apply the membership gate (see the handler docs).
fn make_harness(backend: Arc<TestBackend>) -> Harness {
    let store: Arc<dyn GovernanceStateStore> = Arc::new(SledGovernanceStateStore::new(Arc::new(
        icn_store::SledStore::temporary().expect("temp sled"),
    )));
    let manager = GovernanceManager::new()
        .with_receipt_store(backend as Arc<dyn GovernanceReceiptBackend>)
        .with_domain_store(store.clone());
    let ctx = GovernanceContext {
        manager: Arc::new(manager),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: None,
        membership_resolver: None,
        sdis_service: None,
        mandate_gate: None,
        build_mode: http::GovernanceContextBuildMode::Test,
    };
    Harness { ctx, store }
}

macro_rules! gov_app {
    ($ctx:expr, $caller:expr, $scope:expr) => {{
        use actix_web::dev::Service as _;
        use actix_web::HttpMessage as _;
        let caller = $caller.to_string();
        let scope: Option<String> = $scope;
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    req.extensions_mut().insert(BasicClaims {
                        sub: caller.clone(),
                        scope: scope.clone(),
                    });
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

fn declare_uri(domain_id: &str) -> String {
    format!("/domains/{domain_id}/institutional-domain/declare")
}

fn write_scope() -> Option<String> {
    Some("governance:write".to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[actix_web::test]
async fn declare_route_succeeds_persists_and_reloads() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "cooperative" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "POST declare must mount and succeed; body: {}",
        String::from_utf8_lossy(&bytes)
    );

    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json body");
    assert_eq!(body["domain_id"], "coop-alpha");
    assert_eq!(body["owning_entity_class"], "cooperative");
    assert!(body["charter_id"].is_null());
    assert!(body["current_policy_id"].is_null());

    // Persisted and reloadable, declared-but-unbound.
    let loaded = h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .expect("load")
        .expect("persisted");
    assert_eq!(loaded.domain_id, GovernanceDomainId::new("coop-alpha"));
    assert_eq!(loaded.owning_entity_class, BootstrapEntityType::Cooperative);
    assert!(loaded.current_policy().is_none());
}

#[actix_web::test]
async fn declare_route_requires_governance_write_scope() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    // Read scope only — the write guard must reject.
    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:read".to_string()));
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "cooperative" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .is_none());
}

#[actix_web::test]
async fn declare_route_actor_is_token_subject_not_body() {
    // The grant is bound to `granted_did`. A request authenticated as a
    // DIFFERENT token subject must be refused — proving the acting DID is the
    // token subject (there is no actor body field to override it).
    let granted_did = fresh_did();
    let other_did = fresh_did();
    assert_ne!(granted_did, other_did);
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(granted_did, "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let app = gov_app!(h.ctx.clone(), &other_did, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "cooperative" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "the acting DID is the token subject; a non-grantee subject must be refused"
    );
    assert!(h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .is_none());
}

#[actix_web::test]
async fn declare_route_rejects_wrong_domain_via_gate() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    // Grant scoped to coop-beta; declaring coop-alpha.
    let grant = declare_grant(caller.clone(), "coop-beta", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "cooperative" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .is_none());
}

#[actix_web::test]
async fn declare_route_rejects_revoked_authority_via_gate() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mut mandate = backing_mandate(gid);
    mandate.status = MandateStatus::Revoked;
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "cooperative" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .is_none());
}

#[actix_web::test]
async fn declare_route_duplicate_returns_conflict() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let mk = || {
        test::TestRequest::post()
            .uri(&declare_uri("coop-alpha"))
            .set_json(serde_json::json!({ "entity_type": "cooperative" }))
            .to_request()
    };
    let first = test::call_service(&app, mk()).await;
    assert_eq!(first.status(), StatusCode::OK);
    let second = test::call_service(&app, mk()).await;
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "a second declaration by the authorized actor must be a 409"
    );
}

#[actix_web::test]
async fn declare_route_unauthorized_on_existing_is_forbidden_not_conflict() {
    // Gate-before-write / no existence leak: declare once with the authorized
    // actor, then a DIFFERENT actor (no grant) declares the same domain. The
    // gate refuses first → 403, NOT 409 — so existence is not leaked, and the
    // route is provably not bypassing the gate into the ungated seam.
    let authorized = fresh_did();
    let outsider = fresh_did();
    assert_ne!(authorized, outsider);
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(authorized.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let app_ok = gov_app!(h.ctx.clone(), &authorized, write_scope());
    let first = test::call_service(
        &app_ok,
        test::TestRequest::post()
            .uri(&declare_uri("coop-alpha"))
            .set_json(serde_json::json!({ "entity_type": "cooperative" }))
            .to_request(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let app_outsider = gov_app!(h.ctx.clone(), &outsider, write_scope());
    let second = test::call_service(
        &app_outsider,
        test::TestRequest::post()
            .uri(&declare_uri("coop-alpha"))
            .set_json(serde_json::json!({ "entity_type": "cooperative" }))
            .to_request(),
    )
    .await;
    assert_eq!(
        second.status(),
        StatusCode::FORBIDDEN,
        "unauthorized declare on an existing domain must be 403 (gate first), not 409"
    );
}

#[actix_web::test]
async fn declare_route_rejects_non_hex_charter_id() {
    // Correct length (64 chars) but non-hex content → rejected by hex decoding,
    // after the length check, with a 400.
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let non_hex = "z".repeat(64); // 64 chars, but 'z' is not a hex digit
    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "cooperative", "charter_id": non_hex }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a non-hex charter_id must be a 400"
    );
    assert!(h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .is_none());
}

#[actix_web::test]
async fn declare_route_rejects_wrong_length_charter_id() {
    // Valid hex but the wrong length → rejected by the length check BEFORE any
    // hex decoding (an oversize body is never decoded), with a 400.
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let short_hex = "ab".repeat(20); // 40 hex chars = 20 bytes, not 32
    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "cooperative", "charter_id": short_hex }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a wrong-length charter_id must be a 400"
    );
    assert!(h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .is_none());
}

#[actix_web::test]
async fn declare_route_rejects_unknown_entity_type() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "not_a_real_entity" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "an out-of-taxonomy entity_type must be a 400, not coerced"
    );
}

#[actix_web::test]
async fn declare_route_accepts_valid_charter_id() {
    // A well-formed 64-hex charter_id is parsed and echoed back as the stored
    // charter_ref (the seam stores the reference; no charter-existence check).
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = declare_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    let charter_hex = "ab".repeat(32); // 64 hex chars = 32 bytes
    let app = gov_app!(h.ctx.clone(), &caller, write_scope());
    let req = test::TestRequest::post()
        .uri(&declare_uri("coop-alpha"))
        .set_json(serde_json::json!({ "entity_type": "community", "charter_id": charter_hex }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "a valid charter_id must be accepted; body: {}",
        String::from_utf8_lossy(&bytes)
    );
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["owning_entity_class"], "community");
    assert_eq!(body["charter_id"], charter_hex);
}
