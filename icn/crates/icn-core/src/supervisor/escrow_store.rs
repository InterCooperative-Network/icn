//! Sled-backed escrow store for persistent escrow records.
//!
//! Follows the same pattern as `SledExecutionStore`: prefix-scoped keys,
//! JSON serialization, prefix scans for scope queries.

use anyhow::{Context, Result};
use icn_kernel_api::escrow::{EscrowRecord, EscrowStore};
use std::sync::Arc;

/// Sled-backed implementation of `EscrowStore`.
///
/// Keys: `escrow:<escrow_id>`
/// Values: JSON-serialized `EscrowRecord`
pub struct SledEscrowStore<S: icn_store::Store> {
    store: Arc<S>,
}

impl<S: icn_store::Store> SledEscrowStore<S> {
    const PREFIX: &'static [u8] = b"escrow:";

    /// Create a new escrow store backed by the given sled store.
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    fn entry_key(escrow_id: &str) -> Vec<u8> {
        let mut key = Self::PREFIX.to_vec();
        key.extend_from_slice(escrow_id.as_bytes());
        key
    }
}

impl<S: icn_store::Store> EscrowStore for SledEscrowStore<S> {
    fn get(&self, escrow_id: &str) -> Result<Option<EscrowRecord>> {
        let key = Self::entry_key(escrow_id);
        match self.store.get(&key)? {
            Some(data) => {
                let record: EscrowRecord =
                    serde_json::from_slice(&data).context("Failed to deserialize escrow record")?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    fn put(&self, record: &EscrowRecord) -> Result<()> {
        let key = Self::entry_key(&record.escrow_id);
        let value = serde_json::to_vec(record).context("Failed to serialize escrow record")?;
        self.store
            .put(&key, &value)
            .context("Failed to store escrow record")?;
        Ok(())
    }

    fn list_by_scope(&self, scope_id: &str) -> Result<Vec<EscrowRecord>> {
        let entries = self.store.scan(Self::PREFIX)?;
        let mut result = Vec::new();

        for (_key, value) in entries {
            let record: EscrowRecord =
                serde_json::from_slice(&value).context("Failed to deserialize escrow record")?;
            if record.scope_id == scope_id {
                result.push(record);
            }
        }

        result.sort_by_key(|r| r.created_at);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::escrow::EscrowStatus;
    use icn_store::SledStore;

    fn test_store() -> Arc<SledStore> {
        Arc::new(SledStore::temporary().unwrap())
    }

    #[test]
    fn test_put_and_get() {
        let store = test_store();
        let escrow_store = SledEscrowStore::new(store);

        let record = EscrowRecord::new_locked(
            "esc-1",
            "coop-alpha",
            "did:treasury",
            "did:alice",
            5000,
            "HOURS",
        );
        escrow_store.put(&record).unwrap();

        let retrieved = escrow_store.get("esc-1").unwrap().unwrap();
        assert_eq!(retrieved.escrow_id, "esc-1");
        assert_eq!(retrieved.status, EscrowStatus::Locked);
        assert_eq!(retrieved.amount, 5000);
    }

    #[test]
    fn test_get_nonexistent() {
        let store = test_store();
        let escrow_store = SledEscrowStore::new(store);

        assert!(escrow_store.get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_update_on_release() {
        let store = test_store();
        let escrow_store = SledEscrowStore::new(store);

        let mut record = EscrowRecord::new_locked(
            "esc-2",
            "coop-alpha",
            "did:treasury",
            "did:bob",
            3000,
            "HOURS",
        );
        escrow_store.put(&record).unwrap();

        // Release the escrow (two-phase saga)
        record.begin_release("decision-hash-1").unwrap();
        record.confirm_release();
        escrow_store.put(&record).unwrap();

        let retrieved = escrow_store.get("esc-2").unwrap().unwrap();
        assert_eq!(retrieved.status, EscrowStatus::Released);
        assert_eq!(
            retrieved.release_decision_hash.as_deref(),
            Some("decision-hash-1")
        );
        assert!(retrieved.released_at.is_some());
    }

    #[test]
    fn test_list_by_scope() {
        let store = test_store();
        let escrow_store = SledEscrowStore::new(store);

        let r1 =
            EscrowRecord::new_locked("esc-a", "coop-alpha", "did:t", "did:alice", 1000, "HOURS");
        let r2 = EscrowRecord::new_locked("esc-b", "coop-alpha", "did:t", "did:bob", 2000, "HOURS");
        let r3 =
            EscrowRecord::new_locked("esc-c", "coop-beta", "did:t", "did:carol", 3000, "HOURS");

        escrow_store.put(&r1).unwrap();
        escrow_store.put(&r2).unwrap();
        escrow_store.put(&r3).unwrap();

        let alpha = escrow_store.list_by_scope("coop-alpha").unwrap();
        assert_eq!(alpha.len(), 2);

        let beta = escrow_store.list_by_scope("coop-beta").unwrap();
        assert_eq!(beta.len(), 1);
        assert_eq!(beta[0].escrow_id, "esc-c");
    }
}
