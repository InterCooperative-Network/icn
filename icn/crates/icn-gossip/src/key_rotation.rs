//! Key rotation gossip messages (Issue #469)
//!
//! Message types for propagating key rotation events across the network.
//! When a node rotates its keys, it publishes an announcement so peers
//! can update their key caches.

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Topic for key rotation announcements
pub const TOPIC_KEY_ROTATION: &str = "key:rotation";

/// Grace period in seconds during which both old and new keys are accepted
pub const KEY_ROTATION_GRACE_PERIOD_SECS: u64 = 3600; // 1 hour

/// Key rotation gossip message
///
/// These messages propagate on the `key:rotation` topic with Known trust requirement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KeyRotationMessage {
    /// Announce a key rotation
    ///
    /// Published when a node rotates its Ed25519 signing key.
    /// Peers should verify both signatures before updating their key cache.
    RotationAnnouncement {
        /// DID before rotation (old key fingerprint)
        old_did: Did,
        /// DID after rotation (new key fingerprint)
        new_did: Did,
        /// New public key bytes (Ed25519, 32 bytes)
        new_public_key: Vec<u8>,
        /// Unix timestamp when rotation occurred
        timestamp: u64,
        /// Reason for rotation
        reason: RotationReason,
        /// Signature from old key proving authorization
        /// Signs: "key-rotation:{old_did}:{new_did}:{timestamp}"
        signature_old: Vec<u8>,
        /// Signature from new key proving possession
        /// Signs: "key-rotation:{old_did}:{new_did}:{timestamp}"
        signature_new: Vec<u8>,
    },

    /// Query if a peer has rotated their key
    ///
    /// Used to catch up on missed rotation announcements.
    RotationQuery {
        /// DID to query about
        queried_did: Did,
        /// Requester's DID
        requester: Did,
    },

    /// Response to a rotation query
    RotationResponse {
        /// DID queried
        queried_did: Did,
        /// Current active DID (may be same if no rotation)
        current_did: Did,
        /// Timestamp of last rotation (0 if never rotated)
        last_rotation: u64,
    },
}

/// Reason for key rotation (matches identity crate's RotationReason)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RotationReason {
    /// Regular scheduled rotation
    Scheduled,
    /// Key was potentially compromised
    Compromised,
    /// Upgrading to stronger key algorithm
    Upgrade,
    /// Manual user-initiated rotation
    Manual,
}

impl KeyRotationMessage {
    /// Create a rotation announcement message
    pub fn announcement(
        old_did: Did,
        new_did: Did,
        new_public_key: Vec<u8>,
        timestamp: u64,
        reason: RotationReason,
        signature_old: Vec<u8>,
        signature_new: Vec<u8>,
    ) -> Self {
        KeyRotationMessage::RotationAnnouncement {
            old_did,
            new_did,
            new_public_key,
            timestamp,
            reason,
            signature_old,
            signature_new,
        }
    }

    /// Create a rotation query message
    pub fn query(queried_did: Did, requester: Did) -> Self {
        KeyRotationMessage::RotationQuery {
            queried_did,
            requester,
        }
    }

    /// Create a rotation response message
    pub fn response(queried_did: Did, current_did: Did, last_rotation: u64) -> Self {
        KeyRotationMessage::RotationResponse {
            queried_did,
            current_did,
            last_rotation,
        }
    }

    /// Get the message to be signed for rotation verification
    ///
    /// Format: "key-rotation:{old_did}:{new_did}:{timestamp}"
    pub fn rotation_message(old_did: &Did, new_did: &Did, timestamp: u64) -> String {
        format!("key-rotation:{old_did}:{new_did}:{timestamp}")
    }
}

/// Tracks active key rotations for grace period handling
#[derive(Debug, Default)]
pub struct KeyRotationCache {
    /// Map from old DID to (new DID, expiry timestamp)
    rotations: std::collections::HashMap<String, (Did, u64)>,
}

