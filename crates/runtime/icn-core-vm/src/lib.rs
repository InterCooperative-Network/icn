/*!
# ICN Core VM

The Core Virtual Machine for the ICN Runtime, enabling secure execution of WASM modules
within a sandboxed environment.
*/

pub mod mem_helpers;
pub mod resources;
pub mod host_abi;
mod credentials;
pub mod storage_helpers;
pub mod blob_storage;
pub mod cid_utils;
pub mod dag_helpers;
pub mod economics_helpers;
pub mod monitor;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::*;
use icn_identity::{KeyPair, IdentityScope, IdentityManager, IdentityId as IcnIdentityId};
use icn_identity::error::IdentityError;
use icn_models::storage::{BasicStorageManager, DagStorageManager, StorageError, StorageResult};
use icn_models::{DagNodeBuilder, DagNode, DagNodeMetadata, Cid, dag_storage_codec, DagCodec};
use libipld::{Ipld, codec::Encode, codec::Decode};
use anyhow::{anyhow, Context, Result};
use uuid::Uuid;
use sha2::{Sha256, Digest};
use hex;
use cid::Cid;
use std::convert::TryFrom;
use icn_identity::{IdentityError, IdentityManager, IdentityScope, KeyPair, PublicJwk};
use icn_models::storage::{BasicStorageManager, DagStorageManager, StorageError};
use icn_models::{dag_storage_codec, DagNode, DagNodeBuilder, DagNodeMetadata, IcnIdentityId};
use libipld::codec::{Decode, Encode};
use thiserror::Error;

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
    execution_id: String, // Add execution_id field
}

impl VMContext {
    /// Create a new VM context
    pub fn new(
        identity_context: Arc<IdentityContext>,
        resource_authorizations: Vec<ResourceAuthorization>,
    ) -> Self {
        Self {
            identity_context,
            resource_authorizations,
            execution_id: uuid::Uuid::new_v4().to_string(), // Generate a unique execution ID
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
    
    /// Get the execution ID for this VM context
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }
}

impl Default for VMContext {
    fn default() -> Self {
        Self {
            identity_context: Arc::new(IdentityContext {
                keypair: Arc::new(KeyPair::generate_random()),
                did: "did:icn:anonymous".to_string(),
            }),
            resource_authorizations: Vec::new(),
            execution_id: uuid::Uuid::new_v4().to_string(), // Generate a unique execution ID for default context
        }
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
    StorageError(String),
    #[error("DAG operation failed: {0}")]
    DagError(String),
    #[error("Serialization/Deserialization error: {0}")]
    CodecError(String),
    #[error("Invalid input from WASM: {0}")]
    InvalidInput(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("Virtual machine error: {0}")]
    VmError(String),
    #[error("Generic internal error: {0}")]
    Other(String),
}

impl From<anyhow::Error> for InternalHostError {
    fn from(e: anyhow::Error) -> Self {
        InternalHostError::Other(e.to_string())
    }
}

impl From<VmError> for InternalHostError {
    fn from(e: VmError) -> Self {
        Self::VmError(e.to_string())
    }
}

impl From<StorageError> for InternalHostError {
    fn from(e: StorageError) -> Self {
        Self::StorageError(e.to_string())
    }
}

/// Host environment trait for VM execution (keep existing, maybe add new methods later)
// pub trait HostEnvironment { ... }

/// Concrete implementation of the host environment
#[derive(Clone)]
pub struct ConcreteHostEnvironment {
    vm_context: VMContext,
    storage_manager: Arc<dyn BasicStorageManager + Send + Sync>,
    identity_manager: Arc<dyn IdentityManager>,
    parent_federation_did: Option<String>,
    consumed_resources: Arc<RwLock<HashMap<ResourceType, u64>>>,
    // --- Added temporary state for entity creation --- 
    last_created_entity_info: Option<(String, Cid)>, // Store (DID, Genesis CID)
    /// Resources consumed during execution
    resource_usage: Arc<RwLock<HashMap<ResourceType, u64>>>,
    
    /// The last DAG anchor CID created during execution
    last_anchor_cid: Arc<RwLock<Option<String>>>,
    
