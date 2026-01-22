//! Authentication endpoints

use actix_web::{post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::audit::AuditLogger;
use crate::auth::AuthManager;
use crate::error::Result;
use crate::models::{ChallengeRequest, ChallengeResponse, TokenResponse, VerifyRequest};
use crate::rate_limit::IpRateLimiter;
use crate::validation;
use icn_obs::metrics::gateway;

/// Extract client IP address from request
/// Returns IP as string, using X-Forwarded-For header if present (proxy support)
fn get_client_ip(req: &HttpRequest) -> String {
    // Check for X-Forwarded-For header first (reverse proxy support)
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // X-Forwarded-For can contain multiple IPs (client, proxy1, proxy2...)
            // Use the first one (actual client)
            if let Some(client_ip) = forwarded_str.split(',').next() {
                return client_ip.trim().to_string();
            }
        }
    }

    // Fall back to connection info
    if let Some(peer_addr) = req.peer_addr() {
        return peer_addr.ip().to_string();
    }

    // Last resort: return placeholder (should never happen)
    "unknown".to_string()
}

/// POST /auth/challenge - Request authentication challenge
#[post("/auth/challenge")]
pub async fn challenge(
    http_req: HttpRequest,
    auth: web::Data<Arc<AuthManager>>,
    ip_limiter: web::Data<Arc<IpRateLimiter>>,
    req: web::Json<ChallengeRequest>,
) -> Result<HttpResponse> {
    // SECURITY: IP-based rate limiting for unauthenticated endpoint (Bug #29 fix)
    let client_ip = get_client_ip(&http_req);
    ip_limiter.check_rate_limit(&client_ip)?;

    let did = req
        .did
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let nonce = auth.create_challenge(&did)?;

    // Increment challenge metric
    gateway::auth_challenges_inc();

    let response = ChallengeResponse {
        nonce,
        expires_in: 300, // 5 minutes
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /auth/verify - Verify signed challenge and get token
#[post("/auth/verify")]
pub async fn verify(
    http_req: HttpRequest,
    auth: web::Data<Arc<AuthManager>>,
    ip_limiter: web::Data<Arc<IpRateLimiter>>,
    req: web::Json<VerifyRequest>,
) -> Result<HttpResponse> {
    // SECURITY: IP-based rate limiting for unauthenticated endpoint (Bug #29 fix)
    let client_ip = get_client_ip(&http_req);
    ip_limiter.check_rate_limit(&client_ip)?;

    // Increment verification attempt metric
    gateway::auth_verifications_inc();

    let did = req.did.parse().map_err(|e| {
        gateway::auth_failures_inc("invalid_did");
        crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}"))
    })?;

    // Validate scopes
    let validation_result = validation::validate_scopes(&req.scopes);
    if validation_result.is_err() {
        gateway::auth_failures_inc("invalid_scopes");
        // AUDIT: Log invalid scope request (potential privilege escalation)
        AuditLogger::log_invalid_scope_request(&req.did, &req.scopes, &client_ip);
    }
    validation_result?;

    // Validate coop_id format
    validation::validate_coop_id(&req.coop_id).inspect_err(|_e| {
        gateway::auth_failures_inc("invalid_coop_id");
    })?;

    let signature = hex::decode(&req.signature).map_err(|e| {
        gateway::auth_failures_inc("invalid_signature_encoding");
        crate::error::GatewayError::BadRequest(format!("Invalid signature encoding: {e}"))
    })?;

    // Validate signature length BEFORE expensive verification
    // Ed25519 signatures are exactly 64 bytes
    if signature.len() != 64 {
        gateway::auth_failures_inc("invalid_signature_length");
        return Err(crate::error::GatewayError::BadRequest(format!(
            "Invalid signature length: expected 64 bytes, got {}",
            signature.len()
        )));
    }

    let token = auth
        .verify_challenge(&did, &signature, &req.coop_id, req.scopes.clone())
        .inspect_err(|e| {
            gateway::auth_failures_inc("verification_failed");
            // AUDIT: Log failed authentication attempt
            AuditLogger::log_auth_attempt(&req.did, false, Some(&e.to_string()), &client_ip);
        })?;

    // Track successful authentication
    gateway::auth_successes_inc();

    // AUDIT: Log successful authentication
    AuditLogger::log_auth_attempt(&req.did, true, None, &client_ip);

    let response = TokenResponse {
        token,
        expires_in: 3600, // 1 hour
    };

    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_challenge_endpoint() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .app_data(web::Data::new(ip_limiter))
                .service(challenge),
        )
        .await;

        let bundle = IdentityBundle::generate().unwrap();
        let req_body = ChallengeRequest {
            did: bundle.did().to_string(),
        };

        let req = test::TestRequest::post()
            .uri("/auth/challenge")
            .set_json(&req_body)
            .to_request();

        let resp: ChallengeResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.nonce.len(), 64); // 32 bytes hex-encoded
        assert_eq!(resp.expires_in, 300);
    }

    #[actix_web::test]
    async fn test_verify_endpoint_success() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
        let bundle = IdentityBundle::generate().unwrap();

        // First, create a challenge
        let nonce = auth.create_challenge(bundle.did()).unwrap();

        // Sign the nonce
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce (software key or hardware signer configured)");
        let signature_bytes = signature.to_bytes();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .app_data(web::Data::new(ip_limiter))
                .service(verify),
        )
        .await;

        let req_body = VerifyRequest {
            did: bundle.did().to_string(),
            signature: hex::encode(signature_bytes),
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()],
        };

        let req = test::TestRequest::post()
            .uri("/auth/verify")
            .set_json(&req_body)
            .to_request();

        let resp: TokenResponse = test::call_and_read_body_json(&app, req).await;
        assert!(!resp.token.is_empty());
        assert_eq!(resp.expires_in, 3600);
    }

    #[actix_web::test]
    async fn test_verify_endpoint_invalid_signature() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
        let bundle = IdentityBundle::generate().unwrap();

        // Create a challenge but don't sign properly
        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .app_data(web::Data::new(ip_limiter))
                .service(verify),
        )
        .await;

        let req_body = VerifyRequest {
            did: bundle.did().to_string(),
            signature: hex::encode([0u8; 64]),
            coop_id: "test-coop".to_string(),
            scopes: vec![],
        };

        let req = test::TestRequest::post()
            .uri("/auth/verify")
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401); // Unauthorized
    }

    #[actix_web::test]
    async fn test_verify_endpoint_invalid_signature_length() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
        let bundle = IdentityBundle::generate().unwrap();

        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .app_data(web::Data::new(ip_limiter))
                .service(verify),
        )
        .await;

        // Test with wrong signature length (32 bytes instead of 64)
        let req_body = VerifyRequest {
            did: bundle.did().to_string(),
            signature: hex::encode([0u8; 32]), // Wrong length!
            coop_id: "test-coop".to_string(),
            scopes: vec![],
        };

        let req = test::TestRequest::post()
            .uri("/auth/verify")
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request
    }

    #[actix_web::test]
    async fn test_verify_endpoint_invalid_coop_id() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
        let bundle = IdentityBundle::generate().unwrap();

        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .app_data(web::Data::new(ip_limiter))
                .service(verify),
        )
        .await;

        // Test with invalid coop_id (contains invalid characters)
        let req_body = VerifyRequest {
            did: bundle.did().to_string(),
            signature: hex::encode([0u8; 64]),
            coop_id: "invalid@coop#id!".to_string(), // Invalid characters
            scopes: vec![],
        };

        let req = test::TestRequest::post()
            .uri("/auth/verify")
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request
    }

    #[actix_web::test]
    async fn test_ip_rate_limiting_on_challenge() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .app_data(web::Data::new(ip_limiter))
                .service(challenge),
        )
        .await;

        let bundle = IdentityBundle::generate().unwrap();
        let req_body = ChallengeRequest {
            did: bundle.did().to_string(),
        };

        // Make 20 requests (the burst capacity)
        for i in 0..20 {
            let req = test::TestRequest::post()
                .uri("/auth/challenge")
                .set_json(&req_body)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(
                resp.status(),
                200,
                "Request {} should succeed (within burst capacity)",
                i + 1
            );
        }

        // 21st request should be rate limited (exceeds burst capacity of 20)
        let req = test::TestRequest::post()
            .uri("/auth/challenge")
            .set_json(&req_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            429,
            "Request 21 should be rate limited (HTTP 429)"
        );
    }

    #[actix_web::test]
    async fn test_ip_rate_limiting_on_verify() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        // Use zero refill rate to ensure exactly 20 requests allowed (no token regeneration during test)
        let ip_limiter = Arc::new(IpRateLimiter::new_with_config(
            crate::rate_limit::RateLimitConfig {
                capacity: 20.0,
                refill_rate: 0.0, // No refill during test - ensures deterministic behavior
                cost_per_request: 1.0,
            },
        ));
        let bundle = IdentityBundle::generate().unwrap();

        // Create a valid challenge/signature for test
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce (software key or hardware signer configured)");
        let signature_bytes = signature.to_bytes();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth.clone()))
                .app_data(web::Data::new(ip_limiter))
                .service(verify),
        )
        .await;

        let req_body = VerifyRequest {
            did: bundle.did().to_string(),
            signature: hex::encode(signature_bytes),
            coop_id: "test-coop".to_string(),
            scopes: vec![],
        };

        // Make 20 requests (the burst capacity)
        for i in 0..20 {
            // Create new challenge for each verify attempt
            let _ = auth.create_challenge(bundle.did()).unwrap();

            let req = test::TestRequest::post()
                .uri("/auth/verify")
                .set_json(&req_body)
                .to_request();
            let resp = test::call_service(&app, req).await;
            // Note: Some may fail due to signature/nonce mismatch, but they should NOT be rate limited
            assert_ne!(
                resp.status(),
                429,
                "Request {} should not be rate limited yet",
                i + 1
            );
        }

        // Small delay to ensure rate limiter state is consistent
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 21st request should be rate limited (exceeds burst capacity of 20)
        let req = test::TestRequest::post()
            .uri("/auth/verify")
            .set_json(&req_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            429,
            "Request 21 should be rate limited (HTTP 429)"
        );
    }
}
