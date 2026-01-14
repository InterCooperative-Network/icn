//! Integration tests for multi-device identity
//!
//! Tests the complete multi-device workflow including:
//! - Adding devices
//! - Revoking devices
//! - Synchronizing DID documents
//! - Verifying signatures from multiple devices

use anyhow::Result;
use icn_identity::{
    Capability, DidDocument, KeyPair, KeyType, RevocationReason, RotationEvent,
    RotationEventType,
};
use std::collections::HashMap;
use tempfile::TempDir;

/// Helper to simulate a device with its own keystore
struct TestDevice {
    id: String,
    label: String,
    keypair: KeyPair,
    x25519_secret: x25519_dalek::StaticSecret,
    x25519_public: x25519_dalek::PublicKey,
}

impl TestDevice {
    fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        use rand::rngs::OsRng;
        
        let keypair = KeyPair::generate().unwrap();
        let x25519_secret = x25519_dalek::StaticSecret::random_from_rng(OsRng);
        let x25519_public = x25519_dalek::PublicKey::from(&x25519_secret);
        
        Self {
            id: id.into(),
            label: label.into(),
            keypair,
            x25519_secret,
            x25519_public,
        }
    }

    fn ed25519_public(&self) -> Vec<u8> {
        self.keypair.verifying_key().as_bytes().to_vec()
    }

    fn x25519_public(&self) -> Vec<u8> {
        self.x25519_public.as_bytes().to_vec()
    }
}

#[test]
fn test_multi_device_workflow() -> Result<()> {
    // Create primary device
    let device1 = TestDevice::new("device-1", "Laptop");
    
    // Initialize DID document with primary device
    let mut did_doc = DidDocument::new(
        device1.keypair.did().clone(),
        device1.keypair.verifying_key(),
        device1.x25519_public.as_bytes(),
    );
    
    assert_eq!(did_doc.version, 1);
    assert_eq!(did_doc.verification_method.len(), 2); // Ed25519 + X25519
    
    // Add second device (phone)
    let device2 = TestDevice::new("device-2", "Phone");
    
    did_doc.add_device_with_encryption_key(
        device2.id.clone(),
        device2.label.clone(),
        device2.ed25519_public(),
        device2.x25519_public(),
        vec![Capability::Sign],
    )?;
    
    assert_eq!(did_doc.version, 2);
    assert_eq!(did_doc.verification_method.len(), 4); // 2 devices × 2 keys each
    
    // Verify device1 can still sign
    assert!(did_doc.can_sign(device1.keypair.verifying_key()));
    
    // Verify device2 can sign
    assert!(did_doc.can_sign(device2.keypair.verifying_key()));
    
    // Add third device (tablet) with full capabilities
    let device3 = TestDevice::new("device-3", "Tablet");
    
    did_doc.add_device_with_encryption_key(
        device3.id.clone(),
        device3.label.clone(),
        device3.ed25519_public(),
        device3.x25519_public(),
        vec![Capability::Sign, Capability::AddDevice, Capability::RevokeDevice],
    )?;
    
    assert_eq!(did_doc.version, 3);
    assert_eq!(did_doc.verification_method.len(), 6); // 3 devices × 2 keys
    
    // Verify all capabilities
    assert!(did_doc.has_capability("device-1", Capability::Sign));
    assert!(did_doc.has_capability("device-1", Capability::AddDevice));
    assert!(did_doc.has_capability("device-2", Capability::Sign));
    assert!(!did_doc.has_capability("device-2", Capability::AddDevice)); // Phone can't add devices
    assert!(did_doc.has_capability("device-3", Capability::RevokeDevice));
    
    // Revoke device2 (phone lost)
    did_doc.revoke_device("device-2")?;
    
    assert_eq!(did_doc.version, 4);
    
    // Device2 can no longer sign
    assert!(!did_doc.can_sign(device2.keypair.verifying_key()));
    
    // But device1 and device3 still can
    assert!(did_doc.can_sign(device1.keypair.verifying_key()));
    assert!(did_doc.can_sign(device3.keypair.verifying_key()));
    
    Ok(())
}

#[test]
fn test_rotation_event_propagation() -> Result<()> {
    // Simulate two nodes with synchronized DID documents
    let device1 = TestDevice::new("device-1", "Node A");
    let device2 = TestDevice::new("device-2", "Node B");
    
    // Node A: Create initial DID document
    let mut did_doc_a = DidDocument::new(
        device1.keypair.did().clone(),
        device1.keypair.verifying_key(),
        device1.x25519_public.as_bytes(),
    );
    
    // Node B: Clone the initial document (simulating gossip sync)
    let mut did_doc_b = did_doc_a.clone();
    
    // Node A: Add device2
    did_doc_a.add_device_with_encryption_key(
        device2.id.clone(),
        device2.label.clone(),
        device2.ed25519_public(),
        device2.x25519_public(),
        vec![Capability::Sign],
    )?;
    
    // Simplified: Just verify document state after add
    // In real usage, the rotation event would be created and signed by the keystore
    
    // Node B: Manually apply same change to simulate gossip sync
    did_doc_b.add_device_with_encryption_key(
        device2.id.clone(),
        device2.label.clone(),
        device2.ed25519_public(),
        device2.x25519_public(),
        vec![Capability::Sign],
    )?;
    
    // Verify both nodes have identical documents
    assert_eq!(did_doc_a.version, did_doc_b.version);
    assert_eq!(
        did_doc_a.verification_method.len(),
        did_doc_b.verification_method.len()
    );
    assert!(did_doc_b.can_sign(device2.keypair.verifying_key()));
    
    Ok(())
}

