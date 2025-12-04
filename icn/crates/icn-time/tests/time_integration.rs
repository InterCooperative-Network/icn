//! Time Sync Integration Tests
//!
//! Tests for the icn-time crate covering clock synchronization,
//! timestamp validation, and freshness checking.

use icn_time::{ClockSync, RoughTimeServer, TimeError, MAX_CLOCK_SKEW};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Helper to get current unix timestamp in milliseconds
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// =============================================================================
// ClockSync Lifecycle Tests
// =============================================================================

#[test]
fn test_clock_sync_default_creation() {
    let clock = ClockSync::new();

    // Should start unsynchronized
    assert!(clock.last_sync.is_none());
    assert_eq!(clock.offset_millis, 0);
    assert_eq!(clock.max_clock_skew, MAX_CLOCK_SKEW);
}

#[test]
fn test_clock_sync_with_custom_servers() {
    let servers = vec![
        RoughTimeServer {
            addr: "roughtime.cloudflare.com:2002".to_string(),
            public_key: None,
        },
        RoughTimeServer {
            addr: "roughtime.google.com:2002".to_string(),
            public_key: None,
        },
    ];

    let clock = ClockSync::with_servers(servers, Duration::from_secs(600));

    assert!(clock.last_sync.is_none());
    assert_eq!(clock.max_clock_skew, Duration::from_secs(600));
}

#[test]
fn test_clock_sync_with_empty_servers() {
    let servers: Vec<RoughTimeServer> = vec![];
    let clock = ClockSync::with_servers(servers, MAX_CLOCK_SKEW);

    assert!(clock.last_sync.is_none());
}

#[test]
fn test_clock_sync_default_servers() {
    let servers = ClockSync::default_servers();

    // Should have at least 3 servers (MIN_SERVERS requirement)
    assert!(servers.len() >= 3);

    // Each server should have a valid address
    for server in &servers {
        assert!(!server.addr.is_empty());
        assert!(server.addr.contains(':'));
    }
}

// =============================================================================
// RoughTimeServer Tests
// =============================================================================

#[test]
fn test_rough_time_server_creation() {
    let server = RoughTimeServer {
        addr: "roughtime.example.com:2002".to_string(),
        public_key: None,
    };

    assert_eq!(server.addr, "roughtime.example.com:2002");
    assert!(server.public_key.is_none());
}

#[test]
fn test_rough_time_server_with_public_key() {
    let server = RoughTimeServer {
        addr: "roughtime.example.com:2002".to_string(),
        public_key: Some("abcdef123456".to_string()),
    };

    assert_eq!(server.addr, "roughtime.example.com:2002");
    assert_eq!(server.public_key, Some("abcdef123456".to_string()));
}

#[test]
fn test_rough_time_server_clone() {
    let server = RoughTimeServer {
        addr: "roughtime.example.com:2002".to_string(),
        public_key: None,
    };

    let cloned = server.clone();
    assert_eq!(cloned.addr, server.addr);
}

// =============================================================================
// Timestamp Validation Tests (without sync)
// =============================================================================

#[test]
fn test_validate_timestamp_not_synchronized() {
    let clock = ClockSync::new();
    let timestamp = now_millis();

    let result = clock.validate_timestamp(timestamp);
    assert!(result.is_err());

    match result {
        Err(TimeError::NotSynchronized) => {} // Expected
        other => panic!("Expected NotSynchronized error, got {:?}", other),
    }
}

#[test]
fn test_network_time_not_synchronized() {
    let clock = ClockSync::new();

    let result = clock.network_time();
    assert!(result.is_err());

    match result {
        Err(TimeError::NotSynchronized) => {} // Expected
        other => panic!("Expected NotSynchronized error, got {:?}", other),
    }
}

// =============================================================================
// Freshness Tests
// =============================================================================

#[test]
fn test_is_fresh_not_synchronized() {
    let clock = ClockSync::new();

    // Not synchronized should not be fresh
    assert!(!clock.is_fresh());
}

#[test]
fn test_is_fresh_after_sync() {
    let mut clock = ClockSync::new();

    // Simulate sync
    clock.last_sync = Some(Instant::now());

    // Should be fresh immediately after sync
    assert!(clock.is_fresh());
}

// =============================================================================
// Manual Sync State Tests (simulating synchronized state)
// =============================================================================

