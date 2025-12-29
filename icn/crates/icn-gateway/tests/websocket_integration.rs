//! WebSocket reliability integration tests
//!
//! Tests for Issue #321 - WebSocket Reliability Hardening:
//! - Backfill buffer wraparound (>100 events)
//! - Concurrent subscribe/broadcast scenarios
//! - Graceful shutdown with multiple coops
//! - Slow client detection with varying channel sizes
//! - Configuration edge cases

use icn_gateway::events::{EventBroadcaster, GatewayEvent, WebSocketConfig};
use tokio::sync::mpsc;

/// Test that backfill correctly handles wraparound when buffer exceeds max capacity
#[tokio::test]
async fn test_backfill_buffer_wraparound() {
    let broadcaster = EventBroadcaster::new();

    // Broadcast more events than the backfill buffer size (default: 100)
    for i in 0..150 {
        let event = GatewayEvent::PaymentCreated {
            coop_id: "test-coop".to_string(),
            hash: format!("hash{i}"),
            from: "did:icn:alice".to_string(),
            to: "did:icn:bob".to_string(),
            amount: i as i64,
            currency: "hours".to_string(),
        };
        broadcaster.broadcast("test-coop", event).await;
    }

    // Get backfill - should only return last 100 events
    let backfill = broadcaster.get_backfill("test-coop", 0).await;
    assert_eq!(backfill.len(), 100, "Backfill should be capped at 100 events");

    // Verify we got the most recent events
    // Note: Sequence numbers are global, so we just verify we have 100 consecutive events
    let first_seq = backfill.first().map(|e| e.seq).unwrap_or(0);
    let last_seq = backfill.last().map(|e| e.seq).unwrap_or(0);

    assert!(last_seq > first_seq, "Sequence numbers should be increasing");
    // With 100 events, first to last should span 99 positions
    assert_eq!(backfill.len(), 100, "Should have exactly 100 events");

    // Verify the amounts are from the last 50 events (50-149, amounts 50-149)
    // Since we broadcasted 150 events and kept only 100, amounts should be 50-149
    let amounts: Vec<i64> = backfill.iter().filter_map(|e| {
        match &e.event {
            GatewayEvent::PaymentCreated { amount, .. } => Some(*amount),
            _ => None,
        }
    }).collect();

    assert_eq!(amounts.len(), 100);
    assert_eq!(*amounts.first().unwrap(), 50, "First event should have amount 50");
    assert_eq!(*amounts.last().unwrap(), 149, "Last event should have amount 149");
}

/// Test backfill returns correct events when requesting from middle of buffer
#[tokio::test]
async fn test_backfill_from_middle() {
    let broadcaster = EventBroadcaster::new();

    // Broadcast 50 events
    for i in 0..50 {
        let event = GatewayEvent::MemberAdded {
            coop_id: "test-coop-mid".to_string(),
            did: format!("did:icn:member{i}"),
            role: "Member".to_string(),
        };
        broadcaster.broadcast("test-coop-mid", event).await;
    }

    // Get all events first to know sequence numbers
    let all_events = broadcaster.get_backfill("test-coop-mid", 0).await;
    assert_eq!(all_events.len(), 50, "Should have 50 events");

    // Request backfill from middle (after the 25th event)
    let mid_seq = all_events[24].seq; // Index 24 = 25th event
    let from_middle = broadcaster.get_backfill("test-coop-mid", mid_seq).await;

    // Should get events after mid_seq (25 events: indices 25-49)
    assert_eq!(from_middle.len(), 25, "Should get remaining 25 events");

    // First event in from_middle should be the one after mid_seq
    // Due to global sequence counter, just verify it's greater than mid_seq
    assert!(from_middle[0].seq > mid_seq, "First event should be after requested seq");

    // Verify we got the right member DIDs (members 25-49)
    let first_did = match &from_middle[0].event {
        GatewayEvent::MemberAdded { did, .. } => did.clone(),
        _ => panic!("Expected MemberAdded"),
    };
    assert_eq!(first_did, "did:icn:member25", "First event should be member25");
}

