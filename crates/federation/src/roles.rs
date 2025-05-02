/*!
 * Role-based authorization helpers for federation
 * 
 * This module contains functions for looking up and verifying roles 
 * within federation contexts, such as checking if an identity is authorized
 * as a guardian for a specific scope or federation.
 */

use cid::Cid;
use futures::lock::Mutex;
use multihash::{Code, MultihashDigest};
use serde::{Serialize, Deserialize};
use std::sync::Arc;

use crate::FederationError;
use crate::FederationResult;
use icn_identity::IdentityId;
use icn_storage::StorageBackend;

/// Simple structure to represent governance roles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceRoles {
    /// Guardian DIDs authorized for this context
    pub guardians: Option<Vec<String>>,
    
    /// Other roles can be added here in the future
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stewards: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participants: Option<Vec<String>>,
}

impl Default for GovernanceRoles {
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
/// This is a simplified version that only contains role information.
/// In the future, this would ideally be integrated with the governance-kernel
/// configuration system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// The ID of the scope this configuration applies to (e.g., federation ID)
    pub scope_id: String,
    
    /// Role definitions
    pub roles: GovernanceRoles,
    
    /// Version of this configuration
    pub version: String,
}

impl GovernanceConfig {
    /// Create a new governance configuration
    pub fn new(scope_id: impl Into<String>) -> Self {
        Self {
            scope_id: scope_id.into(),
            roles: GovernanceRoles::default(),
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
    let key_hash = Code::Sha2_256.digest(key_str.as_bytes());
    Cid::new_v1(0x71, key_hash) // Raw codec (0x71)
}

/// Get the list of authorized guardians for a specific context ID
/// 
/// This looks up a governance configuration from storage based on the context ID
/// (which could be a federation ID, scope ID, etc.) and extracts the list of
/// guardian DIDs that are authorized for that context.
pub async fn get_authorized_guardians<S>(
    context_id: &str, 
    storage: &Mutex<S>
) -> FederationResult<Vec<IdentityId>> 
where 
    S: StorageBackend + Send + Sync
{
    // Derive the storage key for this context's configuration
    let key_cid = config_key_for_scope(context_id);
    
    // Access storage to retrieve the config
    let storage_lock = storage.lock().await;
    
    // Retrieve the configuration bytes
    let config_bytes = match storage_lock.get(&key_cid).await {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            tracing::warn!("No governance configuration found for context: {}", context_id);
            return Ok(Vec::new()); // No config = no guardians
        },
        Err(e) => {
            return Err(FederationError::SyncFailed(
                format!("Failed to retrieve governance config from storage: {}", e)
            ));
        }
    };
    
    // Drop the lock as soon as possible
    drop(storage_lock);
    
    // Deserialize the configuration
    let config: GovernanceConfig = match serde_json::from_slice(&config_bytes) {
        Ok(config) => config,
        Err(e) => {
            return Err(FederationError::InvalidPolicy(
                format!("Failed to deserialize governance config: {}", e)
            ));
        }
    };
    
    // Extract the guardian DIDs
    Ok(config.extract_guardian_dids())
}

/// Store a governance configuration in storage
/// 
/// This is mainly used for testing, but could also be used by admin tools
/// to set up initial configurations.
pub async fn store_governance_config<S>(
    config: &GovernanceConfig,
    storage: &Mutex<S>
) -> FederationResult<Cid> 
where 
    S: StorageBackend + Send + Sync
{
    // Serialize the configuration
    let config_bytes = serde_json::to_vec(config)
        .map_err(|e| FederationError::InvalidPolicy(
            format!("Failed to serialize governance config: {}", e)
        ))?;
    
    // Derive the storage key for this context's configuration
    let key_cid = config_key_for_scope(&config.scope_id);
    
    // Store the configuration
    let storage_lock = storage.lock().await;
    let result = storage_lock.put(&config_bytes).await
        .map_err(|e| FederationError::SyncFailed(
            format!("Failed to store governance config: {}", e)
        ))?;
    
    tracing::info!(
        "Stored governance config for {} with CID {} (key: {})", 
        config.scope_id, result, key_cid
    );
    
    Ok(result)
}

/// Check if an identity is an authorized guardian for a specific context
pub async fn is_authorized_guardian<S>(
    identity: &IdentityId,
    context_id: &str,
    storage: &Mutex<S>
) -> FederationResult<bool>
where 
    S: StorageBackend + Send + Sync
{
    let guardians = get_authorized_guardians(context_id, storage).await?;
    Ok(guardians.contains(identity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_storage::AsyncInMemoryStorage;
    
    #[tokio::test]
    async fn test_governance_config_storage() {
        // Create a new in-memory storage
        let storage = Mutex::new(AsyncInMemoryStorage::new());
        
        // Create a test governance config
        let mut config = GovernanceConfig::new("test-federation");
        config.roles.guardians = Some(vec![
            "did:icn:guardian1".to_string(),
            "did:icn:guardian2".to_string(),
            "did:icn:guardian3".to_string(),
        ]);
        
        // Store the config
        let result = store_governance_config(&config, &storage).await;
        assert!(result.is_ok(), "Failed to store governance config: {:?}", result.err());
        
        // Retrieve the guardians
        let guardians = get_authorized_guardians("test-federation", &storage).await.unwrap();
        
        // Check we got the expected guardians
        assert_eq!(guardians.len(), 3);
        assert!(guardians.contains(&IdentityId("did:icn:guardian1".to_string())));
        assert!(guardians.contains(&IdentityId("did:icn:guardian2".to_string())));
        assert!(guardians.contains(&IdentityId("did:icn:guardian3".to_string())));
        
        // Check a specific identity
        let is_guardian = is_authorized_guardian(
            &IdentityId("did:icn:guardian1".to_string()),
            "test-federation",
            &storage
        ).await.unwrap();
        
        assert!(is_guardian, "Identity should be recognized as a guardian");
        
        // Check an unauthorized identity
        let is_guardian = is_authorized_guardian(
            &IdentityId("did:icn:not-a-guardian".to_string()),
            "test-federation",
            &storage
        ).await.unwrap();
        
        assert!(!is_guardian, "Unauthorized identity should not be recognized as a guardian");
    }
    
    #[tokio::test]
    async fn test_missing_config() {
        // Create a new in-memory storage
        let storage = Mutex::new(AsyncInMemoryStorage::new());
        
        // Try to retrieve guardians for a non-existent context
        let guardians = get_authorized_guardians("non-existent-federation", &storage).await.unwrap();
        
        // Should return an empty list, not an error
        assert!(guardians.is_empty(), "Should return empty list for missing config");
    }
} 