//! Trust Service Performance Benchmarks
//!
//! Benchmarks comparing trust_score() vs trust_score_detailed() to identify
//! performance bottlenecks in the enriched trust score computation path.
//!
//! Related: Issue #1001, PR #987

#![allow(clippy::unwrap_used, clippy::expect_used)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ed25519_dalek::SigningKey;
use icn_identity::Did;
use icn_kernel_api::services::TrustService;
use icn_kernel_api::types::Did as KernelDid;
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustScore};
use icn_trust_app::TrustServiceImplTokio;
use rand::Rng;
use std::sync::Arc;
use tempfile::TempDir;

/// Generate a valid DID from a deterministic seed
fn did_from_seed(seed: u32) -> Did {
    let mut seed_bytes = [0u8; 32];
    seed_bytes[0..4].copy_from_slice(&seed.to_le_bytes());
    // Use a simple expansion to fill the rest of the seed
    for i in 4..32 {
        seed_bytes[i] = seed_bytes[i - 4]
            .wrapping_add(seed_bytes[i - 1])
            .wrapping_mul(17)
            .wrapping_add(i as u8);
    }
    // Create a valid Ed25519 keypair from the seed
    let signing_key = SigningKey::from_bytes(&seed_bytes);
    let verifying_key = signing_key.verifying_key();
    Did::from_public_key(&verifying_key)
}

/// Create a trust service with a test graph of specified size.
///
/// Generates ~size × input_edges_per_target total edges, randomly distributed across targets
/// (not evenly distributed per node). Includes ~10% direct edges from own_did.
/// This distribution models realistic trust graphs where some nodes are hubs.
fn create_test_service(
    size: usize,
    input_edges_per_target: usize,
) -> (TrustServiceImplTokio, Vec<KernelDid>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
    let store_dyn: Arc<dyn icn_store::Store> = store.clone();

    let keypair = icn_identity::KeyPair::generate().unwrap();
    let own_did = keypair.did().clone();
    let mut graph = TrustGraph::new(store.clone(), own_did.clone());

    // Create DIDs using valid Ed25519 keys from deterministic seeds
    let dids: Vec<Did> = (0..size).map(|i| did_from_seed(i as u32)).collect();

    let mut rng = rand::thread_rng();

    // Add edges from own_did to ~10% of nodes (direct trust)
    let direct_count = std::cmp::max(1, size / 10);
    for _i in 0..direct_count {
        let target_idx = rng.gen_range(1..size);
        let score = TrustScore::unchecked(0.3 + rng.gen::<f64>() * 0.7); // 0.3 to 1.0
        let edge = TrustEdge::new(dids[0].clone(), dids[target_idx].clone(), score);
        graph.add_edge(edge).unwrap();
    }

    // Add random edges between other nodes to create input edges for targets
    let edge_count = size * input_edges_per_target;
    for _ in 0..edge_count {
        let source_idx = rng.gen_range(1..size);
        let target_idx = rng.gen_range(1..size);
        if source_idx != target_idx {
            let score = TrustScore::unchecked(0.3 + rng.gen::<f64>() * 0.7);
            let edge = TrustEdge::new(dids[source_idx].clone(), dids[target_idx].clone(), score);
            let _ = graph.add_edge(edge); // Ignore errors for duplicates
        }
    }

    // Rebuild reachability filter for accurate benchmarks
    graph.rebuild_reachability_filter().unwrap();

    // Create TrustService with the graph
    let graph_locked = Arc::new(tokio::sync::RwLock::new(graph));
    let service = TrustServiceImplTokio::new(graph_locked, keypair, store_dyn);

    // Convert DIDs to kernel format
    let kernel_dids: Vec<KernelDid> = dids
        .iter()
        .map(|did| KernelDid::from(did.as_str().to_string()))
        .collect();

    (service, kernel_dids, temp_dir)
}

/// Benchmark trust_score() baseline (no SHA-256 hashing)
fn bench_trust_score_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_score_baseline");
    group.sample_size(100);

    // Create a tokio runtime for block_in_place to work
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Test at various network sizes
    for network_size in [100, 1000, 5000].iter() {
        group.bench_with_input(
            BenchmarkId::new("trust_score", network_size),
            network_size,
            |b, &network_size| {
                let (service, dids, _temp_dir) = create_test_service(network_size, 5);
                let mut rng = rand::thread_rng();

                b.iter(|| {
                    // Run in tokio context for block_in_place
                    rt.block_on(async {
                        // Query a random DID - intentionally random to measure average-case
                        // performance across varied cache hit patterns and graph positions.
                        // The comparison benchmark uses fixed targets for apples-to-apples.
                        let target_idx = rng.gen_range(1..network_size);
                        service.trust_score(black_box(&dids[target_idx]))
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark trust_score_detailed() with SHA-256 computation
fn bench_trust_score_detailed(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_score_detailed");
    group.sample_size(100);

    // Create a tokio runtime for block_in_place to work
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Test at various network sizes
    for network_size in [100, 1000, 5000].iter() {
        group.bench_with_input(
            BenchmarkId::new("with_hash", network_size),
            network_size,
            |b, &network_size| {
                let (service, dids, _temp_dir) = create_test_service(network_size, 5);
                let mut rng = rand::thread_rng();

                b.iter(|| {
                    // Run in tokio context for block_in_place
                    rt.block_on(async {
                        // Query a random DID - intentionally random to measure average-case
                        // performance across varied input edge counts and graph positions.
                        let target_idx = rng.gen_range(1..network_size);
                        service.trust_score_detailed(black_box(&dids[target_idx]))
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark comparing both methods side-by-side
fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_score_comparison");
    group.sample_size(100);

    // Create a tokio runtime for block_in_place to work
    let rt = tokio::runtime::Runtime::new().unwrap();

    let network_size = 1000;
    let (service, dids, _temp_dir) = create_test_service(network_size, 5);

    // Random target for consistent comparison
    let target_idx = 42;
    let target_did = &dids[target_idx];

    group.bench_function("trust_score_1000", |b| {
        b.iter(|| rt.block_on(async { service.trust_score(black_box(target_did)) }))
    });

    group.bench_function("trust_score_detailed_1000", |b| {
        b.iter(|| rt.block_on(async { service.trust_score_detailed(black_box(target_did)) }))
    });

    group.finish();
}

/// Benchmark SHA-256 overhead with varying input edge counts
fn bench_hash_by_input_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_by_input_count");
    group.sample_size(100);

    // Create a tokio runtime for block_in_place to work
    let rt = tokio::runtime::Runtime::new().unwrap();

    let network_size = 500;

    // Test with different numbers of input edges per target
    for input_edges in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("detailed", input_edges),
            input_edges,
            |b, &input_edges| {
                let (service, dids, _temp_dir) = create_test_service(network_size, input_edges);
                let target_idx = network_size / 2;

                b.iter(|| {
                    rt.block_on(async {
                        service.trust_score_detailed(black_box(&dids[target_idx]))
                    })
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_trust_score_baseline,
    bench_trust_score_detailed,
    bench_comparison,
    bench_hash_by_input_count
);
criterion_main!(benches);
