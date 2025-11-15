//! Multi-device identity support for ICN
//!
//! Implements DID Document v2 with support for:
//! - Multiple verification methods (keys) per DID
//! - Key rotation and device management
//! - Capability-based permissions
//! - Social recovery

use crate::Did;
use anyhow::{bail, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// DID Document version 2: Multi-device identity
///
/// A DID Document describes all authorized keys for a single DID.
/// It enables multi-device usage, key rotation, and recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DidDocument {
    /// The DID this document describes (did:icn:base58-pubkey)
    /// This is derived from the *original* keypair and remains stable across rotations
    pub id: Did,

    /// Version number (incremented on each update)
    pub version: u64,

    /// Timestamp of last update (Unix timestamp in seconds)
    pub updated_at: u64,

    /// List of authorized verification methods (keys)
    pub verification_method: Vec<VerificationMethod>,

    /// Which keys can authenticate (sign messages)
    /// References to verification_method IDs
    pub authentication: Vec<String>,

    /// Optional recovery configuration
    pub recovery: Option<RecoveryConfig>,
}

/// A verification method represents a key that can be used with this DID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationMethod {
    /// Unique ID within this DID Document (e.g., "device-1", "device-2")
    pub id: String,

    /// Human-readable label (e.g., "Matt's Laptop", "Phone")
    pub label: String,

    /// Key type
    pub key_type: KeyType,

    /// The actual public key (serialized as bytes)
    pub public_key: Vec<u8>,

    /// What this key is authorized to do
    pub capabilities: Vec<Capability>,

    /// When this key was added (Unix timestamp in seconds)
    pub added_at: u64,

    /// Optional: When this key was revoked (Unix timestamp in seconds)
    pub revoked_at: Option<u64>,
}

/// Type of cryptographic key
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyType {
    /// Ed25519 signing key
    Ed25519,

    /// X25519 encryption key
    X25519,
}

/// Capabilities define what a key is authorized to do
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Can sign messages on behalf of this DID
    Sign,

    /// Can add new devices to this DID
    AddDevice,

    /// Can revoke devices (including self)
    RevokeDevice,

    /// Can rotate keys
    RotateKey,

    /// Can participate in recovery
    Recover,

    /// Can encrypt/decrypt messages
    Encrypt,
}

/// Recovery configuration for a DID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryConfig {
    /// Recovery method
    pub method: RecoveryMethod,

    /// Threshold for recovery (how many signatures needed)
    pub threshold: u8,

    /// List of trustee DIDs who can participate in recovery
    pub trustees: Vec<Did>,

    /// Delay period in seconds before recovery can be finalized
    /// This gives the original owner time to cancel fraudulent recovery attempts
    pub delay_period: u64,
}

/// Method of recovery
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryMethod {
    /// M-of-N social recovery
    Social {
        /// Minimum number of trustees required
        m: u8,
        /// Total number of trustees
        n: u8,
    },

    /// Encrypted backup seed (offline)
    BackupSeed,

    /// No recovery (accept total loss risk)
    None,
}

/// Represents a key rotation or device change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEvent {
    /// The DID being rotated
    pub did: Did,

    /// Event type
    pub event_type: RotationEventType,

    /// Signature by the authorized key performing this action
    pub proof: Vec<u8>, // Serialized Signature

    /// Which key signed this (verification method ID)
    /// Must have appropriate capability
    pub signed_by: String,

    /// Timestamp (Unix timestamp in seconds)
    pub timestamp: u64,

    /// New DID Document version after this event
    pub new_version: u64,
}

