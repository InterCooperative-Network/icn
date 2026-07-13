//! HTTP auth-boundary proof for the `/v1/services/*` service-discovery routes
//! (ADR-0015 verification requested by issue #1642).
//!
//! ## What this pins (the route/auth matrix, traced 2026-07-13)
//!
//! Production wiring (`server.rs` `/services` scope) wraps ALL five routes in
//! `HttpAuthentication::bearer(jwt_auth)` with auth outermost, so authentication
//! runs BEFORE any handler or resource lookup:
//!
//! 1. **No token → 401**, with the SAME status code for existing and
//!    nonexistent `service_id`s, so unauthenticated callers get NO enumeration
//!    oracle. (Uniformity here is asserted at the status-code level; because
//!    rejection happens before any lookup, no resource data can reach the
//!    response body either.)
//! 2. **Invalid token → 401**, likewise status-uniform across existence.
//! 3. **Valid token + nonexistent resource → 404** for `GET /{service_id}`,
//!    `DELETE /{service_id}`, and `/discover` with no matches; the list route
//!    (`GET /v1/services`) returns 200 with an empty result set (list semantics).
//! 4. **Valid token + existing resource → 200** (the 401s above are the auth
//!    boundary, not a broken route); withdraw by a non-owning provider → 403
//!    (operation-specific authority boundary).
//!
//! ## Decision context (ADR-0015 as amended 2026-07-13)
//!
//! The amended ADR-0015 makes uniform pre-lookup **401** the accepted behavior
//! for missing/invalid credentials: authentication, authorization, and resource
//! absence are distinct outcomes, and the enumeration-safety invariant binds
//! credential failures to response *uniformity*, not to a literal 404. This
//! matrix is therefore the specified contract, not an interim snapshot. If
//! service visibility later becomes scope-restricted, the ADR requires the
//! authenticated existence-disclosure behavior (the 404/200 rows) to be
//! re-reviewed — update these assertions in the same PR as any such change.
//!
//! The scope wiring below mirrors `server.rs` (`configure` + trust rate-limit
//! wrap + bearer wrap) so the middleware ordering under test matches production.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use actix_web::{middleware::from_fn, test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use icn_gateway::{
    api, auth::AuthManager, middleware::jwt_auth, rate_limit::trust_rate_limit_middleware,
    service_discovery_mgr::ServiceDiscoveryManager,
};
use icn_identity::IdentityBundle;
use icn_kernel_api::naming::{EndpointType, ServiceEndpoint, ServiceType};
use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::types::{Endpoint, Signature};
use std::sync::Arc;

const EXISTING_ID: &str = "svc-auth-boundary-existing";
const MISSING_ID: &str = "svc-auth-boundary-nonexistent";

fn seeded_endpoint() -> ServiceEndpoint {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    ServiceEndpoint {
        service_id: EXISTING_ID.to_string(),
        provider: "did:icn:test-provider".to_string(),
        endpoint_type: EndpointType::Http,
        service_type: ServiceType {
            name: "ledger".to_string(),
            version: "1.0".to_string(),
        },
        endpoints: vec![Endpoint::new("https", "example.com", 8080)],
        addresses: vec![],
        capabilities: vec!["read".to_string()],
        trust_threshold: 0.1,
        scope_visibility: ScopeLevel::Org,
        cell_id: None,
        ttl_secs: 3600,
        signature: Signature::new(vec![0; 64]),
        created_at: now,
        updated_at: now,
    }
}

/// Build a test app mounting `/v1/services` exactly as `server.rs` does:
/// `.configure(api::services::configure)` + trust rate-limit wrap + bearer wrap
/// (last wrap outermost → auth runs first). Seeds one existing service so the
/// uniformity assertions can compare existing vs nonexistent targets.
async fn build_app(
    auth_manager: Arc<AuthManager>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let mgr = Arc::new(ServiceDiscoveryManager::new());
    mgr.announce(seeded_endpoint()).await.expect("seed service");

    let auth = HttpAuthentication::bearer(jwt_auth);
    test::init_service(
        App::new()
            .app_data(web::Data::new(auth_manager))
            .app_data(web::Data::new(mgr))
            .service(
                web::scope("/v1").service(
                    web::scope("/services")
                        .configure(api::services::configure)
                        .wrap(from_fn(trust_rate_limit_middleware))
                        .wrap(auth),
                ),
            ),
    )
    .await
}

fn test_auth_manager() -> Arc<AuthManager> {
    Arc::new(AuthManager::new(
        b"services-auth-boundary-test-secret-32bytes".to_vec(),
    ))
}

fn valid_token(auth_manager: &AuthManager) -> String {
    let bundle = IdentityBundle::generate().unwrap();
    auth_manager
        .issue_token(bundle.did(), "test-coop", vec![])
        .expect("issue token")
}

