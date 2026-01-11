//! Identity synchronization protocol
//!
//! This module implements the protocol for synchronizing DID Documents across
//! the ICN network using gossip. When a device is added, revoked, or rotated,
//! the corresponding RotationEvent is broadcast to all nodes.
//!
//! # Protocol Overview
//!
//! 1. When a device is added/revoked/rotated, create an `IdentityUpdateMessage`
//! 2. Serialize to bytes using bincode
//! 3. Publish to the `identity:updates` gossip topic
//! 4. Receivers apply the RotationEvent to their cached DID Document
//! 5. Message verification uses the updated DID Document

use crate::{Did, DidDocument, RotationEvent};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::warn;

/// Gossip topic for identity updates
pub const IDENTITY_UPDATES_TOPIC: &str = "identity:updates";

/// Identity update message broadcast via gossip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityUpdateMessage {
    /// The DID this update applies to
    pub did: Did,

    /// The rotation event describing the change
    pub event: RotationEvent,

    /// Current DID Document version after applying this event
    pub new_version: u64,

    /// Timestamp when this update was created
    pub timestamp: u64,
}

impl IdentityUpdateMessage {
    /// Create a new identity update message
    pub fn new(did: Did, event: RotationEvent, new_version: u64, timestamp: u64) -> Self {
        Self {
            did,
            event,
            new_version,
            timestamp,
        }
    }

    /// Serialize to bytes for gossip
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(icn_encoding::encode_bincode_legacy(self)?)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(icn_encoding::decode_bincode_legacy(bytes)?)
    }
}

/// Cache of DID Documents for identity verification
///
/// This cache stores the current DID Document for each identity seen on the network.
/// It's used by NetworkActor to verify that incoming messages are signed by authorized keys.
#[derive(Debug, Clone)]
pub struct DidDocumentCache {
    inner: Arc<RwLock<HashMap<Did, CachedDidDocument>>>,
}

/// Cached DID Document with metadata
#[derive(Debug, Clone)]
struct CachedDidDocument {
    /// The DID Document
    document: DidDocument,

    /// When this document was last updated
    last_updated: u64,

    /// Source of this document (local or remote DID)
    /// Reserved for future use (e.g., trust prioritization)
    #[allow(dead_code)]
    source: DocumentSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DocumentSource {
    /// This is our own DID Document
    Local,

    /// Received from another node
    Remote(Did),
}

impl DidDocumentCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or update a DID Document in the cache
    ///
    /// Returns true if the document was added/updated, false if rejected
    /// (e.g., older version than what we already have)
    pub fn update(&self, did: Did, document: DidDocument, source: Did) -> bool {
        let mut cache = self.inner.write().unwrap_or_else(|poisoned| {
            warn!("DID cache lock poisoned, recovering");
            poisoned.into_inner()
        });

        // Check if we already have a newer version
        if let Some(cached) = cache.get(&did) {
            if cached.document.version >= document.version {
                return false; // Already have this version or newer
            }
        }

        // Insert/update
        cache.insert(
            did.clone(),
            CachedDidDocument {
                document,
                last_updated: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::from_secs(0))
                    .as_secs(),
                source: DocumentSource::Remote(source),
            },
        );