    /// DAG storage manager for WASM host ABI functions
    pub dag_storage: Arc<dyn DagStorageManager + Send + Sync>,
}

impl ConcreteHostEnvironment {
    /// Create a new concrete host environment
    pub fn new(
        vm_context: VMContext,
        storage_manager: Arc<dyn BasicStorageManager + Send + Sync>,
        identity_manager: Arc<dyn IdentityManager>,
        parent_federation_did: Option<String>,
        dag_storage: Arc<dyn DagStorageManager + Send + Sync>,
    ) -> Self {
        Self {
            vm_context,
            storage_manager,
            identity_manager,
            parent_federation_did,
            consumed_resources: Arc::new(RwLock::new(HashMap::new())),
            last_created_entity_info: None, // Initialize as None
            resource_usage: Arc::new(RwLock::new(HashMap::new())),
            last_anchor_cid: Arc::new(RwLock::new(None)),
            dag_storage,
        }
    }
    
    /// Get the amount of compute resources consumed
    pub fn get_compute_consumed(&self) -> u64 {
        self.consumed_resources.read().unwrap()
            .get(&ResourceType::Compute).copied().unwrap_or(0)
    }

    /// Get the amount of storage resources consumed
    pub fn get_storage_consumed(&self) -> u64 {
        self.consumed_resources.read().unwrap()
            .get(&ResourceType::Storage).copied().unwrap_or(0)
    }

    /// Get the amount of network resources consumed
    pub fn get_network_consumed(&self) -> u64 {
        self.consumed_resources.read().unwrap()
            .get(&ResourceType::Network).copied().unwrap_or(0)
    }

    /// Record consumption of a resource type
    fn record_resource_consumption(&self, resource_type: ResourceType, amount: u64) -> Result<(), VmError> {
        // Update the usage tracking
        let mut usage = self.resource_usage.write().unwrap();
        let entry = usage.entry(resource_type).or_insert(0);
        
        // Check for overflow
        let new_total = entry.checked_add(amount).ok_or_else(|| {
            VmError::ResourceLimitExceeded(format!(
                "Resource consumption would overflow for {:?}",
                resource_type
            ))
        })?;
        
        // Check authorization limits
        let auth_limit = self.vm_context.resource_authorizations().iter()
            .find(|auth| auth.resource_type == resource_type)
            .map(|auth| auth.limit)
            .unwrap_or(u64::MAX);
        
        if new_total > auth_limit {
            return Err(VmError::ResourceLimitExceeded(format!(
                "Resource limit exceeded for {:?}: {} > {}",
                resource_type, new_total, auth_limit
            )));
        }
        
        // Update the usage tracking
        *entry = new_total;
        
        // Also update the consumed_resources tracker for backward compatibility
        let mut consumed = self.consumed_resources.write().unwrap();
        let consumed_entry = consumed.entry(resource_type).or_insert(0);
        *consumed_entry = new_total;
        
        Ok(())
    }

    /// Record consumption of compute resources
    pub fn record_compute_usage(&self, amount: u64) -> Result<(), VmError> {
        self.record_resource_consumption(ResourceType::Compute, amount)
    }

    /// Record consumption of storage resources
    pub fn record_storage_usage(&self, amount: u64) -> Result<(), VmError> {
        self.record_resource_consumption(ResourceType::Storage, amount)
    }

