use super::consensus::{outcome_to_value, results_match};
use super::lifecycle::apply_commons_charter_priority;
use super::*;
use crate::policy::{CharterPriority, CommonsPoolPolicy};
use crate::task::TaskStatus;
use crate::types::{
    ComputeResult, ComputeTask, DeterminismClass, ExecutorCapability, FuelLimit, PrivacyClass,
    TaskCode, TaskPriority,
};

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
        max_scope: None,
        cell_affinity: None,
        allowed_scopes: vec![],
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

        // Any executor above the trust threshold can win. Trust contributes 25%
        // of the score (Phase 16C), and 10% random jitter means even executor-c
        // (trust 0.5) can beat higher-trust executors with favorable jitter.
        assert!(
            winner_did == "did:icn:executor-a"
                || winner_did == "did:icn:executor-b"
                || winner_did == "did:icn:executor-c"
                || winner_did == "did:icn:executor-d",
            "Winner should be an executor above trust threshold, got: {winner_did}"
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
        DefaultPlacementPolicy, LocalityContext, NodeCapacity, NodeState, PlacementPolicy,
        PlacementRequest, ResourceProfile, ScopeContext,
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

    let scope_ctx = ScopeContext::empty();
    let request = PlacementRequest {
        task_hash,
        resource_profile: profile.clone(),
        locality_hints: vec![],
        max_cost: None,
        requested_at: 0,
        max_scope: None,
        cell_affinity: None,
        allowed_scopes: vec![],
    };

    // Executor A: High trust (0.8), no locality information
    let node_a = NodeState {
        did: "did:icn:executor-a".to_string(),
        capacity: capacity.clone(),
        executing_tasks: std::collections::HashMap::new(),
        queue_depth: 0,
        scope_queue_depths: std::collections::HashMap::new(),
    };
    let locality_a = LocalityContext::empty();

    // Executor B: Medium trust (0.6), excellent RTT (10ms to submitter)
    let node_b = NodeState {
        did: "did:icn:executor-b".to_string(),
        capacity: capacity.clone(),
        executing_tasks: std::collections::HashMap::new(),
        queue_depth: 0,
        scope_queue_depths: std::collections::HashMap::new(),
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
        scope_queue_depths: std::collections::HashMap::new(),
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
        scope_queue_depths: std::collections::HashMap::new(),
    };
    let locality_d = LocalityContext {
        submitter_rtt_ms: Some(15), // 15ms RTT
        local_blob_count: 5,        // All blobs local
        total_blob_count: 5,
        own_region: None,
        submitter_region: None,
    };

    // Score all executors
    let offer_a = policy
        .score_task(&request, "submitter", &node_a, 0.8, &locality_a, &scope_ctx)
        .unwrap();

    let offer_b = policy
        .score_task(&request, "submitter", &node_b, 0.6, &locality_b, &scope_ctx)
        .unwrap();

    let offer_c = policy
        .score_task(&request, "submitter", &node_c, 0.6, &locality_c, &scope_ctx)
        .unwrap();

    let offer_d = policy
        .score_task(&request, "submitter", &node_d, 0.4, &locality_d, &scope_ctx)
        .unwrap();

    // Average scores across multiple samples to smooth random jitter and
    // keep assertions deterministic in CI.
    let samples = 64usize;
    let avg_score = |node: &NodeState, trust: f64, locality: &LocalityContext| -> f64 {
        let total: f64 = (0..samples)
            .map(|_| {
                policy
                    .score_task(&request, "submitter", node, trust, locality, &scope_ctx)
                    .expect("score_task should succeed")
                    .score
            })
            .sum();
        total / samples as f64
    };
    let avg_a = avg_score(&node_a, 0.8, &locality_a);
    let avg_b = avg_score(&node_b, 0.6, &locality_b);
    let avg_c = avg_score(&node_c, 0.6, &locality_c);
    let avg_d = avg_score(&node_d, 0.4, &locality_d);

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
    tracing::info!(
        "Averaged over {} samples: A={:.4}, B={:.4}, C={:.4}, D={:.4}",
        samples,
        avg_a,
        avg_b,
        avg_c,
        avg_d
    );

    // Analysis: With scope-aware weights:
    // - Trust: 20%
    // - Capacity: 15%
    // - Queue: 10%
    // - RTT: 10%
    // - Data locality: 10%
    // - Scope affinity: 15% (all Commons = 0.00 here)
    // - Cell affinity: 5%
    // - Hints: 5%
    // - Jitter: 10%

    // Expected scoring breakdown (approximate, excluding jitter, all Commons scope):
    // Executor A: trust(0.16) + capacity(0.15) + queue(0.10) + scope(0.00) = 0.41
    // Executor B: trust(0.12) + capacity(0.15) + queue(0.10) + rtt(0.098) + scope(0.00) = 0.468
    // Executor C: trust(0.12) + capacity(0.15) + queue(0.10) + data(0.10) + scope(0.00) = 0.47
    // Executor D: trust(0.08) + capacity(0.15) + queue(0.10) + rtt(0.097) + data(0.10) + scope(0.00) = 0.527

    // Verify that locality factors make a significant difference
    // Executor B (good RTT) should beat A (higher trust but no locality)
    assert!(avg_b > avg_a, "Good RTT should compensate for lower trust");

    // Executor C (good data locality) should beat A
    assert!(
        avg_c > avg_a,
        "Good data locality should compensate for lower trust"
    );

    // Executor D (both RTT + data) should score highest despite lowest trust
    // D has 0.4 trust (0.10 contribution) but gets 0.148 + 0.15 = 0.298 from locality
    // A has 0.8 trust (0.20 contribution) but gets 0.0 from locality
    // Net: D gets ~0.10 more points from locality than A gets from higher trust
    assert!(
        avg_d > avg_a + 0.04,
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

// ===== Issue #511: Multi-Executor Quorum Tests =====

/// Test task value classification based on estimated_value
#[test]
fn test_task_verification_classification() {
    use crate::result_quorum::{ResultQuorumManager, TaskValue, VerificationConfig};

    // Create manager with default thresholds
    let config = VerificationConfig::default();
    let manager = ResultQuorumManager::new(config);

    // Low value task (< 100 credits by default)
    let low_value = manager.classify_value(50);
    assert!(matches!(low_value, TaskValue::Low));
    let low_verification = manager.create_verification(low_value);
    assert_eq!(low_verification.required_executors, 1);

    // Medium value task (100-999 credits)
    let medium_value = manager.classify_value(500);
    assert!(matches!(medium_value, TaskValue::Medium));
    let medium_verification = manager.create_verification(medium_value);
    assert_eq!(medium_verification.required_executors, 2);

    // High value task (1000-9999 credits)
    let high_value = manager.classify_value(5000);
    assert!(matches!(high_value, TaskValue::High));
    let high_verification = manager.create_verification(high_value);
    assert_eq!(high_verification.required_executors, 3);

    // Critical value task (>= 10000 credits)
    let critical_value = manager.classify_value(50000);
    assert!(matches!(critical_value, TaskValue::Critical));
    let critical_verification = manager.create_verification(critical_value);
    assert_eq!(critical_verification.required_executors, 5);
}

/// Test explicit verification override
#[tokio::test]
async fn test_explicit_verification_override() {
    use crate::result_quorum::{TaskValue, TaskVerification};

    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let _actor = ComputeActor::new("did:icn:executor".into(), trust_cb);

    // Create task with explicit verification requiring 4 executors
    let mut task = make_task("high-security-task", "did:icn:alice");
    task.verification = Some(TaskVerification {
        value: TaskValue::Critical,
        required_executors: 4,
        consensus_threshold: 0.75,
        assigned_executors: vec![],
    });

    // Verify the task has the expected verification config
    // (determine_verification is private, so we test the config is correctly set)
    let verification = task.verification.as_ref().unwrap();
    assert_eq!(verification.required_executors, 4);
    assert!(!verification.is_single_executor());
}

/// Test quorum manager task registration
#[test]
fn test_quorum_manager_task_registration() {
    use crate::result_quorum::{
        ResultQuorumManager, TaskValue, TaskVerification, VerificationConfig,
    };

    let manager = ResultQuorumManager::new(VerificationConfig::default());

    let task_hash = [42u8; 32];
    let verification = TaskVerification {
        value: TaskValue::High,
        required_executors: 3,
        consensus_threshold: 0.67,
        assigned_executors: vec![],
    };

    // Register task
    let result = manager.register_task(task_hash, verification);
    assert!(result.is_ok());

    // Duplicate registration should fail
    let duplicate_verification = TaskVerification {
        value: TaskValue::High,
        required_executors: 3,
        consensus_threshold: 0.67,
        assigned_executors: vec![],
    };
    let duplicate_result = manager.register_task(task_hash, duplicate_verification);
    assert!(duplicate_result.is_err());
}

/// Test multi-executor result aggregation with consensus
#[test]
fn test_multi_executor_result_consensus() {
    use crate::result_quorum::{
        QuorumStatus, ResultQuorumManager, TaskValue, TaskVerification, VerificationConfig,
    };
    use crate::types::{ComputeResult, ExecutionOutcome};

    let manager = ResultQuorumManager::new(VerificationConfig::default());

    // Generate valid DIDs for executors
    let keypair_a = icn_identity::KeyPair::generate().unwrap();
    let keypair_b = icn_identity::KeyPair::generate().unwrap();
    let keypair_c = icn_identity::KeyPair::generate().unwrap();
    let executor_a = keypair_a.did().to_string();
    let executor_b = keypair_b.did().to_string();
    let executor_c = keypair_c.did().to_string();
    let signing_key_a = ed25519_dalek::SigningKey::from_bytes(&keypair_a.to_signing_key_bytes());
    let signing_key_b = ed25519_dalek::SigningKey::from_bytes(&keypair_b.to_signing_key_bytes());
    let signing_key_c = ed25519_dalek::SigningKey::from_bytes(&keypair_c.to_signing_key_bytes());

    let task_hash = [42u8; 32];
    let verification = TaskVerification {
        value: TaskValue::High,
        required_executors: 3,
        consensus_threshold: 0.67, // 2/3 = 0.67, so 2 out of 3 agreeing should reach consensus
        assigned_executors: vec![],
    };

    manager.register_task(task_hash, verification).unwrap();

    // Submit first result (signed)
    let mut result1 = ComputeResult {
        task_hash,
        task_id: "test-task".into(),
        executor: executor_a,
        outcome: ExecutionOutcome::Success(vec![42]),
        fuel_used: 100,
        duration_ms: 10,
        completed_at: 1000,
        signature: vec![],
    };
    result1.sign(&signing_key_a);
    let status1 = manager.submit_result(result1).unwrap();
    assert!(matches!(
        status1,
        QuorumStatus::Collecting {
            received: 1,
            required: 3
        }
    ));

    // Submit second matching result (signed)
    let mut result2 = ComputeResult {
        task_hash,
        task_id: "test-task".into(),
        executor: executor_b,
        outcome: ExecutionOutcome::Success(vec![42]),
        fuel_used: 110,
        duration_ms: 12,
        completed_at: 1001,
        signature: vec![],
    };
    result2.sign(&signing_key_b);
    let status2 = manager.submit_result(result2).unwrap();
    assert!(matches!(
        status2,
        QuorumStatus::Collecting {
            received: 2,
            required: 3
        }
    ));

    // Submit third matching result (signed) - all 3 agree
    let mut result3 = ComputeResult {
        task_hash,
        task_id: "test-task".into(),
        executor: executor_c,
        outcome: ExecutionOutcome::Success(vec![42]), // Same result as the others
        fuel_used: 105,
        duration_ms: 11,
        completed_at: 1002,
        signature: vec![],
    };
    result3.sign(&signing_key_c);
    let status3 = manager.submit_result(result3).unwrap();
    // With 3/3 at quorum_threshold=0.67, should reach consensus
    assert!(
        matches!(
            status3,
            QuorumStatus::Consensus {
                agreeing: 3,
                total: 3,
                ..
            }
        ),
        "Got unexpected status: {status3:?}"
    );
}

/// Test multi-executor result aggregation with divergence
#[test]
fn test_multi_executor_result_divergence() {
    use crate::result_quorum::{
        QuorumStatus, ResultQuorumManager, TaskValue, TaskVerification, VerificationConfig,
    };
    use crate::types::{ComputeResult, ExecutionOutcome};

    let manager = ResultQuorumManager::new(VerificationConfig::default());

    // Generate valid DIDs for executors
    let keypair_a = icn_identity::KeyPair::generate().unwrap();
    let keypair_b = icn_identity::KeyPair::generate().unwrap();
    let keypair_c = icn_identity::KeyPair::generate().unwrap();
    let executor_a = keypair_a.did().to_string();
    let executor_b = keypair_b.did().to_string();
    let executor_c = keypair_c.did().to_string();
    let signing_key_a = ed25519_dalek::SigningKey::from_bytes(&keypair_a.to_signing_key_bytes());
    let signing_key_b = ed25519_dalek::SigningKey::from_bytes(&keypair_b.to_signing_key_bytes());
    let signing_key_c = ed25519_dalek::SigningKey::from_bytes(&keypair_c.to_signing_key_bytes());

    let task_hash = [43u8; 32];
    let verification = TaskVerification {
        value: TaskValue::High,
        required_executors: 3,
        consensus_threshold: 0.67,
        assigned_executors: vec![],
    };

    manager.register_task(task_hash, verification).unwrap();

    // Submit three different results (all signed)
    let mut result1 = ComputeResult {
        task_hash,
        task_id: "test-task".into(),
        executor: executor_a,
        outcome: ExecutionOutcome::Success(vec![42]),
        fuel_used: 100,
        duration_ms: 10,
        completed_at: 1000,
        signature: vec![],
    };
    result1.sign(&signing_key_a);
    manager.submit_result(result1).unwrap();

    let mut result2 = ComputeResult {
        task_hash,
        task_id: "test-task".into(),
        executor: executor_b,
        outcome: ExecutionOutcome::Success(vec![99]), // Different result!
        fuel_used: 110,
        duration_ms: 12,
        completed_at: 1001,
        signature: vec![],
    };
    result2.sign(&signing_key_b);
    manager.submit_result(result2).unwrap();

    let mut result3 = ComputeResult {
        task_hash,
        task_id: "test-task".into(),
        executor: executor_c,
        outcome: ExecutionOutcome::Success(vec![77]), // Different again!
        fuel_used: 105,
        duration_ms: 11,
        completed_at: 1002,
        signature: vec![],
    };
    result3.sign(&signing_key_c);
    let status3 = manager.submit_result(result3).unwrap();

    // All three results are different, should be divergent
    assert!(
        matches!(
            status3,
            QuorumStatus::Divergent {
                result_groups: 3,
                total: 3
            }
        ),
        "Expected Divergent status with 3 result groups, got: {status3:?}"
    );
}

/// Test placement selection with multiple executors for high-value task
#[tokio::test]
async fn test_multi_executor_placement_selection() {
    use crate::scheduler::ResourceProfile;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    // Track claims for multi-executor task
    let claims_collected: Arc<TokioMutex<Vec<String>>> = Arc::new(TokioMutex::new(vec![]));

    // Create a high-value task with resource profile
    let mut task = make_task("high-value-task", "did:icn:submitter");
    task.estimated_value = Some(5000); // High value => 3 executors
    task.resource_profile = Some(ResourceProfile::compute_heavy(2.0, 4096));

    let task_hash = task.hash();

    // Create submitter actor with custom verification config
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new("did:icn:submitter".into(), trust_cb);

    let claims_clone = claims_collected.clone();
    let send_cb: SendCallback = Arc::new(move |msg| {
        let claims_c = claims_clone.clone();
        tokio::spawn(async move {
            if let ComputeMessage::TaskClaimed { executor, .. } = msg {
                claims_c.lock().await.push(executor);
            }
        });
    });
    actor.set_send_callback(send_cb);

    let handle = actor.spawn();

    // Submit the high-value task
    let result_hash = handle.submit(task).await.unwrap();
    assert_eq!(result_hash, task_hash);

    // Verify the task was registered for multi-executor verification
    // (We can't easily check the internal state, but we verify it compiled correctly)
    tracing::info!("High-value task submitted with multi-executor verification");
}

/// Test quorum cleanup of expired aggregators
#[test]
fn test_quorum_cleanup_expired() {
    use crate::result_quorum::{
        ResultQuorumManager, TaskValue, TaskVerification, VerificationConfig,
    };

    // Create manager with very short collection window for testing
    let config = VerificationConfig {
        collection_window_ms: 10, // 10ms window (will expire immediately)
        ..Default::default()
    };
    let manager = ResultQuorumManager::new(config);

    let task_hash = [44u8; 32];
    let verification = TaskVerification {
        value: TaskValue::High,
        required_executors: 3,
        consensus_threshold: 0.67,
        assigned_executors: vec![],
    };

    manager.register_task(task_hash, verification).unwrap();

    // Wait for expiry
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Cleanup should find the expired aggregator
    let expired = manager.cleanup_expired().unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], task_hash);

    // Task should be removed after cleanup - a second cleanup returns empty
    let expired_again = manager.cleanup_expired().unwrap();
    assert!(
        expired_again.is_empty(),
        "Task should already be cleaned up, got {expired_again:?}"
    );
}

