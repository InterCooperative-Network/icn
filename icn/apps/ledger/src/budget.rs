//! Sled-backed budget store for the execution bridge.
//!
//! Stores budget records keyed by `budget_id` under the `exec:budget:` prefix.
//! All operations are synchronous (sled is a blocking B-tree) wrapped in the
//! `BudgetStore` trait for the executor.

use anyhow::Result;
use icn_kernel_api::budget::{BudgetRecord, BudgetStore};

/// Sled-backed implementation of [`BudgetStore`].
pub struct SledBudgetStore {
    tree: sled::Tree,
}

impl SledBudgetStore {
    /// Open or create the budget store in the given sled DB.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree("exec:budget")?;
        Ok(Self { tree })
    }

    fn key(budget_id: &str) -> Vec<u8> {
        format!("budget:{}", budget_id).into_bytes()
    }

    fn scope_prefix(scope_id: &str) -> String {
        format!("scope:{}:", scope_id)
    }

    fn scope_key(scope_id: &str, budget_id: &str) -> Vec<u8> {
        format!("scope:{}:{}", scope_id, budget_id).into_bytes()
    }
}

impl BudgetStore for SledBudgetStore {
    fn get(&self, budget_id: &str) -> Result<Option<BudgetRecord>> {
        match self.tree.get(Self::key(budget_id))? {
            Some(bytes) => {
                let record: BudgetRecord = serde_json::from_slice(&bytes)?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    fn put(&self, record: &BudgetRecord) -> Result<()> {
        let bytes = serde_json::to_vec(record)?;
        self.tree.insert(Self::key(&record.budget_id), bytes)?;

        // Also maintain scope index
        let scope_key = Self::scope_key(&record.scope_id, &record.budget_id);
        self.tree
            .insert(scope_key, record.budget_id.as_bytes().to_vec())?;

        self.tree.flush()?;
        Ok(())
    }

    fn list_by_scope(&self, scope_id: &str) -> Result<Vec<BudgetRecord>> {
        let prefix = Self::scope_prefix(scope_id);
        let mut records = Vec::new();

        for entry in self.tree.scan_prefix(prefix.as_bytes()) {
            let (_, budget_id_bytes) = entry?;
            let budget_id = String::from_utf8(budget_id_bytes.to_vec())?;
            if let Some(record) = self.get(&budget_id)? {
                records.push(record);
            }
        }

        Ok(records)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use icn_kernel_api::budget::BudgetStatus;

    fn temp_db() -> sled::Db {
        sled::Config::new().temporary(true).open().unwrap()
    }

    #[test]
    fn test_put_and_get() {
        let db = temp_db();
        let store = SledBudgetStore::new(&db).unwrap();

        let record = BudgetRecord::new(
            "budget-1".into(),
            "coop-1".into(),
            "did:icn:treasury".into(),
            "HOURS".into(),
            5000,
            "decision-create-1".into(),
            1000,
        );

        store.put(&record).unwrap();

        let retrieved = store.get("budget-1").unwrap().unwrap();
        assert_eq!(retrieved.budget_id, "budget-1");
        assert_eq!(retrieved.total_limit, 5000);
        assert_eq!(retrieved.status, BudgetStatus::Active);
    }

    #[test]
    fn test_get_nonexistent() {
        let db = temp_db();
        let store = SledBudgetStore::new(&db).unwrap();
        assert!(store.get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_list_by_scope() {
        let db = temp_db();
        let store = SledBudgetStore::new(&db).unwrap();

        // Two budgets for coop-1, one for coop-2
        let b1 = BudgetRecord::new(
            "budget-1".into(),
            "coop-1".into(),
            "did:icn:t1".into(),
            "HOURS".into(),
            1000,
            "d1".into(),
            100,
        );
        let b2 = BudgetRecord::new(
            "budget-2".into(),
            "coop-1".into(),
            "did:icn:t1".into(),
            "USD".into(),
            2000,
            "d2".into(),
            200,
        );
        let b3 = BudgetRecord::new(
            "budget-3".into(),
            "coop-2".into(),
            "did:icn:t2".into(),
            "HOURS".into(),
            3000,
            "d3".into(),
            300,
        );

        store.put(&b1).unwrap();
        store.put(&b2).unwrap();
        store.put(&b3).unwrap();

        let coop1_budgets = store.list_by_scope("coop-1").unwrap();
        assert_eq!(coop1_budgets.len(), 2);

        let coop2_budgets = store.list_by_scope("coop-2").unwrap();
        assert_eq!(coop2_budgets.len(), 1);
        assert_eq!(coop2_budgets[0].budget_id, "budget-3");
    }

    #[test]
    fn test_update_after_spend() {
        let db = temp_db();
        let store = SledBudgetStore::new(&db).unwrap();

        let mut record = BudgetRecord::new(
            "budget-1".into(),
            "coop-1".into(),
            "did:icn:t1".into(),
            "HOURS".into(),
            1000,
            "d1".into(),
            100,
        );

        store.put(&record).unwrap();

        // Simulate a spend saga
        record.begin_spend("spend-decision-1", 300).unwrap();
        store.put(&record).unwrap(); // Persist Spending state

        record.confirm_spend();
        store.put(&record).unwrap(); // Persist confirmed state

        let retrieved = store.get("budget-1").unwrap().unwrap();
        assert_eq!(retrieved.spent_total, 300);
        assert_eq!(retrieved.remaining(), 700);
        assert!(retrieved.pending_spend.is_none());
    }
}
