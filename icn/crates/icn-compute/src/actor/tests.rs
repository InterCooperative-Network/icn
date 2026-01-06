use super::consensus::{outcome_to_value, results_match};
use super::*;
use crate::task::TaskStatus;
use crate::types::{ComputeResult, ComputeTask, ExecutorCapability, FuelLimit, TaskCode};

fn simple_ccl() -> String {
    r#"{
            "name": "SimpleReturn",
            "participants": ["did:icn:alice"],
            "currency": null,
            "state_vars": [],
            "rules": [{
                "name": "run",
                "params": [],
                "requires": [],
                "body": [{ "Return": { "value": { "Literal": { "Int": 42 } } } }]
            }],
            "triggers": []
        }"#
    .to_string()
}

fn make_task(id: &str, submitter: &str) -> ComputeTask {
    ComputeTask {
        id: id.into(),
        submitter: submitter.into(),
        coop_id: None,
        code: TaskCode::Ccl(simple_ccl()),
        inputs: vec![],
        fuel_limit: FuelLimit::default(),
        required_capabilities: vec![ExecutorCapability::Ccl],
        priority: crate::types::TaskPriority::Normal,
        created_at: 1000,
        deadline: None,
        payment_rate: None,
        payment_currency: None,
        resource_profile: None,
        actor_mode: None,
        placement_constraints: None,
        estimated_value: None,
        verification: None,
    }
}

#[tokio::test]
async fn test_submit_task() {
    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    let handle = actor.spawn();

    let task = make_task("task-1", "did:icn:alice");
    let hash = handle.submit(task).await.unwrap();

    let status = handle.status(hash).await.unwrap();
    assert!(status.is_some());
}

#[tokio::test]
async fn test_submit_low_trust_rejected() {
    let trust_cb: TrustCallback = Arc::new(|_| 0.05); // Below MIN_TRUST_SUBMIT
    let actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    let handle = actor.spawn();

    let task = make_task("task-1", "did:icn:untrusted");
    let result = handle.submit(task).await;

    assert!(matches!(
        result,
        Err(ComputeError::InsufficientTrust { .. })
    ));
}

#[tokio::test]
async fn test_gossip_message_handling() {
    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    // Set a valid signing key for result signatures
    actor.set_signing_key(vec![1u8; 32]);
    let handle = actor.spawn();

    let task = make_task("task-1", "did:icn:alice");
    let msg = ComputeMessage::TaskSubmitted(Box::new(task.clone()));

    handle.handle_gossip(msg).await.unwrap();

    // Task should be stored and executed
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    let hash = task.hash();
    let status = handle.status(hash).await.unwrap();
    assert!(matches!(status, Some(TaskStatus::Completed { .. })));
}

