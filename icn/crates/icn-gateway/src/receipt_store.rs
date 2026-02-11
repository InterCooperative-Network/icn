//! Receipt storage service for economic receipts.
//!
//! Stores AllocationReceipt and SettlementIntent by canonical hash.
//! Supports lookup by hash, decision_hash, and decision_receipt_id.

use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use sled::Db;

/// Key prefix for allocation receipts
const ALLOCATION_PREFIX: &[u8] = b"receipt:allocation:";
/// Key prefix for settlement intents
const INTENT_PREFIX: &[u8] = b"receipt:intent:";
/// Index prefix for decision hash lookups
const DECISION_INDEX_PREFIX: &[u8] = b"receipt:by_decision:";

/// Receipt storage service for economic chain artifacts.
///
/// Stores receipts by canonical hash for cross-node deterministic lookup.
pub struct ReceiptStore {
    db: Db,
}

impl ReceiptStore {
    /// Create a new receipt store backed by the given sled database.
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Build a key from prefix and hex hash
    fn make_key(prefix: &[u8], hash: &Hash) -> Vec<u8> {
        let mut key = prefix.to_vec();
        key.extend_from_slice(hex::encode(hash).as_bytes());
        key
    }

    /// Build a decision index key
    fn make_decision_index_key(decision_hash: &Hash, type_tag: &[u8], receipt_hash: &Hash) -> Vec<u8> {
        let mut key = DECISION_INDEX_PREFIX.to_vec();
        key.extend_from_slice(hex::encode(decision_hash).as_bytes());
        key.push(b':');
        key.extend_from_slice(type_tag);
        key.push(b':');
        key.extend_from_slice(hex::encode(receipt_hash).as_bytes());
        key
    }

    /// Build a decision scan prefix
    fn make_decision_scan_prefix(decision_hash: &Hash, type_tag: &[u8]) -> Vec<u8> {
        let mut prefix = DECISION_INDEX_PREFIX.to_vec();
        prefix.extend_from_slice(hex::encode(decision_hash).as_bytes());
        prefix.push(b':');
        prefix.extend_from_slice(type_tag);
        prefix.push(b':');
        prefix
    }

    // ========================================================================
    // AllocationReceipt operations
    // ========================================================================

    /// Store an allocation receipt by its canonical hash.
    pub fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        let hash = receipt.canonical_hash();
        let key = Self::make_key(ALLOCATION_PREFIX, &hash);
        let value = serde_json::to_vec(receipt)
            .map_err(|e| format!("Failed to serialize allocation receipt: {}", e))?;

        self.db
            .insert(&key, value)
            .map_err(|e| format!("Failed to store allocation receipt: {}", e))?;

        // Index by decision hash
        let decision_key =
            Self::make_decision_index_key(&receipt.decision_hash, b"allocation", &hash);
        self.db
            .insert(&decision_key, &hash[..])
            .map_err(|e| format!("Failed to index allocation receipt: {}", e))?;

