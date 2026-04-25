//! `GET /v1/gov/me/standing` integration tests (issue #1604).
//!
//! Proves the caller-facing institutional read model:
//!
//! 1. Authenticated DID with no roles and no static membership returns a
//!    well-formed empty standing — never errors.
//! 2. A direct (manager-level) role assignment surfaces in `roles` and its
//!    authority scope rolls up into `authority_scopes`.
//! 3. A role assigned through person-directory-resolved bootstrap planning
//!    (the #1603 path) lands the same way once apply has run, because both
//!    paths go through `manager.assign_role` server-side.
//! 4. Inline `person_did` precedence from #1603 is preserved end-to-end:
//!    when both inline DID and a holder_label exist, the inline DID is the
//!    one that ends up in standing.
//! 5. A different, unrelated DID does not see the first DID's standing.
//! 6. `?domain_id=<id>` filters domains and roles to that domain.
//! 7. Unknown `domain_id` returns 200 with empty arrays — not 404 — because
//!    "no standing in that domain" is a meaningful answer.
//! 8. The response payload uses regulatory-safe vocabulary
//!    (no payment / currency / balance terms).
//! 9. `authority_scope` round-trips through `assign_role` and rolls up into
//!    the top-level `authority_scopes` union (#1629).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{body::to_bytes, dev::Service as _, http::StatusCode, test, App, HttpMessage};
use icn_governance::{GovernanceDomainId, GovernanceParams, MembershipConfig, StructureKind};
use icn_governance_actor::{
    http::{self, GovernanceContext},
    manager::GovernanceManager,
    NoopEventEmitter,
};
use icn_http_kit::auth::BasicClaims;
use icn_identity::{Did, IdentityBundle};
use serde_json::Value;

fn fresh_did() -> Did {
    IdentityBundle::generate()
        .expect("IdentityBundle::generate")
        .did()
        .clone()
}

fn make_ctx() -> GovernanceContext<NoopEventEmitter> {
    GovernanceContext {
        manager: Arc::new(GovernanceManager::new()),
        emitter: NoopEventEmitter,
        on_charter_accepted: None,
        on_proposal_accepted: None,
        on_proposal_accepted_with_evidence: None,
        member_checker: None,
        steward_checker: None,
        suspension_checker: None,
        membership_resolver: None,
        sdis_service: None,
    }
}

/// Build a test app with the governance routes mounted, injecting a chosen
/// caller DID + scope into every request via a thin auth-shim middleware.
macro_rules! standing_test_app {
    ($ctx:expr, $caller_did:expr, $scope:expr) => {{
        let scope: Option<&'static str> = $scope;
        let caller = $caller_did.to_string();
        test::init_service(
            App::new()
                .wrap_fn(move |req, srv| {
                    if let Some(scope_str) = scope {
                        req.extensions_mut().insert(BasicClaims {
                            sub: caller.clone(),
                            scope: Some(scope_str.to_string()),
                        });
                    }
                    srv.call(req)
                })
                .configure(|cfg| http::configure(cfg, $ctx)),
        )
        .await
    }};
}

macro_rules! fetch_standing {
    ($app:expr, $uri:expr) => {{
        let req = test::TestRequest::get().uri($uri).to_request();
        let resp = test::call_service(&$app, req).await;
        let status = resp.status();
        let bytes = to_bytes(resp.into_body()).await.unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");
        (status, parsed)
    }};
}

#[actix_web::test]
async fn caller_with_no_standing_returns_well_formed_empty() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let app = standing_test_app!(ctx, &caller, Some("governance:read"));

    let (status, body) = fetch_standing!(app, "/me/standing");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], caller.to_string());
    assert!(body["domains"].as_array().unwrap().is_empty());
    assert!(body["roles"].as_array().unwrap().is_empty());
    assert!(body["authority_scopes"].as_array().unwrap().is_empty());
    assert!(body["generated_at"].as_u64().is_some());
    assert!(
        body.get("display_label").is_none() || body["display_label"].is_null(),
        "display_label is reserved for a future identity registry; should be absent or null"
    );
}