#[tokio::test]
async fn test_executor_tracking() {
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new("did:icn:submitter".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    let handle = actor.spawn();

    // Announce two executors with different capabilities
    let msg1 = ComputeMessage::ExecutorAnnounce {
        executor: "did:icn:executor1".into(),
        capabilities: vec![ExecutorCapability::Ccl],
    };
    let msg2 = ComputeMessage::ExecutorAnnounce {
        executor: "did:icn:executor2".into(),
        capabilities: vec![ExecutorCapability::Ccl, ExecutorCapability::Wasm],
    };

    handle.handle_gossip(msg1).await.unwrap();
    handle.handle_gossip(msg2).await.unwrap();

    // Give time for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Both executors should be tracked in the registry
    // (We can't directly access the registry from here, but the announcements succeeded)
    // This test validates the message handling works without errors
}

#[tokio::test]
async fn test_consensus_single_result() {
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new("did:icn:submitter".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    let handle = actor.spawn();

    // Submit a task
    let task = make_task("consensus-task", "did:icn:alice");
    let task_hash = handle.submit(task.clone()).await.unwrap();

    // Generate a valid keypair and result
    let keypair = icn_identity::KeyPair::generate().unwrap();
    let executor_did = keypair.did().as_str();
    let signing_key = keypair.to_signing_key_bytes();

    let mut result = crate::types::ComputeResult {
        task_hash,
        task_id: task.id.clone(),
        executor: executor_did.to_string(),
        outcome: crate::types::ExecutionOutcome::Success(b"42".to_vec()),
        fuel_used: 100,
        duration_ms: 10,
        completed_at: 1000,
        signature: vec![],
    };

    // Sign the result
    let ed25519_key = ed25519_dalek::SigningKey::from_bytes(&signing_key);
    result.sign(&ed25519_key);

    // Submit result via gossip
    let msg = ComputeMessage::TaskResult(result);
    handle.handle_gossip(msg).await.unwrap();

    // Give time for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Task should be completed after consensus (1 result required)
    let status = handle.status(task_hash).await.unwrap();
    assert!(matches!(status, Some(TaskStatus::Completed { .. })));
}

#[tokio::test]
async fn test_priority_based_claiming() {
    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);

    // Set max concurrent to 1 so we can control execution order
    actor.set_max_concurrent_tasks(1);
    let handle = actor.spawn();

    // Submit tasks with different priorities
    let mut low_task = make_task("low-priority", "did:icn:alice");
    low_task.priority = crate::types::TaskPriority::Low;

    let mut normal_task = make_task("normal-priority", "did:icn:alice");
    normal_task.priority = crate::types::TaskPriority::Normal;

    let mut high_task = make_task("high-priority", "did:icn:alice");
    high_task.priority = crate::types::TaskPriority::High;

    let mut critical_task = make_task("critical-priority", "did:icn:alice");
    critical_task.priority = crate::types::TaskPriority::Critical;

    // Submit them via gossip (not direct submit which auto-executes)
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(low_task.clone())))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(normal_task.clone())))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(high_task.clone())))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(
            critical_task.clone(),
        )))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // The critical task should have been claimed and completed first
    // Since we have max_concurrent=1, only highest priority runs at a time
    let critical_hash = critical_task.hash();
    let critical_status = handle.status(critical_hash).await.unwrap();
    assert!(
        matches!(critical_status, Some(TaskStatus::Completed { .. })),
        "Critical task should be completed first, got: {critical_status:?}"
    );

    // High should be next
    let high_hash = high_task.hash();
    let high_status = handle.status(high_hash).await.unwrap();
    assert!(
        matches!(high_status, Some(TaskStatus::Completed { .. })),
        "High priority task should be completed second, got: {high_status:?}"
    );
}

