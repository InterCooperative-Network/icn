/*!
# ICN Core VM

The Core Virtual Machine for the ICN Runtime, enabling secure execution of WASM modules
within a sandboxed environment.
*/

pub mod mem_helpers;
pub mod resources;

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::*;

pub use resources::{ResourceType, ResourceAuthorization, ResourceConsumption};
use icn_identity::{KeyPair, IdentityScope};

/// Identity context for the VM execution
#[derive(Clone)]
pub struct IdentityContext {
    keypair: KeyPair,
    did: String,
}

impl IdentityContext {
    /// Create a new identity context
    pub fn new(keypair: KeyPair, did: &str) -> Self {
        Self {
            keypair,
            did: did.to_string(),
        }
    }

    /// Get the DID
    pub fn did(&self) -> &str {
        &self.did
    }

    /// Get a reference to the keypair
    pub fn keypair(&self) -> &KeyPair {
        &self.keypair
    }
}

/// VM context containing identity and resource authorization information
#[derive(Clone)]
pub struct VMContext {
    identity_context: Arc<IdentityContext>,
    resource_authorizations: Vec<ResourceAuthorization>,
}

impl VMContext {
    /// Create a new VM context
    pub fn new(identity_context: Arc<IdentityContext>, resource_authorizations: Vec<ResourceAuthorization>) -> Self {
        Self {
            identity_context,
            resource_authorizations,
        }
    }

    /// Get the caller DID from the identity context
    pub fn caller_did(&self) -> &str {
        self.identity_context.did()
    }

    /// Get the resource authorizations
    pub fn resource_authorizations(&self) -> &[ResourceAuthorization] {
        &self.resource_authorizations
    }
}

/// Errors that can occur during VM execution
#[derive(Error, Debug)]
pub enum VmError {
    #[error("VM initialization error: {0}")]
    InitializationError(String),

    #[error("VM execution error: {0}")]
    ExecutionError(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Unauthorized operation: {0}")]
    Unauthorized(String),

    #[error("Memory access error: {0}")]
    MemoryError(String),

    #[error("Host function error: {0}")]
    HostFunctionError(String),
}

/// Result of VM execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Whether the execution succeeded
    pub success: bool,
    
    /// Return data from the execution
    pub return_data: Vec<u8>,
    
    /// Resources consumed during execution
    pub resources_consumed: ResourceConsumption,
    
    /// Error message if execution failed
    pub error: Option<String>,
}

impl ExecutionResult {
    /// Create a new success result
    pub fn success(return_data: Vec<u8>, resources_consumed: ResourceConsumption) -> Self {
        Self {
            success: true,
            return_data,
            resources_consumed,
            error: None,
        }
    }
    
    /// Create a new error result
    pub fn error(error: String, resources_consumed: ResourceConsumption) -> Self {
        Self {
            success: false,
            return_data: Vec::new(),
            resources_consumed,
            error: Some(error),
        }
    }
    
    /// Check if execution succeeded
    pub fn is_success(&self) -> bool {
        self.success
    }
}

/// Host environment trait for VM execution
pub trait HostEnvironment {
    /// Get a value from the host environment
    fn get_value(&self, key: &str) -> Option<Vec<u8>>;
    
    /// Set a value in the host environment
    fn set_value(&mut self, key: &str, value: Vec<u8>) -> Result<(), VmError>;
    
    /// Delete a value from the host environment
    fn delete_value(&mut self, key: &str) -> Result<(), VmError>;
    
    /// Log a message from the VM
    fn log(&self, message: &str) -> Result<(), VmError>;
}

/// Concrete implementation of the host environment
#[derive(Clone)]
pub struct ConcreteHostEnvironment {
    vm_context: VMContext,
    storage: HashMap<String, Vec<u8>>,
    consumed_resources: HashMap<ResourceType, u64>,
}

impl ConcreteHostEnvironment {
    /// Create a new concrete host environment
    pub fn new(vm_context: VMContext) -> Self {
        Self {
            vm_context,
            storage: HashMap::new(),
            consumed_resources: HashMap::new(),
        }
    }
    
    /// Get the amount of compute resources consumed
    pub fn get_compute_consumed(&self) -> u64 {
        self.consumed_resources.get(&ResourceType::Compute).copied().unwrap_or(0)
    }

