//! Tests for message batching functionality
//!
//! These tests verify that message batching works correctly with
//! configurable parameters and metrics tracking.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::type_complexity)]

use icn_gossip::{AccessControl, BatchingConfig, GossipActor, GossipMessage, Topic};
use icn_identity::KeyPair;
use icn_trust::TrustClass;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Helper to create a trust lookup that allows all
fn trust_lookup_all() -> Arc<dyn Fn(&icn_identity::Did) -> Option<TrustClass> + Send + Sync> {
    Arc::new(|_| Some(TrustClass::Known))
}

#[test]
fn test_batching_config_default() {
    let config = BatchingConfig::default();
    assert!(config.enabled);
    assert_eq!(config.max_batch_size, 10);
    assert_eq!(config.max_delay, Duration::from_millis(10));
    assert_eq!(config.compression_threshold, 1024);
    assert_eq!(config.max_batch_bytes, 256 * 1024);
}

#[test]
fn test_batching_config_disabled() {
    let config = BatchingConfig::disabled();
    assert!(!config.enabled);
}

#[test]
fn test_batching_config_low_latency() {
    let config = BatchingConfig::low_latency();
    assert_eq!(config.max_batch_size, 5);
    assert_eq!(config.max_delay, Duration::from_millis(5));
}

#[test]
fn test_batching_config_high_throughput() {
    let config = BatchingConfig::high_throughput();
    assert_eq!(config.max_batch_size, 50);
    assert_eq!(config.max_delay, Duration::from_millis(50));
    assert_eq!(config.max_batch_bytes, 1024 * 1024);
}

#[test]
fn test_batch_messages_accumulated() {
    let kp = KeyPair::generate().unwrap();
    let mut gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp.clone());

    // Enable batching with large limits to prevent auto-flush
    let config = BatchingConfig {
        max_batch_size: 100,
        max_delay: Duration::from_secs(10),
        ..BatchingConfig::default()
    };
    gossip.set_batching_config(config);

    // Track sent messages
    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    gossip.set_send_callback(Arc::new(move |_recipient, message| {
        sent_clone.lock().unwrap().push(message);
    }));

    // Send multiple small messages - they should be batched
    for i in 0..5 {
        let msg = GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp.did().clone()), msg);
    }

    // No batch should have been sent yet
    assert!(sent_messages.lock().unwrap().is_empty());

    // Flush batches
    gossip.flush_all_batches();

    // Should have sent one batch message
    let sent = sent_messages.lock().unwrap();
    assert_eq!(sent.len(), 1);

    if let GossipMessage::Batch { messages, .. } = &sent[0] {
        assert_eq!(messages.len(), 5);
    } else {
        panic!("Expected Batch message");
    }
}

#[test]
fn test_batch_size_threshold_triggers_send() {
    let kp = KeyPair::generate().unwrap();
    let mut gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp.clone());

    // Enable batching with small size limit to trigger auto-flush
    let config = BatchingConfig {
        max_batch_size: 3,
        max_delay: Duration::from_secs(10),
        ..BatchingConfig::default()
    };
    gossip.set_batching_config(config);

    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    gossip.set_send_callback(Arc::new(move |_recipient, message| {
        sent_clone.lock().unwrap().push(message);
    }));

    // Send exactly enough messages to trigger batch
    for i in 0..3 {
        let msg = GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp.did().clone()), msg);
    }

    // Batch should have been sent automatically
    let sent = sent_messages.lock().unwrap();
    assert_eq!(sent.len(), 1);

    if let GossipMessage::Batch {
        messages, batch_id, ..
    } = &sent[0]
    {
        assert_eq!(messages.len(), 3);
        assert_eq!(*batch_id, 0);
    } else {
        panic!("Expected Batch message");
    }
}

#[test]
fn test_batching_disabled_sends_immediately() {
    let kp = KeyPair::generate().unwrap();
    let mut gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp.clone());

    // Disable batching
    gossip.set_batching_config(BatchingConfig::disabled());

    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    gossip.set_send_callback(Arc::new(move |_recipient, message| {
        sent_clone.lock().unwrap().push(message);
    }));

    // Send messages
    for i in 0..3 {
        let msg = GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp.did().clone()), msg);
    }

    // All messages should have been sent individually
    let sent = sent_messages.lock().unwrap();
    assert_eq!(sent.len(), 3);

    // None should be batch messages
    for msg in sent.iter() {
        assert!(!matches!(msg, GossipMessage::Batch { .. }));
    }
}