/// Integration test for Phase 16B placement negotiation
/// Tests 5 executors competing for a task with deliberation window
#[tokio::test]
async fn test_placement_negotiation_multi_executor() {
    use crate::scheduler::ResourceProfile;
    use crate::types::ExecutorCapability;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    // Track all offers from all executors
    let offers_collected: Arc<TokioMutex<Vec<(String, f64)>>> = Arc::new(TokioMutex::new(vec![]));
    let claims_collected: Arc<TokioMutex<Vec<String>>> = Arc::new(TokioMutex::new(vec![]));

    // Create a compute-heavy task (CPU+RAM, no GPU to simplify test)
    let task = make_task("data-processing", "did:icn:submitter");
    let task_hash = task.hash();
    let resource_profile = ResourceProfile::compute_heavy(2.0, 4096);

    // Executor configurations: (did, trust)
    let executor_configs = vec![
        ("did:icn:executor-a", 0.9), // Highest trust
        ("did:icn:executor-b", 0.7), // Medium trust
        ("did:icn:executor-c", 0.5), // Low trust (but above MIN_TRUST_EXECUTE = 0.3)
        ("did:icn:executor-d", 0.8), // High trust
        ("did:icn:executor-e", 0.2), // Very low trust (below MIN_TRUST_EXECUTE = 0.3, should reject)
    ];

    // Spawn all executors
    let mut executor_handles = vec![];
    for (did, trust) in executor_configs {
        let trust_cb: TrustCallback = Arc::new(move |_| trust);
        let mut actor = ComputeActor::new(did.to_string(), trust_cb);
        actor.set_signing_key(vec![1u8; 32]);

        // Note: Actors use placeholder capacity internally (8 cores, 16GB RAM)
        // which is sufficient for our compute_heavy(2.0, 4096) test task

        let offers_clone = offers_collected.clone();
        let claims_clone = claims_collected.clone();
        let did_owned = did.to_string();

        // Set up send callback to intercept offers and claims
        let send_cb: SendCallback = Arc::new(move |msg| {
            let offers_c = offers_clone.clone();
            let claims_c = claims_clone.clone();
            let did_c = did_owned.clone();

            tokio::spawn(async move {
                match msg {
                    ComputeMessage::PlacementOffer { score, .. } => {
                        let mut offers = offers_c.lock().await;
                        offers.push((did_c.clone(), score));
                        tracing::info!(
                            executor = %did_c,
                            score = score,
                            "Executor broadcast offer"
                        );
                    }
                    ComputeMessage::TaskClaimed { executor, .. } => {
                        let mut claims = claims_c.lock().await;
                        claims.push(executor.clone());
                        tracing::info!(
                            executor = %executor,
                            "Task claimed"
                        );
                    }
                    _ => {}
                }
            });
        });
        actor.set_send_callback(send_cb);

        let handle = actor.spawn();
        executor_handles.push((did.to_string(), handle));
    }

    // Register all executors by having them announce themselves
    tracing::info!("Registering all executors via ExecutorAnnounce");
    for (did, handle) in &executor_handles {
        let announce_msg = ComputeMessage::ExecutorAnnounce {
            executor: did.clone(),
            capabilities: vec![ExecutorCapability::Ccl], // CCL capability
        };
        handle.handle_gossip(announce_msg).await.unwrap();
    }

    // Small delay to ensure registrations complete
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Broadcast PlacementRequest to all executors
    let placement_request = ComputeMessage::PlacementRequest {
        task_hash,
        submitter: "did:icn:submitter".to_string(),
        resource_profile: resource_profile.clone(),
        locality_hints: vec![],
        max_cost: Some(1000),
        requested_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    tracing::info!("Broadcasting PlacementRequest to all executors");
    for (did, handle) in &executor_handles {
        tracing::debug!(executor = %did, "Sending PlacementRequest");
        handle
            .handle_gossip(placement_request.clone())
            .await
            .unwrap();
    }

    // Wait for deliberation window (500ms) + grace period (500ms) + processing time
    tracing::info!("Waiting for deliberation and offers...");
    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

    // Check collected offers
    let offers = offers_collected.lock().await;
    tracing::info!("Collected {} offers", offers.len());
    for (executor, score) in offers.iter() {
        tracing::info!(executor = %executor, score = score, "Offer received");
    }

    // Verify expectations:
    // 1. executor-e should NOT offer (trust < MIN_TRUST_EXECUTE = 0.3)
    // 2. executor-a, executor-b, executor-c, executor-d should offer
    assert!(
        offers.iter().any(|(did, _)| did == "did:icn:executor-a"),
        "Executor A should offer (high trust)"
    );
    assert!(
        offers.iter().any(|(did, _)| did == "did:icn:executor-b"),
        "Executor B should offer (medium trust)"
    );
    assert!(
        offers.iter().any(|(did, _)| did == "did:icn:executor-c"),
        "Executor C should offer (trust above minimum)"
    );
    assert!(
        offers.iter().any(|(did, _)| did == "did:icn:executor-d"),
        "Executor D should offer (high trust, even if busy)"
    );
    assert!(
        !offers.iter().any(|(did, _)| did == "did:icn:executor-e"),
        "Executor E should NOT offer (trust below MIN_TRUST_EXECUTE = 0.3)"
    );

    // Check that we got 4 offers (all except executor-e)
    assert_eq!(
        offers.len(),
        4,
        "Expected 4 offers from executors above trust threshold"
    );

    // Find the highest score (using total_cmp for deterministic NaN handling)
    let highest = offers.iter().max_by(|a, b| a.1.total_cmp(&b.1));

    if let Some((winner_did, winner_score)) = highest {
        tracing::info!(
            winner = %winner_did,
            score = winner_score,
            "Highest scoring executor"
        );

        // Winner should be executor-a, executor-b, or executor-d (trust: 0.9, 0.7, 0.8)
        // Phase 16C: Trust now contributes 25% (reduced from 40%)
        // With 10% random jitter, executor-b can win with favorable jitter
        // executor-c (0.5) should still be excluded due to significantly lower trust
        assert!(
            winner_did == "did:icn:executor-a"
                || winner_did == "did:icn:executor-b"
                || winner_did == "did:icn:executor-d",
            "Winner should be executor A, B, or D (reasonable trust), got: {winner_did}"
        );
    } else {
        panic!("No offers received!");
    }

    tracing::info!("✅ Phase 16B placement negotiation test passed!");
    tracing::info!("   - 4 executors broadcast offers after deliberation window");
    tracing::info!("   - 1 executor rejected due to low trust");
    tracing::info!("   - High-trust executor won (executor-a, executor-b, or executor-d)");

    // Note: Full end-to-end test with submitter-side selection will be added
    // in a future integration test that simulates the complete gossip protocol
}

/// Integration test for Phase 16C locality-aware placement
/// Demonstrates how RTT and data locality affect executor selection
#[tokio::test]
async fn test_locality_aware_placement_scoring() {
    use crate::scheduler::{
        DefaultPlacementPolicy, LocalityContext, LocalityHint, NodeCapacity, NodeState,
        PlacementPolicy, ResourceProfile,
    };

    // Create task requiring compute resources
    let task_hash = [5u8; 32];
    let profile = ResourceProfile::compute_heavy(2.0, 4096);
    let policy = DefaultPlacementPolicy::default();

    // Baseline node state (all executors have same capacity)
    let capacity = NodeCapacity {
        cpu_cores_total: 8.0,
        cpu_cores_available: 8.0,
        memory_mb_total: 16384,
        memory_mb_available: 16384,
        storage_mb_available: 100_000,
        network_mbps: 1000.0,
        gpu_devices: vec![],
        updated_at: 0,
    };

    // Executor A: High trust (0.8), no locality information
    let node_a = NodeState {
        did: "did:icn:executor-a".to_string(),
        capacity: capacity.clone(),
        executing_tasks: std::collections::HashMap::new(),
        queue_depth: 0,
    };
    let locality_a = LocalityContext::empty();

    // Executor B: Medium trust (0.6), excellent RTT (10ms to submitter)
    let node_b = NodeState {
        did: "did:icn:executor-b".to_string(),
        capacity: capacity.clone(),
        executing_tasks: std::collections::HashMap::new(),
        queue_depth: 0,
    };
    let locality_b = LocalityContext {
        submitter_rtt_ms: Some(10), // 10ms RTT
        local_blob_count: 0,
        total_blob_count: 0,
        own_region: None,
        submitter_region: None,
    };

    // Executor C: Medium trust (0.6), excellent data locality (all blobs local)
    let node_c = NodeState {
        did: "did:icn:executor-c".to_string(),
        capacity: capacity.clone(),
        executing_tasks: std::collections::HashMap::new(),
        queue_depth: 0,
    };
    let locality_c = LocalityContext {
        submitter_rtt_ms: None,
        local_blob_count: 5, // All 5 required blobs are local
        total_blob_count: 5,
        own_region: None,
        submitter_region: None,
    };

    // Executor D: Low trust (0.4), both good RTT and data locality
    let node_d = NodeState {
        did: "did:icn:executor-d".to_string(),
        capacity: capacity.clone(),
        executing_tasks: std::collections::HashMap::new(),
        queue_depth: 0,
    };
    let locality_d = LocalityContext {
        submitter_rtt_ms: Some(15), // 15ms RTT
        local_blob_count: 5,        // All blobs local
        total_blob_count: 5,
        own_region: None,
        submitter_region: None,
    };

    let no_hints: Vec<LocalityHint> = vec![];

    // Score all executors
    let offer_a = policy
        .score_task(
            &task_hash,
            &profile,
            "submitter",
            &node_a,
            0.8,
            &no_hints,
            &locality_a,
        )
        .unwrap();

    let offer_b = policy
        .score_task(
            &task_hash,
            &profile,
            "submitter",
            &node_b,
            0.6,
            &no_hints,
            &locality_b,
        )
        .unwrap();

    let offer_c = policy
        .score_task(
            &task_hash,
            &profile,
            "submitter",
            &node_c,
            0.6,
            &no_hints,
            &locality_c,
        )
        .unwrap();

    let offer_d = policy
        .score_task(
            &task_hash,
            &profile,
            "submitter",
            &node_d,
            0.4,
            &no_hints,
            &locality_d,
        )
        .unwrap();

    tracing::info!("=== Phase 16C Locality-Aware Placement Scoring ===");
    tracing::info!(
        "Executor A (trust=0.8, no locality): score = {:.4}",
        offer_a.score
    );
    tracing::info!(
        "Executor B (trust=0.6, RTT=10ms): score = {:.4}",
        offer_b.score
    );
    tracing::info!(
        "Executor C (trust=0.6, 5/5 blobs local): score = {:.4}",
        offer_c.score
    );
    tracing::info!(
        "Executor D (trust=0.4, RTT=15ms + 5/5 blobs): score = {:.4}",
        offer_d.score
    );

    // Analysis: With rebalanced weights (Phase 16C Week 3):
    // - Trust: 25% (was 40%)
    // - Capacity: 20%
    // - Queue: 15%
    // - RTT: 15% (NEW)
    // - Data locality: 15% (NEW)
    // - Hints: 10% (NEW)
    // - Jitter: 10%

    // Expected scoring breakdown (approximate, excluding jitter):
    // Executor A: trust(0.20) + capacity(0.20) + queue(0.15) = 0.55
    // Executor B: trust(0.15) + capacity(0.20) + queue(0.15) + rtt(0.148) = 0.648
    // Executor C: trust(0.15) + capacity(0.20) + queue(0.15) + data(0.15) = 0.65
    // Executor D: trust(0.10) + capacity(0.20) + queue(0.15) + rtt(0.145) + data(0.15) = 0.745

    // Verify that locality factors make a significant difference
    // Executor B (good RTT) should beat A (higher trust but no locality)
    assert!(
        offer_b.score + 0.03 > offer_a.score,
        "Good RTT should compensate for lower trust"
    );

    // Executor C (good data locality) should beat A
    assert!(
        offer_c.score + 0.03 > offer_a.score,
        "Good data locality should compensate for lower trust"
    );

    // Executor D (both RTT + data) should score highest despite lowest trust
    // D has 0.4 trust (0.10 contribution) but gets 0.148 + 0.15 = 0.298 from locality
    // A has 0.8 trust (0.20 contribution) but gets 0.0 from locality
    // Net: D gets ~0.10 more points from locality than A gets from higher trust
    assert!(
        offer_d.score + 0.05 > offer_a.score,
        "Combined RTT + data locality should beat high trust alone"
    );

    tracing::info!("✅ Phase 16C locality-aware placement test passed!");
    tracing::info!("   - Locality factors (RTT + data) effectively compensate for lower trust");
    tracing::info!("   - Executor selection properly balances trust, capacity, and locality");
    tracing::info!("   - Scoring demonstrates Phase 16C's 'compute goes to data' principle");
}

/// Integration test for Phase 16D actor migration
/// Demonstrates complete migration flow with checkpoint transfer
#[tokio::test]
async fn test_actor_migration_integration() {
    use crate::actor_model::{ActorCheckpoint, ActorId, ActorRuntimeState, MigrationReason};
    use crate::checkpoint_store::{CheckpointStore, InMemoryBackend};
    use crate::migration_manager::{ActorMigrationManager, MigrationSender};
    use crate::migration_policy::DefaultMigrationPolicy;
    use crate::scheduler::NodeCapacity;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    // Track migration messages
    let messages_collected: Arc<TokioMutex<Vec<ComputeMessage>>> =
        Arc::new(TokioMutex::new(vec![]));

    // Create two executors
    let executor_a_keypair = icn_identity::KeyPair::generate().unwrap();
    let executor_a_did = executor_a_keypair.did().to_string();

    let executor_b_keypair = icn_identity::KeyPair::generate().unwrap();
    let executor_b_did = executor_b_keypair.did().to_string();

    tracing::info!("Executor A: {}", executor_a_did);
    tracing::info!("Executor B: {}", executor_b_did);

    // Create checkpoint stores for both executors
    let store_a = Arc::new(CheckpointStore::new(Arc::new(InMemoryBackend::new())));
    let store_b = Arc::new(CheckpointStore::new(Arc::new(InMemoryBackend::new())));

    // Create migration managers
    let policy = Arc::new(DefaultMigrationPolicy::default());

    let messages_a = messages_collected.clone();
    let send_a: MigrationSender = Arc::new(move |msg| {
        let m = messages_a.clone();
        tokio::spawn(async move {
            m.lock().await.push(msg);
        });
        Ok(())
    });

    let manager_a = Arc::new(ActorMigrationManager::new(
        policy.clone(),
        store_a.clone(),
        send_a,
        executor_a_did.clone(),
    ));

    let messages_b = messages_collected.clone();
    let send_b: MigrationSender = Arc::new(move |msg| {
        let m = messages_b.clone();
        tokio::spawn(async move {
            m.lock().await.push(msg);
        });
        Ok(())
    });

    let manager_b = Arc::new(ActorMigrationManager::new(
        policy.clone(),
        store_b.clone(),
        send_b,
        executor_b_did.clone(),
    ));

    // Create ComputeActors
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);

    let mut actor_a = ComputeActor::new(executor_a_did.clone(), trust_cb.clone());
    actor_a.set_checkpoint_store(store_a.clone());
    actor_a.set_migration_manager(manager_a.clone());
    let _handle_a = actor_a.spawn();

    let mut actor_b = ComputeActor::new(executor_b_did.clone(), trust_cb.clone());
    actor_b.set_checkpoint_store(store_b.clone());
    actor_b.set_migration_manager(manager_b.clone());
    let handle_b = actor_b.spawn();

    // Create a stateful actor on executor A
    let actor_id: ActorId = [1u8; 32];
    let actor_state = vec![42u8, 43, 44]; // Simple state

    let mut checkpoint = ActorCheckpoint {
        actor_id,
        sequence: 1,
        state: actor_state.clone(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        executor: executor_a_did.clone(),
        state_hash: ActorCheckpoint::compute_state_hash(&actor_state),
        signature: vec![],
    };

    // Sign checkpoint
    let signing_key_a =
        ed25519_dalek::SigningKey::from_bytes(&executor_a_keypair.to_signing_key_bytes());
    checkpoint.sign(&signing_key_a);

    // Store initial checkpoint on A
    store_a.store(checkpoint.clone()).await.unwrap();

    tracing::info!("Created stateful actor on executor A");

    // Simulate migration evaluation on A (overloaded)
    let network_state = crate::migration_policy::NetworkState {
        executor_load: 0.94, // Very high load on current executor
        available_executors: vec![
            crate::migration_policy::ExecutorInfo {
                did: executor_a_did.clone(),
                trust_score: 0.8,
                load: 0.94, // Very high - overloaded
                capacity: NodeCapacity {
                    cpu_cores_total: 8.0,
                    cpu_cores_available: 0.5,
                    memory_mb_total: 16384,
                    memory_mb_available: 2048,
                    storage_mb_available: 100_000,
                    network_mbps: 1000.0,
                    gpu_devices: vec![],
                    updated_at: 0,
                },
                queue_depth: 15, // High load
                last_seen: 0,
            },
            crate::migration_policy::ExecutorInfo {
                did: executor_b_did.clone(),
                trust_score: 0.8,
                load: 0.12, // Very low - idle
                capacity: NodeCapacity {
                    cpu_cores_total: 8.0,
                    cpu_cores_available: 7.5,
                    memory_mb_total: 16384,
                    memory_mb_available: 15000,
                    storage_mb_available: 100_000,
                    network_mbps: 1000.0,
                    gpu_devices: vec![],
                    updated_at: 0,
                },
                queue_depth: 1, // Low load
                last_seen: 0,
            },
        ],
        executor_rtt: std::collections::HashMap::new(),
        blob_locations: std::collections::HashMap::new(),
        snapshot_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };

    let actor_runtime_state = ActorRuntimeState {
        actor_id,
        uptime_secs: 3600,
        memory_bytes: 1024 * 1024,
        cpu_utilization: 0.5,
        msg_throughput: 10.0,
        last_checkpoint_at: checkpoint.created_at,
        checkpoint_sequence: 1,
        required_blobs: vec![],
    };

    // Evaluate migration
    let decision = manager_a
        .evaluate_migration(actor_runtime_state, &network_state)
        .await;

    assert!(decision.is_some(), "Migration should be recommended");
    let decision = decision.unwrap();

    tracing::info!(
        "Migration decision: from={} to={} reason={:?} benefit={}",
        decision.from_executor,
        decision.target_executor,
        decision.reason,
        decision.benefit_score
    );

    assert_eq!(decision.target_executor, executor_b_did);

    // Initiate migration from A
    manager_a
        .initiate_migration(decision, &signing_key_a)
        .await
        .unwrap();

    // Give time for message to be sent
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Check messages
    let messages = messages_collected.lock().await;
    assert!(!messages.is_empty(), "Should have sent migration request");

    // Find the MigrationRequest message
    let migration_request = messages.iter().find_map(|msg| {
        if let ComputeMessage::MigrationRequest {
            actor_id: aid,
            from_executor,
            to_executor,
            checkpoint: ckpt,
            reason,
        } = msg
        {
            Some((aid, from_executor, to_executor, ckpt, reason))
        } else {
            None
        }
    });

    assert!(
        migration_request.is_some(),
        "Should have MigrationRequest message"
    );
    let (aid, from, to, ckpt, reason) = migration_request.unwrap();

    assert_eq!(aid, &actor_id);
    assert_eq!(from, &executor_a_did);
    assert_eq!(to, &executor_b_did);
    assert!(matches!(reason, MigrationReason::LoadBalancing));

    tracing::info!("Migration request sent from A to B");

    // Clone values before dropping messages
    let migration_msg = ComputeMessage::MigrationRequest {
        actor_id: *aid,
        from_executor: from.clone(),
        to_executor: to.clone(),
        checkpoint: ckpt.clone(),
        reason: reason.clone(),
    };

    // Clear messages for next phase
    drop(messages);
    messages_collected.lock().await.clear();

    // Executor B handles migration request
    handle_b.handle_gossip(migration_msg).await.unwrap();

    // Give time for B to evaluate and respond
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Check for MigrationAccept message
    let messages = messages_collected.lock().await;
    let migration_accept = messages.iter().find_map(|msg| {
        if let ComputeMessage::MigrationAccept {
            actor_id: aid,
            to_executor,
        } = msg
        {
            Some((aid, to_executor))
        } else {
            None
        }
    });

    assert!(
        migration_accept.is_some(),
        "Should have MigrationAccept message"
    );
    let (aid, to) = migration_accept.unwrap();

    assert_eq!(aid, &actor_id);
    assert_eq!(to, &executor_b_did);

    tracing::info!("Migration accepted by B");

    // Verify checkpoint was stored on B
    let checkpoint_b = store_b.get(&actor_id).await.unwrap();
    assert!(checkpoint_b.is_some(), "Checkpoint should be stored on B");
    let checkpoint_b = checkpoint_b.unwrap();
    assert_eq!(checkpoint_b.state, actor_state);
    assert_eq!(checkpoint_b.sequence, 1);

    tracing::info!("✅ Phase 16D actor migration integration test passed!");
    tracing::info!("   - Migration decision correctly identified overloaded executor");
    tracing::info!("   - Migration request sent from A to B with checkpoint");
    tracing::info!("   - Executor B accepted migration and stored checkpoint");
    tracing::info!("   - Actor state preserved across migration");
}

// ===== Conflict Detection Tests (Issue #54) =====

#[test]
fn test_results_match_both_success_same_data() {
    let r1 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:alice".into(),
        outcome: crate::types::ExecutionOutcome::Success(vec![1, 2, 3]),
        fuel_used: 100,
        duration_ms: 10,
        completed_at: 1000,
        signature: vec![],
    };
    let r2 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:bob".into(),
        outcome: crate::types::ExecutionOutcome::Success(vec![1, 2, 3]),
        fuel_used: 110,
        duration_ms: 12,
        completed_at: 1001,
        signature: vec![],
    };

    assert!(results_match(&r1, &r2), "Same success data should match");
}

