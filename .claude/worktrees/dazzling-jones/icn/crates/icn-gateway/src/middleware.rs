//! Authentication middleware

use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpRequest,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::sync::Arc;
use std::time::Instant;

use crate::auth::{AuthManager, TokenClaims};
use crate::error::GatewayError;
use icn_obs::metrics::gateway;

/// Extract and verify JWT token from Authorization header
pub async fn jwt_auth(
    req: ServiceRequest,
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

    // Debug-only: log truncated token signature for local troubleshooting.
    #[cfg(debug_assertions)]
    {
        let token_parts: Vec<&str> = token.split('.').collect();
        if token_parts.len() == 3 {
            tracing::debug!(
                "JWT auth attempt - signature: {}...",
                &token_parts[2][..std::cmp::min(20, token_parts[2].len())]
            );
        }
    }

    match auth_manager.verify_token(token) {
        Ok(claims) => {
            // Insert BasicClaims first so apps/governance handlers can extract them
            // without depending on gateway-internal TokenClaims type.
            let basic = icn_http_kit::auth::BasicClaims {
                sub: claims.sub.clone(),
                scope: if claims.scopes.is_empty() {
                    None
                } else {
                    Some(claims.scopes.join(" "))
                },
            };
            req.extensions_mut().insert(basic);
            // Insert gateway-internal TokenClaims for existing gateway handlers.
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

/// Check if request has required scope
pub fn require_scope(req: &HttpRequest, required_scope: &str) -> Result<(), GatewayError> {
    let claims = get_claims(req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    if !claims.scopes.contains(&required_scope.to_string()) {
        // Track authorization failure
        gateway::authorization_failures_inc(required_scope);
        return Err(GatewayError::AuthorizationFailed(format!(
            "Missing required scope: {required_scope}"
        )));
    }

    Ok(())
}

/// Check if request has a scope (non-erroring version)
///
/// Returns true if the authenticated user has the given scope, false otherwise.
/// If no claims are present or the user is not authenticated, returns false.
pub fn has_scope(req: &HttpRequest, scope: &str) -> bool {
    get_claims(req)
        .map(|claims| claims.scopes.contains(&scope.to_string()))
        .unwrap_or(false)
}

/// Check if authenticated user has access to the requested cooperative
/// CRITICAL: Prevents cross-cooperative authorization bypass
pub fn require_coop_access(req: &HttpRequest, coop_id: &str) -> Result<(), GatewayError> {
    let claims = get_claims(req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    if claims.coop_id != coop_id {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Access denied: token is for cooperative '{}', but requested access to '{}'",
            claims.coop_id, coop_id
        )));
    }

    Ok(())
}

/// Middleware for tracking request metrics (count and duration)
pub struct MetricsMiddleware;

impl<S, B> Transform<S, ServiceRequest> for MetricsMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = MetricsMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(MetricsMiddlewareService { service }))
    }
}

pub struct MetricsMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for MetricsMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let start = Instant::now();
        let method = req.method().to_string();

        // Generate unique request ID
        let request_id = uuid::Uuid::new_v4().to_string();

        // SECURITY: Use route pattern instead of raw path to prevent cardinality explosion
        // Raw path contains user-controlled values (coop_id, did) which could create millions of unique labels
        // Pattern normalizes: /ledger/test-coop/balance/did:icn:abc → /ledger/{coop_id}/balance/{did}
        // This prevents Prometheus OOM from unbounded metric cardinality
        let path = req
            .match_pattern()
            .unwrap_or_else(|| req.path().to_string());

        // Extract user_id from claims if authenticated
        let user_id = req
            .extensions()
            .get::<TokenClaims>()
            .map(|claims| claims.sub.clone());

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;

            // Calculate duration
            let duration = start.elapsed().as_secs_f64();
            let status = res.status().as_u16();

            // Record metrics with normalized path
            gateway::requests_total_inc(&path, &method);
            gateway::request_duration_record(&path, status, duration);

            // Structured log with contextual fields
            if let Some(uid) = user_id {
                tracing::info!(
                    request_id = %request_id,
                    user_id = %uid,
                    method = %method,
                    path = %path,
                    status = status,
                    duration_ms = duration * 1000.0,
                    "Request completed"
                );
            } else {
                tracing::info!(
                    request_id = %request_id,
                    method = %method,
                    path = %path,
                    status = status,
                    duration_ms = duration * 1000.0,
                    "Request completed"
                );
            }

            Ok(res)
        })
    }
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
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce (software key or hardware signer configured)");

        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature.to_bytes(),
                "test-coop",
                vec!["ledger:read".to_string()],
            )
            .unwrap();

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
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce (software key or hardware signer configured)");

        let token = auth1
            .verify_challenge(bundle.did(), &signature.to_bytes(), "test-coop", vec![])
            .unwrap();

        // Try to verify with different secret
        let result = auth2.verify_token(&token);
        assert!(matches!(result, Err(GatewayError::AuthenticationFailed(_))));
    }
}
