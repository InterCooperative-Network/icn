#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Partition Detection and Healing Integration Tests
//!
//! Phase 18 Week 3: Tests for network partition detection, vector clock merging,
//! and conflict resolution when partitioned nodes reconnect.

use icn_gossip::{
    AccessControl, GossipActor, GossipMessage, PartitionConfig, PartitionDetector, PartitionHealer,
    Topic, VectorClock,
};
use icn_identity::KeyPair;
use icn_trust::TrustClass;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Helper to create a test GossipActor with partition detection enabled
fn create_test_gossip_with_partition(
    keypair: &KeyPair,
    partition_threshold_ms: u64,
) -> (
    GossipActor,
    Arc<RwLock<PartitionDetector>>,
    Arc<RwLock<PartitionHealer>>,
) {
    let did = keypair.did().clone();
    let trust_lookup = Arc::new(|_: &icn_identity::Did| Some(TrustClass::Partner));

    let mut gossip = GossipActor::new_with_legacy_trust(did, trust_lookup);

    // Configure partition detector with short threshold for testing
    let config = PartitionConfig {
        partition_threshold: Duration::from_millis(partition_threshold_ms),
        check_interval: Duration::from_millis(50),
    };
    let detector = Arc::new(RwLock::new(PartitionDetector::new(config)));
    let healer = Arc::new(RwLock::new(PartitionHealer::new(VectorClock::new())));

    gossip.set_partition_detector(detector.clone());
    gossip.set_partition_healer(healer.clone());

    (gossip, detector, healer)
}

#[tokio::test]
async fn test_partition_detection_basic() {
    // Create two nodes
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let (mut gossip1, detector1, _healer1) = create_test_gossip_with_partition(&kp1, 100);

    // Node 1 receives a message from Node 2 (simulates contact)
    detector1.write().await.record_contact(kp2.did());

    // Initially, Node 2 should NOT be partitioned
    assert!(
        !detector1.read().await.is_partitioned(kp2.did()),
        "Peer should not be partitioned immediately after contact"
    );

    // Wait for partition threshold to expire
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Now Node 2 should be considered partitioned
    assert!(
        detector1.read().await.is_partitioned(kp2.did()),
        "Peer should be partitioned after threshold expires"
    );

    // Record new contact
    detector1.write().await.record_contact(kp2.did());

    // Should no longer be partitioned
    assert!(
        !detector1.read().await.is_partitioned(kp2.did()),
        "Peer should not be partitioned after new contact"
    );

    // Verify we can create a topic
    gossip1.create_topic(Topic::new("test:topic".to_string(), AccessControl::Public));
    assert!(gossip1.get_topics().contains(&"test:topic".to_string()));
}