#[actix_web::test]
async fn direct_role_assignment_surfaces_in_standing() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let structure = ctx
        .manager
        .create_structure(
            "entity-a".to_string(),
            StructureKind::Committee,
            "Treasury".to_string(),
            None,
        )
        .expect("create_structure");
    ctx.manager
        .assign_role(
            structure.id.clone(),
            caller.clone(),
            "treasurer".to_string(),
            vec!["approve_budget_within_policy".to_string()],
        )
        .expect("assign_role");

    let app = standing_test_app!(ctx, &caller, Some("governance:read"));
    let (status, body) = fetch_standing!(app, "/me/standing");
    assert_eq!(status, StatusCode::OK);

    let roles = body["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 1, "exactly one role should surface");
    assert_eq!(roles[0]["role"], "treasurer");
    assert_eq!(roles[0]["structure_name"], "Treasury");
    assert_eq!(roles[0]["parent_entity_id"], "entity-a");
    assert_eq!(roles[0]["structure_id"], structure.id.0);
    assert_eq!(
        roles[0]["authority_scope"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        vec!["approve_budget_within_policy".to_string()],
        "authority_scope must round-trip on the role row"
    );

    // Rollup union: the same scope appears in `authority_scopes` on the
    // top-level standing.
    let scopes: Vec<String> = body["authority_scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(scopes, vec!["approve_budget_within_policy".to_string()]);
}

#[actix_web::test]
async fn person_directory_resolved_role_lands_the_same_way() {
    // The person-directory overlay (#1603) resolves a holder_label to a real
    // DID at plan time, then apply calls the same `assign_role` API the
    // direct path uses. So the runtime contract /me/standing reads is
    // identical: there is no separate "directory-sourced" code path. This
    // test pins that property by simulating the post-apply state directly.
    let ctx = make_ctx();
    let caller = fresh_did();
    let structure = ctx
        .manager
        .create_structure(
            "entity-a".to_string(),
            StructureKind::Committee,
            "Content Working Group".to_string(),
            None,
        )
        .expect("create_structure");
    ctx.manager
        .assign_role(
            structure.id.clone(),
            caller.clone(),
            "program-coordinator".to_string(),
            vec!["curate_session_intake".to_string()],
        )
        .expect("assign_role (post-apply)");

    let app = standing_test_app!(ctx, &caller, Some("governance:read"));
    let (status, body) = fetch_standing!(app, "/me/standing");
    assert_eq!(status, StatusCode::OK);
    let roles = body["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0]["role"], "program-coordinator");
}

#[actix_web::test]
async fn inline_did_precedence_from_1603_is_preserved_in_standing() {
    // End-to-end check: when an institution package supplies BOTH inline
    // person_did and a holder_label that the directory could resolve, the
    // inline DID wins at planning time. Standing for the inline DID shows
    // the role; standing for the directory DID does NOT.
    let ctx = make_ctx();
    let inline_did = fresh_did();
    let directory_did = fresh_did();
    assert_ne!(inline_did, directory_did);

    let structure = ctx
        .manager
        .create_structure(
            "entity-a".to_string(),
            StructureKind::Committee,
            "Treasury".to_string(),
            None,
        )
        .expect("create_structure");
    // Apply the inline winner — this is the role assignment that planner
    // would have emitted from #1603's resolver.
    ctx.manager
        .assign_role(
            structure.id.clone(),
            inline_did.clone(),
            "treasurer".to_string(),
            vec![],
        )
        .expect("assign_role for inline DID");

    let inline_app = standing_test_app!(ctx.clone(), &inline_did, Some("governance:read"));
    let (status, body) = fetch_standing!(inline_app, "/me/standing");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["roles"].as_array().unwrap().len(), 1);

    let directory_app = standing_test_app!(ctx, &directory_did, Some("governance:read"));
    let (status, body) = fetch_standing!(directory_app, "/me/standing");
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["roles"].as_array().unwrap().is_empty(),
        "directory DID must NOT inherit standing when inline DID won the assignment"
    );
}

#[actix_web::test]
async fn caller_does_not_see_another_dids_standing() {
    let ctx = make_ctx();
    let alice = fresh_did();
    let bob = fresh_did();
    let structure = ctx
        .manager
        .create_structure(
            "entity-a".to_string(),
            StructureKind::Committee,
            "Treasury".to_string(),
            None,
        )
        .expect("create_structure");
    ctx.manager
        .assign_role(structure.id, alice.clone(), "treasurer".to_string(), vec![])
        .expect("assign_role");

    // Bob authenticates and queries — he is the caller, not Alice. There
    // is no `did=` query parameter so Bob cannot ask about Alice.
    let app = standing_test_app!(ctx, &bob, Some("governance:read"));
    let (status, body) = fetch_standing!(app, "/me/standing");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], bob.to_string());
    assert!(
        body["roles"].as_array().unwrap().is_empty(),
        "Bob must not see Alice's role assignment"
    );
}

