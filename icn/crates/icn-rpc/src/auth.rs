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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

use ed25519_dalek::{Signature, Verifier};
use icn_identity::Did;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// Authentication error types
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

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
    /// Issued at (Unix timestamp)
    pub iat: u64,
    /// Expiration (Unix timestamp)
    pub exp: u64,
    /// Scopes/permissions
    pub scopes: Vec<String>,
    /// Cooperative ID (optional, for compute task attribution)
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

/// RPC Authentication Manager
pub struct RpcAuthManager {
    challenges: Arc<RwLock<HashMap<Did, Challenge>>>,
    jwt_secret: Vec<u8>,
    challenge_ttl: Duration,
    token_ttl: Duration,
    /// If true, authentication is required. If false, all requests are allowed (dev mode).
    enabled: bool,
}

impl RpcAuthManager {
    /// Create a new auth manager
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
        }
    }

    /// Check if authentication is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Generate a challenge for a DID
    pub fn create_challenge(&self, did: &Did) -> Result<String, AuthError> {
        let nonce = self.generate_nonce();
        let challenge = Challenge {
            nonce: nonce.clone(),
            did: did.clone(),
            created_at: Self::current_timestamp()?,
        };

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

        // Issue JWT token
        self.issue_token(did, scopes)
    }

    /// Issue a JWT token
    fn issue_token(&self, did: &Did, scopes: Vec<String>) -> Result<String, AuthError> {
        let now = Self::current_timestamp()?;
        let claims = RpcTokenClaims {
            sub: did.to_string(),
            iat: now,
            exp: now + self.token_ttl.as_secs(),
            scopes,
            coop_id: None, // Can be set later via token refresh with coop context
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
    pub fn verify_token(&self, token: &str) -> Result<RpcTokenClaims, AuthError> {
        let validation = Validation::default();

        let token_data = decode::<RpcTokenClaims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )
        .map_err(|e| AuthError::AuthenticationFailed(format!("Invalid token: {e}")))?;

        Ok(token_data.claims)
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

    /// Clean up expired challenges
    pub fn cleanup_expired_challenges(&self) -> Result<usize, AuthError> {
        let mut challenges = self
            .challenges
            .write()
            .map_err(|e| AuthError::InternalError(format!("Lock poisoned: {e}")))?;

        let now = Self::current_timestamp()?;
        let ttl = self.challenge_ttl.as_secs();
        let initial_count = challenges.len();

        challenges.retain(|_, challenge| now.saturating_sub(challenge.created_at) < ttl);

        Ok(initial_count - challenges.len())
    }

    /// Start background task to periodically clean up expired challenges
    ///
    /// Returns immediately. The cleanup task runs every 5 minutes until the shutdown
    /// signal is received. This prevents memory growth from abandoned authentication attempts.
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
                        match manager.cleanup_expired_challenges() {
                            Ok(0) => {} // No expired challenges
                            Ok(n) => {
                                tracing::debug!(cleaned = n, "Cleaned up expired RPC auth challenges");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to clean up expired challenges");
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

    // Write scopes
    pub const NETWORK_WRITE: &str = "network:write";
    pub const LEDGER_WRITE: &str = "ledger:write";
    pub const CONTRACT_WRITE: &str = "contract:write";
    pub const GOVERNANCE_WRITE: &str = "governance:write";
    pub const COMPUTE_WRITE: &str = "compute:write";
    pub const POLICY_WRITE: &str = "policy:write";
    pub const TRUST_WRITE: &str = "trust:write";
    pub const RECOVERY_WRITE: &str = "recovery:write";

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
    ];
}

/// Method to scope mapping
pub fn required_scope_for_method(method: &str) -> Option<&'static str> {
    match method {
        // Auth methods don't require auth (bootstrap)
        "auth.challenge" | "auth.verify" => None,

        // Network methods
        "network.peers" | "network.stats" | "network.status" => Some(scopes::NETWORK_READ),
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
        let signature = bundle.keypair().sign(&nonce_bytes);
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
    }
}
