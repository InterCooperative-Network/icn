//! Integration tests for the ICN gossip layer.
//!
//! These tests verify gossip functionality including:
//! - Vector clock causal ordering
//! - Bloom filter anti-entropy
//! - Topic configuration
//! - Message serialization
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::{AccessControl, BloomFilter, GossipEntry, GossipMessage, Topic, VectorClock};
use icn_identity::KeyPair;

use sha2::{Digest, Sha256};

// ============================================================================
// Helper Functions
// ============================================================================

fn compute_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

// ============================================================================
// Vector Clock Integration Tests
// ============================================================================

#[test]
fn test_vector_clock_causal_ordering() {
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();

    let mut clock_a = VectorClock::new();
    let mut clock_b = VectorClock::new();

    // Alice increments
    clock_a.increment(alice.did());
    clock_a.increment(alice.did());

    // Bob merges Alice's clock and increments
    clock_b.merge(&clock_a);
    clock_b.increment(bob.did());

    // Bob's clock should happen-after Alice's
    assert!(
        clock_b.happened_after(&clock_a),
        "Bob's clock should happen after Alice's"
    );
    assert!(
        !clock_a.happened_after(&clock_b),
        "Alice's clock should not happen after Bob's"
    );

    // Carol creates independent clock
    let mut clock_c = VectorClock::new();
    clock_c.increment(carol.did());
    clock_c.increment(carol.did());

    // Carol's clock is concurrent with both
    assert!(
        !clock_c.happened_after(&clock_a),
        "Carol's clock should not happen after Alice's"
    );
    assert!(
        !clock_a.happened_after(&clock_c),
        "Alice's clock should not happen after Carol's"
    );
    assert!(
        clock_c.is_concurrent(&clock_a),
        "Clocks should be concurrent"
    );
}

#[test]
fn test_vector_clock_merge_preserves_causality() {
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();
    let carol = KeyPair::generate().unwrap();

    // Create three clocks with different histories
    let mut clock_a = VectorClock::new();
    clock_a.increment(alice.did());
    clock_a.increment(alice.did());

    let mut clock_b = VectorClock::new();
    clock_b.increment(bob.did());
    clock_b.increment(bob.did());
    clock_b.increment(bob.did());

    let mut clock_c = VectorClock::new();
    clock_c.increment(carol.did());

    // Merge all into one
    let mut merged = VectorClock::new();
    merged.merge(&clock_a);
    merged.merge(&clock_b);
    merged.merge(&clock_c);

    // Merged clock should have max of each component
    assert_eq!(merged.get(alice.did()), 2);
    assert_eq!(merged.get(bob.did()), 3);
    assert_eq!(merged.get(carol.did()), 1);

    // Merged clock should happen-after all originals
    assert!(merged.happened_after(&clock_a));
    assert!(merged.happened_after(&clock_b));
    assert!(merged.happened_after(&clock_c));
}

#[test]
fn test_vector_clock_empty_comparisons() {
    let alice = KeyPair::generate().unwrap();

    let empty = VectorClock::new();
    let mut non_empty = VectorClock::new();
    non_empty.increment(alice.did());

    // Non-empty should happen after empty
    assert!(non_empty.happened_after(&empty));
    assert!(!empty.happened_after(&non_empty));

    // Empty clocks are equal
    let empty2 = VectorClock::new();
    assert!(!empty.happened_after(&empty2));
    assert!(!empty2.happened_after(&empty));
    assert!(!empty.is_concurrent(&empty2));
}

