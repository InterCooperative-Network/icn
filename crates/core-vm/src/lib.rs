/*!
# ICN Core VM

This crate implements the WASM execution engine for the ICN Runtime (CoVM V3).
It provides a sandboxed environment for executing compiled CCL programs (.dsl files)
and defines the Host ABI trait that exposes system functionality to WASM modules.

## Architectural Tenets
- WASM as the compilation target for CCL templates
- Host ABI for exposing runtime capabilities to WASM
- Metering for resource usage tracking and limiting
- Secure sandboxing of user-provided code
*/

use anyhow::{Error as AnyhowError, Result};
use async_trait::async_trait;
use cid::Cid;
use cid::multihash::{MultihashDigest, Code};
use icn_economics::ResourceType;
use icn_identity::{IdentityId, IdentityScope, KeyPair, Signature, verify_signature as identity_verify_signature};
use icn_storage::StorageBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use futures::lock::Mutex;
use thiserror::Error;
use tracing::{debug, info, warn, error};
use wasmtime::{Engine, Linker, Module, Store};

/// Log level for VM execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Errors that can occur during host environment operations
#[derive(Debug, Error)]
pub enum HostError {
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Identity error: {0}")]
    IdentityError(String),
    
    #[error("Economics error: {0}")]
    EconomicsError(String),
    
    #[error("DAG error: {0}")]
    DagError(String),
    
    #[error("Unauthorized access: {0}")]
    UnauthorizedAccess(String),
    
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
    
    #[error("WASM error: {0}")]
    WasmError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    
    #[error("System error: {0}")]
    SystemError(String),
}

/// Result type for host operations
pub type HostResult<T> = Result<T, HostError>;

/// Host environment trait that defines the interface between WASM modules and the ICN Runtime
#[async_trait]
pub trait HostEnvironment: Send + Sync {
    // Storage operations
    async fn storage_get(&self, key: Cid) -> HostResult<Option<Vec<u8>>>;
    async fn storage_put(&mut self, key: Cid, value: Vec<u8>) -> HostResult<()>;
    
    // Blob storage operations
    async fn blob_put(&mut self, content: Vec<u8>) -> HostResult<Cid>;
    async fn blob_get(&self, cid: Cid) -> HostResult<Option<Vec<u8>>>;
    
    // Identity operations
    fn get_caller_did(&self) -> HostResult<String>;
    fn get_caller_scope(&self) -> HostResult<IdentityScope>;
    async fn verify_signature(&self, did_str: &str, message: &[u8], signature: &[u8]) -> HostResult<bool>;
    
    // Economics operations
    fn check_resource_authorization(&self, resource: &ResourceType, amount: u64) -> HostResult<bool>;
    fn record_resource_usage(&mut self, resource: ResourceType, amount: u64) -> HostResult<()>;
    
    // Budgeting operations
    async fn budget_allocate(&mut self, budget_id: &str, amount: u64, resource: ResourceType) -> HostResult<()>;
    
    // DAG operations
    async fn anchor_to_dag(&mut self, content: Vec<u8>, parents: Vec<Cid>) -> HostResult<Cid>;
    
    // Logging operations
    fn log_message(&self, level: LogLevel, message: &str) -> HostResult<()>;
}

/// VM context for execution environment
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VmContext {
    /// The DID of the caller
    pub caller_did: String,
    
    /// The scope of the caller
    pub caller_scope: IdentityScope,
    
    /// Resource authorizations for this execution
    pub resource_authorizations: Vec<ResourceType>,
    
    /// Unique execution ID
    pub execution_id: String,
    
    /// Timestamp of this execution
    pub timestamp: i64,
    
    /// Optional CID of the proposal triggering this execution
    pub proposal_cid: Option<Cid>,
}

impl VmContext {
    /// Create a new VM context
    pub fn new(
        caller_did: String,
        caller_scope: IdentityScope,
        resource_authorizations: Vec<ResourceType>,
        execution_id: String,
        timestamp: i64,
        proposal_cid: Option<Cid>,
    ) -> Self {
        Self {
            caller_did,
            caller_scope,
            resource_authorizations,
            execution_id,
            timestamp,
            proposal_cid,
        }
    }
}

/// Errors that can occur during VM operations
#[derive(Debug, Error)]
pub enum VmError {
    #[error("Failed to load WASM module: {0}")]
    ModuleLoadError(String),
    
