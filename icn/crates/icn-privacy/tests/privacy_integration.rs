//! Privacy Crate Integration Tests
//!
//! These tests validate the full privacy stack:
//! 1. Topic encryption with Bloom filter discovery
//! 2. Onion routing with multi-hop circuits
//! 3. Traffic obfuscation (padding, delays, cover traffic)
//! 4. Combined privacy scenarios

use icn_identity::KeyPair;
use icn_privacy::{
    select_relays, ObfuscationConfig, OnionRouter, TopicBloomFilter, TopicEncryptor,
    TrafficObfuscator,
};
use std::collections::HashMap;
use std::time::Duration;
use x25519_dalek::{PublicKey, StaticSecret};

// === Topic Encryption Integration Tests ===

#[test]
fn test_multi_party_topic_encryption() {
    // Simulate multiple parties sharing a cooperative key
    let shared_key = [42u8; 32]; // In production: derived from member DIDs

    let alice_encryptor = TopicEncryptor::new(shared_key);
    let bob_encryptor = TopicEncryptor::new(shared_key);
    let charlie_encryptor = TopicEncryptor::new(shared_key);

    // Alice encrypts a topic
    let topic = "coop:governance:proposals";
    let encrypted = alice_encryptor.encrypt(topic).unwrap();

    // Bob can decrypt it (same shared key)
    let decrypted_by_bob = bob_encryptor.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted_by_bob, topic);

    // Charlie can also decrypt it
    let decrypted_by_charlie = charlie_encryptor.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted_by_charlie, topic);

    // But different keys fail
    let different_key = [99u8; 32];
    let outsider_encryptor = TopicEncryptor::new(different_key);
    assert!(outsider_encryptor.decrypt(&encrypted).is_err());
}

