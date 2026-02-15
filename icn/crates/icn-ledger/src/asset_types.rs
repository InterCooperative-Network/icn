//! Asset type system for cooperative resource tracking.
//!
//! This module provides typed asset tracking for cooperatives, supporting
//! durable goods, consumables, perishables, and services. Assets are stored
//! in the ledger's KV store with custody transfer and condition lifecycle.
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_ledger::asset_types::{AssetRegistry, AssetMetadata, AssetClass, AssetCondition};
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! let store = Arc::new(SledStore::temporary()?);
//! let registry = AssetRegistry::new(store);
//!
//! let metadata = AssetMetadata {
//!     id: "drill-press-001".to_string(),
//!     name: "Drill Press".to_string(),
//!     class: AssetClass::Durable,
//!     description: "Heavy-duty drill press for woodworking".to_string(),
//!     owner_did: alice_did.clone(),
//!     created_at: 1700000000,
//!     condition: AssetCondition::New,
//! };
//!
//! let id = registry.register_asset(metadata)?;
//! let asset = registry.get_asset(&id)?.expect("asset exists");
//! ```

use crate::error::{LedgerError, Result};
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Key prefix for asset records in the KV store.
const ASSET_PREFIX: &str = "asset:";

/// Key prefix for the owner-to-asset index.
const OWNER_INDEX_PREFIX: &str = "asset_owner:";

/// Key prefix for custody transfer records.
const TRANSFER_PREFIX: &str = "asset_transfer:";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Unique identifier for an asset.
pub type AssetId = String;

/// Classification of an asset by its lifecycle characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetClass {
    /// Long-lived physical goods (tools, equipment, vehicles).
    Durable,
    /// Goods consumed on use (office supplies, raw materials).
    Consumable,
    /// Goods with a limited shelf life (food, medicine).
    Perishable,
    /// Non-physical services or time-based access.
    Service,
}

/// Current physical condition of an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetCondition {
    /// Brand new, unused.
    New,
    /// Used but in good working order.
    Good,
    /// Functional but showing wear.
    Fair,
    /// Requires maintenance before safe use.
    NeedsRepair,
    /// End of life, removed from active inventory.
    Decommissioned,
}

/// Metadata describing a tracked asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMetadata {
    /// Unique identifier (caller-assigned or generated).
    pub id: AssetId,
    /// Human-readable name.
    pub name: String,
    /// Asset classification.
    pub class: AssetClass,
    /// Free-text description.
    pub description: String,
    /// DID of the current custodian/owner.
    pub owner_did: Did,
    /// Unix timestamp of creation (seconds).
    pub created_at: u64,
    /// Current physical condition.
    pub condition: AssetCondition,
}

/// Record of a single custody transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustodyTransfer {
    /// Asset that was transferred.
    pub asset_id: AssetId,
    /// Previous custodian.
    pub from_did: Did,
    /// New custodian.
    pub to_did: Did,
    /// Unix timestamp of transfer (seconds).
    pub transferred_at: u64,
}

// ---------------------------------------------------------------------------
// AssetRegistry
// ---------------------------------------------------------------------------

/// Registry for tracking cooperative assets backed by a KV store.
///
/// All data is persisted under well-known key prefixes so it can coexist
/// with other ledger data in the same store.
pub struct AssetRegistry {
    store: Arc<dyn Store>,
}

impl AssetRegistry {
    /// Create a new registry backed by the given store.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    // -- Registration -------------------------------------------------------

    /// Register a new asset. Returns the asset ID.
    ///
    /// If `metadata.id` is empty, a simple sequential ID is assigned.
    /// Returns `LedgerError::DuplicateEntry` if the ID already exists.
    pub fn register_asset(&self, mut metadata: AssetMetadata) -> Result<AssetId> {
        if metadata.id.is_empty() {
            metadata.id = self.next_id()?;
        }

        let key = asset_key(&metadata.id);
        if self.store.get(key.as_bytes()).map_err(store_err)?.is_some() {
            return Err(LedgerError::DuplicateEntry(format!(
                "asset already exists: {}",
                metadata.id
            )));
        }

        let value = serde_json::to_vec(&metadata)?;
        self.store.put(key.as_bytes(), &value).map_err(store_err)?;

        // Maintain owner index
        self.add_owner_index(&metadata.owner_did, &metadata.id)?;

        Ok(metadata.id)
    }