#[test]
fn test_manual_sync_state() {
    let mut clock = ClockSync::new();

    // Manually set sync state for testing
    clock.offset_millis = 0;
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    assert!(clock.last_sync.is_some());
    assert_eq!(clock.offset_millis, 0);
    assert_eq!(clock.uncertainty, Duration::from_millis(100));
}

#[test]
fn test_validate_timestamp_within_skew() {
    let mut clock = ClockSync::new();
    clock.max_clock_skew = Duration::from_secs(300); // 5 minutes
    clock.offset_millis = 0;
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    let timestamp = now_millis();
    let result = clock.validate_timestamp(timestamp);

    assert!(result.is_ok());
}

#[test]
fn test_validate_timestamp_too_old() {
    let mut clock = ClockSync::new();
    clock.max_clock_skew = Duration::from_secs(300); // 5 minutes
    clock.offset_millis = 0;
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    // Timestamp from 10 minutes ago (beyond 5 min skew)
    let old_timestamp = now_millis() - 600_000;
    let result = clock.validate_timestamp(old_timestamp);

    assert!(result.is_err());
    match result {
        Err(TimeError::TimestampOutOfRange(..)) => {} // Expected
        other => panic!("Expected TimestampOutOfRange error, got {:?}", other),
    }
}

#[test]
fn test_validate_timestamp_in_future() {
    let mut clock = ClockSync::new();
    clock.max_clock_skew = Duration::from_secs(300); // 5 minutes
    clock.offset_millis = 0;
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    // Timestamp 10 minutes in the future (beyond 5 min skew)
    let future_timestamp = now_millis() + 600_000;
    let result = clock.validate_timestamp(future_timestamp);

    assert!(result.is_err());
    match result {
        Err(TimeError::TimestampOutOfRange(..)) => {} // Expected
        other => panic!("Expected TimestampOutOfRange error, got {:?}", other),
    }
}

#[test]
fn test_validate_timestamp_at_skew_boundary() {
    let mut clock = ClockSync::new();
    clock.max_clock_skew = Duration::from_secs(300); // 5 minutes
    clock.offset_millis = 0;
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    // Timestamp exactly at the skew boundary (should be valid)
    let boundary_timestamp = now_millis() - 299_000; // 4:59 ago
    let result = clock.validate_timestamp(boundary_timestamp);

    assert!(result.is_ok());
}

// =============================================================================
// Network Time Tests
// =============================================================================

#[test]
fn test_network_time_synchronized() {
    let mut clock = ClockSync::new();
    clock.offset_millis = 0;
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    let result = clock.network_time();
    assert!(result.is_ok());

    let network_time = result.unwrap();
    let local_time = now_millis();

    // Should be close to current time (within a few milliseconds)
    let diff = network_time.abs_diff(local_time);
    assert!(
        diff < 1000,
        "Network time should be within 1 second of local time"
    );
}

#[test]
fn test_network_time_with_positive_offset() {
    let mut clock = ClockSync::new();
    clock.offset_millis = 1000; // +1 second offset (local ahead)
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    let result = clock.network_time();
    assert!(result.is_ok());

    let network_time = result.unwrap();
    let local_time = now_millis();

    // Network time should be behind local time by ~1 second
    // network = local - offset = local - 1000
    assert!(
        local_time > network_time,
        "With positive offset, network time should be behind local time"
    );
}

#[test]
fn test_network_time_with_negative_offset() {
    let mut clock = ClockSync::new();
    clock.offset_millis = -1000; // -1 second offset (local behind)
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    let result = clock.network_time();
    assert!(result.is_ok());

    let network_time = result.unwrap();
    let local_time = now_millis();

    // Network time should be ahead of local time by ~1 second
    // network = local - (-1000) = local + 1000
    assert!(
        network_time > local_time,
        "With negative offset, network time should be ahead of local time"
    );
}

// =============================================================================
// Clock Offset and Uncertainty Tests
// =============================================================================

#[test]
fn test_offset_positive_local_ahead() {
    let mut clock = ClockSync::new();

    // Positive offset means local clock is ahead
    clock.offset_millis = 500;
    clock.last_sync = Some(Instant::now());

    // Verify the offset interpretation
    // network_time = local_time - offset
    let local_time = 10000i64;
    let expected_network = local_time - clock.offset_millis;
    assert_eq!(expected_network, 9500);
}

