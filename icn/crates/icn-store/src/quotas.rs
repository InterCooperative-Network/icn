//! Storage Quota Management
//!
//! Phase 18 Week 6: Prevents storage exhaustion by enforcing per-DID quotas and global limits
//!
//! This module implements storage quota enforcement with priority-based eviction. When storage
//! approaches capacity, low-priority data is automatically evicted to prevent crashes.

use anyhow::{anyhow, Result};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use tracing::{debug, info, warn};

/// Priority level for storage quota
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
)]
pub enum QuotaPriority {
    /// Lowest priority - evicted first (e.g., cached data, temp files)
    Low = 0,

    /// Normal priority - evicted second (e.g., gossip entries, routine data)
    #[default]
    Normal = 1,

    /// High priority - evicted third (e.g., contracts, trust edges)
    High = 2,

    /// Critical priority - never auto-evicted (e.g., keystore, ledger, governance)
    Critical = 3,
}

/// Storage quota for a single DID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageQuota {
    /// Maximum storage bytes allowed for this DID
    pub max_bytes: u64,

    /// Current storage usage in bytes
    pub current_bytes: u64,

    /// Priority level for this DID's data
    pub priority: QuotaPriority,

    /// When this quota was last updated
    pub updated_at: SystemTime,
}

impl StorageQuota {
    /// Create a new storage quota
    pub fn new(max_bytes: u64, priority: QuotaPriority) -> Self {
        Self {
            max_bytes,
            current_bytes: 0,
            priority,
            updated_at: SystemTime::now(),
        }
    }

    /// Check if adding size would exceed quota
    pub fn would_exceed(&self, size: u64) -> bool {
        self.current_bytes.saturating_add(size) > self.max_bytes
    }

    /// Record storage usage increase
    pub fn record_usage(&mut self, size: u64) {
        self.current_bytes = self.current_bytes.saturating_add(size);
        self.updated_at = SystemTime::now();
    }

    /// Record storage usage decrease (after deletion)
    pub fn release_usage(&mut self, size: u64) {
        self.current_bytes = self.current_bytes.saturating_sub(size);
        self.updated_at = SystemTime::now();
    }

    /// Get usage as percentage (0.0 to 1.0)
    pub fn usage_percentage(&self) -> f64 {
        if self.max_bytes == 0 {
            0.0
        } else {
            self.current_bytes as f64 / self.max_bytes as f64
        }
    }
}

/// Tracked storage item for eviction
#[derive(Debug, Clone)]
pub struct StorageItem {
    /// Storage key
    pub key: Vec<u8>,
    /// Size in bytes
    pub size: u64,
    /// Priority level for eviction
    pub priority: QuotaPriority,
    /// Owner DID
    pub did: Did,
    /// Creation timestamp
    pub created_at: SystemTime,
}

/// Storage quota manager
pub struct StorageQuotaManager {
    /// Per-DID quotas
    quotas: HashMap<Did, StorageQuota>,

    /// Global storage limit (total across all DIDs)
    global_limit: u64,

    /// Current global usage
    global_usage: u64,

    /// Tracked items for potential eviction
    items: Vec<StorageItem>,

    /// Eviction threshold (e.g., 0.9 = evict when 90% full)
    eviction_threshold: f64,
}

impl StorageQuotaManager {
    /// Create a new quota manager
    pub fn new(global_limit: u64, eviction_threshold: f64) -> Self {
        Self {
            quotas: HashMap::new(),
            global_limit,
            global_usage: 0,
            items: Vec::new(),
            eviction_threshold: eviction_threshold.clamp(0.0, 1.0),
        }
    }

    /// Set quota for a DID
    pub fn set_quota(&mut self, did: Did, max_bytes: u64, priority: QuotaPriority) {
        debug!(
            "Set quota for {}: {} bytes (priority: {:?})",
            did, max_bytes, priority
        );
        self.quotas
            .insert(did, StorageQuota::new(max_bytes, priority));
    }

    /// Get quota for a DID
    pub fn get_quota(&self, did: &Did) -> Option<&StorageQuota> {
        self.quotas.get(did)
    }

    /// Get mutable quota for a DID
    pub fn get_quota_mut(&mut self, did: &Did) -> Option<&mut StorageQuota> {
        self.quotas.get_mut(did)
    }

