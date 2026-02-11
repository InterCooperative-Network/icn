#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Receipt Persistence E2E Test
//!
//! Verifies that AllocationReceipt and SettlementIntent survive node restart
//! and that canonical hashes remain identical after reload.
//!
//! This is the Sprint 9 durability proof for economic receipts.

use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use icn_kernel_api::ScopeLevel;
use std::path::PathBuf;
use tempfile::TempDir;

// =============================================================================
// Test: Receipt Store Persistence
// =============================================================================

/// Test that receipts persist across "restart" (db close/reopen).
///
/// This simulates:
/// 1. Create receipts and store them
/// 2. Get canonical hashes
/// 3. "Restart" by dropping store and reopening db
/// 4. Retrieve receipts and verify hashes match
#[test]
fn test_receipt_store_persistence_across_restart() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   RECEIPT PERSISTENCE E2E TEST (Sprint 9)                        ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    // Create temp directory that persists across the test
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path: PathBuf = temp_dir.path().join("receipt_store.db");

    let decision_hash: Hash = [42u8; 32];
    let original_intent_hash: Hash;
    let original_allocation_hash: Hash;

    // === Phase 1: Create and store receipts ===
    println!("║ Phase 1: Creating and storing receipts...                        ║");
    {
        let db = sled::open(&db_path).expect("Failed to open db");
        let store = ReceiptStore::new(db);

        // Create a settlement intent
        let intent = SettlementIntent::new(
            "decision-001",
            decision_hash,
            "treasury",
            "member-alice",
            1000,
            "HOURS",
        )
        .with_timestamp(1704067200);

        // Create allocation receipt with intent
        let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1704067200)
            .add_intent(intent.clone());

        // Store them
        let intent_hash = store.put_intent(&intent).expect("Failed to store intent");
        let alloc_hash = store
            .put_allocation(&allocation)
            .expect("Failed to store allocation");

        original_intent_hash = intent_hash;
        original_allocation_hash = alloc_hash;

        println!(
            "║   Intent hash:     {}... ║",
            &hex::encode(intent_hash)[..24]
        );
        println!(
            "║   Allocation hash: {}... ║",
            &hex::encode(alloc_hash)[..24]
        );

        // Force db to flush and drop
        drop(store);
    }

    println!("║ Phase 2: Simulating node restart...                              ║");

    // === Phase 2: Reopen and verify ===
    {
        let db = sled::open(&db_path).expect("Failed to reopen db");
        let store = ReceiptStore::new(db);

        // Retrieve and verify intent
        let retrieved_intent = store
            .get_intent(&original_intent_hash)
            .expect("Failed to get intent")
            .expect("Intent not found after restart");

        let retrieved_intent_hash = retrieved_intent.canonical_hash();

        // Retrieve and verify allocation
        let retrieved_allocation = store
            .get_allocation(&original_allocation_hash)
            .expect("Failed to get allocation")
            .expect("Allocation not found after restart");

        let retrieved_allocation_hash = retrieved_allocation.canonical_hash();

        println!("║ Phase 3: Verifying canonical hashes after restart...            ║");
        println!(
            "║   Intent hash:     {}... ║",
            &hex::encode(retrieved_intent_hash)[..24]
        );
        println!(
            "║   Allocation hash: {}... ║",
            &hex::encode(retrieved_allocation_hash)[..24]
        );

        // Verify hashes match
        assert_eq!(
            retrieved_intent_hash, original_intent_hash,
            "Intent canonical hash changed after restart!"
        );
        assert_eq!(
            retrieved_allocation_hash, original_allocation_hash,
            "Allocation canonical hash changed after restart!"
        );

        // Verify we can query by decision hash
        let (allocations, intents) = store
            .get_chain_by_decision(&decision_hash)
            .expect("Failed to get chain");

        assert_eq!(allocations.len(), 1, "Expected 1 allocation");
        assert_eq!(intents.len(), 1, "Expected 1 intent");

        println!("║                                                                  ║");
        println!("║   ✅ Intent hash MATCHES after restart                          ║");
        println!("║   ✅ Allocation hash MATCHES after restart                      ║");
        println!("║   ✅ Chain query returns correct counts                         ║");
    }

    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║   RECEIPT PERSISTENCE VERIFIED                                   ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}