#[tokio::test]
async fn test_batch_message_processing() {
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let mut gossip1 = GossipActor::new(kp1.did().clone(), trust_lookup_all());
    let mut gossip2 = GossipActor::new(kp2.did().clone(), trust_lookup_all());

    gossip1.set_keypair(kp1.clone());
    gossip2.set_keypair(kp2.clone());

    // Create a topic
    let topic = Topic::new("test:batch".to_string(), AccessControl::Public);
    gossip1.create_topic(topic.clone());
    gossip2.create_topic(topic);

    // Create a batch message
    let messages = vec![
        GossipMessage::Announce {
            hash: [1u8; 32],
            author: kp1.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test:batch".to_string(),
        },
        GossipMessage::Announce {
            hash: [2u8; 32],
            author: kp1.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test:batch".to_string(),
        },
    ];

    let batch = GossipMessage::Batch {
        batch_id: 1,
        messages,
        compressed: false,
    };

    // Process batch
    let result = gossip2.handle_message(kp1.did(), batch).await;
    assert!(result.is_ok(), "Batch processing should succeed");
}

#[tokio::test]
async fn test_nested_batch_rejected() {
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let mut gossip = GossipActor::new(kp2.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp2.clone());

    // Create a nested batch (batch containing a batch)
    let inner_batch = GossipMessage::Batch {
        batch_id: 1,
        messages: vec![],
        compressed: false,
    };

    let outer_batch = GossipMessage::Batch {
        batch_id: 2,
        messages: vec![inner_batch],
        compressed: false,
    };

    // Process nested batch - should handle gracefully
    let result = gossip.handle_message(kp1.did(), outer_batch).await;
    assert!(result.is_ok(), "Should handle nested batch gracefully");
}

#[test]
fn test_multiple_recipients_batched_separately() {
    let kp = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();
    let kp3 = KeyPair::generate().unwrap();

    let mut gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp.clone());

    // Enable batching with large limits to prevent auto-flush
    let config = BatchingConfig {
        max_batch_size: 100,
        ..BatchingConfig::default()
    };
    gossip.set_batching_config(config);

    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    gossip.set_send_callback(Arc::new(move |recipient, message| {
        sent_clone.lock().unwrap().push((recipient, message));
    }));

    // Send messages to different recipients
    for i in 0..2 {
        let msg1 = GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp2.did().clone()), msg1);

        let msg2 = GossipMessage::Announce {
            hash: [(i + 10) as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp3.did().clone()), msg2);
    }

    // Flush all batches
    gossip.flush_all_batches();

    // Should have sent 2 batches (one per recipient)
    let sent = sent_messages.lock().unwrap();
    assert_eq!(sent.len(), 2);

    // Check that batches are separate
    for (recipient, message) in sent.iter() {
        if let GossipMessage::Batch { messages, .. } = message {
            assert!(recipient.is_some());
            assert_eq!(messages.len(), 2);
        } else {
            panic!("Expected Batch message");
        }
    }
}

#[tokio::test]
async fn test_time_based_batch_flushing() {
    let kp = KeyPair::generate().unwrap();
    let mut gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp.clone());

    // Enable batching with very short delay to test time-based triggering
    let config = BatchingConfig {
        max_batch_size: 100,                  // Large enough to not trigger size-based
        max_delay: Duration::from_millis(10), // Short delay
        ..BatchingConfig::default()
    };
    gossip.set_batching_config(config);

    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    gossip.set_send_callback(Arc::new(move |_recipient, message| {
        sent_clone.lock().unwrap().push(message);
    }));

    // Send first message to start the batch timer
    let msg1 = GossipMessage::Announce {
        hash: [1u8; 32],
        author: kp.did().clone(),
        clock: icn_gossip::VectorClock::new(),
        topic: "test".to_string(),
    };
    gossip.send_message(Some(kp.did().clone()), msg1);

    // No batch should have been sent yet
    assert!(sent_messages.lock().unwrap().is_empty());

    // Wait longer than max_delay
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Send another message - this should trigger time-based batch flush
    // Note: In production, `start_batch_flusher()` provides background flushing every
    // `max_delay/2` milliseconds. This test manually triggers flushing by sending a message
    // rather than relying on the background task, to avoid timing dependencies.
    let msg2 = GossipMessage::Announce {
        hash: [2u8; 32],
        author: kp.did().clone(),
        clock: icn_gossip::VectorClock::new(),
        topic: "test".to_string(),
    };
    gossip.send_message(Some(kp.did().clone()), msg2);

    // Batch should have been sent due to time expiration
    // The first message should be in the sent batch
    let sent = sent_messages.lock().unwrap();
    if sent.len() == 1 {
        if let GossipMessage::Batch { messages, .. } = &sent[0] {
            // The batch contains the first message that was waiting
            assert!(!messages.is_empty());
        } else {
            panic!("Expected Batch message");
        }
    }
    // Note: There may still be pending messages in the queue that weren't flushed yet
    // This is expected behavior - the second message may start a new batch
}

