//! ICN Store - Persistent key-value storage abstraction

pub mod quotas;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

// Re-export quota types
pub use quotas::{QuotaPriority, QuotaStats, StorageItem, StorageQuota, StorageQuotaManager};

/// Content hash type (32-byte SHA-256)
pub type ContentHash = [u8; 32];

/// Replica information for a single peer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplicaInfo {
    /// DID of the peer holding this replica
    pub peer_did: String,
    /// When this replica was last seen/verified
    pub last_seen: SystemTime,
    /// Health status of this replica
    pub health: ReplicaHealth,
}

/// Health status of a replica
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplicaHealth {
    /// Replica verified recently (healthy)
    Healthy,
    /// Replica not verified recently (stale)
    Stale,
    /// Peer reported as offline/unreachable
    Unreachable,
}

/// Metadata about all replicas for a given content hash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaMetadata {
    /// Content hash this metadata describes
    pub content_hash: ContentHash,
    /// List of all known replicas
    pub replicas: Vec<ReplicaInfo>,
    /// When this metadata was last updated
    pub updated_at: SystemTime,
}

impl ReplicaMetadata {
    /// Create new replica metadata for a content hash
    pub fn new(content_hash: ContentHash) -> Self {
        Self {
            content_hash,
            replicas: Vec::new(),
            updated_at: SystemTime::now(),
        }
    }

    /// Add or update a replica entry
    pub fn add_replica(&mut self, peer_did: String, health: ReplicaHealth) {
        // Update existing replica if found
        for replica in &mut self.replicas {
            if replica.peer_did == peer_did {
                replica.last_seen = SystemTime::now();
                replica.health = health;
                self.updated_at = SystemTime::now();
                return;
            }
        }

        // Add new replica
        self.replicas.push(ReplicaInfo {
            peer_did,
            last_seen: SystemTime::now(),
            health,
        });
        self.updated_at = SystemTime::now();
    }

    /// Remove a replica entry
    pub fn remove_replica(&mut self, peer_did: &str) -> bool {
        let before_len = self.replicas.len();
        self.replicas.retain(|r| r.peer_did != peer_did);
        if self.replicas.len() < before_len {
            self.updated_at = SystemTime::now();
            true
        } else {
            false
        }
    }

    /// Get count of healthy replicas
    pub fn healthy_count(&self) -> usize {
        self.replicas
            .iter()
            .filter(|r| r.health == ReplicaHealth::Healthy)
            .count()
    }

    /// Get all healthy replica DIDs
    pub fn healthy_replicas(&self) -> Vec<String> {
        self.replicas
            .iter()
            .filter(|r| r.health == ReplicaHealth::Healthy)
            .map(|r| r.peer_did.clone())
            .collect()
    }
}

/// Storage trait for pluggable backends
pub trait Store: Send + Sync {
    // Core KV operations
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    // Replica tracking operations (Phase 17)
    /// Get replica metadata for a content hash
    fn get_replica_metadata(&self, content_hash: &ContentHash) -> Result<Option<ReplicaMetadata>>;

    /// Store replica metadata for a content hash
    fn put_replica_metadata(&self, metadata: &ReplicaMetadata) -> Result<()>;

    /// List all content hashes with replica metadata
    fn list_replica_hashes(&self) -> Result<Vec<ContentHash>>;

    /// Get count of replicas for a content hash
    fn get_replica_count(&self, content_hash: &ContentHash) -> Result<usize> {
        Ok(self
            .get_replica_metadata(content_hash)?
            .map(|m| m.replicas.len())
            .unwrap_or(0))
    }

    /// Add or update a replica for a content hash
    fn add_replica(
        &self,
        content_hash: &ContentHash,
        peer_did: String,
        health: ReplicaHealth,
    ) -> Result<()> {
        let mut metadata = self
            .get_replica_metadata(content_hash)?
            .unwrap_or_else(|| ReplicaMetadata::new(*content_hash));

        metadata.add_replica(peer_did, health);
        self.put_replica_metadata(&metadata)
    }

    /// Remove a replica for a content hash
    fn remove_replica(&self, content_hash: &ContentHash, peer_did: &str) -> Result<bool> {
        if let Some(mut metadata) = self.get_replica_metadata(content_hash)? {
            let removed = metadata.remove_replica(peer_did);
            if removed {
                self.put_replica_metadata(&metadata)?;
            }
            Ok(removed)
        } else {
            Ok(false)
        }
    }
}

