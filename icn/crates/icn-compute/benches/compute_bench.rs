//! Performance benchmarks for icn-compute
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Covers:
//! - Scheduler decision making
//! - Resource profile operations
//! - Task serialization

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use icn_compute::{
    ComputeTask, DeterminismClass, ExecutorCapability, FuelLimit, NodeCapacity, NodeState,
    PlacementOffer, PrivacyClass, ResourceProfile, TaskCode, TaskId, TaskPriority,
};
use icn_identity::Did;
use std::collections::HashMap;

/// Create a test DID from a seed byte
fn test_did(seed: u8) -> Did {
    Did::from_anchor_id(&[seed; 32])
}

/// Create a test compute task
fn create_test_task(id: u8) -> ComputeTask {
    ComputeTask {
        id: TaskId::new(),
        submitter: test_did(id).to_string(),
        coop_id: None,
        code: TaskCode::Ccl("(rule test (allow true))".to_string()),
        inputs: vec![1, 2, 3, 4],
        fuel_limit: FuelLimit::default(),
        priority: TaskPriority::Normal,
        created_at: 0,
        deadline: None,
        payment_rate: None,
        payment_currency: None,
        required_capabilities: vec![ExecutorCapability::Ccl],
        resource_profile: Some(ResourceProfile::minimal()),
        actor_mode: None,
        placement_constraints: None,
        federation_constraints: None,
        estimated_value: None,
        verification: None,
        inputs_hash: None,
        policy_hash: None,
        determinism_class: DeterminismClass::default(),
        privacy_class: PrivacyClass::default(),
    }
}

/// Create a test node capacity
fn create_node_capacity(available_factor: f64) -> NodeCapacity {
    NodeCapacity {
        cpu_cores_total: 8.0,
        cpu_cores_available: 8.0 * available_factor,
        memory_mb_total: 16384,
        memory_mb_available: (16384.0 * available_factor) as u64,
        storage_mb_available: 100000,
        network_mbps: 1000.0,
        gpu_devices: vec![],
        updated_at: 0,
    }
}

fn bench_resource_profile_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("resource_profile");

    group.bench_function("create_minimal", |b| {
        b.iter(ResourceProfile::minimal);
    });

    group.bench_function("create_compute_heavy", |b| {
        b.iter(|| ResourceProfile::compute_heavy(black_box(4.0), black_box(8192)));
    });

    group.bench_function("validate_profile", |b| {
        let profile = ResourceProfile::compute_heavy(4.0, 8192);
        b.iter(|| profile.validate());
    });

    // Benchmark can_fit operations
    group.bench_function("capacity_can_fit", |b| {
        let capacity = create_node_capacity(1.0);
        let profile = ResourceProfile::compute_heavy(2.0, 4096);
        b.iter(|| capacity.can_fit(black_box(&profile)));
    });

    group.finish();
}

fn bench_task_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_operations");

    group.bench_function("create_task", |b| {
        b.iter(|| create_test_task(black_box(1)));
    });

    group.bench_function("task_id_generation", |b| {
        b.iter(TaskId::new);
    });

    // Benchmark task hash computation
    group.bench_function("task_hash", |b| {
        let task = create_test_task(1);
        b.iter(|| task.hash());
    });

    // Benchmark postcard encoding (new default)
    group.bench_function("serialize_task_postcard", |b| {
        let task = create_test_task(1);
        b.iter(|| icn_encoding::encode(black_box(&task)).unwrap());
    });

    let task = create_test_task(1);
    let serialized_postcard = icn_encoding::encode(&task).unwrap();

    group.bench_function("deserialize_task_postcard", |b| {
        b.iter(|| icn_encoding::decode::<ComputeTask>(black_box(&serialized_postcard)).unwrap());
    });

    // Benchmark versioned storage encoding (with version prefix)
    group.bench_function("serialize_task_versioned", |b| {
        let task = create_test_task(1);
        b.iter(|| icn_encoding::encode_versioned(black_box(&task)).unwrap());
    });

    let serialized_versioned = icn_encoding::encode_versioned(&task).unwrap();

    group.bench_function("deserialize_task_versioned", |b| {
        b.iter(|| {
            icn_encoding::decode_versioned::<ComputeTask>(black_box(&serialized_versioned)).unwrap()
        });
    });

    // Benchmark task serialization with JSON (for comparison)
    group.bench_function("serialize_task_json", |b| {
        let task = create_test_task(1);
        b.iter(|| serde_json::to_vec(black_box(&task)).unwrap());
    });

    // Log size comparison
    let json_bytes = serde_json::to_vec(&task).unwrap();
    println!(
        "Postcard (wire): {} bytes, Postcard (storage): {} bytes, JSON: {} bytes",
        serialized_postcard.len(),
        serialized_versioned.len(),
        json_bytes.len()
    );

    group.finish();
}