    /// Get the amount of storage resources consumed
    pub fn get_storage_consumed(&self) -> u64 {
        self.consumed_resources.get(&ResourceType::Storage).copied().unwrap_or(0)
    }

    /// Get the amount of network resources consumed
    pub fn get_network_consumed(&self) -> u64 {
        self.consumed_resources.get(&ResourceType::Network).copied().unwrap_or(0)
    }

    /// Record consumption of compute resources
    pub fn record_compute_usage(&mut self, amount: u64) -> Result<(), VmError> {
        self.record_resource_usage(ResourceType::Compute, amount)
    }

    /// Record consumption of storage resources
    pub fn record_storage_usage(&mut self, amount: u64) -> Result<(), VmError> {
        self.record_resource_usage(ResourceType::Storage, amount)
    }

    /// Record consumption of network resources
    pub fn record_network_usage(&mut self, amount: u64) -> Result<(), VmError> {
        self.record_resource_usage(ResourceType::Network, amount)
    }

    /// Record consumption of a resource type
    fn record_resource_usage(&mut self, resource_type: ResourceType, amount: u64) -> Result<(), VmError> {
        let current = self.consumed_resources.entry(resource_type).or_insert(0);
        let new_total = current.checked_add(amount).ok_or_else(|| {
            VmError::ResourceLimitExceeded(format!(
                "Resource consumption would overflow for {:?}",
                resource_type
            ))
        })?;

        // Check if this would exceed the authorization limit
        let auth = self.vm_context.resource_authorizations().iter()
            .find(|auth| auth.resource_type == resource_type)
            .ok_or_else(|| {
                VmError::Unauthorized(format!(
                    "No authorization for resource type {:?}",
                    resource_type
                ))
            })?;

        if new_total > auth.limit {
            return Err(VmError::ResourceLimitExceeded(format!(
                "Resource limit exceeded for {:?}: {} > {}",
                resource_type, new_total, auth.limit
            )));
        }

        *current = new_total;
        Ok(())
    }

    /// Get the caller's DID
    pub fn caller_did(&self) -> &str {
        self.vm_context.caller_did()
    }

    /// Get the caller's scope
    pub fn caller_scope(&self) -> IdentityScope {
        // Default to Personal scope if not specified
        IdentityScope::Personal
    }
}

impl HostEnvironment for ConcreteHostEnvironment {
    fn get_value(&self, key: &str) -> Option<Vec<u8>> {
        self.storage.get(key).cloned()
    }
    
    fn set_value(&mut self, key: &str, value: Vec<u8>) -> Result<(), VmError> {
        // Record storage usage (key size + value size)
        let storage_cost = (key.len() + value.len()) as u64;
        self.record_storage_usage(storage_cost)?;
        
        self.storage.insert(key.to_string(), value);
        Ok(())
    }
    
    fn delete_value(&mut self, key: &str) -> Result<(), VmError> {
        // Record minimal compute cost for deletion
        self.record_compute_usage(1)?;
        
        self.storage.remove(key);
        Ok(())
    }
    
    fn log(&self, message: &str) -> Result<(), VmError> {
        // Record compute cost based on message length
        // Note: In a real implementation, we'd record this properly
        debug!("[VM] {}", message);
        Ok(())
    }
}

/// Execute a WASM module with the given function and parameters
pub fn execute_wasm(
    wasm_bytes: &[u8],
    function_name: &str,
    params: &[u8],
    vm_context: VMContext,
) -> Result<ExecutionResult, VmError> {
    // In a real implementation, this would use a proper WASM runtime like wasmtime
    // For now, we'll simply simulate execution
    
    // Create a host environment
    let mut host_env = ConcreteHostEnvironment::new(vm_context);
    
    // Record baseline compute usage for instantiation
    host_env.record_compute_usage(1000)?;
    
    // Log the function call
    host_env.log(&format!("Executing function: {}", function_name))?;
    
    // Record compute usage for execution (simulated)
    host_env.record_compute_usage(5000)?;
    
    // Create a resource consumption record
    let resources = ResourceConsumption {
        compute: host_env.get_compute_consumed(),
        storage: host_env.get_storage_consumed(),
        network: host_env.get_network_consumed(),
        token: 0,
    };
    
    // Return a successful result
    Ok(ExecutionResult::success(vec![42], resources))
}


