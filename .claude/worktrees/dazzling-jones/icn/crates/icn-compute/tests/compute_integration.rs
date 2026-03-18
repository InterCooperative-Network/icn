//! Integration tests for the ICN compute layer.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! These tests verify the distributed compute functionality including:
//! - Task submission and execution
//! - Actor runtime lifecycle
//! - Checkpoint storage

use icn_compute::{
    ActorCheckpoint, ActorId, ActorMode, CheckpointStore, ComputeActor, ComputeMessage,
    ComputeTask, DeterminismClass, ExecutionOutcome, Executor, ExecutorCapability, FuelLimit,
    InMemoryBackend, LocalExecutor, PauseReason, PrivacyClass, StatefulActorRegistry,
    StatefulActorState, TaskCode, TaskPriority,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Valid test DID (proper multibase-encoded Ed25519 public key)
/// Generated from deterministic seed [1u8; 32]
const TEST_ALICE_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";

/// Helper to create a test CCL task.
fn create_test_task(id: &str, submitter: &str) -> ComputeTask {
    let ccl_source = format!(
        r#"{{
        "name": "TestContract",
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
    );

    ComputeTask {
        id: id.to_string(),
        submitter: submitter.to_string(),
        coop_id: None,
        code: TaskCode::Ccl(ccl_source),
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
        federation_constraints: None,
        estimated_value: None,
        verification: None,
        // E1: Workload manifest fields
        inputs_hash: None,
        policy_hash: None,
        determinism_class: DeterminismClass::default(),
        privacy_class: PrivacyClass::default(),
        // E4: Storage specification fields
        storage_class: None,
        data_locality: None,
        scope: Default::default(),
    }
}

/// Test signing key for results.
fn test_signing_key() -> [u8; 32] {
    [1u8; 32]
}

// ============================================================================
// Local Executor Integration Tests
// ============================================================================

#[test]
fn test_local_executor_ccl_execution() {
    let executor = LocalExecutor::new();
    let task = create_test_task("task-1", TEST_ALICE_DID);

    let result = executor
        .execute_task(&task, TEST_ALICE_DID, &test_signing_key())
        .expect("Task should execute successfully");

    assert!(matches!(result.outcome, ExecutionOutcome::Success(_)));
    assert!(result.fuel_used > 0);
    // Note: duration_ms may be 0 for sub-millisecond execution
    assert!(!result.signature.is_empty());
}

#[test]
fn test_local_executor_capability_check() {
    let executor = LocalExecutor::new();
    let mut task = create_test_task("task-wasm", TEST_ALICE_DID);
    task.required_capabilities = vec![ExecutorCapability::Wasm];
    task.code = TaskCode::WasmRef([0u8; 32]);

    // Should fail because LocalExecutor doesn't have WASM capability
    assert!(!executor.can_execute(&task));
}

#[test]
fn test_local_executor_ccl_with_calculator() {
    let executor = LocalExecutor::new();

    let ccl_source = format!(
        r#"{{
        "name": "Calculator",
        "participants": ["{TEST_ALICE_DID}"],
        "currency": null,
        "state_vars": [],
        "rules": [{{
            "name": "add",
            "params": [{{"name": "a"}}, {{"name": "b"}}],
            "requires": [],
            "body": [{{
                "Return": {{
                    "value": {{
                        "BinOp": {{
                            "op": "Add",
                            "left": {{"Var": "a"}},
                            "right": {{"Var": "b"}}
                        }}
                    }}
                }}
            }}]
        }}],
        "triggers": []
    }}"#
    );

    let mut task = create_test_task("calc-task", TEST_ALICE_DID);
    task.code = TaskCode::Ccl(ccl_source);
    task.inputs = r#"{"a": {"Int": 5}, "b": {"Int": 3}}"#.as_bytes().to_vec();

    let result = executor
        .execute_task(&task, TEST_ALICE_DID, &test_signing_key())
        .expect("Task should execute successfully");

    assert!(
        matches!(result.outcome, ExecutionOutcome::Success(_)),
        "Expected success, got {:?}",
        result.outcome
    );
}

// ============================================================================
// Stateful Actor Registry Tests
// ============================================================================

