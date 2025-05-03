use axum::{
    body::Body,
    extract::Extension,
    http::{Request, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use base64::{Engine, engine::general_purpose::URL_SAFE};
use crate::state::AppState;

// Auth token structure that mirrors the one in wallet-core
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthToken {
    pub did: String,
    pub exp: DateTime<Utc>,
    pub thread_scope: Option<String>,
    pub iat: DateTime<Utc>,
}

// User claims extracted from a valid token
#[derive(Debug, Clone)]
pub struct UserClaims {
    pub did: String,
    pub thread_scope: Option<String>,
    pub exp: DateTime<Utc>,
}

// Helper to extract user from request extensions
#[derive(Debug, Clone)]
pub struct AuthUser(pub UserClaims);

// Rename the auth_middleware to did_auth_middleware
pub async fn did_auth_middleware<B>(
    mut req: Request<B>,
    next: Next,
) -> Result<Response, StatusCode> 
where
    B: Send + 'static,
{
    // Extract the Authorization header
    let auth_header = req.headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");
    
    // For development: Skip authentication if no token is provided
    if auth_header.is_empty() || !auth_header.starts_with("Bearer ") {
        // Create a mock user for development
        let user_claims = UserClaims {
            did: "did:key:mock".to_string(),
            thread_scope: None,
            exp: Utc::now() + chrono::Duration::hours(1),
        };
        
        req.extensions_mut().insert(AuthUser(user_claims));
        
        // Convert the request type from B to Body (this is what Axum middleware expects)
        let (parts, _) = req.into_parts();
        let new_req = Request::from_parts(parts, Body::empty());
        
        return Ok(next.run(new_req).await);
    }
    
    // Process the bearer token
    let token = &auth_header["Bearer ".len()..];
    let token_parts: Vec<&str> = token.split('.').collect();
    
    if token_parts.len() != 2 {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Extract payload to get DID
    let payload_bytes = URL_SAFE.decode(token_parts[0])
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    let payload_json = String::from_utf8(payload_bytes)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
        
    let token: AuthToken = serde_json::from_str(&payload_json)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    // For simplicity, we'll skip signature validation in this example
    // In a real app, we'd verify the signature against the user's public key
    
    // Check token expiration
    let now = Utc::now();
    if token.exp < now {
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Create user claims and inject into request
    let user_claims = UserClaims {
        did: token.did,
        thread_scope: token.thread_scope,
        exp: token.exp,
    };
    
    req.extensions_mut().insert(AuthUser(user_claims));
    
    // Convert the request type from B to Body (this is what Axum middleware expects)
    let (parts, _) = req.into_parts();
    let new_req = Request::from_parts(parts, Body::empty());
    
    // Continue with the request
    Ok(next.run(new_req).await)
}

// Endpoint to verify a token (for testing)
pub async fn verify_token(
    Extension(AuthUser(user)): Extension<AuthUser>,
) -> String {
    format!("Token valid for DID: {}", user.did)
}

// Define the permission enum
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    Read,
    PostMessage,
    ModerateContent,
    ReactToMessage,
    CreateThread,
    DeleteThread,
    AnchorDag,
}

// Define DidAuth type
#[derive(Debug, Clone)]
pub struct DidAuth(pub String);

// Check if a user has a specific permission
pub async fn check_permission(
    did: &str,
    permission: Permission,
    federation_id: Option<&str>,
    state: &AppState,
) -> Result<bool, StatusCode> {
    // For now, all authenticated users have all permissions
    // In a real implementation, this would check against credentials
    Ok(true)
} 