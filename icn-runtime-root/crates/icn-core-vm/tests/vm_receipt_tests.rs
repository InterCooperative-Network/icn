use std::sync::Arc;
use anyhow::Result;
use serde_json::Value;
use icn_core_vm::{
    ConcreteHostEnvironment, 
    VMContext, 
    execute_wasm, 
    resources::ResourceAuthorization,
    resources::ResourceType,
    credentials::{issue_execution_receipt, get_execution_receipt_by_cid},
};
use icn_identity::{IdentityManager, KeyPair};
use icn_storage::{InMemoryStorageManager, StorageManager};

// Simple test module that logs and returns a success code
const SUCCESS_MODULE_WAT: &str = r#"
(module
  ;; Import host functions
  (import "env" "host_log" (func $host_log (param i32 i32) (result i32)))
  
  ;; Memory
  (memory (export "memory") 1)
  
  ;; Log message
  (data (i32.const 1024) "Test module executed successfully")
  
  ;; Main entry point
  (func (export "execute") (result i32)
    ;; Log success message
    (call $host_log 
      (i32.const 1024)
      (i32.const 31)
    )
    (drop) ;; Ignore result
    
    ;; Return success code
    (i32.const 0)
  )
)
"#;

// Module that produces an error by dividing by zero
const ERROR_MODULE_WAT: &str = r#"
(module
  ;; Import host functions
  (import "env" "host_log" (func $host_log (param i32 i32) (result i32)))
  
  ;; Memory
  (memory (export "memory") 1)
  
  ;; Log message
  (data (i32.const 1024) "About to generate an error")
  
  ;; Main entry point
  (func (export "execute") (result i32)
    ;; Log message
    (call $host_log 
      (i32.const 1024)
      (i32.const 24)
    )
    (drop) ;; Ignore result
    
    ;; Trigger a divide by zero trap
    (i32.div_s
      (i32.const 1)
      (i32.const 0)
    )
    
    ;; This will never execute
    (i32.const 0)
  )
)
"#;

// WAT module that executes and then retrieves its receipt
const RECEIPT_RETRIEVAL_WAT: &str = r#"
(module
  ;; Import host functions
  (import "env" "host_log" (func $host_log (param i32 i32) (result i32)))
  (import "env" "host_get_execution_receipts" (func $host_get_execution_receipts (param i32 i32 i32 i32 i32) (result i32)))
  
  ;; Memory
  (memory (export "memory") 1)
  
  ;; Static data
  (data (i32.const 1024) "test-federation") ;; scope string
  (data (i32.const 2048) "Retrieving execution receipts...")
  
  ;; Buffer for results
  (global $result_buffer i32 (i32.const 4096))
  (global $result_size i32 (i32.const 8192)) ;; 8KB buffer
  
  ;; Helper to calculate string length
  (func $strlen (param $ptr i32) (result i32)
    (local $len i32)
    (local $char i32)
    
    (local.set $len (i32.const 0))
    
    (block $done
      (loop $loop
        ;; Load byte from memory
        (local.set $char (i32.load8_u (i32.add (local.get $ptr) (local.get $len))))
        
        ;; If byte is 0, break
        (br_if $done (i32.eqz (local.get $char)))
        
        ;; Increment length and continue
        (local.set $len (i32.add (local.get $len) (i32.const 1)))
        (br $loop)
      )
    )
    
    (local.get $len)
  )
  
  ;; Main entry point
  (func (export "execute") (result i32)
    (local $scope_ptr i32)
    (local $scope_len i32)
    (local $log_ptr i32)
    (local $log_len i32)
    (local $result i32)
    
    ;; Log that we're retrieving receipts
    (local.set $log_ptr (i32.const 2048))
    (local.set $log_len (call $strlen (local.get $log_ptr)))
    
    (call $host_log 
      (local.get $log_ptr)
      (local.get $log_len)
    )
    (drop) ;; Ignore log result
    
    ;; Set scope pointer and length
    (local.set $scope_ptr (i32.const 1024))
    (local.set $scope_len (call $strlen (local.get $scope_ptr)))
    
    ;; Call host_get_execution_receipts
    ;; Parameters: scope_ptr, scope_len, timestamp_ptr (0 for none), result_ptr, result_max_len
    (local.set $result 
      (call $host_get_execution_receipts
        (local.get $scope_ptr)    ;; scope_ptr
        (local.get $scope_len)    ;; scope_len
        (i32.const 0)             ;; timestamp_ptr (0 = no timestamp filter)
        (global.get $result_buffer) ;; result_ptr
        (global.get $result_size)   ;; result_max_len
      )
    )
    
    ;; Return result (number of bytes written to buffer, or error code)
    (local.get $result)
  )
)
"#;