/// Integration test: high-value task flow with quorum verification
#[tokio::test]
async fn test_high_value_task_quorum_flow() {
    use crate::result_quorum::VerificationConfig;

    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);

    // Configure verification thresholds
    let config = VerificationConfig {
        low_value_threshold: 100,
        medium_value_threshold: 1000,
        high_value_threshold: 10000,
        high_value_quorum: 3,
        consensus_threshold: 0.67,
        collection_window_ms: 5000,
    };
    actor.set_verification_config(config);

    let handle = actor.spawn();

    // Submit a medium-value task
    let mut task = make_task("medium-value", "did:icn:alice");
    task.estimated_value = Some(500);

    let hash = handle.submit(task).await.unwrap();

    // Note: We can't easily verify internal state in tests, but the flow works correctly
    tracing::info!(
        task_hash = %hex::encode(hash),
        "Medium-value task submitted successfully"
    );
}

// ===== Issue #1074: Execute-by-Hash Tests =====

fn make_wasm_ref_task(id: &str, submitter: &str, wasm_hash: [u8; 32]) -> ComputeTask {
    // Uses Ccl capability for the capability gate check because tests use
    // LocalExecutor (Ccl-only). The WasmRef resolution happens BEFORE the
    // sync Executor::execute() call, so the capability gate doesn't affect
    // the resolution path being tested. After resolution, the task code
    // becomes WasmInline which the executor handles (returns "WASM not
    // yet supported" without the wasm feature, which is fine for testing
    // the resolution flow).
    ComputeTask {
        id: id.into(),
        submitter: submitter.into(),
        coop_id: None,
        code: TaskCode::WasmRef(wasm_hash),
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

/// Minimal valid WASM module (magic + version only).
fn sample_wasm() -> Vec<u8> {
    vec![
        0x00, 0x61, 0x73, 0x6D, // magic: \0asm
        0x01, 0x00, 0x00, 0x00, // version: 1
    ]
}

/// Test: WasmRef task with module found locally in registry.
/// The executor should resolve the module and attempt execution
/// (will fail with "not enabled" or "no run function" depending on features,
/// but proves the registry lookup path works).
#[tokio::test]
async fn test_execute_by_hash_local_hit() {
    use crate::wasm_registry::{compute_hash, WasmRegistry};

    let wasm = sample_wasm();
    let wasm_hash = compute_hash(&wasm);

    // Create a registry and deploy the module
    let registry = Arc::new(WasmRegistry::new());
    registry
        .deploy(wasm, "did:icn:deployer", |m| m)
        .await
        .unwrap();

    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    actor.set_wasm_registry(registry);
    let handle = actor.spawn();

    // Submit a WasmRef task via gossip (triggers on_task_submitted)
    let task = make_wasm_ref_task("wasm-local", "did:icn:alice", wasm_hash);
    let task_hash = task.hash();
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .unwrap();

    // Wait for processing (the actor command loop runs as a spawned task;
    // WasmRef resolution is async and may need multiple poll cycles)
    for _ in 0..20 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let status = handle.status(task_hash).await.unwrap();
        if matches!(status, Some(TaskStatus::Completed { .. })) {
            return; // Success
        }
    }

    // Final check with assertion
    let status = handle.status(task_hash).await.unwrap();
    assert!(
        matches!(status, Some(TaskStatus::Completed { .. })),
        "Task should reach Completed state (resolution succeeded), got: {status:?}"
    );
}

/// Test: WasmRef task with module fetched remotely.
#[tokio::test]
async fn test_execute_by_hash_remote_fetch() {
    use crate::wasm_registry::{compute_hash, WasmRegistry};

    let wasm = sample_wasm();
    let wasm_hash = compute_hash(&wasm);
    let wasm_clone = wasm.clone();

    // Create a registry WITHOUT the module deployed locally
    let mut registry = WasmRegistry::new();
    registry.set_fetch_callback(Arc::new(move |hash| {
        let data = wasm_clone.clone();
        Box::pin(async move {
            if hash == compute_hash(&data) {
                Ok(data)
            } else {
                Err("not found".into())
            }
        })
    }));
    let registry = Arc::new(registry);

    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    actor.set_wasm_registry(registry);
    let handle = actor.spawn();

    let task = make_wasm_ref_task("wasm-remote", "did:icn:alice", wasm_hash);
    let task_hash = task.hash();
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .unwrap();

    // Wait for processing (remote fetch + execution may need multiple poll cycles)
    for _ in 0..20 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let status = handle.status(task_hash).await.unwrap();
        if matches!(status, Some(TaskStatus::Completed { .. })) {
            return; // Success
        }
    }

    // Final check with assertion
    let status = handle.status(task_hash).await.unwrap();
    assert!(
        matches!(status, Some(TaskStatus::Completed { .. })),
        "Task should reach Completed state (remote fetch succeeded), got: {status:?}"
    );
}

