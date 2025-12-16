use anyhow::Result;
use serde::{Deserialize, Serialize};
use sled::Db;

/// Budget period
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetPeriod {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Budget status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetStatus {
    Active,
    Paused,
    Exceeded,
    Expired,
}

/// Budget limit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Budget {
    /// Unique ID
    pub id: String,
    /// Owner/creator DID
    pub owner: String,
    /// Account this budget applies to
    pub account: String,
    /// Currency
    pub currency: String,
    /// Spending limit
    pub limit: i64,
    /// Current spending in this period
    pub spent: i64,
    /// Budget period
    pub period: BudgetPeriod,
    /// Period start timestamp
    pub period_start: u64,
    /// Period end timestamp
    pub period_end: u64,
    /// Current status
    pub status: BudgetStatus,
    /// Notification thresholds (percentages)
    pub notification_thresholds: Vec<u8>,
    /// Thresholds that have been notified
    pub notified_thresholds: Vec<u8>,
    /// Description
    pub description: String,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
}

impl Budget {
    /// Check if limit is exceeded
    pub fn is_exceeded(&self) -> bool {
        self.spent >= self.limit
    }

    /// Get remaining budget
    pub fn remaining(&self) -> i64 {
        self.limit - self.spent
    }

    /// Get percentage used
    pub fn percentage_used(&self) -> f64 {
        if self.limit == 0 {
            return 100.0;
        }
        (self.spent as f64 / self.limit as f64) * 100.0
    }

    /// Check if threshold should trigger notification
    pub fn should_notify(&self) -> Option<u8> {
        let pct = self.percentage_used() as u8;
        for threshold in &self.notification_thresholds {
            if pct >= *threshold && !self.notified_thresholds.contains(threshold) {
                return Some(*threshold);
            }
        }
        None
    }
}

/// Persistent store for budgets
#[derive(Clone)]
pub struct BudgetStore {
    db: Db,
}

impl BudgetStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Insert or update a budget
    pub fn insert(&self, budget: Budget) -> Result<()> {
        let key = format!("budget:{}", budget.id);
        let value = serde_json::to_vec(&budget)?;
        self.db.insert(key.as_bytes(), value)?;

        // Update indices
        self.update_indices(&budget)?;

        Ok(())
    }

    /// Get a budget by ID
    pub fn get(&self, id: &str) -> Result<Option<Budget>> {
        let key = format!("budget:{id}");
        if let Some(value) = self.db.get(key.as_bytes())? {
            let budget = serde_json::from_slice(&value)?;
            Ok(Some(budget))
        } else {
            Ok(None)
        }
    }

    /// List all budgets
    pub fn list(&self) -> Result<Vec<Budget>> {
        let prefix = b"budget:";
        let mut budgets = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            // Filter out indices which might start with budget: but shouldn't be here
            if let Ok(budget) = serde_json::from_slice::<Budget>(&value) {
                budgets.push(budget);
            }
        }
        Ok(budgets)
    }

    /// Update a budget
    pub fn update(&self, id: &str, budget: Budget) -> Result<()> {
        if budget.id != id {
            return Err(anyhow::anyhow!("Budget ID mismatch"));
        }
        self.insert(budget)
    }

    /// Delete a budget
    pub fn delete(&self, id: &str) -> Result<()> {
        if let Some(budget) = self.get(id)? {
            self.remove_indices(&budget)?;
            let key = format!("budget:{id}");
            self.db.remove(key.as_bytes())?;
        }
        Ok(())
    }

    /// List budgets by owner
    pub fn list_by_owner(&self, did: &str) -> Result<Vec<Budget>> {
        let mut budgets = Vec::new();
        let prefix = format!("idx_budget_owner:{did}:");

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if let Some(id) = key_str.strip_prefix(&prefix) {
                if let Some(budget) = self.get(id)? {
                    budgets.push(budget);
                }
            }
        }

        Ok(budgets)
    }

    // --- Index Management ---

    fn update_indices(&self, budget: &Budget) -> Result<()> {
        // Owner index
        let owner_key = format!("idx_budget_owner:{}:{}", budget.owner, budget.id);
        self.db.insert(owner_key.as_bytes(), &[])?;
        Ok(())
    }

    fn remove_indices(&self, budget: &Budget) -> Result<()> {
        let owner_key = format!("idx_budget_owner:{}:{}", budget.owner, budget.id);
        self.db.remove(owner_key.as_bytes())?;
        Ok(())
    }

    /// Check spending against budget
    pub fn check_spending(&self, account: &str, amount: i64) -> Result<bool> {
        let budgets = self.list()?; // Inefficient for large datasets, but acceptable for now

        for budget in budgets {
            if budget.account == account {
                // Deny if budget is exceeded
                if budget.status == BudgetStatus::Exceeded {
                    return Ok(false);
                }
                // Deny if spending would exceed limit
                if budget.status == BudgetStatus::Active && budget.spent + amount > budget.limit {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Record spending
    pub fn record_spending(&self, account: &str, amount: i64) -> Result<Vec<String>> {
        let mut notified_budgets = Vec::new();
        let budgets = self.list()?;

        for mut budget in budgets {
            if budget.account == account && budget.status == BudgetStatus::Active {
                budget.spent += amount;
                budget.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                // Check if exceeded
                if budget.is_exceeded() {
                    budget.status = BudgetStatus::Exceeded;
                }

                // Check notification thresholds
                if let Some(threshold) = budget.should_notify() {
                    budget.notified_thresholds.push(threshold);
                    notified_budgets.push(budget.id.clone());
                }

                // Update in store
                self.insert(budget)?;
            }
        }

        Ok(notified_budgets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sled::Config;

    fn temp_db() -> Db {
        Config::new().temporary(true).open().unwrap()
    }

    #[test]
    fn test_crud() -> Result<()> {
        let db = temp_db();
        let store = BudgetStore::new(db);

        let budget = Budget {
            id: "1".to_string(),
            owner: "did:alice".to_string(),
            account: "acc1".to_string(),
            currency: "USD".to_string(),
            limit: 1000,
            spent: 0,
            period: BudgetPeriod::Monthly,
            period_start: 0,
            period_end: 1000,
            status: BudgetStatus::Active,
            notification_thresholds: vec![80, 100],
            notified_thresholds: vec![],
            description: "Test".to_string(),
            created_at: 0,
            updated_at: 0,
        };

        // Insert
        store.insert(budget.clone())?;

        // Get
        let retrieved = store.get("1")?.unwrap();
        assert_eq!(retrieved, budget);

        // Update
        let mut updated = budget.clone();
        updated.spent = 500;
        store.update("1", updated.clone())?;
        assert_eq!(store.get("1")?.unwrap().spent, 500);

        // List by owner
        let alice_budgets = store.list_by_owner("did:alice")?;
        assert_eq!(alice_budgets.len(), 1);

        let bob_budgets = store.list_by_owner("did:bob")?;
        assert_eq!(bob_budgets.len(), 0);

        // Delete
        store.delete("1")?;
        assert!(store.get("1")?.is_none());
        assert!(store.list_by_owner("did:alice")?.is_empty());

        Ok(())
    }
}