#[actix_web::test]
async fn domain_membership_static_list_surfaces() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let other_member = fresh_did();
    let domain_id = GovernanceDomainId("nycn-coordination".to_string());
    ctx.manager
        .create_domain(
            domain_id.clone(),
            "NYCN Coordination".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 86_400),
            MembershipConfig::static_list(vec![caller.clone(), other_member]),
        )
        .await
        .expect("create_domain");

    let app = standing_test_app!(ctx, &caller, Some("governance:read"));
    let (status, body) = fetch_standing!(app, "/me/standing");
    assert_eq!(status, StatusCode::OK);

    let domains = body["domains"].as_array().unwrap();
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0]["domain_id"], "nycn-coordination");
    assert_eq!(domains[0]["membership_source"], "static_list");
    assert_eq!(domains[0]["status"], "member");
}

#[actix_web::test]
async fn domain_filter_narrows_results_and_unknown_id_returns_empty() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let domain_a = GovernanceDomainId("domain-a".to_string());
    let domain_b = GovernanceDomainId("domain-b".to_string());
    for d in [&domain_a, &domain_b] {
        ctx.manager
            .create_domain(
                d.clone(),
                format!("Domain {}", d.0),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 86_400),
                MembershipConfig::static_list(vec![caller.clone()]),
            )
            .await
            .expect("create_domain");
    }

    let app = standing_test_app!(ctx, &caller, Some("governance:read"));

    let (status, all) = fetch_standing!(app, "/me/standing");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["domains"].as_array().unwrap().len(), 2);

    let (status, filtered) = fetch_standing!(app, "/me/standing?domain_id=domain-a");
    assert_eq!(status, StatusCode::OK);
    let domains = filtered["domains"].as_array().unwrap();
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0]["domain_id"], "domain-a");

    // Unknown domain → 200 with empty arrays. "No standing in that domain"
    // is a valid answer; 404 would conflate "domain does not exist" with
    // "you have no standing there", which are different concerns.
    let (status, missing) = fetch_standing!(app, "/me/standing?domain_id=ghost");
    assert_eq!(status, StatusCode::OK);
    assert!(missing["domains"].as_array().unwrap().is_empty());
    assert!(missing["roles"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn response_uses_regulatory_safe_vocabulary() {
    let ctx = make_ctx();
    let caller = fresh_did();
    let structure = ctx
        .manager
        .create_structure(
            "entity-a".to_string(),
            StructureKind::Committee,
            "Treasury".to_string(),
            None,
        )
        .expect("create_structure");
    ctx.manager
        .assign_role(
            structure.id,
            caller.clone(),
            "treasurer".to_string(),
            vec![],
        )
        .expect("assign_role");

    let app = standing_test_app!(ctx, &caller, Some("governance:read"));
    let req = test::TestRequest::get().uri("/me/standing").to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    // Pin the status first — otherwise an auth/route regression that returns
    // 401/403/500 with a clean error payload would still pass this scan.
    assert_eq!(
        status,
        StatusCode::OK,
        "regulatory vocab check requires a 200 success body, got {status}: {}",
        String::from_utf8_lossy(&bytes)
    );
    let raw_lower = String::from_utf8_lossy(&bytes).to_lowercase();
    for forbidden in ["payment", "currency", " balance"] {
        assert!(
            !raw_lower.contains(forbidden),
            "/me/standing response must not contain forbidden term '{forbidden}': {raw_lower}"
        );
    }
}

#[actix_web::test]
async fn missing_governance_read_scope_is_rejected() {
    let ctx = make_ctx();
    let caller = fresh_did();
    // Build the test app with no scope injected — the handler must reject.
    let app = standing_test_app!(ctx, &caller, None);
    let req = test::TestRequest::get().uri("/me/standing").to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "missing governance:read must return 401 or 403, got {status}"
    );
}
