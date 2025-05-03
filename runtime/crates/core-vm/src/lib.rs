/*!
# ICN Core VM

The Core Virtual Machine for the ICN Runtime, enabling secure execution of WASM modules
within a sandboxed environment.
*/

pub mod mem_helpers;
pub mod resources;
pub mod host_abi;
mod credentials;

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
use std::sync::RwLock;

pub use resources::{ResourceType, ResourceAuthorization, ResourceConsumption};

// Re-export credentials module functionality
pub use credentials::{
    CredentialType,
    VerifiableCredential,
    ExecutionReceiptSubject,
    issue_execution_receipt,
    get_execution_receipt_by_cid,
    get_execution_receipts_by_proposal,
};

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

    #[error("Engine creation failed: {0}")]
    EngineCreationFailed(String),

    #[error("Module creation failed: {0}")]
    ModuleCreationFailed(String),

    #[error("Fuel allocation failed: {0}")]
    FuelAllocationFailed(String),

    #[error("Instantiation failed: {0}")]
    InstantiationFailed(String),

    #[error("Entry point not found: {0}")]
    EntryPointNotFound(String),
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
    /// Resources consumed during execution
    resource_usage: RwLock<HashMap<ResourceType, u64>>,
    
    /// The last DAG anchor CID created during execution
    last_anchor_cid: RwLock<Option<String>>,
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
            resource_usage: RwLock::new(HashMap::new()),
            last_anchor_cid: RwLock::new(None),
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
    pub fn record_compute_usage(&self, amount: u64) -> Result<(), VmError> {
        self.record_resource_usage(ResourceType::Compute, amount)
    }

    /// Record consumption of storage resources
    pub fn record_storage_usage(&self, amount: u64) -> Result<(), VmError> {
        self.record_resource_usage(ResourceType::Storage, amount)
    }

    /// Record consumption of network resources
    pub fn record_network_usage(&self, amount: u64) -> Result<(), VmError> {
        self.record_resource_usage(ResourceType::Network, amount)
    }

    /// Record consumption of a resource type
    fn record_resource_usage(&self, resource_type: ResourceType, amount: u64) -> Result<(), VmError> {
        let usage = self.resource_usage.read().unwrap();
        let current = usage.entry(resource_type).or_insert(0);
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

    /// Helper function to compute a CID for content
    fn compute_content_cid(data: &[u8]) -> Result<String, InternalHostError> {
        use sha2::{Sha256, Digest};
        
        // Create a SHA-256 hash of the data
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        
        // Convert to a base58 string prefixed with 'bafybeih'
        let hex_string = format!("bafybeih{}", hex::encode(&hash[0..16]));
        
        Ok(hex_string)
    }

    /// Anchors data to the DAG with the given key
    /// Returns the CID of the anchored data on success
    pub async fn anchor_to_dag(&self, key: &str, data: Vec<u8>) -> Result<String, InternalHostError> {
        // Get the dag_store from environment
        let dag_store = self.storage_manager.dag_store()
            .map_err(|e| InternalHostError::StorageError(format!("Failed to get DAG store: {}", e)))?;
            
        // Calculate content CID
        let content_cid = compute_content_cid(&data)
            .map_err(|e| InternalHostError::DagError(format!("Failed to compute content CID: {}", e)))?;
        
        // Prepare DAG node with key, data, and execution context
        let caller_did = self.caller_did().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| InternalHostError::Other(format!("Failed to get timestamp: {}", e)))?
            .as_secs();
            
        // Create metadata
        let metadata = serde_json::json!({
            "key": key,
            "timestamp": timestamp,
            "execution_id": self.vm_context.execution_id(),
            "caller_did": caller_did
        });
        
        // Store the content first
        dag_store.store_blob(&content_cid, data)
            .await
            .map_err(|e| InternalHostError::StorageError(format!("Failed to store data blob: {}", e)))?;
            
        // Create DAG node that references the content
        let dag_node = serde_json::json!({
            "key": key,
            "content_cid": content_cid,
            "metadata": metadata,
            "issuer": caller_did
        });
        
        // Serialize the DAG node
        let node_bytes = serde_json::to_vec(&dag_node)
            .map_err(|e| InternalHostError::CodecError(format!("Failed to serialize DAG node: {}", e)))?;
            
        // Store DAG node and get its CID
        let node_cid = dag_store.store_node(node_bytes)
            .await
            .map_err(|e| InternalHostError::StorageError(format!("Failed to store DAG node: {}", e)))?;
            
        // Record a mapping from key to CID for easier lookup
        let key_mapping = format!("key:{}", key);
        self.set_value(&key_mapping, node_cid.clone().into_bytes())
            .map_err(|e| InternalHostError::StorageError(format!("Failed to store key mapping: {}", e)))?;
            
        Ok(node_cid)
    }
    
    /// Mint tokens of a specific resource type to a recipient
    /// Only Guardians can call this method successfully
    pub async fn mint_tokens(&self, resource_type: ResourceType, recipient: &str, amount: u64) -> Result<(), InternalHostError> {
        // Check if caller is a Guardian
        if self.caller_scope() != IdentityScope::Guardian {
            return Err(InternalHostError::Other(format!(
                "Only Guardians can mint tokens, caller scope: {:?}", 
                self.caller_scope()
            )));
        }
        
        // Log the minting operation
        info!(
            resource_type = ?resource_type,
            recipient = %recipient,
            amount = %amount,
            "Minting tokens"
        );
        
        // In a real implementation, this would interact with a token management system
        // For now, we just simulate success
        
        // Record the minting in storage for tracking
        let mint_record = serde_json::json!({
            "operation": "mint",
            "resource_type": format!("{:?}", resource_type),
            "recipient": recipient,
            "amount": amount,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| InternalHostError::Other(format!("Failed to get timestamp: {}", e)))?
                .as_secs(),
            "minter": self.caller_did()
        });
        
        // Store the mint record
        let record_key = format!("mint:{}:{}", recipient, uuid::Uuid::new_v4());
        let record_bytes = serde_json::to_vec(&mint_record)
            .map_err(|e| InternalHostError::CodecError(format!("Failed to serialize mint record: {}", e)))?;
            
        self.set_value(&record_key, record_bytes)
            .map_err(|e| InternalHostError::StorageError(format!("Failed to store mint record: {}", e)))?;
            
        Ok(())
    }
    
    /// Transfer resources from one identity to another
    pub async fn transfer_resources(
        &self, 
        resource_type: ResourceType, 
        from_did: &str, 
        to_did: &str, 
        amount: u64
    ) -> Result<(), InternalHostError> {
        // Check authorization
        if self.caller_did() != from_did {
            // Allow Guardians to transfer on behalf of others
            if self.caller_scope() != IdentityScope::Guardian {
                return Err(InternalHostError::Other(format!(
                    "Caller {} not authorized to transfer from {}", 
                    self.caller_did(), from_did
                )));
            }
        }
        
        // Log the transfer operation
        info!(
            resource_type = ?resource_type,
            from = %from_did,
            to = %to_did,
            amount = %amount,
            "Transferring resources"
        );
        
        // In a real implementation, this would interact with a token management system
        // For now, we just simulate success
        
        // Record the transfer in storage for tracking
        let transfer_record = serde_json::json!({
            "operation": "transfer",
            "resource_type": format!("{:?}", resource_type),
            "from": from_did,
            "to": to_did,
            "amount": amount,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| InternalHostError::Other(format!("Failed to get timestamp: {}", e)))?
                .as_secs(),
            "authorized_by": self.caller_did()
        });
        
        // Store the transfer record
        let record_key = format!("transfer:{}:{}:{}", from_did, to_did, uuid::Uuid::new_v4());
        let record_bytes = serde_json::to_vec(&transfer_record)
            .map_err(|e| InternalHostError::CodecError(format!("Failed to serialize transfer record: {}", e)))?;
            
        self.set_value(&record_key, record_bytes)
            .map_err(|e| InternalHostError::StorageError(format!("Failed to store transfer record: {}", e)))?;
            
        Ok(())
    }

    /// Get the resource usage map
    pub fn get_resource_usage(&self) -> HashMap<ResourceType, u64> {
        self.resource_usage.read().unwrap().clone()
    }
    
    /// Record resource usage
    pub fn record_resource_usage(&self, resource_type: ResourceType, amount: u64) {
        let mut usage = self.resource_usage.write().unwrap();
        let entry = usage.entry(resource_type).or_insert(0);
        *entry += amount;
    }
    
    /// Get the last DAG anchor CID created during execution
    pub fn get_last_anchor_cid(&self) -> Option<String> {
        self.last_anchor_cid.read().unwrap().clone()
    }
    
    /// Set the last DAG anchor CID
    pub fn set_last_anchor_cid(&self, cid: String) {
        let mut last_cid = self.last_anchor_cid.write().unwrap();
        *last_cid = Some(cid);
    }
}

