//! Integration test for end-to-end encrypted messages
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Tests the complete flow:
//! 1. Alice encrypts a message for Bob using X25519
//! 2. Alice signs the encrypted envelope
//! 3. Bob receives and verifies the signature
//! 4. Bob decrypts the message
//!
//! This demonstrates Phase 10 encryption working with Phase 9 authentication.

use anyhow::Result;
use icn_identity::IdentityBundle;
use icn_net::{
    EncryptedEnvelope, IncomingMessageHandler, MessagePayload, NetworkActor, NetworkMessage,
    PayloadType, SignedEnvelope,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Get a random available port using portpicker
fn pick_port() -> u16 {
    portpicker::pick_unused_port().expect("No available ports")
}

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
    let plaintext = bincode::serde::encode_to_vec(&message, bincode::config::legacy()).unwrap();

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
    let encrypted_bytes =
        bincode::serde::encode_to_vec(&encrypted_envelope, bincode::config::legacy()).unwrap();

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
        bincode::serde::decode_from_slice(&signed_envelope.payload, bincode::config::legacy())
            .map(|(v, _)| v)
            .unwrap();

    // Verify envelope metadata
    assert_eq!(received_encrypted.from, *alice_bundle.did());
    assert_eq!(received_encrypted.to, *bob_bundle.did());

    // 3. Decrypt using Bob's X25519 secret key
    let decrypted_bytes = received_encrypted
        .decrypt(&bob_bundle.x25519_secret(), &alice_bundle.x25519_public())
        .unwrap();

    // 4. Deserialize the plaintext message
    let decrypted_message: SecretMessage =
        bincode::serde::decode_from_slice(&decrypted_bytes, bincode::config::legacy())
            .map(|(v, _)| v)
            .unwrap();

    // ========== VERIFICATION ==========
    assert_eq!(decrypted_message, message);
    assert_eq!(
        decrypted_message.content,
        "Meet me at the lighthouse at midnight"
    );
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
    let plaintext = bincode::serde::encode_to_vec(&message, bincode::config::legacy()).unwrap();
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

    assert!(
        result.is_err(),
        "Charlie should not be able to decrypt Bob's message"
    );
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
    let plaintext = bincode::serde::encode_to_vec(&message, bincode::config::legacy()).unwrap();
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
    let result =
        encrypted_envelope.decrypt(&bob_bundle.x25519_secret(), &alice_bundle.x25519_public());

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
    let plaintext = bincode::serde::encode_to_vec(&message, bincode::config::legacy()).unwrap();
    let encrypted_envelope = EncryptedEnvelope::encrypt(
        alice_bundle.did(),
        bob_bundle.did(),
        1,
        &alice_bundle.x25519_secret(),
        &bob_bundle.x25519_public(),
        &plaintext,
    )
    .unwrap();

    let encrypted_bytes =
        bincode::serde::encode_to_vec(&encrypted_envelope, bincode::config::legacy()).unwrap();
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
            content: format!("Message #{seq}"),
            timestamp: 1234567890 + seq,
        };

        let plaintext = bincode::serde::encode_to_vec(&message, bincode::config::legacy()).unwrap();
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

        let decrypted_message: SecretMessage =
            bincode::serde::decode_from_slice(&decrypted_bytes, bincode::config::legacy())
                .map(|(v, _)| v)
                .unwrap();
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

    let plaintext = bincode::serde::encode_to_vec(&message, bincode::config::legacy()).unwrap();
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

    let decrypted_message: SecretMessage =
        bincode::serde::decode_from_slice(&decrypted_bytes, bincode::config::legacy())
            .map(|(v, _)| v)
            .unwrap();
    assert_eq!(decrypted_message.content, large_content);
}

// ============================================================================
// Network-Level Integration Tests
// ============================================================================

/// Test node helper for network integration tests
struct TestNode {
    identity_bundle: IdentityBundle,
    listen_addr: SocketAddr,
    network_handle: icn_net::NetworkHandle,
    _shutdown_tx: broadcast::Sender<()>,
    messages_received: Arc<RwLock<Vec<NetworkMessage>>>,
    _message_receiver_task: tokio::task::JoinHandle<()>,
}