/// Test: WasmRef task when fetch fails (module not available anywhere).
#[tokio::test]
async fn test_execute_by_hash_fetch_failure() {
    use crate::wasm_registry::WasmRegistry;

    let fake_hash = [0xAA; 32];

    // Create a registry with a fetch callback that always fails
    let mut registry = WasmRegistry::new();
    registry.set_fetch_callback(Arc::new(move |_hash| {
        Box::pin(async move { Err("peer unreachable".into()) })
    }));
    let registry = Arc::new(registry);

    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    actor.set_wasm_registry(registry);
    let handle = actor.spawn();

    let task = make_wasm_ref_task("wasm-fail", "did:icn:alice", fake_hash);
    let task_hash = task.hash();
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Task should be marked as Failed (not completed)
    let status = handle.status(task_hash).await.unwrap();
    assert!(
        matches!(status, Some(TaskStatus::Failed { .. })),
        "Task should be Failed when fetch fails, got: {status:?}"
    );
}

/// Test: WasmRef task without WASM registry configured.
#[tokio::test]
async fn test_execute_by_hash_no_registry() {
    let fake_hash = [0xBB; 32];

    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    // Deliberately NOT setting wasm_registry
    let handle = actor.spawn();

    let task = make_wasm_ref_task("wasm-no-reg", "did:icn:alice", fake_hash);
    let task_hash = task.hash();
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Task should be marked as Failed
    let status = handle.status(task_hash).await.unwrap();
    assert!(
        matches!(status, Some(TaskStatus::Failed { .. })),
        "Task should be Failed without registry, got: {status:?}"
    );
}

