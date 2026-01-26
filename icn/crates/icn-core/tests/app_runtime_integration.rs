//! App Runtime Integration Tests
//!
//! Tests for app lifecycle management, concurrent shutdown, and performance.

use icn_core::apps::dispatcher::StateSnapshot;
use icn_core::apps::manifest::{KvConfig, StateConfig};
use icn_core::apps::state_factory::{AppNamespace, StateFactory};
use std::time::Instant;

/// Test that StateSnapshot handles large key counts efficiently.
///
/// This test verifies performance with >1000 keys doesn't degrade significantly.
#[tokio::test]
async fn test_state_snapshot_performance_with_large_state() {
    let factory = StateFactory::new();
    let namespace = AppNamespace::new("did:icn:perf-test", "large-state").expect("Valid namespace");
    let config = StateConfig {
        kv: vec![KvConfig {
            name: "data".to_string(),
        }],
        ..Default::default()
    };

    let state = factory.create_for_app(namespace, &config).await.unwrap();
    let kv = state.kv("data").unwrap();

    // Insert 2000 keys (exceeds the "acceptable" 1000 key threshold)
    for i in 0..2000 {
        kv.set(&format!("key:{:04}", i), vec![0u8; 100])
            .await
            .unwrap();
    }

    // Measure snapshot creation time
    let start = Instant::now();
    let snapshot = StateSnapshot::from_app_state(&state).await;
    let duration = start.elapsed();

    // Verify all keys are in snapshot
    assert_eq!(snapshot.total_key_count(), 2000);

    // Snapshot should complete in reasonable time (< 1 second for 2000 keys)
    // This is a performance regression test, not a strict requirement
    assert!(
        duration.as_secs() < 1,
        "Snapshot creation took {:?}, which exceeds 1 second threshold",
        duration
    );

    // Log timing for visibility in test output
    println!(
        "StateSnapshot with 2000 keys took {:?} ({:.2} keys/ms)",
        duration,
        2000.0 / duration.as_millis() as f64
    );
}

/// Test that snapshot key count tracking is accurate.
#[tokio::test]
async fn test_state_snapshot_key_count_accuracy() {
    let factory = StateFactory::new();
    let namespace = AppNamespace::new("did:icn:count-test", "key-count").expect("Valid namespace");
    let config = StateConfig {
        kv: vec![
            KvConfig {
                name: "store1".to_string(),
            },
            KvConfig {
                name: "store2".to_string(),
            },
        ],
        ..Default::default()
    };

    let state = factory.create_for_app(namespace, &config).await.unwrap();

    // Add keys to store1
    let kv1 = state.kv("store1").unwrap();
    for i in 0..50 {
        kv1.set(&format!("key{}", i), vec![i as u8]).await.unwrap();
    }

    // Add keys to store2
    let kv2 = state.kv("store2").unwrap();
    for i in 0..30 {
        kv2.set(&format!("key{}", i), vec![i as u8]).await.unwrap();
    }

    let snapshot = StateSnapshot::from_app_state(&state).await;

    // Total should be 50 + 30 = 80
    assert_eq!(snapshot.total_key_count(), 80);
}

/// Test that empty state produces empty snapshot.
#[tokio::test]
async fn test_state_snapshot_empty_state() {
    let factory = StateFactory::new();
    let namespace = AppNamespace::new("did:icn:empty-test", "empty").expect("Valid namespace");
    let config = StateConfig {
        kv: vec![KvConfig {
            name: "data".to_string(),
        }],
        ..Default::default()
    };

    let state = factory.create_for_app(namespace, &config).await.unwrap();
    let snapshot = StateSnapshot::from_app_state(&state).await;

    assert_eq!(snapshot.total_key_count(), 0);
}
