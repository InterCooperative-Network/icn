//! Membership tracking for credit limit ramping
//!
//! This module provides infrastructure for tracking when members joined,
//! enabling new member credit limit ramping. New members start with conservative
//! limits that increase over time based on tenure.
//!
//! # Credit Limit Ramping
//!
//! New members start with a 10-hour credit limit that ramps linearly to the
//! full calculated limit over 90 days. Members who have cleared 50+ hours
//! (tracked via the ledger's `cleared_volume_index`) get full limits immediately.
//!
//! "Cleared volume" = total credits RECEIVED for services/goods provided.
//!
//! # Timestamp Assumptions
//!
//! Member registration timestamps use Unix seconds (u64). The system expects:
//! - Timestamps are reasonably accurate (within seconds of actual time)
//! - The ramping calculation uses `SystemTime::now()` for current time
//! - Member-since timestamps are set once on first transaction and never updated
//!
//! # Concurrency
//!
//! ## Race Condition in Member Registration
//!
//! The `set_member_since` operation uses a check-then-write pattern which is
//! not fully atomic. Concurrent registration of the same member (e.g., two
//! simultaneous first transactions) may result in slightly different timestamps.
//!
//! **Why this is acceptable:**
//! - Concurrent first transactions for the same DID are rare in practice
//! - The worst case is a timestamp difference of milliseconds
//! - The ramping period is 90 days, so small timestamp differences are negligible
//! - The first write semantically wins since we only write if key doesn't exist
//!
//! For production systems requiring strict atomicity, consider implementing
//! compare-and-swap at the Store trait level.

use anyhow::{Context, Result};
use icn_identity::Did;
use icn_store::Store;
use std::sync::Arc;
use tracing::debug;

/// Key prefix for member-since timestamps
const MEMBERSHIP_SINCE_PREFIX: &str = "membership:since:";

/// Trait for tracking membership information
///
/// Implementations store when members joined, enabling credit limit
/// calculations that ramp up over time based on tenure.
pub trait MembershipStore: Send + Sync {
    /// Get the timestamp when a member joined (Unix seconds)
    ///
    /// Returns `None` if the member is not registered yet.
    fn get_member_since(&self, did: &Did) -> Result<Option<u64>>;

    /// Set the timestamp when a member joined
    ///
    /// This should only be called once per member. Subsequent calls are ignored
    /// to preserve the original join date.
    ///
    /// # Concurrency Note
    ///
    /// This uses a check-then-write pattern. See module-level docs for details
    /// on the race condition and why it's acceptable.
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
}

impl MembershipStore for SledMembershipStore {
    fn get_member_since(&self, did: &Did) -> Result<Option<u64>> {
        let key = Self::since_key(did);
        match self.store.get(&key)? {
            Some(bytes) => {
                let timestamp: u64 = serde_json::from_slice(&bytes)
                    .with_context(|| format!("Failed to deserialize member_since for {did}"))?;
                Ok(Some(timestamp))
            }
            None => Ok(None),
        }
    }

    fn set_member_since(&self, did: &Did, timestamp: u64) -> Result<()> {
        let key = Self::since_key(did);
        // Only set if not already present (preserve original join date)
        // Note: This is not fully atomic. See module docs for race condition details.
        if self.store.get(&key)?.is_none() {
            let bytes = serde_json::to_vec(&timestamp)
                .with_context(|| format!("Failed to serialize member_since for {did}"))?;
            self.store.put(&key, &bytes)?;
            debug!(did = %did, timestamp, "Registered new member");
        }
        Ok(())
    }
}

/// In-memory membership store for testing
#[cfg(test)]
pub struct InMemoryMembershipStore {
    since: std::sync::RwLock<std::collections::HashMap<String, u64>>,
}

#[cfg(test)]
impl InMemoryMembershipStore {
    /// Create a new in-memory membership store
    pub fn new() -> Self {
        Self {
            since: std::sync::RwLock::new(std::collections::HashMap::new()),
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

        // Register member
        store.set_member_since(&did, 1000).unwrap();
        assert_eq!(store.get_member_since(&did).unwrap(), Some(1000));

        // Can't change join date once set
        store.set_member_since(&did, 2000).unwrap();
        assert_eq!(store.get_member_since(&did).unwrap(), Some(1000));
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