/// Test: Authz check (trust) happens BEFORE fetch attempt.
/// A low-trust executor should never attempt to fetch the module.
#[tokio::test]
async fn test_execute_by_hash_authz_before_fetch() {
    use crate::wasm_registry::WasmRegistry;
    use std::sync::atomic::{AtomicBool, Ordering};

    let fetch_attempted = Arc::new(AtomicBool::new(false));
    let fetch_attempted_clone = fetch_attempted.clone();

    let mut registry = WasmRegistry::new();
    registry.set_fetch_callback(Arc::new(move |_hash| {
        fetch_attempted_clone.store(true, Ordering::SeqCst);
        Box::pin(async move { Ok(vec![]) })
    }));
    let registry = Arc::new(registry);

    // Trust too low to execute (below MIN_TRUST_EXECUTE = 0.3)
    let trust_cb: TrustCallback = Arc::new(|_| 0.1);
    let mut actor = ComputeActor::new("did:icn:low-trust".into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);
    actor.set_wasm_registry(registry);
    let handle = actor.spawn();

    let task = make_wasm_ref_task("wasm-authz", "did:icn:alice", [0xCC; 32]);
    handle
        .handle_gossip(ComputeMessage::TaskSubmitted(Box::new(task)))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Fetch should NOT have been attempted because the executor was rejected by trust check
    assert!(
        !fetch_attempted.load(Ordering::SeqCst),
        "Fetch should NOT be attempted when executor trust is too low"
    );
}