    #[error("Failed to instantiate WASM module: {0}")]
    InstantiationError(String),
    
    #[error("Host function call failed: {0}")]
    HostFunctionError(String),
    
    #[error("Memory access error: {0}")]
    MemoryAccessError(String),
    
    #[error("Execution exceeded resource limits: {0}")]
    ResourceLimitExceeded(String),
    
    #[error("VM internal error: {0}")]
    InternalError(String),
}

// Implement From<AnyhowError> for VmError to convert anyhow errors to VmError
impl From<AnyhowError> for VmError {
    fn from(error: AnyhowError) -> Self {
        VmError::InternalError(error.to_string())
    }
}

/// Execution result from running a WASM module
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Whether execution was successful
    pub success: bool,
    
    /// Optional output data
    pub output_data: Option<Vec<u8>>,
    
    /// Execution logs
    pub logs: Vec<String>,
    
    /// Resources consumed during execution
    pub resources_consumed: HashMap<ResourceType, u64>,
}

impl ExecutionResult {
    /// Create a stub execution result for testing
    pub fn stub() -> Self {
        Self {
            success: true,
            output_data: None,
            logs: vec!["Stub execution result".to_string()],
            resources_consumed: HashMap::new(),
        }
    }
}

/// Simple identity context for the host environment
pub struct IdentityContext {
    /// Key pair for signing operations
    pub keypair: KeyPair,
    
    /// DID of the identity
    pub did: IdentityId,
}

impl IdentityContext {
    /// Create a new identity context
    pub fn new(keypair: KeyPair, did: impl Into<String>) -> Self {
        Self {
            keypair,
            did: IdentityId::new(did),
        }
    }
    
    /// Get a clone of the DID
    pub fn clone_did(&self) -> IdentityId {
        IdentityId::new(self.did.as_str())
    }
}

/// Concrete implementation of the Host Environment
pub mod host_impl {
    use super::*;
    
    /// Concrete implementation of the Host Environment
    pub struct ConcreteHostEnvironment {
        /// Storage backend
        storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>,
        
        /// Identity context for signing host actions
        identity_context: Arc<IdentityContext>,
        
        /// VM context for the current execution
        pub vm_context: VmContext,
        
        /// Resource usage tracking
        pub resource_usage: Vec<(ResourceType, u64)>,
    }
    
    impl ConcreteHostEnvironment {
        /// Create a new concrete host environment
        pub fn new(
            storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>,
            identity_context: Arc<IdentityContext>,
            vm_context: VmContext,
        ) -> Self {
            Self {
                storage,
                identity_context,
                vm_context,
                resource_usage: Vec::new(),
            }
        }
        
        /// Helper to convert storage errors to host errors
        fn storage_error_to_host_error(e: icn_storage::StorageError) -> HostError {
            HostError::StorageError(e.to_string())
        }
        
        /// Helper to convert identity errors to host errors
        fn identity_error_to_host_error(e: icn_identity::IdentityError) -> HostError {
            HostError::IdentityError(e.to_string())
        }
        
        /// Track resource usage
        fn track_resource(&mut self, resource: ResourceType, amount: u64) {
            // Find existing resource entry or add a new one
            for entry in &mut self.resource_usage {
                if entry.0 == resource {
                    entry.1 += amount;
                    return;
                }
            }
            // If not found, add new entry
            self.resource_usage.push((resource, amount));
        }
        
        /// Get resource usage amount
        pub fn get_resource_usage(&self, resource: ResourceType) -> u64 {
            for entry in &self.resource_usage {
                if entry.0 == resource {
                    return entry.1;
                }
            }
            0
        }
    }
    
    #[async_trait]
    impl HostEnvironment for ConcreteHostEnvironment {
        // Storage operations
        async fn storage_get(&self, key: Cid) -> HostResult<Option<Vec<u8>>> {
            // Acquire a lock on storage
            let storage = self.storage.lock().await;
            
            // Call the storage backend
            storage.get(&key)
                .await
                .map_err(Self::storage_error_to_host_error)
        }
        
