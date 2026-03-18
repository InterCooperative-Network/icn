//! RPC Authentication
//!
//! DID-based authentication for JSON-RPC endpoints.
//! Uses the same JWT flow as the Gateway API for consistency.
//!
//! Flow:
//! 1. Client calls `auth.challenge` with their DID
//! 2. Server returns a random nonce
//! 3. Client signs nonce with their DID keypair
//! 4. Client calls `auth.verify` with signature
//! 5. Server issues JWT token
//! 6. Client includes `Authorization: Bearer <token>` header on subsequent requests

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

use ed25519_dalek::{Signature, Verifier};
use icn_identity::Did;
use icn_store::Store;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use metrics::{counter, gauge, histogram};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Authentication error types
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

    #[error("Token has been revoked")]
    TokenRevoked,

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Challenge storage with expiration
#[derive(Clone)]
struct Challenge {
    nonce: String,
    #[allow(dead_code)]
    did: Did,
    created_at: u64,
}

/// JWT claims for RPC tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTokenClaims {
    /// Subject (DID string)
    pub sub: String,
    /// JWT ID - unique identifier for this token (enables revocation)
    pub jti: String,
    /// Issued at (Unix timestamp)
    pub iat: u64,
    /// Expiration (Unix timestamp)
    pub exp: u64,
    /// Scopes/permissions
    pub scopes: Vec<String>,
    /// Cooperative ID (optional, for coop isolation and compute task attribution)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coop_id: Option<String>,
}

impl RpcTokenClaims {
    /// Check if the token has a specific scope
    pub fn has_scope(&self, scope: &str) -> bool {
        // Wildcard scope grants everything
        if self.scopes.contains(&"*".to_string()) {
            return true;
        }

        // Check exact match
        if self.scopes.contains(&scope.to_string()) {
            return true;
        }

        // Check namespace wildcard (e.g., "compute:*" grants "compute:submit")
        let parts: Vec<&str> = scope.split(':').collect();
        if parts.len() == 2 {
            let wildcard = format!("{}:*", parts[0]);
            if self.scopes.contains(&wildcard) {
                return true;
            }
        }

        false
    }
}

/// Cleanup interval for expired challenges (5 minutes)
const CLEANUP_INTERVAL: Duration = Duration::from_secs(300);

/// Storage key prefix for revoked tokens
const REVOKED_TOKEN_PREFIX: &[u8] = b"auth:revoked:";

/// Storage key prefix for challenge nonces
const CHALLENGE_PREFIX: &[u8] = b"auth:challenge:";

/// Revoked token entry stored in persistent storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RevokedToken {
    /// The JWT ID that was revoked
    jti: String,
    /// The DID that owned this token
    subject: String,
    /// When the token was revoked (Unix timestamp)
    revoked_at: u64,
    /// When the original token would have expired (Unix timestamp)
    /// Used for cleanup - no need to store revocations past token expiry
    original_expiry: u64,
    /// Reason for revocation (optional)
    reason: Option<String>,
}

/// Persistent challenge entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentChallenge {
    /// The challenge nonce
    nonce: String,
    /// The DID that requested this challenge
    did: String,
    /// When the challenge was created (Unix timestamp)
    created_at: u64,
}

/// Token Revocation List with persistence
///
/// Provides fast lookups for revoked tokens using both in-memory cache
/// and persistent storage for durability across restarts.
pub struct TokenRevocationList<S: Store> {
    /// In-memory cache of revoked token IDs for fast lookup
    cache: Arc<RwLock<std::collections::HashSet<String>>>,
    /// Persistent storage backend
    store: Arc<S>,
}

impl<S: Store> TokenRevocationList<S> {
    /// Create a new Token Revocation List with the given store
    pub fn new(store: Arc<S>) -> Result<Self, AuthError> {
        let trl = Self {
            cache: Arc::new(RwLock::new(std::collections::HashSet::new())),
            store,
        };

        // Load existing revocations into cache
        trl.load_cache()?;

        Ok(trl)
    }

    /// Load revoked tokens from persistent storage into memory cache
    fn load_cache(&self) -> Result<(), AuthError> {
        let start = Instant::now();

        let entries = self
            .store
            .scan(REVOKED_TOKEN_PREFIX)
            .map_err(|e| AuthError::InternalError(format!("Failed to scan revoked tokens: {e}")))?;

        let mut cache = self
            .cache
            .write()
            .map_err(|e| AuthError::InternalError(format!("Lock poisoned: {e}")))?;

        let now = Self::current_timestamp()?;
        let mut scanned = 0;

        for (_key, value) in entries {
            scanned += 1;
            if let Ok(revoked) = serde_json::from_slice::<RevokedToken>(&value) {
                // Only cache tokens that haven't expired yet
                if revoked.original_expiry > now {
                    cache.insert(revoked.jti);
                }
            }
        }

        // Record load metrics
        let duration = start.elapsed();
        histogram!("icn_rpc_revocation_load_duration_seconds").record(duration.as_secs_f64());
        gauge!("icn_rpc_revoked_tokens_total").set(cache.len() as f64);

        tracing::info!(
            loaded = cache.len(),
            scanned = scanned,
            duration_ms = duration.as_millis(),
            "Loaded token revocation list from storage"
        );

        Ok(())
    }

