//! Contract Registry - persistent storage and lookup for deployed contracts
//!
//! Enables "deploy once, invoke many times" pattern:
//! 1. Deploy contract to registry (gets content hash)
//! 2. Reference contract by hash in compute tasks
//! 3. Executor loads contract from local registry or fetches from peers
//!
//! ## Storage Schema
//!
//! ```text
//! contract:<hash>          → Contract (bincode)
//! metadata:<hash>          → ContractMetadata (bincode)
//! index:name:<name>        → Vec<(version, hash)>
//! index:owner:<did>        → Vec<hash>
//! ```

use crate::ast::Contract;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Content hash for contract addressing (blake3)
pub type ContentHash = [u8; 32];

/// Registry errors
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Contract not found: {0}")]
    NotFound(String),

    #[error("Contract already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid contract: {0}")]
    InvalidContract(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Authorization error: {0}")]
    AuthorizationError(String),
}

pub type Result<T> = std::result::Result<T, RegistryError>;

/// Contract visibility level
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Visibility {
    /// Only visible to owner
    #[default]
    Private,
    /// Visible to cooperative members
    Coop(String),
    /// Publicly visible (may require governance approval)
    Public,
}

/// Contract metadata stored alongside the contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    /// Content hash of the contract
    pub code_hash: ContentHash,
    /// Contract name
    pub name: String,
    /// Version number (incrementing)
    pub version: u32,
    /// Owner/deployer DID
    pub owner: String,
    /// Deployment timestamp (Unix millis)
    pub deployed_at: u64,
    /// Optional description
    pub description: Option<String>,
    /// Contract participants
    pub participants: Vec<String>,
    /// Currency if specified
    pub currency: Option<String>,
    /// Rule names for discovery
    pub rules: Vec<String>,
    /// Visibility level
    pub visibility: Visibility,
}

impl ContractMetadata {
    /// Create metadata from a contract and deployer info
    ///
    /// Returns an error if the contract cannot be hashed.
    pub fn from_contract(contract: &Contract, owner: &str, version: u32) -> Result<Self> {
        let code_hash = compute_hash(contract)?;
        Ok(ContractMetadata {
            code_hash,
            name: contract.name.clone(),
            version,
            owner: owner.to_string(),
            deployed_at: icn_time::current_timestamp_millis(),
            description: None,
            participants: contract
                .participants
                .iter()
                .map(|d| d.to_string())
                .collect(),
            currency: contract.currency.clone(),
            rules: contract.rules.iter().map(|r| r.name.clone()).collect(),
            visibility: Visibility::default(),
        })
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set visibility
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }
}

/// Compute content hash for a contract
///
/// Returns an error if the contract cannot be serialized to JSON.
pub fn compute_hash(contract: &Contract) -> Result<ContentHash> {
    let bytes = serde_json::to_vec(contract).map_err(|e| {
        RegistryError::SerializationError(format!("Failed to serialize contract: {e}"))
    })?;
    Ok(*blake3::hash(&bytes).as_bytes())
}

/// Contract Registry - stores contracts with metadata
///
/// Thread-safe with in-memory caching and optional persistent storage.
#[allow(clippy::type_complexity)]
pub struct ContractRegistry {
    /// In-memory contract cache
    contracts: Arc<RwLock<HashMap<ContentHash, Contract>>>,
    /// In-memory metadata cache
    metadata: Arc<RwLock<HashMap<ContentHash, ContractMetadata>>>,
    /// Name → versions index
    name_index: Arc<RwLock<HashMap<String, Vec<(u32, ContentHash)>>>>,
    /// Owner → contracts index
    owner_index: Arc<RwLock<HashMap<String, Vec<ContentHash>>>>,
    /// Optional persistent store
    store: Option<Arc<dyn icn_store::Store>>,
}

impl ContractRegistry {
    /// Create a new in-memory registry
    pub fn new() -> Self {
        Self {
            contracts: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            owner_index: Arc::new(RwLock::new(HashMap::new())),
            store: None,
        }
    }

    /// Create a registry with persistent storage
    pub fn with_store(store: Arc<dyn icn_store::Store>) -> Self {
        Self {
            contracts: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            owner_index: Arc::new(RwLock::new(HashMap::new())),
            store: Some(store),
        }
    }