impl TestNode {
    async fn spawn(port: u16) -> Result<Self> {
        let identity_bundle = IdentityBundle::generate()?;
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(1);

        let messages_received: Arc<RwLock<Vec<NetworkMessage>>> = Arc::new(RwLock::new(Vec::new()));
        let messages_clone = messages_received.clone();

        // Use a channel to avoid race conditions with spawned tasks
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<NetworkMessage>();

        // Spawn task to collect messages from channel into the RwLock
        let message_receiver_task = tokio::spawn(async move {
            while let Some(net_msg) = msg_rx.recv().await {
                messages_clone.write().await.push(net_msg);
            }
        });

        // Set up incoming message handler to send to channel
        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let _ = msg_tx.send(net_msg);
        });

        let listen_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let network_handle = NetworkActor::spawn(
            identity_bundle.clone(),
            listen_addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No trust graph
            None, // No trust-gated config
            None, // No fallback config
            None, // No topology config
            None, // No STUN servers for tests
            None, // No TURN config for tests
            None, // No misbehavior detector for tests
        )
        .await?;

        Ok(TestNode {
            identity_bundle,
            listen_addr,
            network_handle,
            _shutdown_tx: shutdown_tx,
            messages_received,
            _message_receiver_task: message_receiver_task,
        })
    }

    async fn dial(&self, other: &TestNode) -> Result<()> {
        self.network_handle
            .dial(other.listen_addr, other.identity_bundle.did().clone())
            .await
    }

    async fn wait_for_messages(&self, count: usize, timeout_ms: u64) -> bool {
        let timeout = Duration::from_millis(timeout_ms);
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if self.messages_received.read().await.len() >= count {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        false
    }
}

#[tokio::test]
#[ignore = "Uses legacy encrypt-sign API; new transparent encryption uses sign-encrypt-sign via supervisor"]
async fn test_network_x25519_key_exchange_and_encrypted_message() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    // ========== SETUP: Create two nodes ==========
    let alice = TestNode::spawn(pick_port()).await?;
    let bob = TestNode::spawn(pick_port()).await?;

    println!("Alice DID: {}", alice.identity_bundle.did());
    println!("Bob DID: {}", bob.identity_bundle.did());

    // ========== STEP 1: Connect nodes (triggers X25519 key exchange) ==========
    alice.dial(&bob).await?;

    // Wait for connection to establish and Hello messages to be exchanged
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ========== STEP 2: Verify X25519 keys were exchanged ==========
    let alice_has_bob_key = alice
        .network_handle
        .get_peer_x25519_key(bob.identity_bundle.did())
        .await;
    let bob_has_alice_key = bob
        .network_handle
        .get_peer_x25519_key(alice.identity_bundle.did())
        .await;

    assert!(
        alice_has_bob_key.is_some(),
        "Alice should have Bob's X25519 public key"
    );
    assert!(
        bob_has_alice_key.is_some(),
        "Bob should have Alice's X25519 public key"
    );

    println!("✓ X25519 keys successfully exchanged during Hello handshake");

    // Verify the exchanged keys match the actual public keys
    assert_eq!(
        alice_has_bob_key.unwrap(),
        *bob.identity_bundle.x25519_public_bytes(),
        "Alice's stored key for Bob should match Bob's actual public key"
    );
    assert_eq!(
        bob_has_alice_key.unwrap(),
        *alice.identity_bundle.x25519_public_bytes(),
        "Bob's stored key for Alice should match Alice's actual public key"
    );

    println!("✓ Exchanged keys verified to match actual public keys");

    // ========== STEP 3: Alice sends encrypted message to Bob ==========
    let secret_message = SecretMessage {
        content: "The falcon has landed".to_string(),
        timestamp: 1234567890,
    };

    // Serialize the secret message
    let plaintext = bincode::serde::encode_to_vec(&secret_message, bincode::config::legacy())?;

    // Encrypt using X25519 keys retrieved from network
    let bob_x25519_public = alice_has_bob_key.unwrap();
    let bob_x25519_public_key = x25519_dalek::PublicKey::from(bob_x25519_public);

    let sequence = 1;
    let encrypted_envelope = EncryptedEnvelope::encrypt(
        alice.identity_bundle.did(),
        bob.identity_bundle.did(),
        sequence,
        &alice.identity_bundle.x25519_secret(),
        &bob_x25519_public_key,
        &plaintext,
    )?;

    // Serialize and sign the encrypted envelope
    let encrypted_bytes =
        bincode::serde::encode_to_vec(&encrypted_envelope, bincode::config::legacy())?;
    let signed_envelope = SignedEnvelope::new(
        alice.identity_bundle.did(),
        alice.identity_bundle.keypair(),
        sequence,
        PayloadType::Encrypted,
        encrypted_bytes,
    )?;

    // Send over the network
    let network_msg =
        NetworkMessage::signed(Some(bob.identity_bundle.did().clone()), signed_envelope);

    alice
        .network_handle
        .send_message(bob.identity_bundle.did().clone(), network_msg)
        .await?;

    println!("✓ Alice sent encrypted message to Bob");

    // ========== STEP 4: Wait for Bob to receive the message ==========
    let received = bob.wait_for_messages(1, 2000).await;
    assert!(received, "Bob should receive Alice's encrypted message");

    // ========== STEP 5: Bob decrypts the message ==========
    let received_messages = bob.messages_received.read().await;
    assert_eq!(
        received_messages.len(),
        1,
        "Bob should have exactly one message"
    );

    let received_msg = &received_messages[0];

    // Extract the signed envelope
    let signed_envelope = match &received_msg.payload {
        MessagePayload::Signed(env) => env,
        _ => panic!("Expected Signed message payload"),
    };

    // Verify the signature
    signed_envelope.verify(300)?;
    println!("✓ Bob verified Alice's signature");

    // Verify it's an encrypted payload
    assert_eq!(signed_envelope.payload_type, PayloadType::Encrypted);

    // Deserialize the encrypted envelope
    let received_encrypted: EncryptedEnvelope =
        bincode::serde::decode_from_slice(&signed_envelope.payload, bincode::config::legacy())
            .map(|(v, _)| v)?;

    // Decrypt using Bob's X25519 secret key and Alice's public key
    let alice_x25519_public = bob_has_alice_key.unwrap();
    let alice_x25519_public_key = x25519_dalek::PublicKey::from(alice_x25519_public);

    let decrypted_bytes = received_encrypted.decrypt(
        &bob.identity_bundle.x25519_secret(),
        &alice_x25519_public_key,
    )?;

    // Deserialize the plaintext message
    let decrypted_message: SecretMessage =
        bincode::serde::decode_from_slice(&decrypted_bytes, bincode::config::legacy())
            .map(|(v, _)| v)?;

    println!(
        "✓ Bob decrypted the message: '{}'",
        decrypted_message.content
    );

    // ========== VERIFICATION ==========
    assert_eq!(decrypted_message, secret_message);
    assert_eq!(decrypted_message.content, "The falcon has landed");

    println!("✓ End-to-end encrypted messaging successful!");

    Ok(())
}

