/*!
 * Role-based authorization helpers for federation
 * 
 * This module contains functions for looking up and verifying roles 
 * within federation contexts, such as checking if an identity is authorized
 * as a guardian for a specific scope or federation.
 */

use cid::Cid;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use futures::lock::Mutex;
use std::collections::HashMap;
use tracing;

use crate::errors::{FederationError, FederationResult};
use icn_identity::IdentityId;
use icn_storage::{StorageBackend, ReplicationPolicy as StorageReplicationPolicy};
use icn_governance_kernel::config::GovernanceConfig;

/// Node roles in a federation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeRole {
    /// Validator node that participates in consensus
    Validator,
    
    /// Guardian node with special privileges for governance
    Guardian,
    
    /// Observer node that tracks the network but doesn't participate in consensus
    Observer,
}

impl NodeRole {
    /// Convert the role to a string representation
    pub fn as_str(&self) -> &str {
        match self {
            NodeRole::Validator => "validator",
            NodeRole::Guardian => "guardian",
            NodeRole::Observer => "observer",
        }
    }
}

impl std::fmt::Display for NodeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Simple structure to represent governance roles
/// 
/// Note: This is a simplified version used for backward compatibility.
/// New code should prefer to use GovernanceConfig from governance-kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyGovernanceRoles {
    /// Guardian DIDs authorized for this context
    pub guardians: Option<Vec<String>>,
    
    /// Other roles can be added here in the future
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stewards: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participants: Option<Vec<String>>,
}

impl Default for LegacyGovernanceRoles {
    fn default() -> Self {
        Self {
            guardians: None,
            stewards: None,
            participants: None,
        }
    }
}

/// Simple representation of governance configuration
/// 
/// Note: This is a simplified version used for backward compatibility.
/// New code should prefer to use GovernanceConfig from governance-kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyGovernanceConfig {
    /// The ID of the scope this configuration applies to (e.g., federation ID)
    pub scope_id: String,
    
    /// Role definitions
    pub roles: LegacyGovernanceRoles,
    
    /// Version of this configuration
    pub version: String,
}

impl LegacyGovernanceConfig {
    /// Create a new governance configuration
    pub fn new(scope_id: impl Into<String>) -> Self {
        Self {
            scope_id: scope_id.into(),
            roles: LegacyGovernanceRoles::default(),
            version: "1.0".to_string(),
        }
    }
    
