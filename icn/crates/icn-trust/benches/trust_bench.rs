use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use icn_identity::Did;
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph};
use std::sync::Arc;
use tempfile::TempDir;

fn bench_trust_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_graph");

    for network_size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("compute", network_size),
            network_size,
            |b, &network_size| {
                let temp_dir = TempDir::new().unwrap();
                let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
                let own_did = Did::from_anchor_id(&[0u8; 32]);
                let mut graph = TrustGraph::new(store, own_did.clone());

                // Create a network with semi-random trust edges
                let dids: Vec<Did> = (0..network_size)
                    .map(|i| Did::from_anchor_id(&[i as u8; 32]))
                    .collect();

                // Each node trusts ~5 others
                for i in 0..network_size {
                    for j in 0..5 {
                        let target_idx = (i + j + 1) % network_size;
                        let edge = TrustEdge::new(
                            dids[i].clone(),
                            dids[target_idx].clone(),
                            0.5 + (j as f64 * 0.1),
                        );
                        graph.add_edge(edge).unwrap();
                    }
                }

                let target = dids[network_size / 2].clone();

                b.iter(|| graph.compute_trust_score(black_box(&target)));
            },
        );
    }

    group.finish();
}

fn bench_trust_edge_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_operations");

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

    group.bench_function("compute_trust_score", |b| {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let own_did = Did::from_anchor_id(&[0u8; 32]);
        let mut graph = TrustGraph::new(store, own_did.clone());
        let target = Did::from_anchor_id(&[1u8; 32]);

        // Add 50 edges from own_did to create a trust path
        for i in 0..50 {
            let intermediate = Did::from_anchor_id(&[(i + 10) as u8; 32]);
            let edge1 = TrustEdge::new(
                own_did.clone(),
                intermediate.clone(),
                0.5 + (i as f64 * 0.01),
            );
            graph.add_edge(edge1).unwrap();

            let edge2 = TrustEdge::new(intermediate, target.clone(), 0.6);
            graph.add_edge(edge2).unwrap();
        }

        b.iter(|| graph.compute_trust_score(black_box(&target)));
    });

    group.finish();
}

fn bench_transitive_trust(c: &mut Criterion) {
    let mut group = c.benchmark_group("transitive_trust");

    for depth in [2, 3, 4].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(depth), depth, |b, &depth| {
            let temp_dir = TempDir::new().unwrap();
            let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
            let own_did = Did::from_anchor_id(&[0u8; 32]);
            let mut graph = TrustGraph::new(store, own_did.clone());

            // Create a chain of trust
            let chain: Vec<Did> = (0..=depth)
                .map(|i| Did::from_anchor_id(&[i as u8; 32]))
                .collect();

            for i in 0..depth {
                let edge = TrustEdge::new(chain[i].clone(), chain[i + 1].clone(), 0.8);
                graph.add_edge(edge).unwrap();
            }

            let target = chain[depth].clone();

            b.iter(|| graph.compute_trust_score(black_box(&target)));
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_trust_computation,
    bench_trust_edge_operations,
    bench_transitive_trust
);

criterion_main!(benches);