/// Sled-based storage implementation
pub struct SledStore {
    db: sled::Db,
}

impl SledStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(SledStore { db })
    }

    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(SledStore { db })
    }
}

impl Store for SledStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get(key)?.map(|v| v.to_vec()))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.insert(key, value)?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.remove(key)?;
        Ok(())
    }

    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (k, v) = item?;
            results.push((k.to_vec(), v.to_vec()));
        }

        Ok(results)
    }

    // Replica tracking implementation
    fn get_replica_metadata(&self, content_hash: &ContentHash) -> Result<Option<ReplicaMetadata>> {
        let key = Self::replica_key(content_hash);
        if let Some(value) = self.db.get(&key)? {
            let metadata: ReplicaMetadata =
                serde_json::from_slice(&value).context("Failed to deserialize replica metadata")?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }

    fn put_replica_metadata(&self, metadata: &ReplicaMetadata) -> Result<()> {
        let key = Self::replica_key(&metadata.content_hash);
        let value = serde_json::to_vec(metadata).context("Failed to serialize replica metadata")?;
        self.db.insert(&key, value)?;
        Ok(())
    }

    fn list_replica_hashes(&self) -> Result<Vec<ContentHash>> {
        let prefix = b"replica:";
        let mut hashes = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (key, _) = item?;
            // Key format: "replica:" + 32-byte hash
            if key.len() == prefix.len() + 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&key[prefix.len()..]);
                hashes.push(hash);
            }
        }

        Ok(hashes)
    }
}

impl SledStore {
    /// Generate storage key for replica metadata
    fn replica_key(content_hash: &ContentHash) -> Vec<u8> {
        let mut key = Vec::with_capacity(8 + 32);
        key.extend_from_slice(b"replica:");
        key.extend_from_slice(content_hash);
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hash() -> ContentHash {
        let mut hash = [0u8; 32];
        hash[0] = 0xAA;
        hash[31] = 0xBB;
        hash
    }

    fn test_hash2() -> ContentHash {
        let mut hash = [0u8; 32];
        hash[0] = 0xCC;
        hash[31] = 0xDD;
        hash
    }

    #[test]
    fn test_replica_metadata_new() {
        let hash = test_hash();
        let metadata = ReplicaMetadata::new(hash);

        assert_eq!(metadata.content_hash, hash);
        assert_eq!(metadata.replicas.len(), 0);
    }

    #[test]
    fn test_replica_metadata_add_replica() {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);

        // Add first replica
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        assert_eq!(metadata.replicas.len(), 1);
        assert_eq!(metadata.replicas[0].peer_did, "did:icn:peer1");
        assert_eq!(metadata.replicas[0].health, ReplicaHealth::Healthy);

        // Add second replica
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Healthy);
        assert_eq!(metadata.replicas.len(), 2);