/// Types of rotation events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationEventType {
    /// Add a new device to this DID
    AddDevice {
        /// Device identifier
        device_id: String,
        /// Device label
        label: String,
        /// Public key (serialized)
        public_key: Vec<u8>,
        /// Key type
        key_type: KeyType,
        /// Capabilities granted to this device
        capabilities: Vec<Capability>,
    },

    /// Add a new device with both signing and encryption keys
    ///
    /// This represents the complete addition of a device that has both
    /// Ed25519 (signing) and X25519 (encryption) keys. Using this variant
    /// ensures that remote nodes applying this event will have the same
    /// DID Document state as the local node that created the event.
    AddDeviceWithEncryption {
        /// Device identifier (Ed25519 key will use this ID, X25519 will be {device_id}-enc)
        device_id: String,
        /// Device label
        label: String,
        /// Ed25519 public key for signing (serialized)
        ed25519_public_key: Vec<u8>,
        /// X25519 public key for encryption (serialized)
        x25519_public_key: Vec<u8>,
        /// Capabilities granted to the signing key
        signing_capabilities: Vec<Capability>,
    },

    /// Revoke a device
    RevokeDevice {
        /// Device identifier to revoke
        device_id: String,
        /// Reason for revocation
        reason: RevocationReason,
    },

    /// Rotate a key (change the underlying keypair for a device)
    RotateKey {
        /// Device identifier
        device_id: String,
        /// Old public key (for verification)
        old_key: Vec<u8>,
        /// New public key
        new_key: Vec<u8>,
    },

    /// Full recovery (new root key after total loss)
    Recover {
        /// New root public key
        new_root_key: Vec<u8>,
        /// Recovery proofs from trustees
        recovery_proofs: Vec<RecoveryProof>,
    },
}

/// Reason for device revocation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RevocationReason {
    /// Normal removal
    Removed,

    /// Device compromised
    Compromised,

    /// Device lost/stolen
    Lost,

    /// Key rotated to new keypair
    Rotated,
}

/// Proof from a recovery trustee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryProof {
    /// Trustee DID
    pub trustee: Did,

    /// Trustee's signature on recovery request
    pub signature: Vec<u8>, // Serialized Signature

    /// Timestamp when trustee signed
    pub timestamp: u64,
}

impl DidDocument {
    /// Create a new DID Document for a fresh identity
    pub fn new(did: Did, initial_key: &VerifyingKey, initial_x25519_key: &[u8; 32]) -> Self {
        let now = current_timestamp();

        DidDocument {
            id: did,
            version: 1,
            updated_at: now,
            verification_method: vec![
                VerificationMethod {
                    id: "device-1".to_string(),
                    label: "Primary Device".to_string(),
                    key_type: KeyType::Ed25519,
                    public_key: initial_key.as_bytes().to_vec(),
                    capabilities: vec![
                        Capability::Sign,
                        Capability::AddDevice,
                        Capability::RevokeDevice,
                        Capability::RotateKey,
                        Capability::Recover,
                    ],
                    added_at: now,
                    revoked_at: None,
                },
                VerificationMethod {
                    id: "device-1-enc".to_string(),
                    label: "Primary Device (encryption)".to_string(),
                    key_type: KeyType::X25519,
                    public_key: initial_x25519_key.to_vec(),
                    capabilities: vec![Capability::Encrypt],
                    added_at: now,
                    revoked_at: None,
                },
            ],
            authentication: vec!["device-1".to_string()],
            recovery: None, // User can configure later
        }
    }

    /// Check if a verification method has a specific capability
    pub fn has_capability(&self, method_id: &str, capability: Capability) -> bool {
        self.verification_method
            .iter()
            .find(|vm| vm.id == method_id)
            .map(|vm| {
                // Must not be revoked
                vm.revoked_at.is_none() && vm.capabilities.contains(&capability)
            })
            .unwrap_or(false)
    }

    /// Get a verification method by ID
    pub fn get_verification_method(&self, method_id: &str) -> Option<&VerificationMethod> {
        self.verification_method
            .iter()
            .find(|vm| vm.id == method_id)
    }

    /// Check if a public key is authorized to sign for this DID
    pub fn can_sign(&self, public_key: &VerifyingKey) -> bool {
        let key_bytes = public_key.as_bytes();

        self.verification_method.iter().any(|vm| {
            vm.key_type == KeyType::Ed25519
                && vm.public_key == key_bytes
                && vm.revoked_at.is_none()
                && vm.capabilities.contains(&Capability::Sign)
        })
    }

