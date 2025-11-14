//! Integration test for end-to-end encrypted messages
//!
//! Tests the complete flow:
//! 1. Alice encrypts a message for Bob using X25519
//! 2. Alice signs the encrypted envelope
//! 3. Bob receives and verifies the signature
//! 4. Bob decrypts the message
//!
//! This demonstrates Phase 10 encryption working with Phase 9 authentication.

use icn_identity::{IdentityBundle, KeyPair};
use icn_net::{EncryptedEnvelope, PayloadType, SignedEnvelope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SecretMessage {
    content: String,
    timestamp: u64,
}

#[test]
fn test_encrypt_sign_decrypt_flow() {
    // Generate identity bundles for Alice and Bob
    let alice_bundle = IdentityBundle::generate().unwrap();
    let bob_bundle = IdentityBundle::generate().unwrap();

    // Create a secret message
    let message = SecretMessage {
        content: "Meet me at the lighthouse at midnight".to_string(),
        timestamp: 1234567890,
    };

    // ========== ALICE SIDE: ENCRYPT AND SIGN ==========

    // 1. Serialize the message
    let plaintext = bincode::serialize(&message).unwrap();

    // 2. Encrypt for Bob using X25519 keys
    let sequence = 42;
    let encrypted_envelope = EncryptedEnvelope::encrypt(
        alice_bundle.did(),
        bob_bundle.did(),
        sequence,
        &alice_bundle.x25519_secret(),
        &bob_bundle.x25519_public(),
        &plaintext,
    )
    .unwrap();

    assert_eq!(encrypted_envelope.from, *alice_bundle.did());
    assert_eq!(encrypted_envelope.to, *bob_bundle.did());
    assert_eq!(encrypted_envelope.sequence, sequence);

    // 3. Serialize the encrypted envelope
    let encrypted_bytes = bincode::serialize(&encrypted_envelope).unwrap();

    // 4. Sign the encrypted envelope
    let signed_envelope = SignedEnvelope::new(
        alice_bundle.did(),
        alice_bundle.keypair(),
        sequence,
        PayloadType::Encrypted,
        encrypted_bytes.clone(),
    )
    .unwrap();

    // ========== TRANSMISSION (simulated) ==========
    // In a real scenario, signed_envelope would be sent over the network
    // wrapped in a NetworkMessage with MessagePayload::Signed(signed_envelope)

    // ========== BOB SIDE: VERIFY AND DECRYPT ==========

    // 1. Verify the signature (300 second max age)
    signed_envelope.verify(300).unwrap();

    // Verify it's from Alice
    assert_eq!(signed_envelope.from, *alice_bundle.did());

    // Verify it's an encrypted payload
    assert_eq!(signed_envelope.payload_type, PayloadType::Encrypted);

    // 2. Deserialize the encrypted envelope
    let received_encrypted: EncryptedEnvelope =
        bincode::deserialize(&signed_envelope.payload).unwrap();

    // Verify envelope metadata
    assert_eq!(received_encrypted.from, *alice_bundle.did());
    assert_eq!(received_encrypted.to, *bob_bundle.did());

    // 3. Decrypt using Bob's X25519 secret key
    let decrypted_bytes = received_encrypted
        .decrypt(&bob_bundle.x25519_secret(), &alice_bundle.x25519_public())
        .unwrap();

    // 4. Deserialize the plaintext message
    let decrypted_message: SecretMessage = bincode::deserialize(&decrypted_bytes).unwrap();

    // ========== VERIFICATION ==========
    assert_eq!(decrypted_message, message);
    assert_eq!(decrypted_message.content, "Meet me at the lighthouse at midnight");
}

#[test]
fn test_wrong_recipient_cannot_decrypt() {
    // Generate three identity bundles
    let alice_bundle = IdentityBundle::generate().unwrap();
    let bob_bundle = IdentityBundle::generate().unwrap();
    let charlie_bundle = IdentityBundle::generate().unwrap();

    let message = SecretMessage {
        content: "Secret for Bob only".to_string(),
        timestamp: 1234567890,
    };

    // Alice encrypts for Bob
    let plaintext = bincode::serialize(&message).unwrap();
    let encrypted_envelope = EncryptedEnvelope::encrypt(
        alice_bundle.did(),
        bob_bundle.did(),
        1,
        &alice_bundle.x25519_secret(),
        &bob_bundle.x25519_public(),
        &plaintext,
    )
    .unwrap();

    // Charlie tries to decrypt (should fail)
    let result = encrypted_envelope.decrypt(
        &charlie_bundle.x25519_secret(),
        &alice_bundle.x25519_public(),
    );

    assert!(result.is_err(), "Charlie should not be able to decrypt Bob's message");
}

#[test]
fn test_tampering_detected_after_encryption() {
    let alice_bundle = IdentityBundle::generate().unwrap();
    let bob_bundle = IdentityBundle::generate().unwrap();

    let message = SecretMessage {
        content: "Original message".to_string(),
        timestamp: 1234567890,
    };

    // Encrypt the message
    let plaintext = bincode::serialize(&message).unwrap();
    let mut encrypted_envelope = EncryptedEnvelope::encrypt(
        alice_bundle.did(),
        bob_bundle.did(),
        1,
        &alice_bundle.x25519_secret(),
        &bob_bundle.x25519_public(),
        &plaintext,
    )
    .unwrap();

    // Tamper with the ciphertext
    encrypted_envelope.ciphertext[0] ^= 0xFF;

    // Decryption should fail (Poly1305 authentication fails)
    let result = encrypted_envelope.decrypt(
        &bob_bundle.x25519_secret(),
        &alice_bundle.x25519_public(),
    );

    assert!(result.is_err(), "Tampering should be detected");
}

#[test]
fn test_signature_protects_encrypted_envelope() {
    let alice_bundle = IdentityBundle::generate().unwrap();
    let bob_bundle = IdentityBundle::generate().unwrap();

    let message = SecretMessage {
        content: "Authenticated and encrypted".to_string(),
        timestamp: 1234567890,
    };

    // Encrypt and sign
    let plaintext = bincode::serialize(&message).unwrap();
    let encrypted_envelope = EncryptedEnvelope::encrypt(
        alice_bundle.did(),
        bob_bundle.did(),
        1,
        &alice_bundle.x25519_secret(),
        &bob_bundle.x25519_public(),
        &plaintext,
    )
    .unwrap();

    let encrypted_bytes = bincode::serialize(&encrypted_envelope).unwrap();
    let mut signed_envelope = SignedEnvelope::new(
        alice_bundle.did(),
        alice_bundle.keypair(),
        1,
        PayloadType::Encrypted,
        encrypted_bytes,
    )
    .unwrap();

    // Tamper with the signed envelope's payload
    signed_envelope.payload[0] ^= 0xFF;

    // Signature verification should fail
    let result = signed_envelope.verify(300);
    assert!(result.is_err(), "Signature should detect tampering");
}

#[test]
fn test_multiple_encrypted_messages_different_nonces() {
    let alice_bundle = IdentityBundle::generate().unwrap();
    let bob_bundle = IdentityBundle::generate().unwrap();

    // Send multiple messages with increasing sequence numbers
    for seq in 1..=5 {
        let message = SecretMessage {
            content: format!("Message #{}", seq),
            timestamp: 1234567890 + seq,
        };

        let plaintext = bincode::serialize(&message).unwrap();
        let encrypted_envelope = EncryptedEnvelope::encrypt(
            alice_bundle.did(),
            bob_bundle.did(),
            seq,
            &alice_bundle.x25519_secret(),
            &bob_bundle.x25519_public(),
            &plaintext,
        )
        .unwrap();

        // Decrypt and verify
        let decrypted_bytes = encrypted_envelope
            .decrypt(&bob_bundle.x25519_secret(), &alice_bundle.x25519_public())
            .unwrap();

        let decrypted_message: SecretMessage = bincode::deserialize(&decrypted_bytes).unwrap();
        assert_eq!(decrypted_message, message);
    }
}

#[test]
fn test_large_encrypted_message() {
    let alice_bundle = IdentityBundle::generate().unwrap();
    let bob_bundle = IdentityBundle::generate().unwrap();

    // Create a large message (1MB)
    let large_content = "A".repeat(1024 * 1024);
    let message = SecretMessage {
        content: large_content.clone(),
        timestamp: 1234567890,
    };

    let plaintext = bincode::serialize(&message).unwrap();
    let encrypted_envelope = EncryptedEnvelope::encrypt(
        alice_bundle.did(),
        bob_bundle.did(),
        1,
        &alice_bundle.x25519_secret(),
        &bob_bundle.x25519_public(),
        &plaintext,
    )
    .unwrap();

    // Decrypt and verify
    let decrypted_bytes = encrypted_envelope
        .decrypt(&bob_bundle.x25519_secret(), &alice_bundle.x25519_public())
        .unwrap();

    let decrypted_message: SecretMessage = bincode::deserialize(&decrypted_bytes).unwrap();
    assert_eq!(decrypted_message.content, large_content);
}
