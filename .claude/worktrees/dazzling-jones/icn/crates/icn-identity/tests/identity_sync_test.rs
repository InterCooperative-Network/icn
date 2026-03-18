//! Integration test for identity synchronization protocol
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::{
    AgeKeyStore, Capability, DidDocumentCache, IdentityUpdateMessage, KeyPair, KeyStore, KeyType,
    RotationEvent, RotationEventType, IDENTITY_UPDATES_TOPIC,
};
use tempfile::tempdir;

#[test]
fn test_identity_sync_add_device_workflow() {
    // Simulate two nodes: Alice (primary device) and Bob (peer receiving updates)

    // === Alice's Setup ===
    let alice_dir = tempdir().unwrap();
    let alice_keystore_path = alice_dir.path().join("alice.age");
    let alice_passphrase = b"alice-password";

    // Alice creates her identity
    let mut alice_ks = AgeKeyStore::init(&alice_keystore_path, alice_passphrase).unwrap();
    let alice_did = alice_ks.get_keypair().unwrap().did().clone();
    let alice_doc_v1 = alice_ks.get_did_document().unwrap().clone();

    println!("Alice's DID: {alice_did}");
    println!(
        "Alice's initial DID Document version: {}",
        alice_doc_v1.version
    );

    // === Bob's Setup ===
    // Bob creates his own identity and cache
    let bob_dir = tempdir().unwrap();
    let bob_keystore_path = bob_dir.path().join("bob.age");
    let bob_passphrase = b"bob-password";

    let bob_ks = AgeKeyStore::init(&bob_keystore_path, bob_passphrase).unwrap();
    let bob_did = bob_ks.get_keypair().unwrap().did().clone();

    // Bob maintains a cache of DID Documents for identity verification
    let bob_cache = DidDocumentCache::new();

    // Bob adds Alice's initial DID Document to his cache
    bob_cache.update(alice_did.clone(), alice_doc_v1.clone(), bob_did.clone());
    println!("Bob cached Alice's DID Document v{}", alice_doc_v1.version);

    // === Alice adds a new device ===
    println!("\n=== Alice adds a new device ===");

    // Generate keys for the new device
    let device2_keypair = KeyPair::generate().unwrap();
    let device2_ed25519_pubkey = device2_keypair.verifying_key().as_bytes().to_vec();

    // Create rotation event
    let add_device_event = RotationEvent {
        did: alice_did.clone(),
        event_type: RotationEventType::AddDevice {
            device_id: "device-2".to_string(),
            label: "Alice's Laptop".to_string(),
            public_key: device2_ed25519_pubkey.clone(),
            key_type: KeyType::Ed25519,
            capabilities: vec![Capability::Sign, Capability::Encrypt],
        },
        proof: vec![], // In production, this would be signed
        signed_by: "device-1".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        new_version: 2,
    };

    // Alice updates her local DID Document
    alice_ks
        .update_did_document(
            |did_doc| {
                did_doc.add_device(
                    "device-2".to_string(),
                    "Alice's Laptop".to_string(),
                    device2_ed25519_pubkey.clone(),
                    KeyType::Ed25519,
                    vec![Capability::Sign, Capability::Encrypt],
                )
            },
            Some(add_device_event.clone()),
            alice_passphrase,
        )
        .unwrap();

    let alice_doc_v2 = alice_ks.get_did_document().unwrap();
    println!(
        "Alice updated her DID Document to version {}",
        alice_doc_v2.version
    );

    // === Alice broadcasts the update via gossip ===
    println!("\n=== Alice broadcasts identity update ===");

    let update_msg = IdentityUpdateMessage::new(
        alice_did.clone(),
        add_device_event.clone(),
        2,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    );

    // Serialize for gossip
    let update_bytes = update_msg.to_bytes().unwrap();
    println!(
        "Serialized identity update: {} bytes (topic: {})",
        update_bytes.len(),
        IDENTITY_UPDATES_TOPIC
    );

    // === Bob receives the update ===
    println!("\n=== Bob receives identity update ===");

    // Deserialize
    let received_msg = IdentityUpdateMessage::from_bytes(&update_bytes).unwrap();
    println!("Bob received update for DID: {}", received_msg.did);
    println!("Event type: AddDevice");

    // Apply the rotation event to Bob's cached DID Document
    let applied = bob_cache
        .apply_event(&alice_did, &received_msg.event)
        .unwrap();
    assert!(applied, "Event should be applied successfully");

    println!("Bob applied rotation event to cached DID Document");

    // Verify Bob's cache is updated
    let alice_doc_in_bobs_cache = bob_cache.get(&alice_did).unwrap();
    assert_eq!(alice_doc_in_bobs_cache.version, 2);
    assert_eq!(alice_doc_in_bobs_cache.verification_method.len(), 3); // device-1 Ed25519, enc-1 X25519, device-2 Ed25519

    println!(
        "Bob's cached version of Alice's DID Document: v{}",
        alice_doc_in_bobs_cache.version
    );

    // === Verify message authentication ===
    println!("\n=== Testing message verification ===");

    // Bob can now verify that device-2's public key is authorized to sign for Alice
    assert!(
        bob_cache.can_sign(&alice_did, &device2_ed25519_pubkey),
        "Device-2 should be authorized to sign after update"
    );

    println!("✓ Bob verified that device-2 can sign for Alice's DID");

    // === Alice revokes a device ===
    println!("\n=== Alice revokes device-2 ===");

    let revoke_event = RotationEvent {
        did: alice_did.clone(),
        event_type: RotationEventType::RevokeDevice {
            device_id: "device-2".to_string(),
            reason: icn_identity::RevocationReason::Compromised,
        },
        proof: vec![],
        signed_by: "device-1".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        new_version: 3,
    };

    // Alice updates locally
    alice_ks
        .update_did_document(
            |did_doc| did_doc.revoke_device("device-2"),
            Some(revoke_event.clone()),
            alice_passphrase,
        )
        .unwrap();

    let alice_doc_v3 = alice_ks.get_did_document().unwrap();
    assert_eq!(alice_doc_v3.version, 3);

    // Broadcast revocation
    let revoke_msg = IdentityUpdateMessage::new(
        alice_did.clone(),
        revoke_event,
        3,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    );

    let revoke_bytes = revoke_msg.to_bytes().unwrap();

    // Bob receives and applies
    let received_revoke = IdentityUpdateMessage::from_bytes(&revoke_bytes).unwrap();
    bob_cache
        .apply_event(&alice_did, &received_revoke.event)
        .unwrap();

    // Verify device-2 can no longer sign
    assert!(
        !bob_cache.can_sign(&alice_did, &device2_ed25519_pubkey),
        "Device-2 should NOT be authorized after revocation"
    );

    println!("✓ Bob verified that device-2 can no longer sign for Alice's DID");

    let final_doc = bob_cache.get(&alice_did).unwrap();
    println!(
        "\nFinal state: Alice's DID Document v{} with {} devices ({} active)",
        final_doc.version,
        final_doc.verification_method.len() / 2,
        final_doc
            .verification_method
            .iter()
            .filter(|vm| vm.revoked_at.is_none() && vm.key_type == KeyType::Ed25519)
            .count()
    );
}

