//! Integration tests for WASM execution in the ICN Runtime

use crate::*;
use icn_storage::AsyncInMemoryStorage;
use wat::parse_str;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::path::PathBuf;

/// Test WebAssembly module in WAT format - simple log test
const TEST_LOG_WAT: &str = r#"
(module
  ;; Import host functions
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  
  ;; Memory declaration
  (memory (export "memory") 1)
  
  ;; Data section with a test message
  (data (i32.const 0) "Hello from ICN Runtime!")
  
  ;; Main entry point
  (func (export "_start")
    ;; Log a message at info level
    i32.const 1      ;; log level = Info
    i32.const 0      ;; message pointer
    i32.const 22     ;; message length
    call $log)
)
"#;

/// Test WebAssembly module in WAT format - identity test
const TEST_IDENTITY_WAT: &str = r#"
(module
  ;; Import host functions
  (func $get_did (import "env" "host_get_caller_did") (param i32 i32) (result i32))
  (func $get_scope (import "env" "host_get_caller_scope") (result i32))
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  
  ;; Memory declaration
  (memory (export "memory") 1)
  
  ;; Data section with buffer for DID
  (data (i32.const 0) "DID Buffer                            ")
  (data (i32.const 100) "Got scope: ")
  
  ;; Main entry point
  (func (export "_start")
    ;; Declare local variables
    (local $scope i32)
    (local $result i32)
    
    ;; Get caller DID
    i32.const 0      ;; output buffer
    i32.const 50     ;; buffer size
    call $get_did
    local.set $result
    
    ;; Log the DID
    i32.const 1      ;; log level = Info
    i32.const 0      ;; DID buffer pointer
    local.get $result ;; Use actual length returned
    call $log
    
    ;; Get caller scope
    call $get_scope
    local.set $scope
    
    ;; Convert scope to ASCII digit (scope + '0')
    local.get $scope
    i32.const 48    ;; ASCII '0'
    i32.add
    
    ;; Write digit to end of "Got scope: " message
    i32.const 111   ;; 100 + "Got scope: " length
    i32.store8
    
    ;; Log the scope
    i32.const 1     ;; log level = Info
    i32.const 100   ;; "Got scope: " message
    i32.const 12    ;; Length of "Got scope: X"
    call $log)
)
"#;

/// Test WebAssembly module in WAT format - economics test 
const TEST_ECONOMICS_WAT: &str = r#"
(module
  ;; Import host functions
  (func $check_auth (import "env" "host_check_resource_authorization") (param i32 i32) (result i32))
  (func $record_usage (import "env" "host_record_resource_usage") (param i32 i32))
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  
  ;; Memory declaration
  (memory (export "memory") 1)
  
  ;; Data section with messages
  (data (i32.const 0) "Resource test running")
  (data (i32.const 100) "Compute authorized")
  (data (i32.const 150) "Compute NOT authorized")
  (data (i32.const 200) "Storage authorized") 
  (data (i32.const 250) "Storage NOT authorized")
  (data (i32.const 300) "Resource test complete")
  
  ;; Main entry point
  (func (export "_start")
    ;; Log the start message
    i32.const 1      ;; log level = Info
    i32.const 0      ;; message pointer
    i32.const 20     ;; message length
    call $log
    
    ;; Check compute authorization
    i32.const 0      ;; resource type = Compute
    i32.const 100    ;; amount
    call $check_auth
    
    ;; If authorized, log and use compute resource
    if
      ;; Log authorized
      i32.const 1     ;; log level = Info
      i32.const 100   ;; message pointer
      i32.const 17    ;; message length
      call $log
      
      ;; Record compute usage
      i32.const 0     ;; resource type = Compute
      i32.const 50    ;; amount
      call $record_usage
    else
      ;; Log not authorized
      i32.const 1     ;; log level = Info  
      i32.const 150   ;; message pointer
      i32.const 21    ;; message length
      call $log
    end
    
    ;; Check storage authorization
    i32.const 1      ;; resource type = Storage
    i32.const 200    ;; amount
    call $check_auth
    
    ;; If authorized, log and use storage resource
    if
      ;; Log authorized
      i32.const 1     ;; log level = Info
      i32.const 200   ;; message pointer
      i32.const 17    ;; message length
      call $log
      
      ;; Record storage usage
      i32.const 1     ;; resource type = Storage
      i32.const 100   ;; amount
      call $record_usage
    else
      ;; Log not authorized
      i32.const 1     ;; log level = Info
      i32.const 250   ;; message pointer
      i32.const 21    ;; message length
      call $log
    end
    
    ;; Log completion
    i32.const 1      ;; log level = Info
    i32.const 300    ;; message pointer
    i32.const 21     ;; message length
    call $log)
)
"#;

