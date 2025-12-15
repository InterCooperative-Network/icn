//! Performance benchmarks for CommonsManager operations
//!
//! Run with: cargo bench -p icn-gateway
//!
//! These benchmarks measure the critical path operations for Commons Evolution:
//! - Anchor creation and lookup
//! - Holder creation and lookup
//! - Charter operations

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use icn_gateway::CommonsManager;
use icn_governance::{
    Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType,
};
use icn_identity::KeyPair;
use tokio::runtime::Runtime;

fn create_test_charter(index: usize) -> Charter {
    let mut charter = Charter::new(
        OrgType::Cooperative,
        format!("coop:bench-{index}"),
        format!("Benchmark Test Coop {index}"),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );

    // Add a founder
    let keypair = KeyPair::generate().unwrap();
    let sig = icn_governance::FounderSignature {
        did: keypair.did().clone(),
        signature: vec![0u8; 64],
        timestamp: 0,
        role: Some("founder".to_string()),
    };
    charter.add_founder(sig);

    charter
}

/// Benchmark anchor creation via enrollment
fn bench_anchor_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("anchor_create", |b| {
        b.to_async(&rt).iter(|| async {
            let commons_mgr = CommonsManager::new();
            let keypair = KeyPair::generate().unwrap();
            let did = keypair.did();
            commons_mgr
                .create_anchor_from_enrollment(black_box(did), None)
                .await
                .unwrap()
        });
    });
}

/// Benchmark holder creation from anchor
fn bench_holder_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("holder_create", |b| {
        b.to_async(&rt).iter(|| async {
            let commons_mgr = CommonsManager::new();
            let keypair = KeyPair::generate().unwrap();
            let did = keypair.did();

            // Create anchor first
            let anchor = commons_mgr
                .create_anchor_from_enrollment(did, None)
                .await
                .unwrap();

            // Create holder from anchor
            let anchor_id = hex::encode(anchor.anchor.id);
            commons_mgr
                .create_holder_from_anchor(black_box(&anchor_id), black_box(did))
                .await
                .unwrap()
        });
    });
}

/// Benchmark charter creation
fn bench_charter_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("charter_create", |b| {
        let mut i = 0;
        b.to_async(&rt).iter(|| {
            i += 1;
            async move {
                let commons_mgr = CommonsManager::new();
                let charter = create_test_charter(i);
                commons_mgr.store_charter(black_box(charter)).await.unwrap()
            }
        });
    });
}

/// Benchmark anchor lookup by DID
fn bench_anchor_lookup(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let commons_mgr = CommonsManager::new();

    // Pre-populate with anchors
    let keypairs: Vec<_> = (0..100).map(|_| KeyPair::generate().unwrap()).collect();
    let dids: Vec<_> = keypairs.iter().map(|kp| kp.did().clone()).collect();

    rt.block_on(async {
        for keypair in &keypairs {
            commons_mgr
                .create_anchor_from_enrollment(keypair.did(), None)
                .await
                .unwrap();
        }
    });

    c.bench_function("anchor_lookup_by_did", |b| {
        let mut i = 0;
        b.to_async(&rt).iter(|| {
            let did = dids[i % dids.len()].clone();
            i += 1;
            let mgr = &commons_mgr;
            async move { mgr.get_anchor_by_did(black_box(&did)).await.unwrap() }
        });
    });
}

/// Benchmark holder lookup by DID
fn bench_holder_lookup(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let commons_mgr = CommonsManager::new();

    // Pre-populate with holders
    let keypairs: Vec<_> = (0..100).map(|_| KeyPair::generate().unwrap()).collect();
    let dids: Vec<_> = keypairs.iter().map(|kp| kp.did().clone()).collect();

    rt.block_on(async {
        for keypair in &keypairs {
            let anchor = commons_mgr
                .create_anchor_from_enrollment(keypair.did(), None)
                .await
                .unwrap();
            let anchor_id = hex::encode(anchor.anchor.id);
            commons_mgr
                .create_holder_from_anchor(&anchor_id, keypair.did())
                .await
                .unwrap();
        }
    });

    c.bench_function("holder_lookup_by_did", |b| {
        let mut i = 0;
        b.to_async(&rt).iter(|| {
            let did = dids[i % dids.len()].clone();
            i += 1;
            let mgr = &commons_mgr;
            async move { mgr.get_holder_by_did(black_box(&did)).await.unwrap() }
        });
    });
}

/// Benchmark charter listing (varies with count)
fn bench_charter_list_scaling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("charter_list_scaling");

    for count in [10, 50, 100, 200].iter() {
        let commons_mgr = CommonsManager::new();

        // Pre-populate with charters
        rt.block_on(async {
            for i in 0..*count {
                let charter = create_test_charter(i);
                commons_mgr.store_charter(charter).await.unwrap();
            }
        });

        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            let mgr = &commons_mgr;
            b.to_async(&rt)
                .iter(|| async { mgr.list_charters(None, None).await.unwrap() });
        });
    }

    group.finish();
}

/// Benchmark charter lookup by domain
fn bench_charter_lookup_by_domain(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let commons_mgr = CommonsManager::new();

    // Pre-populate with charters
    rt.block_on(async {
        for i in 0..100 {
            let charter = create_test_charter(i);
            commons_mgr.store_charter(charter).await.unwrap();
        }
    });

    c.bench_function("charter_lookup_by_domain", |b| {
        let mut i = 0;
        b.to_async(&rt).iter(|| {
            let domain = format!("coop:bench-{}", i % 100);
            i += 1;
            let mgr = &commons_mgr;
            async move { mgr.get_charter_by_domain(black_box(&domain)).await.unwrap() }
        });
    });
}

criterion_group!(
    benches,
    bench_anchor_creation,
    bench_holder_creation,
    bench_charter_creation,
    bench_anchor_lookup,
    bench_holder_lookup,
    bench_charter_list_scaling,
    bench_charter_lookup_by_domain,
);

criterion_main!(benches);