// Helper function to create a test environment
fn create_test_environment() -> ConcreteHostEnvironment {
    // Create identity manager mock
    let identity_manager = Arc::new(TestIdentityManager::new());
    
    // Create storage manager
    let storage_manager = Arc::new(TestStorageManager::new());
    
    // Create DAG storage manager
    let dag_storage = Arc::new(InMemoryStorageManager::new());
    
    // Create test identity context
    let keypair = KeyPair::new();
    let did = "did:icn:test-executor";
    let identity_context = icn_core_vm::IdentityContext::new(keypair, did);
    
    // Set up resource authorizations
    let resource_authorizations = vec![
        ResourceAuthorization {
            resource_type: ResourceType::Compute,
            limit: 1_000_000,
        },
        ResourceAuthorization {
            resource_type: ResourceType::Storage,
            limit: 1_000_000,
        },
        ResourceAuthorization {
            resource_type: ResourceType::Network,
            limit: 1_000_000,
        },
    ];
    
    // Create VM context
    let vm_context = VMContext::new(
        Arc::new(identity_context),
        resource_authorizations,
    );
    
    // Create host environment
    ConcreteHostEnvironment::new(
        vm_context,
        storage_manager,
        identity_manager,
        None, // No parent federation
        dag_storage,
    )
}

// Test identity manager
struct TestIdentityManager;

impl TestIdentityManager {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl IdentityManager for TestIdentityManager {
    // Get key for a DID
    async fn get_key(&self, _did: &str) -> Result<Option<KeyPair>, anyhow::Error> {
        Ok(Some(KeyPair::new()))
    }
    
    // Get ID for an entity
    async fn get_identity(&self, _did: &str) -> Result<Option<icn_identity::Identity>, anyhow::Error> {
        Ok(None)
    }
    
    // Check if a DID exists
    async fn identity_exists(&self, _did: &str) -> Result<bool, anyhow::Error> {
        Ok(true)
    }
    
    // Get the scope of an identity
    async fn get_scope(&self, _did: &str) -> Result<Option<icn_identity::IdentityScope>, anyhow::Error> {
        Ok(Some(icn_identity::IdentityScope::Federation))
    }
    
    // Store an identity (returns created DID)
    async fn store_identity(&self, _identity: icn_identity::Identity) -> Result<String, anyhow::Error> {
        Ok("did:icn:new-test-identity".to_string())
    }
    
    // Verify a signature
    async fn verify_signature(&self, _did: &str, _message: &[u8], _signature: &[u8]) -> Result<bool, anyhow::Error> {
        Ok(true) // Always return true for testing
    }
    
    // List all identities in a particular scope
    async fn list_identities(&self, _scope: Option<icn_identity::IdentityScope>) -> Result<Vec<String>, anyhow::Error> {
        Ok(vec!["did:icn:test-identity".to_string()])
    }
    
    // Get JWKs for a DID
    async fn get_jwk(&self, _did: &str) -> Result<Option<icn_identity::JWK>, anyhow::Error> {
        Ok(None)
    }
    
    // Register a new public key for an existing DID
    async fn register_key(&self, _did: &str, _key: icn_identity::PublicKey) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

// Test storage manager that can track anchored data
struct TestStorageManager {
    anchored_data: tokio::sync::Mutex<Vec<(String, Vec<u8>)>>,
}

impl TestStorageManager {
    fn new() -> Self {
        Self {
            anchored_data: tokio::sync::Mutex::new(Vec::new()),
        }
    }
    
    async fn get_last_anchored_data(&self) -> Option<(String, Vec<u8>)> {
        let data = self.anchored_data.lock().await;
        data.last().cloned()
    }
    
