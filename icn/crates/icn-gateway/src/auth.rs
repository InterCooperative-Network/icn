//! Authentication and authorization
//!
//! DID-based authentication flow:
//! 1. Client requests challenge for their DID
//! 2. Server generates random nonce, stores temporarily
//! 3. Client signs nonce with DID keypair
//! 4. Client submits signed nonce
//! 5. Server verifies signature using DID public key
//! 6. Server issues JWT capability token
//!
//! # Hardware-Backed Signing Support
//!
//! This module supports both software and hardware-backed signing:
//!
//! - **Software keys**: Private key in memory, signs directly via `IdentityBundle::sign()`
//! - **Hardware keys**: Private key in HSM/TPM, signs via `DidSigner` backend
//!
//! The authentication flow is agnostic to key storage. Clients use `IdentityBundle::sign()`
//! which automatically routes to the appropriate signing method:
//! - For software keys: uses in-memory private key
//! - For hardware keys: delegates to `DidSigner` backend (HSM/TPM)
//!
//! ## Example (Software Key)
//!
//! ```rust,no_run
//! # use icn_identity::IdentityBundle;
//! # use icn_gateway::auth::AuthManager;
//! let bundle = IdentityBundle::generate().unwrap();
//! // Dev/demo only: enable self-service issuance with a caller-supplied coop_id.
//! // Production leaves this off (fail-closed) and binds coop authority via
//! // trusted issuance paths (invites/sessions/enrollment). See issue #2075.
//! let auth = AuthManager::new(b"secret".to_vec()).with_self_asserted_coop(true);
//!
//! // Get challenge
//! let nonce = auth.create_challenge(bundle.did()).unwrap();
//!
//! // Sign with software key (private key in memory)
//! let nonce_bytes = hex::decode(&nonce).unwrap();
//! let signature = bundle.sign(&nonce_bytes).unwrap();
//!
//! // Verify
//! let token = auth.verify_challenge(
//!     bundle.did(),
//!     &signature.to_bytes(),
//!     "test-coop",
//!     vec!["ledger:read".to_string()],
//! ).unwrap();
//! ```
//!
//! ## Example (Hardware Key)
//!
//! ```rust,no_run
//! # use icn_identity::{IdentityBundle, DidKey, SoftwareSigner};
//! # use icn_gateway::auth::AuthManager;
//! # use std::sync::Arc;
//! # use ed25519_dalek::SigningKey;
//! # use rand_core::OsRng;
//! // Dev/demo only: enable self-service issuance with a caller-supplied coop_id.
//! // Production leaves this off (fail-closed) and binds coop authority via
//! // trusted issuance paths (invites/sessions/enrollment). See issue #2075.
//! let auth = AuthManager::new(b"secret".to_vec()).with_self_asserted_coop(true);
//!
//! // Hardware-backed key (private key in HSM/TPM)
//! # let sk = SigningKey::generate(&mut OsRng);
//! # let vk = sk.verifying_key();
//! let did_key = DidKey::from_hardware(
//!     vk,
//!     "pkcs11".to_string(),
//!     "slot=0".to_string(),
//! );
//!
//! // Create signer backend for hardware
//! # let software_key = DidKey::from_software_bytes(sk.to_bytes(), vk.to_bytes()).unwrap();
//! # let signer = Arc::new(SoftwareSigner::new(software_key).unwrap());
//! let bundle = IdentityBundle::from_did_key_with_signer(did_key, Some(signer)).unwrap();
//!
//! // Same authentication flow - signing is abstracted
//! let nonce = auth.create_challenge(bundle.did()).unwrap();
//! let nonce_bytes = hex::decode(&nonce).unwrap();
//! let signature = bundle.sign(&nonce_bytes).unwrap(); // Routes to hardware signer
//!
//! let token = auth.verify_challenge(
//!     bundle.did(),
//!     &signature.to_bytes(),
//!     "test-coop",
//!     vec!["ledger:read".to_string()],
//! ).unwrap();
//! ```

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

    /// Optional canonical entity context (#2080 PR1, RFC-0018 migration rail).
    ///
    /// The typed `EntityId` string (`entity:icn:<type>:<slug>`) the token is bound
    /// to. **NON-ENFORCING in this PR**: no request guard consults it yet —
    /// `require_coop_access` still decides solely on `coop_id`. It may be populated
    /// only by a TRUSTED issuance path; the dev/self-asserted path never sets it.
    /// Optional + skipped-when-absent so pre-#2080 tokens still decode and minted
    /// tokens stay byte-compatible with the old shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,

    /// Optional entity type (`individual`/`cooperative`/`community`/`federation`)
    /// derived from [`TokenClaims::entity_id`] at mint time. Same non-enforcing,
    /// back-compatible contract as `entity_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
}