fn bench_placement_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("placement_scoring");

    // Create test node states
    fn create_node_states(count: usize) -> HashMap<String, NodeState> {
        (0..count)
            .map(|i| {
                let did = format!("did:icn:node{i}");
                let state = NodeState {
                    did: did.clone(),
                    capacity: NodeCapacity {
                        cpu_cores_total: 8.0,
                        cpu_cores_available: 8.0 - (i as f64 * 0.5),
                        memory_mb_total: 16384,
                        memory_mb_available: 16384 - (i as u64 * 1024),
                        storage_mb_available: 100000,
                        network_mbps: 1000.0,
                        gpu_devices: vec![],
                        updated_at: 0,
                    },
                    executing_tasks: HashMap::new(),
                    queue_depth: i,
                    scope_queue_depths: HashMap::new(),
                };
                (did, state)
            })
            .collect()
    }

    for node_count in [5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("find_best_node", node_count),
            node_count,
            |b, &count| {
                let nodes = create_node_states(count);
                let profile = ResourceProfile::compute_heavy(2.0, 4096);

                b.iter(|| {
                    // Simulate finding best placement among nodes
                    let mut best_score = f64::MIN;
                    let mut best_node = None;
                    for (did, state) in nodes.iter() {
                        if state.capacity.can_fit(&profile) {
                            // Simple scoring: available resources minus queue depth penalty
                            let score = state.capacity.cpu_cores_available
                                - (state.queue_depth as f64 * 0.5);
                            if score > best_score {
                                best_score = score;
                                best_node = Some(did.clone());
                            }
                        }
                    }
                    black_box(best_node)
                });
            },
        );
    }

    group.finish();
}

fn bench_offer_evaluation(c: &mut Criterion) {
    let mut group = c.benchmark_group("offer_evaluation");

    group.bench_function("create_offer", |b| {
        b.iter(|| PlacementOffer {
            executor: "did:icn:executor1".to_string(),
            score: 0.85,
            cost: 100,
            estimated_start: 1700000000000,
            offered_at: 1700000000000,
        });
    });

    // Benchmark sorting offers by score
    for offer_count in [5, 10, 25].iter() {
        group.bench_with_input(
            BenchmarkId::new("sort_offers", offer_count),
            offer_count,
            |b, &count| {
                let offers: Vec<PlacementOffer> = (0..count)
                    .map(|i| PlacementOffer {
                        executor: format!("did:icn:executor{i}"),
                        score: 0.5 + (i as f64 * 0.02),
                        cost: 100 + i as u64,
                        estimated_start: 1700000000000,
                        offered_at: 1700000000000,
                    })
                    .collect();

                b.iter(|| {
                    let mut offers = offers.clone();
                    // Use total_cmp for deterministic ordering (handles NaN)
                    offers.sort_by(|a, b| b.score.total_cmp(&a.score));
                    black_box(offers)
                });
            },
        );
    }

    group.finish();
}

fn bench_node_capacity(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_capacity");

    group.bench_function("can_fit_minimal", |b| {
        let capacity = create_node_capacity(1.0);
        let profile = ResourceProfile::minimal();
        b.iter(|| capacity.can_fit(black_box(&profile)));
    });

    group.bench_function("can_fit_heavy", |b| {
        let capacity = create_node_capacity(1.0);
        let profile = ResourceProfile::compute_heavy(4.0, 8192);
        b.iter(|| capacity.can_fit(black_box(&profile)));
    });

    group.bench_function("can_fit_insufficient", |b| {
        let capacity = create_node_capacity(0.1); // Only 10% available
        let profile = ResourceProfile::compute_heavy(4.0, 8192);
        b.iter(|| capacity.can_fit(black_box(&profile)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_resource_profile_operations,
    bench_task_operations,
    bench_placement_scoring,
    bench_offer_evaluation,
    bench_node_capacity,
);

criterion_main!(benches);
