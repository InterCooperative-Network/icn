//! QR Login Session API endpoints
//!
//! RESTful API for QR-based web authentication.
//!
//! Flow:
//! 1. Web UI calls POST /sessions to create a session
//! 2. Web UI displays QR code containing session data
//! 3. Web UI polls GET /sessions/{id} for status
//! 4. Mobile wallet scans QR and calls POST /sessions/{id}/approve
//! 5. Web UI receives token on next poll

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::{GatewayError, Result};
use crate::middleware::get_claims;
use crate::models::{
    CreateSessionRequest, CreateSessionResponse, SessionQrData, SessionStatusResponse,
};
use crate::rate_limit::IpRateLimiter;
use crate::session::{SessionManager, SessionStatus};
use crate::session_authority::SessionAuthority;
use crate::validation;
use icn_identity::Did;

/// The widest authority this flow may ever delegate to an approved web session.
///
/// This is a **ceiling, not a grant** (issue #2436). The scopes actually issued
/// are this set intersected with the approving credential's own scopes, so a
/// read-only wallet approves a read-only web session. Before attenuation, this
/// list was issued verbatim to any caller holding any valid credential for the
/// cooperative, which let a narrowly-scoped member credential mint
/// `governance:write` for itself.
///
/// The ceiling is deliberately unchanged from the historical list: narrowing it
/// further is a product decision about what a wallet-approved web session is
/// *for*, not a security fix, and attenuation already removes the escalation.
const WEB_SESSION_SCOPE_CEILING: &[&str] = &[
    "coop:read",
    "coop:write",
    "ledger:read",
    "ledger:transact",
    "governance:read",
    "governance:write",
];

/// Extract client IP address from request (for rate limiting).
///
/// SECURITY: `X-Forwarded-For` is honoured only when the immediate transport peer is a
/// trusted proxy; otherwise the socket peer is the client (#2567). See [`crate::client_ip`].
///
/// That trust is scoped to *rate-limit identity* and nothing else. It does not extend to the
/// origin advertised in QR material — see [`crate::advertised_origin`], which takes no request
/// at all (#2569).
fn get_client_ip(req: &HttpRequest) -> String {
    crate::client_ip::client_ip_key(req)
}

// ============================================================================
// Session Endpoints
// ============================================================================

/// POST /v1/sessions - Create a new login session (PUBLIC)
///
/// Creates a new pending session for QR-based login.
/// Returns session ID and QR data to display to the user.
#[post("")]
pub async fn create_session(
    http_req: HttpRequest,
    session_mgr: web::Data<Arc<SessionManager>>,
    ip_limiter: web::Data<Arc<IpRateLimiter>>,
    req: web::Json<CreateSessionRequest>,
) -> Result<HttpResponse> {
    // Rate limit by IP (public endpoint)
    let client_ip = get_client_ip(&http_req);
    ip_limiter.check_rate_limit(&client_ip)?;

    // Validate coop_id
    validation::validate_coop_id(&req.coop_id)?;

    // The origin a scanning device will send its bearer credential to. Resolved BEFORE the
    // session is allocated: an unadvertisable session is useless, and minting one per refused
    // request would let an unauthenticated caller accumulate session state (#2569).
    let gateway_url = crate::advertised_origin::advertised_origin()?;

    // Create session
    let session = session_mgr
        .create_session(req.coop_id.clone())
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to create session: {e}")))?;

    let qr_data = SessionQrData {
        session_id: session.session_id.clone(),
        gateway_url,
        coop_id: session.coop_id.clone(),
        expires_at: session.expires_at,
    };

    tracing::info!(
        session_id = %session.session_id,
        coop_id = %session.coop_id,
        "QR login session created"
    );

    Ok(HttpResponse::Created().json(CreateSessionResponse {
        session_id: session.session_id,
        expires_at: session.expires_at,
        qr_data,
    }))
}

/// GET /v1/sessions/{session_id} - Check session status (PUBLIC)
///
/// Poll this endpoint to check if the session has been approved.
/// When approved, returns the token and DID (one-time use).
#[get("/{session_id}")]
pub async fn get_session_status(
    http_req: HttpRequest,
    session_mgr: web::Data<Arc<SessionManager>>,
    ip_limiter: web::Data<Arc<IpRateLimiter>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Rate limit by IP (public endpoint)
    let client_ip = get_client_ip(&http_req);
    ip_limiter.check_rate_limit(&client_ip)?;

    let session_id = path.into_inner();

    // Get session
    let session = session_mgr
        .get_session(&session_id)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get session: {e}")))?
        .ok_or_else(|| GatewayError::NotFound("Session not found or expired".to_string()))?;

    // If approved, consume the session (one-time token retrieval)
    if session.status == SessionStatus::Approved {
        let consumed = session_mgr
            .consume_session(&session_id)
            .await
            .map_err(|e| GatewayError::InternalError(format!("Failed to consume session: {e}")))?;

        tracing::info!(
            session_id = %session_id,
            did = ?consumed.approved_by,
            "QR login session consumed (token delivered)"
        );

        return Ok(HttpResponse::Ok().json(SessionStatusResponse {
            session_id: consumed.session_id,
            status: "approved".to_string(),
            expires_at: consumed.expires_at,
            token: consumed.token,
            token_expires_in: consumed.token_expires_in,
            did: consumed.approved_by.map(|d| d.to_string()),
            scopes: Some(consumed.scopes),
        }));
    }

    // Return current status (pending, expired, or consumed)
    let status_str = match session.status {
        SessionStatus::Pending => "pending",
        SessionStatus::Approved => "approved", // Should not reach here, but handle it
        SessionStatus::Expired => "expired",
        SessionStatus::Consumed => "consumed",
    };

    Ok(HttpResponse::Ok().json(SessionStatusResponse {
        session_id: session.session_id,
        status: status_str.to_string(),
        expires_at: session.expires_at,
        token: None,
        token_expires_in: None,
        did: None,
        scopes: None,
    }))
}

