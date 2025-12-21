//! Trust Graph Scalability Benchmarks
//!
//! Tests trust computation performance at various network sizes to validate
//! the Phase 22 optimizations (bloom filter, priority queue, caching).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use icn_identity::Did;
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph};
use rand::Rng;
use std::sync::Arc;
use tempfile::TempDir;

/// Create a test graph with specified number of nodes
/// Each node trusts ~5-10 other nodes randomly
fn create_test_graph(size: usize) -> (TrustGraph, Vec<Did>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
    let own_did = Did::from_anchor_id(&[0u8; 32]);
    let mut graph = TrustGraph::new(store, own_did.clone());

    // Create DIDs
    let dids: Vec<Did> = (0..size)
        .map(|i| {
            let mut bytes = [0u8; 32];
            bytes[0..4].copy_from_slice(&(i as u32).to_le_bytes());
            Did::from_anchor_id(&bytes)
        })
        .collect();

    let mut rng = rand::thread_rng();

    // Add edges from own_did to ~10% of nodes (direct trust)
    let direct_count = std::cmp::max(1, size / 10);
    for _i in 0..direct_count {
        let target_idx = rng.gen_range(1..size);
        let score = 0.3 + rng.gen::<f64>() * 0.7; // 0.3 to 1.0
        let edge = TrustEdge::new(dids[0].clone(), dids[target_idx].clone(), score);
        graph.add_edge(edge).unwrap();
    }

    // Add random edges between other nodes (transitive paths)
    let edge_count = size * 5; // ~5 edges per node
    for _ in 0..edge_count {
        let source_idx = rng.gen_range(1..size);
        let target_idx = rng.gen_range(1..size);
        if source_idx != target_idx {
            let score = 0.3 + rng.gen::<f64>() * 0.7;
            let edge = TrustEdge::new(dids[source_idx].clone(), dids[target_idx].clone(), score);
            let _ = graph.add_edge(edge); // Ignore errors for duplicates
        }
    }

    // Rebuild reachability filter for accurate benchmarks
    graph.rebuild_reachability_filter().unwrap();

    (graph, dids, temp_dir)
}

/// Benchmark trust computation at various network sizes
fn bench_trust_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_computation");
    group.sample_size(50); // Reduce samples for large graphs

    // Test at various network sizes: 100, 1k, 5k, 10k
    for network_size in [100, 1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("compute", network_size),
            network_size,
            |b, &network_size| {
                let (graph, dids, _temp_dir) = create_test_graph(network_size);
                let mut rng = rand::thread_rng();

                b.iter(|| {
                    // Query a random DID
                    let target_idx = rng.gen_range(1..network_size);
                    graph.compute_trust_score(black_box(&dids[target_idx]))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark negative lookups (DIDs with no trust path)
fn bench_negative_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("negative_lookups");
    group.sample_size(100);

    for network_size in [100, 1000, 5000].iter() {
        group.bench_with_input(
            BenchmarkId::new("no_path", network_size),
            network_size,
            |b, &network_size| {
                let (graph, _dids, _temp_dir) = create_test_graph(network_size);

                // Create DIDs that definitely don't exist in the graph
                let unreachable_dids: Vec<Did> = (0..100)
                    .map(|i| {
                        let mut bytes = [0xFFu8; 32];
                        bytes[0..4].copy_from_slice(&(i as u32).to_le_bytes());
                        Did::from_anchor_id(&bytes)
                    })
                    .collect();

                let mut idx = 0;
                b.iter(|| {
                    let target = &unreachable_dids[idx % 100];
                    idx += 1;
                    graph.compute_trust_score(black_box(target))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark threshold-based queries with early termination
fn bench_threshold_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("threshold_queries");
    group.sample_size(50);

    for network_size in [1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("with_threshold", network_size),
            network_size,
            |b, &network_size| {
                let (graph, dids, _temp_dir) = create_test_graph(network_size);
                let mut rng = rand::thread_rng();

                b.iter(|| {
                    let target_idx = rng.gen_range(1..network_size);
                    graph.compute_trust_score_with_threshold(
                        black_box(&dids[target_idx]),
                        Some(0.3), // Stop when we find trust >= 0.3
                    )
                });
            },
        );
    }

    group.finish();
}

/// Benchmark cached lookups (should be O(1))
fn bench_cached_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("cached_lookups");

    for network_size in [1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("cached", network_size),
            network_size,
            |b, &network_size| {
                let (graph, dids, _temp_dir) = create_test_graph(network_size);

                // Pre-warm cache with some lookups
                let targets: Vec<usize> = (1..std::cmp::min(100, network_size)).collect();
                for &idx in &targets {
                    let _ = graph.compute_trust_score(&dids[idx]);
                }

                let mut idx = 0;
                b.iter(|| {
                    let target = &dids[targets[idx % targets.len()]];
                    idx += 1;
                    graph.compute_trust_score(black_box(target))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark edge operations
fn bench_edge_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("edge_operations");

    group.bench_function("add_edge", |b| {
        b.iter_batched(
            || {
                let temp_dir = TempDir::new().unwrap();
                let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
                let own_did = Did::from_anchor_id(&[0u8; 32]);
                (TrustGraph::new(store, own_did), temp_dir)
            },
            |(mut graph, _temp_dir)| {
                let from = Did::from_anchor_id(&[1u8; 32]);
                let to = Did::from_anchor_id(&[2u8; 32]);
                let edge = TrustEdge::new(from, to, 0.7);
                graph.add_edge(black_box(edge)).unwrap();
                graph
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("remove_edge", |b| {
        let from = Did::from_anchor_id(&[1u8; 32]);
        let to = Did::from_anchor_id(&[2u8; 32]);

        b.iter_batched(
            || {
                let temp_dir = TempDir::new().unwrap();
                let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
                let own_did = Did::from_anchor_id(&[0u8; 32]);
                let mut graph = TrustGraph::new(store, own_did);
                let edge = TrustEdge::new(from.clone(), to.clone(), 0.7);
                graph.add_edge(edge).unwrap();
                (graph, temp_dir)
            },
            |(mut graph, _temp_dir)| {
                graph.remove_edge(black_box(&from), black_box(&to)).unwrap();
                graph
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Benchmark bloom filter rebuild
fn bench_bloom_filter_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("bloom_filter");
    group.sample_size(20);

    for network_size in [1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("rebuild", network_size),
            network_size,
            |b, &network_size| {
                let (graph, _dids, _temp_dir) = create_test_graph(network_size);

                b.iter(|| graph.rebuild_reachability_filter());
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_trust_computation,
    bench_negative_lookups,
    bench_threshold_queries,
    bench_cached_lookups,
    bench_edge_operations,
    bench_bloom_filter_rebuild,
);

criterion_main!(benches);