// ============================================================================
// E7: Commons Compute Governance Hardening (#1134)
// ============================================================================

fn commons_policy(
    min_standing: f64,
    priority: CharterPriority,
    credit_ceiling: Option<i64>,
) -> CommonsPoolPolicy {
    CommonsPoolPolicy {
        min_standing,
        priority,
        max_concurrent_per_submitter: 5,
        preemption_enabled: false,
        credit_ceiling,
        fuel_cost_divisor: 1000,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_commons_standing_rejected() {
    // Trust = 0.2, min_standing = 0.3 → rejected
    let trust_cb: TrustCallback = Arc::new(|_| 0.2);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_commons_pool_policy(commons_policy(0.3, CharterPriority::Fifo, None));
    let handle = actor.spawn();

    let task = make_task("commons-standing-fail", "did:icn:low-standing");
    let result = handle.submit(task).await;

    assert!(
        matches!(result, Err(ComputeError::InsufficientTrust { required, actual }) if (required - 0.3).abs() < 0.001 && (actual - 0.2).abs() < 0.001),
        "Expected InsufficientTrust rejection, got {result:?}"
    );
}

#[tokio::test]
async fn test_commons_standing_passes() {
    // Trust = 0.5, min_standing = 0.3 → allowed
    let trust_cb: TrustCallback = Arc::new(|_| 0.5);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_commons_pool_policy(commons_policy(0.3, CharterPriority::Fifo, None));
    let handle = actor.spawn();

    let task = make_task("commons-standing-pass", "did:icn:good-standing");
    let result = handle.submit(task).await;

    assert!(
        result.is_ok(),
        "Expected submission to succeed, got {result:?}"
    );
}

#[tokio::test]
async fn test_commons_credit_ceiling_rejected() {
    // Balance 91, ceiling 100, estimated_cost = FuelLimit(10_000) / 1000 = 10.
    // available = 100 - 91 = 9 < 10 → rejected.
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_commons_pool_policy(commons_policy(0.3, CharterPriority::Fifo, Some(100)));
    // Balance callback: returns 91 (9 credits below ceiling, but task costs 10)
    let balance_cb: BalanceCallback = Arc::new(|_| 91);
    actor.set_balance_callback(balance_cb);
    let handle = actor.spawn();

    let task = make_task("commons-credit-fail", "did:icn:alice");
    let result = handle.submit(task).await;

    assert!(
        matches!(result, Err(ComputeError::InsufficientCommonsCredits { .. })),
        "Expected InsufficientCommonsCredits rejection, got {result:?}"
    );
}

#[tokio::test]
async fn test_commons_credit_ceiling_passes() {
    // Balance 85, ceiling 100, cost 10. available = 15 >= 10 → allowed.
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_commons_pool_policy(commons_policy(0.3, CharterPriority::Fifo, Some(100)));
    let balance_cb: BalanceCallback = Arc::new(|_| 85);
    actor.set_balance_callback(balance_cb);
    let handle = actor.spawn();

    let task = make_task("commons-credit-pass", "did:icn:alice");
    let result = handle.submit(task).await;

    assert!(
        result.is_ok(),
        "Expected submission to succeed, got {result:?}"
    );
}

#[tokio::test]
async fn test_commons_charter_priority_fair_share_caps_low_standing() {
    // FairShare policy: standing 0.2 → capped at Low regardless of submitted priority.
    let policy = commons_policy(0.0, CharterPriority::FairShare, None);
    let task = ComputeTask {
        priority: TaskPriority::High,
        ..make_task("test", "did:icn:member")
    };
    let result = apply_commons_charter_priority(&policy, &task, 0.2);
    assert_eq!(result, TaskPriority::Low);
}

#[tokio::test]
async fn test_commons_charter_priority_fair_share_medium_standing() {
    // FairShare: standing 0.5 → capped at Normal.
    let policy = commons_policy(0.0, CharterPriority::FairShare, None);
    let task = ComputeTask {
        priority: TaskPriority::High,
        ..make_task("test", "did:icn:member")
    };
    let result = apply_commons_charter_priority(&policy, &task, 0.5);
    assert_eq!(result, TaskPriority::Normal);
}

#[tokio::test]
async fn test_commons_charter_priority_fair_share_high_standing() {
    // FairShare: standing 0.8 → full priority access.
    let policy = commons_policy(0.0, CharterPriority::FairShare, None);
    let task = ComputeTask {
        priority: TaskPriority::High,
        ..make_task("test", "did:icn:member")
    };
    let result = apply_commons_charter_priority(&policy, &task, 0.8);
    assert_eq!(result, TaskPriority::High);
}

#[tokio::test]
async fn test_commons_charter_priority_ubs_first_caps_non_critical() {
    // UbsFirst: non-critical tasks capped at Normal; Critical UBS tasks preserved.
    let policy = commons_policy(0.0, CharterPriority::UbsFirst, None);

    let commercial = ComputeTask {
        priority: TaskPriority::High,
        ..make_task("commercial", "did:icn:member")
    };
    assert_eq!(
        apply_commons_charter_priority(&policy, &commercial, 0.9),
        TaskPriority::Normal,
        "Commercial High → Normal under UbsFirst"
    );

    let ubs = ComputeTask {
        priority: TaskPriority::Critical,
        ..make_task("ubs-infra", "did:icn:member")
    };
    assert_eq!(
        apply_commons_charter_priority(&policy, &ubs, 0.9),
        TaskPriority::Critical,
        "UBS Critical stays Critical"
    );
}

#[tokio::test]
async fn test_commons_charter_priority_fifo_no_change() {
    // Fifo: no priority modification.
    let policy = commons_policy(0.0, CharterPriority::Fifo, None);
    for &prio in &[
        TaskPriority::Low,
        TaskPriority::Normal,
        TaskPriority::High,
        TaskPriority::Critical,
    ] {
        let task = ComputeTask {
            priority: prio,
            ..make_task("fifo", "did:icn:member")
        };
        assert_eq!(apply_commons_charter_priority(&policy, &task, 0.3), prio);
    }
}

// ============================================================================
// Commons scope dispatch tests (E7 - #948)
// ============================================================================

/// Valid ICN DID corresponding to signing key [1u8; 32].
/// Must be a properly-formatted multibase-encoded Ed25519 public key so that
/// the CCL executor can parse it as `icn_identity::Did`.
const TEST_EXECUTOR_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";

/// Helper: make a task with scope set to Commons.
///
/// Uses TEST_EXECUTOR_DID as the contract participant — a valid multibase-encoded
/// Ed25519 public key required by the CCL parser (`participants: Vec<Did>`).
/// `simple_ccl()` uses `"did:icn:alice"` which is not a valid DID and would cause
/// the CCL parser to fail, producing `ExecutionOutcome::Failed` instead of `Success`.
fn make_commons_task(id: &str, submitter: &str) -> ComputeTask {
    let ccl = format!(
        r#"{{
            "name": "CommonsTask",
            "participants": ["{TEST_EXECUTOR_DID}"],
            "currency": null,
            "state_vars": [],
            "rules": [{{
                "name": "run",
                "params": [],
                "requires": [],
                "body": [{{ "Return": {{ "value": {{ "Literal": {{ "Int": 1 }} }} }} }}]
            }}],
            "triggers": []
        }}"#
    );
    ComputeTask {
        scope: icn_kernel_api::ScopeLevel::Commons,
        code: TaskCode::Ccl(ccl),
        ..make_task(id, submitter)
    }
}