    /// Check if DID can store data of given size
    ///
    /// If no per-DID quota is configured, only the global limit is checked.
    /// This allows dynamic quota assignment while still enforcing global limits.
    pub fn can_store(&self, did: &Did, size: u64) -> Result<()> {
        // Check per-DID quota if one exists
        if let Some(quota) = self.quotas.get(did) {
            if quota.would_exceed(size) {
                return Err(anyhow!(
                    "Storage quota exceeded for {}: {} + {} > {} bytes",
                    did,
                    quota.current_bytes,
                    size,
                    quota.max_bytes
                ));
            }
        }
        // If no per-DID quota exists, we allow storage up to global limit

        // Check global limit
        if self.global_usage.saturating_add(size) > self.global_limit {
            return Err(anyhow!(
                "Global storage limit exceeded: {} + {} > {} bytes",
                self.global_usage,
                size,
                self.global_limit
            ));
        }

        Ok(())
    }

    /// Record storage usage for a DID
    ///
    /// If no per-DID quota exists, creates a default quota with the global limit.
    /// This allows dynamic quota assignment based on actual usage.
    ///
    /// This method validates against both per-DID quota (if exists) and global limit
    /// before recording usage, ensuring the storage operation is valid.
    pub fn record_usage(
        &mut self,
        did: &Did,
        key: Vec<u8>,
        size: u64,
        priority: QuotaPriority,
    ) -> Result<()> {
        // Check global limit first (always enforced)
        if self.global_usage.saturating_add(size) > self.global_limit {
            return Err(anyhow!(
                "Global storage limit exceeded: {} + {} > {} bytes",
                self.global_usage,
                size,
                self.global_limit
            ));
        }

        // Get or create DID quota with default (global limit as per-DID max)
        let quota = self.quotas.entry(did.clone()).or_insert_with(|| {
            debug!("Auto-creating default quota for DID: {}", did);
            StorageQuota::new(self.global_limit, QuotaPriority::Normal)
        });

        // Check per-DID quota
        if quota.would_exceed(size) {
            return Err(anyhow!(
                "Storage quota exceeded for {}: {} + {} > {} bytes",
                did,
                quota.current_bytes,
                size,
                quota.max_bytes
            ));
        }

        quota.record_usage(size);

        // Update global usage
        self.global_usage = self.global_usage.saturating_add(size);

        // Track item for potential eviction
        self.items.push(StorageItem {
            key,
            size,
            priority,
            did: did.clone(),
            created_at: SystemTime::now(),
        });

        debug!(
            "Recorded usage for {}: {} bytes ({:.1}% of quota, global: {:.1}%)",
            did,
            size,
            quota.usage_percentage() * 100.0,
            self.global_usage_percentage() * 100.0
        );

        Ok(())
    }

    /// Release storage usage for a DID
    pub fn release_usage(&mut self, did: &Did, key: &[u8], size: u64) -> Result<()> {
        // Update DID quota
        if let Some(quota) = self.quotas.get_mut(did) {
            quota.release_usage(size);
        }

        // Update global usage
        self.global_usage = self.global_usage.saturating_sub(size);

        // Remove from tracked items
        self.items.retain(|item| item.key != key);

        debug!(
            "Released usage for {}: {} bytes (global: {:.1}%)",
            did,
            size,
            self.global_usage_percentage() * 100.0
        );

        Ok(())
    }

    /// Get current global usage as percentage (0.0 to 1.0)
    pub fn global_usage_percentage(&self) -> f64 {
        if self.global_limit == 0 {
            0.0
        } else {
            self.global_usage as f64 / self.global_limit as f64
        }
    }

    /// Check if eviction is needed
    pub fn needs_eviction(&self) -> bool {
        self.global_usage_percentage() >= self.eviction_threshold
    }

    /// Evict low-priority data if needed
    pub fn evict_if_needed(&mut self) -> Result<Vec<Vec<u8>>> {
        if !self.needs_eviction() {
            return Ok(Vec::new());
        }

        info!(
            "Storage at {:.1}% capacity, starting eviction (threshold: {:.1}%)",
            self.global_usage_percentage() * 100.0,
            self.eviction_threshold * 100.0
        );

        // Sort items by priority (lowest first) and age (oldest first)
        let mut sorted_items = self.items.clone();
        sorted_items.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        let mut evicted_keys = Vec::new();
        let target_usage = (self.global_limit as f64 * (self.eviction_threshold - 0.1)) as u64;

        for item in sorted_items {
            // Never evict critical priority data
            if item.priority == QuotaPriority::Critical {
                continue;
            }

            // Stop if we're below target
            if self.global_usage <= target_usage {
                break;
            }

            warn!(
                "Evicting item from {} (priority: {:?}, size: {} bytes)",
                item.did, item.priority, item.size
            );

            // Release usage (this will update quotas and global usage)
            self.release_usage(&item.did, &item.key, item.size)?;

            evicted_keys.push(item.key);
        }

        info!(
            "Eviction complete: {} items evicted, usage now {:.1}%",
            evicted_keys.len(),
            self.global_usage_percentage() * 100.0
        );

        Ok(evicted_keys)
    }

