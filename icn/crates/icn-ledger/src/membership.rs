//! Membership tracking for credit limit ramping
//!
//! This module provides infrastructure for tracking when members joined,
//! enabling new member credit limit ramping. New members start with conservative
//! limits that increase over time based on tenure and contribution history.

use anyhow::Result;
use icn_identity::Did;
use icn_store::Store;
use std::sync::Arc;
use tracing::debug;

/// Key prefix for member-since timestamps
const MEMBERSHIP_SINCE_PREFIX: &str = "membership:since:";

/// Key prefix for member contribution totals
const MEMBERSHIP_CONTRIBUTED_PREFIX: &str = "membership:contributed:";

/// Trait for tracking membership information
///
/// Implementations store when members joined and their total contributions,
/// enabling credit limit calculations that ramp up over time.
pub trait MembershipStore: Send + Sync {
    /// Get the timestamp when a member joined (Unix seconds)
    ///
    /// Returns `None` if the member is not registered yet.
    fn get_member_since(&self, did: &Did) -> Result<Option<u64>>;

    /// Set the timestamp when a member joined
    ///
    /// This should only be called once per member. Subsequent calls are ignored
    /// to preserve the original join date.
    fn set_member_since(&self, did: &Did, timestamp: u64) -> Result<()>;

    /// Register a new member if not already registered
    ///
    /// Returns the member_since timestamp (either existing or newly set).
    fn register_if_new(&self, did: &Did, timestamp: u64) -> Result<u64> {
        if let Some(existing) = self.get_member_since(did)? {
            Ok(existing)
        } else {
            self.set_member_since(did, timestamp)?;
            Ok(timestamp)
        }
    }

    /// Get total amount contributed by a member (credits received)
    ///
    /// This is used to allow high contributors to skip the ramping period.
    fn get_total_contributed(&self, did: &Did) -> Result<i64>;

    /// Add to a member's contribution total
    fn add_contribution(&self, did: &Did, amount: i64) -> Result<()>;
}

/// Sled-backed membership store
pub struct SledMembershipStore {
    store: Arc<dyn Store>,
}

impl SledMembershipStore {
    /// Create a new membership store using the given storage backend
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    fn since_key(did: &Did) -> Vec<u8> {
        format!("{MEMBERSHIP_SINCE_PREFIX}{did}").into_bytes()
    }

    fn contributed_key(did: &Did) -> Vec<u8> {
        format!("{MEMBERSHIP_CONTRIBUTED_PREFIX}{did}").into_bytes()
    }
}

impl MembershipStore for SledMembershipStore {
    fn get_member_since(&self, did: &Did) -> Result<Option<u64>> {
        let key = Self::since_key(did);
        match self.store.get(&key)? {
            Some(bytes) => {
                let timestamp: u64 = serde_json::from_slice(&bytes)?;
                Ok(Some(timestamp))
            }
            None => Ok(None),
        }
    }

    fn set_member_since(&self, did: &Did, timestamp: u64) -> Result<()> {
        let key = Self::since_key(did);
        // Only set if not already present (preserve original join date)
        if self.store.get(&key)?.is_none() {
            let bytes = serde_json::to_vec(&timestamp)?;
            self.store.put(&key, &bytes)?;
            debug!(did = %did, timestamp, "Registered new member");
        }
        Ok(())
    }

    fn get_total_contributed(&self, did: &Did) -> Result<i64> {
        let key = Self::contributed_key(did);
        match self.store.get(&key)? {
            Some(bytes) => {
                let amount: i64 = serde_json::from_slice(&bytes)?;
                Ok(amount)
            }
            None => Ok(0),
        }
    }

    fn add_contribution(&self, did: &Did, amount: i64) -> Result<()> {
        if amount <= 0 {
            return Ok(()); // Only track positive contributions (credits received)
        }

        let key = Self::contributed_key(did);
        let current = self.get_total_contributed(did)?;
        let new_total = current.saturating_add(amount);
        let bytes = serde_json::to_vec(&new_total)?;
        self.store.put(&key, &bytes)?;

        debug!(did = %did, amount, new_total, "Updated member contribution total");
        Ok(())
    }
}

/// In-memory membership store for testing
#[cfg(test)]
pub struct InMemoryMembershipStore {
    since: std::sync::RwLock<std::collections::HashMap<String, u64>>,
    contributed: std::sync::RwLock<std::collections::HashMap<String, i64>>,
}

#[cfg(test)]
impl InMemoryMembershipStore {
    /// Create a new in-memory membership store
    pub fn new() -> Self {
        Self {
            since: std::sync::RwLock::new(std::collections::HashMap::new()),
            contributed: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(test)]
impl Default for InMemoryMembershipStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl MembershipStore for InMemoryMembershipStore {
    fn get_member_since(&self, did: &Did) -> Result<Option<u64>> {
        let guard = self
            .since
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(guard.get(&did.to_string()).copied())
    }

    fn set_member_since(&self, did: &Did, timestamp: u64) -> Result<()> {
        let mut guard = self
            .since
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        guard.entry(did.to_string()).or_insert(timestamp);
        Ok(())
    }

    fn get_total_contributed(&self, did: &Did) -> Result<i64> {
        let guard = self
            .contributed
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(guard.get(&did.to_string()).copied().unwrap_or(0))
    }

    fn add_contribution(&self, did: &Did, amount: i64) -> Result<()> {
        if amount <= 0 {
            return Ok(());
        }
        let mut guard = self
            .contributed
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let current = guard.get(&did.to_string()).copied().unwrap_or(0);
        guard.insert(did.to_string(), current.saturating_add(amount));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_in_memory_membership_store() {
        let store = InMemoryMembershipStore::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();

        // Initially no membership
        assert!(store.get_member_since(&did).unwrap().is_none());
        assert_eq!(store.get_total_contributed(&did).unwrap(), 0);

        // Register member
        store.set_member_since(&did, 1000).unwrap();
        assert_eq!(store.get_member_since(&did).unwrap(), Some(1000));

        // Can't change join date once set
        store.set_member_since(&did, 2000).unwrap();
        assert_eq!(store.get_member_since(&did).unwrap(), Some(1000));

        // Add contributions
        store.add_contribution(&did, 50).unwrap();
        assert_eq!(store.get_total_contributed(&did).unwrap(), 50);

        store.add_contribution(&did, 30).unwrap();
        assert_eq!(store.get_total_contributed(&did).unwrap(), 80);

        // Negative contributions ignored
        store.add_contribution(&did, -10).unwrap();
        assert_eq!(store.get_total_contributed(&did).unwrap(), 80);
    }

    #[test]
    fn test_register_if_new() {
        let store = InMemoryMembershipStore::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();

        // First registration sets the timestamp
        let ts1 = store.register_if_new(&did, 1000).unwrap();
        assert_eq!(ts1, 1000);

        // Second registration returns existing timestamp
        let ts2 = store.register_if_new(&did, 2000).unwrap();
        assert_eq!(ts2, 1000);
    }
}