    /// Add a new device to this DID Document
    pub fn add_device(
        &mut self,
        device_id: String,
        label: String,
        public_key: Vec<u8>,
        key_type: KeyType,
        capabilities: Vec<Capability>,
    ) -> Result<()> {
        // Check device ID doesn't already exist
        if self.get_verification_method(&device_id).is_some() {
            bail!("Device ID '{}' already exists", device_id);
        }

        let now = current_timestamp();

        // Check if this will be a signing key before moving capabilities
        let is_signing_key = capabilities.contains(&Capability::Sign);

        self.verification_method.push(VerificationMethod {
            id: device_id.clone(),
            label,
            key_type,
            public_key,
            capabilities,
            added_at: now,
            revoked_at: None,
        });

        // If it's a signing key, add to authentication list
        if is_signing_key {
            self.authentication.push(device_id);
        }

        // Update version and timestamp
        self.version += 1;
        self.updated_at = now;

        Ok(())
    }

    /// Add a device with both signing (Ed25519) and encryption (X25519) keys
    ///
    /// This is a convenience method for adding a complete device that has both
    /// signing and encryption capabilities. It adds two VerificationMethods
    /// (one for each key type) but only increments the version once, since
    /// this represents a single logical operation.
    ///
    /// The signing key will have the specified capabilities.
    /// The encryption key will automatically get only the Encrypt capability.
    pub fn add_device_with_encryption_key(
        &mut self,
        device_id: String,
        label: String,
        ed25519_public_key: Vec<u8>,
        x25519_public_key: Vec<u8>,
        signing_capabilities: Vec<Capability>,
    ) -> Result<()> {
        // Check device ID doesn't already exist (for either key)
        if self.get_verification_method(&device_id).is_some() {
            bail!("Device ID '{}' already exists", device_id);
        }
        let enc_id = format!("{}-enc", device_id);
        if self.get_verification_method(&enc_id).is_some() {
            bail!("Encryption key ID '{}' already exists", enc_id);
        }

        let now = current_timestamp();

        // Check if this will be a signing key
        let is_signing_key = signing_capabilities.contains(&Capability::Sign);

        // Add Ed25519 signing key
        self.verification_method.push(VerificationMethod {
            id: device_id.clone(),
            label: label.clone(),
            key_type: KeyType::Ed25519,
            public_key: ed25519_public_key,
            capabilities: signing_capabilities,
            added_at: now,
            revoked_at: None,
        });

        // Add X25519 encryption key
        self.verification_method.push(VerificationMethod {
            id: enc_id,
            label: format!("{} (encryption)", label),
            key_type: KeyType::X25519,
            public_key: x25519_public_key,
            capabilities: vec![Capability::Encrypt],
            added_at: now,
            revoked_at: None,
        });

        // If it's a signing key, add to authentication list
        if is_signing_key {
            self.authentication.push(device_id);
        }

        // Update version and timestamp ONCE for the logical operation
        self.version += 1;
        self.updated_at = now;

        Ok(())
    }

    /// Revoke a device
    pub fn revoke_device(&mut self, device_id: &str) -> Result<()> {
        let now = current_timestamp();

        // Find the verification method
        let vm = self
            .verification_method
            .iter_mut()
            .find(|vm| vm.id == device_id)
            .ok_or_else(|| anyhow::anyhow!("Device '{}' not found", device_id))?;

        // Mark as revoked
        vm.revoked_at = Some(now);

        // Remove from authentication list
        self.authentication.retain(|id| id != device_id);

        // Update version and timestamp
        self.version += 1;
        self.updated_at = now;

        Ok(())
    }

    /// Rotate a key for a device
    pub fn rotate_key(
        &mut self,
        device_id: &str,
        old_key: &[u8],
        new_key: Vec<u8>,
    ) -> Result<()> {
        let now = current_timestamp();

        // Find the verification method
        let vm = self
            .verification_method
            .iter_mut()
            .find(|vm| vm.id == device_id)
            .ok_or_else(|| anyhow::anyhow!("Device '{}' not found", device_id))?;

        // Verify old key matches
        if vm.public_key != old_key {
            bail!("Old key does not match current key for device '{}'", device_id);
        }

        // Update to new key
        vm.public_key = new_key;

        // Update version and timestamp
        self.version += 1;
        self.updated_at = now;

        Ok(())
    }
}