/// Test the convenience API for sending encrypted messages
#[tokio::test]
#[ignore = "Uses legacy encrypt-sign API; new transparent encryption uses sign-encrypt-sign via supervisor"]
async fn test_send_encrypted_message_convenience_api() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt::try_init();

    println!("\n=== Testing send_encrypted_message() Convenience API ===\n");

    // ========== SETUP: Spawn Alice and Bob test nodes ==========
    let alice = TestNode::spawn(pick_port()).await?;
    let bob = TestNode::spawn(pick_port()).await?;

    println!("Alice DID: {}", alice.identity_bundle.did());
    println!("Bob DID:   {}", bob.identity_bundle.did());

    // ========== STEP 1: Alice dials Bob ==========
    println!("\n--- Alice connecting to Bob ---");
    alice.dial(&bob).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ========== STEP 2: Use convenience API to send encrypted message ==========
    println!("\n--- Alice sending encrypted message using convenience API ---");

    let secret_payload = b"This message was encrypted with the convenience API!";
    let sequence = 1;

    alice
        .network_handle
        .send_encrypted_message(
            bob.identity_bundle.did(),
            alice.identity_bundle.keypair(),
            &alice.identity_bundle.x25519_secret(),
            sequence,
            secret_payload,
        )
        .await?;

    println!("✓ Alice sent encrypted message using send_encrypted_message()");

    // ========== STEP 3: Wait for Bob to receive ==========
    bob.wait_for_messages(1, 500).await;

    // ========== STEP 4: Bob decrypts ==========
    let bob_received = bob.messages_received.read().await;
    assert_eq!(
        bob_received.len(),
        1,
        "Bob should have received exactly one message"
    );

    let msg = &bob_received[0];

    // Extract signed envelope
    let signed_env = match &msg.payload {
        MessagePayload::Signed(env) => env,
        _ => panic!("Expected Signed payload"),
    };

    // Verify signature
    signed_env.verify(300)?;
    println!("✓ Bob verified Alice's signature");

    // Extract encrypted envelope
    let encrypted_env: EncryptedEnvelope =
        bincode::serde::decode_from_slice(&signed_env.payload, bincode::config::legacy())
            .map(|(v, _)| v)?;

    // Get Alice's X25519 public key
    let alice_x25519_public = bob
        .network_handle
        .get_peer_x25519_key(alice.identity_bundle.did())
        .await
        .expect("Bob should have Alice's X25519 key from Hello exchange");
    let alice_x25519_public_key = x25519_dalek::PublicKey::from(alice_x25519_public);

    // Decrypt
    let decrypted = encrypted_env.decrypt(
        &bob.identity_bundle.x25519_secret(),
        &alice_x25519_public_key,
    )?;

    println!("✓ Bob decrypted: {:?}", String::from_utf8_lossy(&decrypted));

    // ========== VERIFICATION ==========
    assert_eq!(decrypted, secret_payload);

    println!("\n✓ Convenience API test successful!");

    Ok(())
}