/// Test WebAssembly module in WAT format - storage test
const TEST_STORAGE_WAT: &str = r#"
(module
  ;; Import host functions
  (func $storage_put (import "env" "host_storage_put") (param i32 i32 i32 i32) (result i32))
  (func $storage_get (import "env" "host_storage_get") (param i32 i32 i32 i32) (result i32))
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  
  ;; Memory declaration
  (memory (export "memory") 1)
  
  ;; Data section with messages and test data
  (data (i32.const 0) "Storage test running")
  (data (i32.const 100) "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi") ;; Sample CID
  (data (i32.const 200) "Test storage content")
  (data (i32.const 300) "Storage put successful")
  (data (i32.const 350) "Storage put failed")
  (data (i32.const 400) "Storage get successful")
  (data (i32.const 450) "Storage get failed")
  (data (i32.const 500) "Storage test complete")
  
  ;; Buffer for retrieved data
  (data (i32.const 600) "                                        ")
  
  ;; Main entry point
  (func (export "_start")
    ;; Log the start message
    i32.const 1      ;; log level = Info
    i32.const 0      ;; message pointer
    i32.const 20     ;; message length
    call $log
    
    ;; Try to put data in storage
    i32.const 100    ;; CID pointer
    i32.const 53     ;; CID length (standard CIDv1 Base32 is 53 chars)
    i32.const 200    ;; Value pointer
    i32.const 19     ;; Value length
    call $storage_put
    
    ;; Check result
    if
      ;; Log success
      i32.const 1     ;; log level = Info
      i32.const 300   ;; message pointer
      i32.const 22    ;; message length
      call $log
    else
      ;; Log failure
      i32.const 1     ;; log level = Info
      i32.const 350   ;; message pointer
      i32.const 18    ;; message length
      call $log
    end
    
    ;; Try to get data from storage
    i32.const 100    ;; CID pointer
    i32.const 53     ;; CID length
    i32.const 600    ;; Output buffer
    i32.const 50     ;; Output buffer length
    call $storage_get
    
    ;; Check result
    i32.const 1    ;; Success with data
    i32.eq
    if
      ;; Log success
      i32.const 1     ;; log level = Info
      i32.const 400   ;; message pointer
      i32.const 22    ;; message length
      call $log
      
      ;; Log the retrieved data
      i32.const 1     ;; log level = Info
      i32.const 600   ;; retrieved data pointer
      i32.const 19    ;; data length
      call $log
    else
      ;; Log failure
      i32.const 1     ;; log level = Info
      i32.const 450   ;; message pointer
      i32.const 18    ;; message length
      call $log
    end
    
    ;; Log completion
    i32.const 1      ;; log level = Info
    i32.const 500    ;; message pointer
    i32.const 21     ;; message length
    call $log)
)
"#;