#[tokio::test]
async fn test_actor_registry_lifecycle() {
    let registry = StatefulActorRegistry::new("did:icn:executor".to_string());
    let actor_id: ActorId = [1u8; 32];

    // Register actor
    registry
        .register(actor_id, ActorMode::default())
        .await
        .expect("Should register");

    // Verify running state
    let info = registry.get(&actor_id).await.expect("Should exist");
    assert_eq!(info.state, StatefulActorState::Running);

    // Pause for checkpointing
    registry
        .pause(actor_id, PauseReason::Checkpointing)
        .await
        .expect("Should pause");

    let info = registry.get(&actor_id).await.expect("Should exist");
    assert!(matches!(info.state, StatefulActorState::Paused { .. }));

    // Resume
    registry.resume(actor_id).await.expect("Should resume");

    let info = registry.get(&actor_id).await.expect("Should exist");
    assert_eq!(info.state, StatefulActorState::Running);
}

#[tokio::test]
async fn test_actor_registry_migration_flow() {
    let registry = StatefulActorRegistry::new("did:icn:source".to_string());
    let actor_id: ActorId = [2u8; 32];

    // Register and pause for migration
    registry
        .register(actor_id, ActorMode::default())
        .await
        .expect("Should register");

    registry
        .pause(actor_id, PauseReason::MigrationPending)
        .await
        .expect("Should pause");

    // Start migration
    registry
        .start_migration(actor_id, "did:icn:target".to_string())
        .await
        .expect("Should start migration");

    let info = registry.get(&actor_id).await.expect("Should exist");
    assert!(matches!(info.state, StatefulActorState::Migrating { .. }));

    // Complete migration (removes from source)
    registry
        .complete_migration(actor_id)
        .await
        .expect("Should complete migration");

    assert!(registry.get(&actor_id).await.is_none());
}

#[tokio::test]
async fn test_actor_registry_message_queueing() {
    let registry = StatefulActorRegistry::new("did:icn:executor".to_string());
    let actor_id: ActorId = [3u8; 32];

    registry
        .register(actor_id, ActorMode::default())
        .await
        .expect("Should register");

    registry
        .pause(actor_id, PauseReason::Checkpointing)
        .await
        .expect("Should pause");

    // Queue messages while paused
    registry
        .queue_message(&actor_id, b"message-1".to_vec())
        .await
        .expect("Should queue");
    registry
        .queue_message(&actor_id, b"message-2".to_vec())
        .await
        .expect("Should queue");

    registry.resume(actor_id).await.expect("Should resume");

    // Drain queued messages
    let messages = registry
        .drain_queued_messages(&actor_id)
        .await
        .expect("Should drain");

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0], b"message-1");
    assert_eq!(messages[1], b"message-2");
}

#[tokio::test]
async fn test_actor_registry_restore_from_checkpoint() {
    let registry = StatefulActorRegistry::new("did:icn:target".to_string());

    let checkpoint = ActorCheckpoint {
        actor_id: [4u8; 32],
        sequence: 10,
        state: b"restored state data".to_vec(),
        created_at: 5000,
        executor: "did:icn:source".to_string(),
        state_hash: ActorCheckpoint::compute_state_hash(b"restored state data"),
        signature: vec![],
    };

    registry
        .register_from_checkpoint(&checkpoint, ActorMode::default())
        .await
        .expect("Should restore");

    let info = registry
        .get(&checkpoint.actor_id)
        .await
        .expect("Should exist");
    assert_eq!(info.state, StatefulActorState::Running);
    assert_eq!(info.current_state, b"restored state data");
    assert_eq!(info.last_checkpoint_seq, 10);
}

#[tokio::test]
async fn test_actor_registry_state_updates() {
    let registry = StatefulActorRegistry::new("did:icn:executor".to_string());
    let actor_id: ActorId = [5u8; 32];

    registry
        .register(actor_id, ActorMode::default())
        .await
        .expect("Should register");

    // Update state multiple times
    for i in 0..5 {
        let state = format!("state-{i}").into_bytes();
        registry
            .update_state(&actor_id, state)
            .await
            .expect("Should update");
    }

    let info = registry.get(&actor_id).await.expect("Should exist");
    assert_eq!(info.current_state, b"state-4");
    assert_eq!(info.messages_processed, 5);
}