/// The five (method, path) pairs registered by `api::services::configure`.
fn route_matrix() -> Vec<(actix_web::http::Method, String)> {
    use actix_web::http::Method;
    vec![
        (Method::POST, "/v1/services/announce".to_string()),
        (Method::GET, "/v1/services/discover".to_string()),
        (Method::GET, "/v1/services".to_string()),
        (Method::GET, format!("/v1/services/{MISSING_ID}")),
        (Method::DELETE, format!("/v1/services/{MISSING_ID}")),
    ]
}

async fn status_for(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    method: actix_web::http::Method,
    path: &str,
    bearer: Option<&str>,
) -> u16 {
    let mut req = test::TestRequest::with_uri(path).method(method);
    if let Some(token) = bearer {
        req = req.insert_header(("Authorization", format!("Bearer {token}")));
    }
    test::call_service(app, req.to_request())
        .await
        .status()
        .as_u16()
}

#[actix_web::test]
async fn no_token_is_401_on_every_route_and_uniform_across_existence() {
    let app = build_app(test_auth_manager()).await;

    for (method, path) in route_matrix() {
        let status = status_for(&app, method.clone(), &path, None).await;
        assert_eq!(
            status, 401,
            "expected 401 without a token on {method} {path}"
        );
    }

    // Anti-oracle uniformity: the unauthenticated response must not depend on
    // whether the service exists. (The seeded service IS retrievable — see the
    // authorized tests — so a leak here would be observable.)
    let existing = status_for(
        &app,
        actix_web::http::Method::GET,
        &format!("/v1/services/{EXISTING_ID}"),
        None,
    )
    .await;
    let missing = status_for(
        &app,
        actix_web::http::Method::GET,
        &format!("/v1/services/{MISSING_ID}"),
        None,
    )
    .await;
    assert_eq!(existing, 401);
    assert_eq!(
        existing, missing,
        "unauthenticated response status must not differ between existing and nonexistent ids"
    );
}

#[actix_web::test]
async fn invalid_token_is_401_on_every_route_and_uniform_across_existence() {
    let app = build_app(test_auth_manager()).await;

    for (method, path) in route_matrix() {
        let status = status_for(&app, method.clone(), &path, Some("not-a-real-jwt")).await;
        assert_eq!(
            status, 401,
            "expected 401 with a garbage token on {method} {path}"
        );
    }

    let existing = status_for(
        &app,
        actix_web::http::Method::GET,
        &format!("/v1/services/{EXISTING_ID}"),
        Some("not-a-real-jwt"),
    )
    .await;
    let missing = status_for(
        &app,
        actix_web::http::Method::GET,
        &format!("/v1/services/{MISSING_ID}"),
        Some("not-a-real-jwt"),
    )
    .await;
    assert_eq!(existing, 401);
    assert_eq!(
        existing, missing,
        "invalid-token response status must not differ between existing and nonexistent ids"
    );
}

#[actix_web::test]
async fn authorized_semantics_nonexistent_vs_existing() {
    let auth_manager = test_auth_manager();
    let app = build_app(auth_manager.clone()).await;
    let token = valid_token(&auth_manager);
    use actix_web::http::Method;

    // Existing resource is retrievable — proving the 401s above are the auth
    // boundary, not a dead route.
    assert_eq!(
        status_for(
            &app,
            Method::GET,
            &format!("/v1/services/{EXISTING_ID}"),
            Some(&token)
        )
        .await,
        200
    );

    // Authorized + nonexistent single resource → 404 (ADR-0015 point 2).
    assert_eq!(
        status_for(
            &app,
            Method::GET,
            &format!("/v1/services/{MISSING_ID}"),
            Some(&token)
        )
        .await,
        404
    );
    // `provider` is a required query param on withdraw: omitting it is a 400
    // (input validation), independent of resource existence.
    assert_eq!(
        status_for(
            &app,
            Method::DELETE,
            &format!("/v1/services/{MISSING_ID}"),
            Some(&token)
        )
        .await,
        400
    );
    assert_eq!(
        status_for(
            &app,
            Method::DELETE,
            &format!("/v1/services/{MISSING_ID}?provider=did:icn:test-provider"),
            Some(&token)
        )
        .await,
        404
    );
    // Authorized withdraw of an EXISTING service by a non-owning provider → 403
    // (the provider-mismatch authority boundary; nothing is withdrawn).
    assert_eq!(
        status_for(
            &app,
            Method::DELETE,
            &format!("/v1/services/{EXISTING_ID}?provider=did:icn:not-the-owner"),
            Some(&token)
        )
        .await,
        403
    );

    // Discover with no matches → 404 (documented handler mapping).
    assert_eq!(
        status_for(
            &app,
            Method::GET,
            "/v1/services/discover?type=no-such-service-type",
            Some(&token)
        )
        .await,
        404
    );

    // List route: 200 with results (list semantics — never a 404).
    assert_eq!(
        status_for(&app, Method::GET, "/v1/services", Some(&token)).await,
        200
    );
}