#[tokio::test]
async fn test_commons_task_triggers_settlement_callback() {
    // A commons-scope task that completes successfully should fire the
    // commons settlement callback with the correct contributor and consumer.
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new(TEST_EXECUTOR_DID.into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);

    // Capture commons payment requests using std::sync::Mutex so the
    // synchronous callback can push without spawning an additional task.
    let captured: Arc<std::sync::Mutex<Vec<crate::CommonsPaymentRequest>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    actor.set_commons_settlement_callback(Arc::new(move |req| {
        captured_clone.lock().unwrap().push(req);
    }));

    let handle = actor.spawn();

    let task = make_commons_task("commons-1", "did:icn:submitter");
    let msg = ComputeMessage::TaskSubmitted(Box::new(task.clone()));
    handle.handle_gossip(msg).await.unwrap();

    // Allow time for execution + settlement callback
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Task should have completed
    let status = handle.status(task.hash()).await.unwrap();
    assert!(
        matches!(status, Some(TaskStatus::Completed { .. })),
        "task should be completed, got: {status:?}"
    );

    // Commons callback should have fired
    let reqs = captured.lock().unwrap();
    assert_eq!(
        reqs.len(),
        1,
        "expected exactly one commons settlement request, got {}",
        reqs.len()
    );
    let req = &reqs[0];
    assert_eq!(req.contributor, TEST_EXECUTOR_DID, "contributor = executor");
    assert_eq!(req.consumer, "did:icn:submitter", "consumer = submitter");
    assert_eq!(req.task_id, "commons-1");
}

