//! `GET /v1/gov/me/pending-publish-summary` integration tests (issue icn#1728).
//!
//! Proves the runtime read-model that projects
//! `urn:icn:contract:pending-publish-summary:v1` over the gateway so the
//! organizer rehearsal shell (icn#1726) can bind to a real endpoint for the
//! "preview parsed proposals before any publish/mutation" step.
//!
//! The read model is deliberately narrow:
//! - Read-only and self-scoped (`governance:read`, caller = token subject).
//! - Non-production build modes serve deterministic, clearly-labeled
//!   committed-fixture rows (`origin = committed_fixture`); the `production`
//!   build mode serves NO rows (`origin = live_runtime`) so fictional data
//!   never appears on a production surface.
//! - Every row carries an evidence *expectation*, never authority; no write or
//!   action-card creation occurs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{body::to_bytes, http::StatusCode, test, App};
use icn_governance_actor::{
    http::{self, GovernanceContext, GovernanceContextBuildMode},
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

fn make_ctx(build_mode: GovernanceContextBuildMode) -> GovernanceContext<NoopEventEmitter> {
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
        mandate_gate: None,
        build_mode,
    }
}

macro_rules! pps_test_app {
    ($ctx:expr, $caller_did:expr, $scope:expr) => {{
        use actix_web::dev::Service as _;
        use actix_web::HttpMessage as _;
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

macro_rules! fetch {
    ($app:expr, $uri:expr) => {{
        let req = test::TestRequest::get().uri($uri).to_request();
        let resp = test::call_service(&$app, req).await;
        let status = resp.status();
        let bytes = to_bytes(resp.into_body()).await.unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");
        (status, parsed)
    }};
}

/// Non-production build modes serve deterministic committed-fixture rows,
/// explicitly labeled so they can never read as live participant state.
#[actix_web::test]
async fn non_production_serves_labeled_fixture_rows() {
    let ctx = make_ctx(GovernanceContextBuildMode::Bootstrap);
    let caller = fresh_did();
    let app = pps_test_app!(ctx, &caller, Some("governance:read"));

    let (status, body) = fetch!(app, "/me/pending-publish-summary");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], caller.to_string());
    assert_eq!(
        body["origin"], "committed_fixture",
        "non-production must label rows as committed_fixture, never live"
    );
    let rows = body["rows"].as_array().expect("rows is an array");
    assert!(!rows.is_empty(), "non-production serves rehearsal rows");
    assert!(body["generated_at"].as_u64().is_some());

    // Every row must carry a review status and an expected-receipt expectation
    // (an evidence expectation, not authority), and never a DID.
    for row in rows {
        assert!(row["status"].is_string(), "row has a review status");
        assert!(
            row["receipt_expected"]["expected"].is_boolean(),
            "receipt_expected is an evidence expectation"
        );
        assert!(row["source_provenance"].is_string());
        let raw = row.to_string();
        assert!(
            !raw.contains("did:icn:"),
            "a preview row must not embed a DID: {raw}"
        );
    }
}

/// Rows are deterministic: two requests return byte-identical row sets.
#[actix_web::test]
async fn fixture_rows_are_deterministic() {
    let caller = fresh_did();

    let a = {
        let ctx = make_ctx(GovernanceContextBuildMode::Bootstrap);
        let app = pps_test_app!(ctx, &caller, Some("governance:read"));
        let (_, body) = fetch!(app, "/me/pending-publish-summary");
        body["rows"].clone()
    };
    let b = {
        let ctx = make_ctx(GovernanceContextBuildMode::Bootstrap);
        let app = pps_test_app!(ctx, &caller, Some("governance:read"));
        let (_, body) = fetch!(app, "/me/pending-publish-summary");
        body["rows"].clone()
    };
    assert_eq!(a, b, "fixture rows must be deterministic across requests");
}

// The `production` build mode serves NO rows (origin `live_runtime`) — fictional
// rehearsal data must never appear on a production surface. That invariant is
// unit-tested in `http::handlers` (`pending_publish_projection_gates_on_build_mode`)
// rather than here: a Production-mode `configure` fail-closes when
// standing/mandate dependencies are missing (#2075), so a bare production actix
// app cannot be stood up in an integration test.

/// The response uses regulatory-safe vocabulary and no institution-specific
/// nouns leak into the generic read model.
#[actix_web::test]
async fn response_uses_safe_generic_vocabulary() {
    let ctx = make_ctx(GovernanceContextBuildMode::Bootstrap);
    let caller = fresh_did();
    let app = pps_test_app!(ctx, &caller, Some("governance:read"));

    let (status, body) = fetch!(app, "/me/pending-publish-summary");
    assert_eq!(status, StatusCode::OK);
    let raw_lower = body.to_string().to_lowercase();
    for forbidden in [
        "payment",
        "wallet",
        "balance",
        "currency",
        "token",
        "blockchain",
    ] {
        assert!(
            !raw_lower.contains(forbidden),
            "pending-publish-summary response must not contain forbidden term '{forbidden}'"
        );
    }
    // No NYCN / package-specific nouns in the generic core contract.
    for pkg_noun in ["nycn", "sponsor", "summit", "registration"] {
        assert!(
            !raw_lower.contains(pkg_noun),
            "generic read model must not embed package noun '{pkg_noun}'"
        );
    }
}

/// A status value outside the closed taxonomy fails to deserialize (fail-closed):
/// the typed enum never coerces an unknown string.
#[actix_web::test]
async fn unknown_status_fails_closed() {
    use icn_governance_actor::http::models::PendingPublishRowStatus;
    let ok: Result<PendingPublishRowStatus, _> = serde_json::from_str("\"pending_review\"");
    assert!(ok.is_ok(), "known status deserializes");
    let bad: Result<PendingPublishRowStatus, _> = serde_json::from_str("\"approved\"");
    assert!(
        bad.is_err(),
        "an out-of-taxonomy status must fail closed, never coerce"
    );
}

/// The endpoint is self-scoped: two different DIDs both get their own snapshot
/// (the read model is keyed on the token subject), and nothing is written.
#[actix_web::test]
async fn self_scoped_by_token_subject() {
    let alice = fresh_did();
    let bob = fresh_did();
    assert_ne!(alice, bob);

    let ctx = make_ctx(GovernanceContextBuildMode::Bootstrap);
    let app = pps_test_app!(ctx, &alice, Some("governance:read"));
    let (status, body) = fetch!(app, "/me/pending-publish-summary");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["did"], alice.to_string());

    let ctx2 = make_ctx(GovernanceContextBuildMode::Bootstrap);
    let app2 = pps_test_app!(ctx2, &bob, Some("governance:read"));
    let (status2, body2) = fetch!(app2, "/me/pending-publish-summary");
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["did"], bob.to_string());
}