#[test]
fn test_results_match_both_success_different_data() {
    let r1 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:alice".into(),
        outcome: crate::types::ExecutionOutcome::Success(vec![1, 2, 3]),
        fuel_used: 100,
        duration_ms: 10,
        completed_at: 1000,
        signature: vec![],
    };
    let r2 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:bob".into(),
        outcome: crate::types::ExecutionOutcome::Success(vec![4, 5, 6]),
        fuel_used: 110,
        duration_ms: 12,
        completed_at: 1001,
        signature: vec![],
    };

    assert!(
        !results_match(&r1, &r2),
        "Different success data should not match"
    );
}

#[test]
fn test_results_match_success_vs_failed() {
    let r1 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:alice".into(),
        outcome: crate::types::ExecutionOutcome::Success(vec![1, 2, 3]),
        fuel_used: 100,
        duration_ms: 10,
        completed_at: 1000,
        signature: vec![],
    };
    let r2 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:bob".into(),
        outcome: crate::types::ExecutionOutcome::Failed("error".into()),
        fuel_used: 50,
        duration_ms: 5,
        completed_at: 1001,
        signature: vec![],
    };

    assert!(
        !results_match(&r1, &r2),
        "Success vs Failed should not match"
    );
}

#[test]
fn test_results_match_both_out_of_fuel() {
    let r1 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:alice".into(),
        outcome: crate::types::ExecutionOutcome::OutOfFuel,
        fuel_used: 10000,
        duration_ms: 100,
        completed_at: 1000,
        signature: vec![],
    };
    let r2 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:bob".into(),
        outcome: crate::types::ExecutionOutcome::OutOfFuel,
        fuel_used: 10000,
        duration_ms: 105,
        completed_at: 1001,
        signature: vec![],
    };

    assert!(results_match(&r1, &r2), "Both OutOfFuel should match");
}