/// Test that multiple receipts persist correctly.
#[test]
fn test_multiple_receipts_persist_across_restart() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path: PathBuf = temp_dir.path().join("multi_receipt.db");

    let decision_hash_1: Hash = [1u8; 32];
    let decision_hash_2: Hash = [2u8; 32];
    let mut stored_hashes: Vec<(Hash, Hash)> = Vec::new();

    // Store multiple receipts
    {
        let db = sled::open(&db_path).expect("Failed to open db");
        let store = ReceiptStore::new(db);

        for (i, decision_hash) in [decision_hash_1, decision_hash_2].iter().enumerate() {
            let intent = SettlementIntent::new(
                &format!("decision-{:03}", i),
                *decision_hash,
                "treasury",
                &format!("member-{}", i),
                (i + 1) as u64 * 500,
                "HOURS",
            )
            .with_timestamp(1704067200 + (i as u64 * 3600));

            let allocation = AllocationReceipt::new(*decision_hash, ScopeLevel::Org)
                .with_timestamp(1704067200 + (i as u64 * 3600))
                .add_intent(intent.clone());

            let intent_hash = store.put_intent(&intent).expect("Failed to store intent");
            let alloc_hash = store
                .put_allocation(&allocation)
                .expect("Failed to store allocation");

            stored_hashes.push((intent_hash, alloc_hash));
        }
    }

    // Reopen and verify all receipts
    {
        let db = sled::open(&db_path).expect("Failed to reopen db");
        let store = ReceiptStore::new(db);

        for (i, (intent_hash, alloc_hash)) in stored_hashes.iter().enumerate() {
            let retrieved_intent = store
                .get_intent(intent_hash)
                .expect("Failed to get intent")
                .expect(&format!("Intent {} not found after restart", i));

            let retrieved_allocation = store
                .get_allocation(alloc_hash)
                .expect("Failed to get allocation")
                .expect(&format!("Allocation {} not found after restart", i));

            assert_eq!(
                &retrieved_intent.canonical_hash(),
                intent_hash,
                "Intent {} hash mismatch",
                i
            );
            assert_eq!(
                &retrieved_allocation.canonical_hash(),
                alloc_hash,
                "Allocation {} hash mismatch",
                i
            );
        }

        // Verify chain queries work for both decision hashes
        let (allocs1, _) = store
            .get_chain_by_decision(&decision_hash_1)
            .expect("Failed to get chain 1");
        let (allocs2, _) = store
            .get_chain_by_decision(&decision_hash_2)
            .expect("Failed to get chain 2");

        assert_eq!(allocs1.len(), 1);
        assert_eq!(allocs2.len(), 1);
    }
}

// =============================================================================
// ReceiptStore Implementation (copied from gateway for E2E testing)
// =============================================================================

use sled::Db;

const ALLOCATION_PREFIX: &[u8] = b"receipt:allocation:";
const INTENT_PREFIX: &[u8] = b"receipt:intent:";
const DECISION_INDEX_PREFIX: &[u8] = b"receipt:by_decision:";

struct ReceiptStore {
    db: Db,
}

impl ReceiptStore {
    fn new(db: Db) -> Self {
        Self { db }
    }

    fn make_key(prefix: &[u8], hash: &Hash) -> Vec<u8> {
        let mut key = prefix.to_vec();
        key.extend_from_slice(hex::encode(hash).as_bytes());
        key
    }

    fn make_decision_index_key(decision_hash: &Hash, type_tag: &[u8], receipt_hash: &Hash) -> Vec<u8> {
        let mut key = DECISION_INDEX_PREFIX.to_vec();
        key.extend_from_slice(hex::encode(decision_hash).as_bytes());
        key.push(b':');
        key.extend_from_slice(type_tag);
        key.push(b':');
        key.extend_from_slice(hex::encode(receipt_hash).as_bytes());
        key
    }

    fn make_decision_scan_prefix(decision_hash: &Hash, type_tag: &[u8]) -> Vec<u8> {
        let mut prefix = DECISION_INDEX_PREFIX.to_vec();
        prefix.extend_from_slice(hex::encode(decision_hash).as_bytes());
        prefix.push(b':');
        prefix.extend_from_slice(type_tag);
        prefix.push(b':');
        prefix
    }

    fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        let hash = receipt.canonical_hash();
        let key = Self::make_key(ALLOCATION_PREFIX, &hash);
        let value = serde_json::to_vec(receipt)
            .map_err(|e| format!("Failed to serialize allocation receipt: {}", e))?;

        self.db
            .insert(&key, value)
            .map_err(|e| format!("Failed to store allocation receipt: {}", e))?;

        let decision_key =
            Self::make_decision_index_key(&receipt.decision_hash, b"allocation", &hash);
        self.db
            .insert(&decision_key, &hash[..])
            .map_err(|e| format!("Failed to index allocation receipt: {}", e))?;

        Ok(hash)
    }

    fn get_allocation(&self, hash: &Hash) -> Result<Option<AllocationReceipt>, String> {
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

    fn list_allocations_by_decision(
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

    fn put_intent(&self, intent: &SettlementIntent) -> Result<Hash, String> {
        let hash = intent.canonical_hash();
        let key = Self::make_key(INTENT_PREFIX, &hash);
        let value = serde_json::to_vec(intent)
            .map_err(|e| format!("Failed to serialize settlement intent: {}", e))?;

        self.db
            .insert(&key, value)
            .map_err(|e| format!("Failed to store settlement intent: {}", e))?;

        let decision_key = Self::make_decision_index_key(&intent.decision_hash, b"intent", &hash);
        self.db
            .insert(&decision_key, &hash[..])
            .map_err(|e| format!("Failed to index settlement intent: {}", e))?;

        Ok(hash)
    }

    fn get_intent(&self, hash: &Hash) -> Result<Option<SettlementIntent>, String> {
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

    fn list_intents_by_decision(
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

    fn get_chain_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<(Vec<AllocationReceipt>, Vec<SettlementIntent>), String> {
        let allocations = self.list_allocations_by_decision(decision_hash)?;
        let intents = self.list_intents_by_decision(decision_hash)?;
        Ok((allocations, intents))
    }
}