    /// Deploy a contract to the registry
    ///
    /// Returns the content hash on success.
    pub async fn deploy(
        &self,
        contract: Contract,
        owner: &str,
        version: Option<u32>,
    ) -> Result<ContentHash> {
        self.deploy_with_visibility(contract, owner, version, Visibility::default())
            .await
    }

    /// Deploy a contract with explicit visibility
    pub async fn deploy_with_visibility(
        &self,
        contract: Contract,
        owner: &str,
        version: Option<u32>,
        visibility: Visibility,
    ) -> Result<ContentHash> {
        // Validate contract
        contract
            .validate()
            .map_err(|e| RegistryError::InvalidContract(e.to_string()))?;

        let hash = compute_hash(&contract)?;
        let hash_hex = hex::encode(hash);

        // Check if already exists
        {
            let contracts = self.contracts.read().await;
            if contracts.contains_key(&hash) {
                return Err(RegistryError::AlreadyExists(hash_hex));
            }
        }

        // Determine version
        let version = match version {
            Some(v) => v,
            None => {
                // Auto-increment based on existing versions
                let name_index = self.name_index.read().await;
                name_index
                    .get(&contract.name)
                    .map(|versions| versions.iter().map(|(v, _)| *v).max().unwrap_or(0) + 1)
                    .unwrap_or(1)
            }
        };

        // Create metadata with visibility
        let metadata =
            ContractMetadata::from_contract(&contract, owner, version)?.with_visibility(visibility);

        // Persist to store if available
        if let Some(store) = &self.store {
            let contract_key = format!("contract:{hash_hex}");
            let contract_bytes = icn_encoding::encode_versioned(&contract)
                .map_err(|e| RegistryError::SerializationError(e.to_string()))?;
            store
                .put(contract_key.as_bytes(), &contract_bytes)
                .map_err(|e| RegistryError::StorageError(e.to_string()))?;

            let metadata_key = format!("metadata:{hash_hex}");
            let metadata_bytes = icn_encoding::encode_versioned(&metadata)
                .map_err(|e| RegistryError::SerializationError(e.to_string()))?;
            store
                .put(metadata_key.as_bytes(), &metadata_bytes)
                .map_err(|e| RegistryError::StorageError(e.to_string()))?;
        }

        // Update in-memory caches
        {
            let mut contracts = self.contracts.write().await;
            contracts.insert(hash, contract.clone());
        }
        {
            let mut meta_cache = self.metadata.write().await;
            meta_cache.insert(hash, metadata.clone());
        }
        {
            let mut name_index = self.name_index.write().await;
            name_index
                .entry(contract.name.clone())
                .or_default()
                .push((version, hash));
        }
        {
            let mut owner_index = self.owner_index.write().await;
            owner_index.entry(owner.to_string()).or_default().push(hash);
        }

        tracing::info!(
            hash = %hash_hex,
            name = %contract.name,
            version = version,
            owner = %owner,
            "Contract deployed to registry"
        );

        Ok(hash)
    }

    /// Get a contract by hash
    pub async fn get(&self, hash: &ContentHash) -> Result<Option<Contract>> {
        // Check cache first
        {
            let contracts = self.contracts.read().await;
            if let Some(contract) = contracts.get(hash) {
                return Ok(Some(contract.clone()));
            }
        }

        // Try persistent store
        if let Some(store) = &self.store {
            let key = format!("contract:{}", hex::encode(hash));
            if let Some(bytes) = store
                .get(key.as_bytes())
                .map_err(|e| RegistryError::StorageError(e.to_string()))?
            {
                let contract: Contract = icn_encoding::decode_versioned(&bytes)
                    .map_err(|e| RegistryError::SerializationError(e.to_string()))?;

                // Populate cache
                let mut contracts = self.contracts.write().await;
                contracts.insert(*hash, contract.clone());

                return Ok(Some(contract));
            }
        }

        Ok(None)
    }

