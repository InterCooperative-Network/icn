#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration test for distributed snapshot coordination
//!
//! Tests the Chandy-Lamport distributed snapshot protocol across multiple nodes.

use anyhow::Result;
use icn_identity::IdentityBundle;
use icn_snapshot::{SnapshotConfig, SnapshotCoordinator, SnapshotMessage};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_three_node_snapshot_coordination() -> Result<()> {
    // Initialize tracing for debugging
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    // Create three nodes with identities
    let node_a = IdentityBundle::generate()?;
    let node_b = IdentityBundle::generate()?;
    let node_c = IdentityBundle::generate()?;

    let did_a = node_a.did().clone();
    let did_b = node_b.did().clone();
    let did_c = node_c.did().clone();

    // Create snapshot coordinators for each node
    let coordinator_a = Arc::new(RwLock::new(SnapshotCoordinator::new(
        did_a.clone(),
        SnapshotConfig::default(),
    )));
    let coordinator_b = Arc::new(RwLock::new(SnapshotCoordinator::new(
        did_b.clone(),
        SnapshotConfig::default(),
    )));
    let coordinator_c = Arc::new(RwLock::new(SnapshotCoordinator::new(
        did_c.clone(),
        SnapshotConfig::default(),
    )));

    // Node A initiates a snapshot with all three nodes as participants
    let participants = vec![did_a.clone(), did_b.clone(), did_c.clone()];
    let (snapshot_id, init_msg) = coordinator_a
        .write()
        .await
        .initiate_snapshot(participants.clone())
        .await?;

    println!("✓ Node A initiated snapshot: {}", snapshot_id.to_hex());

    // Verify initiate message
    match &init_msg {
        SnapshotMessage::InitiateSnapshot {
            snapshot_id: msg_id,
            initiator,
            participants: msg_participants,
            ..
        } => {
            assert_eq!(msg_id, &snapshot_id);
            assert_eq!(initiator, &did_a);
            assert_eq!(msg_participants, &participants);
        }
        _ => panic!("Expected InitiateSnapshot message"),
    }

    // Simulate gossip broadcast: Node B and C receive the InitiateSnapshot message
    let responses_b = coordinator_b
        .write()
        .await
        .handle_message(&did_a, init_msg.clone())
        .await?;

    let responses_c = coordinator_c
        .write()
        .await
        .handle_message(&did_a, init_msg.clone())
        .await?;

    println!("✓ Node B and C received initiate message");

    // Verify Node B sent ACK and Marker
    assert_eq!(responses_b.len(), 2, "Node B should send ACK + Marker");
    assert!(matches!(
        responses_b[0],
        SnapshotMessage::SnapshotAck { .. }
    ));
    assert!(matches!(responses_b[1], SnapshotMessage::Marker { .. }));

    // Verify Node C sent ACK and Marker
    assert_eq!(responses_c.len(), 2, "Node C should send ACK + Marker");
    assert!(matches!(
        responses_c[0],
        SnapshotMessage::SnapshotAck { .. }
    ));
    assert!(matches!(responses_c[1], SnapshotMessage::Marker { .. }));

    println!("✓ Node B and C sent ACK + Marker");

    // Node A receives ACKs from B and C
    // Extract ACKs
    let ack_b = responses_b[0].clone();
    let ack_c = responses_c[0].clone();

    // Node A also needs to ACK itself (since it's a participant)
    let responses_a_self = coordinator_a
        .write()
        .await
        .handle_message(&did_a, init_msg.clone())
        .await?;

    assert_eq!(responses_a_self.len(), 2);
    let ack_a = responses_a_self[0].clone();

    // Node A receives all ACKs
    let response_after_ack_a = coordinator_a
        .write()
        .await
        .handle_message(&did_a, ack_a)
        .await?;

    let response_after_ack_b = coordinator_a
        .write()
        .await
        .handle_message(&did_b, ack_b)
        .await?;

    let response_after_ack_c = coordinator_a
        .write()
        .await
        .handle_message(&did_c, ack_c)
        .await?;

    // After all ACKs received, coordinator should send SnapshotComplete
    let complete_messages: Vec<_> = response_after_ack_a
        .into_iter()
        .chain(response_after_ack_b)
        .chain(response_after_ack_c)
        .collect();

    println!("✓ Node A received all ACKs");

    // Find the SnapshotComplete message
    let complete_msg = complete_messages
        .iter()
        .find(|msg| matches!(msg, SnapshotMessage::SnapshotComplete { .. }));

    assert!(
        complete_msg.is_some(),
        "Coordinator should send SnapshotComplete after all ACKs"
    );

    println!("✓ Node A sent SnapshotComplete");

    if let Some(SnapshotMessage::SnapshotComplete {
        snapshot_id: complete_id,
        coordinator,
        global_state_root,
        participants: complete_participants,
    }) = complete_msg
    {
        assert_eq!(complete_id, &snapshot_id);
        assert_eq!(coordinator, &did_a);
        assert_eq!(complete_participants, &participants);
        println!("✓ Global state root: {}", hex::encode(global_state_root));
    }

    // Nodes B and C receive SnapshotComplete
    if let Some(complete) = complete_msg {
        coordinator_b
            .write()
            .await
            .handle_message(&did_a, complete.clone())
            .await?;

        coordinator_c
            .write()
            .await
            .handle_message(&did_a, complete.clone())
            .await?;

        // Node A also needs to handle its own SnapshotComplete
        coordinator_a
            .write()
            .await
            .handle_message(&did_a, complete.clone())
            .await?;
    }

    // Verify snapshot is complete on all nodes
    assert!(
        coordinator_a.read().await.is_complete(&snapshot_id).await,
        "Node A should mark snapshot as complete"
    );
    assert!(
        coordinator_b.read().await.is_complete(&snapshot_id).await,
        "Node B should mark snapshot as complete"
    );
    assert!(
        coordinator_c.read().await.is_complete(&snapshot_id).await,
        "Node C should mark snapshot as complete"
    );

    // Give async tasks time to complete
    sleep(Duration::from_millis(100)).await;

    println!("✅ Three-node distributed snapshot completed successfully");

    Ok(())
}

