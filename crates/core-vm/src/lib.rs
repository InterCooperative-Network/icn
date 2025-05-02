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
use cid::multihash::MultihashDigest;
use icn_economics::ResourceType;
use icn_identity::IdentityScope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
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

/// Execute a WASM module with the given context and host environment
pub async fn execute_wasm(
    wasm_bytes: &[u8],
    _context: VmContext,
    host_env: Box<dyn HostEnvironment + Send + Sync>,
) -> Result<ExecutionResult, VmError> {
    // Create a new wasmtime engine
    let engine = Engine::default();
    
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
    
    /// Mock implementation of HostEnvironment for testing
    struct MockHostEnvironment;
    
    #[async_trait]
    impl HostEnvironment for MockHostEnvironment {
        async fn storage_get(&self, _key: Cid) -> HostResult<Option<Vec<u8>>> {
            Ok(None)
        }
        
        async fn storage_put(&mut self, _key: Cid, _value: Vec<u8>) -> HostResult<()> {
            Ok(())
        }
        
        async fn blob_put(&mut self, _content: Vec<u8>) -> HostResult<Cid> {
            // Return a dummy CID
            let digest = cid::multihash::Code::Sha2_256.digest(b"test");
            Ok(Cid::new_v0(digest).unwrap())
        }
        
        async fn blob_get(&self, _cid: Cid) -> HostResult<Option<Vec<u8>>> {
            Ok(None)
        }
        
        fn get_caller_did(&self) -> HostResult<String> {
            Ok("did:icn:test".to_string())
        }
        
        fn get_caller_scope(&self) -> HostResult<IdentityScope> {
            Ok(IdentityScope::Individual)
        }
        
        async fn verify_signature(&self, _did_str: &str, _message: &[u8], _signature: &[u8]) -> HostResult<bool> {
            Ok(true)
        }
        
        fn check_resource_authorization(&self, _resource: &ResourceType, _amount: u64) -> HostResult<bool> {
            Ok(true)
        }
        
        fn record_resource_usage(&mut self, _resource: ResourceType, _amount: u64) -> HostResult<()> {
            Ok(())
        }
        
        async fn budget_allocate(&mut self, _budget_id: &str, _amount: u64, _resource: ResourceType) -> HostResult<()> {
            Ok(())
        }
        
        async fn anchor_to_dag(&mut self, _content: Vec<u8>, _parents: Vec<Cid>) -> HostResult<Cid> {
            // Return a dummy CID
            let digest = cid::multihash::Code::Sha2_256.digest(b"test");
            Ok(Cid::new_v0(digest).unwrap())
        }
        
        fn log_message(&self, _level: LogLevel, _message: &str) -> HostResult<()> {
            Ok(())
        }
    }
    
    #[test]
    fn test_create_vm_context() {
        let context = VmContext::new(
            "did:icn:test".to_string(),
            IdentityScope::Individual,
            vec![ResourceType::Compute, ResourceType::Storage],
            "exec-123".to_string(),
            1620000000,
            None,
        );
        
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
} 