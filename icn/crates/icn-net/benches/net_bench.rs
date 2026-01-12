//! Performance benchmarks for icn-net
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Covers:
//! - Message serialization/deserialization
//! - Rate limiting operations
//! - DID operations for networking

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use icn_identity::{Did, KeyPair};
use icn_net::{
    MessagePayload, NetworkMessage, RateLimitConfig, RateLimiter, TrustGatedRateLimitConfig,
};
use icn_trust::TrustClass;

/// Create a test DID using a real keypair.
/// This ensures the DID contains a valid Ed25519 public key for serialization.
/// Note: The seed parameter is ignored - keypairs are randomly generated.
/// This is fine for benchmarks since we only care about serialization performance,
/// not the specific DID values.
fn test_did(_seed: u8) -> Did {
    KeyPair::generate().unwrap().did().clone()
}

/// Create a test network message
fn create_ping_message(from_seed: u8) -> NetworkMessage {
    NetworkMessage {
        version: 1,
        from: test_did(from_seed),
        to: Some(test_did(from_seed + 1)),
        payload: MessagePayload::Ping {
            sent_at: 1700000000000,
        },
        trace_context: None,
    }
}

fn bench_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialization");

    // Benchmark postcard encoding (new default)
    group.bench_function("serialize_ping_postcard", |b| {
        let msg = create_ping_message(1);
        b.iter(|| icn_encoding::encode(black_box(&msg)).unwrap());
    });

    let msg = create_ping_message(1);
    let postcard_bytes = icn_encoding::encode(&msg).unwrap();

    group.bench_function("deserialize_ping_postcard", |b| {
        b.iter(|| icn_encoding::decode::<NetworkMessage>(black_box(&postcard_bytes)).unwrap());
    });

    // Benchmark bincode legacy encoding (backward compatibility)
    group.bench_function("serialize_ping_bincode_legacy", |b| {
        let msg = create_ping_message(1);
        b.iter(|| icn_encoding::encode_bincode_legacy(black_box(&msg)).unwrap());
    });

    let bincode_bytes = icn_encoding::encode_bincode_legacy(&msg).unwrap();

    group.bench_function("deserialize_ping_bincode_legacy", |b| {
        b.iter(|| {
            icn_encoding::decode_bincode_legacy::<NetworkMessage>(black_box(&bincode_bytes))
                .unwrap()
        });
    });

    // Benchmark subscribe message serialization
    group.bench_function("serialize_subscribe_postcard", |b| {
        let msg = NetworkMessage {
            version: 1,
            from: test_did(1),
            to: None,
            payload: MessagePayload::Subscribe {
                topics: vec![
                    "ledger:sync".to_string(),
                    "gossip:announce".to_string(),
                    "compute:submit".to_string(),
                ],
            },
            trace_context: None,
        };
        b.iter(|| icn_encoding::encode(black_box(&msg)).unwrap());
    });

    // Benchmark pong message
    group.bench_function("serialize_pong_postcard", |b| {
        let msg = NetworkMessage {
            version: 1,
            from: test_did(1),
            to: Some(test_did(2)),
            payload: MessagePayload::Pong {
                ping_sent_at: 1700000000000,
                pong_sent_at: 1700000000050,
            },
            trace_context: None,
        };
        b.iter(|| icn_encoding::encode(black_box(&msg)).unwrap());
    });

    // Benchmark JSON serialization for comparison
    group.bench_function("serialize_ping_json", |b| {
        let msg = create_ping_message(1);
        b.iter(|| serde_json::to_vec(black_box(&msg)).unwrap());
    });

    // Log size comparison
    let json_bytes = serde_json::to_vec(&msg).unwrap();
    println!(
        "Postcard: {} bytes, Bincode: {} bytes, JSON: {} bytes",
        postcard_bytes.len(),
        bincode_bytes.len(),
        json_bytes.len()
    );

    group.finish();
}

fn bench_rate_limiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("rate_limiter");

    group.bench_function("create_limiter", |b| {
        b.iter(|| RateLimiter::new(black_box(RateLimitConfig::default())));
    });

    group.bench_function("check_rate_limit_allowed", |b| {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        let did = test_did(1);
        b.iter(|| limiter.check_rate_limit(black_box(&did)));
    });

    // Benchmark with many peers
    for peer_count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("check_many_peers", peer_count),
            peer_count,
            |b, &count| {
                let limiter = RateLimiter::new(RateLimitConfig::default());
                let dids: Vec<Did> = (0..count).map(|i| test_did(i as u8)).collect();

                b.iter(|| {
                    for did in &dids {
                        drop(limiter.check_rate_limit(black_box(did)));
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_trust_gated_rate_limiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_gated_rate_limiter");

    // Test different trust classes
    let trust_classes = [
        (TrustClass::Isolated, "isolated"),
        (TrustClass::Known, "known"),
        (TrustClass::Partner, "partner"),
        (TrustClass::Federated, "federated"),
    ];

    for (trust_class, name) in trust_classes.iter() {
        group.bench_function(format!("check_{name}"), |b| {
            let config = TrustGatedRateLimitConfig::default();
            let limit_config = config.for_class(*trust_class);
            let limiter = RateLimiter::new(limit_config.clone());
            let did = test_did(1);
            b.iter(|| limiter.check_rate_limit(black_box(&did)));
        });
    }

    group.finish();
}

fn bench_did_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("did_operations");

    group.bench_function("create_did", |b| {
        b.iter(|| test_did(black_box(1)));
    });

    group.bench_function("did_to_string", |b| {
        let did = test_did(1);
        b.iter(|| black_box(&did).to_string());
    });

    group.bench_function("did_comparison", |b| {
        let did1 = test_did(1);
        let did2 = test_did(2);
        b.iter(|| black_box(&did1) == black_box(&did2));
    });

    group.bench_function("did_hash", |b| {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let did = test_did(1);
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            black_box(&did).hash(&mut hasher);
            hasher.finish()
        });
    });

    group.finish();
}

fn bench_message_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_sizes");

    // Measure serialized sizes for different message types
    group.bench_function("measure_ping_size_postcard", |b| {
        let msg = create_ping_message(1);
        b.iter(|| {
            let bytes = icn_encoding::encode(black_box(&msg)).unwrap();
            bytes.len()
        });
    });

    group.bench_function("measure_ping_size_bincode_legacy", |b| {
        let msg = create_ping_message(1);
        b.iter(|| {
            let bytes = icn_encoding::encode_bincode_legacy(black_box(&msg)).unwrap();
            bytes.len()
        });
    });

    group.bench_function("measure_subscribe_size_postcard", |b| {
        let msg = NetworkMessage {
            version: 1,
            from: test_did(1),
            to: None,
            payload: MessagePayload::Subscribe {
                topics: (0..10).map(|i| format!("topic:category{i}")).collect(),
            },
            trace_context: None,
        };
        b.iter(|| {
            let bytes = icn_encoding::encode(black_box(&msg)).unwrap();
            bytes.len()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_message_serialization,
    bench_rate_limiter,
    bench_trust_gated_rate_limiter,
    bench_did_operations,
    bench_message_sizes,
);

criterion_main!(benches);