impl KeyRotationCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a key rotation
    pub fn record_rotation(&mut self, old_did: &Did, new_did: Did, timestamp: u64) {
        let expiry = timestamp + KEY_ROTATION_GRACE_PERIOD_SECS;
        self.rotations
            .insert(old_did.to_string(), (new_did, expiry));
    }

    /// Check if a DID has been rotated and return the new DID if within grace period
    pub fn get_rotated_did(&self, old_did: &Did, current_time: u64) -> Option<&Did> {
        self.rotations
            .get(&old_did.to_string())
            .filter(|(_, expiry)| current_time < *expiry)
            .map(|(new_did, _)| new_did)
    }

    /// Check if a DID is valid (either current or within grace period after rotation)
    pub fn is_did_valid(&self, did: &Did, current_time: u64) -> bool {
        // DID is valid if it hasn't been rotated, or if within grace period
        match self.rotations.get(&did.to_string()) {
            Some((_, expiry)) => current_time < *expiry,
            None => true, // Not rotated, still valid
        }
    }

    /// Clean up expired rotation records
    pub fn cleanup_expired(&mut self, current_time: u64) {
        self.rotations
            .retain(|_, (_, expiry)| current_time < *expiry);
    }

    /// Get the number of active rotation records
    pub fn len(&self) -> usize {
        self.rotations.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.rotations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_rotation_message_serialization() {
        let config = bincode::config::standard();
        let old_keypair = KeyPair::generate().unwrap();
        let new_keypair = KeyPair::generate().unwrap();

        let msg = KeyRotationMessage::RotationAnnouncement {
            old_did: old_keypair.did().clone(),
            new_did: new_keypair.did().clone(),
            new_public_key: new_keypair.verifying_key().to_bytes().to_vec(),
            timestamp: 1234567890,
            reason: RotationReason::Manual,
            signature_old: vec![5, 6, 7, 8],
            signature_new: vec![9, 10, 11, 12],
        };

        let serialized = bincode::serde::encode_to_vec(&msg, config).expect("serialize");
        let (deserialized, _): (KeyRotationMessage, _) =
            bincode::serde::decode_from_slice(&serialized, config).expect("deserialize");
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_rotation_cache() {
        let mut cache = KeyRotationCache::new();
        let old_keypair = KeyPair::generate().unwrap();
        let new_keypair = KeyPair::generate().unwrap();
        let old_did = old_keypair.did().clone();
        let new_did = new_keypair.did().clone();

        // Record rotation at time 1000
        cache.record_rotation(&old_did, new_did.clone(), 1000);

        // Within grace period (1 hour = 3600 seconds)
        assert!(cache.is_did_valid(&old_did, 1500));
        assert_eq!(cache.get_rotated_did(&old_did, 1500), Some(&new_did));

        // After grace period
        assert!(!cache.is_did_valid(&old_did, 5000));
        assert_eq!(cache.get_rotated_did(&old_did, 5000), None);
    }

    #[test]
    fn test_rotation_message_format() {
        let old_keypair = KeyPair::generate().unwrap();
        let new_keypair = KeyPair::generate().unwrap();
        let old_did = old_keypair.did().clone();
        let new_did = new_keypair.did().clone();

        let message = KeyRotationMessage::rotation_message(&old_did, &new_did, 1234567890);
        // Verify message has expected format: "key-rotation:{old_did}:{new_did}:{timestamp}"
        assert!(message.starts_with("key-rotation:did:icn:"));
        assert!(message.ends_with(":1234567890"));
        assert!(message.contains(&old_did.to_string()));
        assert!(message.contains(&new_did.to_string()));
    }

    #[test]
    fn test_cache_cleanup() {
        let mut cache = KeyRotationCache::new();
        let kp1_old = KeyPair::generate().unwrap();
        let kp1_new = KeyPair::generate().unwrap();
        let kp2_old = KeyPair::generate().unwrap();
        let kp2_new = KeyPair::generate().unwrap();

        let old1 = kp1_old.did().clone();
        let new1 = kp1_new.did().clone();
        let old2 = kp2_old.did().clone();
        let new2 = kp2_new.did().clone();

        cache.record_rotation(&old1, new1, 1000);
        cache.record_rotation(&old2, new2, 2000);

        assert_eq!(cache.len(), 2);

        // Cleanup at time 5000 - old1 expired (1000 + 3600 = 4600), old2 still valid
        cache.cleanup_expired(5000);
        assert_eq!(cache.len(), 1);

        // Cleanup at time 6000 - both expired
        cache.cleanup_expired(6000);
        assert!(cache.is_empty());
    }
}