    // -- Queries ------------------------------------------------------------

    /// Retrieve asset metadata by ID, or `None` if not found.
    pub fn get_asset(&self, id: &str) -> Result<Option<AssetMetadata>> {
        let key = asset_key(id);
        match self.store.get(key.as_bytes()).map_err(store_err)? {
            Some(bytes) => {
                let meta: AssetMetadata = serde_json::from_slice(&bytes)?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// List all assets currently owned by a given DID.
    pub fn list_assets_by_owner(&self, owner_did: &Did) -> Result<Vec<AssetMetadata>> {
        let ids = self.get_owner_asset_ids(owner_did)?;
        let mut assets = Vec::with_capacity(ids.len());
        for id in &ids {
            if let Some(meta) = self.get_asset(id)? {
                // Double-check owner matches (index could be stale after transfer)
                if meta.owner_did == *owner_did {
                    assets.push(meta);
                }
            }
        }
        Ok(assets)
    }

    // -- Mutations ----------------------------------------------------------

    /// Transfer custody of an asset from one DID to another.
    ///
    /// Fails if the asset does not exist or `from_did` is not the current owner.
    pub fn transfer_custody(&self, asset_id: &str, from_did: &Did, to_did: &Did) -> Result<()> {
        let mut meta = self
            .get_asset(asset_id)?
            .ok_or_else(|| LedgerError::EntryNotFound(format!("asset not found: {asset_id}")))?;

        if meta.owner_did != *from_did {
            return Err(LedgerError::InvalidEntry(format!(
                "asset {} is owned by {}, not {}",
                asset_id, meta.owner_did, from_did
            )));
        }

        // Record transfer
        let transfer = CustodyTransfer {
            asset_id: asset_id.to_string(),
            from_did: from_did.clone(),
            to_did: to_did.clone(),
            transferred_at: current_unix_secs(),
        };
        self.persist_transfer(&transfer)?;

        // Update owner
        meta.owner_did = to_did.clone();
        let value = serde_json::to_vec(&meta)?;
        self.store
            .put(asset_key(asset_id).as_bytes(), &value)
            .map_err(store_err)?;

        // Update indices
        self.remove_owner_index(from_did, asset_id)?;
        self.add_owner_index(to_did, asset_id)?;

        Ok(())
    }

    /// Update the physical condition of an asset.
    ///
    /// Fails if the asset does not exist.
    pub fn update_condition(&self, asset_id: &str, new_condition: AssetCondition) -> Result<()> {
        let mut meta = self
            .get_asset(asset_id)?
            .ok_or_else(|| LedgerError::EntryNotFound(format!("asset not found: {asset_id}")))?;

        meta.condition = new_condition;
        let value = serde_json::to_vec(&meta)?;
        self.store
            .put(asset_key(asset_id).as_bytes(), &value)
            .map_err(store_err)?;

        Ok(())
    }

    // -- Internal helpers ---------------------------------------------------

    /// Generate a simple sequential ID based on current asset count.
    fn next_id(&self) -> Result<AssetId> {
        let entries = self
            .store
            .scan(ASSET_PREFIX.as_bytes())
            .map_err(store_err)?;
        Ok(format!("asset-{:04}", entries.len() + 1))
    }

    /// Persist a custody transfer record.
    fn persist_transfer(&self, transfer: &CustodyTransfer) -> Result<()> {
        let key = format!(
            "{}{}:{}",
            TRANSFER_PREFIX, transfer.asset_id, transfer.transferred_at
        );
        let value = serde_json::to_vec(transfer)?;
        self.store.put(key.as_bytes(), &value).map_err(store_err)?;
        Ok(())
    }

    /// Read asset IDs from the owner index.
    fn get_owner_asset_ids(&self, owner_did: &Did) -> Result<Vec<AssetId>> {
        let prefix = owner_index_key(owner_did);
        let entries = self.store.scan(prefix.as_bytes()).map_err(store_err)?;

        let mut ids = Vec::with_capacity(entries.len());
        for (key_bytes, _) in entries {
            let key_str = String::from_utf8_lossy(&key_bytes);
            // Key format: "asset_owner:<did>:<asset_id>"
            if let Some(asset_id) = key_str.strip_prefix(&prefix) {
                ids.push(asset_id.to_string());
            }
        }
        Ok(ids)
    }

    /// Add an entry to the owner index.
    fn add_owner_index(&self, owner_did: &Did, asset_id: &str) -> Result<()> {
        let key = format!("{}{}", owner_index_key(owner_did), asset_id);
        self.store.put(key.as_bytes(), b"1").map_err(store_err)?;
        Ok(())
    }

    /// Remove an entry from the owner index.
    fn remove_owner_index(&self, owner_did: &Did, asset_id: &str) -> Result<()> {
        let key = format!("{}{}", owner_index_key(owner_did), asset_id);
        self.store.delete(key.as_bytes()).map_err(store_err)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Key helpers
// ---------------------------------------------------------------------------

fn asset_key(id: &str) -> String {
    format!("{}{}", ASSET_PREFIX, id)
}

fn owner_index_key(did: &Did) -> String {
    format!("{}{}:", OWNER_INDEX_PREFIX, did)
}

fn store_err(e: anyhow::Error) -> LedgerError {
    LedgerError::Storage(e.to_string())
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn test_registry() -> (AssetRegistry, Did, Did) {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        (AssetRegistry::new(store), alice, bob)
    }

    fn sample_asset(owner: &Did) -> AssetMetadata {
        AssetMetadata {
            id: "drill-press-001".to_string(),
            name: "Drill Press".to_string(),
            class: AssetClass::Durable,
            description: "Heavy-duty drill press".to_string(),
            owner_did: owner.clone(),
            created_at: 1_700_000_000,
            condition: AssetCondition::New,
        }
    }

    #[test]
    fn test_register_and_get_roundtrip() {
        let (registry, alice, _bob) = test_registry();
        let meta = sample_asset(&alice);

        let id = registry.register_asset(meta.clone()).unwrap();
        assert_eq!(id, "drill-press-001");

        let fetched = registry.get_asset(&id).unwrap().expect("should exist");
        assert_eq!(fetched.name, "Drill Press");
        assert_eq!(fetched.class, AssetClass::Durable);
        assert_eq!(fetched.owner_did, alice);
        assert_eq!(fetched.condition, AssetCondition::New);
    }

    #[test]
    fn test_duplicate_registration_fails() {
        let (registry, alice, _bob) = test_registry();
        let meta = sample_asset(&alice);

        registry.register_asset(meta.clone()).unwrap();
        let err = registry.register_asset(meta).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "expected duplicate error, got: {err}"
        );
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let (registry, _alice, _bob) = test_registry();
        let result = registry.get_asset("no-such-asset").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_custody_transfer_updates_owner() {
        let (registry, alice, bob) = test_registry();
        let meta = sample_asset(&alice);
        let id = registry.register_asset(meta).unwrap();

        registry.transfer_custody(&id, &alice, &bob).unwrap();

        let fetched = registry.get_asset(&id).unwrap().expect("should exist");
        assert_eq!(fetched.owner_did, bob);
    }

    #[test]
    fn test_transfer_from_wrong_owner_fails() {
        let (registry, alice, bob) = test_registry();
        let meta = sample_asset(&alice);
        let id = registry.register_asset(meta).unwrap();

        let err = registry.transfer_custody(&id, &bob, &alice).unwrap_err();
        assert!(
            err.to_string().contains("not"),
            "expected wrong-owner error, got: {err}"
        );
    }

    #[test]
    fn test_transfer_nonexistent_asset_fails() {
        let (registry, alice, bob) = test_registry();
        let err = registry
            .transfer_custody("no-such-asset", &alice, &bob)
            .unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected not-found error, got: {err}"
        );
    }

    #[test]
    fn test_condition_lifecycle() {
        let (registry, alice, _bob) = test_registry();
        let meta = sample_asset(&alice);
        let id = registry.register_asset(meta).unwrap();

        // New -> Good
        registry
            .update_condition(&id, AssetCondition::Good)
            .unwrap();
        let a = registry.get_asset(&id).unwrap().unwrap();
        assert_eq!(a.condition, AssetCondition::Good);

        // Good -> NeedsRepair
        registry
            .update_condition(&id, AssetCondition::NeedsRepair)
            .unwrap();
        let a = registry.get_asset(&id).unwrap().unwrap();
        assert_eq!(a.condition, AssetCondition::NeedsRepair);

        // NeedsRepair -> Decommissioned
        registry
            .update_condition(&id, AssetCondition::Decommissioned)
            .unwrap();
        let a = registry.get_asset(&id).unwrap().unwrap();
        assert_eq!(a.condition, AssetCondition::Decommissioned);
    }

    #[test]
    fn test_update_condition_nonexistent_fails() {
        let (registry, _alice, _bob) = test_registry();
        let err = registry
            .update_condition("nope", AssetCondition::Good)
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_list_assets_by_owner() {
        let (registry, alice, bob) = test_registry();

        // Register two assets for Alice, one for Bob
        let a1 = AssetMetadata {
            id: "tool-001".to_string(),
            name: "Saw".to_string(),
            class: AssetClass::Durable,
            description: "Circular saw".to_string(),
            owner_did: alice.clone(),
            created_at: 1_700_000_000,
            condition: AssetCondition::New,
        };
        let a2 = AssetMetadata {
            id: "tool-002".to_string(),
            name: "Hammer".to_string(),
            class: AssetClass::Durable,
            description: "Claw hammer".to_string(),
            owner_did: alice.clone(),
            created_at: 1_700_000_001,
            condition: AssetCondition::Good,
        };
        let b1 = AssetMetadata {
            id: "tool-003".to_string(),
            name: "Wrench".to_string(),
            class: AssetClass::Durable,
            description: "Adjustable wrench".to_string(),
            owner_did: bob.clone(),
            created_at: 1_700_000_002,
            condition: AssetCondition::New,
        };

        registry.register_asset(a1).unwrap();
        registry.register_asset(a2).unwrap();
        registry.register_asset(b1).unwrap();

        let alice_assets = registry.list_assets_by_owner(&alice).unwrap();
        assert_eq!(alice_assets.len(), 2);
        assert!(alice_assets.iter().all(|a| a.owner_did == alice));

        let bob_assets = registry.list_assets_by_owner(&bob).unwrap();
        assert_eq!(bob_assets.len(), 1);
        assert_eq!(bob_assets[0].name, "Wrench");
    }

    #[test]
    fn test_list_updates_after_transfer() {
        let (registry, alice, bob) = test_registry();
        let meta = sample_asset(&alice);
        let id = registry.register_asset(meta).unwrap();

        assert_eq!(registry.list_assets_by_owner(&alice).unwrap().len(), 1);
        assert_eq!(registry.list_assets_by_owner(&bob).unwrap().len(), 0);

        registry.transfer_custody(&id, &alice, &bob).unwrap();

        assert_eq!(registry.list_assets_by_owner(&alice).unwrap().len(), 0);
        assert_eq!(registry.list_assets_by_owner(&bob).unwrap().len(), 1);
    }

    #[test]
    fn test_auto_id_generation() {
        let (registry, alice, _bob) = test_registry();
        let mut meta = sample_asset(&alice);
        meta.id = String::new(); // Empty ID triggers auto-generation

        let id = registry.register_asset(meta).unwrap();
        assert!(
            id.starts_with("asset-"),
            "auto ID should start with 'asset-': {id}"
        );

        let fetched = registry.get_asset(&id).unwrap();
        assert!(fetched.is_some());
    }

    #[test]
    fn test_serde_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let meta = sample_asset(&alice);

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: AssetMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, meta.id);
        assert_eq!(deserialized.name, meta.name);
        assert_eq!(deserialized.class, meta.class);
        assert_eq!(deserialized.condition, meta.condition);
        assert_eq!(deserialized.owner_did, meta.owner_did);
    }
}