// ============================================================================
// Checkpoint Store Tests
// ============================================================================

#[tokio::test]
async fn test_checkpoint_store_basic() {
    let backend = InMemoryBackend::new();
    let store = CheckpointStore::new(Arc::new(backend));

    let actor_id: ActorId = [10u8; 32];
    let state = b"test state";
    let state_hash = ActorCheckpoint::compute_state_hash(state);

    // Create checkpoint
    let checkpoint = ActorCheckpoint {
        actor_id,
        sequence: 1,
        state: state.to_vec(),
        created_at: 1000,
        executor: "did:icn:executor".to_string(),
        state_hash,
        signature: vec![],
    };

    store.store(checkpoint).await.expect("Should store");

    // Retrieve
    let result = store.get(&actor_id).await.expect("Should get");
    let retrieved = result.expect("Should have checkpoint");
    assert_eq!(retrieved.sequence, 1);
    assert_eq!(retrieved.state, state);
}

#[tokio::test]
async fn test_checkpoint_store_update() {
    let backend = InMemoryBackend::new();
    let store = CheckpointStore::new(Arc::new(backend));

    let actor_id: ActorId = [11u8; 32];

    // Store initial checkpoint
    let checkpoint1 = ActorCheckpoint {
        actor_id,
        sequence: 1,
        state: b"state-1".to_vec(),
        created_at: 1000,
        executor: "did:icn:executor".to_string(),
        state_hash: ActorCheckpoint::compute_state_hash(b"state-1"),
        signature: vec![],
    };
    store.store(checkpoint1).await.expect("Should store");

    // Store newer checkpoint
    let checkpoint2 = ActorCheckpoint {
        actor_id,
        sequence: 2,
        state: b"state-2".to_vec(),
        created_at: 2000,
        executor: "did:icn:executor".to_string(),
        state_hash: ActorCheckpoint::compute_state_hash(b"state-2"),
        signature: vec![],
    };
    store.store(checkpoint2).await.expect("Should store");

    // Retrieve should return newer
    let result = store.get(&actor_id).await.expect("Should get");
    let retrieved = result.expect("Should have checkpoint");
    assert_eq!(retrieved.sequence, 2);
    assert_eq!(retrieved.state, b"state-2");

    // Store stale checkpoint (should be ignored)
    let stale = ActorCheckpoint {
        actor_id,
        sequence: 1, // older
        state: b"stale".to_vec(),
        created_at: 500,
        executor: "did:icn:executor".to_string(),
        state_hash: ActorCheckpoint::compute_state_hash(b"stale"),
        signature: vec![],
    };
    store.store(stale).await.expect("Should store (but ignore)");

    // Still should return sequence 2
    let result = store.get(&actor_id).await.expect("Should get");
    let retrieved = result.expect("Should have checkpoint");
    assert_eq!(retrieved.sequence, 2);
}

#[tokio::test]
async fn test_checkpoint_store_list_actors() {
    let backend = InMemoryBackend::new();
    let store = CheckpointStore::new(Arc::new(backend));

    // Store checkpoints for multiple actors
    for i in 0..5u8 {
        let mut actor_id = [0u8; 32];
        actor_id[0] = i;

        let checkpoint = ActorCheckpoint {
            actor_id,
            sequence: 1,
            state: vec![i],
            created_at: 1000 + i as u64,
            executor: "did:icn:executor".to_string(),
            state_hash: ActorCheckpoint::compute_state_hash(&[i]),
            signature: vec![],
        };

        store.store(checkpoint).await.expect("Should store");
    }

    let actors = store.list_actors().await.expect("Should list");
    assert_eq!(actors.len(), 5);
}

// ============================================================================
// Compute Actor Integration Tests
// ============================================================================

#[tokio::test]
async fn test_compute_actor_task_submission() {
    let trust_callback: Arc<dyn Fn(&str) -> f64 + Send + Sync> = Arc::new(|_did| 0.5);

    let actor = ComputeActor::new("did:icn:executor".to_string(), trust_callback);
    let handle = actor.spawn();

    let task = create_test_task("task-submit", "did:icn:alice");

    // Submit task
    let result = handle.submit(task).await;
    assert!(result.is_ok());

    let hash = result.unwrap();

    // Check status
    let status = handle.status(hash).await.expect("Should get status");
    assert!(status.is_some());
}

