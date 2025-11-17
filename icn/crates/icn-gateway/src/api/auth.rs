//! Authentication endpoints

use actix_web::{post, web, HttpResponse};
use std::sync::Arc;

use crate::auth::AuthManager;
use crate::error::Result;
use crate::models::{ChallengeRequest, ChallengeResponse, TokenResponse, VerifyRequest};
use crate::validation;
use icn_obs::metrics::gateway;

/// POST /auth/challenge - Request authentication challenge
#[post("/auth/challenge")]
pub async fn challenge(
    auth: web::Data<Arc<AuthManager>>,
    req: web::Json<ChallengeRequest>,
) -> Result<HttpResponse> {
    let did = req.did.parse()
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
    auth: web::Data<Arc<AuthManager>>,
    req: web::Json<VerifyRequest>,
) -> Result<HttpResponse> {
    // Increment verification attempt metric
    gateway::auth_verifications_inc();

    let did = req.did.parse()
        .map_err(|e| {
            gateway::auth_failures_inc("invalid_did");
            crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}"))
        })?;

    // Validate scopes
    validation::validate_scopes(&req.scopes).inspect_err(|e| {
        gateway::auth_failures_inc("invalid_scopes");
    })?;

    // Validate coop_id format
    validation::validate_coop_id(&req.coop_id).inspect_err(|e| {
        gateway::auth_failures_inc("invalid_coop_id");
    })?;

    let signature = hex::decode(&req.signature)
        .map_err(|e| {
            gateway::auth_failures_inc("invalid_signature_encoding");
            crate::error::GatewayError::BadRequest(
                format!("Invalid signature encoding: {e}")
            )
        })?;

    // Validate signature length BEFORE expensive verification
    // Ed25519 signatures are exactly 64 bytes
    if signature.len() != 64 {
        gateway::auth_failures_inc("invalid_signature_length");
        return Err(crate::error::GatewayError::BadRequest(
            format!("Invalid signature length: expected 64 bytes, got {}", signature.len())
        ));
    }

    let token = auth.verify_challenge(
        &did,
        &signature,
        &req.coop_id,
        req.scopes.clone(),
    ).inspect_err(|e| {
        gateway::auth_failures_inc("verification_failed");
    })?;

    // Track successful authentication
    gateway::auth_successes_inc();

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
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .service(challenge)
        ).await;

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
        let bundle = IdentityBundle::generate().unwrap();

        // First, create a challenge
        let nonce = auth.create_challenge(bundle.did()).unwrap();

        // Sign the nonce
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .service(verify)
        ).await;

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
        let bundle = IdentityBundle::generate().unwrap();

        // Create a challenge but don't sign properly
        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .service(verify)
        ).await;

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
        let bundle = IdentityBundle::generate().unwrap();

        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .service(verify)
        ).await;

        // Test with wrong signature length (32 bytes instead of 64)
        let req_body = VerifyRequest {
            did: bundle.did().to_string(),
            signature: hex::encode([0u8; 32]),  // Wrong length!
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
        let bundle = IdentityBundle::generate().unwrap();

        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .service(verify)
        ).await;

        // Test with invalid coop_id (contains invalid characters)
        let req_body = VerifyRequest {
            did: bundle.did().to_string(),
            signature: hex::encode([0u8; 64]),
            coop_id: "invalid@coop#id!".to_string(),  // Invalid characters
            scopes: vec![],
        };

        let req = test::TestRequest::post()
            .uri("/auth/verify")
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400); // Bad Request
    }
}