    /// Record consumption of network resources
    pub fn record_network_usage(&self, amount: u64) -> Result<(), VmError> {
        self.record_resource_consumption(ResourceType::Network, amount)
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
        let _ = self.record_compute_usage(5000); // Adjust cost as needed

        // 1. Generate new DID and Keypair
        let (new_did_key_str, _public_jwk) = self
            .identity_manager
            .generate_and_store_did_key()
            .await
            .map_err(|e| InternalHostError::Other(e.to_string()))?;
        tracing::info!(new_did = %new_did_key_str, "Generated new DID for sub-entity");

        // Record cost associated with key generation
        let _ = self.record_compute_usage(1000); // Cost for crypto op

        // 2. Deserialize/Prepare Genesis Payload
        // Create a codec for serialization/deserialization
        let codec = dag_storage_codec();
        let genesis_ipld: Ipld = codec.decode(&genesis_payload_bytes)
            .map_err(|e| InternalHostError::CodecError(e.to_string()))?;
        
        // Record cost for decoding
        let _ = self.record_compute_usage((genesis_payload_bytes.len() / 100) as u64);

        // 3. Construct Genesis DagNode
        //    The issuer ('iss') of the genesis node is the *new* entity's DID.
        //    Parents list is empty for a genesis node.
        let node_builder = DagNodeBuilder::default()
            .with_issuer(IcnIdentityId::new(new_did_key_str.clone())) // Use correct builder method
            .with_payload(genesis_ipld) // Store the provided payload
            .with_parents(vec![]); // Genesis node has no parents

        // 4. Store Genesis Node using DAG Storage Manager
        let store_result = self
            .dag_storage
            .store_new_dag_root(&new_did_key_str, &node_builder)
            .await
            .map_err(|e| InternalHostError::StorageError(e.to_string()));

        let (genesis_cid, genesis_node) = match store_result {
             Ok((cid, node)) => {
                 // Record storage cost - size of the encoded node
                 let encoded_size = serde_json::to_vec(&node)
                     .map_err(|e| InternalHostError::CodecError(format!("Failed to serialize node: {}", e)))?
                     .len();
                 self.record_storage_usage(encoded_size as u64)?;
                 Ok((cid, node))
             }
             Err(e) => {
                 // Attempt to clean up stored key if node storage failed
                 tracing::error!(new_did = %new_did_key_str, "Failed to store genesis node, attempting to delete key");
                 let _ = self.identity_manager.get_key(&new_did_key_str).await; // Example: How to delete key? Needs method in IdentityManager/KeyStorage
                 Err(e) // Convert StorageManager's Result
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
             tracing::error!(
                 new_did = %new_did_key_str,
                 %genesis_cid,
                 parent_did = %parent_did,
                 "CRITICAL: Failed to register entity metadata after storing genesis node: {}", e
             );
             return Err(InternalHostError::Other(e.to_string())); // Convert IdentityManager's Result
         }

        // Record cost for metadata storage (assume small fixed cost)
        self.record_storage_usage(100)?;

        // 6. Store DID and Genesis CID internally
        self.last_created_entity_info = Some((new_did_key_str.clone(), genesis_cid));

        // 7. Return the new DID string
        Ok(new_did_key_str)
    }

    /// Stores a regular DAG node within the specified entity's DAG.
    pub async fn store_dag_node(
        &self,
        entity_did: &str,
        node_payload_bytes: Vec<u8>,
        parent_cids_bytes: Vec<Vec<u8>>,
        signature_bytes: Vec<u8>,
        metadata_bytes: Vec<u8>,
    ) -> Result<Cid, InternalHostError> { // Returns the CID of the stored node
        // Record base compute cost
        self.record_compute_usage(2000)?; // Base cost for storing

        // Create a codec for serialization
        let codec = dag_storage_codec();
        
        // Parse the payload from CBOR
        let payload: Ipld = codec.decode(&node_payload_bytes)
            .map_err(|e| InternalHostError::CodecError(format!("Failed to decode payload: {}", e)))?;
        
        // Parse parent CIDs
        let mut parents = Vec::new();
        for cid_bytes in parent_cids_bytes {
            match Cid::read_bytes(std::io::Cursor::new(&cid_bytes)) {
                Ok(cid) => parents.push(cid),
                Err(e) => return Err(InternalHostError::InvalidInput(format!("Invalid parent CID: {}", e))),
            }
        }
        
        // Parse metadata
        let metadata = if !metadata_bytes.is_empty() {
            match codec.decode::<DagNodeMetadata>(&metadata_bytes) {
                Ok(m) => m,
                Err(e) => return Err(InternalHostError::CodecError(format!("Failed to decode metadata: {}", e))),
            }
        } else {
            // Create default metadata if none provided
            DagNodeMetadata {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| InternalHostError::Other(format!("Failed to get timestamp: {}", e)))?
                    .as_secs(),
                sequence: 0, // Default sequence
                content_type: None,
                tags: Vec::new(),
            }
        };
        
        // Create the node builder
        let node_builder = DagNodeBuilder::default()
            .with_issuer(IcnIdentityId::new(entity_did.to_string()))
            .with_payload(payload)
            .with_parents(parents)
            .with_metadata(metadata);
        
        // Store the node using DagStorageManager
        let result = self.dag_storage.store_node(entity_did, &node_builder).await
            .map_err(|e| InternalHostError::StorageError(e.to_string()));
        
        match result {
            Ok((cid, node)) => {
                // Calculate storage costs based on estimated size
                let encoded_size = serde_json::to_vec(&node)
                    .map_err(|e| InternalHostError::CodecError(format!("Failed to serialize node: {}", e)))?
                    .len();
                self.record_storage_usage(encoded_size as u64)?;
                Ok(cid)
            },
            Err(e) => Err(e),
        }
    }

