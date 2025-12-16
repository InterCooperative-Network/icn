use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use icn_gossip::{VectorClock, GossipEntry};
use icn_identity::Did;

fn bench_vector_clock_merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_clock");
    
    for size in [10, 50, 100, 500].iter().copied() {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut clock1 = VectorClock::new();
            let mut clock2 = VectorClock::new();
            
            // Populate clocks
            for i in 0..size {
                let did = Did::from_anchor_id(&[i as u8; 32]);
                clock1.increment(&did);
                if i % 2 == 0 {
                    clock2.increment(&did);
                }
            }
            
            b.iter(|| {
                let mut merged = clock1.clone();
                merged.merge(black_box(&clock2));
                merged
            });
        });
    }
    
    group.finish();
}

fn bench_content_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_hash");
    
    for size in [100, 1000, 10000, 100000].iter().copied() {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let data = vec![0u8; size];
            
            b.iter(|| {
                let hash = blake3::hash(black_box(&data));
                *hash.as_bytes()
            });
        });
    }
    
    group.finish();
}

fn bench_gossip_entry_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("gossip_entry");
    
    let author = Did::from_anchor_id(&[1u8; 32]);
    let topic = "test:benchmark".to_string();
    let hash = blake3::hash(&[0u8; 1000]).into();
    let clock = VectorClock::new();
    
    let entry = GossipEntry {
        hash,
        author: author.clone(),
        clock: clock.clone(),
        topic: topic.clone(),
        data: vec![0u8; 1000],
        compressed: false,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        replica_offered: Some(true),
    };
    
    group.bench_function("serialize", |b| {
        b.iter(|| {
            bincode::serialize(black_box(&entry)).unwrap()
        });
    });
    
    let serialized = bincode::serialize(&entry).unwrap();
    
    group.bench_function("deserialize", |b| {
        b.iter(|| {
            bincode::deserialize::<GossipEntry>(black_box(&serialized)).unwrap()
        });
    });
    
    group.finish();
}

fn bench_gossip_entry_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("gossip_compression");
    
    for size in [1024, 10240, 102400].iter().copied() {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let author = Did::from_anchor_id(&[1u8; 32]);
            let hash = blake3::hash(&vec![0u8; size]).into();
            
            b.iter(|| {
                let mut entry = GossipEntry {
                    hash,
                    author: author.clone(),
                    clock: VectorClock::new(),
                    topic: "test:benchmark".to_string(),
                    data: vec![0u8; size],
                    compressed: false,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    replica_offered: None,
                };
                entry.compress().unwrap();
                entry
            });
        });
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_vector_clock_merge,
    bench_content_hash,
    bench_gossip_entry_serialization,
    bench_gossip_entry_compression
);

criterion_main!(benches);
