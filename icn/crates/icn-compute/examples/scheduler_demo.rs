//! Demonstration of the scheduler primitives (Phase 16A).
//!
//! This example shows how to use ResourceProfile, NodeCapacity, and
//! PlacementPolicy to implement intelligent task placement.
//!
//! Run with: cargo run --example scheduler_demo

use icn_compute::{
    DefaultPlacementPolicy, GpuDevice, NodeCapacity, NodeState, PlacementPolicy, ResourceProfile,
};
use std::collections::HashMap;

fn main() {
    println!("=== ICN Scheduler Demo (Phase 16A) ===\n");

    // Scenario: ML training job needs GPU resources
    demo_gpu_placement();

    println!("\n=== Demo Complete ===");
}

fn demo_gpu_placement() {
    println!("Scenario: ML Training Job Placement\n");

    // 1. Task resource requirements
    let task_profile = ResourceProfile::gpu(24, "sm_70".to_string());
    println!("Task Requirements:");
    println!("  - GPU: 24 GB, compute capability sm_70+");
    println!("  - CPU: 1 core");
    println!("  - RAM: 2048 MB");
    println!();

    // 2. Three candidate executors
    let executors = vec![
        // Executor A: Powerful but busy
        (
            "did:icn:executor-a",
            NodeCapacity {
                cpu_cores_total: 16.0,
                cpu_cores_available: 2.0, // Mostly busy
                memory_mb_total: 64_000,
                memory_mb_available: 8_000,
                storage_mb_available: 500_000,
                network_mbps: 10_000.0,
                gpu_devices: vec![GpuDevice {
                    device_id: "GPU-0".into(),
                    memory_gb: 40,
                    compute_capability: "sm_80".into(), // A100
                    device_name: "NVIDIA A100".into(),
                    available: true,
                }],
                updated_at: 1000,
            },
            0.85, // High trust
            5,    // Queue depth
        ),
        // Executor B: Medium power, available
        (
            "did:icn:executor-b",
            NodeCapacity {
                cpu_cores_total: 8.0,
                cpu_cores_available: 6.0, // Mostly idle
                memory_mb_total: 32_000,
                memory_mb_available: 24_000,
                storage_mb_available: 200_000,
                network_mbps: 1_000.0,
                gpu_devices: vec![GpuDevice {
                    device_id: "GPU-0".into(),
                    memory_gb: 24,
                    compute_capability: "sm_75".into(), // RTX 2080 Ti
                    device_name: "NVIDIA RTX 2080 Ti".into(),
                    available: true,
                }],
                updated_at: 1000,
            },
            0.65, // Medium trust
            1,    // Queue depth
        ),
        // Executor C: Low-end, no GPU
        (
            "did:icn:executor-c",
            NodeCapacity {
                cpu_cores_total: 4.0,
                cpu_cores_available: 3.0,
                memory_mb_total: 8_000,
                memory_mb_available: 6_000,
                storage_mb_available: 50_000,
                network_mbps: 100.0,
                gpu_devices: vec![], // No GPU
                updated_at: 1000,
            },
            0.50,
            0,
        ),
    ];

    println!("Candidate Executors:");
    for (did, capacity, trust, queue) in &executors {
        let gpu_name = capacity
            .gpu_devices
            .first()
            .map(|g| g.device_name.as_str())
            .unwrap_or("None");
        println!("  {did}: {gpu_name} | Trust: {trust} | Queue: {queue}");
    }
    println!();

    // 3. Score each executor using placement policy
    let policy = DefaultPlacementPolicy::default();
    let task_hash = [0u8; 32];

    println!("Placement Scoring:");
    let mut offers = vec![];

    for (did, capacity, trust, queue_depth) in executors {
        let node_state = NodeState {
            did: did.to_string(),
            capacity,
            executing_tasks: HashMap::new(),
            queue_depth,
        };

        // Empty locality hints and context for demo
        let empty_hints: Vec<icn_compute::LocalityHint> = vec![];
        let empty_ctx = icn_compute::LocalityContext::empty();

        match policy.score_task(
            &task_hash,
            &task_profile,
            "did:icn:submitter",
            &node_state,
            trust,
            &empty_hints,
            &empty_ctx,
        ) {
            Some(offer) => {
                println!("  {}: score = {:.3}", did, offer.score);
                println!("    - Trust component: {:.3}", (trust * 0.25).min(0.25));
                println!(
                    "    - Capacity component: {:.3}",
                    node_state.capacity.available_ratio() * 0.20
                );
                println!(
                    "    - Queue component: {:.3}",
                    (1.0 - (queue_depth as f64 / 10.0).min(1.0)) * 0.15
                );
                println!(
                    "    - Estimated start: {} ms from now",
                    node_state.queue_depth_ms()
                );
                println!("    - Cost: {} credits per 1000 fuel", offer.cost);
                offers.push((did, offer));
            }
            None => {
                println!("  {did}: REJECTED (capacity or trust)");
            }
        }
        println!();
    }

    // 4. Select winner (highest score)
    if let Some((winner_did, winner_offer)) = offers.iter().max_by(|a, b| {
        a.1.score
            .partial_cmp(&b.1.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        println!("✅ Winner: {winner_did}");
        println!("   Final Score: {:.3}", winner_offer.score);
        println!("   Cost: {} credits/1000 fuel", winner_offer.cost);
        println!(
            "   Estimated Start: +{} ms",
            winner_offer.estimated_start - 1000
        );
    } else {
        println!("❌ No suitable executor found");
    }
}