        true
    }

    /// Add our own DID Document to the cache
    pub fn update_local(&self, did: Did, document: DidDocument) {
        let mut cache = self.inner.write().unwrap_or_else(|poisoned| {
            warn!("DID cache lock poisoned, recovering");
            poisoned.into_inner()
        });

        cache.insert(
            did.clone(),
            CachedDidDocument {
                document,
                last_updated: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::from_secs(0))
                    .as_secs(),
                source: DocumentSource::Local,
            },
        );
    }

    /// Get a DID Document from the cache
    pub fn get(&self, did: &Did) -> Option<DidDocument> {
        let cache = self.inner.read().unwrap_or_else(|poisoned| {
            warn!("DID cache lock poisoned, recovering");
            poisoned.into_inner()
        });
        cache.get(did).map(|cached| cached.document.clone())
    }

    /// Check if a public key is authorized to sign for a DID
    pub fn can_sign(&self, did: &Did, public_key_bytes: &[u8]) -> bool {
        let cache = self.inner.read().unwrap_or_else(|poisoned| {
            warn!("DID cache lock poisoned, recovering");
            poisoned.into_inner()
        });

        if let Some(cached) = cache.get(did) {
            // Check if any non-revoked Ed25519 key matches
            cached.document.verification_method.iter().any(|vm| {
                vm.key_type == crate::KeyType::Ed25519
                    && vm.public_key == public_key_bytes
                    && vm.revoked_at.is_none()
                    && vm.capabilities.contains(&crate::Capability::Sign)
            })
        } else {
            false
        }
    }

    /// Apply a rotation event to update a cached DID Document
    ///
    /// Returns true if the event was applied successfully
    pub fn apply_event(&self, did: &Did, event: &RotationEvent) -> Result<bool> {
        let mut cache = self.inner.write().unwrap_or_else(|poisoned| {
            warn!("DID cache lock poisoned, recovering");
            poisoned.into_inner()
        });

        let cached = match cache.get_mut(did) {
            Some(c) => c,
            None => return Ok(false), // Don't have this DID cached yet
        };

        // Clone the document to apply the event
        let mut updated_doc = cached.document.clone();

        // Apply the event based on type
        match &event.event_type {
            crate::RotationEventType::AddDevice {
                device_id,
                label,
                public_key,
                key_type,
                capabilities,
            } => {
                updated_doc.add_device(
                    device_id.clone(),
                    label.clone(),
                    public_key.clone(),
                    *key_type,
                    capabilities.clone(),
                )?;
            }
            crate::RotationEventType::AddDeviceWithEncryption {
                device_id,
                label,
                ed25519_public_key,
                x25519_public_key,
                signing_capabilities,
            } => {
                updated_doc.add_device_with_encryption_key(
                    device_id.clone(),
                    label.clone(),
                    ed25519_public_key.clone(),
                    x25519_public_key.clone(),
                    signing_capabilities.clone(),
                )?;
            }
            crate::RotationEventType::RevokeDevice { device_id, .. } => {
                updated_doc.revoke_device(device_id)?;
            }
            crate::RotationEventType::RotateKey {
                device_id,
                old_key,
                new_key,
            } => {
                updated_doc.rotate_key(device_id, old_key, new_key.clone())?;
            }
            crate::RotationEventType::Recover {
                new_root_key,
                recovery_proofs,
            } => {
                // Verify recovery proofs against RecoveryConfig
                if let Some(recovery_config) = &updated_doc.recovery {
                    // Verify we have enough valid proofs
                    if recovery_proofs.len() < recovery_config.threshold as usize {
                        tracing::warn!(
                            "Insufficient recovery proofs: got {}, need {}",
                            recovery_proofs.len(),
                            recovery_config.threshold
                        );
                        return Ok(false);
                    }

                    // Verify each proof is from a valid trustee
                    let mut valid_proofs = 0;
                    for proof in recovery_proofs {
                        if recovery_config.trustees.contains(&proof.trustee) {
                            // In production, verify the cryptographic signature here
                            // For now, accept if trustee is in the list
                            valid_proofs += 1;
                        }
                    }

                    if valid_proofs < recovery_config.threshold as usize {
                        tracing::warn!(
                            "Insufficient valid recovery proofs: got {}, need {}",
                            valid_proofs,
                            recovery_config.threshold
                        );
                        return Ok(false);
                    }

                    // Recovery verified - reset to new root key
                    // Clear all existing verification methods
                    updated_doc.verification_method.clear();
                    updated_doc.authentication.clear();

                    // Add new root device
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs();

                    updated_doc
                        .verification_method
                        .push(crate::VerificationMethod {
                            id: "root-recovered".to_string(),
                            label: "Recovered Root Device".to_string(),
                            key_type: crate::KeyType::Ed25519,
                            public_key: new_root_key.clone(),
                            capabilities: vec![
                                crate::Capability::Sign,
                                crate::Capability::AddDevice,
                                crate::Capability::RevokeDevice,
                                crate::Capability::Recover,
                            ],
                            added_at: now,
                            revoked_at: None,
                        });

                    updated_doc
                        .authentication
                        .push("root-recovered".to_string());
                    updated_doc.version += 1;
                    updated_doc.updated_at = now;

                    tracing::info!(
                        "Successfully recovered DID {:?} with new root key",
                        updated_doc.id
                    );
                } else {
                    tracing::warn!(
                        "Cannot recover DID {:?}: no recovery config",
                        updated_doc.id
                    );
                    return Ok(false);
                }
            }
        }

        // Verify the version matches
        if updated_doc.version != event.new_version {
            anyhow::bail!(
                "Version mismatch: event claims version {}, but applying it resulted in version {}",
                event.new_version,
                updated_doc.version
            );
        }

        // Update the cache
        cached.document = updated_doc;
        cached.last_updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs();

        Ok(true)
    }

    /// Get the number of cached documents
    pub fn len(&self) -> usize {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("DID cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("DID cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .is_empty()
    }
}

