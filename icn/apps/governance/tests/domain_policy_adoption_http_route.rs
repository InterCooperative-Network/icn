//! HTTP route proof for gated, persisted `DomainPolicy` adoption — issue #2142.
//!
//! The adoption machinery already exists and is proven at the manager layer
//! (`domain_policy_adoption.rs` unit tests): the pure-core structural commit
//! (`icn-governance`), the app-side `DefaultMandateGate` authority resolution,
//! and the persisted `GovernanceManager::adopt_domain_policy_persisted`
//! (`load → gate → adopt → save`, #2170). This test pins the missing surface:
//! **one governance HTTP route that drives that persisted, gated path end to
//! end**:
//!
//!   `POST /gov/domains/{domain_id}/domain-policy/adopt`
//!     → governance:write scope + domain-membership gate
//!     → GovernanceManager::adopt_domain_policy_persisted
//!       → load InstitutionalDomain → DefaultMandateGate → pure-core adopt
//!       → save InstitutionalDomain
//!     → adopted DomainPolicyRef projection ({policy_id, domain_id})
//!
//! It adds NO declare/create route (declaring a governed domain is not
//! mandate-gated yet), no CCL runtime, no policy registry, and no auth change.
//!
//! Pins:
//!
//! 1. The route is mounted and reachable (not 404) and succeeds with an
//!    active, domain-scoped adopt grant.
//! 2. The adoption persists `current_policy` and survives a reload from the
//!    `InstitutionalDomain` store.
//! 3. The route requires the existing `governance:write` write guard.
//! 4. A non-member is rejected by the existing domain-membership gate (403).
//! 5. An undeclared `InstitutionalDomain` returns 404 (not found).
//! 6. The wrong actor is rejected through the `DefaultMandateGate` (403),
//!    leaving state unchanged.
//! 7. A revoked backing mandate is rejected through the gate (403).
//! 8. An empty `policy_content` is a 400.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{body::to_bytes, http::StatusCode, test, App};
use icn_governance::{
    AuthorityClass, AuthorityGrant, AuthorityGrantId, BootstrapEntityType, DecisionProvenance,
    DomainPolicy, GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, Grantee,
    GrantorEntityId, Mandate, MandateStatus, MembershipConfig, MembershipSource, Timestamp,
    TypedScope,
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
// Gate-serving receipt backend: serves the grants + mandates the
// DefaultMandateGate reads. Mirrors `domain_policy_adoption::tests::TestBackend`.
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
/// `domain_policy:adopt` act token, provenance-linked to `decision()`.
///
/// `valid_until: None` so the grant is active at wall-clock `now` (the handler
/// resolves authority at `current_time_secs()`, not a fixed test timestamp).
fn adopt_grant(actor: Did, dom: &str, grant_id: AuthorityGrantId) -> AuthorityGrant {
    AuthorityGrant {
        id: grant_id,
        class: AuthorityClass::Execution,
        grantor: GrantorEntityId("coop:alpha".into()),
        grantee: Grantee::Person(actor),
        scope: TypedScope {
            domain: Some(GovernanceDomainId::new(dom)),
            action_kind: vec!["domain_policy:adopt".into()],
            ..TypedScope::default()
        },
        granted_by: Some(decision()),
        valid_from: 0,
        valid_until: None,
        revoked_at: None,
    }
}

/// A live mandate backing `grant_id` (no deadline → live at any time).
fn backing_mandate(grant_id: AuthorityGrantId) -> Mandate {
    Mandate::new(decision(), [9u8; 32], vec![grant_id], None, None, 0)
        .expect("grant-bearing mandate is valid")
}

struct Harness {
    ctx: GovernanceContext<NoopEventEmitter>,
    store: Arc<dyn GovernanceStateStore>,
}

/// Build a manager wired with the gate backend + a temp Sled
/// `InstitutionalDomain` store, then return a `Test`-mode context.
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

/// Seed the in-memory `GovernanceDomain` (for the membership gate) with `member`
/// as a static-list member.
async fn seed_governance_domain(mgr: &GovernanceManager, member: &Did, domain_id: &str) {
    mgr.create_domain(
        GovernanceDomainId::new(domain_id),
        "Test Coop".to_string(),
        "default".to_string(),
        GovernanceParams {
            quorum_percentage: 1,
            approval_threshold_percentage: 51,
            voting_period_seconds: 86_400,
            require_deliberation: false,
            ..GovernanceParams::default()
        },
        MembershipConfig {
            source: MembershipSource::StaticList(vec![member.clone()]),
        },
    )
    .await
    .expect("create_domain");
}

/// Build an actix app for the governance routes, injecting `BasicClaims` for
/// `caller` with `scope`.
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

fn adopt_uri(domain_id: &str) -> String {
    format!("/domains/{domain_id}/domain-policy/adopt")
}

// ============================================================================
// Tests
// ============================================================================

#[actix_web::test]
async fn adopt_route_persists_current_policy_and_survives_reload() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));

    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare InstitutionalDomain");

    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "POST adopt must mount and succeed; body: {}",
        String::from_utf8_lossy(&bytes)
    );

    // The response is the adopted policy ref projection, content-addressed
    // server-side from the request content + bound to the path domain.
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json body");
    let expected = DomainPolicy::new(GovernanceDomainId::new("coop-alpha"), b"policy v1");
    assert_eq!(body["domain_id"], "coop-alpha");
    assert_eq!(body["policy_id"], expected.id.to_hex());

    // current_policy survives reload from the InstitutionalDomain store.
    let reloaded = h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .expect("load")
        .expect("persisted");
    assert_eq!(reloaded.current_policy(), Some(&expected.policy_ref()));
}