    /// Retrieves a DAG node by CID
    pub async fn get_dag_node(
        &self,
        entity_did: &str,
        cid_bytes: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, InternalHostError> { // Returns CBOR bytes of the node
        // Record base compute cost
        self.record_compute_usage(500)?;

        let cid = Cid::read_bytes(std::io::Cursor::new(cid_bytes))
            .map_err(|e| InternalHostError::InvalidInput(format!("Invalid CID bytes: {}", e)))?;

        let node_bytes_opt = self.dag_storage.get_node_bytes(entity_did, &cid).await
            .map_err(|e| InternalHostError::StorageError(e.to_string()))?;

        // Record storage cost based on size if found.
        if let Some(bytes) = &node_bytes_opt {
            self.record_storage_usage((bytes.len() / 2) as u64)?; // Cost for reading (adjust factor as needed)
        }

        Ok(node_bytes_opt)
    }

    /// Checks if a DAG node exists
    pub async fn contains_dag_node(
        &self,
        entity_did: &str,
        cid_bytes: Vec<u8>,
    ) -> Result<bool, InternalHostError> {
        // Record base compute cost
        self.record_compute_usage(200)?; // Cheaper than get

        let cid = Cid::read_bytes(std::io::Cursor::new(cid_bytes))
            .map_err(|e| InternalHostError::InvalidInput(format!("Invalid CID bytes: {}", e)))?;

        let exists = self.dag_storage.contains_node(entity_did, &cid).await
            .map_err(|e| InternalHostError::StorageError(e.to_string()))?;

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
        
        // Convert to a hex string prefixed with 'bafybeih'
        let hex_string = format!("bafybeih{}", hex::encode(&hash[0..16]));
        
        Ok(hex_string)
    }

    /// Anchors data to the DAG with the given key
    /// Returns the CID of the anchored data on success
    pub async fn anchor_to_dag(&self, key: &str, data: Vec<u8>) -> Result<String, InternalHostError> {
        // Get the dag_store from environment
        let storage_manager = self.storage_manager()?;
        let dag_store = storage_manager.dag_store()
            .map_err(|e| InternalHostError::StorageError(format!("Failed to get DAG store: {}", e)))?;
            
        // Calculate content CID
        let content_cid = Self::compute_content_cid(&data)
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
    /// Only administrators can call this method successfully
    pub async fn mint_tokens(&self, resource_type: ResourceType, recipient: &str, amount: u64) -> Result<(), InternalHostError> {
        // Check if caller is an administrator
        if self.caller_scope() != IdentityScope::Administrator {
            return Err(InternalHostError::Other(format!(
                "Only administrators can mint tokens, caller scope: {:?}", 
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
            // Allow administrators to transfer on behalf of others
            if self.caller_scope() != IdentityScope::Administrator {
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

    /// Retrieve DAG anchors by scope and type
    pub async fn get_anchors_by_scope_and_type(&self, scope: &str, anchor_type: &str) -> Result<Vec<(String, Vec<u8>)>, InternalHostError> {
        // Get storage manager
        let storage_manager = self.storage_manager()
            .map_err(|e| InternalHostError::StorageError(format!("Failed to get storage manager: {}", e)))?;
        
        // Create a prefix for the anchor keys
        let prefix = if anchor_type.starts_with("credential:") {
            anchor_type.to_string()
        } else {
            format!("{}:{}", scope, anchor_type)
        };
        
        // List DAG nodes with this prefix
        // Note: In a real implementation, we would need a way to list DAG nodes by prefix
        // For now, we'll assume the storage manager has this capability
        
        // Mock implementation - this would be replaced with actual implementation
        // that uses the storage manager to get anchors matching the prefix
        let anchors = Vec::new();
        
        Ok(anchors)
    }

    /// Get a reference to the storage manager
    pub fn storage_manager(&self) -> Result<Arc<dyn BasicStorageManager + Send + Sync>, InternalHostError> {
        Ok(self.storage_manager.clone())
    }

    /// Get a value from storage
    pub fn get_value(&self, key: &str) -> Option<Vec<u8>> {
        // This is a simplified implementation for the moment
        // In a real implementation, this would use a key-value store
        tracing::debug!("get_value called for key: {}", key);
        None // Always return None for now
    }

    /// Store a key-value pair in storage
    pub fn set_value(&self, key: &str, value: Vec<u8>) -> Result<(), InternalHostError> {
        // Record storage usage
        self.record_compute_usage(100)?;
        self.record_storage_usage(value.len() as u64)?;
        
        // In a real implementation, this would store the value in a storage system
        // For now, we just log it
        tracing::info!(key = %key, value_len = %value.len(), "Storing key-value pair");
        
        Ok(())
    }

    /// Delete a value from storage
    pub fn delete_value(&self, key: &str) -> Result<(), InternalHostError> {
        // Record compute usage
        self.record_compute_usage(50)?;
        
        // In a real implementation, this would delete from storage
        // For now, we just log it
        tracing::info!(key = %key, "Deleting key-value pair");
        
        Ok(())
    }

    /// Store a DAG node using the DagStorageManager
    pub async fn store_node(&self, node: DagNode) -> Result<(), InternalHostError> {
        // Record base compute cost for storing a node
        self.record_compute_usage(500)?;
        
        // Get the entity DID - here we use the caller's DID as the entity owner
        let entity_did = self.vm_context.caller_did();
        
        // Ensure we don't try to store a node with mismatched issuer
        if entity_did != node.issuer.to_string() {
            return Err(InternalHostError::Other(format!(
                "Node issuer ({}) must match caller's DID ({})",
                node.issuer, entity_did
            )));
        }
        
        // Create a node builder from the existing node
        // This is a bit inefficient since we're converting to a builder and back,
        // but follows the current DagStorageManager interface
        let builder = DagNodeBuilder::new()
            .with_issuer(node.issuer.to_string())
            .with_parents(node.parents.clone())
            .with_metadata(node.metadata.clone())
            .with_payload(node.payload.clone())
            .with_signature(node.signature.clone());
        
        // Store the node using the DAG storage manager
        match self.dag_storage.store_node(entity_did, &builder).await {
            Ok(_) => {
                // Record storage cost based on node size
                // For simplicity, we'll use a rough estimate - in a real implementation,
                // we might want to serialize the node to get exact byte count
                let estimated_size = 256 + node.signature.len() as u64; // Base size + signature
                self.record_storage_usage(estimated_size)?;
                Ok(())
            },
            Err(e) => Err(InternalHostError::Other(format!("Failed to store node: {}", e))),
        }
    }
    
    /// Retrieve a DAG node by its CID
    pub async fn get_node(&self, cid: &Cid) -> Result<Option<DagNode>, InternalHostError> {
        // Record base compute cost for retrieving a node
        self.record_compute_usage(200)?;
        
        // Get the entity DID - here we use the caller's DID 
        let entity_did = self.vm_context.caller_did();
        
        // Retrieve the node using the DAG storage manager
        match self.dag_storage.get_node(entity_did, cid).await {
            Ok(node_opt) => {
                if let Some(ref node) = node_opt {
                    // Record network cost based on node size
                    let estimated_size = 256 + node.signature.len() as u64;
                    self.record_network_usage(estimated_size)?;
                }
                Ok(node_opt)
            },
            Err(e) => Err(InternalHostError::Other(format!("Failed to retrieve node: {}", e))),
        }
    }
    
    /// Check if a DAG node exists by its CID
    pub async fn contains_node(&self, cid: &Cid) -> Result<bool, InternalHostError> {
        // Record base compute cost for checking node existence
        self.record_compute_usage(100)?;
        
        // Get the entity DID - here we use the caller's DID
        let entity_did = self.vm_context.caller_did();
        
        // Check if the node exists using the DAG storage manager
        match self.dag_storage.contains_node(entity_did, cid).await {
            Ok(exists) => {
                // Minimal network cost for boolean result
                self.record_network_usage(4)?;
                Ok(exists)
            },
            Err(e) => Err(InternalHostError::Other(format!("Failed to check node existence: {}", e))),
        }
    }

    /// Anchors metadata to the DAG
    /// This is a specialized function for WASM modules to anchor metadata
    /// like governance receipts, economic actions, or verifiable messages
    pub async fn anchor_metadata_to_dag(&self, anchor_json: &str) -> Result<(), InternalHostError> {
        // Parse the JSON payload
        let payload = serde_json::from_str::<serde_json::Value>(anchor_json)
            .map_err(|e| InternalHostError::InvalidInput(format!("Invalid JSON payload: {}", e)))?;
        
        // Extract important metadata fields if they exist
        let anchor_type = payload["type"].as_str().unwrap_or("generic");
        let scope = payload["scope"].as_str().unwrap_or("unknown");
        
        // Create a unique key for this anchor
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| InternalHostError::Other(format!("Failed to get timestamp: {}", e)))?
            .as_secs();
        
        let key = format!("{}:{}:{}", scope, anchor_type, timestamp);
        
        // Record compute usage
        self.record_compute_usage(500)?;
        
        // Record storage usage (approximate size of the payload)
        let payload_size = anchor_json.len() as u64;
        self.record_storage_usage(payload_size)?;
        
        // Create enriched metadata
        let enriched_payload = serde_json::json!({
            "original": payload,
            "metadata": {
                "anchored_by": self.caller_did(),
                "timestamp": timestamp,
                "anchor_type": anchor_type,
                "scope": scope
            }
        });
        
        // Serialize the enriched payload
        let data = serde_json::to_vec(&enriched_payload)
            .map_err(|e| InternalHostError::CodecError(format!("Failed to serialize enriched payload: {}", e)))?;
        
        // Anchor the data using the existing method
        let cid = self.anchor_to_dag(&key, data).await?;
        
        // Store the CID as the last anchor CID
        self.set_last_anchor_cid(cid);
        
        Ok(())
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
        
    // Clone the host environment for the store
    let mut host_env = host_env.clone();
    
    // Create a store with the host environment
    let mut store = Store::new(&engine, host_env);
    
    // Allocate fuel for the execution (1,000,000 units as default)
    let fuel_limit = context.resource_authorizations()
        .iter()
        .find(|auth| auth.resource_type == ResourceType::Compute)
        .map_or(1_000_000, |auth| auth.limit);
        
    store.add_fuel(fuel_limit)
        .map_err(|e| VmError::FuelAllocationFailed(e.to_string()))?;
    
    // Create import object with host functions
    let mut linker = host_abi::create_import_object(&mut store);
    
    // Instantiate the module
    let instance = linker.instantiate(&mut store, &module)
        .map_err(|e| VmError::InstantiationFailed(e.to_string()))?;
        
    // Check for a "main" export
    let main_func = instance.get_typed_func::<(), i32>(&mut store, "main")
        .or_else(|_| instance.get_typed_func::<(), i32>(&mut store, "_start"))
        .or_else(|_| instance.get_typed_func::<(), i32>(&mut store, "__main"))
        .map_err(|_| VmError::EntryPointNotFound("No main/_start/__main function found".to_string()))?;
        
    // Execute the function
    let return_code = main_func.call(&mut store, ())
        .map_err(|e| {
            if e.to_string().contains("out of fuel") {
                VmError::ResourceLimitExceeded("Execution exceeded fuel limit".to_string())
            } else {
                VmError::ExecutionError(e.to_string())
            }
        })?;
    
    // Get resource usage
    let resource_usage = store.data().get_resource_usage();
    
    // Get the last anchor CID if there was one
    let dag_anchor_cid = store.data().get_last_anchor_cid();
    
    Ok(VmExecutionResult {
        code: return_code,
        resource_usage,
        dag_anchor_cid,
    })
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