/// Represents the result of a VM execution
#[derive(Debug, Clone)]
pub struct VmExecutionResult {
    /// Return code from the WASM execution (0 typically means success)
    pub code: i32,
    
    /// Resources consumed during execution
    pub resource_usage: HashMap<ResourceType, u64>,
    
    /// CID of the last DAG anchor created during execution, if any
    pub dag_anchor_cid: Option<String>,
}

/// Execute a WASM module in a sandboxed environment
pub async fn execute_wasm(
    wasm_bytes: &[u8],
    context: Option<VMContext>,
    host_env: &ConcreteHostEnvironment,
    proposal_id: Option<&str>,
    federation_scope: Option<&str>,
) -> Result<VmExecutionResult, VmError> {
    let context = context.unwrap_or_default();
    
    // Create a new Wasmtime engine with appropriate config
    let mut config = Config::new();
    config.wasm_bulk_memory(true);
    config.wasm_reference_types(true);
    config.async_support(true);
    config.consume_fuel(true);
    
    let engine = Engine::new(&config)
        .map_err(|e| VmError::EngineCreationFailed(e.to_string()))?;
    
    // Set up the WASM module
    let module = Module::new(&engine, wasm_bytes)
        .map_err(|e| VmError::ModuleCreationFailed(e.to_string()))?;
    
    // Create a new store with the host environment
    let mut store = Store::new(&engine, host_env.clone());
    
    // Allocate fuel for execution (1M units by default)
    store.add_fuel(1_000_000)
        .map_err(|e| VmError::FuelAllocationFailed(e.to_string()))?;
    
    // Set up the initial resource usage tracking
    let mut resource_usage = HashMap::new();
    
    // Create an instance of the module with imported functions
    let instance = Instance::new_async(&mut store, &module, &host_abi::create_import_object(&mut store))
        .await
        .map_err(|e| VmError::InstantiationFailed(e.to_string()))?;
    
    // Get the default export function from the module
    let execute_fn = instance.get_typed_func::<(), i32>(&mut store, "execute")
        .map_err(|e| VmError::EntryPointNotFound(e.to_string()))?;
    
    // Execute the WASM function
    let result = execute_fn.call_async(&mut store, ()).await;
    
    // Calculate fuel used
    let fuel_used = store.fuel_consumed().unwrap_or(0);
    resource_usage.insert(ResourceType::Compute, fuel_used as u64);
    
    // Get environment from store to check resource usage
    let host_env = store.into_data();
    
    let execution_result = match result {
        Ok(code) => {
            // Add resource usage from host environment
            // This would be populated by the various host functions during execution
            for (resource_type, amount) in host_env.get_resource_usage() {
                let entry = resource_usage.entry(resource_type).or_insert(0);
                *entry += amount;
            }
            
            let outcome = if code == 0 { "Success" } else { "Failure" };
            
            // If we have a proposal ID, issue an execution receipt credential
            if let (Some(pid), Some(scope)) = (proposal_id, federation_scope) {
                if let Err(e) = issue_execution_receipt(
                    &host_env,
                    pid,
                    outcome,
                    resource_usage.clone(),
                    &host_env.get_last_anchor_cid().unwrap_or_default(),
                    scope,
                ).await {
                    tracing::warn!(error = %e, "Failed to issue execution receipt");
                }
            }
            
            Ok(VmExecutionResult {
                code,
                resource_usage,
                dag_anchor_cid: host_env.get_last_anchor_cid(),
            })
        },
        Err(e) => {
            let error_message = e.to_string();
            
            // If we have a proposal ID, issue an execution receipt credential for the failure
            if let (Some(pid), Some(scope)) = (proposal_id, federation_scope) {
                if let Err(e) = issue_execution_receipt(
                    &host_env,
                    pid,
                    "Error",
                    resource_usage.clone(),
                    &host_env.get_last_anchor_cid().unwrap_or_default(),
                    scope,
                ).await {
                    tracing::warn!(error = %e, "Failed to issue execution receipt for error");
                }
            }
            
            Err(VmError::ExecutionFailed(error_message))
        }
    };
    
    execution_result
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