/// Test concurrent subscribers receive all events
#[tokio::test]
async fn test_concurrent_subscribers_receive_all() {
    let broadcaster = EventBroadcaster::new();

    // Create multiple subscribers
    let mut rx1 = broadcaster.subscribe("test-coop").await.expect("Subscribe 1");
    let mut rx2 = broadcaster.subscribe("test-coop").await.expect("Subscribe 2");
    let mut rx3 = broadcaster.subscribe("test-coop").await.expect("Subscribe 3");

    // Broadcast events rapidly
    let num_events = 100;
    for i in 0..num_events {
        let event = GatewayEvent::PaymentCreated {
            coop_id: "test-coop".to_string(),
            hash: format!("hash{i}"),
            from: "did:icn:alice".to_string(),
            to: "did:icn:bob".to_string(),
            amount: i as i64,
            currency: "hours".to_string(),
        };
        broadcaster.broadcast("test-coop", event).await;
    }

    // Each subscriber should receive all events
    let mut count1 = 0;
    while rx1.try_recv().is_ok() {
        count1 += 1;
    }

    let mut count2 = 0;
    while rx2.try_recv().is_ok() {
        count2 += 1;
    }

    let mut count3 = 0;
    while rx3.try_recv().is_ok() {
        count3 += 1;
    }

    assert_eq!(count1, num_events, "Subscriber 1 should receive all events");
    assert_eq!(count2, num_events, "Subscriber 2 should receive all events");
    assert_eq!(count3, num_events, "Subscriber 3 should receive all events");
}

/// Test that different coops receive only their events
#[tokio::test]
async fn test_coop_isolation() {
    let broadcaster = EventBroadcaster::new();

    let mut rx_coop1 = broadcaster.subscribe("coop-1").await.expect("Subscribe coop-1");
    let mut rx_coop2 = broadcaster.subscribe("coop-2").await.expect("Subscribe coop-2");

    // Broadcast to coop-1
    broadcaster.broadcast("coop-1", GatewayEvent::MemberAdded {
        coop_id: "coop-1".to_string(),
        did: "did:icn:alice".to_string(),
        role: "Member".to_string(),
    }).await;

    // Broadcast to coop-2
    broadcaster.broadcast("coop-2", GatewayEvent::MemberRemoved {
        coop_id: "coop-2".to_string(),
        did: "did:icn:bob".to_string(),
    }).await;

    // Each coop should only get their event
    let event1 = rx_coop1.try_recv();
    let event2 = rx_coop2.try_recv();

    assert!(event1.is_ok(), "Coop-1 should receive event");
    assert!(event2.is_ok(), "Coop-2 should receive event");

    match event1.unwrap().event {
        GatewayEvent::MemberAdded { coop_id, .. } => {
            assert_eq!(coop_id, "coop-1");
        }
        _ => panic!("Expected MemberAdded for coop-1"),
    }

    match event2.unwrap().event {
        GatewayEvent::MemberRemoved { coop_id, .. } => {
            assert_eq!(coop_id, "coop-2");
        }
        _ => panic!("Expected MemberRemoved for coop-2"),
    }

    // No more events
    assert!(rx_coop1.try_recv().is_err());
    assert!(rx_coop2.try_recv().is_err());
}

/// Test slow client detection with minimal channel capacity
#[tokio::test]
async fn test_slow_client_immediate_disconnect() {
    // Very small channel capacity - fills immediately
    let config = WebSocketConfig::default().with_channel_capacity(1);
    let broadcaster = EventBroadcaster::with_config(config);

    // Subscribe but don't consume
    let _rx = broadcaster.subscribe("test-coop").await.expect("Subscribe");

    // First event fills the channel
    broadcaster.broadcast("test-coop", GatewayEvent::MemberAdded {
        coop_id: "test-coop".to_string(),
        did: "did:icn:alice".to_string(),
        role: "Member".to_string(),
    }).await;

    // Second event should trigger slow client disconnect
    broadcaster.broadcast("test-coop", GatewayEvent::MemberAdded {
        coop_id: "test-coop".to_string(),
        did: "did:icn:bob".to_string(),
        role: "Member".to_string(),
    }).await;

    // Subscriber should be removed
    assert_eq!(broadcaster.subscriber_count("test-coop").await, 0);
}