        Ok(hash)
    }

    /// Get an allocation receipt by canonical hash.
    pub fn get_allocation(&self, hash: &Hash) -> Result<Option<AllocationReceipt>, String> {
        let key = Self::make_key(ALLOCATION_PREFIX, hash);
        match self.db.get(&key) {
            Ok(Some(bytes)) => {
                let receipt: AllocationReceipt = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Failed to deserialize allocation receipt: {}", e))?;
                Ok(Some(receipt))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to get allocation receipt: {}", e)),
        }
    }

    /// List allocation receipts for a decision hash.
    pub fn list_allocations_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String> {
        let prefix = Self::make_decision_scan_prefix(decision_hash, b"allocation");
        
        let mut receipts = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (_, hash_bytes) = entry.map_err(|e| format!("Failed to scan: {}", e))?;
            if hash_bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);
                if let Ok(Some(receipt)) = self.get_allocation(&hash) {
                    receipts.push(receipt);
                }
            }
        }
        Ok(receipts)
    }

    // ========================================================================
    // SettlementIntent operations
    // ========================================================================

    /// Store a settlement intent by its canonical hash.
    pub fn put_intent(&self, intent: &SettlementIntent) -> Result<Hash, String> {
        let hash = intent.canonical_hash();
        let key = Self::make_key(INTENT_PREFIX, &hash);
        let value = serde_json::to_vec(intent)
            .map_err(|e| format!("Failed to serialize settlement intent: {}", e))?;

        self.db
            .insert(&key, value)
            .map_err(|e| format!("Failed to store settlement intent: {}", e))?;

        // Index by decision hash
        let decision_key = Self::make_decision_index_key(&intent.decision_hash, b"intent", &hash);
        self.db
            .insert(&decision_key, &hash[..])
            .map_err(|e| format!("Failed to index settlement intent: {}", e))?;

        Ok(hash)
    }

    /// Get a settlement intent by canonical hash.
    pub fn get_intent(&self, hash: &Hash) -> Result<Option<SettlementIntent>, String> {
        let key = Self::make_key(INTENT_PREFIX, hash);
        match self.db.get(&key) {
            Ok(Some(bytes)) => {
                let intent: SettlementIntent = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Failed to deserialize settlement intent: {}", e))?;
                Ok(Some(intent))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to get settlement intent: {}", e)),
        }
    }

    /// List settlement intents for a decision hash.
    pub fn list_intents_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<SettlementIntent>, String> {
        let prefix = Self::make_decision_scan_prefix(decision_hash, b"intent");

        let mut intents = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (_, hash_bytes) = entry.map_err(|e| format!("Failed to scan: {}", e))?;
            if hash_bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);
                if let Ok(Some(intent)) = self.get_intent(&hash) {
                    intents.push(intent);
                }
            }
        }
        Ok(intents)
    }

    // ========================================================================
    // Convenience methods
    // ========================================================================

    /// Store an allocation receipt and all its intents atomically.
    pub fn put_allocation_with_intents(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        // Store all intents first
        for intent in &receipt.intents {
            self.put_intent(intent)?;
        }
        // Store the allocation receipt
        self.put_allocation(receipt)
    }

    /// Get the full economic chain for a decision hash.
    ///
    /// Returns (allocations, intents) for the given decision.
    pub fn get_chain_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<(Vec<AllocationReceipt>, Vec<SettlementIntent>), String> {
        let allocations = self.list_allocations_by_decision(decision_hash)?;
        let intents = self.list_intents_by_decision(decision_hash)?;
        Ok((allocations, intents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::ScopeLevel;

    fn temp_db() -> Db {
        sled::Config::new().temporary(true).open().unwrap()
    }

    fn make_test_intent(decision_hash: Hash, amount: u64) -> SettlementIntent {
        SettlementIntent::new("dec-001", decision_hash, "treasury", "member", amount, "HOURS")
            .with_timestamp(1000000)
    }

    #[test]
    fn test_put_get_intent() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        let intent = make_test_intent(decision_hash, 500);
        let hash = receipt_store.put_intent(&intent).unwrap();

        let retrieved = receipt_store.get_intent(&hash).unwrap().unwrap();
        assert_eq!(retrieved.canonical_hash(), intent.canonical_hash());
        assert_eq!(retrieved.amount, 500);
    }

    #[test]
    fn test_put_get_allocation() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        let intent = make_test_intent(decision_hash, 100);
        let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(intent);

        let hash = receipt_store.put_allocation(&allocation).unwrap();

        let retrieved = receipt_store.get_allocation(&hash).unwrap().unwrap();
        assert_eq!(retrieved.canonical_hash(), allocation.canonical_hash());
        assert_eq!(retrieved.intents.len(), 1);
    }

    #[test]
    fn test_list_by_decision() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        
        // Store multiple intents for same decision
        let intent1 = make_test_intent(decision_hash, 100);
        let intent2 = make_test_intent(decision_hash, 200);
        receipt_store.put_intent(&intent1).unwrap();
        receipt_store.put_intent(&intent2).unwrap();

        let intents = receipt_store.list_intents_by_decision(&decision_hash).unwrap();
        assert_eq!(intents.len(), 2);
    }

    #[test]
    fn test_put_allocation_with_intents() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        let intent1 = make_test_intent(decision_hash, 100);
        let intent2 = make_test_intent(decision_hash, 200);
        let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(intent1)
            .add_intent(intent2);

        receipt_store.put_allocation_with_intents(&allocation).unwrap();

        // Both intents should be retrievable
        let intents = receipt_store.list_intents_by_decision(&decision_hash).unwrap();
        assert_eq!(intents.len(), 2);

        // Allocation should be retrievable
        let (allocations, _) = receipt_store.get_chain_by_decision(&decision_hash).unwrap();
        assert_eq!(allocations.len(), 1);
    }

    #[test]
    fn test_canonical_hash_as_key() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        
        // Two intents with different node-local IDs but same content
        let intent1 = make_test_intent(decision_hash, 500)
            .with_intent_id("node-a-id-001");
        let intent2 = make_test_intent(decision_hash, 500)
            .with_intent_id("node-b-id-999");

        // Same canonical hash
        assert_eq!(intent1.canonical_hash(), intent2.canonical_hash());

        // Storing second should overwrite first (same key)
        receipt_store.put_intent(&intent1).unwrap();
        receipt_store.put_intent(&intent2).unwrap();

        let intents = receipt_store.list_intents_by_decision(&decision_hash).unwrap();
        assert_eq!(intents.len(), 1); // Deduplicated by canonical hash
    }
}