        async fn storage_put(&mut self, _key: Cid, value: Vec<u8>) -> HostResult<()> {
            // Get the value size for resource tracking
            let value_size = value.len() as u64;
            
            // Acquire a lock on storage
            let storage = self.storage.lock().await;
            
            // Call the storage backend to generate the CID and store value
            // (Note: StorageBackend::put returns a Cid, but we discard it as the ABI expects Cid in input)
            storage.put(&value)
                .await
                .map_err(Self::storage_error_to_host_error)?;
                
            // Track the resource usage (after operation succeeds)
            // Clone self.resource_usage to avoid borrowing issues
            let mut usage = self.resource_usage.clone();
            
            // Find existing entry or add new one
            let mut found = false;
            for entry in &mut usage {
                if entry.0 == ResourceType::Storage {
                    entry.1 += value_size;
                    found = true;
                    break;
                }
            }
            
            if !found {
                usage.push((ResourceType::Storage, value_size));
            }
            
            // Update self.resource_usage
            self.resource_usage = usage;
            
            Ok(())
        }
        
        // Blob storage operations
        async fn blob_put(&mut self, content: Vec<u8>) -> HostResult<Cid> {
            // Get the content size for resource tracking
            let content_size = content.len() as u64;
            
            // Acquire a lock on storage
            let storage = self.storage.lock().await;
            
            // Call the storage backend to calculate CID and store content
            let cid = storage.put(&content)
                .await
                .map_err(Self::storage_error_to_host_error)?;
                
            // Track the resource usage (after operation succeeds)
            // Clone self.resource_usage to avoid borrowing issues
            let mut usage = self.resource_usage.clone();
            
            // Find existing entry or add new one
            let mut found = false;
            for entry in &mut usage {
                if entry.0 == ResourceType::Storage {
                    entry.1 += content_size;
                    found = true;
                    break;
                }
            }
            
            if !found {
                usage.push((ResourceType::Storage, content_size));
            }
            
            // Update self.resource_usage
            self.resource_usage = usage;
            
            Ok(cid)
        }
        
        async fn blob_get(&self, cid: Cid) -> HostResult<Option<Vec<u8>>> {
            // This is the same as storage_get for now
            self.storage_get(cid).await
        }
        
        // Identity operations
        fn get_caller_did(&self) -> HostResult<String> {
            // Return directly from VM context
            Ok(self.vm_context.caller_did.clone())
        }
        
        fn get_caller_scope(&self) -> HostResult<IdentityScope> {
            // Return directly from VM context
            Ok(self.vm_context.caller_scope)
        }
        
        async fn verify_signature(&self, did_str: &str, message: &[u8], signature: &[u8]) -> HostResult<bool> {
            // Create identity ID from string
            let identity_id = IdentityId::new(did_str);
            
            // Create signature from bytes
            let sig = Signature::new(signature.to_vec());
            
            // Use the identity verification function
            identity_verify_signature(message, &sig, &identity_id)
                .map_err(Self::identity_error_to_host_error)
        }
        
        // Economics operations
        fn check_resource_authorization(&self, resource: &ResourceType, amount: u64) -> HostResult<bool> {
            // TODO(V3-MVP): Implement actual economic logic using icn-economics state.
            // For now, check if the resource type is in the authorized list
            let authorized = self.vm_context.resource_authorizations.contains(resource);
            
            // Log the authorization check
            debug!(
                "Resource authorization check: resource={:?}, amount={}, authorized={}",
                resource, amount, authorized
            );
            
            Ok(authorized)
        }
        
        fn record_resource_usage(&mut self, resource: ResourceType, amount: u64) -> HostResult<()> {
            // TODO(V3-MVP): Implement actual economic logic using icn-economics state.
            
            // Clone self.resource_usage to avoid borrowing issues
            let mut usage = self.resource_usage.clone();
            
            // Find existing entry or add new one
            let mut found = false;
            for entry in &mut usage {
                if entry.0 == resource {
                    entry.1 += amount;
                    found = true;
                    break;
                }
            }
            
            if !found {
                usage.push((resource, amount));
            }
            
            // Update self.resource_usage
            self.resource_usage = usage;
            
            // Log the resource usage
            debug!(
                "Resource usage recorded: resource={:?}, amount={}",
                resource, amount
            );
            
            Ok(())
        }
        
        // Budgeting operations
        async fn budget_allocate(&mut self, budget_id: &str, amount: u64, resource: ResourceType) -> HostResult<()> {
            // TODO(V3-MVP): Implement actual economic logic using icn-economics state.
            // For now, just log and return success
            debug!(
                "Budget allocation: budget_id={}, amount={}, resource={:?}",
                budget_id, amount, resource
            );
            
            Ok(())
        }
        