        // Update first replica (should not add duplicate)
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Stale);
        assert_eq!(metadata.replicas.len(), 2);
        assert_eq!(metadata.replicas[0].health, ReplicaHealth::Stale);
    }

    #[test]
    fn test_replica_metadata_remove_replica() {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);

        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Healthy);
        assert_eq!(metadata.replicas.len(), 2);

        // Remove existing replica
        let removed = metadata.remove_replica("did:icn:peer1");
        assert!(removed);
        assert_eq!(metadata.replicas.len(), 1);
        assert_eq!(metadata.replicas[0].peer_did, "did:icn:peer2");

        // Try to remove non-existent replica
        let removed = metadata.remove_replica("did:icn:peer3");
        assert!(!removed);
        assert_eq!(metadata.replicas.len(), 1);
    }

    #[test]
    fn test_replica_metadata_healthy_count() {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);

        assert_eq!(metadata.healthy_count(), 0);

        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Stale);
        metadata.add_replica("did:icn:peer3".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer4".to_string(), ReplicaHealth::Unreachable);

        assert_eq!(metadata.healthy_count(), 2);
        assert_eq!(metadata.healthy_replicas().len(), 2);
        assert!(metadata
            .healthy_replicas()
            .contains(&"did:icn:peer1".to_string()));
        assert!(metadata
            .healthy_replicas()
            .contains(&"did:icn:peer3".to_string()));
    }

    #[test]
    fn test_store_replica_metadata_roundtrip() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Initially no metadata
        assert!(store.get_replica_metadata(&hash)?.is_none());

        // Create and store metadata
        let mut metadata = ReplicaMetadata::new(hash);
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Stale);
        store.put_replica_metadata(&metadata)?;

        // Retrieve and verify
        let retrieved = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(retrieved.content_hash, hash);
        assert_eq!(retrieved.replicas.len(), 2);
        assert_eq!(retrieved.replicas[0].peer_did, "did:icn:peer1");
        assert_eq!(retrieved.replicas[1].peer_did, "did:icn:peer2");

        Ok(())
    }

    #[test]
    fn test_store_get_replica_count() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Initially 0
        assert_eq!(store.get_replica_count(&hash)?, 0);

        // Add replicas via convenience method
        store.add_replica(&hash, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        assert_eq!(store.get_replica_count(&hash)?, 1);

        store.add_replica(&hash, "did:icn:peer2".to_string(), ReplicaHealth::Healthy)?;
        assert_eq!(store.get_replica_count(&hash)?, 2);

        Ok(())
    }

    #[test]
    fn test_store_add_replica() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Add first replica
        store.add_replica(&hash, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        let metadata = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 1);
        assert_eq!(metadata.replicas[0].peer_did, "did:icn:peer1");

        // Add second replica
        store.add_replica(&hash, "did:icn:peer2".to_string(), ReplicaHealth::Stale)?;
        let metadata = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 2);

        // Update first replica health
        store.add_replica(
            &hash,
            "did:icn:peer1".to_string(),
            ReplicaHealth::Unreachable,
        )?;
        let metadata = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 2); // Still 2, not 3
        assert_eq!(metadata.replicas[0].health, ReplicaHealth::Unreachable);

        Ok(())
    }

    #[test]
    fn test_store_remove_replica() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Setup: add two replicas
        store.add_replica(&hash, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        store.add_replica(&hash, "did:icn:peer2".to_string(), ReplicaHealth::Healthy)?;

        // Remove existing replica
        let removed = store.remove_replica(&hash, "did:icn:peer1")?;
        assert!(removed);
        assert_eq!(store.get_replica_count(&hash)?, 1);

        // Try to remove non-existent replica
        let removed = store.remove_replica(&hash, "did:icn:peer3")?;
        assert!(!removed);
        assert_eq!(store.get_replica_count(&hash)?, 1);

        // Remove from non-existent hash
        let removed = store.remove_replica(&test_hash2(), "did:icn:peer1")?;
        assert!(!removed);

        Ok(())
    }

    #[test]
    fn test_store_list_replica_hashes() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash1 = test_hash();
        let hash2 = test_hash2();

        // Initially empty
        assert_eq!(store.list_replica_hashes()?.len(), 0);

        // Add metadata for two hashes
        store.add_replica(&hash1, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        store.add_replica(&hash2, "did:icn:peer2".to_string(), ReplicaHealth::Healthy)?;

        // Should list both hashes
        let hashes = store.list_replica_hashes()?;
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains(&hash1));
        assert!(hashes.contains(&hash2));

        Ok(())
    }

    #[test]
    fn test_replica_metadata_serialization() -> Result<()> {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Stale);

        // Serialize
        let json = serde_json::to_string(&metadata)?;
        assert!(json.contains("did:icn:peer1"));
        assert!(json.contains("Healthy"));
        assert!(json.contains("Stale"));

        // Deserialize
        let deserialized: ReplicaMetadata = serde_json::from_str(&json)?;
        assert_eq!(deserialized.content_hash, hash);
        assert_eq!(deserialized.replicas.len(), 2);

        Ok(())
    }

    #[test]
    fn test_replica_key_format() {
        let hash = test_hash();
        let key = SledStore::replica_key(&hash);

        // Should be "replica:" + 32 bytes
        assert_eq!(key.len(), 8 + 32);
        assert_eq!(&key[0..8], b"replica:");
        assert_eq!(&key[8..], &hash);
    }
}