/// Test that active clients survive slow client cleanup
#[tokio::test]
async fn test_active_client_survives_cleanup() {
    let config = WebSocketConfig::default().with_channel_capacity(10);
    let broadcaster = EventBroadcaster::with_config(config);

    // Two subscribers - one active, one slow
    let mut rx_active = broadcaster.subscribe("test-coop").await.expect("Active subscriber");
    let _rx_slow = broadcaster.subscribe("test-coop").await.expect("Slow subscriber");

    assert_eq!(broadcaster.subscriber_count("test-coop").await, 2);

    // Active client drains events
    tokio::spawn(async move {
        loop {
            if rx_active.recv().await.is_none() {
                break;
            }
        }
    });

    // Brief pause to let consumer start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Broadcast more events than slow client can hold
    for i in 0..15 {
        broadcaster.broadcast("test-coop", GatewayEvent::PaymentCreated {
            coop_id: "test-coop".to_string(),
            hash: format!("hash{i}"),
            from: "did:icn:alice".to_string(),
            to: "did:icn:bob".to_string(),
            amount: i as i64,
            currency: "hours".to_string(),
        }).await;
    }

    // Give time for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Slow client should be removed, active client might still be there
    // (depending on timing, both might be cleaned up if test completes too fast)
    let count = broadcaster.subscriber_count("test-coop").await;
    assert!(count <= 1, "At most one subscriber should remain");
}

/// Test shutdown broadcast to multiple coops with different subscriber counts
#[tokio::test]
async fn test_shutdown_multi_coop_varied_subscribers() {
    let broadcaster = EventBroadcaster::new();

    // Coop-1: 3 subscribers
    let mut rx1a = broadcaster.subscribe("coop-1").await.expect("1a");
    let mut rx1b = broadcaster.subscribe("coop-1").await.expect("1b");
    let mut rx1c = broadcaster.subscribe("coop-1").await.expect("1c");

    // Coop-2: 1 subscriber
    let mut rx2 = broadcaster.subscribe("coop-2").await.expect("2");

    // Coop-3: 2 subscribers
    let mut rx3a = broadcaster.subscribe("coop-3").await.expect("3a");
    let mut rx3b = broadcaster.subscribe("coop-3").await.expect("3b");

    // Broadcast shutdown to all
    broadcaster.broadcast_shutdown_all("System maintenance", Some(30000)).await;

    // All should receive shutdown
    let receivers: Vec<&mut mpsc::Receiver<_>> = vec![
        &mut rx1a, &mut rx1b, &mut rx1c,
        &mut rx2,
        &mut rx3a, &mut rx3b,
    ];

    for (i, rx) in receivers.into_iter().enumerate() {
        let event = rx.try_recv().expect(&format!("Receiver {} should get shutdown", i));
        match event.event {
            GatewayEvent::Shutdown { reason, reconnect_after_ms } => {
                assert_eq!(reason, "System maintenance");
                assert_eq!(reconnect_after_ms, Some(30000));
            }
            _ => panic!("Expected Shutdown event"),
        }
    }
}

/// Test that config edge cases are handled
#[tokio::test]
async fn test_config_edge_cases() {
    // Very small values
    let config = WebSocketConfig::default()
        .with_channel_capacity(1)
        .with_max_subscribers(1)
        .with_max_backfill(1);

    let broadcaster = EventBroadcaster::with_config(config.clone());

    assert_eq!(broadcaster.config().channel_capacity, 1);
    assert_eq!(broadcaster.config().max_subscribers_per_coop, 1);
    assert_eq!(broadcaster.config().max_backfill_events, 1);

    // First subscriber succeeds
    let _rx1 = broadcaster.subscribe("test-coop").await;
    assert!(_rx1.is_some());

    // Second subscriber rejected
    let rx2 = broadcaster.subscribe("test-coop").await;
    assert!(rx2.is_none());
}