        // DAG operations
        async fn anchor_to_dag(&mut self, content: Vec<u8>, parents: Vec<Cid>) -> HostResult<Cid> {
            // TODO(V3-MVP): Implement proper DAG node creation and linking.
            
            // Calculate CID using multihash
            let hash = Code::Sha2_256.digest(&content);
            let cid = Cid::new_v0(hash).map_err(|e| HostError::DagError(e.to_string()))?;
            
            // Get the content size for resource tracking
            let content_size = content.len() as u64;
            
            // Acquire a lock on storage
            let storage = self.storage.lock().await;
            
            // Call the storage backend
            let result = storage.put(&content)
                .await
                .map_err(Self::storage_error_to_host_error);
                
            // If storage operation was successful, track the resource usage
            if result.is_ok() {
                // Clone self.resource_usage to avoid borrowing issues
                let mut usage = self.resource_usage.clone();
                
                // Find existing entry or add new one
                let mut found = false;
                for entry in &mut usage {
                    if entry.0 == ResourceType::Storage {
                        entry.1 += content_size;
                        found = true;
                        break;
                    }
                }
                
                if !found {
                    usage.push((ResourceType::Storage, content_size));
                }
                
                // Update self.resource_usage
                self.resource_usage = usage;
            }
            
            // Check if storage operation succeeded
            result?;
            
            // Log the DAG operation
            debug!(
                "DAG anchor operation: content_len={}, parents_count={}, cid={}",
                content.len(), parents.len(), cid.to_string()
            );
            
            Ok(cid)
        }
        
        // Logging operations
        fn log_message(&self, level: LogLevel, message: &str) -> HostResult<()> {
            // Format the log message with execution context
            let formatted_message = format!(
                "[Exec:{}] {}",
                self.vm_context.execution_id, message
            );
            
            // Log using the appropriate level
            match level {
                LogLevel::Debug => debug!("{}", formatted_message),
                LogLevel::Info => info!("{}", formatted_message),
                LogLevel::Warn => warn!("{}", formatted_message),
                LogLevel::Error => error!("{}", formatted_message),
            }
            
            Ok(())
        }
    }
}

// Re-export the concrete host environment
pub use host_impl::ConcreteHostEnvironment;

