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

/// Semantic version following semver.org specification
///
/// Format: MAJOR.MINOR.PATCH
/// - MAJOR: Incompatible API changes
/// - MINOR: Backwards-compatible functionality additions
/// - PATCH: Backwards-compatible bug fixes
///
/// Note: The default value (0.0.0) represents an "unset" version,
/// typically from legacy metadata before semantic versioning was added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemanticVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Default for SemanticVersion {
    /// Returns 0.0.0, which represents an unset/unknown version
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

impl SemanticVersion {
    /// Create a new semantic version
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Check if this version is unset (0.0.0)
    ///
    /// Returns true for the default value, which represents legacy metadata
    /// or explicitly unset versions.
    pub const fn is_unset(&self) -> bool {
        self.major == 0 && self.minor == 0 && self.patch == 0
    }

    /// Parse a semantic version from a string (e.g., "1.2.3")
    pub fn parse(s: &str) -> std::result::Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!(
                "Invalid semantic version format: expected 'major.minor.patch', got '{s}'"
            ));
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| format!("Invalid major version: {}", parts[0]))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|_| format!("Invalid minor version: {}", parts[1]))?;
        let patch = parts[2]
            .parse::<u32>()
            .map_err(|_| format!("Invalid patch version: {}", parts[2]))?;

        Ok(Self::new(major, minor, patch))
    }

    /// Check if this version is compatible with another version
    ///
    /// Compatible if:
    /// - Same major version (for major > 0)
    /// - This version is greater than or equal to the other
    /// - For major == 0, minor versions must match (pre-1.0 breaking changes)
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        if self.major == 0 || other.major == 0 {
            // Pre-1.0: minor version must match
            self.major == other.major && self.minor == other.minor && self >= other
        } else {
            // Post-1.0: major version must match
            self.major == other.major && self >= other
        }
    }
}

