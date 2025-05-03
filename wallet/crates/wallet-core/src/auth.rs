use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Duration};
use crate::identity::IdentityWallet;
use crate::error::WalletResult;
use base64::{Engine, engine::general_purpose::URL_SAFE};

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthToken {
    pub did: String,
    pub exp: DateTime<Utc>,
    pub thread_scope: Option<String>,  // Optional thread ID this token is scoped to
    pub iat: DateTime<Utc>,           // Issued at timestamp
}

impl AuthToken {
    pub fn new(did: &str, expires_in_minutes: i64, thread_scope: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            did: did.to_string(),
            exp: now + Duration::minutes(expires_in_minutes),
            thread_scope,
            iat: now,
        }
    }
    
    pub fn encode_sign(&self, identity: &IdentityWallet) -> WalletResult<String> {
        // Serialize payload to JSON
        let payload = serde_json::to_string(&self)?;
        
        // Sign payload with identity's private key
        let signature = identity.sign(payload.as_bytes())?;
        
        // Base64-encode the payload and signature
        let encoded_payload = URL_SAFE.encode(payload);
        let encoded_signature = URL_SAFE.encode(&signature);
        
        // Format as: {base64_payload}.{base64_signature}
        Ok(format!("{}.{}", encoded_payload, encoded_signature))
    }
}

pub fn generate_auth_token(
    identity: &IdentityWallet,
    expires_in_minutes: i64,
    thread_scope: Option<String>
) -> WalletResult<String> {
    let token = AuthToken::new(&identity.did, expires_in_minutes, thread_scope);
    token.encode_sign(identity)
} 