    // List anchors by prefix
    async fn list_anchors(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, anyhow::Error> {
        let data = self.anchored_data.lock().await;
        let mut result = Vec::new();
        
        // Filter data by prefix
        for (key, value) in data.iter() {
            if key.starts_with(prefix) {
                result.push((key.clone(), value.clone()));
            }
        }
        
        Ok(result)
    }
}

#[async_trait::async_trait]
impl StorageManager for TestStorageManager {
    async fn anchor_to_dag(&self, key: &str, data: Vec<u8>) -> Result<String, anyhow::Error> {
        // Generate a mock CID
        let cid = format!("bafybeih{}", uuid::Uuid::new_v4().to_simple());
        
        // Store the data
        let mut anchored_data = self.anchored_data.lock().await;
        anchored_data.push((key.to_string(), data));
        
        Ok(cid)
    }
    
    // StorageManager trait implementation stubs
    async fn get_value(&self, _key: &str) -> Result<Option<Vec<u8>>, anyhow::Error> {
        Ok(None)
    }
    
    async fn set_value(&self, _key: &str, _value: Vec<u8>) -> Result<(), anyhow::Error> {
        Ok(())
    }
    
    async fn delete_value(&self, _key: &str) -> Result<(), anyhow::Error> {
        Ok(())
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    // Other required methods with minimal implementation
    async fn store_node(&self, _entity_did: &str, _node_builder: icn_models::DagNodeBuilder) -> Result<(icn_models::Cid, icn_models::DagNode), anyhow::Error> {
        // Generate a mock CID
        let cid = icn_models::Cid::default();
        let node = icn_models::DagNode {
            cid: cid.clone(),
            parents: vec![],
            issuer: icn_identity::IdentityId::new("did:icn:test"),
            signature: vec![],
            payload: libipld::Ipld::Null,
            metadata: icn_models::DagNodeMetadata {
                timestamp: 0,
                sequence: 0,
                content_type: None,
                tags: vec![],
            },
        };
        Ok((cid, node))
    }
    
    async fn get_node(&self, _entity_did: &str, _cid: &icn_models::Cid) -> Result<Option<icn_models::DagNode>, anyhow::Error> {
        Ok(None)
    }
    
    async fn contains_node(&self, _entity_did: &str, _cid: &icn_models::Cid) -> Result<bool, anyhow::Error> {
        Ok(false)
    }
    
    async fn get_node_bytes(&self, _entity_did: &str, _cid: &icn_models::Cid) -> Result<Option<Vec<u8>>, anyhow::Error> {
        Ok(None)
    }
    
    async fn store_new_dag_root(&self, _entity_did: &str, _node_builder: icn_models::DagNodeBuilder) -> Result<(icn_models::Cid, icn_models::DagNode), anyhow::Error> {
        // Generate a mock CID
        let cid = icn_models::Cid::default();
        let node = icn_models::DagNode {
            cid: cid.clone(),
            parents: vec![],
            issuer: icn_identity::IdentityId::new("did:icn:test"),
            signature: vec![],
            payload: libipld::Ipld::Null,
            metadata: icn_models::DagNodeMetadata {
                timestamp: 0,
                sequence: 0,
                content_type: None,
                tags: vec![],
            },
        };
        Ok((cid, node))
    }
    
    fn dag_store(&self) -> Result<&dyn icn_storage::DagStore, icn_storage::StorageError> {
        // For testing purposes, we don't need actual DagStore implementation
        Err(icn_storage::StorageError::NotImplemented("DagStore not implemented for TestStorageManager".to_string()))
    }
}

#[tokio::test]
async fn test_success_receipt_generation() -> Result<()> {
    // Create the test environment
    let host_env = create_test_environment();
    
    // Compile the WebAssembly success module
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, SUCCESS_MODULE_WAT)?;
    
    // Execute the WASM module
    let result = execute_wasm(
        &wasmtime::wat::parse_bytes(SUCCESS_MODULE_WAT.as_bytes())?, 
        None, // Use default context
        &host_env,
        Some("test-proposal-123"), // Provide a proposal ID
        Some("test-federation"), // Provide a federation scope
    ).await?;
    
    // Check the result
    assert_eq!(result.code, 0, "Execution should succeed");
    
