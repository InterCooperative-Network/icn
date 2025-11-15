//! Authentication and authorization
//!
//! DID-based authentication flow:
//! 1. Client requests challenge for their DID
//! 2. Server generates random nonce, stores temporarily
//! 3. Client signs nonce with DID keypair
//! 4. Client submits signed nonce
//! 5. Server verifies signature using DID public key
//! 6. Server issues JWT capability token

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use icn_identity::Did;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use ed25519_dalek::{Signature, Verifier};

use crate::error::{GatewayError, Result};

/// Challenge nonce (32 bytes, hex-encoded)
pub type ChallengeNonce = String;

/// Challenge storage with expiration
#[derive(Clone)]
struct Challenge {
    nonce: ChallengeNonce,
    did: Did,
    created_at: u64,
}

/// JWT claims for capability tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,      // Subject (DID)
    pub iat: u64,         // Issued at (timestamp)
    pub exp: u64,         // Expiration (timestamp)
    pub coop_id: String,  // Cooperative namespace
    pub scopes: Vec<String>, // Capabilities (e.g., "ledger:read", "ledger:write")
}

/// Authentication manager
pub struct AuthManager {
    challenges: Arc<RwLock<HashMap<Did, Challenge>>>,
    jwt_secret: Vec<u8>,
    challenge_ttl: Duration,
    token_ttl: Duration,
}

impl AuthManager {
    /// Create new auth manager
    pub fn new(jwt_secret: Vec<u8>) -> Self {
        Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret,
            challenge_ttl: Duration::from_secs(300), // 5 minutes
            token_ttl: Duration::from_secs(3600),    // 1 hour
        }
    }

    /// Generate a challenge for a DID
    pub fn create_challenge(&self, did: &Did) -> Result<ChallengeNonce> {
        let nonce = self.generate_nonce();
        let challenge = Challenge {
            nonce: nonce.clone(),
            did: did.clone(),
            created_at: Self::current_timestamp(),
        };

        let mut challenges = self.challenges.write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {}", e)))?;

        challenges.insert(did.clone(), challenge);

        Ok(nonce)
    }

    /// Verify signed challenge and issue token
    pub fn verify_challenge(
        &self,
        did: &Did,
        signature: &[u8],
        coop_id: &str,
        scopes: Vec<String>,
    ) -> Result<String> {
        // Retrieve and remove challenge
        let challenge = {
            let mut challenges = self.challenges.write()
                .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {}", e)))?;

            challenges.remove(did)
                .ok_or_else(|| GatewayError::AuthenticationFailed(
                    "No challenge found for DID".to_string()
                ))?
        };

        // Check expiration
        let now = Self::current_timestamp();
        if now - challenge.created_at >= self.challenge_ttl.as_secs() {
            return Err(GatewayError::AuthenticationFailed(
                "Challenge expired".to_string()
            ));
        }

        // Verify signature
        let verifying_key = did.to_verifying_key()
            .map_err(|e| GatewayError::AuthenticationFailed(
                format!("Invalid DID: {}", e)
            ))?;

        let nonce_bytes = hex::decode(&challenge.nonce)
            .map_err(|e| GatewayError::InternalError(
                format!("Invalid nonce encoding: {}", e)
            ))?;

        let signature_obj = Signature::from_slice(signature)
            .map_err(|e| GatewayError::AuthenticationFailed(
                format!("Invalid signature format: {}", e)
            ))?;

        verifying_key.verify(&nonce_bytes, &signature_obj)
            .map_err(|_| GatewayError::AuthenticationFailed(
                "Signature verification failed".to_string()
            ))?;

        // Issue JWT token
        self.issue_token(did, coop_id, scopes)
    }

    /// Issue a JWT capability token
    fn issue_token(&self, did: &Did, coop_id: &str, scopes: Vec<String>) -> Result<String> {
        let now = Self::current_timestamp();
        let claims = TokenClaims {
            sub: did.to_string(),
            iat: now,
            exp: now + self.token_ttl.as_secs(),
            coop_id: coop_id.to_string(),
            scopes,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| GatewayError::InternalError(format!("JWT encoding failed: {}", e)))?;

        Ok(token)
    }

    /// Generate cryptographically random nonce (32 bytes, hex-encoded)
    fn generate_nonce(&self) -> ChallengeNonce {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let nonce_bytes: [u8; 32] = rng.gen();
        hex::encode(nonce_bytes)
    }

    /// Get current Unix timestamp
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Clean up expired challenges (periodic task)
    pub fn cleanup_expired_challenges(&self) -> Result<usize> {
        let mut challenges = self.challenges.write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {}", e)))?;

        let now = Self::current_timestamp();
        let ttl = self.challenge_ttl.as_secs();
        let initial_count = challenges.len();

        challenges.retain(|_, challenge| {
            now - challenge.created_at < ttl
        });

        Ok(initial_count - challenges.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;

    #[test]
    fn test_create_challenge() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();

        let nonce = auth.create_challenge(bundle.did()).unwrap();
        assert_eq!(nonce.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_verify_challenge_success() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();

        // Create challenge
        let nonce = auth.create_challenge(bundle.did()).unwrap();

        // Sign nonce
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        // Verify and get token
        let token = auth.verify_challenge(
            bundle.did(),
            &signature_bytes,
            "test-coop",
            vec!["ledger:read".to_string()],
        ).unwrap();

        assert!(!token.is_empty());
    }

    #[test]
    fn test_verify_challenge_no_challenge() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();

        let result = auth.verify_challenge(
            bundle.did(),
            &[0u8; 64],
            "test-coop",
            vec![],
        );

        assert!(matches!(result, Err(GatewayError::AuthenticationFailed(_))));
    }

    #[test]
    fn test_verify_challenge_invalid_signature() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();

        // Create challenge
        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        // Invalid signature
        let result = auth.verify_challenge(
            bundle.did(),
            &[0u8; 64],
            "test-coop",
            vec![],
        );

        assert!(matches!(result, Err(GatewayError::AuthenticationFailed(_))));
    }

    #[test]
    fn test_cleanup_expired_challenges() {
        let mut auth = AuthManager::new(b"test_secret".to_vec());
        auth.challenge_ttl = Duration::from_secs(0); // Instant expiration

        let bundle = IdentityBundle::generate().unwrap();
        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        std::thread::sleep(Duration::from_millis(10));

        let removed = auth.cleanup_expired_challenges().unwrap();
        assert_eq!(removed, 1);
    }
}