    /// Check if a token ID has been revoked
    ///
    /// Returns `false` on cache lock errors (fail-open). This is intentional:
    /// - Lock poisoning is extremely rare (requires a panic while holding the lock)
    /// - Failing closed (rejecting all tokens) creates a DoS vector
    /// - The persistent storage is the source of truth; cache is only a performance optimization
    /// - Better to allow one potentially-revoked token through than reject all valid tokens
    pub fn is_revoked(&self, jti: &str) -> bool {
        counter!("icn_rpc_revocation_checks_total").increment(1);

        let is_revoked = self
            .cache
            .read()
            .map(|cache| cache.contains(jti))
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, jti = %jti, "Cache lock poisoned in is_revoked, failing open");
                false
            });

        if is_revoked {
            counter!("icn_rpc_revocation_hits_total").increment(1);
        }

        is_revoked
    }

    /// Revoke a token
    pub fn revoke(
        &self,
        jti: &str,
        subject: &str,
        original_expiry: u64,
        reason: Option<String>,
    ) -> Result<(), AuthError> {
        let start = Instant::now();
        let now = Self::current_timestamp()?;

        // Don't revoke already-expired tokens
        if original_expiry <= now {
            return Ok(());
        }

        let revoked = RevokedToken {
            jti: jti.to_string(),
            subject: subject.to_string(),
            revoked_at: now,
            original_expiry,
            reason,
        };

        // Store persistently
        let key = Self::make_key(jti);
        let value = serde_json::to_vec(&revoked).map_err(|e| {
            counter!("icn_rpc_revocation_errors_total").increment(1);
            AuthError::InternalError(format!("Failed to serialize revoked token: {e}"))
        })?;

        self.store.put(&key, &value).map_err(|e| {
            counter!("icn_rpc_revocation_errors_total").increment(1);
            AuthError::InternalError(format!("Failed to store revoked token: {e}"))
        })?;

        // Update cache
        let mut cache = self.cache.write().map_err(|e| {
            counter!("icn_rpc_revocation_errors_total").increment(1);
            AuthError::InternalError(format!("Lock poisoned: {e}"))
        })?;

        cache.insert(jti.to_string());

        // Update gauge with current count
        gauge!("icn_rpc_revoked_tokens_total").set(cache.len() as f64);

        // Record operation duration
        histogram!("icn_rpc_revocation_add_duration_seconds").record(start.elapsed().as_secs_f64());

        tracing::info!(jti = %jti, subject = %subject, "Token revoked");

        Ok(())
    }

    /// Clean up expired revocations from storage
    pub fn cleanup_expired(&self) -> Result<usize, AuthError> {
        let start = Instant::now();

        let entries = self.store.scan(REVOKED_TOKEN_PREFIX).map_err(|e| {
            counter!("icn_rpc_revocation_errors_total").increment(1);
            AuthError::InternalError(format!("Failed to scan revoked tokens: {e}"))
        })?;

        let now = Self::current_timestamp()?;
        let mut cleaned = 0;
        let mut scanned = 0;
        let mut to_remove = Vec::new();

        for (key, value) in entries {
            scanned += 1;
            if let Ok(revoked) = serde_json::from_slice::<RevokedToken>(&value) {
                if revoked.original_expiry <= now {
                    // Token has expired, remove revocation record
                    self.store.delete(&key).map_err(|e| {
                        counter!("icn_rpc_revocation_errors_total").increment(1);
                        AuthError::InternalError(format!(
                            "Failed to delete expired revocation: {e}"
                        ))
                    })?;
                    cleaned += 1;
                    to_remove.push(revoked.jti);
                }
            }
        }

        // Batch remove from cache - single lock acquisition instead of O(n)
        if !to_remove.is_empty() {
            let mut cache = self.cache.write().map_err(|e| {
                counter!("icn_rpc_revocation_errors_total").increment(1);
                AuthError::InternalError(format!("Lock poisoned: {e}"))
            })?;
            for jti in to_remove {
                cache.remove(&jti);
            }
        }

        // Record cleanup metrics
        let duration = start.elapsed();
        histogram!("icn_rpc_revocation_cleanup_duration_seconds").record(duration.as_secs_f64());
        counter!("icn_rpc_revocation_cleanup_total").increment(cleaned as u64);
        gauge!("icn_rpc_revocation_scan_entries").set(scanned as f64);

        // Always update the revoked tokens gauge to reflect current cache size
        if let Ok(cache) = self.cache.read() {
            gauge!("icn_rpc_revoked_tokens_total").set(cache.len() as f64);
        }

        Ok(cleaned)
    }

    /// Get count of revoked tokens
    pub fn count(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }

    fn make_key(jti: &str) -> Vec<u8> {
        let mut key = Vec::with_capacity(REVOKED_TOKEN_PREFIX.len() + jti.len());
        key.extend_from_slice(REVOKED_TOKEN_PREFIX);
        key.extend_from_slice(jti.as_bytes());
        key
    }

    fn current_timestamp() -> Result<u64, AuthError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .map_err(|e| AuthError::InternalError(format!("System clock error: {e}")))
    }
}

/// RPC Authentication Manager
pub struct RpcAuthManager<S: Store = icn_store::SledStore> {
    challenges: Arc<RwLock<HashMap<Did, Challenge>>>,
    jwt_secret: Vec<u8>,
    challenge_ttl: Duration,
    token_ttl: Duration,
    /// If true, authentication is required. If false, all requests are allowed (dev mode).
    enabled: bool,
    /// Token revocation list (optional, requires persistent storage)
    revocation_list: Option<Arc<TokenRevocationList<S>>>,
    /// Persistent storage for challenges (optional)
    store: Option<Arc<S>>,
}

