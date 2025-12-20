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

use crate::auth::AuthManager;
use crate::error::{GatewayError, Result};
use crate::middleware::get_claims;
use crate::models::{
    CreateSessionRequest, CreateSessionResponse, SessionQrData, SessionStatusResponse,
};
use crate::rate_limit::IpRateLimiter;
use crate::session::{SessionManager, SessionStatus};
use crate::validation;
use icn_identity::Did;

/// Extract client IP address from request (for rate limiting)
fn get_client_ip(req: &HttpRequest) -> String {
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(client_ip) = forwarded_str.split(',').next() {
                return client_ip.trim().to_string();
            }
        }
    }
    if let Some(peer_addr) = req.peer_addr() {
        return peer_addr.ip().to_string();
    }
    "unknown".to_string()
}

/// Get gateway base URL from environment or request headers
/// Handles reverse proxy scenarios (K8s ingress, Cloudflare, nginx, etc.)
fn get_gateway_url(req: &HttpRequest) -> String {
    // First try environment variable (recommended for production)
    if let Ok(url) = std::env::var("GATEWAY_BASE_URL") {
        return url;
    }

    // Determine scheme from X-Forwarded-Proto (set by reverse proxies)
    let scheme = req
        .headers()
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| {
            // Check if connection is secure
            if req.connection_info().scheme() == "https" {
                "https"
            } else {
                "http"
            }
        });

    // Get host from X-Forwarded-Host (reverse proxy) or Host header
    let host = req
        .headers()
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .or_else(|| req.headers().get("host").and_then(|v| v.to_str().ok()));

    if let Some(h) = host {
        // Strip port for standard ports
        let clean_host = if (scheme == "https" && h.ends_with(":443"))
            || (scheme == "http" && h.ends_with(":80"))
        {
            h.split(':').next().unwrap_or(h)
        } else {
            h
        };
        return format!("{scheme}://{clean_host}");
    }

    // Fallback
    "http://localhost:8080".to_string()
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

    // Create session
    let session = session_mgr
        .create_session(req.coop_id.clone())
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to create session: {e}")))?;

    // Get gateway URL for QR data
    let gateway_url = get_gateway_url(&http_req);

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
/// Called by the mobile wallet to approve a web login session.
/// Requires valid JWT token from the wallet.
#[post("/{session_id}/approve")]
pub async fn approve_session(
    http_req: HttpRequest,
    session_mgr: web::Data<Arc<SessionManager>>,
    auth_mgr: web::Data<Arc<AuthManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // session_id comes from the path parameter /{session_id}/approve
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

    // Verify the wallet's token is for the same cooperative
    if claims.coop_id != session.coop_id {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Token coop_id '{}' does not match session coop_id '{}'",
            claims.coop_id, session.coop_id
        )));
    }

    // Issue a new token for the web session
    // Use standard web session scopes
    let scopes = vec![
        "coop:read".to_string(),
        "coop:write".to_string(),
        "ledger:read".to_string(),
        "ledger:transact".to_string(),
        "gov:read".to_string(),
        "gov:write".to_string(),
    ];

    let token = auth_mgr
        .issue_token(&did, &session.coop_id, scopes.clone())
        .map_err(|e| GatewayError::InternalError(format!("Failed to issue token: {e}")))?;

    // Approve the session
    session_mgr
        .approve_session(&session_id, did.clone(), token, 3600, scopes)
        .await
        .map_err(|e| GatewayError::BadRequest(format!("Failed to approve session: {e}")))?;

    tracing::info!(
        session_id = %session_id,
        did = %did,
        coop_id = %session.coop_id,
        "QR login session approved"
    );

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Session approved. Web login will complete shortly."
    })))
}

/// Handler wrapper for approve_session (for use with web::resource().to())
pub async fn approve_session_handler(
    http_req: HttpRequest,
    session_mgr: web::Data<Arc<SessionManager>>,
    auth_mgr: web::Data<Arc<AuthManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Delegate to the main implementation
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

    // Verify the wallet's token is for the same cooperative
    if claims.coop_id != session.coop_id {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Token coop_id '{}' does not match session coop_id '{}'",
            claims.coop_id, session.coop_id
        )));
    }

    // Issue a new token for the web session
    let scopes = vec![
        "coop:read".to_string(),
        "coop:write".to_string(),
        "ledger:read".to_string(),
        "ledger:transact".to_string(),
        "gov:read".to_string(),
        "gov:write".to_string(),
    ];

    let token = auth_mgr
        .issue_token(&did, &session.coop_id, scopes.clone())
        .map_err(|e| GatewayError::InternalError(format!("Failed to issue token: {e}")))?;

    // Approve the session
    session_mgr
        .approve_session(&session_id, did.clone(), token, 3600, scopes)
        .await
        .map_err(|e| GatewayError::BadRequest(format!("Failed to approve session: {e}")))?;

    tracing::info!(
        session_id = %session_id,
        did = %did,
        coop_id = %session.coop_id,
        "QR login session approved"
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

    #[actix_web::test]
    async fn test_create_session() {
        let session_mgr = Arc::new(SessionManager::new());
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_mgr))
                .app_data(web::Data::new(ip_limiter))
                .service(web::scope("/sessions").service(create_session)),
        )
        .await;

        let req_body = CreateSessionRequest {
            coop_id: "test-coop".to_string(),
        };

        let req = test::TestRequest::post()
            .uri("/sessions")
            .set_json(&req_body)
            .to_request();

        let resp: CreateSessionResponse = test::call_and_read_body_json(&app, req).await;

        assert_eq!(resp.session_id.len(), 64); // 32 bytes hex
        assert_eq!(resp.qr_data.coop_id, "test-coop");
        assert!(!resp.qr_data.gateway_url.is_empty());
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