/// Authentication manager
pub struct AuthManager {
    challenges: Arc<RwLock<HashMap<Did, Challenge>>>,
    jwt_secret: Vec<u8>,
    challenge_ttl: Duration,
    token_ttl: Duration,
    /// Whether the challenge/verify flow may mint a token carrying a
    /// **caller-supplied** `coop_id` (see [`AuthManager::verify_challenge`]).
    ///
    /// SECURITY (issue #2075): DID ownership proves only that the caller holds a
    /// key — it does **not** authorize that DID to act for an arbitrary
    /// cooperative. When this is `false` (the safe default), `verify_challenge`
    /// fails closed instead of binding self-asserted coop authority into a token.
    /// It may be enabled only for dev/demo deployments via
    /// [`AuthManager::with_self_asserted_coop`]. Production binds coop authority
    /// through trusted issuance paths (invites/sessions/enrollment) and,
    /// ultimately, RFC-0018's membership/entity binding.
    allow_self_asserted_coop: bool,
}

/// Canonical derivation of the HMAC signing-key bytes from the configured
/// gateway secret **string**: its raw UTF-8 bytes, verbatim.
///
/// SECURITY / correctness (follow-up to #2396): the configured secret is treated
/// as an opaque passphrase and keyed directly — it is **never** hex- or
/// base64-decoded. Every component that signs or verifies gateway JWTs (the
/// daemon at `icn-core` `init_gateway`, and the `icnctl --local-mint`
/// trusted-local issuer) MUST derive the key through this one function, or a
/// token minted by one silently fails verification under the other. A review of
/// #2396 caught exactly that: a co-issuer that `hex::decode`d a hex-looking
/// secret produced tokens the daemon rejected. Centralizing here makes that
/// divergence impossible to reintroduce.
pub fn signing_key_bytes(secret: &str) -> Vec<u8> {
    secret.as_bytes().to_vec()
}