impl<S: Store + 'static> RpcAuthManager<S> {
    /// Create a new auth manager with persistent storage
    ///
    /// This enables token revocation and challenge persistence across restarts.
    ///
    /// # Arguments
    /// * `jwt_secret` - Secret for signing JWTs (should be at least 32 bytes)
    /// * `enabled` - If false, authentication is bypassed (for development only)
    /// * `store` - Persistent storage backend for challenges and revocations
    pub fn new_with_store(
        jwt_secret: Vec<u8>,
        enabled: bool,
        store: Arc<S>,
    ) -> Result<Self, AuthError> {
        let revocation_list = TokenRevocationList::new(Arc::clone(&store))?;

        let mut manager = Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret,
            challenge_ttl: Duration::from_secs(300), // 5 minutes
            token_ttl: Duration::from_secs(86400),   // 24 hours
            enabled,
            revocation_list: Some(Arc::new(revocation_list)),
            store: Some(store),
        };

        // Load persisted challenges
        manager.load_challenges()?;

        Ok(manager)
    }

    /// Check if authentication is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Check if token revocation is enabled
    pub fn has_revocation_support(&self) -> bool {
        self.revocation_list.is_some()
    }

    /// Load challenges from persistent storage
    fn load_challenges(&mut self) -> Result<(), AuthError> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()),
        };

        let entries = store
            .scan(CHALLENGE_PREFIX)
            .map_err(|e| AuthError::InternalError(format!("Failed to scan challenges: {e}")))?;

        let mut challenges = self
            .challenges
            .write()
            .map_err(|e| AuthError::InternalError(format!("Lock poisoned: {e}")))?;

        let now = Self::current_timestamp()?;

        for (_key, value) in entries {
            if let Ok(persistent) = serde_json::from_slice::<PersistentChallenge>(&value) {
                // Only load non-expired challenges
                if now.saturating_sub(persistent.created_at) < self.challenge_ttl.as_secs() {
                    if let Ok(did) = persistent.did.parse::<Did>() {
                        let challenge = Challenge {
                            nonce: persistent.nonce,
                            did: did.clone(),
                            created_at: persistent.created_at,
                        };
                        challenges.insert(did, challenge);
                    }
                }
            }
        }

        Ok(())
    }

    /// Persist a challenge to storage
    fn persist_challenge(&self, did: &Did, challenge: &Challenge) -> Result<(), AuthError> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()),
        };

        let persistent = PersistentChallenge {
            nonce: challenge.nonce.clone(),
            did: did.to_string(),
            created_at: challenge.created_at,
        };

        let key = Self::make_challenge_key(did);
        let value = serde_json::to_vec(&persistent)
            .map_err(|e| AuthError::InternalError(format!("Failed to serialize challenge: {e}")))?;

        store
            .put(&key, &value)
            .map_err(|e| AuthError::InternalError(format!("Failed to store challenge: {e}")))?;

        Ok(())
    }

    /// Remove a challenge from persistent storage
    fn remove_persisted_challenge(&self, did: &Did) -> Result<(), AuthError> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(()),
        };

        let key = Self::make_challenge_key(did);
        store
            .delete(&key)
            .map_err(|e| AuthError::InternalError(format!("Failed to delete challenge: {e}")))?;

        Ok(())
    }

    fn make_challenge_key(did: &Did) -> Vec<u8> {
        let did_str = did.to_string();
        let mut key = Vec::with_capacity(CHALLENGE_PREFIX.len() + did_str.len());
        key.extend_from_slice(CHALLENGE_PREFIX);
        key.extend_from_slice(did_str.as_bytes());
        key
    }
}

/// Non-generic impl for backwards compatibility (no persistence)
impl RpcAuthManager<icn_store::SledStore> {
    /// Create a new auth manager without persistence
    ///
    /// Token revocation and challenge persistence will NOT be available.
    /// Use `new_with_store` for production deployments.
    ///
    /// # Arguments
    /// * `jwt_secret` - Secret for signing JWTs (should be at least 32 bytes)
    /// * `enabled` - If false, authentication is bypassed (for development only)
    pub fn new(jwt_secret: Vec<u8>, enabled: bool) -> Self {
        Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret,
            challenge_ttl: Duration::from_secs(300), // 5 minutes
            token_ttl: Duration::from_secs(86400),   // 24 hours
            enabled,
            revocation_list: None,
            store: None,
        }
    }
}