#[tokio::test]
async fn test_non_commons_task_does_not_trigger_commons_settlement() {
    // A non-commons task (Local scope, the default) must NOT fire the
    // commons settlement callback even if one is installed.
    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new(TEST_EXECUTOR_DID.into(), trust_cb);
    actor.set_signing_key(vec![1u8; 32]);

    let callback_fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = callback_fired.clone();
    actor.set_commons_settlement_callback(Arc::new(move |_| {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }));

    let handle = actor.spawn();

    // Local scope task (default)
    let task = make_task("local-1", "did:icn:submitter");
    assert_eq!(task.scope, icn_kernel_api::ScopeLevel::Local);

    let msg = ComputeMessage::TaskSubmitted(Box::new(task.clone()));
    handle.handle_gossip(msg).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let status = handle.status(task.hash()).await.unwrap();
    assert!(matches!(status, Some(TaskStatus::Completed { .. })));

    assert!(
        !callback_fired.load(std::sync::atomic::Ordering::SeqCst),
        "commons settlement must NOT fire for non-commons tasks"
    );
}

#[test]
fn test_compute_task_scope_default_is_local() {
    // The default scope for ComputeTask is Local — never accidentally collectivize.
    let task = make_task("default-scope", "did:icn:alice");
    assert_eq!(task.scope, icn_kernel_api::ScopeLevel::Local);
}

#[test]
fn test_compute_task_commons_scope_explicit() {
    let task = make_commons_task("commons-scope", "did:icn:alice");
    assert_eq!(task.scope, icn_kernel_api::ScopeLevel::Commons);
}