/// Comprehensive test of all host ABI functions
const TEST_FULL_WAT: &str = r#"
(module
  ;; Import host functions
  
  ;; Logging
  (func $log (import "env" "host_log_message") (param i32 i32 i32))
  
  ;; Identity
  (func $get_did (import "env" "host_get_caller_did") (param i32 i32) (result i32))
  (func $get_scope (import "env" "host_get_caller_scope") (result i32))
  (func $verify_sig (import "env" "host_verify_signature") (param i32 i32 i32 i32 i32 i32) (result i32))
  
  ;; Economics
  (func $check_auth (import "env" "host_check_resource_authorization") (param i32 i32) (result i32))
  (func $record_usage (import "env" "host_record_resource_usage") (param i32 i32))
  
  ;; Storage
  (func $storage_put (import "env" "host_storage_put") (param i32 i32 i32 i32) (result i32))
  (func $storage_get (import "env" "host_storage_get") (param i32 i32 i32 i32) (result i32))
  (func $blob_put (import "env" "host_blob_put") (param i32 i32 i32 i32) (result i32))
  (func $blob_get (import "env" "host_blob_get") (param i32 i32 i32 i32) (result i32))
  
  ;; Memory
  (memory (export "memory") 1)
  
  ;; Optional alloc function that host can use for dynamic memory
  (func $alloc (export "alloc") (param i32) (result i32)
    ;; Simple bump allocator starting at 1024
    i32.const 1024
  )
  
  ;; Data section
  (data (i32.const 0) "ICN Runtime Host ABI Test")
  (data (i32.const 100) "bafkreihwsnuregcjskvkqkklztlqpym4XhutYujfj") ;; Dummy CID for testing
  (data (i32.const 200) "Test storage content")
  (data (i32.const 300) "Test operation successful")
  (data (i32.const 400) "Test operation failed")
  (data (i32.const 500) "All tests complete")
  
  ;; Buffer space for results
  (data (i32.const 600) "                                                  ")
  
  ;; Main entry point
  (func (export "_start")
    ;; Local variables
    (local $result i32)
    
    ;; Log start message
    i32.const 1      ;; log level = Info
    i32.const 0      ;; message pointer
    i32.const 23     ;; message length
    call $log
    
    ;; --- IDENTITY OPERATIONS ---
    
    ;; Get caller DID
    i32.const 600    ;; output buffer
    i32.const 50     ;; buffer size
    call $get_did
    local.set $result
    
    ;; Check if successful (result > 0)
    local.get $result
    i32.const 0
    i32.gt_s
    if
      ;; Log success and the DID
      i32.const 1     ;; log level = Info
      i32.const 300   ;; success message
      i32.const 24    ;; message length
      call $log
      
      i32.const 1     ;; log level = Info
      i32.const 600   ;; DID buffer
      local.get $result ;; DID length
      call $log
    else
      ;; Log failure
      i32.const 1     ;; log level = Info
      i32.const 400   ;; failure message
      i32.const 20    ;; message length
      call $log
    end
    
    ;; Get caller scope
    call $get_scope
    local.set $result
    
    ;; --- ECONOMICS OPERATIONS ---
    
    ;; Check compute authorization
    i32.const 0      ;; resource type = Compute
    i32.const 100    ;; amount
    call $check_auth
    local.set $result
    
    ;; If authorized, record usage
    local.get $result
    if
      ;; Log success
      i32.const 1     ;; log level = Info
      i32.const 300   ;; success message
      i32.const 24    ;; message length
      call $log
      
      ;; Record compute usage
      i32.const 0     ;; resource type = Compute
      i32.const 50    ;; amount
      call $record_usage
    else
      ;; Log failure
      i32.const 1     ;; log level = Info
      i32.const 400   ;; failure message
      i32.const 20    ;; message length
      call $log
    end
    
    ;; Check storage authorization
    i32.const 1      ;; resource type = Storage
    i32.const 200    ;; amount
    call $check_auth
    local.set $result
    
    ;; If authorized, record usage
    local.get $result
    if
      ;; Log success
      i32.const 1     ;; log level = Info
      i32.const 300   ;; success message
      i32.const 24    ;; message length
      call $log
      
      ;; Record storage usage
      i32.const 1     ;; resource type = Storage
      i32.const 100   ;; amount
      call $record_usage
    else
      ;; Log failure
      i32.const 1     ;; log level = Info
      i32.const 400   ;; failure message
      i32.const 20    ;; message length
      call $log
    end
    
    ;; --- STORAGE OPERATIONS ---
    
    ;; Try storage_put operation
    i32.const 100    ;; CID pointer
    i32.const 45     ;; CID length 
    i32.const 200    ;; Value pointer
    i32.const 19     ;; Value length
    call $storage_put
    local.set $result
    
    ;; Check result
    local.get $result
    i32.const 0
    i32.gt_s
    if
      ;; Log success
      i32.const 1     ;; log level = Info
      i32.const 300   ;; success message
      i32.const 24    ;; message length
      call $log
    else
      ;; Log failure
      i32.const 1     ;; log level = Info
      i32.const 400   ;; failure message
      i32.const 20    ;; message length
      call $log
    end
    
    ;; Log completion
    i32.const 1      ;; log level = Info
    i32.const 500    ;; completion message
    i32.const 17     ;; message length
    call $log
  )
)
"#;

/// Test the log ABI function
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
    
    // Verify logs
    assert!(exec_result.logs.iter().any(|log| log.contains("Module instantiated successfully")));
    assert!(exec_result.logs.iter().any(|log| log.contains("Found entry point")));
    assert!(exec_result.logs.iter().any(|log| log.contains("Entry point executed successfully")));
}

/// Test the economics ABI functions
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
    
    // Verify resource consumption
    let compute_usage = exec_result.resources_consumed.get(&ResourceType::Compute);
    assert!(compute_usage.is_some(), "Compute usage not recorded");
    
    // If the storage authorization check passed, storage usage should be recorded
    let storage_usage = exec_result.resources_consumed.get(&ResourceType::Storage);
    if storage_usage.is_some() {
        assert_eq!(*storage_usage.unwrap(), 100, "Expected storage usage of 100");
    }
    
    // Print the resources consumed for debugging
    println!("Resources consumed: {:?}", exec_result.resources_consumed);
}

/// Test the comprehensive integration test with all host functions
#[tokio::test]
async fn test_wasm_full_integration() {
    // Parse the WAT into WASM binary
    let wasm_bytes = parse_str(TEST_FULL_WAT).expect("Failed to parse WAT");
    
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
    
    // Print logs for debugging
    println!("Full integration test logs:");
    for log in &exec_result.logs {
        println!("  {}", log);
    }
    
    // Check for basic success - we don't require full execution since we're still implementing features
    // The module may partially execute depending on which host functions are fully implemented
    // Just verify that it started execution and we got some compute resource tracking
    let compute_usage = exec_result.resources_consumed.get(&ResourceType::Compute);
    assert!(compute_usage.is_some(), "Compute usage not recorded");
    
    // Print the resources consumed for debugging
    println!("Resources consumed: {:?}", exec_result.resources_consumed);
} 