/// Execute a WASM module with the given context and host environment
pub async fn execute_wasm(
    wasm_bytes: &[u8],
    _context: VmContext,
    host_env: Box<dyn HostEnvironment + Send + Sync>,
) -> Result<ExecutionResult, VmError> {
    // Create a new wasmtime engine with async support explicitly enabled
    let mut config = wasmtime::Config::new();
    config.async_support(true);
    let engine = Engine::new(&config)
        .map_err(|e| VmError::InternalError(format!("Failed to create wasmtime engine: {}", e)))?;
    
    // Create a store with the host environment as data
    let mut store = Store::new(&engine, host_env);
    
    // Create a new linker
    let linker = Linker::new(&engine);
    
    // Define basic functions for host environment (minimal dummy implementations)
    // In a full implementation, these would call corresponding methods on HostEnvironment

    // Try to compile the module
    let module = Module::new(&engine, wasm_bytes)
        .map_err(|e| VmError::ModuleLoadError(e.to_string()))?;
    
    // Try to instantiate the module with imports
    let _instance = linker
        .instantiate_async(&mut store, &module)
        .await
        .map_err(|e| VmError::InstantiationError(e.to_string()))?;
    
    // We don't actually call any WASM functions yet since this is just a stub
    
    // Return a stub execution result
    Ok(ExecutionResult::stub())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::multihash::MultihashDigest;
    use icn_storage::AsyncInMemoryStorage;
    
    /// Helper function to create a test identity context
    fn create_test_identity_context() -> Arc<IdentityContext> {
        // Generate a simple keypair for testing
        let private_key = vec![1, 2, 3, 4]; // Dummy private key
        let public_key = vec![5, 6, 7, 8]; // Dummy public key
        let keypair = KeyPair::new(private_key, public_key);
        
        // Create the identity context
        let identity_context = IdentityContext::new(keypair, "did:icn:test");
        
        Arc::new(identity_context)
    }
    
    /// Helper function to create a test VM context
    fn create_test_vm_context() -> VmContext {
        VmContext::new(
            "did:icn:test".to_string(),
            IdentityScope::Individual,
            vec![ResourceType::Compute, ResourceType::Storage],
            "exec-123".to_string(),
            1620000000,
            None,
        )
    }
    
    /// Helper function to create a concrete host environment for testing
    async fn create_test_host_environment() -> ConcreteHostEnvironment {
        // Create an in-memory storage backend
        let storage = AsyncInMemoryStorage::new();
        let storage_arc = Arc::new(Mutex::new(storage));
        
        // Create the identity context
        let identity_context = create_test_identity_context();
        
        // Create the VM context
        let vm_context = create_test_vm_context();
        
        // Create the host environment
        ConcreteHostEnvironment::new(
            storage_arc,
            identity_context,
            vm_context,
        )
    }
    
    #[test]
    fn test_create_vm_context() {
        let context = create_test_vm_context();
        
        assert_eq!(context.caller_did, "did:icn:test");
        assert_eq!(context.caller_scope, IdentityScope::Individual);
        assert_eq!(context.execution_id, "exec-123");
    }
    
    #[test]
    fn test_execution_result_stub() {
        // Just test the stub creation
        let result = ExecutionResult::stub();
        assert!(result.success);
        assert_eq!(result.logs, vec!["Stub execution result".to_string()]);
        assert!(result.output_data.is_none());
        assert!(result.resources_consumed.is_empty());
    }
    
    #[tokio::test]
    async fn test_concrete_host_environment_creation() {
        let host_env = create_test_host_environment().await;
        
        // Check that basic field values match expectations
        assert_eq!(host_env.vm_context.caller_did, "did:icn:test");
        assert_eq!(host_env.vm_context.caller_scope, IdentityScope::Individual);
        assert_eq!(host_env.vm_context.execution_id, "exec-123");
        assert!(host_env.resource_usage.is_empty());
    }
    
    #[tokio::test]
    async fn test_storage_operations() {
        let mut host_env = create_test_host_environment().await;
        
        // Test data
        let test_content = b"test content".to_vec();
        
        // Calculate expected CID
        let hash = Code::Sha2_256.digest(&test_content);
        let expected_cid = Cid::new_v0(hash).unwrap();
        
        // Test blob_put
        let cid = host_env.blob_put(test_content.clone()).await.unwrap();
        assert_eq!(cid, expected_cid);
        
        // Test blob_get
        let retrieved = host_env.blob_get(cid).await.unwrap();
        assert_eq!(retrieved, Some(test_content.clone()));
        
        // Check that storage usage was tracked
        assert_eq!(host_env.get_resource_usage(ResourceType::Storage), test_content.len() as u64);
    }
    
    #[tokio::test]
    async fn test_identity_operations() {
        let host_env = create_test_host_environment().await;
        
        // Test get_caller_did
        let did = host_env.get_caller_did().unwrap();
        assert_eq!(did, "did:icn:test");
        
        // Test get_caller_scope
        let scope = host_env.get_caller_scope().unwrap();
        assert_eq!(scope, IdentityScope::Individual);
    }
    
    #[tokio::test]
    async fn test_resource_authorization() {
        let mut host_env = create_test_host_environment().await;
        
        // Test check_resource_authorization for an authorized resource
        let authorized = host_env.check_resource_authorization(&ResourceType::Compute, 100).unwrap();
        assert!(authorized);
        
        // Test check_resource_authorization for an unauthorized resource
        let authorized = host_env.check_resource_authorization(&ResourceType::Network, 100).unwrap();
        assert!(!authorized);
        
        // Test record_resource_usage
        host_env.record_resource_usage(ResourceType::Compute, 100).unwrap();
        assert_eq!(host_env.get_resource_usage(ResourceType::Compute), 100);
    }
    
    #[tokio::test]
    async fn test_execute_wasm_with_concrete_environment() {
        // Create the host environment
        let host_env = create_test_host_environment().await;
        
        // Create the VM context
        let vm_context = create_test_vm_context();
        
        // Create dummy WASM module bytes (just the magic number + version)
        let wasm_bytes = b"\0asm\x01\0\0\0";
        
        // Execute the WASM module with our concrete host environment
        let result = execute_wasm(
            wasm_bytes,
            vm_context,
            Box::new(host_env)
        ).await;
        
        // Check that we got a stub result
        assert!(result.is_ok());
        let exec_result = result.unwrap();
        assert!(exec_result.success);
        assert_eq!(exec_result.logs, vec!["Stub execution result".to_string()]);
    }
} 