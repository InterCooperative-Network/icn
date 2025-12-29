//! ICN Store - Persistent key-value storage abstraction
#![allow(missing_docs)]
// Prevent panics in production code paths
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// Budget management for ledger accounts
pub mod budgets;
/// Escrow functionality for conditional payments
pub mod escrow;
/// Notification storage and management
pub mod notifications;
/// Storage quota management
pub mod quotas;
/// Recurring payment scheduling
pub mod recurring_payments;

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
    /// Get a value by key
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    /// Store a key-value pair
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    /// Delete a key
    fn delete(&self, key: &[u8]) -> Result<()>;
    /// Scan all key-value pairs with the given prefix
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Count entries with a given prefix without loading values
    ///
    /// More efficient than `scan().len()` as it doesn't deserialize values.
    fn scan_count(&self, prefix: &[u8]) -> Result<usize> {
        // Default implementation uses scan - backends can override for efficiency
        Ok(self.scan(prefix)?.len())
    }

    /// Scan entries with pagination support
    ///
    /// Returns entries starting at `offset` with a maximum of `limit` entries.
    /// More memory-efficient than loading all entries for large datasets.
    ///
    /// # Arguments
    /// * `prefix` - Key prefix to scan
    /// * `offset` - Number of entries to skip
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count) where total_count is the total matching entries
    #[allow(clippy::type_complexity)]
    fn scan_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // Default implementation - backends can override for efficiency
        let all = self.scan(prefix)?;
        let total = all.len();
        let paginated = all.into_iter().skip(offset).take(limit).collect();
        Ok((paginated, total))
    }

    /// Scan entries in reverse key order with pagination support
    ///
    /// Returns entries in reverse key order (most recent first for timestamp-keyed data),
    /// starting at `offset` with a maximum of `limit` entries.
    /// This is efficient for audit trails where keys include timestamps.
    ///
    /// # Arguments
    /// * `prefix` - Key prefix to scan
    /// * `offset` - Number of entries to skip
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count) where entries are in reverse key order
    #[allow(clippy::type_complexity)]
    fn scan_reverse_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // Default implementation - backends can override for efficiency
        // For Sled, this can use .rev() on the iterator for true efficiency
        let all = self.scan(prefix)?;
        let total = all.len();
        let paginated = all.into_iter().rev().skip(offset).take(limit).collect();
        Ok((paginated, total))
    }

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
    /// Open a Sled database at the given path
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(SledStore { db })
    }

    /// Create a temporary in-memory Sled database
    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(SledStore { db })
    }

    /// Get direct access to underlying Sled database
    ///
    /// This is useful for components that need raw Sled access
    /// rather than the Store trait abstraction.
    pub fn db(&self) -> &sled::Db {
        &self.db
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

    fn scan_count(&self, prefix: &[u8]) -> Result<usize> {
        // Efficient count using iterator - doesn't materialize values
        let count = self.db.scan_prefix(prefix).count();
        Ok(count)
    }

    fn scan_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // First get total count efficiently
        let total = self.db.scan_prefix(prefix).count();

        // Then get paginated results
        let mut results = Vec::with_capacity(limit.min(total.saturating_sub(offset)));
        for item in self.db.scan_prefix(prefix).skip(offset).take(limit) {
            let (k, v) = item?;
            results.push((k.to_vec(), v.to_vec()));
        }

        Ok((results, total))
    }

    fn scan_reverse_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // First get total count efficiently (without loading values)
        let total = self.db.scan_prefix(prefix).count();

        // Then get paginated results in reverse order
        // Sled's scan_prefix().rev() efficiently iterates in reverse
        let mut results = Vec::with_capacity(limit.min(total.saturating_sub(offset)));
        for item in self.db.scan_prefix(prefix).rev().skip(offset).take(limit) {
            let (k, v) = item?;
            results.push((k.to_vec(), v.to_vec()));
        }

        Ok((results, total))
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

    #[test]
    fn test_basic_store_operations() -> Result<()> {
        let store = SledStore::temporary()?;

        // Test put and get
        let key = b"test:key1";
        let value = b"hello world";
        store.put(key, value)?;

        let retrieved = store.get(key)?;
        assert_eq!(retrieved.as_deref(), Some(value.as_slice()));

        // Test overwrite
        let new_value = b"updated value";
        store.put(key, new_value)?;
        let retrieved = store.get(key)?;
        assert_eq!(retrieved.as_deref(), Some(new_value.as_slice()));

        // Test get non-existent key
        let missing = store.get(b"does:not:exist")?;
        assert!(missing.is_none());

        // Test delete
        store.delete(key)?;
        let deleted = store.get(key)?;
        assert!(deleted.is_none());

        Ok(())
    }

    #[test]
    fn test_store_scan() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert multiple values with same prefix
        store.put(b"users:alice", b"Alice data")?;
        store.put(b"users:bob", b"Bob data")?;
        store.put(b"users:charlie", b"Charlie data")?;
        store.put(b"posts:post1", b"Post data")?;

        // Scan users
        let users = store.scan(b"users:")?;
        assert_eq!(users.len(), 3);

        // Scan posts
        let posts = store.scan(b"posts:")?;
        assert_eq!(posts.len(), 1);

        // Scan with non-matching prefix
        let empty = store.scan(b"nonexistent:")?;
        assert_eq!(empty.len(), 0);

        Ok(())
    }

    #[test]
    fn test_store_scan_count() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert multiple values
        store.put(b"items:1", b"data1")?;
        store.put(b"items:2", b"data2")?;
        store.put(b"items:3", b"data3")?;
        store.put(b"other:1", b"other")?;

        // Count with prefix
        assert_eq!(store.scan_count(b"items:")?, 3);
        assert_eq!(store.scan_count(b"other:")?, 1);
        assert_eq!(store.scan_count(b"nonexistent:")?, 0);

        Ok(())
    }

    #[test]
    fn test_store_scan_paginated() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert 10 items
        for i in 0..10 {
            store.put(format!("items:{i:02}").as_bytes(), b"data")?;
        }

        // Get first page (5 items)
        let (page1, total) = store.scan_paginated(b"items:", 0, 5)?;
        assert_eq!(total, 10);
        assert_eq!(page1.len(), 5);

        // Get second page
        let (page2, total) = store.scan_paginated(b"items:", 5, 5)?;
        assert_eq!(total, 10);
        assert_eq!(page2.len(), 5);

        // Get partial last page
        let (page3, total) = store.scan_paginated(b"items:", 8, 5)?;
        assert_eq!(total, 10);
        assert_eq!(page3.len(), 2);

        // Offset beyond total
        let (empty, total) = store.scan_paginated(b"items:", 20, 5)?;
        assert_eq!(total, 10);
        assert_eq!(empty.len(), 0);

        Ok(())
    }

    #[test]
    fn test_store_scan_reverse_paginated() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert items (they'll be sorted by key)
        store.put(b"items:001", b"first")?;
        store.put(b"items:002", b"second")?;
        store.put(b"items:003", b"third")?;
        store.put(b"items:004", b"fourth")?;
        store.put(b"items:005", b"fifth")?;

        // Get first page in reverse (should start with 005)
        let (page, total) = store.scan_reverse_paginated(b"items:", 0, 3)?;
        assert_eq!(total, 5);
        assert_eq!(page.len(), 3);
        assert_eq!(&page[0].0, b"items:005");
        assert_eq!(&page[1].0, b"items:004");
        assert_eq!(&page[2].0, b"items:003");

        // Get second page
        let (page2, _) = store.scan_reverse_paginated(b"items:", 3, 3)?;
        assert_eq!(page2.len(), 2);
        assert_eq!(&page2[0].0, b"items:002");
        assert_eq!(&page2[1].0, b"items:001");

        Ok(())
    }

    #[test]
    fn test_replica_health_enum() {
        // Test all enum variants
        let healthy = ReplicaHealth::Healthy;
        let stale = ReplicaHealth::Stale;
        let unreachable = ReplicaHealth::Unreachable;

        // Test equality
        assert_eq!(healthy, ReplicaHealth::Healthy);
        assert_ne!(healthy, stale);
        assert_ne!(stale, unreachable);

        // Test serialization
        let json = serde_json::to_string(&healthy).unwrap();
        assert_eq!(json, "\"Healthy\"");

        let parsed: ReplicaHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, healthy);
    }

    #[test]
    fn test_replica_info_creation() {
        let info = ReplicaInfo {
            peer_did: "did:icn:test".to_string(),
            last_seen: SystemTime::now(),
            health: ReplicaHealth::Healthy,
        };

        assert_eq!(info.peer_did, "did:icn:test");
        assert_eq!(info.health, ReplicaHealth::Healthy);

        // Test serialization roundtrip
        let json = serde_json::to_string(&info).unwrap();
        let parsed: ReplicaInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.peer_did, info.peer_did);
        assert_eq!(parsed.health, info.health);
    }

    #[test]
    fn test_sled_store_temporary() -> Result<()> {
        // Temporary store should be independent
        let store1 = SledStore::temporary()?;
        let store2 = SledStore::temporary()?;

        store1.put(b"key", b"value1")?;

        // Store 2 should not see store 1's data
        assert!(store2.get(b"key")?.is_none());

        Ok(())
    }

    #[test]
    fn test_store_delete_nonexistent() -> Result<()> {
        let store = SledStore::temporary()?;

        // Deleting non-existent key should succeed silently
        store.delete(b"nonexistent:key")?;

        // Verify the key still doesn't exist
        assert!(store.get(b"nonexistent:key")?.is_none());

        Ok(())
    }

    #[test]
    fn test_store_empty_values() -> Result<()> {
        let store = SledStore::temporary()?;

        // Empty value
        store.put(b"empty:value", b"")?;
        let retrieved = store.get(b"empty:value")?;
        assert_eq!(retrieved, Some(vec![]));

        // Empty key (edge case)
        store.put(b"", b"value for empty key")?;
        let retrieved = store.get(b"")?;
        assert_eq!(
            retrieved.as_deref(),
            Some(b"value for empty key".as_slice())
        );

        Ok(())
    }

    #[test]
    fn test_store_binary_data() -> Result<()> {
        let store = SledStore::temporary()?;

        // Binary data with null bytes
        let binary_key = &[0x00, 0x01, 0x02, 0xFF, 0xFE];
        let binary_value = &[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00];

        store.put(binary_key, binary_value)?;
        let retrieved = store.get(binary_key)?;
        assert_eq!(retrieved.as_deref(), Some(binary_value.as_slice()));

        Ok(())
    }
}