    /// Get contract metadata by hash
    pub async fn get_metadata(&self, hash: &ContentHash) -> Result<Option<ContractMetadata>> {
        // Check cache first
        {
            let metadata = self.metadata.read().await;
            if let Some(meta) = metadata.get(hash) {
                return Ok(Some(meta.clone()));
            }
        }

        // Try persistent store
        if let Some(store) = &self.store {
            let key = format!("metadata:{}", hex::encode(hash));
            if let Some(bytes) = store
                .get(key.as_bytes())
                .map_err(|e| RegistryError::StorageError(e.to_string()))?
            {
                let meta: ContractMetadata = icn_encoding::decode_versioned(&bytes)
                    .map_err(|e| RegistryError::SerializationError(e.to_string()))?;

                // Populate cache
                let mut metadata = self.metadata.write().await;
                metadata.insert(*hash, meta.clone());

                return Ok(Some(meta));
            }
        }

        Ok(None)
    }

    /// Resolve contract by name and optional version
    ///
    /// If version is None, returns the latest version.
    pub async fn resolve(&self, name: &str, version: Option<u32>) -> Result<Option<ContentHash>> {
        let name_index = self.name_index.read().await;

        if let Some(versions) = name_index.get(name) {
            match version {
                Some(v) => {
                    // Find specific version
                    Ok(versions.iter().find(|(ver, _)| *ver == v).map(|(_, h)| *h))
                }
                None => {
                    // Find latest version
                    Ok(versions.iter().max_by_key(|(v, _)| *v).map(|(_, h)| *h))
                }
            }
        } else {
            Ok(None)
        }
    }

    /// List all contracts
    pub async fn list_all(&self) -> Result<Vec<ContractMetadata>> {
        let metadata = self.metadata.read().await;
        Ok(metadata.values().cloned().collect())
    }

    /// List contracts by owner
    pub async fn list_by_owner(&self, owner: &str) -> Result<Vec<ContractMetadata>> {
        let owner_index = self.owner_index.read().await;
        let metadata = self.metadata.read().await;

        if let Some(hashes) = owner_index.get(owner) {
            Ok(hashes
                .iter()
                .filter_map(|h| metadata.get(h).cloned())
                .collect())
        } else {
            Ok(vec![])
        }
    }

    /// Get all versions of a contract by name
    pub async fn get_versions(&self, name: &str) -> Result<Vec<(u32, ContentHash)>> {
        let name_index = self.name_index.read().await;
        Ok(name_index.get(name).cloned().unwrap_or_default())
    }

    /// Check if a contract exists
    pub async fn exists(&self, hash: &ContentHash) -> bool {
        let contracts = self.contracts.read().await;
        if contracts.contains_key(hash) {
            return true;
        }

        if let Some(store) = &self.store {
            let key = format!("contract:{}", hex::encode(hash));
            store.get(key.as_bytes()).ok().flatten().is_some()
        } else {
            false
        }
    }

    /// Get registry statistics
    pub async fn stats(&self) -> RegistryStats {
        let contracts = self.contracts.read().await;
        let name_index = self.name_index.read().await;
        let owner_index = self.owner_index.read().await;

        RegistryStats {
            total_contracts: contracts.len(),
            unique_names: name_index.len(),
            unique_owners: owner_index.len(),
        }
    }