#[actix_web::test]
async fn adopt_route_stores_policy_body_and_resolves() {
    // #2180: adoption now persists the raw policy_content body by DomainPolicyId,
    // so an adopted ref can be resolved back to the bytes that produced it.
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1 body" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK, "adopt must succeed");

    let expected = DomainPolicy::new(GovernanceDomainId::new("coop-alpha"), b"policy v1 body");

    // The body is persisted under the adopted DomainPolicyId, retrievable
    // directly from the store...
    assert_eq!(
        h.store
            .get_domain_policy_body(&expected.id)
            .expect("get body"),
        Some(b"policy v1 body".to_vec()),
        "adoption must persist the policy_content body by DomainPolicyId"
    );
    // ...and via the manager-level resolve path.
    assert_eq!(
        h.ctx
            .manager
            .get_domain_policy_body(&expected.id)
            .expect("resolve body"),
        Some(b"policy v1 body".to_vec()),
        "the adopted ref must resolve to its body via the manager"
    );
}

#[actix_web::test]
async fn adopt_route_oversize_does_not_store_body() {
    use icn_governance_actor::http::validation::MAX_DOMAIN_POLICY_CONTENT_BYTES;
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let oversize = "a".repeat(MAX_DOMAIN_POLICY_CONTENT_BYTES + 1);
    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": oversize }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "oversize must be 400"
    );

    let rejected = DomainPolicy::new(GovernanceDomainId::new("coop-alpha"), oversize.as_bytes());
    assert!(
        h.store
            .get_domain_policy_body(&rejected.id)
            .expect("get body")
            .is_none(),
        "a rejected oversize body must not be persisted"
    );
}

#[actix_web::test]
async fn adopt_route_empty_does_not_store_body() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "   " }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "empty must be 400");

    let rejected = DomainPolicy::new(GovernanceDomainId::new("coop-alpha"), b"   ");
    assert!(
        h.store
            .get_domain_policy_body(&rejected.id)
            .expect("get body")
            .is_none(),
        "a rejected empty/whitespace body must not be persisted"
    );
}

#[actix_web::test]
async fn adopt_route_unauthorized_does_not_store_body() {
    // An actor with no authorizing grant is rejected by the gate (403); the
    // body must not be persisted, mirroring the unchanged current_policy.
    let member_a = fresh_did();
    let member_b = fresh_did();
    assert_ne!(member_a, member_b);
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(member_a.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    h.ctx
        .manager
        .create_domain(
            GovernanceDomainId::new("coop-alpha"),
            "Test Coop".to_string(),
            "default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![member_a.clone(), member_b.clone()]),
            },
        )
        .await
        .expect("create_domain");
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let app = gov_app!(
        h.ctx.clone(),
        &member_b,
        Some("governance:write".to_string())
    );
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1 body" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "no grant -> 403");

    let rejected = DomainPolicy::new(GovernanceDomainId::new("coop-alpha"), b"policy v1 body");
    assert!(
        h.store
            .get_domain_policy_body(&rejected.id)
            .expect("get body")
            .is_none(),
        "a gate-rejected adoption must not persist the body"
    );
}

