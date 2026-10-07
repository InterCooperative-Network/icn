//! `/v1/invites` containment (issue #2589).
//!
//! # The defect
//!
//! `POST /v1/invites/join` issued a session token whose subject was whatever DID
//! the request body named, with `coop:write` and `ledger:transact`, and never
//! asked the caller to prove control of that DID. Every authority gate
//! downstream trusts `claims.sub`, so any bearer holding an invite code could
//! mint a session as any DID — including an office holder's.
//!
//! # What these tests exercise
//!
//! The real `jwt_auth` middleware over a `SessionAuthority` assembled the way
//! production assembles it, mounted through `icn_gateway::server::invites_scope`
//! — the same constructor `GatewayServer::run` calls — so the route set, nesting
//! and middleware are production's, not a test-local rebuild.
//!
//! # What they do NOT prove
//!
//! The *surrounding* route table is still assembled inline in
//! `GatewayServer::run` (#2421); these tests cover the `/invites` subtree only.
//! They also do not provide a safe redemption: re-enabling invite joins needs a
//! proof-of-possession design, which is out of scope here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{http::StatusCode, test, web, App};
use icn_gateway::auth::AuthManager;
use icn_gateway::invite::InviteManager;
use icn_gateway::server::invites_scope;
use icn_gateway::session_authority::{
    AuthorityProfile, InMemoryRevocationAuthority, SessionAuthority, TokenLifetimePolicy,
};
use icn_identity::Did;

const SECRET: &[u8] = b"invite-join-containment-test-secret-32b";
const COOP: &str = "test-coop";

fn authority() -> Arc<SessionAuthority> {
    let lifetime = TokenLifetimePolicy::from_hours(1).unwrap();
    let auth = Arc::new(AuthManager::new(SECRET.to_vec()).with_token_ttl(lifetime.ttl()));
    Arc::new(
        SessionAuthority::new(
            auth,
            Arc::new(InMemoryRevocationAuthority::new()),
            lifetime,
            AuthorityProfile::PortableEvaluator,
        )
        .expect("authority assembles"),
    )
}

fn generated_did() -> Did {
    icn_identity::IdentityBundle::generate()
        .expect("generate test identity")
        .did()
        .clone()
}

fn mint(authority: &SessionAuthority, did: &Did, scopes: &[&str]) -> String {
    authority
        .auth_manager()
        .issue_token(did, COOP, scopes.iter().map(|s| s.to_string()).collect())
        .expect("mint")
}

/// Mount `/invites` the way production does, by calling the same constructor
/// `GatewayServer::run` calls.
macro_rules! invites_app {
    ($authority:expr, $invites:expr) => {{
        test::init_service(
            App::new()
                .app_data(web::Data::new($authority))
                .app_data(web::Data::new($invites))
                .service(web::scope("/v1").service(invites_scope())),
        )
        .await
    }};
}

/// The #2589 defect, expressed as an invariant: a caller authenticated as one
/// DID must not be able to obtain a session for another DID by redeeming an
/// invite. The route is absent, and the invite is not consumed.
#[actix_web::test]
async fn a_bearer_cannot_redeem_an_invite_as_another_did() {
    let authority = authority();
    let invites = Arc::new(InviteManager::new());
    let creator = generated_did();
    let invite = invites
        .create_invite(COOP.to_string(), "member".to_string(), creator, 3600)
        .await
        .expect("create invite");

    let caller = generated_did();
    let victim = generated_did();
    let token = mint(&authority, &caller, &["coop:read", "coop:write"]);
    let app = invites_app!(authority.clone(), invites.clone());

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/v1/invites/join")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({
                "invite_code": invite.code,
                "did": victim.to_string(),
            }))
            .to_request(),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "redeeming an invite for a caller-chosen DID must not be routable"
    );
    let body = test::read_body(response).await;
    assert!(
        !String::from_utf8_lossy(&body).contains("token"),
        "no session may be issued"
    );

    let stored = invites
        .get_invite(&invite.code)
        .await
        .expect("read invite")
        .expect("invite still exists");
    assert!(!stored.used, "the invite must not be consumed");
    assert!(stored.used_by.is_none());
}

/// The containment is targeted: the rest of the scope still serves, under the
/// same bearer authentication.
#[actix_web::test]
async fn the_invite_scope_still_lists_invites_for_an_authorized_caller() {
    let authority = authority();
    let invites = Arc::new(InviteManager::new());
    let creator = generated_did();
    let invite = invites
        .create_invite(COOP.to_string(), "member".to_string(), creator, 3600)
        .await
        .expect("create invite");

    let caller = generated_did();
    let token = mint(&authority, &caller, &["coop:read"]);
    let app = invites_app!(authority.clone(), invites.clone());

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/v1/invites?coop_id={COOP}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = test::read_body(response).await;
    assert!(String::from_utf8_lossy(&body).contains(&invite.code));
}

/// And the scope still refuses anonymous callers before any handler runs.
#[actix_web::test]
async fn the_invite_scope_still_requires_a_bearer() {
    let authority = authority();
    let invites = Arc::new(InviteManager::new());
    let app = invites_app!(authority.clone(), invites.clone());

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/v1/invites?coop_id={COOP}"))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
