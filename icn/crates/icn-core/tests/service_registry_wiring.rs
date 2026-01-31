//! Bootstrap Handles Wiring Integration Tests
//!
//! Verifies that daemon-provided handles (governance parameter store, ledger)
//! can be correctly shared via typed fields, preventing double sled opens
//! and avoiding type-erased downcasting.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_governance::{ProtocolParameterStore, SledParameterStore};
use icn_store::SledStore;

/// Test that the parameter store can be upcast to the trait object
/// that governance init expects (mirrors lifecycle.rs extraction pattern).
#[test]
fn test_parameter_store_upcast() {
    let tmp = tempfile::tempdir().unwrap();
    let param_db = sled::open(tmp.path().join("params")).unwrap();
    let store = Arc::new(SledParameterStore::new(Arc::new(param_db)).unwrap());

    // Upcast to trait object — this is what lifecycle.rs does with the
    // protocol_parameter_store field from BootstrapHandles.
    let as_trait: Arc<dyn ProtocolParameterStore> = store;

    // Verify it's functional
    let params = as_trait.list().unwrap();
    assert!(params.is_empty(), "Fresh store should have no parameters");
}

/// Test that mutations through the daemon's Arc are visible through a clone.
///
/// This verifies that typed handle sharing (via BootstrapHandles fields)
/// propagates writes between daemon and supervisor, just as the old
/// raw_handle mechanism did.
#[test]
fn test_parameter_store_mutation_propagation() {
    use icn_governance::{ParameterValue, ProtocolParameter};

    let tmp = tempfile::tempdir().unwrap();
    let param_db = sled::open(tmp.path().join("params")).unwrap();
    let store = Arc::new(SledParameterStore::new(Arc::new(param_db)).unwrap());

    // Clone for "supervisor side" (same as BootstrapHandles sharing)
    let supervisor_store = store.clone();

    // Daemon writes a parameter
    let param = ProtocolParameter::new(
        "test.param",
        "Test Param",
        "A test parameter",
        ParameterValue::Integer(42),
    );
    store.set(param, None, None).unwrap();

    // Supervisor should see it
    let retrieved = supervisor_store.get("test.param").unwrap();
    assert!(retrieved.is_some(), "Supervisor should see daemon's write");
    assert_eq!(retrieved.unwrap().id, "test.param");
}

/// Verify that opening the same sled path twice fails with exclusive locking.
///
/// This test documents WHY the daemon must share store handles via
/// `BootstrapHandles` rather than letting the supervisor re-open the same
/// path. On Linux, sled uses `flock(LOCK_EX)` which rejects a second open
/// from a different file descriptor.
#[test]
fn test_sled_double_open_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("exclusive_db");

    // First open succeeds
    let _db1 = SledStore::open(&path).unwrap();

    // Second open on the same path should fail (exclusive file lock)
    let result = SledStore::open(&path);
    assert!(
        result.is_err(),
        "Opening the same sled path twice should fail due to exclusive file locking"
    );
}