#[test]
fn test_bloom_filter_topic_discovery_multiple_subscribers() {
    let shared_key = [1u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    // Create encrypted topics
    let topics = [
        "ledger:sync",
        "governance:votes",
        "compute:tasks",
        "identity:updates",
    ];

    let encrypted_topics: Vec<_> = topics
        .iter()
        .map(|t| encryptor.encrypt(t).unwrap())
        .collect();

    // Subscriber A is interested in ledger and governance
    let mut filter_a = TopicBloomFilter::new();
    filter_a.insert("ledger:sync");
    filter_a.insert("governance:votes");

    // Subscriber B is interested in compute and identity
    let mut filter_b = TopicBloomFilter::new();
    filter_b.insert("compute:tasks");
    filter_b.insert("identity:updates");

    // Test discovery
    assert!(filter_a.matches(&encrypted_topics[0])); // ledger:sync
    assert!(filter_a.matches(&encrypted_topics[1])); // governance:votes
    assert!(!filter_a.matches(&encrypted_topics[2])); // compute:tasks (not subscribed)
    assert!(!filter_a.matches(&encrypted_topics[3])); // identity:updates (not subscribed)

    assert!(!filter_b.matches(&encrypted_topics[0])); // ledger:sync (not subscribed)
    assert!(!filter_b.matches(&encrypted_topics[1])); // governance:votes (not subscribed)
    assert!(filter_b.matches(&encrypted_topics[2])); // compute:tasks
    assert!(filter_b.matches(&encrypted_topics[3])); // identity:updates
}

#[test]
fn test_topic_encryption_prevents_linkability() {
    let shared_key = [5u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    // Encrypt same topic multiple times
    let topic = "secret:topic";
    let encrypted1 = encryptor.encrypt(topic).unwrap();
    let encrypted2 = encryptor.encrypt(topic).unwrap();
    let encrypted3 = encryptor.encrypt(topic).unwrap();

    // Nonces should be different (prevents linkability)
    assert_ne!(encrypted1.nonce, encrypted2.nonce);
    assert_ne!(encrypted2.nonce, encrypted3.nonce);
    assert_ne!(encrypted1.nonce, encrypted3.nonce);

    // Ciphertext should also be different (due to nonces)
    assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
    assert_ne!(encrypted2.ciphertext, encrypted3.ciphertext);

    // But all should decrypt to same topic
    assert_eq!(encryptor.decrypt(&encrypted1).unwrap(), topic);
    assert_eq!(encryptor.decrypt(&encrypted2).unwrap(), topic);
    assert_eq!(encryptor.decrypt(&encrypted3).unwrap(), topic);

    // Bloom hints ARE the same (hash of plaintext)
    // This is intentional - allows discovery while hiding content
    assert_eq!(encrypted1.bloom_hint, encrypted2.bloom_hint);
    assert_eq!(encrypted2.bloom_hint, encrypted3.bloom_hint);
}

#[test]
fn test_find_matches_across_many_topics() {
    let shared_key = [10u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    // Create a large set of encrypted topics
    let all_topics: Vec<String> = (0..100).map(|i| format!("topic:{i}")).collect();

    let encrypted_all: Vec<_> = all_topics
        .iter()
        .map(|t| encryptor.encrypt(t).unwrap())
        .collect();

    // Find specific topics
    let matches_42 = encryptor.find_matches("topic:42", &encrypted_all);
    assert_eq!(matches_42.len(), 1);

    let matches_99 = encryptor.find_matches("topic:99", &encrypted_all);
    assert_eq!(matches_99.len(), 1);

    // Non-existent topic
    let matches_none = encryptor.find_matches("topic:200", &encrypted_all);
    assert_eq!(matches_none.len(), 0);
}

// === Onion Routing Integration Tests ===

fn make_test_node() -> (icn_identity::Did, StaticSecret, PublicKey) {
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let secret = StaticSecret::random_from_rng(rand::thread_rng());
    let public = PublicKey::from(&secret);
    (did, secret, public)
}

#[test]
fn test_onion_circuit_creation_and_validation() {
    // Create nodes
    let (sender_did, sender_secret, _sender_pub) = make_test_node();
    let (relay1_did, _relay1_secret, relay1_pub) = make_test_node();
    let (relay2_did, _relay2_secret, relay2_pub) = make_test_node();
    let (recipient_did, _recipient_secret, recipient_pub) = make_test_node();

    // Create sender's router
    let mut sender_router = OnionRouter::new(sender_did.clone(), sender_secret);

    // Register peer public keys
    sender_router.add_peer_key(relay1_did.clone(), relay1_pub);
    sender_router.add_peer_key(relay2_did.clone(), relay2_pub);
    sender_router.add_peer_key(recipient_did.clone(), recipient_pub);

    // Create circuit with 2 relays
    let circuit = sender_router
        .create_circuit(
            vec![relay1_did.clone(), relay2_did.clone()],
            recipient_did.clone(),
        )
        .unwrap();

    assert_eq!(circuit.relays.len(), 2);
    assert_eq!(circuit.relays[0], relay1_did);
    assert_eq!(circuit.relays[1], relay2_did);
    assert_eq!(circuit.recipient, recipient_did);
}

#[test]
fn test_onion_circuit_missing_key_fails() {
    let (sender_did, sender_secret, _) = make_test_node();
    let (relay1_did, _, _) = make_test_node();
    let (recipient_did, _, _) = make_test_node();

    let router = OnionRouter::new(sender_did, sender_secret);
    // Don't register relay1's key

    // Creating circuit should fail due to missing key
    let result = router.create_circuit(vec![relay1_did], recipient_did);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Missing public key"));
}

#[test]
fn test_onion_wrap_peel_extract_two_hops() {
    // Create participants
    let (sender_did, sender_secret, sender_pub) = make_test_node();
    let (relay_did, relay_secret, relay_pub) = make_test_node();
    let (recipient_did, recipient_secret, recipient_pub) = make_test_node();

    // Setup sender's router
    let mut sender_router = OnionRouter::new(sender_did.clone(), sender_secret);
    sender_router.add_peer_key(relay_did.clone(), relay_pub);
    sender_router.add_peer_key(recipient_did.clone(), recipient_pub);

    // Setup relay's router (needs sender's pubkey for potential replies)
    let mut relay_router = OnionRouter::new(relay_did.clone(), relay_secret);
    relay_router.add_peer_key(sender_did.clone(), sender_pub);
    relay_router.add_peer_key(recipient_did.clone(), recipient_pub);

    // Setup recipient's router
    let mut recipient_router = OnionRouter::new(recipient_did.clone(), recipient_secret);
    recipient_router.add_peer_key(sender_did.clone(), sender_pub);
    recipient_router.add_peer_key(relay_did.clone(), relay_pub);

    // Create circuit: sender -> relay -> recipient
    let circuit = sender_router
        .create_circuit(vec![relay_did.clone()], recipient_did.clone())
        .unwrap();

    // Sender wraps message
    let original_payload = b"Secret message for recipient";
    let onion_msg = sender_router
        .wrap_message(&circuit, original_payload)
        .unwrap();

    // Relay peels one layer
    let peel_result = relay_router.peel_layer(onion_msg).unwrap();
    assert!(
        peel_result.is_some(),
        "Relay should be able to peel a layer"
    );
    let (next_hop, peeled) = peel_result.unwrap();
    assert_eq!(next_hop, recipient_did);

    // Recipient extracts final payload
    let extracted = recipient_router.extract_payload(&peeled).unwrap();
    assert_eq!(extracted, original_payload.to_vec());
}

#[test]
fn test_relay_selection_with_trust_scores() {
    // Create candidate relays
    let (relay1, _, _) = make_test_node();
    let (relay2, _, _) = make_test_node();
    let (relay3, _, _) = make_test_node();
    let (relay4, _, _) = make_test_node();

    let candidates = vec![
        relay1.clone(),
        relay2.clone(),
        relay3.clone(),
        relay4.clone(),
    ];

    let mut trust_scores = HashMap::new();
    trust_scores.insert(relay1.clone(), 0.9); // High trust
    trust_scores.insert(relay2.clone(), 0.7); // Good trust
    trust_scores.insert(relay3.clone(), 0.3); // Low trust
    trust_scores.insert(relay4.clone(), 0.8); // Good trust

    // Select 2 relays with min trust 0.5
    let selected = select_relays(&candidates, &trust_scores, 2, 0.5);

    // Should get 2 relays
    assert_eq!(selected.len(), 2);

    // All selected should have trust >= 0.5
    for did in &selected {
        let trust = trust_scores.get(did).unwrap();
        assert!(*trust >= 0.5);
    }

    // relay3 (0.3 trust) should NOT be selected
    assert!(!selected.contains(&relay3));
}

#[test]
fn test_relay_selection_insufficient_trusted() {
    let (relay1, _, _) = make_test_node();
    let (relay2, _, _) = make_test_node();

    let candidates = vec![relay1.clone(), relay2.clone()];

    let mut trust_scores = HashMap::new();
    trust_scores.insert(relay1.clone(), 0.3); // Below threshold
    trust_scores.insert(relay2.clone(), 0.2); // Below threshold

    // Try to select 2 relays with min trust 0.5 - should fail
    let selected = select_relays(&candidates, &trust_scores, 2, 0.5);
    assert!(selected.is_empty());
}

// === Traffic Obfuscation Integration Tests ===

#[test]
fn test_message_padding_roundtrip() {
    let config = ObfuscationConfig {
        enable_padding: true,
        padded_size: 1024,
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // Test with various message sizes
    let test_messages = vec![
        b"short".to_vec(),
        b"medium length message with more content".to_vec(),
        vec![42u8; 500], // Larger message
        vec![0u8; 1],    // Minimal message
    ];

    for original in test_messages {
        let padded = obfuscator.pad_message(&original).unwrap();

        // All padded messages should be same size
        assert_eq!(padded.payload.len() + padded.padding.len(), 1024);
        assert_eq!(padded.original_len, original.len());

        // Unpad should recover original
        let unpadded = obfuscator.unpad_message(&padded).unwrap();
        assert_eq!(unpadded, original);
    }
}

#[test]
fn test_message_padding_size_uniformity() {
    let config = ObfuscationConfig {
        enable_padding: true,
        padded_size: 2048,
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // Create messages of varying sizes
    let sizes = [1, 10, 100, 500, 1000, 1500, 2000];
    let mut total_sizes = Vec::new();

    for size in sizes {
        let msg = vec![0u8; size];
        let padded = obfuscator.pad_message(&msg).unwrap();
        total_sizes.push(padded.payload.len() + padded.padding.len());
    }

    // All should be same size (observer can't distinguish based on size)
    let first_size = total_sizes[0];
    for size in &total_sizes {
        assert_eq!(*size, first_size, "All padded messages should be same size");
    }
}

#[test]
fn test_message_too_large_for_padding() {
    let config = ObfuscationConfig {
        enable_padding: true,
        padded_size: 100,
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // Message larger than padded_size should fail
    let large_msg = vec![0u8; 200];
    let result = obfuscator.pad_message(&large_msg);
    assert!(result.is_err());
}

#[test]
fn test_random_delays_within_bounds() {
    let config = ObfuscationConfig {
        enable_delays: true,
        min_delay_ms: 100,
        max_delay_ms: 500,
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // Generate many delays and verify they're within bounds
    for _ in 0..100 {
        let delay = obfuscator.random_delay();
        let delay_ms = delay.as_millis() as u64;
        assert!(delay_ms >= 100, "Delay {delay_ms} should be >= 100ms");
        assert!(delay_ms <= 500, "Delay {delay_ms} should be <= 500ms");
    }
}

#[test]
fn test_delays_disabled() {
    let config = ObfuscationConfig {
        enable_delays: false,
        min_delay_ms: 100,
        max_delay_ms: 500,
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // All delays should be 0 when disabled
    for _ in 0..10 {
        let delay = obfuscator.random_delay();
        assert_eq!(delay.as_millis(), 0);
    }
}

#[test]
fn test_cover_traffic_generation_rate() {
    let config = ObfuscationConfig {
        enable_cover_traffic: true,
        cover_traffic_rate: 1.0, // 1 message per second
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // If we just sent, shouldn't need to send cover traffic
    let _just_sent = Duration::from_millis(100);
    // May or may not trigger depending on probability

    // If it's been a while, should be more likely to send
    let long_time = Duration::from_secs(10);
    let mut should_send_count = 0;
    for _ in 0..100 {
        if obfuscator.should_send_cover_traffic(long_time) {
            should_send_count += 1;
        }
    }
    // With a 10 second gap and 1 msg/sec rate, should often trigger
    assert!(
        should_send_count > 50,
        "Cover traffic should trigger frequently after long delay"
    );
}

#[test]
fn test_cover_traffic_disabled() {
    let config = ObfuscationConfig {
        enable_cover_traffic: false,
        cover_traffic_rate: 1.0, // Even at 100%, should not trigger
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // Should never trigger when disabled
    for _ in 0..100 {
        assert!(!obfuscator.should_send_cover_traffic(Duration::from_secs(100)));
    }
}

#[test]
fn test_cover_traffic_content() {
    let config = ObfuscationConfig {
        enable_padding: true,
        padded_size: 512,
        ..Default::default()
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // Generate cover traffic
    let cover1 = obfuscator.generate_cover_traffic();
    let cover2 = obfuscator.generate_cover_traffic();

    // Cover traffic should be padded_size bytes
    assert_eq!(cover1.len(), 512);
    assert_eq!(cover2.len(), 512);

    // Cover traffic should be random (different each time)
    assert_ne!(cover1, cover2);
}

// === Combined Privacy Stack Tests ===

#[test]
fn test_combined_topic_encryption_with_obfuscation() {
    // Setup topic encryption
    let shared_key = [123u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    // Setup traffic obfuscation
    let config = ObfuscationConfig {
        enable_padding: true,
        padded_size: 512,
        enable_delays: false, // Disable for test determinism
        ..Default::default()
    };
    let obfuscator = TrafficObfuscator::with_config(config);

    // Encrypt topic
    let topic = "private:channel";
    let encrypted_topic = encryptor.encrypt(topic).unwrap();

    // Serialize encrypted topic for transmission
    let serialized = bincode::serde::encode_to_vec(&encrypted_topic, bincode::config::legacy()).unwrap();

    // Pad the serialized data
    let padded = obfuscator.pad_message(&serialized).unwrap();

    // Verify padding
    assert_eq!(padded.payload.len() + padded.padding.len(), 512);

    // Simulate transmission...

    // Unpad on receive
    let unpadded = obfuscator.unpad_message(&padded).unwrap();

    // Deserialize
    let received_encrypted: icn_privacy::EncryptedTopic = bincode::serde::decode_from_slice(&unpadded, bincode::config::legacy())
        .map(|(v, _)| v)
        .unwrap();

    // Decrypt
    let decrypted = encryptor.decrypt(&received_encrypted).unwrap();
    assert_eq!(decrypted, topic);
}

#[test]
fn test_obfuscation_config_accessors() {
    let config = ObfuscationConfig {
        enable_padding: true,
        padded_size: 4096,
        enable_delays: true,
        min_delay_ms: 50,
        max_delay_ms: 200,
        enable_cover_traffic: true,
        cover_traffic_rate: 0.25,
    };

    let obfuscator = TrafficObfuscator::with_config(config);

    // Test that config is accessible
    assert_eq!(obfuscator.padded_size(), 4096);
}

#[test]
fn test_default_obfuscation_config() {
    let obfuscator = TrafficObfuscator::new();

    // Default should have reasonable values
    assert_eq!(obfuscator.padded_size(), 1024);

    // Default delays enabled with reasonable range
    let delay = obfuscator.random_delay();
    assert!(delay.as_millis() <= 500);
}

// === Error Handling Tests ===

#[test]
fn test_topic_decryption_with_corrupted_ciphertext() {
    let shared_key = [50u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    let encrypted = encryptor.encrypt("test:topic").unwrap();

    // Corrupt the ciphertext
    let mut corrupted = encrypted.clone();
    if !corrupted.ciphertext.is_empty() {
        corrupted.ciphertext[0] ^= 0xFF;
    }

    // Decryption should fail
    assert!(encryptor.decrypt(&corrupted).is_err());
}

#[test]
fn test_topic_decryption_with_wrong_nonce() {
    let shared_key = [60u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    let encrypted = encryptor.encrypt("test:topic").unwrap();

    // Use wrong nonce
    let mut wrong_nonce = encrypted.clone();
    wrong_nonce.nonce = [0u8; 12];

    // Decryption should fail
    assert!(encryptor.decrypt(&wrong_nonce).is_err());
}

#[test]
fn test_empty_topic_encryption() {
    let shared_key = [70u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    // Empty topic should work
    let encrypted = encryptor.encrypt("").unwrap();
    let decrypted = encryptor.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, "");
}

#[test]
fn test_unicode_topic_encryption() {
    let shared_key = [80u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    // Unicode topic
    let topic = "协作社:提案:投票";
    let encrypted = encryptor.encrypt(topic).unwrap();
    let decrypted = encryptor.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, topic);
}

#[test]
fn test_large_topic_encryption() {
    let shared_key = [90u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);

    // Large topic name
    let topic = "a".repeat(1000);
    let encrypted = encryptor.encrypt(&topic).unwrap();
    let decrypted = encryptor.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, topic);
}

// === Bloom Filter Edge Cases ===

#[test]
fn test_bloom_filter_empty() {
    let filter = TopicBloomFilter::new();
    assert!(filter.is_empty());
    assert_eq!(filter.len(), 0);

    // Should not match anything
    let shared_key = [1u8; 32];
    let encryptor = TopicEncryptor::new(shared_key);
    let encrypted = encryptor.encrypt("any:topic").unwrap();
    assert!(!filter.matches(&encrypted));
}

#[test]
fn test_bloom_filter_many_topics() {
    let mut filter = TopicBloomFilter::new();

    // Add many topics
    for i in 0..1000 {
        filter.insert(&format!("topic:{i}"));
    }

    assert_eq!(filter.len(), 1000);

    // Check containment
    assert!(filter.contains("topic:500"));
    assert!(filter.contains("topic:999"));
    assert!(!filter.contains("topic:1000"));
    assert!(!filter.contains("nonexistent"));
}