#[actix_web::test]
async fn adopt_route_requires_governance_write_scope() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    // Token carries only the read scope — the write guard must reject it.
    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:read".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "missing governance:write scope must be rejected"
    );

    // Nothing was adopted.
    let reloaded = h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .unwrap();
    assert!(reloaded.current_policy().is_none());
}

#[actix_web::test]
async fn adopt_route_rejects_non_member() {
    let member = fresh_did();
    let outsider = fresh_did();
    assert_ne!(member, outsider);
    // The outsider even *holds* an adopt grant, but is not a domain member: the
    // coarse membership gate must still reject before the adopt seam runs.
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(outsider.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &member, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let app = gov_app!(
        h.ctx.clone(),
        &outsider,
        Some("governance:write".to_string())
    );
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a non-member must be rejected by the membership gate"
    );
    let reloaded = h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .unwrap();
    assert!(reloaded.current_policy().is_none());
}

#[actix_web::test]
async fn adopt_route_undeclared_institutional_domain_is_not_found() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    // GovernanceDomain exists (membership passes), but the InstitutionalDomain
    // is never declared.
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;

    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "adoption on an undeclared InstitutionalDomain must be 404"
    );
}

#[actix_web::test]
async fn adopt_route_rejects_wrong_actor_via_gate() {
    // The grant is for the domain member, but a *different* member is the
    // authenticated caller and holds no grant of their own.
    let member_a = fresh_did();
    let member_b = fresh_did();
    assert_ne!(member_a, member_b);
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(member_a.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    // Both are members so the membership gate passes; the gate must still
    // reject member_b for lack of an authorizing grant.
    h.ctx
        .manager
        .create_domain(
            GovernanceDomainId::new("coop-alpha"),
            "Test Coop".to_string(),
            "default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(vec![member_a.clone(), member_b.clone()]),
            },
        )
        .await
        .expect("create_domain");
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let app = gov_app!(
        h.ctx.clone(),
        &member_b,
        Some("governance:write".to_string())
    );
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "an actor with no authorizing grant must be rejected by the gate"
    );
    let reloaded = h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .unwrap();
    assert!(
        reloaded.current_policy().is_none(),
        "state must be unchanged on a gate rejection"
    );
}

#[actix_web::test]
async fn adopt_route_rejects_revoked_authority_via_gate() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mut mandate = backing_mandate(gid);
    mandate.status = MandateStatus::Revoked; // grant active, mandate dead
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "policy v1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a revoked backing mandate must be rejected by the gate"
    );
    let reloaded = h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .unwrap();
    assert!(reloaded.current_policy().is_none());
}

#[actix_web::test]
async fn adopt_route_rejects_empty_policy_content() {
    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": "   " }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "an empty/whitespace policy_content must be a 400"
    );
}

#[actix_web::test]
async fn adopt_route_rejects_oversize_policy_content() {
    // The handler content-addresses (blake3-hashes) the whole body, so an
    // unbounded payload is a DoS vector. A body over the configured cap must be
    // rejected with 400 before any hashing/persistence, and nothing adopted.
    use icn_governance_actor::http::validation::MAX_DOMAIN_POLICY_CONTENT_BYTES;

    let caller = fresh_did();
    let gid = AuthorityGrantId::new();
    let grant = adopt_grant(caller.clone(), "coop-alpha", gid.clone());
    let mandate = backing_mandate(gid);
    let h = make_harness(TestBackend::new(vec![mandate], vec![grant]));
    seed_governance_domain(&h.ctx.manager, &caller, "coop-alpha").await;
    h.ctx
        .manager
        .declare_institutional_domain(
            GovernanceDomainId::new("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .expect("declare");

    let oversize = "a".repeat(MAX_DOMAIN_POLICY_CONTENT_BYTES + 1);
    let app = gov_app!(h.ctx.clone(), &caller, Some("governance:write".to_string()));
    let req = test::TestRequest::post()
        .uri(&adopt_uri("coop-alpha"))
        .set_json(serde_json::json!({ "policy_content": oversize }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a policy_content over the byte cap must be a 400"
    );
    let reloaded = h
        .store
        .get_institutional_domain(&GovernanceDomainId::new("coop-alpha"))
        .unwrap()
        .unwrap();
    assert!(
        reloaded.current_policy().is_none(),
        "nothing may be adopted when the body is rejected"
    );
}