#[tokio::test]
async fn test_partition_heal_request_response() {
    // Create two nodes
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let (mut gossip1, _detector1, _healer1) = create_test_gossip_with_partition(&kp1, 100);
    let (mut gossip2, _detector2, _healer2) = create_test_gossip_with_partition(&kp2, 100);

    // Create a test topic on both nodes
    gossip1.create_topic(Topic::new("test:data".to_string(), AccessControl::Public));
    gossip2.create_topic(Topic::new("test:data".to_string(), AccessControl::Public));

    // Publish data on gossip2 to create version divergence
    let data = b"test data from node 2".to_vec();
    let hash = gossip2.publish("test:data", data).await.unwrap();

    // Set up message capture for gossip1
    let captured_messages = Arc::new(RwLock::new(Vec::new()));
    let captured_clone = captured_messages.clone();
    let send_callback: icn_gossip::SendMessageCallback = Arc::new(move |recipient, message| {
        let captured = captured_clone.clone();
        tokio::spawn(async move {
            captured.write().await.push((recipient, message));
        });
    });
    gossip1.set_send_callback(send_callback);

    // Node 1 initiates partition healing with Node 2
    gossip1.initiate_partition_healing(kp2.did());

    // Check that a PartitionHealRequest was sent
    tokio::time::sleep(Duration::from_millis(50)).await;
    let messages = captured_messages.read().await;
    assert!(
        !messages.is_empty(),
        "Should have sent at least one message"
    );

    // Find the PartitionHealRequest
    let heal_request = messages
        .iter()
        .find(|(_, msg)| matches!(msg, GossipMessage::PartitionHealRequest { .. }));
    assert!(
        heal_request.is_some(),
        "Should have sent a PartitionHealRequest"
    );

    // Verify the request contains expected data
    if let Some((
        recipient,
        GossipMessage::PartitionHealRequest {
            requesting_peer, ..
        },
    )) = heal_request
    {
        assert_eq!(
            recipient.as_ref().unwrap(),
            kp2.did(),
            "Request should be sent to peer 2"
        );
        assert_eq!(
            requesting_peer,
            kp1.did(),
            "Requesting peer should be node 1"
        );
    }

    // Now simulate gossip2 receiving the request and responding
    if let Some((_, GossipMessage::PartitionHealRequest { vector_clock, .. })) = heal_request {
        // Set up message capture for gossip2
        let captured2 = Arc::new(RwLock::new(Vec::new()));
        let captured2_clone = captured2.clone();
        let send_callback2: icn_gossip::SendMessageCallback =
            Arc::new(move |recipient, message| {
                let captured = captured2_clone.clone();
                tokio::spawn(async move {
                    captured.write().await.push((recipient, message));
                });
            });
        gossip2.set_send_callback(send_callback2);

        // Handle the request on gossip2
        gossip2
            .handle_message(
                kp1.did(),
                GossipMessage::PartitionHealRequest {
                    requesting_peer: kp1.did().clone(),
                    vector_clock: vector_clock.clone(),
                    last_contact_ms: 5000,
                },
            )
            .await
            .unwrap();

        // Check that a PartitionHealResponse was sent
        tokio::time::sleep(Duration::from_millis(50)).await;
        let messages2 = captured2.read().await;

        let heal_response = messages2
            .iter()
            .find(|(_, msg)| matches!(msg, GossipMessage::PartitionHealResponse { .. }));
        assert!(
            heal_response.is_some(),
            "Should have sent a PartitionHealResponse"
        );

        // Verify the response contains diverged topics info
        if let Some((
            _,
            GossipMessage::PartitionHealResponse {
                diverged_topics,
                entries_behind,
                ..
            },
        )) = heal_response
        {
            // gossip2 has one entry that gossip1 doesn't have
            assert!(
                *entries_behind > 0 || !diverged_topics.is_empty(),
                "Should detect divergence or entries behind"
            );
        }
    }

    println!("Entry hash on node 2: {:?}", hex::encode(hash));
}

#[tokio::test]
async fn test_vector_clock_merge_on_partition_heal() {
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let (mut gossip1, _detector1, _healer1) = create_test_gossip_with_partition(&kp1, 100);

    // Create topic
    gossip1.create_topic(Topic::new("test:merge".to_string(), AccessControl::Public));

    // Publish data to advance our vector clock
    gossip1
        .publish("test:merge", b"entry1".to_vec())
        .await
        .unwrap();
    gossip1
        .publish("test:merge", b"entry2".to_vec())
        .await
        .unwrap();

    // Get our current clock state
    let our_clock_before = gossip1.get_clock().clone();
    let our_version_before = our_clock_before.get(kp1.did());

    // Create a "remote" clock that's ahead on kp2
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(kp2.did());
    remote_clock.increment(kp2.did());
    remote_clock.increment(kp2.did());

    // Simulate receiving a PartitionHealResponse
    let result = gossip1
        .handle_message(
            kp2.did(),
            GossipMessage::PartitionHealResponse {
                responding_peer: kp2.did().clone(),
                vector_clock: remote_clock.clone(),
                diverged_topics: vec!["test:merge".to_string()],
                entries_behind: 3,
            },
        )
        .await;
    assert!(result.is_ok(), "Handle message should succeed");

    // Check that our clock was merged
    let our_clock_after = gossip1.get_clock().clone();

    // Our own entries should still be there
    assert_eq!(
        our_clock_after.get(kp1.did()),
        our_version_before,
        "Our own version should remain unchanged"
    );

    // Remote peer's version should now be in our clock
    assert_eq!(
        our_clock_after.get(kp2.did()),
        3,
        "Remote peer's version should be merged into our clock"
    );
}