impl RotationEvent {
    /// Create a new AddDevice event
    pub fn add_device(
        did: Did,
        device_id: String,
        label: String,
        public_key: Vec<u8>,
        key_type: KeyType,
        capabilities: Vec<Capability>,
        signed_by: String,
        proof: Signature,
        new_version: u64,
    ) -> Self {
        RotationEvent {
            did,
            event_type: RotationEventType::AddDevice {
                device_id,
                label,
                public_key,
                key_type,
                capabilities,
            },
            proof: proof.to_vec(),
            signed_by,
            timestamp: current_timestamp(),
            new_version,
        }
    }

    /// Create a new RevokeDevice event
    pub fn revoke_device(
        did: Did,
        device_id: String,
        reason: RevocationReason,
        signed_by: String,
        proof: Signature,
        new_version: u64,
    ) -> Self {
        RotationEvent {
            did,
            event_type: RotationEventType::RevokeDevice { device_id, reason },
            proof: proof.to_vec(),
            signed_by,
            timestamp: current_timestamp(),
            new_version,
        }
    }

    /// Verify this rotation event is properly signed
    pub fn verify(&self, did_doc: &DidDocument) -> Result<()> {
        // Check DID matches
        if self.did != did_doc.id {
            bail!("Rotation event DID does not match document");
        }

        // Check version increment
        if self.new_version != did_doc.version + 1 {
            bail!(
                "Invalid version increment: expected {}, got {}",
                did_doc.version + 1,
                self.new_version
            );
        }

        // Check signer has appropriate capability
        let required_cap = match &self.event_type {
            RotationEventType::AddDevice { .. } => Capability::AddDevice,
            RotationEventType::AddDeviceWithEncryption { .. } => Capability::AddDevice,
            RotationEventType::RevokeDevice { .. } => Capability::RevokeDevice,
            RotationEventType::RotateKey { .. } => Capability::RotateKey,
            RotationEventType::Recover { .. } => Capability::Recover,
        };

        if !did_doc.has_capability(&self.signed_by, required_cap) {
            bail!(
                "Signer '{}' does not have capability {:?}",
                self.signed_by,
                required_cap
            );
        }

        // Get the signing key
        let vm = did_doc
            .get_verification_method(&self.signed_by)
            .ok_or_else(|| anyhow::anyhow!("Signer '{}' not found", self.signed_by))?;

        if vm.key_type != KeyType::Ed25519 {
            bail!("Signer must use Ed25519 key");
        }

        // Convert public key
        let public_key_bytes: [u8; 32] = vm
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid public key length"))?;
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)?;

        // Verify signature
        let signature: Signature = self
            .proof
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature format"))?;

        // Sign the event payload (excluding the proof itself)
        let message = self.signing_message()?;

