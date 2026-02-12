//! VUI Registry - Distributed Uniqueness Checking
//!
//! The VUI registry ensures that each Verifiable Unique Identifier can only
//! be registered once. It uses Bloom filters for efficient probabilistic
//! membership checking with full verification on positives.

use bloomfilter::Bloom;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::config::StewardConfig;

/// VUI Registry for uniqueness checking
pub struct VuiRegistry {
    /// Bloom filter for fast probabilistic checks
    bloom: Bloom<[u8; 32]>,

    /// Exact VUI hashes for verification (stewards only)
    /// In production, this would be stored in icn-store
    exact_hashes: HashMap<[u8; 32], VuiRegistration>,

    /// Current checkpoint hash
    checkpoint_hash: [u8; 32],

    /// Sequence number for sync ordering
    sequence: u64,

    /// Configuration (reserved for future use)
    #[allow(dead_code)]
    config: StewardConfig,
}

/// Registration record for a VUI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VuiRegistration {
    /// Hash of the VUI
    pub vui_hash: [u8; 32],

    /// DID that registered this VUI
    pub registered_by: Did,

    /// Unix timestamp of registration
    pub registered_at: u64,

    /// Steward that witnessed the registration
    pub witnessing_steward: Did,
}

/// Result of a local uniqueness check
#[derive(Debug, Clone)]
pub enum LocalCheckResult {
    /// Definitely not registered (Bloom filter negative)
    NotRegistered,

    /// Possibly registered (Bloom filter positive, needs verification)
    PossiblyRegistered,

    /// Definitely registered (found in exact set)
    Registered(VuiRegistration),
}

/// Result of a distributed uniqueness check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedCheckResult {
    /// VUI hash that was checked
    pub vui_hash: [u8; 32],

    /// Whether the VUI is registered
    pub is_registered: bool,

    /// Number of stewards that confirmed
    pub confirmations: u32,

    /// Threshold required
    pub threshold: u32,

    /// Registration details if found
    pub registration: Option<VuiRegistration>,
}

impl VuiRegistry {
    /// Create a new VUI registry with given configuration
    pub fn new(config: StewardConfig) -> Self {
        // SAFETY: StewardConfig provides valid bloom filter parameters
        #[allow(clippy::unwrap_used)]
        let bloom = Bloom::new_for_fp_rate(
            config.bloom_expected_items,
            config.bloom_false_positive_rate,
        )
        .unwrap();

        Self {
            bloom,
            exact_hashes: HashMap::new(),
            checkpoint_hash: [0u8; 32],
            sequence: 0,
            config,
        }
    }

    /// Create a VUI registry with default configuration
    pub fn with_defaults() -> Self {
        Self::new(StewardConfig::default())
    }

    /// Check if a VUI is registered (local check only)
    pub fn check_local(&self, vui_hash: &[u8; 32]) -> LocalCheckResult {
        // First check Bloom filter for fast negative
        if !self.bloom.check(vui_hash) {
            return LocalCheckResult::NotRegistered;
        }

        // Bloom positive - check exact set
        if let Some(registration) = self.exact_hashes.get(vui_hash) {
            LocalCheckResult::Registered(registration.clone())
        } else {
            // False positive from Bloom filter
            LocalCheckResult::PossiblyRegistered
        }
    }

    /// Register a VUI (called after successful enrollment)
    pub fn register(
        &mut self,
        vui_hash: [u8; 32],
        registered_by: Did,
        witnessing_steward: Did,
    ) -> Result<(), VuiRegistryError> {
        // Check if already registered
        if self.exact_hashes.contains_key(&vui_hash) {
            return Err(VuiRegistryError::AlreadyRegistered);
        }

        let now = icn_time::current_timestamp_secs();

        let registration = VuiRegistration {
            vui_hash,
            registered_by,
            registered_at: now,
            witnessing_steward,
        };

        // Add to Bloom filter
        self.bloom.set(&vui_hash);

        // Add to exact set
        self.exact_hashes.insert(vui_hash, registration);

        // Update checkpoint
        self.update_checkpoint(&vui_hash);
        self.sequence += 1;

        Ok(())
    }

    /// Get the current checkpoint hash
    pub fn checkpoint_hash(&self) -> &[u8; 32] {
        &self.checkpoint_hash
    }

    /// Get the current sequence number
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Get the number of registered VUIs
    pub fn len(&self) -> usize {
        self.exact_hashes.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.exact_hashes.is_empty()
    }

    /// Update checkpoint hash after a new registration
    fn update_checkpoint(&mut self, new_vui_hash: &[u8; 32]) {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-vui-checkpoint-v1");
        hasher.update(self.checkpoint_hash);
        hasher.update(new_vui_hash);
        hasher.update(self.sequence.to_le_bytes());

        let hash = hasher.finalize();
        self.checkpoint_hash.copy_from_slice(&hash);
    }

