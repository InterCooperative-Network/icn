//! Sled-backed execution store for persistent decision execution records.
//!
//! Follows the same pattern as `DeadLetterQueue`: prefix-scoped keys,
//! JSON serialization, prefix scans for status queries.

use anyhow::{Context, Result};
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};
use std::collections::HashMap;
use std::sync::Arc;

/// Sled-backed implementation of `ExecutionStore`.
///
/// Keys: `exec:<decision_hash>`
/// Values: JSON-serialized `ExecutionRecord`
pub struct SledExecutionStore<S: icn_store::Store> {
    store: Arc<S>,
}

impl<S: icn_store::Store> SledExecutionStore<S> {
    const PREFIX: &'static [u8] = b"exec:";

    /// Create a new execution store backed by the given sled store.
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    fn entry_key(decision_hash: &str) -> Vec<u8> {
        let mut key = Self::PREFIX.to_vec();
        key.extend_from_slice(decision_hash.as_bytes());
        key
    }
}

impl<S: icn_store::Store> ExecutionStore for SledExecutionStore<S> {
    fn get(&self, decision_hash: &str) -> Result<Option<ExecutionRecord>> {
        let key = Self::entry_key(decision_hash);
        match self.store.get(&key)? {
            Some(data) => {
                let record: ExecutionRecord = serde_json::from_slice(&data)
                    .context("Failed to deserialize execution record")?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    fn put(&self, record: &ExecutionRecord) -> Result<()> {
        let key = Self::entry_key(&record.decision_hash);
        let value = serde_json::to_vec(record).context("Failed to serialize execution record")?;
        self.store
            .put(&key, &value)
            .context("Failed to store execution record")?;
        Ok(())
    }

    fn delete(&self, decision_hash: &str) -> Result<()> {
        let key = Self::entry_key(decision_hash);
        self.store
            .delete(&key)
            .context("Failed to delete execution record")?;
        Ok(())
    }

    fn list_by_status(&self, status: ExecutionStatus) -> Result<Vec<ExecutionRecord>> {
        let entries = self.store.scan(Self::PREFIX)?;
        let mut result = Vec::new();

        for (_key, value) in entries {
            let record: ExecutionRecord =
                serde_json::from_slice(&value).context("Failed to deserialize execution record")?;
            if record.status == status {
                result.push(record);
            }
        }

        result.sort_by_key(|r| r.started_at);
        Ok(result)
    }

    fn count_by_status(&self) -> Result<HashMap<ExecutionStatus, usize>> {
        let entries = self.store.scan(Self::PREFIX)?;
        let mut counts = HashMap::new();

        for (_key, value) in entries {
            let record: ExecutionRecord =
                serde_json::from_slice(&value).context("Failed to deserialize execution record")?;
            *counts.entry(record.status).or_insert(0) += 1;
        }

        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_store::SledStore;

    fn test_store() -> Arc<SledStore> {
        Arc::new(SledStore::temporary().unwrap())
    }

    #[test]
    fn test_put_and_get() {
        let store = test_store();
        let exec_store = SledExecutionStore::new(store);

        let record = ExecutionRecord::new_pending("hash123", "proposal-1", "receipt-1", vec![]);
        exec_store.put(&record).unwrap();

        let retrieved = exec_store.get("hash123").unwrap().unwrap();
        assert_eq!(retrieved.decision_hash, "hash123");
        assert_eq!(retrieved.proposal_id, "proposal-1");
        assert_eq!(retrieved.status, ExecutionStatus::Pending);
    }

    #[test]
    fn test_get_nonexistent() {
        let store = test_store();
        let exec_store = SledExecutionStore::new(store);

        assert!(exec_store.get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_update() {
        let store = test_store();
        let exec_store = SledExecutionStore::new(store);

        let mut record = ExecutionRecord::new_pending("hash456", "proposal-2", "receipt-2", vec![]);
        exec_store.put(&record).unwrap();

        record.mark_executing();
        exec_store.put(&record).unwrap();

        record.mark_confirmed(vec!["entry-1".into()], vec!["state-1".into()]);
        exec_store.put(&record).unwrap();

        let retrieved = exec_store.get("hash456").unwrap().unwrap();
        assert_eq!(retrieved.status, ExecutionStatus::Confirmed);
        assert_eq!(retrieved.ledger_entry_ids, vec!["entry-1"]);
    }

    #[test]
    fn test_delete() {
        let store = test_store();
        let exec_store = SledExecutionStore::new(store);

        let record = ExecutionRecord::new_pending("hash-delete", "proposal-1", "receipt-1", vec![]);
        exec_store.put(&record).unwrap();
        assert!(exec_store.get("hash-delete").unwrap().is_some());

        exec_store.delete("hash-delete").unwrap();
        assert!(exec_store.get("hash-delete").unwrap().is_none());
    }

    #[test]
    fn test_list_by_status() {
        let store = test_store();
        let exec_store = SledExecutionStore::new(store);

        // Add records in different states
        let pending = ExecutionRecord::new_pending("h1", "p1", "r1", vec![]);
        exec_store.put(&pending).unwrap();

        let mut confirmed = ExecutionRecord::new_pending("h2", "p2", "r2", vec![]);
        confirmed.mark_confirmed(vec![], vec![]);
        exec_store.put(&confirmed).unwrap();

        let mut failed = ExecutionRecord::new_pending("h3", "p3", "r3", vec![]);
        failed.mark_failed("oops");
        exec_store.put(&failed).unwrap();

        let pending_list = exec_store.list_by_status(ExecutionStatus::Pending).unwrap();
        assert_eq!(pending_list.len(), 1);
        assert_eq!(pending_list[0].decision_hash, "h1");

        let confirmed_list = exec_store
            .list_by_status(ExecutionStatus::Confirmed)
            .unwrap();
        assert_eq!(confirmed_list.len(), 1);
    }

    #[test]
    fn test_count_by_status() {
        let store = test_store();
        let exec_store = SledExecutionStore::new(store);

        let r1 = ExecutionRecord::new_pending("a", "p1", "r1", vec![]);
        let r2 = ExecutionRecord::new_pending("b", "p2", "r2", vec![]);
        let mut r3 = ExecutionRecord::new_pending("c", "p3", "r3", vec![]);
        r3.mark_confirmed(vec![], vec![]);

        exec_store.put(&r1).unwrap();
        exec_store.put(&r2).unwrap();
        exec_store.put(&r3).unwrap();

        let counts = exec_store.count_by_status().unwrap();
        assert_eq!(counts.get(&ExecutionStatus::Pending), Some(&2));
        assert_eq!(counts.get(&ExecutionStatus::Confirmed), Some(&1));
    }
}