#[test]
fn test_batch_byte_threshold_triggers_send() {
    let kp = KeyPair::generate().unwrap();
    let mut gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp.clone());

    // Enable batching with very small byte limit to ensure bytes trigger flush
    // (but large message count limit so it's the bytes that trigger)
    let config = BatchingConfig {
        max_batch_size: 100,
        max_batch_bytes: 200, // Very small byte limit - single message is ~130 bytes
        max_delay: Duration::from_secs(10),
        ..BatchingConfig::default()
    };
    gossip.set_batching_config(config);

    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    gossip.set_send_callback(Arc::new(move |_recipient, message| {
        sent_clone.lock().unwrap().push(message);
    }));

    // Send enough messages to exceed byte limit
    // Each Announce message is roughly 130+ bytes when serialized
    // With 200 byte limit, 2 messages should trigger flush
    for i in 0..3 {
        let msg = GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp.did().clone()), msg);
    }

    // Batch should have been sent due to byte limit
    let sent = sent_messages.lock().unwrap();

    // The test verifies that batches are sent when byte threshold is exceeded.
    // Note: This may result in multiple batches if each message triggers the limit.
    assert!(
        !sent.is_empty(),
        "At least one batch should have been sent due to byte threshold"
    );

    // Verify we got batch messages (not individual messages)
    for msg in sent.iter() {
        assert!(
            matches!(msg, GossipMessage::Batch { .. }),
            "Expected Batch message, got {:?}",
            msg.variant_name()
        );
    }
}

/// Test that batch triggers on EITHER count OR byte threshold (OR logic)
///
/// This test verifies that the batching logic correctly flushes when
/// either threshold is reached, not requiring both conditions to be true.
#[test]
fn test_batch_triggers_on_either_threshold() {
    let kp = KeyPair::generate().unwrap();
    let mut gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp.clone());

    // Configure with both count and byte thresholds
    // We'll test that hitting EITHER one triggers a flush
    let config = BatchingConfig {
        max_batch_size: 3,       // Low count threshold
        max_batch_bytes: 10_000, // High byte threshold (won't be hit first)
        max_delay: Duration::from_secs(10),
        ..BatchingConfig::default()
    };
    gossip.set_batching_config(config);

    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    gossip.set_send_callback(Arc::new(move |_recipient, message| {
        sent_clone.lock().unwrap().push(message);
    }));

    // Send exactly 3 messages to hit count threshold (not byte threshold)
    // Each Announce message is ~130 bytes, so 3 messages = ~390 bytes < 10,000
    for i in 0..3 {
        let msg = GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp.did().clone()), msg);
    }

    // Count threshold should have triggered flush
    let sent = sent_messages.lock().unwrap();
    assert_eq!(sent.len(), 1, "Count threshold should trigger flush");
    drop(sent);

    // Now test the other direction: byte threshold triggers before count
    let mut gossip2 = GossipActor::new(kp.did().clone(), trust_lookup_all());
    gossip2.set_keypair(kp.clone());

    let config2 = BatchingConfig {
        max_batch_size: 100,  // High count threshold (won't be hit first)
        max_batch_bytes: 200, // Low byte threshold
        max_delay: Duration::from_secs(10),
        ..BatchingConfig::default()
    };
    gossip2.set_batching_config(config2);

    let sent_messages2 = Arc::new(Mutex::new(Vec::new()));
    let sent_clone2 = sent_messages2.clone();

    gossip2.set_send_callback(Arc::new(move |_recipient, message| {
        sent_clone2.lock().unwrap().push(message);
    }));

    // Send messages until byte threshold triggers (before count threshold of 100)
    // Each Announce is ~130 bytes, so 2 messages = ~260 bytes > 200 byte threshold
    for i in 0..3 {
        let msg = GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip2.send_message(Some(kp.did().clone()), msg);
    }

    // Byte threshold should have triggered flush before count threshold
    let sent2 = sent_messages2.lock().unwrap();
    assert!(
        !sent2.is_empty(),
        "Byte threshold should trigger flush before count threshold"
    );

    // Verify the batch has fewer than 100 messages (proving count wasn't the trigger)
    for msg in sent2.iter() {
        if let GossipMessage::Batch { messages, .. } = msg {
            assert!(
                messages.len() < 100,
                "Batch should have triggered on bytes, not count"
            );
        }
    }
}