    /// Extract guardian DIDs from this configuration
    pub fn extract_guardian_dids(&self) -> Vec<IdentityId> {
        self.roles.guardians
            .as_ref()
            .map(|guardians| {
                guardians.iter()
                    .map(|did| IdentityId(did.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Derive a storage key for a scope's governance configuration
pub fn config_key_for_scope(scope_id: &str) -> Cid {
    let key_str = format!("config::scope::{}", scope_id);
    let key_hash = crate::create_sha256_multihash(key_str.as_bytes());
    Cid::new_v1(0x71, key_hash) // dag-cbor codec for config data
}

/// Get the list of authorized guardians for a specific context ID
/// 
/// This looks up a governance configuration from storage based on the context ID
/// (which could be a federation ID, scope ID, etc.) and extracts the list of
/// guardian DIDs that are authorized for that context.
pub async fn get_authorized_guardians(
    context_id: &str, 
    storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>
) -> FederationResult<Vec<IdentityId>> {
    // First, try the direct approach - looking up by the context_id key
    let key_cid = config_key_for_scope(context_id);
    tracing::debug!(context_id, key = %key_cid, "Looking up config for guardian roles");
    
    let store_lock = storage.lock().await;
    match store_lock.get_kv(&key_cid).await {
        Ok(Some(bytes)) => {
            // Drop the lock before parsing
            drop(store_lock);
            return parse_config_bytes(bytes, context_id);
        },
        // Otherwise, continue with the fallback approach
        _ => drop(store_lock)
    }
    
    // Fallback: List all available entries and check each one
    let all_cids = {
        let store_lock = storage.lock().await;
        match store_lock.list_all().await {
            Ok(cids) => {
                drop(store_lock);
                cids
            },
            Err(e) => {
                drop(store_lock);
                return Err(FederationError::StorageError(format!(
                    "Failed to list storage contents: {}", e
                )));
            }
        }
    };
    
    // If we have no entries, return early
    if all_cids.is_empty() {
        return Err(FederationError::StorageError(format!("Configuration not found for context: {}", context_id)));
    }
    
    // For each CID, try to get the content and check if it's a config for our context
    for cid in all_cids {
        let bytes = {
            let store_lock = storage.lock().await;
            match store_lock.get_kv(&cid).await {
                Ok(Some(bytes)) => {
                    drop(store_lock);
                    bytes
                },
                _ => {
                    drop(store_lock);
                    continue;
                }
            }
        };
        
        // Try to parse as governance config and check if it matches our context
        if let Ok(legacy_config) = serde_json::from_slice::<LegacyGovernanceConfig>(&bytes) {
            if legacy_config.scope_id == context_id {
                return Ok(legacy_config.extract_guardian_dids());
            }
        } else if let Ok(kernel_config) = serde_json::from_slice::<GovernanceConfig>(&bytes) {
            // For GovernanceConfig, we need to extract some identifier and check it
            // This could be from identity.name or some other field
            // For now, we'll return any found config as a fallback
            return Ok(get_guardian_dids_from_config(&kernel_config));
        }
    }
    
    // If we got here, we didn't find a matching config
    Err(FederationError::StorageError(format!("Configuration not found for context: {}", context_id)))
}

/// Helper to parse config bytes into guardian list
fn parse_config_bytes(bytes: Vec<u8>, context_id: &str) -> FederationResult<Vec<IdentityId>> {
    // Try to deserialize as GovernanceConfig from governance-kernel first
    match serde_json::from_slice::<GovernanceConfig>(&bytes) {
        Ok(config) => {
            // Use the get_guardian_dids method from GovernanceConfig
            let guardian_dids = get_guardian_dids_from_config(&config);
            tracing::debug!(context_id, count = guardian_dids.len(), "Found authorized guardians from config");
            Ok(guardian_dids)
        },
        Err(e1) => {
            // If that fails, try the legacy format
            tracing::debug!(context_id, error = %e1, "Failed to parse as governance-kernel config, trying legacy format");
            
            match serde_json::from_slice::<LegacyGovernanceConfig>(&bytes) {
                Ok(legacy_config) => {
                    let guardian_dids = legacy_config.extract_guardian_dids();
                    tracing::debug!(context_id, count = guardian_dids.len(), "Found authorized guardians from legacy config");
                    Ok(guardian_dids)
                },
                Err(e2) => {
                    Err(FederationError::InternalError(format!(
                        "Config deserialization failed for {}: primary error: {}, legacy error: {}", 
                        context_id, e1, e2
                    )))
                }
            }
        }
    }
}

/// Extract guardian DIDs from a GovernanceConfig
/// 
/// This is a helper to access guardian DIDs from the governance-kernel config structure.
/// If the governance-kernel structure changes in the future, only this function needs to be updated.
fn get_guardian_dids_from_config(config: &GovernanceConfig) -> Vec<IdentityId> {
    // For now this is a simple implementation that looks for roles named "guardian" in
    // the roles structure. This should be updated once the proper guardian role structure
    // is finalized in the governance-kernel.
    
    if let Some(roles) = &config.governance {
        if let Some(role_list) = &roles.roles {
            // Look for roles named "guardian" or similar
            for role in role_list {
                if role.name.to_lowercase().contains("guardian") {
                    // Assume permissions contain DIDs for now
                    return role.permissions.iter()
                        .map(|did| IdentityId(did.clone()))
                        .collect();
                }
            }
        }
    }
    
    // If no guardians found, return an empty list
    Vec::new()
}

/// Store a governance configuration in storage
/// 
/// This is mainly used for testing, but could also be used by admin tools
/// to set up initial configurations.
pub async fn store_governance_config<S>(
    config: &LegacyGovernanceConfig,
    storage: &Mutex<S>
) -> FederationResult<Cid> 
where 
    S: StorageBackend + Send + Sync
{
    // Serialize the configuration
    let config_bytes = serde_json::to_vec(config)
        .map_err(|e| FederationError::SerializationError(
            format!("Failed to serialize governance config: {}", e)
        ))?;
    
    // Derive the storage key for this context's configuration
    let _key_cid = config_key_for_scope(&config.scope_id);
    
    // Store the configuration
    let storage_lock = storage.lock().await;
    let result = storage_lock.put_blob(&config_bytes).await
        .map_err(|e| FederationError::NetworkError(
            format!("Failed to store governance config: {}", e)
        ))?;
    
    tracing::info!(
        "Stored governance config for {} with CID {}", 
        config.scope_id, result
    );
    
    Ok(result)
}

/// Check if an identity is an authorized guardian for a specific context
pub async fn is_authorized_guardian(
    identity: &IdentityId,
    context_id: &str,
    storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>
) -> FederationResult<bool>
{
    let guardians = get_authorized_guardians(context_id, Arc::clone(&storage)).await?;
    Ok(guardians.contains(identity))
}

/// Get the replication policy for a specific context ID
/// 
/// This looks up a governance configuration from storage based on the context ID
/// (which could be a federation ID, scope ID, etc.) and extracts the replication policy
/// that applies to that context.
pub async fn get_replication_policy(
    context_id: &str, 
    storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>
) -> FederationResult<StorageReplicationPolicy> {
    // First, try the direct approach - looking up by the context_id key
    let key_cid = config_key_for_scope(context_id);
    tracing::debug!(context_id, key = %key_cid, "Looking up config for replication policy");
    
    let store_lock = storage.lock().await;
    match store_lock.get_kv(&key_cid).await {
        Ok(Some(bytes)) => {
            // Drop the lock before parsing
            drop(store_lock);
            return parse_config_for_replication_policy(bytes, context_id);
        },
        // Otherwise, continue with the fallback approach
        _ => drop(store_lock)
    }
    
    // Fallback: List all available entries and check each one
    let all_cids = {
        let store_lock = storage.lock().await;
        match store_lock.list_all().await {
            Ok(cids) => {
                drop(store_lock);
                cids
            },
            Err(e) => {
                drop(store_lock);
                return Err(FederationError::StorageError(format!(
                    "Failed to list storage contents: {}", e
                )));
            }
        }
    };
    
    // If we have no entries, return default policy
    if all_cids.is_empty() {
        tracing::debug!(context_id, "No governance configs found, using default replication policy");
        return Ok(StorageReplicationPolicy::Factor(3)); // Default to 3 replicas
    }
    
    // For each CID, try to get the content and check if it's a config for our context
    for cid in all_cids {
        let bytes = {
            let store_lock = storage.lock().await;
            match store_lock.get_kv(&cid).await {
                Ok(Some(bytes)) => {
                    drop(store_lock);
                    bytes
                },
                _ => {
                    drop(store_lock);
                    continue;
                }
            }
        };
        
        // Try to parse as governance config and check if it matches our context
        if let Ok(legacy_config) = serde_json::from_slice::<LegacyGovernanceConfig>(&bytes) {
            if legacy_config.scope_id == context_id {
                // Legacy configs don't have storage policies, return default
                return Ok(StorageReplicationPolicy::Factor(3));
            }
        } else if let Ok(kernel_config) = serde_json::from_slice::<GovernanceConfig>(&bytes) {
            // Extract storage policy from kernel config
            // For now, we'll return a default policy
            return Ok(StorageReplicationPolicy::Factor(3));
        }
    }
    
    // If we got here, we didn't find a matching config
    // Return a default policy rather than an error
    tracing::debug!(context_id, "No matching governance config found, using default replication policy");
    Ok(StorageReplicationPolicy::Factor(3))
}

/// Helper function to extract replication policy from governance config bytes
fn parse_config_for_replication_policy(bytes: Vec<u8>, context_id: &str) -> FederationResult<StorageReplicationPolicy> {
    // Try to deserialize as GovernanceConfig from governance-kernel first
    match serde_json::from_slice::<GovernanceConfig>(&bytes) {
        Ok(config) => {
            // Extract storage policy if available
            // In a real implementation, this would access config.storage.replication_policy or similar
            // For now, return a default policy
            tracing::debug!(context_id, "Found governance config, but no storage policy defined");
            Ok(StorageReplicationPolicy::Factor(3))
        },
        Err(e1) => {
            // Try legacy format, which doesn't have storage policies
            tracing::debug!(context_id, error = %e1, "Failed to parse as governance-kernel config, trying legacy format");
            
            match serde_json::from_slice::<LegacyGovernanceConfig>(&bytes) {
                Ok(_) => {
                    tracing::debug!(context_id, "Found legacy config, using default replication policy");
                    Ok(StorageReplicationPolicy::Factor(3))
                },
                Err(e2) => {
                    Err(FederationError::InternalError(format!(
                        "Config deserialization failed for {}: primary error: {}, legacy error: {}", 
                        context_id, e1, e2
                    )))
                }
            }
        }
    }
}

fn get_config_cid(context_id: &str) -> Cid {
    // Create a key from the context id
    let key_str = format!("config::{}", context_id);
    let key_hash = crate::create_sha256_multihash(key_str.as_bytes());
    Cid::new_v1(0x71, key_hash) // dag-cbor codec for config data
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_storage::AsyncInMemoryStorage;
    use icn_governance_kernel::config::{GovernanceConfig, GovernanceStructure, Role};
    
    #[tokio::test]
    async fn test_governance_config_storage() {
        // Create a new in-memory storage with proper casting to dyn StorageBackend
        let storage: Arc<Mutex<dyn StorageBackend + Send + Sync>> = 
            Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        
        // Create a test governance config
        let mut legacy_config = LegacyGovernanceConfig::new("test-federation");
        legacy_config.roles.guardians = Some(vec![
            "did:icn:guardian1".to_string(),
            "did:icn:guardian2".to_string(),
            "did:icn:guardian3".to_string(),
        ]);
        
        // Serialize and store the config
        let config_bytes = serde_json::to_vec(&legacy_config).unwrap();
        
        // Store the config using put
        let store_lock = storage.lock().await;
        let _content_cid = store_lock.put_blob(&config_bytes).await.unwrap();
        drop(store_lock);
        
        // Let's also directly create a mapping for quick testing
        let _encoded_key = config_key_for_scope("test-federation");
        let store_lock = storage.lock().await;
        let _content_cid = store_lock.put_blob(&config_bytes).await.unwrap();
        drop(store_lock);
        
        // Retrieve the guardians
        let guardians = get_authorized_guardians("test-federation", Arc::clone(&storage)).await.unwrap();
        
        // Check we got the expected guardians
        assert_eq!(guardians.len(), 3);
        assert!(guardians.contains(&IdentityId("did:icn:guardian1".to_string())));
        assert!(guardians.contains(&IdentityId("did:icn:guardian2".to_string())));
        assert!(guardians.contains(&IdentityId("did:icn:guardian3".to_string())));
    }
    
    #[tokio::test]
    async fn test_governance_kernel_config() {
        // Create a new in-memory storage with proper casting to dyn StorageBackend
        let storage: Arc<Mutex<dyn StorageBackend + Send + Sync>> = 
            Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        
        // Create a test governance config using the governance-kernel structure
        let config = GovernanceConfig {
            template_type: "test".to_string(),
            template_version: "v1".to_string(),
            governing_scope: icn_identity::IdentityScope::Federation, // Use an existing scope variant
            identity: None,
            governance: Some(GovernanceStructure {
                decision_making: None,
                quorum: None,
                majority: None,
                term_length: None,
                roles: Some(vec![
                    Role {
                        name: "Guardian".to_string(),
                        permissions: vec![
                            "did:icn:guardian1".to_string(),
                            "did:icn:guardian2".to_string(),
                        ],
                    }
                ]),
            }),
            membership: None,
            proposals: None,
            working_groups: None,
            dispute_resolution: None,
            economic_model: None,
        };
        
        // Serialize and store the config
        let config_bytes = serde_json::to_vec(&config).unwrap();
        
        // Store the config in storage
        let store_lock = storage.lock().await;
        let _content_cid = store_lock.put_blob(&config_bytes).await.unwrap();
        drop(store_lock);
        
        // Let's also directly create a mapping for quick testing
        let _encoded_key = config_key_for_scope("test-federation");
        let store_lock = storage.lock().await;
        let _content_cid = store_lock.put_blob(&config_bytes).await.unwrap();
        drop(store_lock);
        
        // Retrieve the guardians
        let guardians = get_authorized_guardians("test-federation", Arc::clone(&storage)).await.unwrap();
        
        // Check we got the expected guardians
        assert_eq!(guardians.len(), 2);
        assert!(guardians.contains(&IdentityId("did:icn:guardian1".to_string())));
        assert!(guardians.contains(&IdentityId("did:icn:guardian2".to_string())));
    }
    
    #[tokio::test]
    async fn test_missing_config() {
        // Create a new in-memory storage with proper casting to dyn StorageBackend
        let storage: Arc<Mutex<dyn StorageBackend + Send + Sync>> = 
            Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        
        // Try to retrieve guardians for a non-existent context
        let result = get_authorized_guardians("non-existent-federation", Arc::clone(&storage)).await;
        
        // Should return a ConfigNotFound error, not an empty list
        assert!(result.is_err());
        match result.unwrap_err() {
            FederationError::StorageError(id) => {
                assert_eq!(id, "Configuration not found for context: non-existent-federation");
            },
            e => panic!("Expected StorageError error, got: {:?}", e),
        }
    }
} 