impl std::fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for SemanticVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemanticVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => match self.minor.cmp(&other.minor) {
                std::cmp::Ordering::Equal => self.patch.cmp(&other.patch),
                other_ord => other_ord,
            },
            other_ord => other_ord,
        }
    }
}

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

    #[error("Invalid version format: {0}")]
    InvalidVersion(String),

    #[error("Version incompatibility: {0}")]
    IncompatibleVersion(String),
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
    /// Version number (incrementing, for backwards compatibility)
    pub version: u32,
    /// Semantic version (e.g., 1.2.3)
    /// Defaults to `0.{version}.0` for pre-1.0 development if not explicitly set
    #[serde(default)]
    pub semantic_version: SemanticVersion,
    /// Minimum interpreter version required to execute this contract
    /// Note: Metadata-only in this version. Not enforced at runtime yet.
    #[serde(default)]
    pub min_interpreter_version: Option<SemanticVersion>,
    /// List of versions this contract is explicitly compatible with
    /// Note: Metadata-only in this version. Use `set_semantic_version()` to control
    /// compatibility via standard semver rules instead.
    #[serde(default)]
    pub compatible_versions: Vec<SemanticVersion>,
    /// Whether this version is deprecated
    #[serde(default)]
    pub deprecated: bool,
    /// Reason for deprecation (if deprecated)
    #[serde(default)]
    pub deprecated_reason: Option<String>,
    /// Successor version to upgrade to (if deprecated)
    #[serde(default)]
    pub successor_version: Option<SemanticVersion>,
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
    /// 
    /// Note: Semantic version defaults to `0.{version}.0` (pre-1.0 development convention).
    /// Use `with_semantic_version()` or `set_semantic_version()` to set an explicit semver.
    pub fn from_contract(contract: &Contract, owner: &str, version: u32) -> Result<Self> {
        let code_hash = compute_hash(contract)?;
        // Default semantic version to 0.<version>.0 for pre-1.0 development
        // Per semver: 0.x versions are incompatible with each other (0.1.0 -> 0.2.0 is a breaking change)
        let semantic_version = SemanticVersion::new(0, version, 0);
        
        Ok(ContractMetadata {
            code_hash,
            name: contract.name.clone(),
            version,
            semantic_version,
            min_interpreter_version: None,
            compatible_versions: vec![],
            deprecated: false,
            deprecated_reason: None,
            successor_version: None,
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

    /// Set semantic version explicitly
    pub fn with_semantic_version(mut self, version: SemanticVersion) -> Self {
        self.semantic_version = version;
        self
    }

    /// Set minimum interpreter version
    pub fn with_min_interpreter_version(mut self, version: SemanticVersion) -> Self {
        self.min_interpreter_version = Some(version);
        self
    }

    /// Add a compatible version
    pub fn add_compatible_version(mut self, version: SemanticVersion) -> Self {
        if !self.compatible_versions.contains(&version) {
            self.compatible_versions.push(version);
        }
        self
    }

    /// Mark as deprecated with reason and optional successor
    pub fn deprecate(
        mut self,
        reason: impl Into<String>,
        successor: Option<SemanticVersion>,
    ) -> Self {
        self.deprecated = true;
        self.deprecated_reason = Some(reason.into());
        self.successor_version = successor;
        self
    }

    /// Check if this contract version is compatible with another version
    pub fn is_compatible_with(&self, other_version: &SemanticVersion) -> bool {
        // Self is compatible if the semantic version is compatible
        // or if the other version is in the explicit compatible list
        self.semantic_version.is_compatible_with(other_version)
            || self.compatible_versions.contains(other_version)
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

    /// Get full metadata for all versions of a contract by name
    /// 
    /// Store-aware: loads metadata from persistent storage on cache miss.
    pub async fn list_versions_metadata(&self, name: &str) -> Result<Vec<ContractMetadata>> {
        // Clone hash list to avoid holding lock across await
        let hashes: Vec<ContentHash> = {
            let name_index = self.name_index.read().await;
            name_index
                .get(name)
                .map(|versions| versions.iter().map(|(_, h)| *h).collect())
                .unwrap_or_default()
        };

        let mut result = Vec::new();
        for hash in hashes {
            // Use get_metadata which is store-aware (no lock held here)
            if let Some(meta) = self.get_metadata(&hash).await? {
                result.push(meta);
            }
        }
        Ok(result)
    }

    /// Resolve contract by name and semantic version
    ///
    /// Returns the contract hash if found.
    /// Store-aware: loads metadata from persistent storage on cache miss.
    pub async fn get_by_semantic_version(
        &self,
        name: &str,
        version: &SemanticVersion,
    ) -> Result<Option<ContentHash>> {
        // Clone hash list to avoid holding lock across await
        let hashes: Vec<ContentHash> = {
            let name_index = self.name_index.read().await;
            name_index
                .get(name)
                .map(|versions| versions.iter().map(|(_, h)| *h).collect())
                .unwrap_or_default()
        };

        for hash in hashes {
            // Use get_metadata which is store-aware (no lock held here)
            if let Some(meta) = self.get_metadata(&hash).await? {
                if meta.semantic_version == *version {
                    return Ok(Some(hash));
                }
            }
        }

        Ok(None)
    }

    /// Mark a specific version as deprecated
    ///
    /// Updates both in-memory cache and persistent storage if available.
    pub async fn mark_deprecated(
        &self,
        hash: &ContentHash,
        reason: impl Into<String>,
        successor: Option<SemanticVersion>,
    ) -> Result<()> {
        let reason = reason.into();
        let hash_hex = hex::encode(hash);

        // Update in-memory metadata
        {
            let mut metadata = self.metadata.write().await;
            let meta = metadata
                .get_mut(hash)
                .ok_or_else(|| RegistryError::NotFound(hash_hex.clone()))?;

            meta.deprecated = true;
            meta.deprecated_reason = Some(reason.clone());
            meta.successor_version = successor;

            // Update persistent store if available
            if let Some(store) = &self.store {
                let metadata_key = format!("metadata:{hash_hex}");
                let metadata_bytes = icn_encoding::encode_versioned(meta)
                    .map_err(|e| RegistryError::SerializationError(e.to_string()))?;
                store
                    .put(metadata_key.as_bytes(), &metadata_bytes)
                    .map_err(|e| RegistryError::StorageError(e.to_string()))?;
            }
        }

        tracing::info!(
            hash = %hash_hex,
            reason = %reason,
            successor = ?successor,
            "Contract version marked as deprecated"
        );

        Ok(())
    }

    /// Update the semantic version for a deployed contract
    ///
    /// Updates both in-memory cache and persistent storage if available.
    /// This allows correcting the semantic version after deployment.
    pub async fn set_semantic_version(
        &self,
        hash: &ContentHash,
        semantic_version: SemanticVersion,
    ) -> Result<()> {
        let hash_hex = hex::encode(hash);

        // Update in-memory metadata
        {
            let mut metadata = self.metadata.write().await;
            let meta = metadata
                .get_mut(hash)
                .ok_or_else(|| RegistryError::NotFound(hash_hex.clone()))?;

            meta.semantic_version = semantic_version;

            // Update persistent store if available
            if let Some(store) = &self.store {
                let metadata_key = format!("metadata:{hash_hex}");
                let metadata_bytes = icn_encoding::encode_versioned(meta)
                    .map_err(|e| RegistryError::SerializationError(e.to_string()))?;
                store
                    .put(metadata_key.as_bytes(), &metadata_bytes)
                    .map_err(|e| RegistryError::StorageError(e.to_string()))?;
            }
        }

        tracing::info!(
            hash = %hash_hex,
            semver = %semantic_version,
            "Contract semantic version updated"
        );

        Ok(())
    }

    /// Get the upgrade path from one version to another
    ///
    /// Returns a list of versions to upgrade through, or None if no path exists.
    /// 
    /// An upgrade is considered valid if:
    /// - Neither version is unset (0.0.0)
    /// - The target version can be used in place of the source version (backwards compatible)
    /// - For major >= 1.0: same major version and target >= source
    /// - For major == 0: same major and minor versions and target >= source
    ///
    /// Note: Unset versions (0.0.0) from legacy metadata are not eligible for
    /// automatic upgrades. Use `set_semantic_version()` to assign explicit versions first.
    ///
    /// Currently returns direct upgrade if compatible, or None.
    /// Future: Could implement multi-step migration paths.
    pub async fn get_upgrade_path(
        &self,
        name: &str,
        from_version: &SemanticVersion,
        to_version: &SemanticVersion,
    ) -> Result<Option<Vec<SemanticVersion>>> {
        // Reject unset versions
        if from_version.is_unset() || to_version.is_unset() {
            return Ok(None);
        }

        let metadata = self.metadata.read().await;
        let name_index = self.name_index.read().await;

        // Get all versions for this contract
        let Some(versions) = name_index.get(name) else {
            return Ok(None);
        };

        // Check if both versions exist
        let mut from_meta = None;
        let mut to_meta = None;

        for (_, hash) in versions {
            if let Some(meta) = metadata.get(hash) {
                if meta.semantic_version == *from_version {
                    from_meta = Some(meta.clone());
                }
                if meta.semantic_version == *to_version {
                    to_meta = Some(meta.clone());
                }
            }
        }

        let Some(_from) = from_meta else {
            return Ok(None);
        };
        let Some(to) = to_meta else {
            return Ok(None);
        };

        // Check if target version is backwards-compatible with source version
        // This means: can we use to_version in place of from_version?
        // For valid upgrade: same major version family AND to_version >= from_version
        if to.is_compatible_with(from_version) {
            Ok(Some(vec![*from_version, *to_version]))
        } else {
            // No upgrade path (incompatible versions)
            Ok(None)
        }
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

                            if let Ok(contract) = icn_encoding::decode_versioned::<Contract>(&value)
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

    // ========================================================================
    // SemanticVersion Tests
    // ========================================================================

    #[test]
    fn test_semantic_version_parsing() {
        let v = SemanticVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);

        let v_display = format!("{v}");
        assert_eq!(v_display, "1.2.3");
    }

    #[test]
    fn test_semantic_version_parsing_errors() {
        assert!(SemanticVersion::parse("1.2").is_err());
        assert!(SemanticVersion::parse("1.2.3.4").is_err());
        assert!(SemanticVersion::parse("a.b.c").is_err());
        assert!(SemanticVersion::parse("1.2.x").is_err());
    }

    #[test]
    fn test_semantic_version_comparison() {
        let v1_0_0 = SemanticVersion::new(1, 0, 0);
        let v1_0_1 = SemanticVersion::new(1, 0, 1);
        let v1_1_0 = SemanticVersion::new(1, 1, 0);
        let v2_0_0 = SemanticVersion::new(2, 0, 0);

        assert!(v1_0_0 < v1_0_1);
        assert!(v1_0_1 < v1_1_0);
        assert!(v1_1_0 < v2_0_0);
        assert!(v2_0_0 > v1_1_0);
        assert_eq!(v1_0_0, v1_0_0);
    }

    #[test]
    fn test_semantic_version_compatibility() {
        let v1_0_0 = SemanticVersion::new(1, 0, 0);
        let v1_0_1 = SemanticVersion::new(1, 0, 1);
        let v1_1_0 = SemanticVersion::new(1, 1, 0);
        let v2_0_0 = SemanticVersion::new(2, 0, 0);

        // Same major version (>= 1.0.0) is compatible
        assert!(v1_1_0.is_compatible_with(&v1_0_0));
        assert!(v1_0_1.is_compatible_with(&v1_0_0));
        assert!(!v1_0_0.is_compatible_with(&v1_0_1)); // Older not compatible with newer

        // Different major version is not compatible
        assert!(!v2_0_0.is_compatible_with(&v1_0_0));
        assert!(!v1_0_0.is_compatible_with(&v2_0_0));
    }

    #[test]
    fn test_semantic_version_compatibility_pre_1_0() {
        let v0_1_0 = SemanticVersion::new(0, 1, 0);
        let v0_1_1 = SemanticVersion::new(0, 1, 1);
        let v0_2_0 = SemanticVersion::new(0, 2, 0);

        // Pre-1.0: minor version must match
        assert!(v0_1_1.is_compatible_with(&v0_1_0));
        assert!(!v0_1_0.is_compatible_with(&v0_1_1));
        assert!(!v0_2_0.is_compatible_with(&v0_1_0));
        assert!(!v0_1_0.is_compatible_with(&v0_2_0));
    }

    // ========================================================================
    // Version Management Tests
    // ========================================================================

    #[tokio::test]
    async fn test_semantic_version_in_metadata() {
        let registry = ContractRegistry::new();
        let contract = create_test_contract("SemanticTest");

        let hash = registry
            .deploy(contract.clone(), "did:icn:owner", Some(1))
            .await
            .unwrap();

        let meta = registry.get_metadata(&hash).await.unwrap().unwrap();
        // Version 1 maps to 0.1.0 (pre-1.0 development)
        assert_eq!(meta.semantic_version, SemanticVersion::new(0, 1, 0));
    }

    #[tokio::test]
    async fn test_set_semantic_version() {
        let registry = ContractRegistry::new();
        let contract = create_test_contract("CustomVersionTest");

        let hash = registry
            .deploy(contract.clone(), "did:icn:owner", Some(1))
            .await
            .unwrap();

        // Set a custom semantic version using the public API
        let custom_version = SemanticVersion::new(2, 5, 3);
        registry
            .set_semantic_version(&hash, custom_version)
            .await
            .unwrap();

        // Verify it was updated
        let meta = registry.get_metadata(&hash).await.unwrap().unwrap();
        assert_eq!(meta.semantic_version, custom_version);
        assert_eq!(meta.version, 1); // Numeric version unchanged
    }

    #[tokio::test]
    async fn test_list_versions_metadata() {
        let registry = ContractRegistry::new();

        // Deploy multiple versions
        let contract1 = create_test_contract("MultiVersion");
        registry
            .deploy(contract1, "did:icn:owner", Some(1))
            .await
            .unwrap();

        let contract2 = Contract::new("MultiVersion".to_string())
            .add_participant(icn_identity::KeyPair::generate().unwrap().did().clone())
            .add_rule(
                Rule::new("different_rule".to_string()).add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Int(42)),
                }),
            );
        registry
            .deploy(contract2, "did:icn:owner", Some(2))
            .await
            .unwrap();

        // List all versions with metadata
        let versions = registry.list_versions_metadata("MultiVersion").await.unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 1);
        assert_eq!(versions[1].version, 2);
    }

    #[tokio::test]
    async fn test_get_by_semantic_version() {
        let registry = ContractRegistry::new();

        // Deploy with version 1 (will be 0.1.0 per pre-1.0 convention)
        let contract = create_test_contract("BySemanticVersion");
        let hash = registry
            .deploy(contract.clone(), "did:icn:owner", Some(1))
            .await
            .unwrap();

        // Resolve by semantic version
        let v0_1_0 = SemanticVersion::new(0, 1, 0);
        let resolved = registry
            .get_by_semantic_version("BySemanticVersion", &v0_1_0)
            .await
            .unwrap();

        assert_eq!(resolved, Some(hash));

        // Non-existent semantic version
        let v0_2_0 = SemanticVersion::new(0, 2, 0);
        let not_found = registry
            .get_by_semantic_version("BySemanticVersion", &v0_2_0)
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_mark_deprecated() {
        let registry = ContractRegistry::new();

        // Deploy a contract
        let contract = create_test_contract("DeprecatedTest");
        let hash = registry
            .deploy(contract.clone(), "did:icn:owner", Some(1))
            .await
            .unwrap();

        // Mark as deprecated
        let successor = SemanticVersion::new(2, 0, 0);
        registry
            .mark_deprecated(&hash, "Security vulnerability fixed in v2", Some(successor))
            .await
            .unwrap();

        // Verify deprecation
        let meta = registry.get_metadata(&hash).await.unwrap().unwrap();
        assert!(meta.deprecated);
        assert_eq!(
            meta.deprecated_reason,
            Some("Security vulnerability fixed in v2".to_string())
        );
        assert_eq!(meta.successor_version, Some(successor));
    }

    #[tokio::test]
    async fn test_mark_deprecated_not_found() {
        let registry = ContractRegistry::new();

        let non_existent_hash = [0u8; 32];
        let result = registry
            .mark_deprecated(&non_existent_hash, "Test", None)
            .await;

        assert!(matches!(result, Err(RegistryError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_get_upgrade_path_compatible() {
        let registry = ContractRegistry::new();

        // Deploy v1.0.0 explicitly
        let contract1 = create_test_contract("UpgradePath");
        let hash1 = registry
            .deploy(contract1, "did:icn:owner", Some(1))
            .await
            .unwrap();

        // Set explicit semantic version 1.0.0 using public API
        registry
            .set_semantic_version(&hash1, SemanticVersion::new(1, 0, 0))
            .await
            .unwrap();

        // Deploy v1.1.0 (compatible - same major version)
        let contract2 = Contract::new("UpgradePath".to_string())
            .add_participant(icn_identity::KeyPair::generate().unwrap().did().clone())
            .add_rule(
                Rule::new("new_rule".to_string()).add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::Int(99)),
                }),
            );
        let hash2 = registry
            .deploy(contract2, "did:icn:owner", Some(2))
            .await
            .unwrap();

        // Set explicit semantic version 1.1.0 using public API
        registry
            .set_semantic_version(&hash2, SemanticVersion::new(1, 1, 0))
            .await
            .unwrap();

        let v1_0_0 = SemanticVersion::new(1, 0, 0);
        let v1_1_0 = SemanticVersion::new(1, 1, 0);

        let path = registry
            .get_upgrade_path("UpgradePath", &v1_0_0, &v1_1_0)
            .await
            .unwrap();

        assert_eq!(path, Some(vec![v1_0_0, v1_1_0]));
    }

    #[tokio::test]
    async fn test_get_upgrade_path_incompatible() {
        let registry = ContractRegistry::new();

        // Deploy v1.0.0
        let contract1 = create_test_contract("IncompatibleUpgrade");
        registry
            .deploy(contract1, "did:icn:owner", Some(1))
            .await
            .unwrap();

        // Deploy v2.0.0 (incompatible - different major version)
        let contract2 = Contract::new("IncompatibleUpgrade".to_string())
            .add_participant(icn_identity::KeyPair::generate().unwrap().did().clone())
            .add_rule(
                Rule::new("breaking_change".to_string()).add_stmt(Stmt::Return {
                    value: Expr::Literal(Value::String("new".to_string())),
                }),
            );
        registry
            .deploy(contract2, "did:icn:owner", Some(2))
            .await
            .unwrap();

        let v1_0_0 = SemanticVersion::new(1, 0, 0);
        let v2_0_0 = SemanticVersion::new(2, 0, 0);

        let path = registry
            .get_upgrade_path("IncompatibleUpgrade", &v1_0_0, &v2_0_0)
            .await
            .unwrap();

        // No upgrade path for incompatible major versions
        assert!(path.is_none());
    }

    #[tokio::test]
    async fn test_contract_metadata_compatibility_methods() {
        let contract = create_test_contract("CompatTest");
        let mut meta = ContractMetadata::from_contract(&contract, "did:icn:owner", 1).unwrap();

        // Set semantic version to 1.2.0
        meta.semantic_version = SemanticVersion::new(1, 2, 0);

        // Add explicit compatible versions
        meta = meta.add_compatible_version(SemanticVersion::new(1, 0, 0));
        meta = meta.add_compatible_version(SemanticVersion::new(1, 1, 0));

        // Test compatibility
        assert!(meta.is_compatible_with(&SemanticVersion::new(1, 0, 0))); // Explicit
        assert!(meta.is_compatible_with(&SemanticVersion::new(1, 1, 0))); // Explicit
        assert!(!meta.is_compatible_with(&SemanticVersion::new(2, 0, 0))); // Different major
    }

    #[tokio::test]
    async fn test_contract_metadata_builder_methods() {
        let contract = create_test_contract("BuilderTest");
        let meta = ContractMetadata::from_contract(&contract, "did:icn:owner", 1)
            .unwrap()
            .with_description("A test contract")
            .with_semantic_version(SemanticVersion::new(2, 5, 3))
            .with_min_interpreter_version(SemanticVersion::new(1, 0, 0))
            .add_compatible_version(SemanticVersion::new(2, 5, 0))
            .add_compatible_version(SemanticVersion::new(2, 4, 0))
            .deprecate("Replaced by v3", Some(SemanticVersion::new(3, 0, 0)));

        assert_eq!(meta.description, Some("A test contract".to_string()));
        assert_eq!(meta.semantic_version, SemanticVersion::new(2, 5, 3));
        assert_eq!(
            meta.min_interpreter_version,
            Some(SemanticVersion::new(1, 0, 0))
        );
        assert_eq!(meta.compatible_versions.len(), 2);
        assert!(meta.deprecated);
        assert_eq!(meta.deprecated_reason, Some("Replaced by v3".to_string()));
        assert_eq!(
            meta.successor_version,
            Some(SemanticVersion::new(3, 0, 0))
        );
    }

    #[test]
    fn test_backwards_compatible_deserialization() {
        // Test that old metadata (without new fields) can still deserialize
        // This simulates stored metadata from before the version management feature
        //
        // Note: This tests serde schema evolution (JSON deserialization with missing fields).
        // The persistent store uses icn_encoding::encode_versioned/decode_versioned which
        // should handle the same schema evolution via serde, but wire format testing would
        // require encoding an old struct snapshot.
        let old_metadata_json = r#"{
            "code_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
            "name": "OldContract",
            "version": 1,
            "owner": "did:icn:test",
            "deployed_at": 1234567890,
            "description": null,
            "participants": [],
            "currency": null,
            "rules": [],
            "visibility": "Private"
        }"#;

        let meta: ContractMetadata = serde_json::from_str(old_metadata_json)
            .expect("Failed to deserialize old metadata");

        // Verify defaults were applied (0.0.0 represents "unset" for legacy metadata)
        assert_eq!(meta.semantic_version, SemanticVersion::default());
        assert!(meta.semantic_version.is_unset());
        assert_eq!(meta.min_interpreter_version, None);
        assert_eq!(meta.compatible_versions.len(), 0);
        assert!(!meta.deprecated);
        assert_eq!(meta.deprecated_reason, None);
        assert_eq!(meta.successor_version, None);

        // Verify original fields are intact
        assert_eq!(meta.name, "OldContract");
        assert_eq!(meta.version, 1);
    }

    #[tokio::test]
    async fn test_upgrade_path_rejects_unset_versions() {
        let registry = ContractRegistry::new();

        // Deploy a contract
        let contract = create_test_contract("UnsetTest");
        let hash = registry
            .deploy(contract, "did:icn:owner", Some(1))
            .await
            .unwrap();

        // Set explicit semantic version
        registry
            .set_semantic_version(&hash, SemanticVersion::new(1, 0, 0))
            .await
            .unwrap();

        let v1_0_0 = SemanticVersion::new(1, 0, 0);
        let v0_0_0 = SemanticVersion::new(0, 0, 0); // Unset

        // Upgrade from unset should be rejected
        let path = registry
            .get_upgrade_path("UnsetTest", &v0_0_0, &v1_0_0)
            .await
            .unwrap();
        assert!(path.is_none());

        // Upgrade to unset should be rejected
        let path = registry
            .get_upgrade_path("UnsetTest", &v1_0_0, &v0_0_0)
            .await
            .unwrap();
        assert!(path.is_none());
    }
}
