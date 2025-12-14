//! Invite Manager for Gateway API
//!
//! Manages cooperative invitations with secure codes and expiration.

use anyhow::Result;
use icn_identity::Did;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Stored invite data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    pub code: String,
    pub coop_id: String,
    pub role: String,
    pub created_by: Did,
    pub created_at: u64,
    pub expires_at: u64,
    pub used: bool,
    pub used_by: Option<Did>,
    pub used_at: Option<u64>,
}

/// Invite manager for gateway API
pub struct InviteManager {
    invites: RwLock<HashMap<String, Invite>>,
}

impl Default for InviteManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InviteManager {
    /// Create a new invite manager with in-memory storage
    pub fn new() -> Self {
        InviteManager {
            invites: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new invite code
    pub async fn create_invite(
        &self,
        coop_id: String,
        role: String,
        created_by: Did,
        expires_in_seconds: u64,
    ) -> Result<Invite> {
        let code = Self::generate_code();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expires_at = now + expires_in_seconds;

        let invite = Invite {
            code: code.clone(),
            coop_id,
            role,
            created_by,
            created_at: now,
            expires_at,
            used: false,
            used_by: None,
            used_at: None,
        };

        let mut invites = self
            .invites
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        invites.insert(code.clone(), invite.clone());

        Ok(invite)
    }

    /// Get an invite by code
    pub async fn get_invite(&self, code: &str) -> Result<Option<Invite>> {
        let invites = self
            .invites
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        Ok(invites.get(code).cloned())
    }

    /// List all invites for a cooperative
    pub async fn list_invites(&self, coop_id: &str) -> Result<Vec<Invite>> {
        let invites = self
            .invites
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let mut result: Vec<_> = invites
            .values()
            .filter(|i| i.coop_id == coop_id)
            .cloned()
            .collect();

        result.sort_by_key(|i| std::cmp::Reverse(i.created_at));

        Ok(result)
    }

    /// Mark an invite as used
    pub async fn mark_used(&self, code: &str, used_by: Did) -> Result<()> {
        let mut invites = self
            .invites
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let invite = invites
            .get_mut(code)
            .ok_or_else(|| anyhow::anyhow!("Invite not found"))?;

        if invite.used {
            anyhow::bail!("Invite already used");
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > invite.expires_at {
            anyhow::bail!("Invite expired");
        }

        invite.used = true;
        invite.used_by = Some(used_by);
        invite.used_at = Some(now);

        Ok(())
    }

    /// Validate an invite (check if it exists, hasn't been used, and hasn't expired)
    pub async fn validate_invite(&self, code: &str) -> Result<Invite> {
        let invites = self
            .invites
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let invite = invites
            .get(code)
            .ok_or_else(|| anyhow::anyhow!("Invalid invite code"))?;

        if invite.used {
            anyhow::bail!("Invite already used");
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > invite.expires_at {
            anyhow::bail!("Invite expired");
        }

        Ok(invite.clone())
    }

    /// Generate a random invite code (12 characters, alphanumeric)
    fn generate_code() -> String {
        const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut rng = thread_rng();

        (0..12)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_create_and_get_invite() {
        let mgr = InviteManager::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        let invite = mgr
            .create_invite("coop1".to_string(), "member".to_string(), did.clone(), 3600)
            .await
            .unwrap();

        assert_eq!(invite.code.len(), 12);
        assert_eq!(invite.coop_id, "coop1");
        assert_eq!(invite.role, "member");
        assert!(!invite.used);

        let retrieved = mgr.get_invite(&invite.code).await.unwrap().unwrap();
        assert_eq!(retrieved.code, invite.code);
    }

    #[tokio::test]
    async fn test_mark_used() {
        let mgr = InviteManager::new();
        let creator_kp = KeyPair::generate().unwrap();
        let joiner_kp = KeyPair::generate().unwrap();
        let creator = creator_kp.did();
        let joiner = joiner_kp.did();

        let invite = mgr
            .create_invite(
                "coop1".to_string(),
                "member".to_string(),
                creator.clone(),
                3600,
            )
            .await
            .unwrap();

        mgr.mark_used(&invite.code, joiner.clone()).await.unwrap();

        let used = mgr.get_invite(&invite.code).await.unwrap().unwrap();
        assert!(used.used);
        assert_eq!(used.used_by, Some(joiner.clone()));
    }

    #[tokio::test]
    async fn test_validate_invite() {
        let mgr = InviteManager::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        let invite = mgr
            .create_invite("coop1".to_string(), "member".to_string(), did.clone(), 3600)
            .await
            .unwrap();

        // Valid invite
        mgr.validate_invite(&invite.code).await.unwrap();

        // Mark as used
        mgr.mark_used(&invite.code, did.clone()).await.unwrap();

        // Should fail validation
        assert!(mgr.validate_invite(&invite.code).await.is_err());
    }

    #[tokio::test]
    async fn test_list_invites() {
        let mgr = InviteManager::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        mgr.create_invite("coop1".to_string(), "member".to_string(), did.clone(), 3600)
            .await
            .unwrap();
        mgr.create_invite("coop1".to_string(), "admin".to_string(), did.clone(), 3600)
            .await
            .unwrap();
        mgr.create_invite("coop2".to_string(), "member".to_string(), did.clone(), 3600)
            .await
            .unwrap();

        let coop1_invites = mgr.list_invites("coop1").await.unwrap();
        assert_eq!(coop1_invites.len(), 2);

        let coop2_invites = mgr.list_invites("coop2").await.unwrap();
        assert_eq!(coop2_invites.len(), 1);
    }
}