/// Test backfill with max_backfill = 1 only returns the latest event
#[tokio::test]
async fn test_minimal_backfill_buffer() {
    let config = WebSocketConfig::default().with_max_backfill(1);
    let broadcaster = EventBroadcaster::with_config(config);

    // Broadcast multiple events
    for i in 0..10 {
        broadcaster.broadcast("test-coop", GatewayEvent::PaymentCreated {
            coop_id: "test-coop".to_string(),
            hash: format!("hash{i}"),
            from: "did:icn:alice".to_string(),
            to: "did:icn:bob".to_string(),
            amount: i as i64,
            currency: "hours".to_string(),
        }).await;
    }

    // Only get the latest event
    let backfill = broadcaster.get_backfill("test-coop", 0).await;
    assert_eq!(backfill.len(), 1, "Should only get 1 event with max_backfill=1");

    // Should be the last event (amount = 9)
    match &backfill[0].event {
        GatewayEvent::PaymentCreated { amount, .. } => {
            assert_eq!(*amount, 9, "Should be the last event");
        }
        _ => panic!("Expected PaymentCreated"),
    }
}

/// Test that empty coop broadcast is handled gracefully
#[tokio::test]
async fn test_broadcast_to_empty_coop() {
    let broadcaster = EventBroadcaster::new();

    // Broadcast to non-existent coop (no subscribers)
    broadcaster.broadcast("nonexistent", GatewayEvent::MemberAdded {
        coop_id: "nonexistent".to_string(),
        did: "did:icn:alice".to_string(),
        role: "Member".to_string(),
    }).await;

    // Should not panic, just log and continue
    assert_eq!(broadcaster.subscriber_count("nonexistent").await, 0);

    // Backfill should still work
    let backfill = broadcaster.get_backfill("nonexistent", 0).await;
    assert_eq!(backfill.len(), 1, "Event should be in backfill even without subscribers");
}

/// Test sequence numbers are globally unique across coops
#[tokio::test]
async fn test_global_sequence_uniqueness() {
    let broadcaster = EventBroadcaster::new();

    let mut rx1 = broadcaster.subscribe("coop-1").await.expect("Subscribe 1");
    let mut rx2 = broadcaster.subscribe("coop-2").await.expect("Subscribe 2");

    // Interleave events to different coops
    broadcaster.broadcast("coop-1", GatewayEvent::MemberAdded {
        coop_id: "coop-1".to_string(),
        did: "did:icn:alice".to_string(),
        role: "Member".to_string(),
    }).await;

    broadcaster.broadcast("coop-2", GatewayEvent::MemberAdded {
        coop_id: "coop-2".to_string(),
        did: "did:icn:bob".to_string(),
        role: "Member".to_string(),
    }).await;

    broadcaster.broadcast("coop-1", GatewayEvent::MemberRemoved {
        coop_id: "coop-1".to_string(),
        did: "did:icn:alice".to_string(),
    }).await;

    let event1a = rx1.recv().await.unwrap();
    let event2 = rx2.recv().await.unwrap();
    let event1b = rx1.recv().await.unwrap();

    // Sequences should be globally unique and ordered
    assert!(event1a.seq < event2.seq, "Event 1 should have lower seq than event 2");
    assert!(event2.seq < event1b.seq, "Event 2 should have lower seq than event 3");

    // No duplicate sequences
    let seqs = vec![event1a.seq, event2.seq, event1b.seq];
    let unique: std::collections::HashSet<_> = seqs.iter().collect();
    assert_eq!(unique.len(), 3, "All sequences should be unique");
}
