//! Integration test for compute event → WebSocket delivery
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! This test demonstrates the complete flow:
//! 1. Compute actor emits events via callback
//! 2. Events forwarded to EventBroadcaster
//! 3. WebSocket subscribers receive events in real-time

use icn_compute::{
    ComputeActor, ComputeTask, ExecutorCapability, FuelLimit, TaskCode, TaskPriority, TrustCallback,
};
use icn_gateway::{create_forwarding_callback, EventBroadcaster, GatewayEvent};
use std::sync::Arc;

/// Valid test DIDs (proper multibase-encoded Ed25519 public keys)
/// Generated from deterministic seeds: alice=[1u8;32], executor=[2u8;32]
const TEST_EXECUTOR_DID: &str = "did:icn:z9hSR6S7WPtxmTojgo6GG3k4yDPecgJY292j7xrsUGWBu";
const TEST_ALICE_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";

/// Simple CCL contract that returns a constant
fn simple_ccl() -> String {
    format!(
        r#"{{
        "name": "SimpleReturn",
        "participants": ["{TEST_ALICE_DID}"],
        "currency": null,
        "state_vars": [],
        "rules": [{{
            "name": "run",
            "params": [],
            "requires": [],
            "body": [{{ "Return": {{ "value": {{ "Literal": {{ "Int": 42 }} }} }} }}]
        }}],
        "triggers": []
    }}"#
    )
}