    /// Get all VUI hashes since a given checkpoint (for sync)
    pub fn get_hashes_since(&self, _checkpoint: &[u8; 32]) -> Vec<[u8; 32]> {
        // In production, this would use a proper log or index
        // For now, return all hashes (simplified)
        self.exact_hashes.keys().copied().collect()
    }

    /// Apply a batch of VUI hashes from sync
    pub fn apply_sync_batch(&mut self, vui_hashes: &[[u8; 32]]) {
        for hash in vui_hashes {
            self.bloom.set(hash);
            // Note: We don't have full registration info from sync
            // In production, sync would include the full VuiRegistration
        }
    }

    /// Get a registration by VUI hash
    pub fn get_registration(&self, vui_hash: &[u8; 32]) -> Option<&VuiRegistration> {
        self.exact_hashes.get(vui_hash)
    }
}

/// VUI Registry errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum VuiRegistryError {
    /// VUI is already registered
    #[error("VUI is already registered")]
    AlreadyRegistered,

    /// Sync error
    #[error("Sync error: {0}")]
    SyncError(String),

    /// Invalid checkpoint
    #[error("Invalid checkpoint")]
    InvalidCheckpoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_registry_creation() {
        let registry = VuiRegistry::with_defaults();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert_eq!(registry.sequence(), 0);
    }

    #[test]
    fn test_vui_registration() {
        let mut registry = VuiRegistry::with_defaults();
        let vui_hash = [42u8; 32];
        let user_did = test_did();
        let steward_did = test_did();

        // Should not be registered initially
        assert!(matches!(
            registry.check_local(&vui_hash),
            LocalCheckResult::NotRegistered
        ));

        // Register
        registry
            .register(vui_hash, user_did.clone(), steward_did)
            .unwrap();

        // Should be registered now
        match registry.check_local(&vui_hash) {
            LocalCheckResult::Registered(reg) => {
                assert_eq!(reg.vui_hash, vui_hash);
                assert_eq!(reg.registered_by, user_did);
            }
            _ => panic!("Expected Registered result"),
        }

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.sequence(), 1);
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = VuiRegistry::with_defaults();
        let vui_hash = [42u8; 32];
        let user_did = test_did();
        let steward_did = test_did();

        // First registration should succeed
        registry
            .register(vui_hash, user_did.clone(), steward_did.clone())
            .unwrap();

        // Second registration should fail
        let result = registry.register(vui_hash, user_did, steward_did);
        assert!(matches!(result, Err(VuiRegistryError::AlreadyRegistered)));
    }

    #[test]
    fn test_bloom_filter_efficiency() {
        let mut registry = VuiRegistry::with_defaults();
        let steward_did = test_did();

        // Register some VUIs
        for i in 0..100u8 {
            let mut vui_hash = [0u8; 32];
            vui_hash[0] = i;
            let user_did = test_did();
            registry
                .register(vui_hash, user_did, steward_did.clone())
                .unwrap();
        }

        // Check an unregistered VUI - Bloom filter should give quick negative
        let unregistered = [255u8; 32];
        assert!(matches!(
            registry.check_local(&unregistered),
            LocalCheckResult::NotRegistered
        ));
    }

    #[test]
    fn test_checkpoint_updates() {
        let mut registry = VuiRegistry::with_defaults();
        let steward_did = test_did();

        let initial_checkpoint = *registry.checkpoint_hash();
        assert_eq!(initial_checkpoint, [0u8; 32]);

        // Register first VUI
        let vui1 = [1u8; 32];
        registry
            .register(vui1, test_did(), steward_did.clone())
            .unwrap();
        let checkpoint1 = *registry.checkpoint_hash();
        assert_ne!(checkpoint1, initial_checkpoint);

        // Register second VUI
        let vui2 = [2u8; 32];
        registry.register(vui2, test_did(), steward_did).unwrap();
        let checkpoint2 = *registry.checkpoint_hash();
        assert_ne!(checkpoint2, checkpoint1);
    }

    #[test]
    fn test_get_registration() {
        let mut registry = VuiRegistry::with_defaults();
        let vui_hash = [42u8; 32];
        let user_did = test_did();
        let steward_did = test_did();

        registry
            .register(vui_hash, user_did.clone(), steward_did)
            .unwrap();

        let reg = registry.get_registration(&vui_hash).unwrap();
        assert_eq!(reg.registered_by, user_did);

        // Non-existent VUI
        let unknown = [99u8; 32];
        assert!(registry.get_registration(&unknown).is_none());
    }
}