#[test]
fn test_vector_clock_partial_cmp() {
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let mut clock_a = VectorClock::new();
    clock_a.increment(alice.did());

    let mut clock_b = VectorClock::new();
    clock_b.increment(bob.did());

    // Concurrent clocks return None for partial_cmp
    assert!(clock_a.partial_cmp(&clock_b).is_none());

    // Happens-after returns Greater
    let mut clock_c = VectorClock::new();
    clock_c.merge(&clock_a);
    clock_c.increment(alice.did());

    assert_eq!(
        clock_c.partial_cmp(&clock_a),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(
        clock_a.partial_cmp(&clock_c),
        Some(std::cmp::Ordering::Less)
    );
}

// ============================================================================
// Bloom Filter Integration Tests
// ============================================================================

#[test]
fn test_bloom_filter_anti_entropy() {
    let mut filter_a = BloomFilter::new(1000, 0.01);
    let mut filter_b = BloomFilter::new(1000, 0.01);

    // Alice has entries 1-50
    let entries_a: Vec<[u8; 32]> = (1..=50)
        .map(|i| compute_hash(format!("entry-{i}").as_bytes()))
        .collect();

    for entry in &entries_a {
        filter_a.insert(entry);
    }

    // Bob has entries 40-80 (overlap with Alice: 40-50)
    let entries_b: Vec<[u8; 32]> = (40..=80)
        .map(|i| compute_hash(format!("entry-{i}").as_bytes()))
        .collect();

    for entry in &entries_b {
        filter_b.insert(entry);
    }

    // Check that Alice's filter contains Alice's entries
    for entry in &entries_a {
        assert!(filter_a.contains(entry));
    }

    // Check overlap detection
    let mut alice_missing = 0;
    let mut bob_missing = 0;

    // Entries Bob has that Alice might not
    for entry in &entries_b {
        if !filter_a.contains(entry) {
            bob_missing += 1; // Bob has it, Alice doesn't
        }
    }

    // Entries Alice has that Bob might not
    for entry in &entries_a {
        if !filter_b.contains(entry) {
            alice_missing += 1; // Alice has it, Bob doesn't
        }
    }

    // Alice is missing entries 51-80 (30 entries)
    // Due to false positives, actual count may be slightly lower
    assert!(
        bob_missing >= 25,
        "Bob should have ~30 entries Alice doesn't have: {bob_missing}"
    );

    // Bob is missing entries 1-39 (39 entries)
    assert!(
        alice_missing >= 35,
        "Alice should have ~39 entries Bob doesn't have: {alice_missing}"
    );
}

#[test]
fn test_bloom_filter_no_false_negatives() {
    let mut filter = BloomFilter::new(10000, 0.01);

    // Insert 5000 entries
    let entries: Vec<[u8; 32]> = (0..5000)
        .map(|i| compute_hash(format!("entry-{i}").as_bytes()))
        .collect();

    for entry in &entries {
        filter.insert(entry);
    }

    // Verify all entries are found (no false negatives)
    let mut false_negatives = 0;
    for entry in &entries {
        if !filter.contains(entry) {
            false_negatives += 1;
        }
    }

    assert_eq!(
        false_negatives, 0,
        "Bloom filters should never have false negatives"
    );
}

#[test]
fn test_bloom_filter_false_positive_bounded() {
    let mut filter = BloomFilter::new(1000, 0.01);

    // Insert 500 entries
    for i in 0..500 {
        let hash = compute_hash(format!("entry-{i}").as_bytes());
        filter.insert(&hash);
    }

    // Test 1000 entries that were never inserted
    let mut false_positives = 0;
    for i in 10000..11000 {
        let hash = compute_hash(format!("entry-{i}").as_bytes());
        if filter.contains(&hash) {
            false_positives += 1;
        }
    }

    // False positive rate should be around 1%, allow up to 5%
    let fp_rate = false_positives as f64 / 1000.0;
    assert!(
        fp_rate < 0.05,
        "False positive rate too high: {}%",
        fp_rate * 100.0
    );
}

// ============================================================================
// Topic and Access Control Tests
// ============================================================================

#[test]
fn test_topic_creation() {
    let topic = Topic::new("test:topic".to_string(), AccessControl::Public);
    assert_eq!(topic.name, "test:topic");
    assert!(matches!(topic.acl, AccessControl::Public));
}

#[test]
fn test_topic_trust_score_access() {
    let topic = Topic::new("secure:data".to_string(), AccessControl::MinTrustScore(0.4));

    assert_eq!(topic.name, "secure:data");
    match topic.acl {
        AccessControl::MinTrustScore(score) => {
            assert!((score - 0.4).abs() < f64::EPSILON);
        }
        _ => panic!("Expected MinTrustScore access control"),
    }
}

#[test]
fn test_topic_participant_access() {
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let participants = vec![alice.did().clone(), bob.did().clone()];
    let topic = Topic::new(
        "private:channel".to_string(),
        AccessControl::Participants(participants.clone()),
    );

    assert_eq!(topic.name, "private:channel");
    match &topic.acl {
        AccessControl::Participants(p) => {
            assert_eq!(p.len(), 2);
            assert!(p.contains(alice.did()));
            assert!(p.contains(bob.did()));
        }
        _ => panic!("Expected Participants access control"),
    }
}

#[test]
fn test_access_control_variants() {
    // Public
    let public = AccessControl::Public;
    assert!(matches!(public, AccessControl::Public));

    // MinTrustScore
    let trust_gated = AccessControl::MinTrustScore(0.1);
    assert!(matches!(trust_gated, AccessControl::MinTrustScore(_)));

    // Participants
    let participants = AccessControl::Participants(vec![]);
    assert!(matches!(participants, AccessControl::Participants(_)));
}

// ============================================================================
// GossipEntry Tests
// ============================================================================

#[test]
fn test_gossip_entry_creation() {
    let alice = KeyPair::generate().unwrap();
    let data = b"test data".to_vec();
    let hash = compute_hash(&data);

    let entry = GossipEntry {
        hash,
        author: alice.did().clone(),
        clock: VectorClock::new(),
        topic: "test:topic".to_string(),
        data: data.clone(),
        compressed: false,
        timestamp: 1000,
        replica_offered: None,
    };

    assert_eq!(entry.hash, hash);
    assert_eq!(entry.author, *alice.did());
    assert_eq!(entry.data, data);
    assert!(!entry.compressed);
}

#[test]
fn test_gossip_entry_compression() {
    let alice = KeyPair::generate().unwrap();

    // Create large compressible data (repeated pattern)
    let data = "Hello World! ".repeat(200).into_bytes(); // ~2.6KB

    let mut entry = GossipEntry {
        hash: compute_hash(&data),
        author: alice.did().clone(),
        clock: VectorClock::new(),
        topic: "test:topic".to_string(),
        data: data.clone(),
        compressed: false,
        timestamp: 1000,
        replica_offered: None,
    };

    let original_size = entry.data.len();

    // Compress
    entry.compress().unwrap();

    // Should be compressed and smaller
    assert!(entry.compressed);
    assert!(
        entry.data.len() < original_size,
        "Compressed size {} should be less than original {}",
        entry.data.len(),
        original_size
    );

    // Get data should return decompressed
    let retrieved = entry.get_data().unwrap();
    assert_eq!(retrieved, data);
}

#[test]
fn test_gossip_entry_small_data_no_compression() {
    let alice = KeyPair::generate().unwrap();

    // Small data (below compression threshold)
    let data = b"small".to_vec();

    let mut entry = GossipEntry {
        hash: compute_hash(&data),
        author: alice.did().clone(),
        clock: VectorClock::new(),
        topic: "test:topic".to_string(),
        data: data.clone(),
        compressed: false,
        timestamp: 1000,
        replica_offered: None,
    };

    // Compress should not change small data
    entry.compress().unwrap();

    assert!(!entry.compressed, "Small data should not be compressed");
    assert_eq!(entry.data, data);
}

#[test]
fn test_gossip_entry_decompress() {
    let alice = KeyPair::generate().unwrap();

    // Create compressible data
    let data = "Repeated pattern for compression. "
        .repeat(100)
        .into_bytes();

    let mut entry = GossipEntry {
        hash: compute_hash(&data),
        author: alice.did().clone(),
        clock: VectorClock::new(),
        topic: "test:topic".to_string(),
        data: data.clone(),
        compressed: false,
        timestamp: 1000,
        replica_offered: None,
    };

    // Compress
    entry.compress().unwrap();
    assert!(entry.compressed);

    // Decompress
    entry.decompress().unwrap();
    assert!(!entry.compressed);
    assert_eq!(entry.data, data);
}

// ============================================================================
// GossipMessage Tests
// ============================================================================

#[test]
fn test_gossip_message_announce() {
    let alice = KeyPair::generate().unwrap();
    let hash = compute_hash(b"test");
    let clock = VectorClock::new();

    let msg = GossipMessage::Announce {
        hash,
        author: alice.did().clone(),
        clock: clock.clone(),
        topic: "test:topic".to_string(),
    };

    match msg {
        GossipMessage::Announce {
            hash: h,
            author,
            clock: c,
            topic,
        } => {
            assert_eq!(h, hash);
            assert_eq!(author, *alice.did());
            assert_eq!(c, clock);
            assert_eq!(topic, "test:topic");
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_gossip_message_request_response() {
    let alice = KeyPair::generate().unwrap();
    let data = b"test data".to_vec();
    let hash = compute_hash(&data);

    // Request
    let request = GossipMessage::Request { hash };
    match &request {
        GossipMessage::Request { hash: h } => {
            assert_eq!(*h, hash);
        }
        _ => panic!("Wrong message type"),
    }

    // Response
    let entry = GossipEntry {
        hash,
        author: alice.did().clone(),
        clock: VectorClock::new(),
        topic: "test:topic".to_string(),
        data: data.clone(),
        compressed: false,
        timestamp: 1000,
        replica_offered: None,
    };

    let response = GossipMessage::Response {
        entry: entry.clone(),
    };
    match response {
        GossipMessage::Response { entry: e } => {
            assert_eq!(e.hash, hash);
            assert_eq!(e.data, data);
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_gossip_message_bloom_filter_request() {
    let topic = "test:topic";

    let request = GossipMessage::RequestBloomFilter {
        topic: topic.to_string(),
    };

    match request {
        GossipMessage::RequestBloomFilter { topic: t } => {
            assert_eq!(t, topic);
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_gossip_message_request_missing() {
    let hashes: Vec<[u8; 32]> = (0..5)
        .map(|i| compute_hash(format!("entry-{i}").as_bytes()))
        .collect();

    let msg = GossipMessage::RequestMissing {
        hashes: hashes.clone(),
    };

    match msg {
        GossipMessage::RequestMissing { hashes: h } => {
            assert_eq!(h.len(), 5);
            assert_eq!(h, hashes);
        }
        _ => panic!("Wrong message type"),
    }
}

// ============================================================================
// Stress Tests
// ============================================================================

#[test]
fn test_vector_clock_with_many_participants() {
    let participants: Vec<KeyPair> = (0..100).map(|_| KeyPair::generate().unwrap()).collect();

    let mut clock = VectorClock::new();

    // Each participant increments once
    for kp in &participants {
        clock.increment(kp.did());
    }

    // All should be at version 1
    for kp in &participants {
        assert_eq!(clock.get(kp.did()), 1);
    }

    // Increment first 10 again
    for kp in &participants[0..10] {
        clock.increment(kp.did());
    }

    // First 10 should be at 2, rest at 1
    for (i, kp) in participants.iter().enumerate() {
        if i < 10 {
            assert_eq!(clock.get(kp.did()), 2, "Participant {i} should be at 2");
        } else {
            assert_eq!(clock.get(kp.did()), 1, "Participant {i} should be at 1");
        }
    }
}

#[test]
fn test_bloom_filter_high_volume() {
    let mut filter = BloomFilter::new(100000, 0.01);

    // Insert 50000 entries
    let entries: Vec<[u8; 32]> = (0..50000)
        .map(|i| compute_hash(format!("entry-{i}").as_bytes()))
        .collect();

    for entry in &entries {
        filter.insert(entry);
    }

    // Verify all entries are found (no false negatives)
    for entry in &entries {
        assert!(filter.contains(entry), "Should find all inserted entries");
    }
}

#[test]
fn test_multiple_topics_isolation() {
    let alice = KeyPair::generate().unwrap();

    let topic_a = Topic::new("test:topic-a".to_string(), AccessControl::Public);
    let topic_b = Topic::new(
        "test:topic-b".to_string(),
        AccessControl::MinTrustScore(0.1),
    );
    let topic_c = Topic::new(
        "secure:topic".to_string(),
        AccessControl::Participants(vec![alice.did().clone()]),
    );

    // Verify topics are distinct
    assert_ne!(topic_a.name, topic_b.name);
    assert_ne!(topic_b.name, topic_c.name);

    // Verify access controls are different
    assert!(matches!(topic_a.acl, AccessControl::Public));
    assert!(matches!(topic_b.acl, AccessControl::MinTrustScore(_)));
    assert!(matches!(topic_c.acl, AccessControl::Participants(_)));
}

#[test]
fn test_gossip_entry_with_vector_clock() {
    let alice = KeyPair::generate().unwrap();
    let bob = KeyPair::generate().unwrap();

    let data = b"test data".to_vec();
    let hash = compute_hash(&data);

    // Entry with vector clock tracking multiple authors
    let mut clock = VectorClock::new();
    clock.increment(alice.did());
    clock.increment(bob.did());

    let entry = GossipEntry {
        hash,
        author: alice.did().clone(),
        clock: clock.clone(),
        topic: "test:topic".to_string(),
        data,
        compressed: false,
        timestamp: 1000,
        replica_offered: Some(true),
    };

    assert_eq!(entry.clock.get(alice.did()), 1);
    assert_eq!(entry.clock.get(bob.did()), 1);
    assert_eq!(entry.replica_offered, Some(true));
}
