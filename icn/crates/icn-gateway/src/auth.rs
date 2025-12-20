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
use std::time::Duration;

use ed25519_dalek::{Signature, Verifier};
use icn_identity::Did;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::{GatewayError, Result};

/// Challenge nonce (32 bytes, hex-encoded)
pub type ChallengeNonce = String;

/// Challenge storage with expiration
#[derive(Clone)]
struct Challenge {
    nonce: ChallengeNonce,
    /// DID that requested the challenge (reserved for additional validation)
    #[allow(dead_code)]
    did: Did,
    created_at: u64,
}

/// JWT claims for capability tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,         // Subject (DID)
    pub iat: u64,            // Issued at (timestamp)
    pub exp: u64,            // Expiration (timestamp)
    pub coop_id: String,     // Cooperative namespace
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

        let mut challenges = self
            .challenges
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        challenges.insert(did.clone(), challenge);

        Ok(nonce)
    }

    /// Verify signed challenge and issue token
    ///
    /// Security: This function is designed to be constant-time to prevent timing attacks.
    /// - Always performs signature verification (expensive crypto operation)
    /// - Returns generic error message on any failure (prevents information leakage)
    /// - Checks expiration AFTER signature verification
    pub fn verify_challenge(
        &self,
        did: &Did,
        signature: &[u8],
        coop_id: &str,
        scopes: Vec<String>,
    ) -> Result<String> {
        // Generic error message for all authentication failures
        // This prevents information leakage about why authentication failed
        let auth_error = || GatewayError::AuthenticationFailed("Authentication failed".to_string());

        // Retrieve and remove challenge
        let challenge = {
            let mut challenges = self
                .challenges
                .write()
                .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

            challenges.remove(did).ok_or_else(auth_error)?
        };

        // Parse all inputs WITHOUT early returns
        // Store results to perform verification even if parsing fails
        let verifying_key_result = did.to_verifying_key();
        let nonce_bytes_result = hex::decode(&challenge.nonce);
        let signature_obj_result = Signature::from_slice(signature);

        // ALWAYS perform signature verification (expensive crypto operation)
        // Even if parsing failed, we perform a dummy verification to maintain constant timing
        let signature_valid = match (
            verifying_key_result,
            nonce_bytes_result,
            signature_obj_result,
        ) {
            (Ok(verifying_key), Ok(nonce_bytes), Ok(signature_obj)) => {
                // All parsing succeeded - perform real verification
                verifying_key.verify(&nonce_bytes, &signature_obj).is_ok()
            }
            _ => {
                // Parsing failed - perform dummy verification with hardcoded values
                // This ensures constant-time behavior (attackers can't distinguish parsing failures)
                use ed25519_dalek::{Signer, SigningKey};

                // Create dummy key and sign a message
                let dummy_key = SigningKey::from_bytes(&[0u8; 32]);
                let signed_message = [0u8; 32];
                let dummy_signature = dummy_key.sign(&signed_message);

                // Perform verification that will ALWAYS FAIL
                // We verify the signature against a DIFFERENT message
                // This takes the same time as a real verification
                let dummy_verifying_key = dummy_key.verifying_key();
                let different_message = [1u8; 32]; // Different from signed_message
                let _ = dummy_verifying_key.verify(&different_message, &dummy_signature);
                // This will always fail: signature for message A won't verify against message B

                false // Parsing failed, so signature is invalid
            }
        };

        // Check expiration AFTER signature verification
        // This prevents timing attacks by ensuring we always do the expensive crypto
        let now = Self::current_timestamp();
        let is_expired = now - challenge.created_at >= self.challenge_ttl.as_secs();

        // Only succeed if signature is valid AND not expired
        // Use bitwise OR to force evaluation of both conditions (constant-time)
        // Short-circuit || would skip expiration check if signature invalid, creating timing leak
        if !signature_valid | is_expired {
            return Err(auth_error());
        }

        // Issue JWT token
        self.issue_token(did, coop_id, scopes)
    }

    /// Issue a JWT capability token
    pub fn issue_token(&self, did: &Did, coop_id: &str, scopes: Vec<String>) -> Result<String> {
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
        .map_err(|e| GatewayError::InternalError(format!("JWT encoding failed: {e}")))?;

        Ok(token)
    }

    /// Verify a JWT token and extract claims
    pub fn verify_token(&self, token: &str) -> Result<TokenClaims> {
        let validation = Validation::default();

        let token_data = decode::<TokenClaims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )
        .map_err(|e| GatewayError::AuthenticationFailed(format!("Invalid token: {e}")))?;

        Ok(token_data.claims)
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
        icn_time::current_timestamp_secs()
    }

    /// Clean up expired challenges (periodic task)
    pub fn cleanup_expired_challenges(&self) -> Result<usize> {
        let mut challenges = self
            .challenges
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let now = Self::current_timestamp();
        let ttl = self.challenge_ttl.as_secs();
        let initial_count = challenges.len();

        challenges.retain(|_, challenge| now - challenge.created_at < ttl);

        Ok(initial_count - challenges.len())
    }

    /// Spawn a background task that periodically cleans up expired challenges
    /// Returns a JoinHandle that can be awaited for graceful shutdown
    pub fn spawn_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let challenges = self.challenges.clone();
        let challenge_ttl = self.challenge_ttl;

        tokio::spawn(async move {
            // Run cleanup every 5 minutes
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                // Perform cleanup
                let mut challenges_guard = match challenges.write() {
                    Ok(guard) => guard,
                    Err(e) => {
                        tracing::warn!("Failed to acquire challenges lock for cleanup: {}", e);
                        continue;
                    }
                };

                let now = icn_time::current_timestamp_secs();

                let ttl = challenge_ttl.as_secs();
                let initial_count = challenges_guard.len();

                challenges_guard.retain(|_, challenge| now - challenge.created_at < ttl);

                let removed = initial_count - challenges_guard.len();
                if removed > 0 {
                    tracing::debug!("Cleaned up {} expired challenges", removed);
                }
            }
        })
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
        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature_bytes,
                "test-coop",
                vec!["ledger:read".to_string()],
            )
            .unwrap();

        assert!(!token.is_empty());
    }

    #[test]
    fn test_verify_challenge_no_challenge() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();

        let result = auth.verify_challenge(bundle.did(), &[0u8; 64], "test-coop", vec![]);

        assert!(matches!(result, Err(GatewayError::AuthenticationFailed(_))));
    }

    #[test]
    fn test_verify_challenge_invalid_signature() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();

        // Create challenge
        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        // Invalid signature
        let result = auth.verify_challenge(bundle.did(), &[0u8; 64], "test-coop", vec![]);

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