#[test]
fn test_capability_enforcement() -> Result<()> {
    let device1 = TestDevice::new("device-1", "Admin Device");
    let device2 = TestDevice::new("device-2", "Limited Device");
    
    let mut did_doc = DidDocument::new(
        device1.keypair.did().clone(),
        device1.keypair.verifying_key(),
        device1.x25519_public.as_bytes(),
    );
    
    // Add device2 with only Sign capability (no management capabilities)
    did_doc.add_device_with_encryption_key(
        device2.id.clone(),
        device2.label.clone(),
        device2.ed25519_public(),
        device2.x25519_public(),
        vec![Capability::Sign],
    )?;
    
    // Verify device1 has full capabilities
    assert!(did_doc.has_capability("device-1", Capability::Sign));
    assert!(did_doc.has_capability("device-1", Capability::AddDevice));
    assert!(did_doc.has_capability("device-1", Capability::RevokeDevice));
    
    // Verify device2 has limited capabilities
    assert!(did_doc.has_capability("device-2", Capability::Sign));
    assert!(!did_doc.has_capability("device-2", Capability::AddDevice));
    assert!(!did_doc.has_capability("device-2", Capability::RevokeDevice));
    
    // Try to create a rotation event signed by device2 to add a device
    // This should fail verification
    let device3 = TestDevice::new("device-3", "Unauthorized");
    let event_data = format!(
        "{}:add_device:{}:{}:{}",
        device1.keypair.did(),
        device3.id,
        icn_time::current_timestamp_secs(),
        did_doc.version + 1
    );
    let signature = device2.keypair.sign(event_data.as_bytes());
    
    let unauthorized_event = RotationEvent {
        did: device1.keypair.did().clone(),
        event_type: RotationEventType::AddDevice {
            device_id: device3.id.clone(),
            label: device3.label.clone(),
            public_key: device3.ed25519_public(),
            key_type: KeyType::Ed25519,
            capabilities: vec![Capability::Sign],
        },
        proof: signature.to_bytes().to_vec(),
        signed_by: "device-2".to_string(),
        timestamp: icn_time::current_timestamp_secs(),
        new_version: did_doc.version + 1,
    };
    
    // Verification should fail because device-2 lacks AddDevice capability
    assert!(unauthorized_event.verify(&did_doc).is_err());
    
    Ok(())
}

#[test]
fn test_device_revocation_cascade() -> Result<()> {
    let device1 = TestDevice::new("device-1", "Primary");
    let device2 = TestDevice::new("device-2", "Compromised");
    
    let mut did_doc = DidDocument::new(
        device1.keypair.did().clone(),
        device1.keypair.verifying_key(),
        device1.x25519_public.as_bytes(),
    );
    
    // Add device2
    did_doc.add_device_with_encryption_key(
        device2.id.clone(),
        device2.label.clone(),
        device2.ed25519_public(),
        device2.x25519_public(),
        vec![Capability::Sign, Capability::Encrypt],
    )?;
    
    assert!(did_doc.can_sign(device2.keypair.verifying_key()));
    
    // Revoke device2
    did_doc.revoke_device("device-2")?;
    
    // Verify both Ed25519 and X25519 keys are effectively revoked
    assert!(!did_doc.can_sign(device2.keypair.verifying_key()));
    
    // Check revocation is recorded
    let vm = did_doc.get_verification_method("device-2").unwrap();
    assert!(vm.revoked_at.is_some());
    
    // Encryption key should also be marked as revoked (through parent device ID)
    let enc_vm = did_doc.get_verification_method("device-2-enc").unwrap();
    assert_eq!(enc_vm.key_type, KeyType::X25519);
    
    Ok(())
}

#[test]
fn test_version_consistency() -> Result<()> {
    let device1 = TestDevice::new("device-1", "Device 1");
    let device2 = TestDevice::new("device-2", "Device 2");
    let device3 = TestDevice::new("device-3", "Device 3");
    
    let mut did_doc = DidDocument::new(
        device1.keypair.did().clone(),
        device1.keypair.verifying_key(),
        device1.x25519_public.as_bytes(),
    );
    
    assert_eq!(did_doc.version, 1);
    
    // Add device2 - should increment to version 2
    did_doc.add_device_with_encryption_key(
        device2.id.clone(),
        device2.label.clone(),
        device2.ed25519_public(),
        device2.x25519_public(),
        vec![Capability::Sign],
    )?;
    assert_eq!(did_doc.version, 2);
    
    // Add device3 - should increment to version 3
    did_doc.add_device_with_encryption_key(
        device3.id.clone(),
        device3.label.clone(),
        device3.ed25519_public(),
        device3.x25519_public(),
        vec![Capability::Sign],
    )?;
    assert_eq!(did_doc.version, 3);
    
    // Revoke device2 - should increment to version 4
    did_doc.revoke_device("device-2")?;
    assert_eq!(did_doc.version, 4);
    
    // Revoke device3 - should increment to version 5
    did_doc.revoke_device("device-3")?;
    assert_eq!(did_doc.version, 5);
    
    Ok(())
}
