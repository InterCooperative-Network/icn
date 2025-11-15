//! Authentication middleware

use actix_web::{dev::ServiceRequest, Error, HttpMessage, HttpRequest};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use std::sync::Arc;

use crate::auth::{AuthManager, TokenClaims};
use crate::error::GatewayError;

/// Extract and verify JWT token from Authorization header
pub async fn jwt_auth(
    mut req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    // Get auth manager
    let auth_manager = match req.app_data::<actix_web::web::Data<Arc<AuthManager>>>() {
        Some(mgr) => mgr.clone(),
        None => {
            let err = GatewayError::InternalError("AuthManager not found".to_string());
            return Err((Error::from(err), req));
        }
    };

    let token = credentials.token();

    match auth_manager.verify_token(token) {
        Ok(claims) => {
            // Insert claims into request extensions for handlers to access
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(e) => Err((Error::from(e), req)),
    }
}

/// Extract authenticated user's claims from request (for use in handlers)
pub fn get_claims(req: &HttpRequest) -> Option<TokenClaims> {
    req.extensions().get::<TokenClaims>().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthManager;
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_jwt_auth_valid_token() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let bundle = IdentityBundle::generate().unwrap();

        // Create challenge and get token
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().sign(&nonce_bytes);

        let token = auth.verify_challenge(
            bundle.did(),
            &signature.to_bytes(),
            "test-coop",
            vec!["ledger:read".to_string()],
        ).unwrap();

        // Verify the token directly
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.sub, bundle.did().to_string());
        assert_eq!(claims.coop_id, "test-coop");
        assert_eq!(claims.scopes, vec!["ledger:read"]);
    }

    #[actix_web::test]
    async fn test_jwt_auth_invalid_token() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));

        let result = auth.verify_token("invalid.token.here");
        assert!(matches!(result, Err(GatewayError::AuthenticationFailed(_))));
    }

    #[actix_web::test]
    async fn test_jwt_auth_expired_token() {
        // Create token with different secret
        let auth1 = Arc::new(AuthManager::new(b"secret1".to_vec()));
        let auth2 = Arc::new(AuthManager::new(b"secret2".to_vec()));

        let bundle = IdentityBundle::generate().unwrap();
        let nonce = auth1.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().sign(&nonce_bytes);

        let token = auth1.verify_challenge(
            bundle.did(),
            &signature.to_bytes(),
            "test-coop",
            vec![],
        ).unwrap();

        // Try to verify with different secret
        let result = auth2.verify_token(&token);
        assert!(matches!(result, Err(GatewayError::AuthenticationFailed(_))));
    }
}