#[tokio::test]
async fn test_compute_actor_trust_gating() {
    // Low trust callback - below MIN_TRUST_SUBMIT
    let trust_callback: Arc<dyn Fn(&str) -> f64 + Send + Sync> = Arc::new(|_did| 0.05);

    let actor = ComputeActor::new("did:icn:executor".to_string(), trust_callback);
    let handle = actor.spawn();

    let task = create_test_task("task-low-trust", "did:icn:untrusted");

    // Submit should fail due to low trust
    let result = handle.submit(task).await;
    assert!(result.is_err());
}

// ============================================================================
// End-to-End Integration Tests
// ============================================================================

#[tokio::test]
async fn test_e2e_task_lifecycle() {
    // Create executor with sufficient trust
    let trust_callback: Arc<dyn Fn(&str) -> f64 + Send + Sync> = Arc::new(|_did| 0.5);

    let mut actor = ComputeActor::new(
        "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
        trust_callback,
    );

    // Set signing key for result signatures
    actor.set_signing_key(test_signing_key().to_vec());

    // Track sent messages
    let sent_messages: Arc<RwLock<Vec<ComputeMessage>>> = Arc::new(RwLock::new(vec![]));
    let sent_clone = sent_messages.clone();
    let send_callback: Arc<dyn Fn(ComputeMessage) + Send + Sync> = Arc::new(move |msg| {
        let sent = sent_clone.clone();
        tokio::spawn(async move {
            sent.write().await.push(msg);
        });
    });
    actor.set_send_callback(send_callback);

    let handle = actor.spawn();

    // Submit a task
    let task = create_test_task("e2e-task", "did:icn:alice");
    let _hash = handle.submit(task.clone()).await.expect("Should submit");

    // Wait a bit for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Check that messages were sent
    let messages = sent_messages.read().await;
    assert!(!messages.is_empty(), "Should have sent messages");
}

#[tokio::test]
async fn test_e2e_checkpoint_and_restore() {
    let backend = InMemoryBackend::new();
    let store = CheckpointStore::new(Arc::new(backend));

    // Create and store a checkpoint
    let actor_id: ActorId = [8u8; 32];
    let state = b"actor state for restoration";

    let keypair = icn_identity::KeyPair::generate().expect("Should generate keypair");
    let signing_key_bytes = keypair.to_signing_key_bytes();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

    let mut checkpoint = ActorCheckpoint {
        actor_id,
        sequence: 5,
        state: state.to_vec(),
        created_at: 10000,
        executor: keypair.did().to_string(),
        state_hash: ActorCheckpoint::compute_state_hash(state),
        signature: vec![],
    };

    checkpoint.sign(&signing_key);
    store.store(checkpoint.clone()).await.expect("Should store");

    // Create target registry and restore
    let target_registry = StatefulActorRegistry::new("did:icn:target".to_string());

    let result = store.get(&actor_id).await.expect("Should get");
    let restored = result.expect("Should have checkpoint");

    // Verify signature
    restored
        .verify_signature(keypair.did())
        .expect("Signature should be valid");

    // Restore to registry
    target_registry
        .register_from_checkpoint(&restored, ActorMode::default())
        .await
        .expect("Should restore");

    // Verify restored state
    let info = target_registry.get(&actor_id).await.expect("Should exist");
    assert_eq!(info.state, StatefulActorState::Running);
    assert_eq!(info.current_state, state);
    assert_eq!(info.last_checkpoint_seq, 5);
}

