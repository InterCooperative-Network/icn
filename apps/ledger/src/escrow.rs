use anyhow::Result;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::collections::HashSet;

/// Escrow status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscrowStatus {
    /// Escrow created but not yet funded
    Pending,
    /// Funds are locked in escrow
    Locked,
    /// Funds have been released to beneficiary
    Released,
    /// Funds have been refunded to creator
    Refunded,
    /// Escrow expired before completion
    Expired,
}

/// Escrow condition type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum EscrowCondition {
    /// Requires approval from specific DID
    RequiresApproval {
        /// DID of the approver
        did: String,
    },
    /// Releases after timestamp
    TimeRelease {
        /// Unix timestamp for release
        timestamp: u64,
    },
    /// Requires external proof (e.g., delivery confirmation)
    ProofRequired {
        /// Type of proof required
        proof_type: String,
    },
}

/// Escrow record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Escrow {
    /// Unique ID
    pub id: String,
    /// Cooperative ID (ledger namespace)
    pub coop_id: String,
    /// Creator/initiator DID
    pub creator: String,
    /// From account
    pub from_account: String,
    /// To account (beneficiary)
    pub to_account: String,
    /// Amount held
    pub amount: i64,
    /// Currency
    pub currency: String,
    /// Current status
    pub status: EscrowStatus,
    /// Release conditions
    pub conditions: Vec<EscrowCondition>,
    /// Approvals received
    pub approvals: Vec<String>,
    /// Expiration timestamp
    pub expires_at: Option<u64>,
    /// Description/purpose
    pub description: String,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
}

/// Persistent store for escrows
#[derive(Clone)]
pub struct EscrowStore {
    db: Db,
}

impl EscrowStore {
    /// Create a new EscrowStore backed by the given Sled database
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Insert or update an escrow
    pub fn insert(&self, escrow: Escrow) -> Result<()> {
        let key = format!("escrow:{}", escrow.id);
        let value = serde_json::to_vec(&escrow)?;
        self.db.insert(key.as_bytes(), value)?;

        // Update indices
        self.update_indices(&escrow)?;

        Ok(())
    }

    /// Get an escrow by ID
    pub fn get(&self, id: &str) -> Result<Option<Escrow>> {
        let key = format!("escrow:{id}");
        if let Some(value) = self.db.get(key.as_bytes())? {
            let escrow = serde_json::from_slice(&value)?;
            Ok(Some(escrow))
        } else {
            Ok(None)
        }
    }

    /// List all escrows
    pub fn list(&self) -> Result<Vec<Escrow>> {
        let prefix = b"escrow:";
        let mut escrows = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            if let Ok(escrow) = serde_json::from_slice::<Escrow>(&value) {
                escrows.push(escrow);
            }
        }
        Ok(escrows)
    }

    /// Update an escrow
    pub fn update(&self, id: &str, escrow: Escrow) -> Result<()> {
        if escrow.id != id {
            return Err(anyhow::anyhow!("Escrow ID mismatch"));
        }
        self.insert(escrow)
    }

    /// Delete an escrow
    pub fn delete(&self, id: &str) -> Result<()> {
        if let Some(escrow) = self.get(id)? {
            self.remove_indices(&escrow)?;
            let key = format!("escrow:{id}");
            self.db.remove(key.as_bytes())?;
        }
        Ok(())
    }

    /// List escrows involving a user (as creator or beneficiary)
    pub fn list_by_user(&self, did: &str) -> Result<Vec<Escrow>> {
        let mut escrows = Vec::new();
        let mut seen_ids = HashSet::new();

        // 1. Scan creator index
        // Prefix includes the trailing colon to strictly match the DID segment
        let creator_prefix = format!("idx_escrow_creator:{did}:");
        for item in self.db.scan_prefix(creator_prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if let Some(id) = key_str.strip_prefix(&creator_prefix) {
                if seen_ids.insert(id.to_string()) {
                    if let Some(escrow) = self.get(id)? {
                        escrows.push(escrow);
                    }
                }
            }
        }

        // 2. Scan beneficiary index
        let beneficiary_prefix = format!("idx_escrow_beneficiary:{did}:");
        for item in self.db.scan_prefix(beneficiary_prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if let Some(id) = key_str.strip_prefix(&beneficiary_prefix) {
                if seen_ids.insert(id.to_string()) {
                    if let Some(escrow) = self.get(id)? {
                        escrows.push(escrow);
                    }
                }
            }
        }

        Ok(escrows)
    }

    // --- Index Management ---

    fn update_indices(&self, escrow: &Escrow) -> Result<()> {
        // Creator index
        let creator_key = format!("idx_escrow_creator:{}:{}", escrow.creator, escrow.id);
        self.db.insert(creator_key.as_bytes(), &[])?;

        // Beneficiary index (assuming to_account is the DID)
        let beneficiary_key = format!("idx_escrow_beneficiary:{}:{}", escrow.to_account, escrow.id);
        self.db.insert(beneficiary_key.as_bytes(), &[])?;

        Ok(())
    }

    fn remove_indices(&self, escrow: &Escrow) -> Result<()> {
        let creator_key = format!("idx_escrow_creator:{}:{}", escrow.creator, escrow.id);
        self.db.remove(creator_key.as_bytes())?;

        let beneficiary_key = format!("idx_escrow_beneficiary:{}:{}", escrow.to_account, escrow.id);
        self.db.remove(beneficiary_key.as_bytes())?;
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
        let store = EscrowStore::new(db);

        let escrow = Escrow {
            id: "1".to_string(),
            coop_id: "coop1".to_string(),
            creator: "did:alice".to_string(),
            from_account: "acc1".to_string(),
            to_account: "did:bob".to_string(), // Beneficiary is Bob
            amount: 100,
            currency: "USD".to_string(),
            status: EscrowStatus::Pending,
            conditions: vec![],
            approvals: vec![],
            expires_at: None,
            description: "Test".to_string(),
            created_at: 0,
            updated_at: 0,
        };

        // Insert
        store.insert(escrow.clone())?;

        // Get
        let retrieved = store.get("1")?.unwrap();
        assert_eq!(retrieved, escrow);

        // Update
        let mut updated = escrow.clone();
        updated.amount = 200;
        store.update("1", updated.clone())?;
        assert_eq!(store.get("1")?.unwrap().amount, 200);

        // List by user
        let alice_escrows = store.list_by_user("did:alice")?;
        assert_eq!(alice_escrows.len(), 1);

        let bob_escrows = store.list_by_user("did:bob")?;
        assert_eq!(bob_escrows.len(), 1);

        let charlie_escrows = store.list_by_user("did:charlie")?;
        assert_eq!(charlie_escrows.len(), 0);

        // Delete
        store.delete("1")?;
        assert!(store.get("1")?.is_none());
        assert!(store.list_by_user("did:alice")?.is_empty());

        Ok(())
    }
}