#[tokio::test]
async fn test_compute_events_to_websocket() {
    // 1. Create event broadcaster (what the gateway would have)
    let broadcaster = Arc::new(EventBroadcaster::new());

    // 2. Subscribe to compute channel (what a WebSocket client would do)
    let mut event_rx = broadcaster
        .subscribe("compute")
        .await
        .expect("Should subscribe to compute channel");

    // 3. Create compute actor with event forwarding callback
    let trust_cb: TrustCallback = Arc::new(|_| 0.8); // High trust
    let mut compute_actor = ComputeActor::new(TEST_EXECUTOR_DID.to_string(), trust_cb);

    // Set up dummy send callback (required for actor to function)
    let send_callback: icn_compute::SendCallback = Arc::new(|_msg| {
        // In a real integration, this would send via gossip
        // For this test, we just need it to exist
    });
    compute_actor.set_send_callback(send_callback);

    // Set up event callback to forward to broadcaster
    let event_callback = create_forwarding_callback(broadcaster.clone());
    compute_actor.set_event_callback(event_callback);

    // Set signing key for result signatures
    compute_actor.set_signing_key(vec![1u8; 32]);

    // Spawn the actor
    let handle = compute_actor.spawn();

    // 4. Submit a task (this will trigger claim + completion events)
    let task = ComputeTask {
        id: "test-task".to_string(),
        submitter: TEST_ALICE_DID.to_string(),
        coop_id: None,
        code: TaskCode::Ccl(simple_ccl()),
        inputs: vec![],
        fuel_limit: FuelLimit(10_000),
        required_capabilities: vec![ExecutorCapability::Ccl],
        priority: TaskPriority::Normal,
        created_at: 1000,
        deadline: None,
        payment_rate: None,
        payment_currency: None,
        resource_profile: None,
        actor_mode: None,
        placement_constraints: None,
        estimated_value: None,
        verification: None,
    };

    let task_hash = handle
        .submit(task.clone())
        .await
        .expect("Should submit task");
    let expected_hash = hex::encode(task_hash);

    // Simulate gossip broadcast loop-back (in real system, gossip would broadcast and come back)
    handle
        .handle_gossip(icn_compute::ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .expect("Should handle gossip message");

    // Give async tasks time to process (execution + event forwarding)
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    tokio::task::yield_now().await; // Allow spawned tasks to run

    // 5. Verify we received TaskClaimed event
    let claimed_event = tokio::time::timeout(tokio::time::Duration::from_secs(1), event_rx.recv())
        .await
        .expect("Should receive event within timeout")
        .expect("Should receive claimed event");

    match claimed_event.event {
        GatewayEvent::ComputeTaskClaimed {
            task_hash,
            executor,
        } => {
            assert_eq!(task_hash, expected_hash, "Task hash should match");
            assert_eq!(executor, TEST_EXECUTOR_DID, "Executor should match");
        }
        other => panic!("Expected ComputeTaskClaimed, got {other:?}"),
    }

    // 6. Verify we received TaskCompleted event
    let completed_event =
        tokio::time::timeout(tokio::time::Duration::from_secs(1), event_rx.recv())
            .await
            .expect("Should receive event within timeout")
            .expect("Should receive completed event");

    match completed_event.event {
        GatewayEvent::ComputeTaskCompleted {
            task_hash,
            executor,
            outcome,
            ..
        } => {
            assert_eq!(task_hash, expected_hash, "Task hash should match");
            assert_eq!(executor, TEST_EXECUTOR_DID, "Executor should match");
            assert_eq!(outcome, "success", "Outcome should be success");
        }
        other => panic!("Expected ComputeTaskCompleted, got {other:?}"),
    }
}

#[tokio::test]
async fn test_multiple_subscribers_receive_events() {
    // Create broadcaster
    let broadcaster = Arc::new(EventBroadcaster::new());

    // Multiple subscribers (simulating multiple WebSocket clients)
    let mut client1 = broadcaster
        .subscribe("compute")
        .await
        .expect("Client 1 should subscribe");

    let mut client2 = broadcaster
        .subscribe("compute")
        .await
        .expect("Client 2 should subscribe");

    let mut client3 = broadcaster
        .subscribe("compute")
        .await
        .expect("Client 3 should subscribe");

    // Create compute actor with forwarding
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut compute_actor = ComputeActor::new(TEST_EXECUTOR_DID.to_string(), trust_cb);

    // Set dummy send callback
    let send_callback: icn_compute::SendCallback = Arc::new(|_msg| {});
    compute_actor.set_send_callback(send_callback);

    compute_actor.set_event_callback(create_forwarding_callback(broadcaster));
    compute_actor.set_signing_key(vec![1u8; 32]);

    let handle = compute_actor.spawn();

    // Submit task
    let task = ComputeTask {
        id: "broadcast-test".to_string(),
        submitter: TEST_ALICE_DID.to_string(),
        coop_id: None,
        code: TaskCode::Ccl(simple_ccl()),
        inputs: vec![],
        fuel_limit: FuelLimit(10_000),
        required_capabilities: vec![ExecutorCapability::Ccl],
        priority: TaskPriority::Normal,
        created_at: 1000,
        deadline: None,
        payment_rate: None,
        payment_currency: None,
        resource_profile: None,
        actor_mode: None,
        placement_constraints: None,
        estimated_value: None,
        verification: None,
    };

    handle
        .submit(task.clone())
        .await
        .expect("Should submit task");

    // Simulate gossip broadcast loop-back
    handle
        .handle_gossip(icn_compute::ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .expect("Should handle gossip message");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    tokio::task::yield_now().await;

    // All 3 clients should receive both events
    for (client_num, client) in [(1, &mut client1), (2, &mut client2), (3, &mut client3)] {
        // Claim event
        let claimed = tokio::time::timeout(tokio::time::Duration::from_secs(1), client.recv())
            .await
            .unwrap_or_else(|_| panic!("Client {client_num} should receive claim within timeout"))
            .unwrap_or_else(|| panic!("Client {client_num} should receive claim event"));

        assert!(
            matches!(claimed.event, GatewayEvent::ComputeTaskClaimed { .. }),
            "Client {client_num} should receive ComputeTaskClaimed"
        );

        // Completion event
        let completed = tokio::time::timeout(tokio::time::Duration::from_secs(1), client.recv())
            .await
            .unwrap_or_else(|_| {
                panic!("Client {client_num} should receive completion within timeout")
            })
            .unwrap_or_else(|| panic!("Client {client_num} should receive completion event"));

        assert!(
            matches!(completed.event, GatewayEvent::ComputeTaskCompleted { .. }),
            "Client {client_num} should receive ComputeTaskCompleted"
        );
    }
}

#[tokio::test]
async fn test_direct_event_forwarding() {
    // Test the forwarding function directly without compute actor
    let broadcaster = Arc::new(EventBroadcaster::new());
    let mut event_rx = broadcaster
        .subscribe("compute")
        .await
        .expect("Should subscribe");

    // Create and forward event directly
    let event = icn_compute::ComputeEvent::TaskClaimed {
        task_hash: "test123".to_string(),
        executor: TEST_EXECUTOR_DID.to_string(),
    };

    icn_gateway::forward_compute_event(&broadcaster, event).await;

    // Should receive immediately
    let received = tokio::time::timeout(tokio::time::Duration::from_secs(1), event_rx.recv())
        .await
        .expect("Should receive within timeout")
        .expect("Should receive event");

    match received.event {
        GatewayEvent::ComputeTaskClaimed { task_hash, .. } => {
            assert_eq!(task_hash, "test123");
        }
        _ => panic!("Wrong event type"),
    }
}

#[tokio::test]
async fn test_events_have_sequence_numbers() {
    let broadcaster = Arc::new(EventBroadcaster::new());
    let mut event_rx = broadcaster
        .subscribe("compute")
        .await
        .expect("Should subscribe");

    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut compute_actor = ComputeActor::new(TEST_EXECUTOR_DID.to_string(), trust_cb);

    // Set dummy send callback
    let send_callback: icn_compute::SendCallback = Arc::new(|_msg| {});
    compute_actor.set_send_callback(send_callback);

    compute_actor.set_event_callback(create_forwarding_callback(broadcaster));
    compute_actor.set_signing_key(vec![1u8; 32]);

    let handle = compute_actor.spawn();

    // Submit task
    let task = ComputeTask {
        id: "seq-test".to_string(),
        submitter: TEST_ALICE_DID.to_string(),
        coop_id: None,
        code: TaskCode::Ccl(simple_ccl()),
        inputs: vec![],
        fuel_limit: FuelLimit(10_000),
        required_capabilities: vec![ExecutorCapability::Ccl],
        priority: TaskPriority::Normal,
        created_at: 1000,
        deadline: None,
        payment_rate: None,
        payment_currency: None,
        resource_profile: None,
        actor_mode: None,
        placement_constraints: None,
        estimated_value: None,
        verification: None,
    };

    handle
        .submit(task.clone())
        .await
        .expect("Should submit task");

    // Simulate gossip broadcast loop-back
    handle
        .handle_gossip(icn_compute::ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .expect("Should handle gossip message");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    tokio::task::yield_now().await;

    // Receive events and verify sequence numbers are increasing
    let event1 = tokio::time::timeout(tokio::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("Should receive first event within timeout")
        .expect("Should receive first event");

    let event2 = tokio::time::timeout(tokio::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("Should receive second event within timeout")
        .expect("Should receive second event");

    assert!(
        event2.seq > event1.seq,
        "Sequence numbers should increase: {} -> {}",
        event1.seq,
        event2.seq
    );
}