impl AuthManager {
    /// Create new auth manager.
    ///
    /// Self-asserted cooperative authority via the challenge/verify flow is
    /// **disabled by default** (fail-closed); enable it for dev/demo only with
    /// [`AuthManager::with_self_asserted_coop`].
    pub fn new(jwt_secret: Vec<u8>) -> Self {
        Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret,
            challenge_ttl: Duration::from_secs(300), // 5 minutes
            token_ttl: Duration::from_secs(3600),    // 1 hour
            allow_self_asserted_coop: false,
        }
    }

    /// Construct an [`AuthManager`] from the configured secret **string**, keying
    /// HMAC with [`signing_key_bytes`] (its raw UTF-8 bytes, verbatim).
    ///
    /// This is the single canonical constructor for both the gateway daemon and
    /// the `icnctl --local-mint` trusted-local issuer, so the two can never drift
    /// in how they turn the secret string into signing-key bytes (see
    /// [`signing_key_bytes`]).
    pub fn from_secret_string(secret: &str) -> Self {
        Self::new(signing_key_bytes(secret))
    }

    /// Enable (or disable) self-service capability-token issuance via the
    /// challenge/verify flow, where the caller chooses its own `coop_id`.
    ///
    /// SECURITY (issue #2075): enabling this permits a DID that has proven only
    /// key ownership to mint a token carrying an **unverified** `coop_id`, which
    /// `require_coop_access` then trusts. It MUST be restricted to dev/demo
    /// deployments. The gateway wires this from the `ICN_DEV_MODE` posture; it is
    /// off in production, where coop authority is bound through trusted issuance
    /// paths (invites/sessions/enrollment) until RFC-0018's membership/entity
    /// binding lands.
    pub fn with_self_asserted_coop(mut self, enabled: bool) -> Self {
        self.allow_self_asserted_coop = enabled;
        self
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

        // SECURITY (issue #2075): identity is now proven, but the caller-supplied
        // `coop_id` is NOT. DID ownership does not authorize this DID to act for
        // an arbitrary cooperative, and `require_coop_access` trusts whatever
        // `coop_id` lands in the token. Fail closed: refuse to bind self-asserted
        // coop authority unless explicitly enabled for a dev/demo deployment.
        // Production binds coop authority via trusted issuance paths
        // (invites/sessions/enrollment) and, ultimately, RFC-0018's
        // membership/entity binding. AuthorizationFailed (403), not
        // AuthenticationFailed (401): the identity check passed; the coop claim
        // is what is unauthorized.
        if !self.allow_self_asserted_coop {
            return Err(GatewayError::AuthorizationFailed(
                "self-asserted cooperative authority is not permitted".to_string(),
            ));
        }

        // Issue JWT token
        self.issue_token(did, coop_id, scopes)
    }

    /// Issue a JWT capability token (legacy shape: no typed entity context).
    ///
    /// Exactly equivalent to [`AuthManager::issue_entity_token`] with
    /// `entity_id = None`: the minted token omits the `entity_id`/`entity_type`
    /// claims entirely. Existing callers keep their behavior unchanged.
    pub fn issue_token(&self, did: &Did, coop_id: &str, scopes: Vec<String>) -> Result<String> {
        self.issue_entity_token(did, coop_id, None, scopes)
    }

    /// Issue a JWT capability token, optionally carrying typed entity context.
    ///
    /// #2080 PR1 (RFC-0018 migration rail). When `entity_id` is `Some`, the minted
    /// token also carries the canonical `EntityId` string and a derived
    /// `entity_type`, ALONGSIDE the legacy `coop_id`.
    ///
    /// SECURITY: this context is **non-enforcing** — no request guard consults it
    /// yet (`require_coop_access` still decides on `coop_id`). It must be populated
    /// only from a TRUSTED issuance path. This PR wires no production caller to
    /// pass `Some(..)`; in particular the dev/self-asserted path
    /// (`verify_challenge` → `issue_token`) always passes `None`, so a
    /// self-asserting DID can never fabricate typed entity authority that later
    /// enforcement work would trust.
    pub fn issue_entity_token(
        &self,
        did: &Did,
        coop_id: &str,
        entity_id: Option<icn_entity::EntityId>,
        scopes: Vec<String>,
    ) -> Result<String> {
        let now = Self::current_timestamp();
        // Derive the entity type from the (canonical) EntityId so the two claims
        // can never disagree; both are absent together when entity_id is None.
        let entity_type = entity_id.as_ref().map(|id| id.entity_type().to_string());
        let claims = TokenClaims {
            sub: did.to_string(),
            iat: now,
            exp: now + self.token_ttl.as_secs(),
            coop_id: coop_id.to_string(),
            scopes,
            entity_id: entity_id.map(|id| id.to_string()),
            entity_type,
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
        let mut rng = rand::rng();
        let nonce_bytes: [u8; 32] = rng.random();
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
        // Dev/demo self-service issuance: caller supplies its own coop_id (#2075).
        let auth = AuthManager::new(b"test_secret".to_vec()).with_self_asserted_coop(true);
        let bundle = IdentityBundle::generate().unwrap();

        // Create challenge
        let nonce = auth.create_challenge(bundle.did()).unwrap();

        // Sign nonce
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce (software key or hardware signer configured)");
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

    #[test]
    fn test_verify_challenge_success_with_hardware_backed_bundle() {
        use ed25519_dalek::SigningKey;
        use icn_identity::{DidKey, IdentityBundle, SoftwareSigner};
        // rand_core 0.6 OsRng: ed25519-dalek 2.x pins rand_core 0.6
        use rand_core::OsRng;
        use std::sync::Arc;

        // Dev/demo self-service issuance: caller supplies its own coop_id (#2075).
        let auth = AuthManager::new(b"test_secret".to_vec()).with_self_asserted_coop(true);

        // Create a software signing key (for the backend), but present it as "hardware" to the bundle
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let verifying_key = signing_key.verifying_key();

        // Signer backend uses software key material (stand-in for PKCS#11/TPM signer)
        let software_did_key =
            DidKey::from_software_bytes(secret_bytes, verifying_key.to_bytes()).unwrap();
        let signer = Arc::new(SoftwareSigner::new(software_did_key).unwrap());

        // Bundle DID key is "hardware" (no secret in memory at the bundle level)
        let hardware_key =
            DidKey::from_hardware(verifying_key, "pkcs11".to_string(), "slot=0".to_string());

        let bundle = IdentityBundle::from_did_key_with_signer(hardware_key, Some(signer)).unwrap();

        // Create challenge
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();

        // This MUST work via signer (and would fail if code tried keypair())
        let signature = bundle.sign(&nonce_bytes).unwrap();

        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature.to_bytes(),
                "test-coop",
                vec!["ledger:read".to_string()],
            )
            .unwrap();

        assert!(!token.is_empty());
    }

    /// SECURITY (#2075): with the safe default (`AuthManager::new`), a DID that
    /// has proven only key ownership must NOT be able to mint a token for a
    /// self-asserted `coop_id`. Issuance fails closed with AuthorizationFailed
    /// even though the signature is valid — identity is proven, but the coop
    /// claim is not authorized.
    #[test]
    fn test_verify_challenge_self_asserted_coop_rejected_by_default() {
        let auth = AuthManager::new(b"test_secret".to_vec()); // fail-closed default
        let bundle = IdentityBundle::generate().unwrap();

        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce");

        let result = auth.verify_challenge(
            bundle.did(),
            &signature.to_bytes(),
            "some-coop",
            vec!["ledger:write".to_string()],
        );

        assert!(
            matches!(result, Err(GatewayError::AuthorizationFailed(_))),
            "self-asserted coop authority must be rejected when not explicitly enabled",
        );
    }

    /// The dev/demo bypass is opt-in and explicit: with
    /// `with_self_asserted_coop(true)` the same flow succeeds and the token
    /// carries the requested coop_id. This documents the intentional dev path.
    #[test]
    fn test_verify_challenge_self_asserted_coop_allowed_in_dev() {
        let auth = AuthManager::new(b"test_secret".to_vec()).with_self_asserted_coop(true);
        let bundle = IdentityBundle::generate().unwrap();

        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce");

        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature.to_bytes(),
                "some-coop",
                vec!["ledger:write".to_string()],
            )
            .expect("dev mode permits self-service issuance");
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.coop_id, "some-coop");
    }

    // ------------------------------------------------------------------------
    // #2080 PR1 — optional typed entity context on the token claim.
    //
    // Non-enforcing migration rail (RFC-0018). These tests pin: back-compat (old
    // tokens decode), the legacy mint carries no entity context, the new typed
    // mint round-trips, and the dev/self-asserted path can NEVER mint typed
    // entity authority.
    // ------------------------------------------------------------------------

    /// Back-compat: a token minted before the entity-context fields existed (no
    /// `entity_id`/`entity_type` keys) must still deserialize, both fields
    /// defaulting to `None`. Pins the `#[serde(default)]` contract.
    #[test]
    fn token_claims_without_entity_fields_deserialize_as_none() {
        let legacy_json = r#"{
            "sub": "did:icn:zlegacy",
            "iat": 1000000000,
            "exp": 9999999999,
            "coop_id": "food-coop",
            "scopes": ["ledger:read"]
        }"#;
        let claims: TokenClaims = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(claims.entity_id, None);
        assert_eq!(claims.entity_type, None);
    }

    /// The legacy `issue_token` mint carries no typed entity context: the verified
    /// claims have `entity_id`/`entity_type` == `None`, and (via
    /// `skip_serializing_if`) the encoded claim omits the keys entirely, so minted
    /// tokens stay byte-compatible with the pre-#2080 shape.
    #[test]
    fn issue_token_mints_without_entity_context() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();
        let token = auth
            .issue_token(bundle.did(), "food-coop", vec!["ledger:read".to_string()])
            .unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.entity_id, None);
        assert_eq!(claims.entity_type, None);
        // skip_serializing_if: a None-context claim omits the keys entirely.
        let serialized = serde_json::to_string(&claims).unwrap();
        assert!(!serialized.contains("entity_id"));
        assert!(!serialized.contains("entity_type"));
    }

    /// The typed mint round-trips: `issue_entity_token` with `Some(EntityId)` binds
    /// the canonical `entity_id` string and a derived `entity_type`, while leaving
    /// the legacy `coop_id` and `sub` untouched.
    #[test]
    fn issue_entity_token_round_trips_typed_entity_context() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();
        let entity = icn_entity::EntityId::cooperative("food-coop").unwrap();
        let token = auth
            .issue_entity_token(
                bundle.did(),
                "food-coop",
                Some(entity),
                vec!["ledger:read".to_string()],
            )
            .unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(
            claims.entity_id.as_deref(),
            Some("entity:icn:cooperative:food-coop")
        );
        assert_eq!(claims.entity_type.as_deref(), Some("cooperative"));
        // Legacy context is preserved unchanged alongside the typed context.
        assert_eq!(claims.coop_id, "food-coop");
        assert_eq!(claims.sub, bundle.did().to_string());
    }

    /// `issue_entity_token` with `None` is exactly the legacy mint (no entity
    /// context), so `issue_token` can delegate to it with no behavior change.
    #[test]
    fn issue_entity_token_with_none_matches_legacy_mint() {
        let auth = AuthManager::new(b"test_secret".to_vec());
        let bundle = IdentityBundle::generate().unwrap();
        let token = auth
            .issue_entity_token(
                bundle.did(),
                "food-coop",
                None,
                vec!["ledger:read".to_string()],
            )
            .unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.entity_id, None);
        assert_eq!(claims.entity_type, None);
        assert_eq!(claims.coop_id, "food-coop");
    }

    /// Safety rule (#2080 PR1): the dev/self-asserted issuance path may still mint a
    /// flat `coop_id` token where enabled, but it must NEVER mint typed entity
    /// authority. `verify_challenge` routes through `issue_token`, so the entity
    /// context is always `None` — a self-asserting DID cannot fabricate an
    /// `entity_id` the later enforcement work would trust.
    #[test]
    fn dev_self_asserted_path_never_mints_entity_context() {
        let auth = AuthManager::new(b"test_secret".to_vec()).with_self_asserted_coop(true);
        let bundle = IdentityBundle::generate().unwrap();
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle
            .sign(&nonce_bytes)
            .expect("test setup: bundle must be able to sign nonce");
        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature.to_bytes(),
                "some-coop",
                vec!["ledger:write".to_string()],
            )
            .expect("dev mode permits self-service issuance");
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.coop_id, "some-coop");
        assert_eq!(
            claims.entity_id, None,
            "dev/self-asserted path must not mint typed entity authority"
        );
        assert_eq!(
            claims.entity_type, None,
            "dev/self-asserted path must not mint typed entity authority"
        );
    }
}