#[test]
fn test_results_match_both_timeout() {
    let r1 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:alice".into(),
        outcome: crate::types::ExecutionOutcome::Timeout,
        fuel_used: 5000,
        duration_ms: 30000,
        completed_at: 1000,
        signature: vec![],
    };
    let r2 = ComputeResult {
        task_hash: [0u8; 32],
        task_id: "test".into(),
        executor: "did:icn:bob".into(),
        outcome: crate::types::ExecutionOutcome::Timeout,
        fuel_used: 4500,
        duration_ms: 30000,
        completed_at: 1001,
        signature: vec![],
    };

    assert!(results_match(&r1, &r2), "Both Timeout should match");
}

#[test]
fn test_outcome_to_value_success() {
    let outcome = crate::types::ExecutionOutcome::Success(vec![0xde, 0xad, 0xbe, 0xef]);
    let value = outcome_to_value(&outcome);

    if let icn_ccl::Value::String(s) = value {
        assert!(s.starts_with("success:"));
        assert!(s.contains("deadbeef"));
    } else {
        panic!("Expected String value");
    }
}

#[test]
fn test_outcome_to_value_failed() {
    let outcome = crate::types::ExecutionOutcome::Failed("test error".into());
    let value = outcome_to_value(&outcome);

    if let icn_ccl::Value::String(s) = value {
        assert_eq!(s, "failed:test error");
    } else {
        panic!("Expected String value");
    }
}

#[test]
fn test_outcome_to_value_out_of_fuel() {
    let outcome = crate::types::ExecutionOutcome::OutOfFuel;
    let value = outcome_to_value(&outcome);

    if let icn_ccl::Value::String(s) = value {
        assert_eq!(s, "out_of_fuel");
    } else {
        panic!("Expected String value");
    }
}
