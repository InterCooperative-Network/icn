//! Service Registry Wiring Integration Tests
//!
//! Verifies that daemon-provided handles (governance parameter store, ledger)
//! are correctly extracted from ServiceRegistry and reused by supervisor init
//! functions, preventing double sled opens.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use tokio::sync::RwLock;

use icn_governance::{ProtocolParameterStore, SledParameterStore};
use icn_kernel_api::ServiceRegistry;
use icn_store::SledStore;

/// Test that a SledParameterStore round-trips through raw_handle correctly.
///
/// The daemon stores Arc<SledParameterStore> (concrete, Sized) and the supervisor
/// retrieves it, then upcasts to Arc<dyn ProtocolParameterStore>.
#[test]
fn test_parameter_store_raw_handle_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let param_db = sled::open(tmp.path().join("params")).unwrap();
    let store = Arc::new(SledParameterStore::new(Arc::new(param_db)).unwrap());

    // Simulate daemon: store concrete type
    let registry =
        ServiceRegistry::new().with_raw_handle("protocol_parameter_store", store.clone());

    // Simulate supervisor: retrieve and upcast
    let retrieved: Option<Arc<SledParameterStore>> =
        registry.raw_handle("protocol_parameter_store");
    assert!(
        retrieved.is_some(),
        "raw_handle should return the stored SledParameterStore"
    );

    let as_trait: Arc<dyn ProtocolParameterStore> = retrieved.unwrap();
    // Verify it's functional
    let params = as_trait.list().unwrap();
    assert!(params.is_empty(), "Fresh store should have no parameters");
}

/// Test that a Ledger handle round-trips through raw_handle correctly.
#[test]
fn test_ledger_handle_raw_handle_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(tmp.path().join("ledger")).unwrap());
    let ledger = icn_ledger::Ledger::new(store).unwrap();
    let handle = Arc::new(RwLock::new(ledger));

    // Simulate daemon: store handle
    let registry = ServiceRegistry::new().with_raw_handle("ledger", handle.clone());

    // Simulate supervisor: retrieve
    let retrieved: Option<Arc<RwLock<icn_ledger::Ledger>>> = registry.raw_handle("ledger");
    assert!(
        retrieved.is_some(),
        "raw_handle should return the stored Ledger handle"
    );

    // Verify it's the same Arc (not a copy)
    assert!(Arc::ptr_eq(&handle, &retrieved.unwrap()));
}

/// Test that raw_handle returns None for a missing key.
#[test]
fn test_raw_handle_missing_key_returns_none() {
    let registry = ServiceRegistry::new();
    let result: Option<Arc<SledParameterStore>> = registry.raw_handle("nonexistent");
    assert!(result.is_none());
}

/// Test that raw_handle returns None for a type mismatch.
#[test]
fn test_raw_handle_type_mismatch_returns_none() {
    let tmp = tempfile::tempdir().unwrap();
    let param_db = sled::open(tmp.path().join("params")).unwrap();
    let store = Arc::new(SledParameterStore::new(Arc::new(param_db)).unwrap());

    let registry = ServiceRegistry::new().with_raw_handle("protocol_parameter_store", store);

    // Try to retrieve as wrong type
    let result: Option<Arc<RwLock<icn_ledger::Ledger>>> =
        registry.raw_handle("protocol_parameter_store");
    assert!(
        result.is_none(),
        "Type mismatch should return None, not panic"
    );
}
