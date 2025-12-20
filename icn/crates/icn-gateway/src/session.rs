//! QR Login Session Manager
//!
//! Manages ephemeral login sessions for QR-based web authentication.
//! Flow:
//! 1. Web UI creates a session (gets session_id for QR code)
//! 2. Mobile wallet scans QR and approves session
//! 3. Gateway issues token to web session
//! 4. Web UI polls for status, receives token when approved

use anyhow::Result;
use icn_identity::Did;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Waiting for wallet approval
    Pending,
    /// Wallet approved, token ready
    Approved,
    /// Session timed out
    Expired,
    /// Web client retrieved the token (one-time use)
    Consumed,
}

/// Login session data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginSession {
    pub session_id: String,
    pub coop_id: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub status: SessionStatus,
    /// DID that approved the session (populated when approved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<Did>,
    /// Timestamp when approved
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<u64>,
    /// JWT token for web session (populated when approved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Token expiry in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_in: Option<u64>,
    /// Scopes granted to the token
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Session manager for QR-based login
pub struct SessionManager {
    sessions: RwLock<HashMap<String, LoginSession>>,
    /// Session TTL (time until pending sessions expire)
    session_ttl: Duration,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// Create a new session manager with default 5-minute TTL
    pub fn new() -> Self {
        SessionManager {
            sessions: RwLock::new(HashMap::new()),
            session_ttl: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Create a new session manager with custom TTL
    pub fn with_ttl(ttl_seconds: u64) -> Self {
        SessionManager {
            sessions: RwLock::new(HashMap::new()),
            session_ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Create a new login session
    pub async fn create_session(&self, coop_id: String) -> Result<LoginSession> {
        let session_id = Self::generate_session_id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expires_at = now + self.session_ttl.as_secs();

        let session = LoginSession {
            session_id: session_id.clone(),
            coop_id,
            created_at: now,
            expires_at,
            status: SessionStatus::Pending,
            approved_by: None,
            approved_at: None,
            token: None,
            token_expires_in: None,
            scopes: Vec::new(),
        };

        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        sessions.insert(session_id.clone(), session.clone());

        Ok(session)
    }

    /// Get a session by ID, checking for expiration
    pub async fn get_session(&self, session_id: &str) -> Result<Option<LoginSession>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(session) = sessions.get_mut(session_id) {
            // Check if session has expired
            if session.status == SessionStatus::Pending && now > session.expires_at {
                session.status = SessionStatus::Expired;
            }
            Ok(Some(session.clone()))
        } else {
            Ok(None)
        }
    }

    /// Approve a session (called by wallet with valid JWT)
    pub async fn approve_session(
        &self,
        session_id: &str,
        approved_by: Did,
        token: String,
        token_expires_in: u64,
        scopes: Vec<String>,
    ) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        // Check if session is still pending
        if session.status != SessionStatus::Pending {
            anyhow::bail!("Session is not pending (status: {:?})", session.status);
        }

        // Check if session has expired
        if now > session.expires_at {
            session.status = SessionStatus::Expired;
            anyhow::bail!("Session has expired");
        }

        // Approve the session
        session.status = SessionStatus::Approved;
        session.approved_by = Some(approved_by);
        session.approved_at = Some(now);
        session.token = Some(token);
        session.token_expires_in = Some(token_expires_in);
        session.scopes = scopes;

        Ok(())
    }

    /// Consume a session (marks as consumed, returns token)
    /// This ensures the token can only be retrieved once
    pub async fn consume_session(&self, session_id: &str) -> Result<LoginSession> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        if session.status != SessionStatus::Approved {
            anyhow::bail!("Session is not approved (status: {:?})", session.status);
        }

        // Clone before consuming
        let consumed_session = session.clone();

        // Mark as consumed
        session.status = SessionStatus::Consumed;
        // Clear sensitive data
        session.token = None;

        Ok(consumed_session)
    }

    /// Clean up expired and consumed sessions
    pub fn cleanup_expired(&self) -> Result<usize> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let initial_count = sessions.len();

        sessions.retain(|_, session| {
            // Remove if:
            // 1. Still pending but time has passed (never retrieved)
            let is_pending_expired =
                session.status == SessionStatus::Pending && now > session.expires_at;
            // 2. Already marked as expired
            let is_expired = session.status == SessionStatus::Expired;
            // 3. Already consumed
            let is_consumed = session.status == SessionStatus::Consumed;

            // Keep if none of the above
            !is_pending_expired && !is_expired && !is_consumed
        });

        Ok(initial_count - sessions.len())
    }

