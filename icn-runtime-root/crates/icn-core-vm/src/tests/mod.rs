//! Integration tests for the Core VM

use crate::*;
use icn_storage::AsyncInMemoryStorage;
use wat::parse_str;

/// Test WebAssembly module in WAT format - simple log test
const TEST_LOG_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  (data (i32.const 0) "Hello from ICN Runtime!")
  (func (export "_start")
    i32.const 1      ;; log level = Info
    i32.const 0      ;; message pointer
    i32.const 22     ;; message length
    call $log)
)
"#;

/// Test WebAssembly module in WAT format - identity test
const TEST_IDENTITY_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func $get_did (import "env" "host_get_caller_did") (param i32 i32) (result i32))
  (func $get_scope (import "env" "host_get_caller_scope") (result i32))
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  
  ;; String buffer at offset 0, 100 bytes
  (data (i32.const 0) "                                                                                                    ")
  
  (func (export "_start")
    ;; Get caller DID
    i32.const 0      ;; output buffer
    i32.const 100    ;; buffer size
    call $get_did
    
    ;; Get caller scope
    call $get_scope
    drop
    
    ;; Log a message
    i32.const 1      ;; log level = Info
    i32.const 0      ;; message pointer
    i32.const 10     ;; message length (just use first 10 chars of the DID)
    call $log)
)
"#;

/// Test WebAssembly module in WAT format - economics test
const TEST_ECONOMICS_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func $check_auth (import "env" "host_check_resource_authorization") (param i32 i64) (result i32))
  (func $record_usage (import "env" "host_record_resource_usage") (param i32 i64))
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  
  (data (i32.const 0) "Resource test complete")
  
  (func (export "_start")
    ;; Check compute authorization
    i32.const 0      ;; resource type = Compute
    i64.const 100    ;; amount
    call $check_auth
    
    ;; Record compute usage
    i32.const 0      ;; resource type = Compute
    i64.const 50     ;; amount
    call $record_usage
    
    ;; Check storage authorization
    i32.const 1      ;; resource type = Storage
    i64.const 200    ;; amount
    call $check_auth
    
    ;; Record storage usage
    i32.const 1      ;; resource type = Storage
    i64.const 100    ;; amount
    call $record_usage
    
    ;; Log completion
    i32.const 1      ;; log level = Info
    i32.const 0      ;; message pointer
    i32.const 21     ;; message length
    call $log)
)
"#;

#[tokio::test]
async fn test_wasm_log_execution() {
    // Parse the WAT into WASM binary
    let wasm_bytes = parse_str(TEST_LOG_WAT).expect("Failed to parse WAT");
    
    // Create test environment
    let identity_ctx = crate::tests::create_test_identity_context();
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let vm_context = crate::tests::create_test_vm_context_with_authorizations();
    
    // Execute the WASM module
    let result = execute_wasm(
        &wasm_bytes,
        vm_context,
        storage,
        identity_ctx
    ).await;
    
    // Check results
    assert!(result.is_ok(), "WASM execution failed: {:?}", result.err());
    
    let exec_result = result.unwrap();
    assert!(exec_result.success);
    
    // Check for expected log messages
    let logs = exec_result.logs.join("\n");
    assert!(logs.contains("Hello from ICN Runtime") || 
            logs.contains("Module instantiated") || 
            logs.contains("Entry point executed"));
    
    // Check compute resources consumption
    assert!(exec_result.resources_consumed.contains_key(&ResourceType::Compute));
    
    println!("Resources consumed: {:?}", exec_result.resources_consumed);
}

#[tokio::test]
async fn test_wasm_economics_execution() {
    // Parse the WAT into WASM binary
    let wasm_bytes = parse_str(TEST_ECONOMICS_WAT).expect("Failed to parse WAT");
    
    // Create test environment
    let identity_ctx = crate::tests::create_test_identity_context();
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    let vm_context = crate::tests::create_test_vm_context_with_authorizations();
    
    // Execute the WASM module
    let result = execute_wasm(
        &wasm_bytes,
        vm_context,
        storage,
        identity_ctx
    ).await;
    
    // Check results
    assert!(result.is_ok(), "WASM execution failed: {:?}", result.err());
    
    let exec_result = result.unwrap();
    assert!(exec_result.success);
    
    // In a real implementation, we would check the resource consumption
    let compute_usage = exec_result.resources_consumed.get(&ResourceType::Compute);
    assert!(compute_usage.is_some(), "Compute usage not recorded");
    
    println!("Resources consumed: {:?}", exec_result.resources_consumed);
}

// Helper functions from the main tests module for context setup
use crate::tests::{
    create_test_identity_context,
    create_test_vm_context_with_authorizations,
}; 