impl Default for DidDocumentCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capability, KeyPair, KeyType};

    #[test]
    fn test_identity_update_serialization() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let event = RotationEvent {
            did: did.clone(),
            event_type: crate::RotationEventType::AddDevice {
                device_id: "device-2".to_string(),
                label: "Laptop".to_string(),
                public_key: vec![1u8; 32],
                key_type: KeyType::Ed25519,
                capabilities: vec![Capability::Sign],
            },
            proof: vec![],
            signed_by: "device-1".to_string(),
            timestamp: 1234567890,
            new_version: 2,
        };

        let msg = IdentityUpdateMessage::new(did.clone(), event, 2, 1234567890);

        // Serialize and deserialize
        let bytes = msg.to_bytes().unwrap();
        let msg2 = IdentityUpdateMessage::from_bytes(&bytes).unwrap();

        assert_eq!(msg.did, msg2.did);
        assert_eq!(msg.new_version, msg2.new_version);
        assert_eq!(msg.timestamp, msg2.timestamp);
    }

    #[test]
    fn test_cache_basic_operations() {
        let cache = DidDocumentCache::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Generate X25519 keys for DID Document
        use rand::rngs::OsRng;
        use x25519_dalek::{PublicKey, StaticSecret};
        let x25519_secret = StaticSecret::random_from_rng(OsRng);
        let x25519_public = PublicKey::from(&x25519_secret);

        let doc = crate::DidDocument::new(
            did.clone(),
            keypair.verifying_key(),
            x25519_public.as_bytes(),
        );

        // Initially empty
        assert!(cache.is_empty());
        assert!(cache.get(&did).is_none());

        // Add document
        cache.update_local(did.clone(), doc.clone());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&did).unwrap().version, 1);

        // Check can_sign
        assert!(cache.can_sign(&did, keypair.verifying_key().as_bytes()));
        assert!(!cache.can_sign(&did, &[0u8; 32])); // Wrong key
    }

    #[test]
    fn test_cache_version_ordering() {
        let cache = DidDocumentCache::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let source_did = KeyPair::generate().unwrap().did().clone();

        use rand::rngs::OsRng;
        use x25519_dalek::{PublicKey, StaticSecret};
        let x25519_secret = StaticSecret::random_from_rng(OsRng);
        let x25519_public = PublicKey::from(&x25519_secret);

        let mut doc_v1 = crate::DidDocument::new(
            did.clone(),
            keypair.verifying_key(),
            x25519_public.as_bytes(),
        );

        // Add v1
        assert!(cache.update(did.clone(), doc_v1.clone(), source_did.clone()));

        // Try to add same version - should reject
        assert!(!cache.update(did.clone(), doc_v1.clone(), source_did.clone()));

        // Create v2 by adding a device
        doc_v1
            .add_device(
                "device-2".to_string(),
                "Laptop".to_string(),
                vec![2u8; 32],
                KeyType::Ed25519,
                vec![Capability::Sign],
            )
            .unwrap();

        // Add v2 - should accept
        assert!(cache.update(did.clone(), doc_v1.clone(), source_did.clone()));
        assert_eq!(cache.get(&did).unwrap().version, 2);

        // Try to add v1 again - should reject (older)
        let x25519_secret2 = StaticSecret::random_from_rng(OsRng);
        let x25519_public2 = PublicKey::from(&x25519_secret2);
        let doc_v1_again = crate::DidDocument::new(
            did.clone(),
            keypair.verifying_key(),
            x25519_public2.as_bytes(),
        );
        assert!(!cache.update(did.clone(), doc_v1_again, source_did));
    }

    #[test]
    fn test_apply_add_device_event() {
        let cache = DidDocumentCache::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        use rand::rngs::OsRng;
        use x25519_dalek::{PublicKey, StaticSecret};
        let x25519_secret = StaticSecret::random_from_rng(OsRng);
        let x25519_public = PublicKey::from(&x25519_secret);

        let doc = crate::DidDocument::new(
            did.clone(),
            keypair.verifying_key(),
            x25519_public.as_bytes(),
        );

        cache.update_local(did.clone(), doc);

        // Create an add device event
        let event = RotationEvent {
            did: did.clone(),
            event_type: crate::RotationEventType::AddDevice {
                device_id: "device-2".to_string(),
                label: "Laptop".to_string(),
                public_key: vec![2u8; 32],
                key_type: KeyType::Ed25519,
                capabilities: vec![Capability::Sign],
            },
            proof: vec![],
            signed_by: "device-1".to_string(),
            timestamp: 1234567890,
            new_version: 2,
        };

        // Apply the event
        assert!(cache.apply_event(&did, &event).unwrap());

        // Check that document was updated
        let updated_doc = cache.get(&did).unwrap();
        assert_eq!(updated_doc.version, 2);
        assert_eq!(updated_doc.verification_method.len(), 3); // device-1 Ed25519, enc-1 X25519, device-2 Ed25519
    }
}