/// POST /v1/sessions/{session_id}/approve - Approve a login session (AUTHENTICATED)
///
/// Called by the mobile wallet to approve a web login session. Requires a valid
/// credential from the wallet, and issues a web-session credential **attenuated
/// to the approver's own authority** (issue #2436): matching `coop_id` alone is
/// not authorization to mint arbitrary scopes.
///
/// This is the only mounted approval handler (`server.rs`, `/sessions` scope).
/// A second, byte-identical copy carrying the same defect existed here as
/// unmounted dead code and was removed rather than fixed twice.
pub async fn approve_session_handler(
    http_req: HttpRequest,
    session_mgr: web::Data<Arc<SessionManager>>,
    authority: web::Data<Arc<SessionAuthority>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let session_id = path.into_inner();

    // Get authenticated DID from JWT claims
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("Authentication required".to_string()))?;

    let did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Get session to validate coop_id
    let session = session_mgr
        .get_session(&session_id)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get session: {e}")))?
        .ok_or_else(|| GatewayError::NotFound("Session not found or expired".to_string()))?;

    // Same-cooperative is a NECESSARY condition, not a sufficient one: the
    // authority actually delegated is bounded by the approver's scopes below.
    if claims.coop_id != session.coop_id {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Token coop_id '{}' does not match session coop_id '{}'",
            claims.coop_id, session.coop_id
        )));
    }

    // Attenuate: issued ⊆ approver's scopes ∩ this flow's ceiling. An approver
    // holding none of the ceiling's scopes has nothing to delegate and is told
    // so (403) rather than handed a broad credential.
    let (token, scopes) = authority.issue_delegated(
        &claims,
        &did,
        &session.coop_id,
        WEB_SESSION_SCOPE_CEILING,
        None,
    )?;

    let session_ttl_secs = authority.lifetime().ttl().as_secs();

    // Approve the session
    session_mgr
        .approve_session(
            &session_id,
            did.clone(),
            token,
            session_ttl_secs,
            scopes.clone(),
        )
        .await
        .map_err(|e| GatewayError::BadRequest(format!("Failed to approve session: {e}")))?;

    tracing::info!(
        session_id = %session_id,
        did = %did,
        coop_id = %session.coop_id,
        granted_scopes = ?scopes,
        "QR login session approved (attenuated to approver authority)"
    );

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Session approved. Web login will complete shortly."
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    // The crate's single test-time origin authority. Under `cfg(test)` the advertised origin
    // resolves from a module-private thread-local rather than the process environment, so this
    // guard pins a per-test value and no env locking is needed here (#2569).
    use crate::advertised_origin::test_env::EnvGuard;

    /// Builds the `/sessions` app under test.
    macro_rules! session_app {
        () => {
            test::init_service(
                App::new()
                    .app_data(web::Data::new(Arc::new(SessionManager::new())))
                    .app_data(web::Data::new(Arc::new(IpRateLimiter::new_for_auth())))
                    .service(web::scope("/sessions").service(create_session)),
            )
            .await
        };
    }

    fn create_request() -> actix_http::Request {
        test::TestRequest::post()
            .uri("/sessions")
            .set_json(&CreateSessionRequest {
                coop_id: "test-coop".to_string(),
            })
            .to_request()
    }

    #[actix_web::test]
    async fn test_create_session() {
        // QR issuance requires an operator-authoritative origin (#2569); this test previously
        // relied on one being inferred from the request.
        let _env = EnvGuard::acquire(Some("https://gateway.example.coop"));
        let app = session_app!();

        let resp: CreateSessionResponse =
            test::call_and_read_body_json(&app, create_request()).await;

        assert_eq!(resp.session_id.len(), 64); // 32 bytes hex
        assert_eq!(resp.qr_data.coop_id, "test-coop");
        assert_eq!(resp.qr_data.gateway_url, "https://gateway.example.coop");
    }

    /// Without a configured origin there is nothing safe to advertise to a scanning device, so
    /// the session is refused rather than issued with an inferred authority (#2569).
    /// Header-level authority coverage lives in `tests/qr_gateway_url_authority.rs`.
    #[actix_web::test]
    async fn test_create_session_fails_closed_without_configured_origin() {
        let _env = EnvGuard::acquire(None);
        let app = session_app!();

        let resp = test::call_service(&app, create_request()).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[actix_web::test]
    async fn test_get_session_status_pending() {
        let session_mgr = Arc::new(SessionManager::new());
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

        // Create a session first
        let session = session_mgr
            .create_session("test-coop".to_string())
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_mgr))
                .app_data(web::Data::new(ip_limiter))
                .service(web::scope("/sessions").service(get_session_status)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/sessions/{}", session.session_id))
            .to_request();

        let resp: SessionStatusResponse = test::call_and_read_body_json(&app, req).await;

        assert_eq!(resp.status, "pending");
        assert!(resp.token.is_none());
        assert!(resp.did.is_none());
    }

    #[actix_web::test]
    async fn test_get_session_not_found() {
        let session_mgr = Arc::new(SessionManager::new());
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_mgr))
                .app_data(web::Data::new(ip_limiter))
                .service(web::scope("/sessions").service(get_session_status)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/sessions/nonexistent-session-id")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }
}
