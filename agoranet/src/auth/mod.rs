use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{request::Parts, StatusCode, Request},
    middleware::Next,
    response::Response,
    body::Body,
};
use serde::{Serialize, Deserialize};
use serde_json::json;
use thiserror::Error;
use base64::{Engine as _, engine::general_purpose};
use crate::state::AppState;
use sqlx::PgPool;
use std::sync::Arc;
use chrono::Utc;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Authentication required")]
    NoAuth,
    
    #[error("Invalid DID format")]
    InvalidDid,
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Invalid auth token")]
    InvalidToken,
    
    #[error("Session expired")]
    SessionExpired,
    
    #[error("Permission denied")]
    PermissionDenied,
    
    #[error("Internal server error")]
    InternalError,
}

/// The DID auth middleware extracts and verifies auth tokens
pub async fn did_auth_middleware<B>(
    State(state): State<AppState>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    // Get the Authorization header
    let auth_header = request.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Validate the token format: "Bearer <token>"
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Verify the token
    let _did = verify_did_token(token, &state).await
        .map_err(|e| {
            tracing::warn!("Token verification failed: {}", e);
            StatusCode::UNAUTHORIZED
        })?;
    
    // Process the request
    Ok(next.run(request).await)
}

/// Represents a DID Credential for authentication
#[derive(Debug, Serialize, Deserialize)]
pub struct DidToken {
    pub sub: String,         // DID of the subject
    pub iss: String,         // DID of the issuer (often the same as sub for self-signed)
    pub exp: i64,            // Expiration timestamp
    pub iat: i64,            // Issued at timestamp
    pub jti: Option<String>, // JWT ID (optional)
    pub nonce: Option<String>, // Nonce for preventing replay attacks
}

async fn verify_did_token(token: &str, state: &AppState) -> Result<String, AuthError> {
    // Decode the token (JWT format)
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(AuthError::InvalidToken);
    }
    
    // Decode header and payload
    let payload_json = general_purpose::STANDARD_NO_PAD
        .decode(parts[1])
        .map_err(|_| AuthError::InvalidToken)?;
    
    let token_data: DidToken = serde_json::from_slice(&payload_json)
        .map_err(|_| AuthError::InvalidToken)?;
    
    // Extract DID from payload
    let did = &token_data.sub;
    
    if !did.starts_with("did:icn:") {
        return Err(AuthError::InvalidDid);
    }
    
    // Check token expiration
    let current_time = Utc::now().timestamp();
    
    if current_time > token_data.exp {
        return Err(AuthError::SessionExpired);
    }
    
    // In a production setting, we would also:
    // 1. Verify the signature using the public key from the DID
    // 2. Verify the nonce to prevent replay attacks
    // 3. Possibly check a revocation list
    
    Ok(did.to_string())
}

/// Helper function to generate an error response
pub fn auth_error_response(error: AuthError) -> Response {
    let body = json!({
        "error": error.to_string(),
    });
    
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

/// Extract DID from the request
#[derive(Clone)]
pub struct DidAuth(pub String);

// Implement extractor for DidAuth
#[async_trait]
impl<S> FromRequestParts<S> for DidAuth
where
    S: Send + Sync,
    S: std::ops::Deref<Target = AppState>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts.headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;
        
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(StatusCode::UNAUTHORIZED)?;
        
        let did = verify_did_token(token, state)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        
        Ok(DidAuth(did))
    }
}

/// Permission types for authorization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Permission {
    ReadThread,
    CreateThread,
    PostMessage,
    ReactToMessage,
    LinkCredential,
    ModerateContent,
}

/// CredentialRepository for checking credentials and permissions
pub struct CredentialRepository {
    pool: PgPool,
}

impl CredentialRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Check if a user has a valid credential of a specific type
    pub async fn has_valid_credential(&self, subject_did: &str, credential_type: &str) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) AS count
            FROM verified_credentials
            WHERE subject_did = $1 
              AND credential_type = $2
              AND (valid_until IS NULL OR valid_until > NOW())
            "#,
            subject_did,
            credential_type
        )
        .fetch_one(&self.pool)
        .await?
        .count
        .unwrap_or(0);
        
        Ok(count > 0)
    }
    
    /// Check if a user is the owner of a specific credential
    pub async fn is_credential_owner(&self, subject_did: &str, credential_cid: &str) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) AS count
            FROM verified_credentials
            WHERE subject_did = $1 AND credential_cid = $2
            "#,
            subject_did,
            credential_cid
        )
        .fetch_one(&self.pool)
        .await?
        .count
        .unwrap_or(0);
        
        Ok(count > 0)
    }
}

/// Check if the given DID has the requested permission
pub async fn check_permission(
    did: &str, 
    permission: Permission, 
    resource_id: Option<&str>,
    state: &Arc<AppState>
) -> Result<bool, AuthError> {
    // For basic permissions like reading threads, allow everyone
    match permission {
        Permission::ReadThread => return Ok(true),
        _ => {}
    }
    
    // For other permissions, check for required credentials
    let credential_repo = CredentialRepository::new(state.db_pool.clone());
    
    match permission {
        Permission::CreateThread => {
            // Allow any authenticated user to create threads for now
            Ok(true)
        },
        Permission::PostMessage => {
            // Allow any authenticated user to post messages for now
            Ok(true)
        },
        Permission::ReactToMessage => {
            // Allow any authenticated user to react to messages
            Ok(true)
        },
        Permission::LinkCredential => {
            // Only allow linking credentials you control or if you're a moderator
            if let Some(cred_id) = resource_id {
                let is_owner = credential_repo.is_credential_owner(did, cred_id).await
                    .map_err(|_| AuthError::InternalError)?;
                    
                if is_owner {
                    return Ok(true);
                }
            }
            
            // Check for moderator status
            credential_repo.has_valid_credential(did, "ModeratorCredential").await
                .map_err(|_| AuthError::InternalError)
        },
        Permission::ModerateContent => {
            // Only moderators can moderate content
            credential_repo.has_valid_credential(did, "ModeratorCredential").await
                .map_err(|_| AuthError::InternalError)
        },
        _ => Ok(false),
    }
} 