        use ed25519_dalek::Verifier;
        verifying_key
            .verify(&message, &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))?;

        Ok(())
    }

    /// Get the message that should be signed for this event
    fn signing_message(&self) -> Result<Vec<u8>> {
        // Serialize event without the proof
        let mut event_copy = self.clone();
        event_copy.proof = vec![]; // Clear proof for signing

        bincode::serialize(&event_copy)
            .map_err(|e| anyhow::anyhow!("Failed to serialize event: {}", e))
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KeyPair;

    #[test]
    fn test_create_did_document() {
        let kp = KeyPair::generate().unwrap();
        let x25519_key = [0u8; 32]; // Dummy encryption key

        let doc = DidDocument::new(kp.did().clone(), kp.verifying_key(), &x25519_key);

        assert_eq!(doc.version, 1);
        assert_eq!(doc.verification_method.len(), 2); // Ed25519 + X25519
        assert_eq!(doc.authentication.len(), 1);
        assert!(doc.can_sign(kp.verifying_key()));
    }

    #[test]
    fn test_add_device() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let x25519_key = [0u8; 32];

        let mut doc = DidDocument::new(kp1.did().clone(), kp1.verifying_key(), &x25519_key);

        // Add second device
        doc.add_device(
            "device-2".to_string(),
            "Phone".to_string(),
            kp2.verifying_key().as_bytes().to_vec(),
            KeyType::Ed25519,
            vec![Capability::Sign],
        )
        .unwrap();

        assert_eq!(doc.version, 2);
        assert_eq!(doc.verification_method.len(), 3); // device-1, enc-1, device-2
        assert!(doc.can_sign(kp2.verifying_key()));
    }

    #[test]
    fn test_revoke_device() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let x25519_key = [0u8; 32];

        let mut doc = DidDocument::new(kp1.did().clone(), kp1.verifying_key(), &x25519_key);

        // Add second device
        doc.add_device(
            "device-2".to_string(),
            "Phone".to_string(),
            kp2.verifying_key().as_bytes().to_vec(),
            KeyType::Ed25519,
            vec![Capability::Sign],
        )
        .unwrap();

        // Revoke it
        doc.revoke_device("device-2").unwrap();

        assert_eq!(doc.version, 3);
        assert!(!doc.can_sign(kp2.verifying_key())); // Can't sign anymore
    }

    #[test]
    fn test_capability_check() {
        let kp = KeyPair::generate().unwrap();
        let x25519_key = [0u8; 32];

        let doc = DidDocument::new(kp.did().clone(), kp.verifying_key(), &x25519_key);

        assert!(doc.has_capability("device-1", Capability::Sign));
        assert!(doc.has_capability("device-1", Capability::AddDevice));
        assert!(doc.has_capability("device-1-enc", Capability::Encrypt));
        assert!(!doc.has_capability("device-1-enc", Capability::Sign)); // Encryption key can't sign
        assert!(!doc.has_capability("nonexistent", Capability::Sign));
    }

    #[test]
    fn test_rotation_event_verification() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let x25519_key = [0u8; 32];

        let doc = DidDocument::new(kp1.did().clone(), kp1.verifying_key(), &x25519_key);

        // Create AddDevice event
        let event = RotationEvent::add_device(
            kp1.did().clone(),
            "device-2".to_string(),
            "Phone".to_string(),
            kp2.verifying_key().as_bytes().to_vec(),
            KeyType::Ed25519,
            vec![Capability::Sign],
            "device-1".to_string(),
            kp1.sign(&RotationEvent::add_device(
                kp1.did().clone(),
                "device-2".to_string(),
                "Phone".to_string(),
                kp2.verifying_key().as_bytes().to_vec(),
                KeyType::Ed25519,
                vec![Capability::Sign],
                "device-1".to_string(),
                Signature::from([0u8; 64]), // Dummy for message construction
                2,
            )
            .signing_message()
            .unwrap()),
            2,
        );

        // Verify the event
        assert!(event.verify(&doc).is_ok());
    }

    #[test]
    fn test_add_device_with_encryption_key_version_increment() {
        let kp = crate::KeyPair::generate().unwrap();
        let initial_x25519_key = [0u8; 32];
        let mut doc = DidDocument::new(kp.did().clone(), kp.verifying_key(), &initial_x25519_key);

        // Initial version should be 1, with 2 keys (device-1 Ed25519 + device-1-enc X25519)
        assert_eq!(doc.version, 1);
        assert_eq!(doc.verification_method.len(), 2);

        // Add second device with both Ed25519 and X25519 keys
        let ed25519_key = vec![1u8; 32];
        let x25519_key = vec![2u8; 32];

        doc.add_device_with_encryption_key(
            "device-2".to_string(),
            "Test Device".to_string(),
            ed25519_key,
            x25519_key,
            vec![Capability::Sign, Capability::AddDevice],
        )
        .unwrap();

        // Version should increment by exactly 1, even though 2 keys were added
        assert_eq!(doc.version, 2);

        // Verify both keys were added (2 from initial + 2 from new device = 4 total)
        assert_eq!(doc.verification_method.len(), 4);

        // Verify signing key
        let signing_vm = doc.get_verification_method("device-2").unwrap();
        assert_eq!(signing_vm.label, "Test Device");
        assert_eq!(signing_vm.key_type, KeyType::Ed25519);
        assert!(signing_vm.capabilities.contains(&Capability::Sign));

        // Verify encryption key
        let enc_vm = doc.get_verification_method("device-2-enc").unwrap();
        assert_eq!(enc_vm.label, "Test Device (encryption)");
        assert_eq!(enc_vm.key_type, KeyType::X25519);
        assert_eq!(enc_vm.capabilities, vec![Capability::Encrypt]);

        // Verify signing key is in authentication list
        assert!(doc.authentication.contains(&"device-2".to_string()));
    }
}