#[tokio::test]
async fn test_partition_detector_multiple_peers() {
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();
    let kp3 = KeyPair::generate().unwrap();
    let kp4 = KeyPair::generate().unwrap();

    let (_gossip1, detector1, _healer1) = create_test_gossip_with_partition(&kp1, 100);

    // Record contact with all peers
    {
        let mut d = detector1.write().await;
        d.record_contact(kp2.did());
        d.record_contact(kp3.did());
        d.record_contact(kp4.did());
    }

    // All should be non-partitioned initially
    {
        let d = detector1.read().await;
        assert!(!d.is_partitioned(kp2.did()));
        assert!(!d.is_partitioned(kp3.did()));
        assert!(!d.is_partitioned(kp4.did()));
    }

    // Wait for threshold
    tokio::time::sleep(Duration::from_millis(150)).await;

    // All should now be partitioned
    {
        let d = detector1.read().await;
        let partitioned = d.get_partitioned_peers();
        assert_eq!(partitioned.len(), 3, "All 3 peers should be partitioned");
    }

    // Record contact for one peer
    detector1.write().await.record_contact(kp2.did());

    // Now only 2 should be partitioned
    {
        let d = detector1.read().await;
        let partitioned = d.get_partitioned_peers();
        assert_eq!(
            partitioned.len(),
            2,
            "Only 2 peers should be partitioned after updating kp2"
        );
        assert!(!d.is_partitioned(kp2.did()));
        assert!(d.is_partitioned(kp3.did()));
        assert!(d.is_partitioned(kp4.did()));
    }
}

#[tokio::test]
async fn test_partition_healer_conflict_resolution() {
    use icn_gossip::{Conflict, ConflictResolution, DataType, ResolutionOutcome};
    use std::time::SystemTime;

    let _kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let local_clock = VectorClock::new();
    let mut healer = PartitionHealer::new(local_clock);

    let now = SystemTime::now();
    let earlier = now - Duration::from_secs(10);

    // Create conflicts for different data types
    let conflicts = vec![
        // Gossip conflict (should keep both)
        Conflict {
            data_type: DataType::GossipEntry,
            content_hash: [1u8; 32],
            local_version: 1,
            remote_version: 1,
            local_timestamp: now,
            remote_timestamp: earlier,
            local_data: vec![1, 2, 3],
            remote_data: vec![4, 5, 6],
        },
        // Contract conflict (should use Last-Write-Wins)
        Conflict {
            data_type: DataType::Contract,
            content_hash: [2u8; 32],
            local_version: 1,
            remote_version: 2,
            local_timestamp: earlier,
            remote_timestamp: now,
            local_data: vec![7, 8, 9],
            remote_data: vec![10, 11, 12],
        },
        // Ledger conflict (should require manual resolution)
        Conflict {
            data_type: DataType::LedgerEntry,
            content_hash: [3u8; 32],
            local_version: 1,
            remote_version: 2,
            local_timestamp: now,
            remote_timestamp: earlier,
            local_data: vec![13, 14, 15],
            remote_data: vec![16, 17, 18],
        },
    ];

    let remote_clock = VectorClock::new();
    let outcomes = healer
        .heal_partition(kp2.did(), remote_clock, conflicts)
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 3);

    // Verify gossip conflict was resolved with KeepBoth
    if let ResolutionOutcome::Resolved { strategy, .. } = &outcomes[0] {
        assert_eq!(*strategy, ConflictResolution::KeepBoth);
    } else {
        panic!("Expected Resolved outcome for gossip");
    }

    // Verify contract conflict used LWW (remote newer)
    if let ResolutionOutcome::Resolved {
        strategy,
        kept_version,
    } = &outcomes[1]
    {
        assert_eq!(*strategy, ConflictResolution::LastWriteWins);
        assert_eq!(kept_version, "remote");
    } else {
        panic!("Expected Resolved outcome for contract");
    }

    // Verify ledger conflict requires manual resolution
    if let ResolutionOutcome::RequiresManual { .. } = &outcomes[2] {
        // Expected
    } else {
        panic!("Expected RequiresManual outcome for ledger");
    }
}
