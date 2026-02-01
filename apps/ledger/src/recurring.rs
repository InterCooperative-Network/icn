use anyhow::Result;
use serde::{Deserialize, Serialize};
use sled::Db;

/// Payment frequency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum PaymentFrequency {
    /// Execute payment once per day
    Daily,
    /// Execute payment once per week
    Weekly,
    /// Execute payment once per month
    Monthly,
    /// Execute payment once per year
    Yearly,
}

/// Recurring payment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum RecurringStatus {
    /// Payment is active and will execute on schedule
    Active,
    /// Payment is temporarily paused
    Paused,
    /// Payment has been cancelled
    Cancelled,
    /// Payment has completed its schedule
    Completed,
}

/// Recurring payment schedule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecurringPayment {
    /// Unique ID
    pub id: String,
    /// Cooperative ID (ledger namespace)
    pub coop_id: String,
    /// Owner/creator DID
    pub owner: String,
    /// From account
    pub from_account: String,
    /// To account
    pub to_account: String,
    /// Amount per payment
    pub amount: i64,
    /// Currency
    pub currency: String,
    /// Payment frequency
    pub frequency: PaymentFrequency,
    /// Next execution timestamp
    pub next_execution: u64,
    /// Optional end date
    pub end_date: Option<u64>,
    /// Current status
    pub status: RecurringStatus,
    /// Total executions
    pub execution_count: u64,
    /// Last execution timestamp
    pub last_execution: Option<u64>,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
}

/// Persistent store for recurring payments
#[derive(Clone)]
pub struct RecurringPaymentStore {
    db: Db,
}

impl RecurringPaymentStore {
    /// Create a new recurring payment store with the given database
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Insert or update a recurring payment
    pub fn insert(&self, payment: RecurringPayment) -> Result<()> {
        let key = format!("payment:{}", payment.id);
        let value = serde_json::to_vec(&payment)?;
        self.db.insert(key.as_bytes(), value)?;

        // Update secondary indices
        self.update_indices(&payment)?;

        Ok(())
    }

    /// Get a recurring payment by ID
    pub fn get(&self, id: &str) -> Result<Option<RecurringPayment>> {
        let key = format!("payment:{id}");
        if let Some(value) = self.db.get(key.as_bytes())? {
            let payment = serde_json::from_slice(&value)?;
            Ok(Some(payment))
        } else {
            Ok(None)
        }
    }

    /// List all recurring payments
    pub fn list(&self) -> Result<Vec<RecurringPayment>> {
        let mut payments = Vec::new();
        let prefix = b"payment:";

        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            // Skip non-primary keys (if any leak into scan, though prefix prevents most)
            if let Ok(payment) = serde_json::from_slice::<RecurringPayment>(&value) {
                payments.push(payment);
            }
        }

        Ok(payments)
    }

    /// Update a recurring payment
    pub fn update(&self, id: &str, payment: RecurringPayment) -> Result<()> {
        if payment.id != id {
            return Err(anyhow::anyhow!("Payment ID mismatch"));
        }
        self.insert(payment)
    }

    /// Delete a recurring payment
    pub fn delete(&self, id: &str) -> Result<()> {
        // Get old payment to remove indices
        if let Some(payment) = self.get(id)? {
            self.remove_indices(&payment)?;
            let key = format!("payment:{id}");
            self.db.remove(key.as_bytes())?;
        }
        Ok(())
    }

    /// List payments by owner
    pub fn list_by_owner(&self, owner: &str) -> Result<Vec<RecurringPayment>> {
        let prefix = format!("idx_owner:{owner}");
        let mut payments = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;
            // Format: idx_owner:{owner}:{id}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() == 3 {
                let id = parts[2];
                if let Some(payment) = self.get(id)? {
                    payments.push(payment);
                }
            }
        }

        Ok(payments)
    }

    /// Get payments due for execution
    pub fn get_due_payments(&self, now: u64) -> Result<Vec<RecurringPayment>> {
        // Full scan approach for now (optimization: use time-based index scanning)
        // Since we need to check status == Active and next_execution <= now
        // An index on next_execution would help start the scan, but we still need to filter by status.
        // For iteration 1, we filter in memory from list().

        let payments = self.list()?;
        Ok(payments
            .into_iter()
            .filter(|p| p.status == RecurringStatus::Active && p.next_execution <= now)
            .collect())
    }

    // --- Index Management ---

    fn update_indices(&self, payment: &RecurringPayment) -> Result<()> {
        // Owner index: idx_owner:{owner}:{id}
        let owner_key = format!("idx_owner:{}:{}", payment.owner, payment.id);
        self.db.insert(owner_key.as_bytes(), &[])?;
        Ok(())
    }

    fn remove_indices(&self, payment: &RecurringPayment) -> Result<()> {
        let owner_key = format!("idx_owner:{}:{}", payment.owner, payment.id);
        self.db.remove(owner_key.as_bytes())?;
        Ok(())
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
        let store = RecurringPaymentStore::new(db);

        let payment = RecurringPayment {
            id: "1".to_string(),
            coop_id: "coop1".to_string(),
            owner: "did:1".to_string(),
            from_account: "acc1".to_string(),
            to_account: "acc2".to_string(),
            amount: 100,
            currency: "USD".to_string(),
            frequency: PaymentFrequency::Monthly,
            next_execution: 1000,
            end_date: None,
            status: RecurringStatus::Active,
            execution_count: 0,
            last_execution: None,
            created_at: 0,
            updated_at: 0,
        };

        // Insert
        store.insert(payment.clone())?;

        // Get
        let retrieved = store.get("1")?.unwrap();
        assert_eq!(retrieved, payment);

        // Update
        let mut updated = payment.clone();
        updated.amount = 200;
        store.update("1", updated.clone())?;
        assert_eq!(store.get("1")?.unwrap().amount, 200);

        // Delete
        store.delete("1")?;
        assert!(store.get("1")?.is_none());

        Ok(())
    }

    #[test]
    fn test_indices() -> Result<()> {
        let db = temp_db();
        let store = RecurringPaymentStore::new(db);

        let p1 = RecurringPayment {
            id: "1".to_string(),
            owner: "alice".to_string(),
            ..Default::default()
        };
        let p2 = RecurringPayment {
            id: "2".to_string(),
            owner: "alice".to_string(),
            ..Default::default()
        };
        let p3 = RecurringPayment {
            id: "3".to_string(),
            owner: "bob".to_string(),
            ..Default::default()
        };

        store.insert(p1)?;
        store.insert(p2)?;
        store.insert(p3)?;

        let alice_payments = store.list_by_owner("alice")?;
        assert_eq!(alice_payments.len(), 2);

        let bob_payments = store.list_by_owner("bob")?;
        assert_eq!(bob_payments.len(), 1);

        Ok(())
    }

    impl Default for RecurringPayment {
        fn default() -> Self {
            Self {
                id: "".to_string(),
                coop_id: "".to_string(),
                owner: "".to_string(),
                from_account: "".to_string(),
                to_account: "".to_string(),
                amount: 0,
                currency: "".to_string(),
                frequency: PaymentFrequency::Daily,
                next_execution: 0,
                end_date: None,
                status: RecurringStatus::Active,
                execution_count: 0,
                last_execution: None,
                created_at: 0,
                updated_at: 0,
            }
        }
    }
}
