pub mod error;
pub mod types;

use std::path::{Path, PathBuf};
use std::fs;
use tokio::fs as tokio_fs;
use types::{IdentityWallet, IdentityScope, Did};
use error::{IdentityError, IdentityResult};
use tracing::{debug, info, warn};
use ed25519_dalek::Signature;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Manager for identity operations
pub struct IdentityManager {
    /// Directory where identities are stored
    identities_dir: PathBuf,
    
    /// Currently active identity
    active_identity: Option<IdentityWallet>,
}

impl IdentityManager {
    /// Create a new identity manager
    pub async fn new(base_dir: impl AsRef<Path>) -> IdentityResult<Self> {
        let identities_dir = base_dir.as_ref().join("identities");
        
        // Create identities directory if it doesn't exist
        if !identities_dir.exists() {
            tokio_fs::create_dir_all(&identities_dir).await?;
            debug!("Created identities directory at {:?}", identities_dir);
        }
        
        Ok(Self {
            identities_dir,
            active_identity: None,
        })
    }
    
    /// Create a new identity
    pub async fn create_identity(&mut self, name: &str, scope: IdentityScope) -> IdentityResult<IdentityWallet> {
        let identity = IdentityWallet::new(name, scope)?;
        
        // Save the identity
        self.save_identity(&identity).await?;
        debug!("Created new identity: {}", identity.did);
        
        // Set as active if no identity is active
        if self.active_identity.is_none() {
            self.active_identity = Some(identity.clone());
            info!("Set {} as active identity", identity.did);
        }
        
        Ok(identity)
    }
    
    /// Save an identity to disk
    pub async fn save_identity(&self, identity: &IdentityWallet) -> IdentityResult<()> {
        let did_id = &identity.did.id;
        let identity_path = self.identities_dir.join(format!("{}.json", did_id));
        
        let serialized = serde_json::to_string_pretty(identity)
            .map_err(|e| IdentityError::SerializationError(format!("Failed to serialize identity: {}", e)))?;
            
        tokio_fs::write(&identity_path, serialized).await?;
        debug!("Saved identity {} to {:?}", identity.did, identity_path);
        
        Ok(())
    }
    
    /// Load an identity by DID
    pub async fn load_identity(&mut self, did: &str) -> IdentityResult<IdentityWallet> {
        // Parse the DID to get the ID part
        let parsed_did = Did::parse(did)?;
        let did_id = &parsed_did.id;
        
        // Look for the identity file
        let identity_path = self.identities_dir.join(format!("{}.json", did_id));
        
        if !identity_path.exists() {
            return Err(IdentityError::NotFound(format!("Identity not found: {}", did)));
        }
        
        let content = tokio_fs::read_to_string(&identity_path).await?;
        let identity: IdentityWallet = serde_json::from_str(&content)
            .map_err(|e| IdentityError::SerializationError(format!("Failed to deserialize identity: {}", e)))?;
        
        debug!("Loaded identity: {}", identity.did);
        
        // Set as active
        self.active_identity = Some(identity.clone());
        
        Ok(identity)
    }
    
    /// List all identities
    pub async fn list_identities(&self) -> IdentityResult<Vec<IdentityWallet>> {
        let mut identities = Vec::new();
        
        let mut dir_entries = tokio_fs::read_dir(&self.identities_dir).await?;
        
        while let Ok(Some(entry)) = dir_entries.next_entry().await {
            let path = entry.path();
            
            // Skip non-json files
            if !path.is_file() || path.extension().map_or(true, |ext| ext != "json") {
                continue;
            }
            
            match tokio_fs::read_to_string(&path).await {
                Ok(content) => {
                    match serde_json::from_str::<IdentityWallet>(&content) {
                        Ok(identity) => {
                            identities.push(identity);
                        },
                        Err(e) => {
                            warn!("Failed to parse identity file {:?}: {}", path, e);
                        }
                    }
                },
                Err(e) => {
                    warn!("Failed to read identity file {:?}: {}", path, e);
                }
            }
        }
        
        Ok(identities)
    }
    
    /// Get the active identity
    pub fn get_active_identity(&self) -> Option<&IdentityWallet> {
        self.active_identity.as_ref()
    }
    
