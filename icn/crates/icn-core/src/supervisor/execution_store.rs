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
    /// Sourced from the canonical prefix in `icn-kernel-api` so the store that
    /// writes `exec:<decision_hash>` records and any external reader that scans
    /// them (e.g. the gateway dispatch-evidence backfill) cannot drift apart.
    const PREFIX: &'static [u8] = icn_kernel_api::execution::EXECUTION_RECORD_KEY_PREFIX.as_bytes();

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

    /// Durability characterization for the dispatch-evidence source (Issue #1987).
    ///
    /// A terminal `ExecutionRecord` carries the `(effects, results)` pair the
    /// gateway dispatch-evidence backfill re-derives `EffectDispatchEvidence`
    /// from. This proves that pair survives a real close-and-reopen of an
    /// on-disk sled store from the same path — i.e. the evidence source is
    /// durable across a restart, not merely within one process. The sibling
    /// tests use `SledStore::temporary()`, which is deleted on drop and so
    /// cannot demonstrate cross-reopen persistence.
    ///
    /// Scope note: this exercises the graceful-shutdown durability barrier
    /// (explicit `flush()` / flush-on-drop). It does NOT simulate a hard crash
    /// between an unflushed `put` and process death; that window is covered by
    /// idempotent recovery (`DecisionExecutor::recover_in_flight`), not here.
    #[test]
    fn terminal_record_with_results_survives_reopen_from_path() {
        use icn_kernel_api::effects::{EffectOutcome, EffectResult};

        let dir = tempfile::TempDir::new().unwrap();

        // A terminal Confirmed record carrying both stored effects and the
        // per-effect results — the durable input the evidence backfill consumes.
        let mut record =
            ExecutionRecord::new_pending("dh-durable", "prop-durable", "receipt-durable", vec![]);
        record.set_results(vec![EffectResult {
            effect_id: "receipt-durable".to_string(),
            success: true,
            message: "applied".to_string(),
            state_change_hash: Some("sch-durable".to_string()),
            ledger_entry_id: Some("ledger-durable".to_string()),
            not_executed: false,
            receipt_ref: Some("steward-durable".to_string()),
            outcome: Some(EffectOutcome::Applied),
        }]);
        record.mark_confirmed(vec!["ledger-durable".into()], vec!["sch-durable".into()]);
        assert!(record.is_terminal());

        // Write + flush through a real on-disk store, then drop it so the sled
        // file lock is released (mirrors a graceful shutdown).
        {
            let store = Arc::new(SledStore::open(dir.path()).unwrap());
            let exec_store = SledExecutionStore::new(store.clone());
            exec_store.put(&record).unwrap();
            // Explicit durability barrier (fsync), matching graceful shutdown.
            store.flush().unwrap();
        } // store + exec_store dropped here → sled Db closed, lock released

        // Reopen from the same path with a fresh handle, as a restarted process
        // would.
        let reopened = SledExecutionStore::new(Arc::new(SledStore::open(dir.path()).unwrap()));
        let got = reopened
            .get("dh-durable")
            .unwrap()
            .expect("terminal execution record must survive store reopen");

        assert_eq!(got.status, ExecutionStatus::Confirmed);
        assert!(got.is_terminal());
        // The evidence source must round-trip intact across the reopen.
        assert_eq!(got.results.len(), 1, "per-effect results survive reopen");
        assert_eq!(
            got.results[0].ledger_entry_id.as_deref(),
            Some("ledger-durable")
        );
        assert_eq!(
            got.results[0].receipt_ref.as_deref(),
            Some("steward-durable"),
            "evidence attribution (receipt_ref) survives reopen"
        );
        assert_eq!(got.ledger_entry_ids, vec!["ledger-durable".to_string()]);
    }
}
