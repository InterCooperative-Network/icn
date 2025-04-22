use crate::identity::{Identity, IdentityError};
use crate::storage::{StorageManager, StorageError};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GuardianError {
    #[error("Identity error: {0}")]
    IdentityError(#[from] IdentityError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("Recovery threshold not met: {0}/{1} signatures required")]
    ThresholdNotMet(usize, usize),
    
    #[error("Invalid guardian: {0}")]
    InvalidGuardian(String),
    
    #[error("Guardian already exists: {0}")]
    GuardianExists(String),
    
    #[error("Recovery bundle not found or invalid")]
    InvalidRecoveryBundle,
    
    #[error("Guardian signature invalid or missing: {0}")]
    InvalidSignature(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Status of a guardian relative to a recovery request
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GuardianStatus {
    /// Guardian has been added but not confirmed
    Pending,
    /// Guardian has confirmed and is active
    Active,
    /// Guardian has approved a recovery
    Approved,
    /// Guardian has rejected a recovery
    Rejected,
}

/// A guardian is a trusted identity that can help recover another identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guardian {
    /// DID of the guardian
    pub did: String,
    /// Name/alias for this guardian
    pub name: String,
    /// When this guardian was added
    pub added_at: DateTime<Utc>,
    /// Current status
    pub status: GuardianStatus,
    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

/// GuardianSet manages a group of guardians for a specific identity
#[derive(Debug, Serialize, Deserialize)]
pub struct GuardianSet {
    /// DID of the identity this set protects
    pub owner_did: String,
    /// List of guardians
    pub guardians: Vec<Guardian>,
    /// Minimum number of guardians needed for recovery (M-of-N)
    pub threshold: usize,
    /// When this guardian set was last modified
    pub last_modified: DateTime<Utc>,
}

impl GuardianSet {
    /// Create a new guardian set for a given identity
    pub fn new(owner_did: String, threshold: usize) -> Self {
        Self {
            owner_did,
            guardians: Vec::new(),
            threshold,
            last_modified: Utc::now(),
        }
    }
    
    /// Add a new guardian
    pub fn add_guardian(&mut self, did: String, name: String) -> Result<(), GuardianError> {
        // Check if guardian already exists
        if self.guardians.iter().any(|g| g.did == did) {
            return Err(GuardianError::GuardianExists(did));
        }
        
        let guardian = Guardian {
            did,
            name,
            added_at: Utc::now(),
            status: GuardianStatus::Pending,
            metadata: HashMap::new(),
        };
        
        self.guardians.push(guardian);
        self.last_modified = Utc::now();
        
        Ok(())
    }
    
    /// Remove a guardian by DID
    pub fn remove_guardian(&mut self, did: &str) -> Result<(), GuardianError> {
        let initial_len = self.guardians.len();
        self.guardians.retain(|g| g.did != did);
        
        if self.guardians.len() == initial_len {
            return Err(GuardianError::InvalidGuardian(did.to_string()));
        }
        
        self.last_modified = Utc::now();
        Ok(())
    }
    
    /// Set the status of a guardian
    pub fn set_guardian_status(&mut self, did: &str, status: GuardianStatus) -> Result<(), GuardianError> {
        let guardian = self.guardians.iter_mut()
            .find(|g| g.did == did)
            .ok_or_else(|| GuardianError::InvalidGuardian(did.to_string()))?;
        
        guardian.status = status;
        self.last_modified = Utc::now();
        
        Ok(())
    }
    
    /// Get a guardian by DID
    pub fn get_guardian(&self, did: &str) -> Option<&Guardian> {
        self.guardians.iter().find(|g| g.did == did)
    }
    
    /// List all active guardians
    pub fn list_active_guardians(&self) -> Vec<&Guardian> {
        self.guardians.iter()
            .filter(|g| g.status == GuardianStatus::Active)
            .collect()
    }
    
    /// Check if the recovery threshold can be met with the given signatures
    pub fn can_recover(&self, signed_dids: &[String]) -> bool {
        let valid_guardians: Vec<_> = self.guardians.iter()
            .filter(|g| g.status == GuardianStatus::Active && signed_dids.contains(&g.did))
            .collect();
        
        valid_guardians.len() >= self.threshold
    }
    
    /// Get recovery threshold details
    pub fn threshold_info(&self) -> (usize, usize) {
        let active_count = self.guardians.iter()
            .filter(|g| g.status == GuardianStatus::Active)
            .count();
        
        (self.threshold, active_count)
    }
}

/// RecoveryBundle contains the encrypted identity data that can be recovered
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoveryBundle {
    /// DID of the identity being recovered
    pub did: String,
    /// Encrypted keypair data
    pub encrypted_data: Vec<u8>,
    /// Encryption nonce
    pub nonce: Vec<u8>,
    /// When this bundle was created
    pub created_at: DateTime<Utc>,
    /// Hash of guardian DIDs used for encryption
    pub guardian_hash: String,
}

/// GuardianSignature represents a guardian's approval for recovery
#[derive(Debug, Serialize, Deserialize)]
pub struct GuardianSignature {
    /// DID of the guardian
    pub guardian_did: String,
    /// Signature over the recovery bundle
    pub signature: Vec<u8>,
    /// Timestamp of signature
    pub timestamp: DateTime<Utc>,
}

/// RecoveryRequest represents an in-progress recovery attempt
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoveryRequest {
    /// ID of this recovery request
    pub id: String,
    /// DID being recovered
    pub did: String,
    /// Collected guardian signatures
    pub signatures: Vec<GuardianSignature>,
    /// When this request was initiated
    pub created_at: DateTime<Utc>,
    /// Current status (pending, complete, failed)
    pub status: String,
}

/// GuardianManager handles all guardian-related operations
pub struct GuardianManager {
    storage: StorageManager,
}

impl GuardianManager {
    /// Create a new guardian manager
    pub fn new(storage: StorageManager) -> Self {
        Self { storage }
    }
    
    /// Create or update a guardian set for an identity
    pub fn create_guardian_set(&self, owner_did: &str, threshold: usize) -> Result<GuardianSet, GuardianError> {
        let guardian_set = GuardianSet::new(owner_did.to_string(), threshold);
        
        // Save the guardian set
        self.storage.save("guardians", &format!("{}_set", owner_did), &guardian_set)
            .map_err(GuardianError::StorageError)?;
        
        Ok(guardian_set)
    }
    
    /// Load a guardian set for an identity
    pub fn load_guardian_set(&self, owner_did: &str) -> Result<GuardianSet, GuardianError> {
        self.storage.load::<GuardianSet>("guardians", &format!("{}_set", owner_did))
            .map_err(GuardianError::StorageError)
    }
    
    /// Add a guardian to an identity's guardian set
    pub fn add_guardian(
        &self,
        owner_did: &str,
        guardian_did: &str,
        name: &str,
    ) -> Result<(), GuardianError> {
        let mut guardian_set = self.load_guardian_set(owner_did)?;
        
        guardian_set.add_guardian(guardian_did.to_string(), name.to_string())?;
        
        // Save updated guardian set
        self.storage.save("guardians", &format!("{}_set", owner_did), &guardian_set)
            .map_err(GuardianError::StorageError)?;
        
        Ok(())
    }
    
    /// Create a recovery bundle for an identity
    pub fn create_recovery_bundle(
        &self,
        identity: &Identity,
        recovery_password: &str,
    ) -> Result<RecoveryBundle, GuardianError> {
        // First check if we have a guardian set
        let guardian_set = self.load_guardian_set(identity.did())?;
        
        // Serialize the identity to JSON
        let identity_json = serde_json::to_string(identity)
            .map_err(|e| GuardianError::SerializationError(e.to_string()))?;
        
        // Create a random nonce
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        
        // Encrypt the identity data with the recovery password
        // Note: In a real implementation, use a proper encryption library like age or ring
        // For simplicity, we'll just simulate encryption here
        let encrypted_data = self.encrypt_data(identity_json.as_bytes(), recovery_password, &nonce)
            .map_err(|e| GuardianError::EncryptionError(e))?;
        
        // Create a bundle
        let bundle = RecoveryBundle {
            did: identity.did().to_string(),
            encrypted_data,
            nonce: nonce.to_vec(),
            created_at: Utc::now(),
            // Create a hash of guardian DIDs (in a real app, use a proper hash)
            guardian_hash: format!("guardians:{}", guardian_set.guardians.len()),
        };
        
        // Save the bundle
        self.storage.save("recovery", identity.did(), &bundle)
            .map_err(GuardianError::StorageError)?;
        
        Ok(bundle)
    }
    
    /// Start a recovery process
    pub fn start_recovery(&self, did: &str) -> Result<RecoveryRequest, GuardianError> {
        // Load the recovery bundle
        let _bundle = self.storage.load::<RecoveryBundle>("recovery", did)
            .map_err(|_| GuardianError::InvalidRecoveryBundle)?;
        
        // Generate a request ID
        let request_id = uuid::Uuid::new_v4().to_string();
        
        // Create a recovery request
        let request = RecoveryRequest {
            id: request_id,
            did: did.to_string(),
            signatures: Vec::new(),
            created_at: Utc::now(),
            status: "pending".to_string(),
        };
        
        // Save the request
        self.storage.save("recovery_requests", &request.id, &request)
            .map_err(GuardianError::StorageError)?;
        
        Ok(request)
    }
    
    /// Add a guardian signature to a recovery request
    pub fn add_recovery_signature(
        &self,
        request_id: &str,
        guardian_did: &str,
        signature: Vec<u8>,
    ) -> Result<RecoveryRequest, GuardianError> {
        // Load the request
        let mut request = self.storage.load::<RecoveryRequest>("recovery_requests", request_id)
            .map_err(|e| GuardianError::StorageError(e))?;
        
        // Load the guardian set for the identity being recovered
        let guardian_set = self.load_guardian_set(&request.did)?;
        
        // Check if this is a valid guardian
        if guardian_set.get_guardian(guardian_did).is_none() {
            return Err(GuardianError::InvalidGuardian(guardian_did.to_string()));
        }
        
        // Add the signature
        let guardian_sig = GuardianSignature {
            guardian_did: guardian_did.to_string(),
            signature,
            timestamp: Utc::now(),
        };
        
        request.signatures.push(guardian_sig);
        
        // Update the request
        self.storage.save("recovery_requests", &request.id, &request)
            .map_err(GuardianError::StorageError)?;
        
        Ok(request)
    }
    
    /// Complete a recovery with the recovery password
    pub fn complete_recovery(
        &self,
        request_id: &str,
        recovery_password: &str,
    ) -> Result<Identity, GuardianError> {
        // Load the request
        let request = self.storage.load::<RecoveryRequest>("recovery_requests", request_id)
            .map_err(|e| GuardianError::StorageError(e))?;
        
        // Load the guardian set
        let guardian_set = self.load_guardian_set(&request.did)?;
        
        // Check if we have enough signatures
        let signed_dids: Vec<String> = request.signatures.iter()
            .map(|sig| sig.guardian_did.clone())
            .collect();
        
        if !guardian_set.can_recover(&signed_dids) {
            let (threshold, total) = guardian_set.threshold_info();
            return Err(GuardianError::ThresholdNotMet(threshold, total));
        }
        
        // Load the recovery bundle
        let bundle = self.storage.load::<RecoveryBundle>("recovery", &request.did)
            .map_err(|_| GuardianError::InvalidRecoveryBundle)?;
        
        // Decrypt the identity data
        let decrypted_data = self.decrypt_data(&bundle.encrypted_data, recovery_password, &bundle.nonce)
            .map_err(|e| GuardianError::EncryptionError(e))?;
        
        // Deserialize the identity
        let identity: Identity = serde_json::from_slice(&decrypted_data)
            .map_err(|e| GuardianError::SerializationError(e.to_string()))?;
        
        // Mark the request as complete
        let mut updated_request = request;
        updated_request.status = "complete".to_string();
        
        self.storage.save("recovery_requests", &updated_request.id, &updated_request)
            .map_err(GuardianError::StorageError)?;
        
        Ok(identity)
    }
    
    /// Export a recovery bundle to a file
    pub fn export_recovery_bundle(&self, did: &str, path: &Path) -> Result<(), GuardianError> {
        // Load the bundle
        let bundle = self.storage.load::<RecoveryBundle>("recovery", did)
            .map_err(|_| GuardianError::InvalidRecoveryBundle)?;
        
        // Serialize to JSON
        let json = serde_json::to_string_pretty(&bundle)
            .map_err(|e| GuardianError::SerializationError(e.to_string()))?;
        
        // Write to file
        std::fs::write(path, json)
            .map_err(|e| GuardianError::StorageError(StorageError::FileWrite(e.to_string())))?;
        
        Ok(())
    }
    
    /// Import a recovery bundle from a file
    pub fn import_recovery_bundle(&self, path: &Path) -> Result<RecoveryBundle, GuardianError> {
        // Read the file
        let json = std::fs::read_to_string(path)
            .map_err(|e| GuardianError::StorageError(StorageError::FileRead(e.to_string())))?;
        
        // Deserialize
        let bundle: RecoveryBundle = serde_json::from_str(&json)
            .map_err(|e| GuardianError::SerializationError(e.to_string()))?;
        
        // Save the bundle
        self.storage.save("recovery", &bundle.did, &bundle)
            .map_err(GuardianError::StorageError)?;
        
        Ok(bundle)
    }
    
    /// Simple encryption function (placeholder - use a real crypto library in production)
    fn encrypt_data(&self, data: &[u8], password: &str, nonce: &[u8]) -> Result<Vec<u8>, String> {
        // This is a placeholder - in a real app, use a proper encryption library
        // For example, use age, ring, or another Rust crypto library
        
        // In this simplified version, we're just XORing with the password
        // DO NOT USE THIS IN PRODUCTION!
        let password_bytes = password.as_bytes();
        let mut result = Vec::with_capacity(data.len());
        
        for (i, &byte) in data.iter().enumerate() {
            let password_byte = password_bytes[i % password_bytes.len()];
            let nonce_byte = nonce[i % nonce.len()];
            result.push(byte ^ password_byte ^ nonce_byte);
        }
        
        Ok(result)
    }
    
    /// Simple decryption function (placeholder - use a real crypto library in production)
    fn decrypt_data(&self, data: &[u8], password: &str, nonce: &[u8]) -> Result<Vec<u8>, String> {
        // Since our encryption is just XOR, decryption is the same operation
        self.encrypt_data(data, password, nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Identity, KeyType};
    use std::path::PathBuf;
    use tempfile::tempdir;
    
    fn create_test_identity() -> Identity {
        Identity::new("test", "alice", KeyType::Ed25519).unwrap()
    }
    
    fn create_test_storage() -> StorageManager {
        let temp_dir = tempdir().unwrap();
        StorageManager::with_base_dir(temp_dir.path().to_path_buf(), crate::storage::StorageType::File).unwrap()
    }
    
    #[test]
    fn test_guardian_set_creation() {
        let storage = create_test_storage();
        let manager = GuardianManager::new(storage);
        
        let identity = create_test_identity();
        let guardian_set = manager.create_guardian_set(identity.did(), 2).unwrap();
        
        assert_eq!(guardian_set.owner_did, identity.did());
        assert_eq!(guardian_set.threshold, 2);
        assert!(guardian_set.guardians.is_empty());
    }
    
    #[test]
    fn test_add_guardian() {
        let storage = create_test_storage();
        let manager = GuardianManager::new(storage);
        
        let identity = create_test_identity();
        let _guardian_set = manager.create_guardian_set(identity.did(), 2).unwrap();
        
        // Add a guardian
        manager.add_guardian(identity.did(), "did:icn:test:bob", "Bob").unwrap();
        
        // Load and check
        let updated_set = manager.load_guardian_set(identity.did()).unwrap();
        assert_eq!(updated_set.guardians.len(), 1);
        assert_eq!(updated_set.guardians[0].did, "did:icn:test:bob");
        assert_eq!(updated_set.guardians[0].name, "Bob");
    }
    
    #[test]
    fn test_recovery_bundle() {
        let storage = create_test_storage();
        let manager = GuardianManager::new(storage);
        
        let identity = create_test_identity();
        let _guardian_set = manager.create_guardian_set(identity.did(), 2).unwrap();
        
        // Create a recovery bundle
        let bundle = manager.create_recovery_bundle(&identity, "secret-password").unwrap();
        
        assert_eq!(bundle.did, identity.did());
        assert!(!bundle.encrypted_data.is_empty());
    }
    
    #[test]
    fn test_recovery_process() {
        let storage = create_test_storage();
        let manager = GuardianManager::new(storage);
        
        let identity = create_test_identity();
        let mut guardian_set = manager.create_guardian_set(identity.did(), 2).unwrap();
        
        // Add guardians
        guardian_set.add_guardian("did:icn:test:bob".to_string(), "Bob".to_string()).unwrap();
        guardian_set.add_guardian("did:icn:test:carol".to_string(), "Carol".to_string()).unwrap();
        
        // Set them as active
        guardian_set.set_guardian_status("did:icn:test:bob", GuardianStatus::Active).unwrap();
        guardian_set.set_guardian_status("did:icn:test:carol", GuardianStatus::Active).unwrap();
        
        // Save the updated guardian set
        storage.save("guardians", &format!("{}_set", identity.did()), &guardian_set).unwrap();
        
        // Create a recovery bundle
        let _bundle = manager.create_recovery_bundle(&identity, "secret-password").unwrap();
        
        // Start recovery
        let request = manager.start_recovery(identity.did()).unwrap();
        
        // Add signatures (simplified for test)
        let dummy_sig = vec![1, 2, 3, 4];
        manager.add_recovery_signature(&request.id, "did:icn:test:bob", dummy_sig.clone()).unwrap();
        manager.add_recovery_signature(&request.id, "did:icn:test:carol", dummy_sig).unwrap();
        
        // Complete recovery
        let recovered = manager.complete_recovery(&request.id, "secret-password").unwrap();
        
        assert_eq!(recovered.did(), identity.did());
    }
} 