// ============================================================================
// Stress Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_actor_operations() {
    let registry = Arc::new(StatefulActorRegistry::new("did:icn:executor".to_string()));

    // Spawn multiple concurrent actor registrations
    let mut handles = vec![];
    for i in 0..10u8 {
        let registry = registry.clone();
        let handle = tokio::spawn(async move {
            let mut actor_id = [0u8; 32];
            actor_id[0] = i;

            registry
                .register(actor_id, ActorMode::default())
                .await
                .expect("Should register");

            // Pause and resume
            registry
                .pause(actor_id, PauseReason::Checkpointing)
                .await
                .expect("Should pause");

            registry.resume(actor_id).await.expect("Should resume");

            actor_id
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        let actor_id = handle.await.expect("Task should complete");

        // Verify final state
        let info = registry.get(&actor_id).await.expect("Should exist");
        assert_eq!(info.state, StatefulActorState::Running);
    }

    // Verify all actors registered
    let counts = registry.count_by_state().await;
    assert_eq!(counts.get("running"), Some(&10));
}

#[tokio::test]
async fn test_checkpoint_store_concurrent_access() {
    let backend = InMemoryBackend::new();
    let store = Arc::new(CheckpointStore::new(Arc::new(backend)));

    let actor_id: ActorId = [9u8; 32];

    // Spawn multiple concurrent stores
    let mut handles = vec![];
    for i in 0..10u64 {
        let store = store.clone();
        let handle = tokio::spawn(async move {
            let checkpoint = ActorCheckpoint {
                actor_id,
                sequence: i + 1,
                state: format!("state-{i}").into_bytes(),
                created_at: 1000 + i,
                executor: "did:icn:executor".to_string(),
                state_hash: ActorCheckpoint::compute_state_hash(format!("state-{i}").as_bytes()),
                signature: vec![],
            };

            store.store(checkpoint).await.expect("Should store");
        });
        handles.push(handle);
    }

    // Wait for all
    for handle in handles {
        handle.await.expect("Task should complete");
    }

    // Final checkpoint should be highest sequence
    let result = store.get(&actor_id).await.expect("Should get");
    let final_checkpoint = result.expect("Should have checkpoint");
    assert_eq!(final_checkpoint.sequence, 10);
}

#[tokio::test]
async fn test_resource_profile_refresh() {
    use icn_compute::ResourceRefreshConfig;
    use std::time::Duration;

    // Create a compute actor with fast refresh interval (1 second for testing)
    let trust_cb: icn_compute::TrustCallback = Arc::new(|_did: &str| 0.8);
    let mut actor = icn_compute::ComputeActor::new("did:icn:test".to_string(), trust_cb);

    // Set fast refresh for testing
    actor.set_resource_refresh_config(ResourceRefreshConfig::with_interval(1));

    // Track events
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let event_cb: icn_compute::EventCallback = Arc::new(move |event| {
        events_clone.lock().unwrap().push(event);
    });
    actor.set_event_callback(event_cb);

    // Spawn the actor
    let _handle = actor.spawn();

    // Wait for at least 2 refresh cycles (2 seconds)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // At this point, we should have at least one refresh
    // (first refresh doesn't detect changes since there's no previous capacity)
    // Note: Without an actual resource change, we won't get ResourcesChanged events
    // This test primarily verifies that the refresh task runs without panicking

    // Clean shutdown
    drop(_handle);
}

#[tokio::test]
async fn test_resource_change_detection() {
    use icn_compute::{NodeCapacity, ResourceChangeType};

    let capacity1 = NodeCapacity {
        cpu_cores_total: 8.0,
        cpu_cores_available: 4.0,
        memory_mb_total: 16384,
        memory_mb_available: 8192,
        storage_mb_available: 100_000,
        network_mbps: 1000.0,
        gpu_devices: vec![],
        updated_at: 1000,
    };

    // Test no change
    let capacity2 = capacity1.clone();
    let changes = capacity1.detect_changes(&capacity2, 0.1);
    assert!(changes.is_empty());

    // Test CPU change
    let mut capacity2 = capacity1.clone();
    capacity2.cpu_cores_available = 7.0; // Significant change
    let changes = capacity1.detect_changes(&capacity2, 0.1);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0], ResourceChangeType::Cpu);

    // Test memory change
    let mut capacity2 = capacity1.clone();
    capacity2.memory_mb_available = 6000;
    let changes = capacity1.detect_changes(&capacity2, 0.1);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0], ResourceChangeType::Memory);

    // Test multiple changes
    let mut capacity2 = capacity1.clone();
    capacity2.cpu_cores_available = 7.0;
    capacity2.memory_mb_available = 6000;
    let changes = capacity1.detect_changes(&capacity2, 0.1);
    assert_eq!(changes.len(), 2);
    assert!(changes.contains(&ResourceChangeType::Cpu));
    assert!(changes.contains(&ResourceChangeType::Memory));
}
