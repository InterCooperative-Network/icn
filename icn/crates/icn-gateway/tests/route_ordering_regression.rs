//! Regression test for actix-web empty-scope ordering footgun (PR #1290).
//!
//! # The bug
//!
//! `web::scope("")` has an empty prefix, so actix-web matches it for **every**
//! path. When a `web::scope("")` is registered before specific-prefix scopes
//! (e.g. `/services`, `/names`, `/compute`), the router matches the empty scope
//! first. If no inner route matches, it returns **404 without falling through**
//! to the specific scopes that follow — silently making them all unreachable.
//!
//! In the ICN gateway, the governance scope (`web::scope("").configure(governance)`)
//! was placed at server.rs line 1183, before `/rights`, `/invites`, `/compute`,
//! `/execution`, `/names`, and `/services`. All of those routes returned 404.
//!
//! # The fix
//!
//! All `web::scope("")` registrations must be **last** in the service list.
//! The gateway already had a `// === Empty-scope services (MUST be last) ===`
//! section; the governance scope was moved there.
//!
//! # What these tests check
//!
//! - `test_empty_scope_first_shadows_later_scopes`: documents the failure mode
//!   (governance first → /services returns 404)
//! - `test_empty_scope_last_all_routes_reachable`: validates the fix
//!   (governance last → /services, /names, /gov/* all return 200)

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{test, web, App, HttpResponse};

fn configure_governance_sim(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/gov/proposals")
            .route(web::get().to(|| async { HttpResponse::Ok().body("gov-proposals") })),
    )
    .service(
        web::resource("/gov/domains")
            .route(web::get().to(|| async { HttpResponse::Ok().body("gov-domains") })),
    );
}

fn configure_services_sim(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/discover")
            .route(web::get().to(|| async { HttpResponse::Ok().body("discover") })),
    )
    .service(
        web::resource("/announce")
            .route(web::post().to(|| async { HttpResponse::Ok().body("announce") })),
    );
}

fn configure_names_sim(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/lookup")
            .route(web::get().to(|| async { HttpResponse::Ok().body("names-lookup") })),
    );
}

/// Documents the failure mode: empty scope registered before specific scopes
/// shadows them entirely, making all routes after it return 404.
///
/// This test exists to lock in our understanding of actix-web's no-fallthrough
/// behavior for empty-prefix scopes. If it starts failing (200 instead of 404),
/// actix-web's routing semantics have changed and the ordering restriction may
/// no longer be necessary.
#[actix_web::test]
async fn test_empty_scope_first_shadows_later_scopes() {
    let app = test::init_service(
        App::new()
            // governance: empty prefix, registered FIRST (the bug)
            .service(web::scope("").configure(configure_governance_sim))
            // specific-prefix scopes registered AFTER (unreachable due to shadow)
            .service(web::scope("/services").configure(configure_services_sim))
            .service(web::scope("/names").configure(configure_names_sim)),
    )
    .await;

    // governance routes inside the empty scope work fine
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/gov/proposals").to_request(),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);

    // /services is unreachable — swallowed by the empty governance scope
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/services/discover").to_request(),
    )
    .await;
    assert_eq!(
        resp.status().as_u16(),
        404,
        "empty scope registered first shadows /services (documents the bug)"
    );

    // /names is unreachable for the same reason
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/names/lookup").to_request(),
    )
    .await;
    assert_eq!(
        resp.status().as_u16(),
        404,
        "empty scope registered first shadows /names (documents the bug)"
    );
}

/// Validates the fix: empty scope registered LAST — all specific-prefix
/// scopes are reachable, and governance routes still work.
///
/// This is the regression guard: if the governance scope is moved back before
/// the specific scopes, these assertions will fail.
#[actix_web::test]
async fn test_empty_scope_last_all_routes_reachable() {
    let app = test::init_service(
        App::new()
            // specific-prefix scopes registered first (correct order)
            .service(web::scope("/services").configure(configure_services_sim))
            .service(web::scope("/names").configure(configure_names_sim))
            // governance: empty prefix, registered LAST (the fix)
            .service(web::scope("").configure(configure_governance_sim)),
    )
    .await;

    // governance routes still reachable
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/gov/proposals").to_request(),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200, "/gov/proposals must be reachable");

    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/gov/domains").to_request(),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200, "/gov/domains must be reachable");

    // /services routes are now reachable
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/services/discover").to_request(),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200, "/services/discover must be reachable");

    let resp = test::call_service(
        &app,
        test::TestRequest::post().uri("/services/announce").to_request(),
    )
    .await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "/services/announce must be reachable"
    );

    // /names routes are now reachable
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/names/lookup").to_request(),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200, "/names/lookup must be reachable");
}