    /// Get statistics about quota usage
    pub fn get_stats(&self) -> QuotaStats {
        let total_quotas = self.quotas.len();
        let mut exceeded_quotas = 0;
        let mut by_priority = HashMap::new();

        for quota in self.quotas.values() {
            if quota.usage_percentage() >= 1.0 {
                exceeded_quotas += 1;
            }

            *by_priority.entry(quota.priority).or_insert(0u64) += quota.current_bytes;
        }

        QuotaStats {
            total_quotas,
            exceeded_quotas,
            global_usage: self.global_usage,
            global_limit: self.global_limit,
            global_usage_percentage: self.global_usage_percentage(),
            usage_by_priority: by_priority,
            tracked_items: self.items.len(),
        }
    }
}

/// Statistics about quota usage
#[derive(Debug, Clone)]
pub struct QuotaStats {
    /// Total number of quotas configured
    pub total_quotas: usize,
    /// Number of quotas that are exceeded
    pub exceeded_quotas: usize,
    /// Total global storage usage in bytes
    pub global_usage: u64,
    /// Global storage limit in bytes
    pub global_limit: u64,
    /// Global usage as percentage of limit
    pub global_usage_percentage: f64,
    /// Storage usage breakdown by priority level
    pub usage_by_priority: HashMap<QuotaPriority, u64>,
    /// Number of items currently tracked
    pub tracked_items: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_storage_quota_new() {
        let quota = StorageQuota::new(1000, QuotaPriority::Normal);

        assert_eq!(quota.max_bytes, 1000);
        assert_eq!(quota.current_bytes, 0);
        assert_eq!(quota.priority, QuotaPriority::Normal);
        assert_eq!(quota.usage_percentage(), 0.0);
    }

    #[test]
    fn test_storage_quota_would_exceed() {
        let quota = StorageQuota::new(1000, QuotaPriority::Normal);

        assert!(!quota.would_exceed(500));
        assert!(!quota.would_exceed(1000));
        assert!(quota.would_exceed(1001));
    }

    #[test]
    fn test_storage_quota_record_usage() {
        let mut quota = StorageQuota::new(1000, QuotaPriority::Normal);

        quota.record_usage(300);
        assert_eq!(quota.current_bytes, 300);
        assert_eq!(quota.usage_percentage(), 0.3);

        quota.record_usage(400);
        assert_eq!(quota.current_bytes, 700);
        assert_eq!(quota.usage_percentage(), 0.7);
    }

    #[test]
    fn test_storage_quota_release_usage() {
        let mut quota = StorageQuota::new(1000, QuotaPriority::Normal);

        quota.record_usage(700);
        assert_eq!(quota.current_bytes, 700);

        quota.release_usage(300);
        assert_eq!(quota.current_bytes, 400);
        assert_eq!(quota.usage_percentage(), 0.4);

        // Should not go negative
        quota.release_usage(500);
        assert_eq!(quota.current_bytes, 0);
    }

    #[test]
    fn test_quota_manager_set_and_get() {
        let mut manager = StorageQuotaManager::new(10000, 0.9);
        let did = make_test_did();

        manager.set_quota(did.clone(), 1000, QuotaPriority::High);

        let quota = manager.get_quota(&did).unwrap();
        assert_eq!(quota.max_bytes, 1000);
        assert_eq!(quota.priority, QuotaPriority::High);
    }

    #[test]
    fn test_quota_manager_can_store() {
        let mut manager = StorageQuotaManager::new(10000, 0.9);
        let did = make_test_did();

        manager.set_quota(did.clone(), 1000, QuotaPriority::Normal);

        // Should allow storage within quota
        assert!(manager.can_store(&did, 500).is_ok());
        assert!(manager.can_store(&did, 1000).is_ok());

        // Should deny storage exceeding quota
        assert!(manager.can_store(&did, 1001).is_err());
    }