#[tokio::test]
async fn test_insufficient_participants() -> Result<()> {
    let node_a = IdentityBundle::generate()?;
    let node_b = IdentityBundle::generate()?;

    let did_a = node_a.did().clone();
    let did_b = node_b.did().clone();

    let coordinator = SnapshotCoordinator::new(did_a.clone(), SnapshotConfig::default());

    // Try to initiate with only 2 participants (need 3 by default)
    let participants = vec![did_a, did_b];
    let result = coordinator.initiate_snapshot(participants).await;

    assert!(
        result.is_err(),
        "Should fail with insufficient participants"
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Insufficient participants"));

    println!("✅ Insufficient participants check passed");

    Ok(())
}

#[tokio::test]
async fn test_snapshot_marker_convergence() -> Result<()> {
    // Create two nodes
    let node_a = IdentityBundle::generate()?;
    let node_b = IdentityBundle::generate()?;
    let node_c = IdentityBundle::generate()?;

    let did_a = node_a.did().clone();
    let did_b = node_b.did().clone();
    let did_c = node_c.did().clone();

    let coordinator_a = Arc::new(RwLock::new(SnapshotCoordinator::new(
        did_a.clone(),
        SnapshotConfig::default(),
    )));

    // Initiate snapshot
    let participants = vec![did_a.clone(), did_b.clone(), did_c.clone()];
    let (snapshot_id, init_msg) = coordinator_a
        .write()
        .await
        .initiate_snapshot(participants)
        .await?;

    // Simulate node A handling its own initiate (as a participant)
    let responses = coordinator_a
        .write()
        .await
        .handle_message(&did_a, init_msg)
        .await?;

    // Should get ACK + Marker
    assert_eq!(responses.len(), 2);
    let marker = &responses[1];

    // Verify marker message
    if let SnapshotMessage::Marker {
        snapshot_id: marker_id,
        sender,
    } = marker
    {
        assert_eq!(marker_id, &snapshot_id);
        assert_eq!(sender, &did_a);
    } else {
        panic!("Expected Marker message");
    }

    println!("✅ Marker generation verified");

    Ok(())
}

#[tokio::test]
async fn test_snapshot_active_and_completed_counts() -> Result<()> {
    let node_a = IdentityBundle::generate()?;
    let node_b = IdentityBundle::generate()?;
    let node_c = IdentityBundle::generate()?;

    let did_a = node_a.did().clone();
    let did_b = node_b.did().clone();
    let did_c = node_c.did().clone();

    let coordinator = Arc::new(RwLock::new(SnapshotCoordinator::new(
        did_a.clone(),
        SnapshotConfig::default(),
    )));

    assert_eq!(coordinator.read().await.active_count().await, 0);
    assert_eq!(coordinator.read().await.completed_count().await, 0);

    // Initiate snapshot
    let participants = vec![did_a.clone(), did_b.clone(), did_c.clone()];
    let (_snapshot_id, _init_msg) = coordinator
        .write()
        .await
        .initiate_snapshot(participants)
        .await?;

    // Should have 1 active snapshot
    assert_eq!(coordinator.read().await.active_count().await, 1);
    assert_eq!(coordinator.read().await.completed_count().await, 0);

    println!("✅ Snapshot count tracking verified");

    Ok(())
}