    /// Set the active identity
    pub fn set_active_identity(&mut self, identity: IdentityWallet) {
        self.active_identity = Some(identity);
    }
    
    /// Sign a message with the active identity
    pub fn sign_message(&self, message: &[u8]) -> IdentityResult<Signature> {
        let identity = self.active_identity.as_ref()
            .ok_or_else(|| IdentityError::Other("No active identity".to_string()))?;
            
        identity.sign(message)
    }
    
    /// Verify a signature with the active identity
    pub fn verify_message(&self, message: &[u8], signature: &Signature) -> IdentityResult<()> {
        let identity = self.active_identity.as_ref()
            .ok_or_else(|| IdentityError::Other("No active identity".to_string()))?;
            
        identity.verify(message, signature)
    }
    
    /// Delete an identity by DID
    pub async fn delete_identity(&mut self, did: &str) -> IdentityResult<()> {
        // Parse the DID to get the ID part
        let parsed_did = Did::parse(did)?;
        let did_id = &parsed_did.id;
        
        // Look for the identity file
        let identity_path = self.identities_dir.join(format!("{}.json", did_id));
        
        if !identity_path.exists() {
            return Err(IdentityError::NotFound(format!("Identity not found: {}", did)));
        }
        
        // Remove the identity file
        tokio_fs::remove_file(&identity_path).await?;
        
        // Clear active identity if it matches
        if let Some(active) = &self.active_identity {
            if active.did.did_string == did {
                self.active_identity = None;
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_create_and_load_identity() -> IdentityResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create an identity manager
        let mut manager = IdentityManager::new(temp_dir.path()).await?;
        
        // Create a new identity
        let identity = manager.create_identity("Test User", IdentityScope::Individual).await?;
        let did = identity.did.did_string.clone();
        
        // Load the identity
        let loaded = manager.load_identity(&did).await?;
        
        // Verify it's the same
        assert_eq!(identity.did.did_string, loaded.did.did_string);
        assert_eq!(identity.name, loaded.name);
        assert_eq!(identity.scope, loaded.scope);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_sign_and_verify() -> IdentityResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create an identity manager
        let mut manager = IdentityManager::new(temp_dir.path()).await?;
        
        // Create a new identity
        let identity = manager.create_identity("Test User", IdentityScope::Individual).await?;
        
        // Sign a message
        let message = b"Hello, world!";
        let signature = manager.sign_message(message)?;
        
        // Verify the signature
        manager.verify_message(message, &signature)?;
        
        // Modify the message and ensure verification fails
        let modified_message = b"Hello, world";
        
        match manager.verify_message(modified_message, &signature) {
            Err(IdentityError::VerificationFailed(_)) => {},
            Err(e) => panic!("Expected VerificationFailed, got: {:?}", e),
            Ok(_) => panic!("Expected verification to fail"),
        }
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_list_identities() -> IdentityResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create an identity manager
        let mut manager = IdentityManager::new(temp_dir.path()).await?;
        
        // Create multiple identities
        manager.create_identity("User 1", IdentityScope::Individual).await?;
        manager.create_identity("User 2", IdentityScope::Individual).await?;
        manager.create_identity("Community", IdentityScope::Community).await?;
        
        // List identities
        let identities = manager.list_identities().await?;
        
        // Verify we have 3 identities
        assert_eq!(identities.len(), 3);
        
        // Verify we have the expected scopes
        let individual_count = identities.iter().filter(|i| i.scope == IdentityScope::Individual).count();
        let community_count = identities.iter().filter(|i| i.scope == IdentityScope::Community).count();
        
        assert_eq!(individual_count, 2);
        assert_eq!(community_count, 1);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_delete_identity() -> IdentityResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create an identity manager
        let mut manager = IdentityManager::new(temp_dir.path()).await?;
        
        // Create multiple identities
        let id1 = manager.create_identity("User 1", IdentityScope::Individual).await?;
        let id2 = manager.create_identity("User 2", IdentityScope::Individual).await?;
        
        // Delete the first identity
        manager.delete_identity(&id1.did.did_string).await?;
        
        // List identities
        let identities = manager.list_identities().await?;
        
        // Verify we have 1 identity left
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].did.did_string, id2.did.did_string);
        
        Ok(())
    }
} 