    /// Generate a cryptographically random session ID (32 bytes, hex-encoded)
    fn generate_session_id() -> String {
        let mut rng = thread_rng();
        let bytes: [u8; 32] = rng.gen();
        hex::encode(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_create_session() {
        let mgr = SessionManager::new();
        let session = mgr.create_session("test-coop".to_string()).await.unwrap();

        assert_eq!(session.session_id.len(), 64); // 32 bytes hex-encoded
        assert_eq!(session.coop_id, "test-coop");
        assert_eq!(session.status, SessionStatus::Pending);
        assert!(session.approved_by.is_none());
        assert!(session.token.is_none());
    }

    #[tokio::test]
    async fn test_get_session() {
        let mgr = SessionManager::new();
        let session = mgr.create_session("test-coop".to_string()).await.unwrap();

        let retrieved = mgr.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(retrieved.session_id, session.session_id);
        assert_eq!(retrieved.status, SessionStatus::Pending);
    }

    #[tokio::test]
    async fn test_approve_session() {
        let mgr = SessionManager::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        let session = mgr.create_session("test-coop".to_string()).await.unwrap();

        mgr.approve_session(
            &session.session_id,
            did.clone(),
            "test-token".to_string(),
            3600,
            vec!["ledger:read".to_string()],
        )
        .await
        .unwrap();

        let approved = mgr.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(approved.status, SessionStatus::Approved);
        assert_eq!(approved.approved_by, Some(did.clone()));
        assert_eq!(approved.token, Some("test-token".to_string()));
        assert_eq!(approved.scopes, vec!["ledger:read".to_string()]);
    }

    #[tokio::test]
    async fn test_consume_session() {
        let mgr = SessionManager::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        let session = mgr.create_session("test-coop".to_string()).await.unwrap();

        mgr.approve_session(
            &session.session_id,
            did.clone(),
            "test-token".to_string(),
            3600,
            vec![],
        )
        .await
        .unwrap();

        // Consume the session
        let consumed = mgr.consume_session(&session.session_id).await.unwrap();
        assert_eq!(consumed.token, Some("test-token".to_string()));

        // Session should now be marked as consumed
        let after = mgr.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Consumed);
        assert!(after.token.is_none()); // Token cleared

        // Trying to consume again should fail
        let result = mgr.consume_session(&session.session_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_expiration() {
        // Create manager with 1 second TTL
        let mgr = SessionManager::with_ttl(1);

        let session = mgr.create_session("test-coop".to_string()).await.unwrap();

        // Wait for expiration (significantly longer than TTL to account for clock granularity)
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Session should be marked as expired when retrieved
        let expired = mgr.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(expired.status, SessionStatus::Expired);
    }

    #[tokio::test]
    async fn test_cannot_approve_expired_session() {
        let mgr = SessionManager::with_ttl(1);
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        let session = mgr.create_session("test-coop".to_string()).await.unwrap();

        // Wait for expiration (significantly longer than TTL)
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Trying to approve should fail
        let result = mgr
            .approve_session(
                &session.session_id,
                did.clone(),
                "token".to_string(),
                3600,
                vec![],
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let mgr = SessionManager::with_ttl(1);

        // Create some sessions
        let s1 = mgr.create_session("coop1".to_string()).await.unwrap();
        let s2 = mgr.create_session("coop2".to_string()).await.unwrap();

        // Wait for expiration (significantly longer than TTL)
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Mark them as expired by getting them (updates status)
        let _ = mgr.get_session(&s1.session_id).await;
        let _ = mgr.get_session(&s2.session_id).await;

        // Check that sessions still exist
        let sessions = mgr.sessions.read().unwrap();
        assert_eq!(sessions.len(), 2);
        drop(sessions);

        // Cleanup should remove expired sessions
        let removed = mgr.cleanup_expired().unwrap();
        assert_eq!(removed, 2);

        let sessions = mgr.sessions.read().unwrap();
        assert_eq!(sessions.len(), 0);
    }
}
