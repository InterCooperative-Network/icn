//! Demonstration of signed message functionality
//!
//! This example shows how to:
//! 1. Create signed envelopes with Ed25519 signatures
//! 2. Verify message authenticity and integrity
//! 3. Detect replay attacks
//! 4. Handle out-of-order delivery
//!
//! Run with: cargo run --example signed_messages_demo

use anyhow::Result;
use icn_identity::KeyPair;
use icn_net::{PayloadType, ReplayGuard, SignedEnvelope};

fn main() -> Result<()> {
    println!("=== ICN Signed Messages Demo ===\n");

    // 1. Generate identities
    println!("1. Generating Ed25519 keypairs...");
    let alice_keypair = KeyPair::generate()?;
    let alice_did = alice_keypair.did();
    let bob_keypair = KeyPair::generate()?;
    let bob_did = bob_keypair.did();

    println!("   Alice: {}", alice_did);
    println!("   Bob: {}\n", bob_did);

    // 2. Create a signed message from Alice
    println!("2. Alice creates a signed message (seq=1)...");
    let message = SignedEnvelope::new(
        alice_did,
        &alice_keypair,
        1,
        PayloadType::Gossip,
        b"Hello Bob!".to_vec(),
    )?;
    println!("   Timestamp: {}", message.timestamp);
    println!("   Signature: {}...{}",
             hex::encode(&message.signature[..4]),
             hex::encode(&message.signature[message.signature.len()-4..]));
    println!("   ✓ Message signed\n");

    // 3. Bob verifies the message
    println!("3. Bob verifies Alice's message...");
    message.verify(300)?;
    println!("   ✓ Signature valid");
    println!("   ✓ Timestamp fresh (within 300s)\n");

    // 4. Replay attack detection
    println!("4. Testing replay protection...");
    let mut replay_guard = ReplayGuard::new(300, 3600);

    // First delivery - accepted
    replay_guard.check(&message)?;
    println!("   ✓ First delivery accepted");

    // Replay attack - rejected
    match replay_guard.check(&message) {
        Ok(_) => println!("   ✗ SECURITY FAILURE: Replay not detected!"),
        Err(e) => println!("   ✓ Replay attack blocked: {}", e),
    }
    println!();

    // 5. Out-of-order delivery
    println!("5. Testing out-of-order delivery...");

    // Alice sends message 3 first
    let msg3 = SignedEnvelope::new(
        alice_did,
        &alice_keypair,
        3,
        PayloadType::Gossip,
        b"Message 3".to_vec(),
    )?;
    replay_guard.check(&msg3)?;
    println!("   ✓ Message 3 accepted (seq=3)");

    // Then message 2 arrives late (out of order)
    let msg2 = SignedEnvelope::new(
        alice_did,
        &alice_keypair,
        2,
        PayloadType::Gossip,
        b"Message 2".to_vec(),
    )?;
    replay_guard.check(&msg2)?;
    println!("   ✓ Message 2 accepted (seq=2, out-of-order)");

    // But trying to replay message 2 is still blocked
    match replay_guard.check(&msg2) {
        Ok(_) => println!("   ✗ SECURITY FAILURE: Replay not detected!"),
        Err(_) => println!("   ✓ Replay of message 2 still blocked"),
    }
    println!();

    // 6. Tampering detection
    println!("6. Testing tamper detection...");
    let mut tampered = SignedEnvelope::new(
        alice_did,
        &alice_keypair,
        4,
        PayloadType::Gossip,
        b"Original message".to_vec(),
    )?;

    // Attacker tries to modify the payload
    tampered.payload = b"TAMPERED!".to_vec();

    match tampered.verify(300) {
        Ok(_) => println!("   ✗ SECURITY FAILURE: Tampering not detected!"),
        Err(e) => println!("   ✓ Tampering detected: {}", e),
    }
    println!();

    // 7. Multiple peers
    println!("7. Testing multiple independent peers...");
    println!("   Peers tracked: {}", replay_guard.peer_count());
    println!("   Alice max seq: {:?}", replay_guard.get_max_seq(alice_did));
    println!("   Bob max seq: {:?}", replay_guard.get_max_seq(bob_did));

    // Bob sends a message (independent sequence)
    let bob_msg = SignedEnvelope::new(
        bob_did,
        &bob_keypair,
        1,
        PayloadType::Gossip,
        b"Hello Alice!".to_vec(),
    )?;
    replay_guard.check(&bob_msg)?;
    println!("   ✓ Bob's message accepted (seq=1, independent from Alice)");
    println!("   Peers tracked: {}", replay_guard.peer_count());
    println!();

    println!("=== Demo Complete ===");
    println!("\nKey Features Demonstrated:");
    println!("  ✓ Ed25519 digital signatures");
    println!("  ✓ Replay attack prevention");
    println!("  ✓ Out-of-order message tolerance");
    println!("  ✓ Tamper detection");
    println!("  ✓ Per-peer sequence tracking");
    println!("\nSecurity Properties:");
    println!("  • Cryptographic authenticity (sender proof)");
    println!("  • Message integrity (tamper detection)");
    println!("  • Freshness (timestamp validation)");
    println!("  • Non-repudiation (Ed25519 signatures)");

    Ok(())
}
