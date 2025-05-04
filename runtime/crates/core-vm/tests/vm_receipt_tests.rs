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