    /// Load contracts from persistent store into cache
    pub async fn load_from_store(&self) -> Result<usize> {
        let Some(store) = &self.store else {
            return Ok(0);
        };

        let mut loaded = 0;

        // Scan for contract keys using the Store trait's scan method
        if let Ok(entries) = store.scan(b"contract:") {
            for (key, value) in entries {
                let key_str = String::from_utf8_lossy(&key);
                if let Some(hash_hex) = key_str.strip_prefix("contract:") {
                    if let Ok(hash_bytes) = hex::decode(hash_hex) {
                        if hash_bytes.len() == 32 {
                            let mut hash = [0u8; 32];
                            hash.copy_from_slice(&hash_bytes);

                            if let Ok(contract) =
                                icn_encoding::decode_versioned::<Contract>(&value)
                            {
                                let mut contracts = self.contracts.write().await;
                                contracts.insert(hash, contract.clone());

                                // Also load metadata
                                let meta_key = format!("metadata:{hash_hex}");
                                if let Ok(Some(meta_bytes)) = store.get(meta_key.as_bytes()) {
                                    if let Ok(meta) =
                                        icn_encoding::decode_versioned::<ContractMetadata>(
                                            &meta_bytes,
                                        )
                                    {
                                        let mut metadata = self.metadata.write().await;
                                        metadata.insert(hash, meta.clone());

                                        // Rebuild indexes
                                        let mut name_index = self.name_index.write().await;
                                        name_index
                                            .entry(meta.name.clone())
                                            .or_default()
                                            .push((meta.version, hash));

                                        let mut owner_index = self.owner_index.write().await;
                                        owner_index
                                            .entry(meta.owner.clone())
                                            .or_default()
                                            .push(hash);
                                    }
                                }

                                loaded += 1;
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(loaded = loaded, "Loaded contracts from persistent store");
        Ok(loaded)
    }
}

impl Default for ContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_contracts: usize,
    pub unique_names: usize,
    pub unique_owners: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Expr, Rule, Stmt, Value};

    fn create_test_contract(name: &str) -> Contract {
        Contract::new(name.to_string())
            .add_participant(icn_identity::KeyPair::generate().unwrap().did().clone())
            .add_rule(Rule::new("test_rule".to_string()).add_stmt(Stmt::Return {
                value: Expr::Literal(Value::String("ok".to_string())),
            }))
    }

    #[tokio::test]
    async fn test_deploy_and_get() {
        let registry = ContractRegistry::new();
        let contract = create_test_contract("TestContract");

        let hash = registry
            .deploy(contract.clone(), "did:icn:owner", None)
            .await
            .unwrap();

        let retrieved = registry.get(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "TestContract");
    }

    #[tokio::test]
    async fn test_resolve_by_name() {
        let registry = ContractRegistry::new();
        let contract = create_test_contract("NamedContract");

        let hash = registry
            .deploy(contract.clone(), "did:icn:owner", Some(1))
            .await
            .unwrap();

        // Resolve by name
        let resolved = registry
            .resolve("NamedContract", None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved, hash);

        // Resolve specific version
        let resolved_v1 = registry
            .resolve("NamedContract", Some(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved_v1, hash);

        // Non-existent version
        let no_v2 = registry.resolve("NamedContract", Some(2)).await.unwrap();
        assert!(no_v2.is_none());
    }

    #[tokio::test]
    async fn test_version_auto_increment() {
        let registry = ContractRegistry::new();

        // Deploy v1
        let contract1 = create_test_contract("Versioned");
        let hash1 = registry
            .deploy(contract1, "did:icn:owner", None)
            .await
            .unwrap();
        let meta1 = registry.get_metadata(&hash1).await.unwrap().unwrap();
        assert_eq!(meta1.version, 1);

        // Deploy v2 (same name, different content)
        let contract2 = Contract::new("Versioned".to_string())
            .add_participant(icn_identity::KeyPair::generate().unwrap().did().clone())
            .add_rule(
                Rule::new("different_rule".to_string()).add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Int(42)),
                }),
            );
        let hash2 = registry
            .deploy(contract2, "did:icn:owner", None)
            .await
            .unwrap();
        let meta2 = registry.get_metadata(&hash2).await.unwrap().unwrap();
        assert_eq!(meta2.version, 2);
    }

    #[tokio::test]
    async fn test_list_by_owner() {
        let registry = ContractRegistry::new();

        // Deploy from owner1
        let contract1 = create_test_contract("Owner1Contract");
        registry
            .deploy(contract1, "did:icn:owner1", None)
            .await
            .unwrap();

        // Deploy from owner2
        let contract2 = create_test_contract("Owner2Contract");
        registry
            .deploy(contract2, "did:icn:owner2", None)
            .await
            .unwrap();

        // List owner1's contracts
        let owner1_contracts = registry.list_by_owner("did:icn:owner1").await.unwrap();
        assert_eq!(owner1_contracts.len(), 1);
        assert_eq!(owner1_contracts[0].name, "Owner1Contract");
    }

    #[tokio::test]
    async fn test_duplicate_rejected() {
        let registry = ContractRegistry::new();
        let contract = create_test_contract("DuplicateTest");

        // First deploy succeeds
        registry
            .deploy(contract.clone(), "did:icn:owner", None)
            .await
            .unwrap();

        // Second deploy of same contract fails
        let result = registry.deploy(contract, "did:icn:owner", None).await;
        assert!(matches!(result, Err(RegistryError::AlreadyExists(_))));
    }
}