    // Verify that an anchor CID was created
    assert!(host_env.get_last_anchor_cid().is_some(), 
           "An anchor CID should have been created");
    
    // Get the StorageManager and verify the anchored data
    let storage_manager = match host_env.storage_manager() {
        Ok(sm) => sm,
        Err(_) => panic!("Failed to get storage manager"),
    };
    
    if let Some(test_storage) = storage_manager.as_any().downcast_ref::<TestStorageManager>() {
        if let Some((key, data)) = test_storage.get_last_anchored_data().await {
            // Verify it's an ExecutionReceipt
            assert!(key.contains("ExecutionReceipt") || key.contains("execution_receipt"), 
                   "Key should contain ExecutionReceipt: {}", key);
            
            // Parse the JSON data
            let receipt_json: Value = serde_json::from_slice(&data)?;
            
            // Verify the receipt fields
            assert_eq!(receipt_json["type"], "ExecutionReceipt", "Should be an ExecutionReceipt");
            assert_eq!(receipt_json["outcome"], "Success", "Outcome should be Success");
            assert_eq!(receipt_json["execution_id"], "test-proposal-123", "Execution ID should match");
            assert_eq!(receipt_json["scope"], "test-federation", "Scope should match");
            assert!(receipt_json["resource_usage"].is_object(), "Resource usage should be an object");
            assert!(receipt_json["timestamp"].is_number(), "Timestamp should be a number");
        } else {
            panic!("No anchored data found");
        }
    } else {
        panic!("Failed to downcast storage manager to TestStorageManager");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_error_receipt_generation() -> Result<()> {
    // Create the test environment
    let host_env = create_test_environment();
    
    // Compile the WebAssembly error module
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, ERROR_MODULE_WAT)?;
    
    // Execute the WASM module - this should fail
    let result = execute_wasm(
        &wasmtime::wat::parse_bytes(ERROR_MODULE_WAT.as_bytes())?, 
        None, // Use default context
        &host_env,
        Some("test-proposal-error"), // Provide a proposal ID
        Some("test-federation"), // Provide a federation scope
    ).await;
    
    // Check that execution failed
    assert!(result.is_err(), "Execution should fail");
    
    // Verify that an anchor CID was created despite the error
    assert!(host_env.get_last_anchor_cid().is_some(), 
           "An anchor CID should have been created for the error");
    
    // Get the StorageManager and verify the anchored data
    let storage_manager = match host_env.storage_manager() {
        Ok(sm) => sm,
        Err(_) => panic!("Failed to get storage manager"),
    };
    
    if let Some(test_storage) = storage_manager.as_any().downcast_ref::<TestStorageManager>() {
        if let Some((key, data)) = test_storage.get_last_anchored_data().await {
            // Verify it's an ExecutionReceipt
            assert!(key.contains("ExecutionReceipt") || key.contains("execution_receipt"), 
                   "Key should contain ExecutionReceipt: {}", key);
            
            // Parse the JSON data
            let receipt_json: Value = serde_json::from_slice(&data)?;
            
            // Verify the receipt fields
            assert_eq!(receipt_json["type"], "ExecutionReceipt", "Should be an ExecutionReceipt");
            assert_eq!(receipt_json["outcome"], "Error", "Outcome should be Error");
            assert_eq!(receipt_json["execution_id"], "test-proposal-error", "Execution ID should match");
            assert_eq!(receipt_json["scope"], "test-federation", "Scope should match");
            assert!(receipt_json["resource_usage"].is_object(), "Resource usage should be an object");
            assert!(receipt_json["timestamp"].is_number(), "Timestamp should be a number");
            assert!(receipt_json["error"].is_string(), "Error field should be a string");
        } else {
            panic!("No anchored data found");
        }
    } else {
        panic!("Failed to downcast storage manager to TestStorageManager");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_receipt_retrieval_from_wasm() -> Result<()> {
    // Create the test environment
    let host_env = create_test_environment();
    
    // First run a successful execution to generate a receipt
    let result = execute_wasm(
        &wasmtime::wat::parse_bytes(SUCCESS_MODULE_WAT.as_bytes())?, 
        None, // Use default context
        &host_env,
        Some("test-proposal-for-retrieval"), // Provide a proposal ID
        Some("test-federation"), // Provide a federation scope
    ).await?;
    
    // Verify the first execution succeeded
    assert_eq!(result.code, 0, "First execution should succeed");
    
    // Now run the receipt retrieval module
    let receipt_retrieval_result = execute_wasm(
        &wasmtime::wat::parse_bytes(RECEIPT_RETRIEVAL_WAT.as_bytes())?, 
        None, // Use default context
        &host_env,
        Some("test-receipt-retrieval"), // Provide a different proposal ID
        Some("test-federation"), // Same federation scope
    ).await?;
    
    // Verify the retrieval execution succeeded and returned a positive length
    assert!(receipt_retrieval_result.code > 0, "Receipt retrieval should return positive length (got {})", receipt_retrieval_result.code);
    
    // We can't directly access the WASM memory from here to see the result buffer,
    // but we can verify that the execution generated the appropriate receipts.
    
    // Verify that our receipt retrieval execution also generated a receipt
    let storage_manager = match host_env.storage_manager() {
        Ok(sm) => sm,
        Err(_) => panic!("Failed to get storage manager"),
    };
    
    if let Some(test_storage) = storage_manager.as_any().downcast_ref::<TestStorageManager>() {
        // Get all receipts in test-federation scope
        let receipts = test_storage.list_anchors("test-federation:ExecutionReceipt").await?;
        
        // Verify we have at least two receipts (original + retrieval)
        assert!(receipts.len() >= 2, "Should find at least two receipts, found {}", receipts.len());
        
        // Find our original and retrieval receipts
        let mut found_original = false;
        let mut found_retrieval = false;
        
        for (_, data) in receipts {
            let receipt: serde_json::Value = serde_json::from_slice(&data)?;
            
            if let Some(execution_id) = receipt["execution_id"].as_str() {
                if execution_id == "test-proposal-for-retrieval" {
                    found_original = true;
                    // Verify the original receipt properties
                    assert_eq!(receipt["outcome"], "Success", "Original receipt should show success");
                    assert_eq!(receipt["scope"], "test-federation", "Original receipt should have correct scope");
                }
                else if execution_id == "test-receipt-retrieval" {
                    found_retrieval = true;
                    // Verify the retrieval receipt properties
                    assert_eq!(receipt["outcome"], "Success", "Retrieval receipt should show success");
                    assert_eq!(receipt["scope"], "test-federation", "Retrieval receipt should have correct scope");
                    
                    // The return code from the retrieval execution should be the number of bytes written
                    // to the result buffer, which should be greater than 0
                    assert_eq!(receipt["code"], receipt_retrieval_result.code, "Return code should match");
                }
            }
        }
        
        assert!(found_original, "Should find the original execution receipt");
        assert!(found_retrieval, "Should find the receipt retrieval execution receipt");
        
        // Retrieve the simplified receipts the same way our WASM module did
        let simplified_receipts = crate::credentials::get_simplified_execution_receipts(
            &host_env, 
            "test-federation", 
            None
        ).await.map_err(|e| anyhow::anyhow!("Failed to get simplified receipts: {}", e))?;
        
        // Verify we got a valid JSON array back
        let receipts_value: serde_json::Value = serde_json::from_str(&simplified_receipts)?;
        assert!(receipts_value.is_array(), "Simplified receipts should be a JSON array");
        
        // Verify we have at least our two receipts
        let receipts_array = receipts_value.as_array().unwrap();
        assert!(receipts_array.len() >= 2, "Should have at least 2 receipts in the JSON result");
        
        // Check that our specific receipts are in the JSON array
        let mut found_original_json = false;
        let mut found_retrieval_json = false;
        
        for receipt in receipts_array {
            if let Some(execution_id) = receipt["execution_id"].as_str() {
                if execution_id == "test-proposal-for-retrieval" {
                    found_original_json = true;
                } else if execution_id == "test-receipt-retrieval" {
                    found_retrieval_json = true;
                }
            }
        }
        
        assert!(found_original_json, "Original receipt should be in JSON result");
        assert!(found_retrieval_json, "Retrieval receipt should be in JSON result");
    } else {
        panic!("Failed to downcast storage manager to TestStorageManager");
    }
    
    Ok(())
} 