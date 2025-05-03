use crate::error::{AgentError, AgentResult};
use wallet_core::identity::IdentityWallet;
use tokio::task;
use chrono::{Duration, Utc};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::json;

/// Generate an authentication token signed by the identity
pub async fn generate_auth_token(
    identity: &IdentityWallet,
    expires_in_minutes: i64,
    thread_scope: Option<String>
) -> AgentResult<String> {
    let identity = identity.clone();
    
    task::spawn_blocking(move || {
        // Create JWT-like token claims
        let now = Utc::now();
        let expiry = now + Duration::minutes(expires_in_minutes);
        
        // Create token payload
        let payload = json!({
            "sub": identity.did,
            "iat": now.timestamp(),
            "exp": expiry.timestamp(),
            "scope": thread_scope.unwrap_or_else(|| "global".to_string())
        });
        
        // Convert to string
        let payload_str = serde_json::to_string(&payload)
            .map_err(|e| AgentError::SerializationError(format!("Failed to serialize token payload: {}", e)))?;
        
        // Sign the payload
        let signature = identity.sign_message(payload_str.as_bytes());
        let signature_b64 = BASE64.encode(signature);
        
        // Create token format: base64(payload).signature
        let payload_b64 = BASE64.encode(payload_str.as_bytes());
        let token = format!("{}.{}", payload_b64, signature_b64);
        
        Ok(token)
    }).await?
} 