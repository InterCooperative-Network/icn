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
    let registry = ServiceRegistry::new()
        .with_raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY, store.clone());

    // Simulate supervisor: retrieve and upcast
    let retrieved: Option<Arc<SledParameterStore>> =
        registry.raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY);
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
    let registry =
        ServiceRegistry::new().with_raw_handle(ServiceRegistry::LEDGER_KEY, handle.clone());

    // Simulate supervisor: retrieve
    let retrieved: Option<Arc<RwLock<icn_ledger::Ledger>>> =
        registry.raw_handle(ServiceRegistry::LEDGER_KEY);
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

    let registry =
        ServiceRegistry::new().with_raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY, store);

    // Try to retrieve as wrong type
    let result: Option<Arc<RwLock<icn_ledger::Ledger>>> =
        registry.raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY);
    assert!(
        result.is_none(),
        "Type mismatch should return None, not panic"
    );
}

/// Test that mutations through the daemon's Arc are visible through the supervisor's Arc.
///
/// This verifies that raw_handle truly shares the same Arc instance (not a clone),
/// so writes by the daemon propagate to the supervisor's view.
#[test]
fn test_parameter_store_mutation_propagation() {
    use icn_governance::{ParameterValue, ProtocolParameter};

    let tmp = tempfile::tempdir().unwrap();
    let param_db = sled::open(tmp.path().join("params")).unwrap();
    let store = Arc::new(SledParameterStore::new(Arc::new(param_db)).unwrap());

    // Simulate daemon: register in registry
    let registry = ServiceRegistry::new()
        .with_raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY, store.clone());

    // Simulate supervisor: retrieve from registry
    let supervisor_store: Arc<SledParameterStore> = registry
        .raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY)
        .unwrap();

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

/// Test the full daemon→supervisor extraction pattern.
///
/// Simulates the complete flow: daemon creates all services and stores,
/// registers them in ServiceRegistry, then supervisor extracts them
/// (mirroring lifecycle.rs extraction logic).
#[test]
fn test_daemon_to_supervisor_registry_extraction() {
    use icn_governance::ParameterValue;

    let tmp = tempfile::tempdir().unwrap();

    // === Daemon side: create all handles ===
    let param_db = sled::open(tmp.path().join("params")).unwrap();
    let param_store = Arc::new(SledParameterStore::new(Arc::new(param_db)).unwrap());

    let ledger_store = Arc::new(SledStore::open(tmp.path().join("ledger")).unwrap());
    let ledger_store_trait: Arc<dyn icn_store::Store> = ledger_store.clone();
    let ledger = icn_ledger::Ledger::new(ledger_store_trait).unwrap();
    let ledger_handle = Arc::new(RwLock::new(ledger));

    let registry = ServiceRegistry::new()
        .with_raw_handle(
            ServiceRegistry::PROTOCOL_PARAM_STORE_KEY,
            param_store.clone(),
        )
        .with_raw_handle(ServiceRegistry::LEDGER_KEY, ledger_handle.clone())
        .with_raw_handle(ServiceRegistry::LEDGER_STORE_KEY, ledger_store.clone());

    // === Supervisor side: extract handles (mirrors lifecycle.rs) ===

    // Ledger handle extraction (lifecycle.rs ~line 221)
    let sup_ledger: Option<Arc<RwLock<icn_ledger::Ledger>>> =
        registry.raw_handle(ServiceRegistry::LEDGER_KEY);
    assert!(sup_ledger.is_some());
    assert!(Arc::ptr_eq(&ledger_handle, sup_ledger.as_ref().unwrap()));

    // Ledger store extraction (lifecycle.rs ~line 223)
    let sup_store: Option<Arc<SledStore>> = registry.raw_handle(ServiceRegistry::LEDGER_STORE_KEY);
    assert!(sup_store.is_some());
    assert!(Arc::ptr_eq(&ledger_store, sup_store.as_ref().unwrap()));

    // Parameter store extraction with upcast (lifecycle.rs ~line 496-502)
    let sup_params: Option<Arc<SledParameterStore>> =
        registry.raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY);
    assert!(sup_params.is_some());
    let sup_params_trait: Arc<dyn icn_governance::ProtocolParameterStore> = sup_params.unwrap();

    // Verify functional: daemon writes, supervisor reads
    let param = icn_governance::ProtocolParameter::new(
        "test.integration",
        "Integration Test",
        "Verifies daemon-supervisor handle sharing",
        ParameterValue::Integer(99),
    );
    param_store.set(param, None, None).unwrap();
    let read_back = sup_params_trait.get("test.integration").unwrap();
    assert!(read_back.is_some());
    assert_eq!(read_back.unwrap().id, "test.integration");
}

/// Test that the ledger store can be shared via raw_handle.
///
/// This verifies the fix for the sled double-open bug: the daemon creates
/// a single SledStore and passes it through raw_handle so the supervisor
/// doesn't need to re-open the same sled path.
#[test]
fn test_ledger_store_shared_via_raw_handle() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(SledStore::open(tmp.path().join("ledger")).unwrap());

    let registry =
        ServiceRegistry::new().with_raw_handle(ServiceRegistry::LEDGER_STORE_KEY, store.clone());

    let retrieved: Option<Arc<SledStore>> = registry.raw_handle(ServiceRegistry::LEDGER_STORE_KEY);
    assert!(retrieved.is_some(), "Should retrieve ledger store");
    assert!(Arc::ptr_eq(&store, &retrieved.unwrap()));
}

/// Verify that opening the same sled path twice fails with exclusive locking.
///
/// This test documents WHY the daemon must share store handles via raw_handle
/// rather than letting the supervisor re-open the same path. On Linux, sled uses
/// `flock(LOCK_EX)` which rejects a second open from a different file descriptor.
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