    #[test]
    fn test_quota_manager_record_and_release() {
        let mut manager = StorageQuotaManager::new(10000, 0.9);
        let did = make_test_did();

        manager.set_quota(did.clone(), 1000, QuotaPriority::Normal);

        // Record usage
        let key = b"test_key".to_vec();
        manager
            .record_usage(&did, key.clone(), 300, QuotaPriority::Normal)
            .unwrap();

        assert_eq!(manager.global_usage, 300);
        assert_eq!(manager.get_quota(&did).unwrap().current_bytes, 300);

        // Release usage
        manager.release_usage(&did, &key, 300).unwrap();

        assert_eq!(manager.global_usage, 0);
        assert_eq!(manager.get_quota(&did).unwrap().current_bytes, 0);
    }

    #[test]
    fn test_quota_manager_global_limit() {
        let mut manager = StorageQuotaManager::new(1000, 0.9);
        let did1 = make_test_did();
        let did2 = make_test_did();

        manager.set_quota(did1.clone(), 800, QuotaPriority::Normal);
        manager.set_quota(did2.clone(), 800, QuotaPriority::Normal);

        // First DID can use 600 bytes
        manager
            .record_usage(&did1, b"key1".to_vec(), 600, QuotaPriority::Normal)
            .unwrap();

        // Second DID can only use 400 bytes due to global limit
        assert!(manager.can_store(&did2, 400).is_ok());
        assert!(manager.can_store(&did2, 401).is_err());
    }

    #[test]
    fn test_quota_manager_eviction() {
        let mut manager = StorageQuotaManager::new(1000, 0.9);
        let did1 = make_test_did();
        let did2 = make_test_did();

        manager.set_quota(did1.clone(), 800, QuotaPriority::Normal);
        manager.set_quota(did2.clone(), 800, QuotaPriority::Normal);

        // Add low-priority data
        manager
            .record_usage(&did1, b"low1".to_vec(), 300, QuotaPriority::Low)
            .unwrap();
        manager
            .record_usage(&did1, b"low2".to_vec(), 300, QuotaPriority::Low)
            .unwrap();

        // Add normal-priority data
        manager
            .record_usage(&did2, b"normal".to_vec(), 300, QuotaPriority::Normal)
            .unwrap();

        // Total usage: 900 bytes (90% of 1000)
        assert_eq!(manager.global_usage, 900);
        assert!(manager.needs_eviction());

        // Trigger eviction
        let evicted = manager.evict_if_needed().unwrap();

        // Should evict low-priority items first
        assert!(!evicted.is_empty());
        assert!(evicted.contains(&b"low1".to_vec()) || evicted.contains(&b"low2".to_vec()));

        // Usage should be reduced
        assert!(manager.global_usage < 900);
    }

    #[test]
    fn test_quota_manager_no_eviction_of_critical() {
        let mut manager = StorageQuotaManager::new(1000, 0.9);
        let did = make_test_did();

        manager.set_quota(did.clone(), 1000, QuotaPriority::Critical);

        // Fill with critical data
        manager
            .record_usage(&did, b"critical".to_vec(), 950, QuotaPriority::Critical)
            .unwrap();

        assert!(manager.needs_eviction());

        // Eviction should not remove critical data
        let evicted = manager.evict_if_needed().unwrap();
        assert_eq!(evicted.len(), 0);
        assert_eq!(manager.global_usage, 950);
    }

    #[test]
    fn test_quota_priority_ordering() {
        assert!(QuotaPriority::Low < QuotaPriority::Normal);
        assert!(QuotaPriority::Normal < QuotaPriority::High);
        assert!(QuotaPriority::High < QuotaPriority::Critical);
    }

    #[test]
    fn test_quota_stats() {
        let mut manager = StorageQuotaManager::new(10000, 0.9);
        let did1 = make_test_did();
        let did2 = make_test_did();

        manager.set_quota(did1.clone(), 1000, QuotaPriority::High);
        manager.set_quota(did2.clone(), 1000, QuotaPriority::Normal);

        manager
            .record_usage(&did1, b"key1".to_vec(), 500, QuotaPriority::High)
            .unwrap();
        manager
            .record_usage(&did2, b"key2".to_vec(), 300, QuotaPriority::Normal)
            .unwrap();

        let stats = manager.get_stats();

        assert_eq!(stats.total_quotas, 2);
        assert_eq!(stats.global_usage, 800);
        assert_eq!(stats.tracked_items, 2);
        assert_eq!(
            *stats.usage_by_priority.get(&QuotaPriority::High).unwrap(),
            500
        );
        assert_eq!(
            *stats.usage_by_priority.get(&QuotaPriority::Normal).unwrap(),
            300
        );
    }
}