#[test]
fn test_offset_negative_local_behind() {
    let mut clock = ClockSync::new();

    // Negative offset means local clock is behind
    clock.offset_millis = -500;
    clock.last_sync = Some(Instant::now());

    // Verify the offset interpretation
    // network_time = local_time - offset = local_time - (-500) = local_time + 500
    let local_time = 10000i64;
    let expected_network = local_time - clock.offset_millis;
    assert_eq!(expected_network, 10500);
}

#[test]
fn test_uncertainty_initialization() {
    let clock = ClockSync::new();

    // Initial uncertainty should be 10 seconds
    assert_eq!(clock.uncertainty, Duration::from_secs(10));
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_timestamp_zero() {
    let mut clock = ClockSync::new();
    clock.max_clock_skew = Duration::from_secs(300);
    clock.offset_millis = 0;
    clock.uncertainty = Duration::from_millis(100);
    clock.last_sync = Some(Instant::now());

    // Timestamp of 0 (Unix epoch) should be way too old
    let result = clock.validate_timestamp(0);
    assert!(result.is_err());
}

#[test]
fn test_very_large_offset() {
    let mut clock = ClockSync::new();
    clock.offset_millis = i64::MAX / 2;
    clock.last_sync = Some(Instant::now());

    // Should handle large offsets without panic
    let result = clock.network_time();
    // May return 0 due to .max(0) protection
    assert!(result.is_ok());
}

#[test]
fn test_multiple_offset_updates() {
    let mut clock = ClockSync::new();

    // First sync
    clock.offset_millis = 100;
    clock.last_sync = Some(Instant::now());
    assert_eq!(clock.offset_millis, 100);

    // Second sync with different values
    clock.offset_millis = 200;
    clock.uncertainty = Duration::from_millis(30);
    assert_eq!(clock.offset_millis, 200);
    assert_eq!(clock.uncertainty, Duration::from_millis(30));
}

#[test]
fn test_max_clock_skew_configuration() {
    let servers = ClockSync::default_servers();

    // Test with tight skew
    let tight_clock = ClockSync::with_servers(servers.clone(), Duration::from_secs(60));
    assert_eq!(tight_clock.max_clock_skew, Duration::from_secs(60));

    // Test with loose skew
    let loose_clock = ClockSync::with_servers(servers, Duration::from_secs(3600));
    assert_eq!(loose_clock.max_clock_skew, Duration::from_secs(3600));
}

#[test]
fn test_default_impl() {
    // ClockSync implements Default via Default trait
    let clock: ClockSync = Default::default();

    assert!(clock.last_sync.is_none());
    assert_eq!(clock.offset_millis, 0);
    assert_eq!(clock.max_clock_skew, MAX_CLOCK_SKEW);
}

// =============================================================================
// Async Sync Tests (marked as ignored - requires network)
// =============================================================================

#[tokio::test]
#[ignore = "Requires network access to Rough Time servers"]
async fn test_real_sync_with_servers() {
    let mut clock = ClockSync::new();

    let result = clock.sync().await;

    // This may fail if servers are unreachable
    if result.is_ok() {
        assert!(clock.last_sync.is_some());
        assert!(clock.is_fresh());
        // Offset should be reasonable (within 1 hour = 3,600,000 ms)
        assert!(clock.offset_millis.abs() < 3_600_000);
    }
}

#[tokio::test]
#[ignore = "Requires network access to Rough Time servers"]
async fn test_real_sync_insufficient_servers() {
    // Single unreachable server should fail
    let servers = vec![RoughTimeServer {
        addr: "invalid.server.example.com:2002".to_string(),
        public_key: None,
    }];
    let mut clock = ClockSync::with_servers(servers, MAX_CLOCK_SKEW);

    let result = clock.sync().await;

    assert!(result.is_err());
    assert!(clock.last_sync.is_none());
}

// =============================================================================
// Error Type Tests
// =============================================================================

#[test]
fn test_time_error_not_synchronized() {
    let err = TimeError::NotSynchronized;
    let msg = format!("{}", err);
    assert!(msg.contains("not synchronized") || msg.contains("NotSynchronized"));
}

#[test]
fn test_time_error_insufficient_responses() {
    let err = TimeError::InsufficientResponses(2, 3);
    let msg = format!("{}", err);
    assert!(msg.contains("2") && msg.contains("3"));
}

#[test]
fn test_time_error_timestamp_out_of_range() {
    let err = TimeError::TimestampOutOfRange(500_000, 300_000);
    let msg = format!("{}", err);
    // Should contain the skew values
    assert!(!msg.is_empty());
}
