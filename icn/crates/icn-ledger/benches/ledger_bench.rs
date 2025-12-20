use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use sha2::{Digest, Sha256};

fn bench_entry_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("ledger_hash");

    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let data = vec![0u8; size];

            b.iter(|| {
                let mut hasher = Sha256::new();
                hasher.update(black_box(&data));
                hasher.finalize()
            });
        });
    }

    group.finish();
}

fn bench_entry_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("ledger_serialization");

    // Simple entry structure for benchmarking
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Entry {
        id: String,
        from: String,
        to: String,
        amount: i64,
        timestamp: u64,
        description: String,
        signature: Vec<u8>,
    }

    let entry = Entry {
        id: "entry-12345".to_string(),
        from: "did:icn:alice".to_string(),
        to: "did:icn:bob".to_string(),
        amount: 100,
        timestamp: 1234567890,
        description: "Test transaction".to_string(),
        signature: vec![0u8; 64],
    };

    group.bench_function("serialize", |b| {
        b.iter(|| {
            bincode::serde::encode_to_vec(black_box(&entry), bincode::config::legacy()).unwrap()
        });
    });

    let serialized = bincode::serde::encode_to_vec(&entry, bincode::config::legacy()).unwrap();

    group.bench_function("deserialize", |b| {
        b.iter(|| {
            bincode::serde::decode_from_slice::<Entry, _>(
                black_box(&serialized),
                bincode::config::legacy(),
            )
            .unwrap()
        });
    });

    group.finish();
}

fn bench_signature_verification(c: &mut Criterion) {
    use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
    use rand::rngs::OsRng;

    let mut group = c.benchmark_group("ledger_crypto");

    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key: VerifyingKey = (&signing_key).into();

    let message = b"Test transaction for benchmarking";
    let signature: Signature = signing_key.sign(message);

    group.bench_function("sign", |b| {
        b.iter(|| signing_key.sign(black_box(message)));
    });

    group.bench_function("verify", |b| {
        b.iter(|| {
            verifying_key
                .verify(black_box(message), black_box(&signature))
                .is_ok()
        });
    });

    group.finish();
}

fn bench_balance_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("ledger_balance");

    for num_entries in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_entries),
            num_entries,
            |b, &num_entries| {
                // Simulate transactions
                let transactions: Vec<(bool, i64)> =
                    (0..num_entries).map(|i| (i % 2 == 0, 100)).collect();

                b.iter(|| {
                    let mut balance: i64 = 0;
                    for (is_credit, amount) in black_box(&transactions) {
                        if *is_credit {
                            balance += amount;
                        } else {
                            balance -= amount;
                        }
                    }
                    balance
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_entry_hashing,
    bench_entry_serialization,
    bench_signature_verification,
    bench_balance_computation
);

criterion_main!(benches);