impl<S: Store + 'static> RpcAuthManager<S> {
    /// Generate a challenge for a DID
    pub fn create_challenge(&self, did: &Did) -> Result<String, AuthError> {
        let nonce = self.generate_nonce();
        let challenge = Challenge {
            nonce: nonce.clone(),
            did: did.clone(),
            created_at: Self::current_timestamp()?,
        };

        // Persist challenge if storage is available
        self.persist_challenge(did, &challenge)?;

        let mut challenges = self
            .challenges
            .write()
            .map_err(|e| AuthError::InternalError(format!("Lock poisoned: {e}")))?;

        challenges.insert(did.clone(), challenge);

        Ok(nonce)
    }

    /// Verify signed challenge and issue token
    pub fn verify_challenge(
        &self,
        did: &Did,
        signature: &[u8],
        scopes: Vec<String>,
    ) -> Result<String, AuthError> {
        self.verify_challenge_with_coop(did, signature, scopes, None)
    }

    /// Verify signed challenge and issue token with optional coop context
    ///
    /// When `coop_id` is provided, the token will include the coop_id claim,
    /// enabling coop isolation checks in RPC handlers.
    ///
    /// SECURITY NOTE: This method only verifies the cryptographic challenge.
    /// For coop-scoped tokens, use `verify_challenge_then_validate` which
    /// ensures membership is validated AFTER identity is proven, preventing
    /// membership enumeration attacks.
    pub fn verify_challenge_with_coop(
        &self,
        did: &Did,
        signature: &[u8],
        scopes: Vec<String>,
        coop_id: Option<String>,
    ) -> Result<String, AuthError> {
        self.verify_challenge_internal(did, signature, scopes, coop_id, || Ok(()))
    }

    /// Verify signed challenge with post-verification validation
    ///
    /// This method verifies the challenge signature FIRST, then calls the
    /// provided validator. This ensures identity is proven before any
    /// membership checks, preventing enumeration attacks.
    ///
    /// The validator is called only after the signature is verified successfully.
    /// If the validator returns an error, no token is issued.
    pub fn verify_challenge_then_validate<F>(
        &self,
        did: &Did,
        signature: &[u8],
        scopes: Vec<String>,
        coop_id: Option<String>,
        validator: F,
    ) -> Result<String, AuthError>
    where
        F: FnOnce() -> Result<(), AuthError>,
    {
        self.verify_challenge_internal(did, signature, scopes, coop_id, validator)
    }

    /// Internal implementation for challenge verification with optional validation
    fn verify_challenge_internal<F>(
        &self,
        did: &Did,
        signature: &[u8],
        scopes: Vec<String>,
        coop_id: Option<String>,
        post_verify_validator: F,
    ) -> Result<String, AuthError>
    where
        F: FnOnce() -> Result<(), AuthError>,
    {
        let auth_error =
            || AuthError::AuthenticationFailed("Invalid challenge or signature".to_string());

        // Retrieve and remove challenge
        let challenge = {
            let mut challenges = self
                .challenges
                .write()
                .map_err(|e| AuthError::InternalError(format!("Lock poisoned: {e}")))?;

            challenges.remove(did).ok_or_else(auth_error)?
        };

        // Remove from persistent storage
        self.remove_persisted_challenge(did)?;

        // Parse inputs
        let verifying_key_result = did.to_verifying_key();
        let nonce_bytes_result = hex::decode(&challenge.nonce);
        let signature_obj_result = Signature::from_slice(signature);

        // Verify signature
        let signature_valid = match (
            verifying_key_result,
            nonce_bytes_result,
            signature_obj_result,
        ) {
            (Ok(verifying_key), Ok(nonce_bytes), Ok(signature_obj)) => {
                verifying_key.verify(&nonce_bytes, &signature_obj).is_ok()
            }
            _ => false,
        };

        // Check expiration
        let now = Self::current_timestamp()?;
        let is_expired = now.saturating_sub(challenge.created_at) >= self.challenge_ttl.as_secs();

        if !signature_valid || is_expired {
            return Err(auth_error());
        }

        // Run post-verification validator (e.g., membership check)
        // This happens AFTER signature verification to prevent enumeration
        post_verify_validator()?;

        // Issue JWT token
        self.issue_token(did, scopes, coop_id)
    }

    /// Issue a JWT token for a DID that has already been verified
    ///
    /// This is used when identity has been proven through other means
    /// (e.g., challenge-response already completed) and we need to issue
    /// a token with additional claims like coop_id.
    ///
    /// SECURITY: Only call this after the DID's identity has been verified.
    pub fn issue_token_for_did(
        &self,
        did: &Did,
        scopes: Vec<String>,
        coop_id: Option<String>,
    ) -> Result<String, AuthError> {
        self.issue_token(did, scopes, coop_id)
    }

    /// Issue a JWT token (internal)
    fn issue_token(
        &self,
        did: &Did,
        scopes: Vec<String>,
        coop_id: Option<String>,
    ) -> Result<String, AuthError> {
        let now = Self::current_timestamp()?;
        let jti = Uuid::new_v4().to_string();

        let claims = RpcTokenClaims {
            sub: did.to_string(),
            jti,
            iat: now,
            exp: now + self.token_ttl.as_secs(),
            scopes,
            coop_id,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| AuthError::InternalError(format!("JWT encoding failed: {e}")))?;

        Ok(token)
    }

    /// Verify a JWT token and extract claims
    ///
    /// This checks:
    /// 1. Token signature is valid
    /// 2. Token has not expired
    /// 3. Token has not been revoked (if revocation list is enabled)
    pub fn verify_token(&self, token: &str) -> Result<RpcTokenClaims, AuthError> {
        let validation = Validation::default();

        let token_data = decode::<RpcTokenClaims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )
        .map_err(|e| AuthError::AuthenticationFailed(format!("Invalid token: {e}")))?;

        // Check if token has been revoked
        if let Some(ref trl) = self.revocation_list {
            if trl.is_revoked(&token_data.claims.jti) {
                return Err(AuthError::TokenRevoked);
            }
        }

        Ok(token_data.claims)
    }

    /// Parse token claims without expiration or revocation validation
    ///
    /// This is useful for:
    /// - Extracting claims from expired tokens (e.g., for revocation)
    /// - Authorization checks where you need claims but token may be expired
    ///
    /// NOTE: This still validates the signature - only expiration and revocation checks are skipped.
    pub fn parse_token_claims(&self, token: &str) -> Result<RpcTokenClaims, AuthError> {
        let mut validation = Validation::default();
        validation.validate_exp = false;

        let token_data = decode::<RpcTokenClaims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )
        .map_err(|e| AuthError::AuthenticationFailed(format!("Invalid token: {e}")))?;

        Ok(token_data.claims)
    }

    /// Revoke a token by its claims
    ///
    /// This adds the token to the revocation list, preventing it from being used
    /// for authentication until it would have naturally expired.
    ///
    /// Returns an error if revocation support is not enabled (no persistent storage).
    pub fn revoke_token(
        &self,
        claims: &RpcTokenClaims,
        reason: Option<String>,
    ) -> Result<(), AuthError> {
        let trl = self.revocation_list.as_ref().ok_or_else(|| {
            AuthError::InternalError(
                "Token revocation not available (no persistent storage)".to_string(),
            )
        })?;

        trl.revoke(&claims.jti, &claims.sub, claims.exp, reason)
    }

    /// Revoke a token by parsing and extracting its claims
    ///
    /// This is a convenience method that extracts claims from a raw token string
    /// and then revokes it. Useful when you have the token but not the claims.
    pub fn revoke_token_string(
        &self,
        token: &str,
        reason: Option<String>,
    ) -> Result<(), AuthError> {
        // Decode without full validation (we don't care if it's expired for revocation)
        let mut validation = Validation::default();
        validation.validate_exp = false;

        let token_data = decode::<RpcTokenClaims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )
        .map_err(|e| AuthError::AuthenticationFailed(format!("Invalid token: {e}")))?;

        self.revoke_token(&token_data.claims, reason)
    }

    /// Get the number of revoked tokens
    pub fn revoked_token_count(&self) -> usize {
        self.revocation_list
            .as_ref()
            .map(|trl| trl.count())
            .unwrap_or(0)
    }

    /// Check if a specific token (by JTI) has been revoked
    ///
    /// Returns `false` if revocation is not enabled or on cache errors (fail-open).
    pub fn is_token_revoked(&self, jti: &str) -> bool {
        match &self.revocation_list {
            Some(trl) => trl.is_revoked(jti),
            None => false,
        }
    }

    /// Generate cryptographically random nonce
    fn generate_nonce(&self) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let nonce_bytes: [u8; 32] = rng.gen();
        hex::encode(nonce_bytes)
    }

    /// Get current Unix timestamp
    fn current_timestamp() -> Result<u64, AuthError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .map_err(|e| AuthError::InternalError(format!("System clock error: {e}")))
    }

    /// Clean up expired challenges from memory and persistent storage
    pub fn cleanup_expired_challenges(&self) -> Result<usize, AuthError> {
        let now = Self::current_timestamp()?;
        let ttl = self.challenge_ttl.as_secs();

        // Collect expired challenge DIDs while holding the lock
        let expired_dids: Vec<Did> = {
            let challenges = self
                .challenges
                .read()
                .map_err(|e| AuthError::InternalError(format!("Lock poisoned: {e}")))?;

            challenges
                .iter()
                .filter(|(_, challenge)| now.saturating_sub(challenge.created_at) >= ttl)
                .map(|(did, _)| did.clone())
                .collect()
        };

        if expired_dids.is_empty() {
            return Ok(0);
        }

        // Remove from in-memory map
        {
            let mut challenges = self
                .challenges
                .write()
                .map_err(|e| AuthError::InternalError(format!("Lock poisoned: {e}")))?;

            for did in &expired_dids {
                challenges.remove(did);
            }
        }

        // Remove from persistent storage (if configured)
        for did in &expired_dids {
            if let Err(e) = self.remove_persisted_challenge(did) {
                tracing::warn!(
                    did = %did,
                    error = %e,
                    "Failed to remove expired persisted RPC auth challenge"
                );
            }
        }

        Ok(expired_dids.len())
    }

    /// Start background task to periodically clean up expired challenges and revocations
    ///
    /// Returns immediately. The cleanup task runs every 5 minutes until the shutdown
    /// signal is received. This prevents memory growth from abandoned authentication attempts
    /// and cleans up expired token revocations.
    pub fn start_cleanup_task(
        self: &Arc<Self>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        let manager = Arc::clone(self);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Clean up expired challenges
                        match manager.cleanup_expired_challenges() {
                            Ok(0) => {} // No expired challenges
                            Ok(n) => {
                                tracing::debug!(cleaned = n, "Cleaned up expired RPC auth challenges");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to clean up expired challenges");
                            }
                        }

                        // Clean up expired token revocations
                        if let Some(ref trl) = manager.revocation_list {
                            match trl.cleanup_expired() {
                                Ok(0) => {} // No expired revocations
                                Ok(n) => {
                                    tracing::debug!(cleaned = n, "Cleaned up expired token revocations");
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to clean up expired revocations");
                                }
                            }
                        }
                    }
                    _ = shutdown.recv() => {
                        tracing::debug!("RPC auth cleanup task shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Get the number of pending challenges (for metrics/debugging)
    pub fn pending_challenge_count(&self) -> usize {
        self.challenges.read().map(|c| c.len()).unwrap_or(0)
    }
}

/// Scope constants for RPC authorization
pub mod scopes {
    // Read-only scopes
    pub const NETWORK_READ: &str = "network:read";
    pub const LEDGER_READ: &str = "ledger:read";
    pub const CONTRACT_READ: &str = "contract:read";
    pub const GOVERNANCE_READ: &str = "governance:read";
    pub const COMPUTE_READ: &str = "compute:read";
    pub const POLICY_READ: &str = "policy:read";
    pub const TRUST_READ: &str = "trust:read";
    pub const RECOVERY_READ: &str = "recovery:read";
    pub const DISPUTE_READ: &str = "dispute:read";

    // Write scopes
    pub const NETWORK_WRITE: &str = "network:write";
    pub const LEDGER_WRITE: &str = "ledger:write";
    pub const CONTRACT_WRITE: &str = "contract:write";
    pub const GOVERNANCE_WRITE: &str = "governance:write";
    pub const COMPUTE_WRITE: &str = "compute:write";
    pub const POLICY_WRITE: &str = "policy:write";
    pub const TRUST_WRITE: &str = "trust:write";
    pub const RECOVERY_WRITE: &str = "recovery:write";
    pub const DISPUTE_WRITE: &str = "dispute:write";

    // Admin scopes
    pub const ADMIN: &str = "admin";

    /// All read scopes
    pub const ALL_READ: &[&str] = &[
        NETWORK_READ,
        LEDGER_READ,
        CONTRACT_READ,
        GOVERNANCE_READ,
        COMPUTE_READ,
        POLICY_READ,
        TRUST_READ,
        RECOVERY_READ,
        DISPUTE_READ,
    ];

    /// All write scopes
    pub const ALL_WRITE: &[&str] = &[
        NETWORK_WRITE,
        LEDGER_WRITE,
        CONTRACT_WRITE,
        GOVERNANCE_WRITE,
        COMPUTE_WRITE,
        POLICY_WRITE,
        TRUST_WRITE,
        RECOVERY_WRITE,
        DISPUTE_WRITE,
    ];
}

/// Method to scope mapping
pub fn required_scope_for_method(method: &str) -> Option<&'static str> {
    match method {
        // Auth methods don't require scope (auth.revoke checks ownership in handler)
        "auth.challenge" | "auth.verify" | "auth.revoke" => None,

        // Network methods
        "network.peers" | "network.stats" | "network.status" | "network.nat_status" => {
            Some(scopes::NETWORK_READ)
        }
        "network.dial" => Some(scopes::NETWORK_WRITE),

        // Ledger methods
        "ledger.head" | "ledger.balance" | "ledger.history" => Some(scopes::LEDGER_READ),
        "ledger.quarantine.list" | "ledger.quarantine.get" => Some(scopes::LEDGER_READ),
        "ledger.quarantine.release" | "ledger.quarantine.drop" | "ledger.quarantine.purge" => {
            Some(scopes::LEDGER_WRITE)
        }

        // Contract methods
        "contract.list" | "receipt.get" => Some(scopes::CONTRACT_READ),
        "contract.deploy" | "contract.call" => Some(scopes::CONTRACT_WRITE),

        // Governance methods
        "governance.domain.list" | "governance.domain.get" => Some(scopes::GOVERNANCE_READ),
        "governance.proposal.list" | "governance.proposal.get" => Some(scopes::GOVERNANCE_READ),
        "governance.domain.create" => Some(scopes::GOVERNANCE_WRITE),
        "governance.proposal.create"
        | "governance.proposal.open"
        | "governance.proposal.close"
        | "governance.vote.cast" => Some(scopes::GOVERNANCE_WRITE),

        // Compute methods
        "compute.status" => Some(scopes::COMPUTE_READ),
        "compute.submit" | "compute.cancel" => Some(scopes::COMPUTE_WRITE),

        // Policy methods
        "policy.get" | "policy.list" | "quota.usage" | "quota.list" => Some(scopes::POLICY_READ),
        "policy.set" | "policy.remove" => Some(scopes::POLICY_WRITE),

        // Trust methods
        "trust.list" | "trust.compute" => Some(scopes::TRUST_READ),
        "trust.add" | "trust.remove" => Some(scopes::TRUST_WRITE),

        // Recovery methods
        "recovery.list" | "recovery.status" => Some(scopes::RECOVERY_READ),
        "recovery.initiate" | "recovery.attest" | "recovery.finalize" | "recovery.cancel" => {
            Some(scopes::RECOVERY_WRITE)
        }

        // Dispute methods (ledger entry disputes)
        "dispute.list" | "dispute.get" => Some(scopes::DISPUTE_READ),
        "dispute.file" | "dispute.add_evidence" | "dispute.assign_mediator" | "dispute.resolve" => {
            Some(scopes::DISPUTE_WRITE)
        }

        // Unknown methods - require admin
        _ => Some(scopes::ADMIN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;

    #[test]
    fn test_create_challenge() {
        let auth = RpcAuthManager::new(b"test_secret".to_vec(), true);
        let bundle = IdentityBundle::generate().unwrap();

        let nonce = auth.create_challenge(bundle.did()).unwrap();
        assert_eq!(nonce.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_verify_challenge_success() {
        let auth = RpcAuthManager::new(b"test_secret".to_vec(), true);
        let bundle = IdentityBundle::generate().unwrap();

        // Create challenge
        let nonce = auth.create_challenge(bundle.did()).unwrap();

        // Sign nonce
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().unwrap().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        // Verify and get token
        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature_bytes,
                vec!["compute:write".to_string()],
            )
            .unwrap();

        assert!(!token.is_empty());

        // Verify token works
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.sub, bundle.did().to_string());
        assert!(claims.has_scope("compute:write"));
    }

    #[test]
    fn test_verify_challenge_invalid_signature() {
        let auth = RpcAuthManager::new(b"test_secret".to_vec(), true);
        let bundle = IdentityBundle::generate().unwrap();

        let _nonce = auth.create_challenge(bundle.did()).unwrap();

        // Invalid signature
        let result = auth.verify_challenge(bundle.did(), &[0u8; 64], vec![]);

        assert!(matches!(result, Err(AuthError::AuthenticationFailed(_))));
    }

    #[test]
    fn test_scope_checking() {
        let claims = RpcTokenClaims {
            sub: "did:icn:test".to_string(),
            jti: "test-token-id".to_string(),
            iat: 0,
            exp: u64::MAX,
            scopes: vec!["compute:write".to_string(), "ledger:*".to_string()],
            coop_id: None,
        };

        assert!(claims.has_scope("compute:write"));
        assert!(!claims.has_scope("compute:read"));
        assert!(claims.has_scope("ledger:read")); // Wildcard
        assert!(claims.has_scope("ledger:write")); // Wildcard
    }

    #[test]
    fn test_wildcard_scope() {
        let claims = RpcTokenClaims {
            sub: "did:icn:test".to_string(),
            jti: "test-token-id".to_string(),
            iat: 0,
            exp: u64::MAX,
            scopes: vec!["*".to_string()],
            coop_id: None,
        };

        assert!(claims.has_scope("anything"));
        assert!(claims.has_scope("compute:write"));
        assert!(claims.has_scope("admin"));
    }

    #[test]
    fn test_required_scope_for_methods() {
        // Auth methods don't require auth
        assert_eq!(required_scope_for_method("auth.challenge"), None);
        assert_eq!(required_scope_for_method("auth.verify"), None);

        // Read methods
        assert_eq!(
            required_scope_for_method("network.peers"),
            Some(scopes::NETWORK_READ)
        );
        assert_eq!(
            required_scope_for_method("compute.status"),
            Some(scopes::COMPUTE_READ)
        );

        // Write methods
        assert_eq!(
            required_scope_for_method("compute.submit"),
            Some(scopes::COMPUTE_WRITE)
        );
        assert_eq!(
            required_scope_for_method("governance.vote.cast"),
            Some(scopes::GOVERNANCE_WRITE)
        );

        // Auth revoke doesn't require specific scope
        assert_eq!(required_scope_for_method("auth.revoke"), None);
    }

    #[test]
    fn test_token_contains_jti() {
        let auth = RpcAuthManager::new(b"test_secret".to_vec(), true);
        let bundle = IdentityBundle::generate().unwrap();

        // Create and verify challenge
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().unwrap().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature_bytes,
                vec!["compute:write".to_string()],
            )
            .unwrap();

        // Verify token contains jti
        let claims = auth.verify_token(&token).unwrap();
        assert!(!claims.jti.is_empty());
        // JTI should be a valid UUID
        assert!(uuid::Uuid::parse_str(&claims.jti).is_ok());
    }

    #[test]
    fn test_token_revocation_with_persistence() {
        // Create a temporary store for testing
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let auth = RpcAuthManager::new_with_store(b"test_secret".to_vec(), true, store).unwrap();
        let bundle = IdentityBundle::generate().unwrap();

        // Create and verify challenge to get a token
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().unwrap().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        let token = auth
            .verify_challenge(
                bundle.did(),
                &signature_bytes,
                vec!["compute:write".to_string()],
            )
            .unwrap();

        // Token should be valid
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.sub, bundle.did().to_string());

        // Revoke the token
        auth.revoke_token(&claims, Some("test revocation".to_string()))
            .unwrap();

        // Token should now be rejected
        let result = auth.verify_token(&token);
        assert!(matches!(result, Err(AuthError::TokenRevoked)));

        // Verify revocation count
        assert_eq!(auth.revoked_token_count(), 1);
    }

    #[test]
    fn test_revoke_token_string() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let auth = RpcAuthManager::new_with_store(b"test_secret".to_vec(), true, store).unwrap();
        let bundle = IdentityBundle::generate().unwrap();

        // Get a token
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().unwrap().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        let token = auth
            .verify_challenge(bundle.did(), &signature_bytes, vec![])
            .unwrap();

        // Revoke using the token string directly
        auth.revoke_token_string(&token, None).unwrap();

        // Token should be rejected
        let result = auth.verify_token(&token);
        assert!(matches!(result, Err(AuthError::TokenRevoked)));
    }

    #[test]
    fn test_challenge_persistence() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let bundle = IdentityBundle::generate().unwrap();

        // Create first auth manager and create a challenge
        let auth1 =
            RpcAuthManager::new_with_store(b"test_secret".to_vec(), true, Arc::clone(&store))
                .unwrap();
        let nonce = auth1.create_challenge(bundle.did()).unwrap();
        assert_eq!(auth1.pending_challenge_count(), 1);

        // Create second auth manager with same store (simulates restart)
        let auth2 = RpcAuthManager::new_with_store(b"test_secret".to_vec(), true, store).unwrap();

        // Challenge should be loaded from persistence
        assert_eq!(auth2.pending_challenge_count(), 1);

        // Should be able to verify the challenge
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().unwrap().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        let token = auth2
            .verify_challenge(bundle.did(), &signature_bytes, vec![])
            .unwrap();

        assert!(!token.is_empty());
    }

    #[test]
    fn test_revocation_persistence_across_restart() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let bundle = IdentityBundle::generate().unwrap();

        let token: String;
        let jti: String;

        // First session: create a token and revoke it
        {
            let auth1 =
                RpcAuthManager::new_with_store(b"test_secret".to_vec(), true, Arc::clone(&store))
                    .unwrap();

            // Get a token
            let nonce = auth1.create_challenge(bundle.did()).unwrap();
            let nonce_bytes = hex::decode(&nonce).unwrap();
            let signature = bundle.keypair().unwrap().sign(&nonce_bytes);
            let signature_bytes = signature.to_bytes();

            token = auth1
                .verify_challenge(bundle.did(), &signature_bytes, vec![])
                .unwrap();

            let claims = auth1.verify_token(&token).unwrap();
            jti = claims.jti.clone();

            // Revoke the token
            auth1
                .revoke_token(&claims, Some("test revocation".to_string()))
                .unwrap();

            // Verify it's revoked in this session
            assert!(matches!(
                auth1.verify_token(&token),
                Err(AuthError::TokenRevoked)
            ));
            assert_eq!(auth1.revoked_token_count(), 1);
        }

        // Second session: create new auth manager with same store (simulates restart)
        {
            let auth2 =
                RpcAuthManager::new_with_store(b"test_secret".to_vec(), true, store).unwrap();

            // Revocation should be loaded from persistence
            assert_eq!(auth2.revoked_token_count(), 1);

            // Token should still be rejected after restart
            assert!(matches!(
                auth2.verify_token(&token),
                Err(AuthError::TokenRevoked)
            ));

            // Check by JTI directly
            assert!(auth2.is_token_revoked(&jti));
        }
    }

    #[test]
    fn test_revocation_without_persistence() {
        // Without persistence, revocation should fail
        let auth = RpcAuthManager::new(b"test_secret".to_vec(), true);
        let bundle = IdentityBundle::generate().unwrap();

        // Get a token
        let nonce = auth.create_challenge(bundle.did()).unwrap();
        let nonce_bytes = hex::decode(&nonce).unwrap();
        let signature = bundle.keypair().unwrap().sign(&nonce_bytes);
        let signature_bytes = signature.to_bytes();

        let token = auth
            .verify_challenge(bundle.did(), &signature_bytes, vec![])
            .unwrap();

        let claims = auth.verify_token(&token).unwrap();

        // Try to revoke - should fail because no persistence
        let result = auth.revoke_token(&claims, None);
        assert!(matches!(result, Err(AuthError::InternalError(_))));
    }

    #[test]
    fn test_token_revocation_list_cleanup() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let trl = TokenRevocationList::new(Arc::clone(&store)).unwrap();

        // Add a revoked token that would have already expired
        let past_expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(3600); // 1 hour ago

        // This should be a no-op since the token is already expired
        trl.revoke("old-jti", "did:icn:test", past_expiry, None)
            .unwrap();
        assert_eq!(trl.count(), 0); // Shouldn't be added since already expired

        // Add a valid revoked token
        let future_expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600; // 1 hour from now

        trl.revoke("new-jti", "did:icn:test", future_expiry, None)
            .unwrap();
        assert_eq!(trl.count(), 1);
        assert!(trl.is_revoked("new-jti"));
        assert!(!trl.is_revoked("old-jti"));
    }

    /// Test concurrent token revocation from multiple threads.
    ///
    /// Verifies that the TRL is thread-safe and handles concurrent revocations
    /// of the same token correctly. The revoke() operation is idempotent - multiple
    /// threads revoking the same token will all succeed, but the token will only
    /// appear once in the cache (HashMap::insert overwrites).
    ///
    /// Note: This test validates in-memory concurrency only. It does not test
    /// concurrent access from different process instances sharing the same storage.
    #[test]
    fn test_concurrent_token_revocation() {
        use std::sync::Arc;
        use std::thread;
        use std::time::{SystemTime, UNIX_EPOCH};

        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let trl = Arc::new(TokenRevocationList::new(Arc::clone(&store)).unwrap());

        // Token expiry 1 hour from now
        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;

        let jti = "concurrent-test-jti";
        let subject = "did:icn:concurrent-test";

        // Spawn multiple threads all trying to revoke the same token
        let num_threads = 10;
        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let trl = Arc::clone(&trl);
                let jti = jti.to_string();
                let subject = subject.to_string();
                thread::spawn(move || {
                    // Each thread tries to revoke the same token
                    let result = trl.revoke(&jti, &subject, expiry, Some(format!("thread-{i}")));
                    // Should succeed (idempotent operation)
                    result.is_ok()
                })
            })
            .collect();

        // Wait for all threads and collect results
        let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All threads should have succeeded
        assert!(
            results.iter().all(|&r| r),
            "All concurrent revocations should succeed"
        );

        // Token should be revoked exactly once in the cache
        assert_eq!(trl.count(), 1, "Token should be in cache exactly once");
        assert!(trl.is_revoked(jti), "Token should be marked as revoked");

        // Verify concurrent reads don't cause issues
        let read_handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let trl = Arc::clone(&trl);
                let jti = jti.to_string();
                thread::spawn(move || trl.is_revoked(&jti))
            })
            .collect();

        let read_results: Vec<bool> = read_handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect();
        assert!(
            read_results.iter().all(|&r| r),
            "All concurrent reads should return revoked"
        );
    }
}