#[test]
fn test_cache_rejects_older_versions() {
    let cache = DidDocumentCache::new();
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let source_did = KeyPair::generate().unwrap().did().clone();

    use rand::rngs::OsRng;
    use x25519_dalek::{PublicKey, StaticSecret};

    let x25519_secret = StaticSecret::random_from_rng(OsRng);
    let x25519_public = PublicKey::from(&x25519_secret);

    let mut doc_v1 = icn_identity::DidDocument::new(
        did.clone(),
        keypair.verifying_key(),
        x25519_public.as_bytes(),
    );

    // Add v1
    cache.update(did.clone(), doc_v1.clone(), source_did.clone());

    // Create v2
    doc_v1
        .add_device(
            "device-2".to_string(),
            "Test Device".to_string(),
            vec![1u8; 32],
            KeyType::Ed25519,
            vec![Capability::Sign],
        )
        .unwrap();

    // Add v2
    cache.update(did.clone(), doc_v1.clone(), source_did.clone());

    // Create v1 again (older)
    let x25519_secret2 = StaticSecret::random_from_rng(OsRng);
    let x25519_public2 = PublicKey::from(&x25519_secret2);
    let doc_v1_again = icn_identity::DidDocument::new(
        did.clone(),
        keypair.verifying_key(),
        x25519_public2.as_bytes(),
    );

    // Should reject older version
    let accepted = cache.update(did.clone(), doc_v1_again, source_did);
    assert!(!accepted, "Should reject older version");

    // Should still have v2
    let cached = cache.get(&did).unwrap();
    assert_eq!(cached.version, 2);
}