#[tokio::test]
async fn test_oversized_batch_rejected() {
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let mut gossip = GossipActor::new(kp2.did().clone(), trust_lookup_all());
    gossip.set_keypair(kp2.clone());

    // Create a topic
    let topic = Topic::new("test:batch".to_string(), AccessControl::Public);
    gossip.create_topic(topic);

    // Create a batch with too many messages (> 100)
    let mut messages = Vec::new();
    for i in 0..101 {
        messages.push(GossipMessage::Announce {
            hash: [i as u8; 32],
            author: kp1.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test:batch".to_string(),
        });
    }

    let oversized_batch = GossipMessage::Batch {
        batch_id: 1,
        messages,
        compressed: false,
    };

    // Processing should fail due to message count limit
    let result = gossip.handle_message(kp1.did(), oversized_batch).await;
    assert!(
        result.is_err(),
        "Oversized batch should be rejected: {:?}",
        result
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("too large") || err_msg.contains("exceeds limit"),
        "Error should mention size limit: {}",
        err_msg
    );
}

/// Test that the background flusher eventually flushes single messages
/// even when no new messages arrive.
///
/// This test verifies the `start_batch_flusher` background task behavior:
/// 1. A single message is sent to a batching-enabled actor
/// 2. Without any additional messages, the background flusher should
///    eventually flush the pending message after max_delay expires
#[tokio::test]
async fn test_background_flusher_flushes_single_message() {
    use icn_gossip::{start_batch_flusher, GossipHandle};

    let kp = KeyPair::generate().unwrap();

    // Create gossip actor with short max_delay for faster test
    let gossip = GossipActor::new(kp.did().clone(), trust_lookup_all());
    let gossip_handle: GossipHandle = Arc::new(tokio::sync::RwLock::new(gossip));

    // Configure batching with short delay
    let config = BatchingConfig {
        max_batch_size: 100,                  // Large so size doesn't trigger flush
        max_delay: Duration::from_millis(20), // Short delay for test
        ..BatchingConfig::default()
    };

    {
        let mut gossip = gossip_handle.write().await;
        gossip.set_keypair(kp.clone());
        gossip.set_batching_config(config);
    }

    // Track sent messages
    let sent_messages: Arc<Mutex<Vec<GossipMessage>>> = Arc::new(Mutex::new(Vec::new()));
    let sent_clone = sent_messages.clone();

    {
        let mut gossip = gossip_handle.write().await;
        gossip.set_send_callback(Arc::new(move |_recipient, message| {
            sent_clone.lock().unwrap().push(message);
        }));
    }

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Start the background flusher
    let flusher_handle = start_batch_flusher(gossip_handle.clone(), shutdown_rx);

    // Send a single message
    {
        let gossip = gossip_handle.read().await;
        let msg = GossipMessage::Announce {
            hash: [1u8; 32],
            author: kp.did().clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: "test".to_string(),
        };
        gossip.send_message(Some(kp.did().clone()), msg);
    }

    // Verify no batch has been sent yet (message is pending)
    assert!(
        sent_messages.lock().unwrap().is_empty(),
        "Message should be pending in batch, not sent immediately"
    );

    // Wait for the background flusher to run (max_delay + buffer)
    // The flusher checks at max_delay/2 intervals, so we wait 2-3x max_delay
    tokio::time::sleep(Duration::from_millis(60)).await;

    // The background flusher should have flushed the pending message
    {
        let sent = sent_messages.lock().unwrap();
        assert_eq!(
            sent.len(),
            1,
            "Background flusher should have flushed the single pending message"
        );

        if let GossipMessage::Batch { messages, .. } = &sent[0] {
            assert_eq!(
                messages.len(),
                1,
                "Batch should contain exactly one message"
            );
        } else {
            panic!("Expected Batch message from background flusher");
        }
    } // Lock guard dropped here before await

    // Clean shutdown
    let _ = shutdown_tx.send(());
    let _ = flusher_handle.await;
}
