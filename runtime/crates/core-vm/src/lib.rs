/*!
# ICN Core VM

The Core Virtual Machine for the ICN Runtime, enabling secure execution of WASM modules
within a sandboxed environment.
*/

pub mod mem_helpers;
pub mod resources;
pub mod host_abi;

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::*;
use icn_identity::{KeyPair, IdentityScope, IdentityManager, IdentityError, IdentityId as IcnIdentityId, JWK};
use icn_storage::{StorageManager, StorageError};
use icn_dag::{DagNodeBuilder, DagNode, codec::DagCborCodec};
use libipld::{Ipld, ipld, codec::Codec};
use anyhow::{anyhow, Result};

pub use resources::{ResourceType, ResourceAuthorization, ResourceConsumption};

/// Identity context for the VM execution
#[derive(Clone)]
pub struct IdentityContext {
    keypair: Arc<KeyPair>, // Wrap KeyPair in Arc to make it clonable
    did: String,
}

impl IdentityContext {
    /// Create a new identity context
    pub fn new(keypair: KeyPair, did: &str) -> Self {
        Self {
            keypair: Arc::new(keypair),
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
    /// Whether the execution succeeded (i.e., no trap)
    pub success: bool,

    /// Return data from the WASM function's explicit return (if any).
    /// Often just a status code (e.g., 0 for success).
    pub return_data: Vec<u8>,

    /// Resources consumed during execution.
    pub resources_consumed: ResourceConsumption,

    /// Error message if execution failed (trap occurred).
    pub error: Option<String>,

    // --- Added fields for entity creation --- 
    /// DID of a newly created entity, if applicable to this execution.
    pub created_entity_did: Option<String>,
    /// Genesis CID of a newly created entity, if applicable.
    pub created_entity_genesis_cid: Option<Cid>,
}

impl ExecutionResult {
    /// Create a new success result (without entity creation info initially)
    pub fn success(return_data: Vec<u8>, resources_consumed: ResourceConsumption) -> Self {
        Self {
            success: true,
            return_data,
            resources_consumed,
            error: None,
            created_entity_did: None, // Default to None
            created_entity_genesis_cid: None, // Default to None
        }
    }

    /// Create a new error result
    pub fn error(error: String, resources_consumed: ResourceConsumption) -> Self {
        Self {
            success: false,
            return_data: Vec::new(),
            resources_consumed,
            error: Some(error),
            created_entity_did: None, // Default to None
            created_entity_genesis_cid: None, // Default to None
        }
    }

    /// Check if execution succeeded
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Set entity creation details (used by execute_wasm)
    pub fn with_entity_creation(mut self, did: String, genesis_cid: Cid) -> Self {
        self.created_entity_did = Some(did);
        self.created_entity_genesis_cid = Some(genesis_cid);
        self
    }
}

/// Errors originating within the host environment logic, distinct from VmError
/// which is returned to the WASM module.
#[derive(Error, Debug)]
pub enum InternalHostError {
    #[error("Identity operation failed: {0}")]
    IdentityError(#[from] IdentityError),
    #[error("Storage operation failed: {0}")]
    StorageError(#[from] StorageError), // Assuming StorageError is defined
    #[error("DAG operation failed: {0}")]
    DagError(#[from] icn_dag::DagError), // Assuming DagError is defined
    #[error("Serialization/Deserialization error: {0}")]
    CodecError(#[from] libipld::error::Error),
    #[error("Invalid input from WASM: {0}")]
    InvalidInput(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("Generic internal error: {0}")]
    Other(#[from] anyhow::Error),
}

/// Host environment trait for VM execution (keep existing, maybe add new methods later)
// pub trait HostEnvironment { ... }

/// Concrete implementation of the host environment
#[derive(Clone)]
pub struct ConcreteHostEnvironment {
    vm_context: VMContext,
    storage_manager: Arc<dyn StorageManager>,
    identity_manager: Arc<dyn IdentityManager>,
    parent_federation_did: Option<String>,
    consumed_resources: HashMap<ResourceType, u64>,
    // --- Added temporary state for entity creation --- 
    last_created_entity_info: Option<(String, Cid)>, // Store (DID, Genesis CID)
}

impl ConcreteHostEnvironment {
    /// Create a new concrete host environment
    pub fn new(
        vm_context: VMContext,
        storage_manager: Arc<dyn StorageManager>,
        identity_manager: Arc<dyn IdentityManager>,
        parent_federation_did: Option<String>,
    ) -> Self {
        Self {
            vm_context,
            storage_manager,
            identity_manager,
            parent_federation_did,
            consumed_resources: HashMap::new(),
            last_created_entity_info: None, // Initialize as None
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
        IdentityScope::Individual
    }

    /// Creates a new sub-entity DAG.
    /// Returns the new DID string on success.
    /// Stores the (DID, Genesis CID) internally for later retrieval by execute_wasm.
    pub async fn create_sub_entity_dag(
        &mut self,
        parent_did: &str,
        genesis_payload_bytes: Vec<u8>,
        entity_type: &str,
    ) -> Result<String, InternalHostError> { // Returns only DID string now
        // --- Basic Compute Cost ---
        // Record some base cost for this complex operation
        self.record_compute_usage(5000)?; // Adjust cost as needed

        // 1. Generate new DID and Keypair
        let (new_did_key_str, _public_jwk) = self
            .identity_manager
            .generate_and_store_did_key()
            .await
            .map_err(|e| InternalHostError::Other(e.into()))?;
        tracing::info!(new_did = %new_did_key_str, "Generated new DID for sub-entity");

        // Record cost associated with key generation
        self.record_compute_usage(1000)?; // Cost for crypto op

        // 2. Deserialize/Prepare Genesis Payload
        // Assume genesis_payload_bytes is CBOR or JSON representing the initial state/metadata
        // For now, let's treat it as CBOR-encoded IPLD data for the node's payload field.
        let genesis_ipld: Ipld = DagCborCodec.decode(&genesis_payload_bytes)?;
        // Record cost for decoding
        self.record_compute_usage((genesis_payload_bytes.len() / 100) as u64)?;


        // 3. Construct Genesis DagNode
        //    The issuer ('iss') of the genesis node is the *new* entity's DID.
        //    Add public key to payload? Or handle via DID doc resolution? Let's assume resolution for now.
        //    Parents list is empty for a genesis node.
        let node_builder = DagNodeBuilder::new()
            .issuer(IcnIdentityId::new(new_did_key_str.clone())) // Use icn_identity::IdentityId
            .payload(genesis_ipld) // Store the provided payload
            .parents(vec![]); // Genesis node has no parents

        // 4. Store Genesis Node using StorageManager
        //    This calculates the CID and stores the node in the new entity's CF.
        let store_result = self
            .storage_manager
            .store_new_dag_root(&new_did_key_str, node_builder)
            .await;

        let (genesis_cid, _genesis_node) = match store_result {
             Ok((cid, node)) => {
                 // Record storage cost - size of the encoded node
                 // We need the encoded size. Let's re-encode for costing (less efficient).
                 // A better way would be if store_new_dag_root returned the size.
                 let encoded_bytes = DagCborCodec.encode(&node)?;
                 self.record_storage_usage(encoded_bytes.len() as u64)?;
                 Ok((cid, node))
             }
             Err(e) => {
                 // Attempt to clean up stored key if node storage failed
                 tracing::error!(new_did = %new_did_key_str, "Failed to store genesis node, attempting to delete key");
                 let _ = self.identity_manager.get_key(&new_did_key_str).await; // Example: How to delete key? Needs method in IdentityManager/KeyStorage
                 Err(InternalHostError::Other(e.into())) // Convert StorageManager's Result
             }
         }?;

        tracing::info!(new_did = %new_did_key_str, %genesis_cid, "Stored genesis node for sub-entity");


        // 5. Register Entity Metadata
        //    Link the new DID to the parent, genesis CID, and type.
        let metadata_result = self
            .identity_manager
            .register_entity_metadata(
                &new_did_key_str,
                Some(parent_did),
                &genesis_cid,
                entity_type, // Pass the type ("Cooperative", "Community")
                None,       // No extra metadata for now
            )
            .await;

         if let Err(e) = metadata_result {
             // Critical failure: Node is stored, but metadata registration failed.
             // This leaves the system in an inconsistent state.
             // Options:
             // 1. Log error and return failure. Requires manual intervention/cleanup.
             // 2. Attempt rollback (delete node? delete key?). Complex and potentially failing.
             tracing::error!(
                 new_did = %new_did_key_str,
                 %genesis_cid,
                 parent_did = %parent_did,
                 "CRITICAL: Failed to register entity metadata after storing genesis node: {}", e
             );
             // For now, log and return error.
              return Err(InternalHostError::Other(e.into())); // Convert IdentityManager's Result
         }

        // Record cost for metadata storage (assume small fixed cost)
        self.record_storage_usage(100)?;

        // --- Parent State Update (Anchoring) ---
        // TODO: Implement anchoring the new entity's creation on the parent's DAG.
        // This likely involves:
        // 1. Constructing an anchor node (e.g., { "event": "entity_created", "did": new_did, "genesis_cid": cid })
        // 2. Calling self.storage_manager.store_node() for the *parent_did*.
        // This should happen *outside* this function, perhaps in the ExecutionManager after this call succeeds.


        // 6. Store DID and Genesis CID internally
        self.last_created_entity_info = Some((new_did_key_str.clone(), genesis_cid));

        // 7. Return the new DID string
        Ok(new_did_key_str)
    }

    /// Stores a regular DAG node within the specified entity's DAG.
    pub async fn store_node(
        &mut self,
        entity_did: &str,
        node_payload_bytes: Vec<u8>, // Expecting CBOR-encoded Ipld payload
        parent_cids_bytes: Vec<Vec<u8>>, // Expecting Vec of CBOR-encoded CIDs
        signature_bytes: Vec<u8>,
        metadata_bytes: Vec<u8>, // Expecting CBOR-encoded DagNodeMetadata
    ) -> Result<Cid, InternalHostError> { // Returns the CID of the stored node
        // Record base compute cost
        self.record_compute_usage(2000)?; // Base cost for storing

        // Decode necessary parts
        let payload: Ipld = DagCborCodec.decode(&node_payload_bytes)?;
        let parents: Vec<Cid> = parent_cids_bytes
            .into_iter()
            .map(|bytes| Cid::read_bytes(std::io::Cursor::new(bytes)).map_err(|e| InternalHostError::InvalidInput(format!("Invalid parent CID bytes: {}", e))))
            .collect::<Result<Vec<_>, _>>()?; // Changed to Cid::read_bytes
        let metadata: DagNodeMetadata = DagCborCodec.decode(&metadata_bytes)?;
        // Record compute cost for decoding
        self.record_compute_usage(((node_payload_bytes.len() + metadata_bytes.len()) / 100) as u64)?; // Simplified cost

        // Assume issuer DID comes from the VM context (caller)
        // Note: caller_did() returns &str, IdentityId::new() takes impl Into<String>
        let issuer_did_str = self.vm_context.caller_did();
        let issuer_did = IcnIdentityId::new(issuer_did_str);

        // Build the node
        let builder = DagNodeBuilder::new()
            .issuer(issuer_did) // Use caller's DID from context
            .payload(payload)
            .parents(parents)
            .signature(signature_bytes) // Signature provided by WASM
            .metadata(metadata);

        // Store the node using StorageManager
        let store_result = self.storage_manager.store_node(entity_did, builder).await;

        let (cid, stored_node) = match store_result {
            Ok((cid, node)) => {
                // Record storage cost
                let encoded_bytes = DagCborCodec.encode(&node)?;
                self.record_storage_usage(encoded_bytes.len() as u64)?;
                Ok((cid, node))
            }
            Err(e) => Err(InternalHostError::Other(e.into())),
        }?; // Use ? to propagate error

        tracing::debug!(%entity_did, %cid, "Stored node via host_store_node");
        Ok(cid) // Return the CID of the newly stored node
    }


    /// Gets a DAG node by CID from the specified entity's DAG.
    pub async fn get_node(
        &mut self,
        entity_did: &str,
        cid_bytes: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, InternalHostError> { // Returns CBOR bytes of the node
        // Record base compute cost
        self.record_compute_usage(500)?;

        let cid = Cid::read_bytes(std::io::Cursor::new(cid_bytes))
            .map_err(|e| InternalHostError::InvalidInput(format!("Invalid CID bytes: {}", e)))?;

        let node_bytes_opt = self.storage_manager.get_node_bytes(entity_did, &cid).await
            .map_err(|e| InternalHostError::Other(e.into()))?;

        // Record storage cost based on size if found.
        if let Some(bytes) = &node_bytes_opt {
            self.record_storage_usage((bytes.len() / 2) as u64)?; // Cost for reading (adjust factor as needed)
        }

        Ok(node_bytes_opt)
    }

    /// Checks if a DAG node exists within the specified entity's DAG.
    pub async fn contains_node(
        &mut self,
        entity_did: &str,
        cid_bytes: Vec<u8>,
    ) -> Result<bool, InternalHostError> {
        // Record base compute cost
        self.record_compute_usage(200)?; // Cheaper than get

        let cid = Cid::read_bytes(std::io::Cursor::new(cid_bytes))
            .map_err(|e| InternalHostError::InvalidInput(format!("Invalid CID bytes: {}", e)))?;

        let exists = self.storage_manager.contains_node(entity_did, &cid).await
            .map_err(|e| InternalHostError::Other(e.into()))?;

        Ok(exists)
    }

    /// Retrieves and clears the info about the last created entity.
    fn take_last_created_entity_info(&mut self) -> Option<(String, Cid)> {
        self.last_created_entity_info.take()
    }
}

/// Execute a WASM module with the given function and parameters
pub async fn execute_wasm(
    wasm_bytes: &[u8],
    function_name: &str,
    _params: &[u8], // Parameters might be passed via memory, not this slice
    vm_context: VMContext,
    // Pass managers and parent DID to create the environment
    storage_manager: Arc<dyn StorageManager>,
    identity_manager: Arc<dyn IdentityManager>,
    parent_federation_did: Option<String>,
) -> Result<ExecutionResult, VmError> { // Changed return type to async
    use wasmtime::{Config, Engine, Module, Store, Linker};
    use crate::host_abi; // Ensure host_abi is correctly referenced
    use icn_dag::codec; // Ensure codec is in scope

    // Create a host environment with managers
    let mut host_env = ConcreteHostEnvironment::new(
        vm_context,
        storage_manager,
        identity_manager,
        parent_federation_did, // Pass it here
    );

    // Record baseline compute usage for instantiation (should be done in host_env)
    // host_env.record_compute_usage(1000)?; // Move recording into methods

    // Create wasmtime engine (consider caching the engine and module compilation)
    let mut config = Config::new();
    config.async_support(true); // Enable async support for host functions
    config.consume_fuel(true); // Enable fuel-based resource limiting
    config.max_wasm_stack(64 * 1024); // Limit stack size to 64k
    // Review security settings
    config.wasm_reference_types(false);
    config.wasm_bulk_memory(false);
    config.wasm_multi_value(false);

    let engine = Engine::new(&config).map_err(|e|
        VmError::InitializationError(format!("Failed to create engine: {}", e)))?;

    // Compile the module
    let module = Module::new(&engine, wasm_bytes).map_err(|e|
        VmError::InitializationError(format!("Failed to compile module: {}", e)))?;

    // Create a store with our host environment
    let mut store = Store::new(&engine, host_env);

    // Set initial fuel allocation (adjust calculation as needed)
    let initial_fuel = wasm_bytes.len() as u64 * 1000; // Increased fuel multiplier
    store.add_fuel(initial_fuel).map_err(|e|
        VmError::InitializationError(format!("Failed to add fuel: {}", e)))?;

    // Create a linker and register host functions
    let mut linker = Linker::new(&engine);
    // Pass the linker to the registration function
    host_abi::register_host_functions(&mut linker) // Pass linker by mutable ref
        .map_err(|e| VmError::InitializationError(format!("Failed to register host functions: {}", e)))?;

    // Instantiate the module asynchronously (required due to async host funcs)
     let instance = linker
        .instantiate_async(&mut store, &module)
        .await // Use await here
        .map_err(|e| VmError::InitializationError(format!("Failed to instantiate module: {}", e)))?;

    // Get the exported function
    let func = instance.get_func(&mut store, function_name).ok_or_else(||
        VmError::ExecutionError(format!("Function '{}' not found", function_name)))?;

    // Log the function call
    debug!("Executing WASM function: {}", function_name);

    // Prepare parameters (if function takes params directly) and results buffer
     // Function signature needs to be known. Assume main takes no args, returns i32 status for now.
     let mut results = vec![wasmtime::Val::I32(0)];
     let params = &[]; // Empty params slice


    // Call the function asynchronously
    func.call_async(&mut store, params, &mut results) // Use call_async
         .await // Use await here
         .map_err(|trap| {
             // Execution trapped, create error result
             let consumed = store.data().consumed_resources.clone(); // Get consumed resources before store is dropped
             let error_msg = format!("WASM execution trap: {}", trap);
             ExecutionResult::error(error_msg, ResourceConsumption::from_map(consumed))
         })?; // Return Err(ExecutionResult) on trap


    // --- Execution Succeeded (No Trap) --- 

    // Extract WASM return value
    let return_value = match results.get(0) {
         Some(wasmtime::Val::I32(i)) => *i as i32,
         _ => {
             warn!("WASM function '{}' did not return an i32", function_name);
             -1 // Indicate unexpected return type
         }
     };

     // Check if the WASM function indicated success (e.g., returned 0)
     if return_value != 0 {
         warn!(%return_value, function=%function_name, "WASM function indicated failure");
         // Potentially create an ExecutionResult error here based on return_value
     }


    // Get final resource consumption
    let consumed_resources_map = store.data().consumed_resources.clone();
    let resources = ResourceConsumption::from_map(consumed_resources_map);

    // Check for and retrieve entity creation info from the host environment
    let entity_info = store.data_mut().take_last_created_entity_info();

    // Build success result
    let mut result = ExecutionResult::success(vec![return_value as u8], resources);

    // Add entity creation info if it exists
    if let Some((did, genesis_cid)) = entity_info {
        result = result.with_entity_creation(did, genesis_cid);
    }

    Ok(result)
}

// Helper trait/impl for ResourceConsumption (if not already existing)
impl ResourceConsumption {
    fn from_map(map: HashMap<ResourceType, u64>) -> Self {
        Self {
            compute: map.get(&ResourceType::Compute).copied().unwrap_or(0),
            storage: map.get(&ResourceType::Storage).copied().unwrap_or(0),
            network: map.get(&ResourceType::Network).copied().unwrap_or(0),
            // Add other resource types if they exist
            token: map.get(&ResourceType::Token).copied().unwrap_or(0), // Example
        }
    }
}


